//! ファイルとフォルダーの索引(D1〜D6). 数十万件を少ないメモリで持ち, 1文字ごとに全件を探す.
//! 名前は1つの大きな文字列に詰め, 親の番号でパスを組み立てる.

use crate::fuzzy::{self, lower_str};
use crate::migemo::{self, Dict};
use crate::search::{Prepared, TopK};

mod query;
pub use query::{FileQuery, FileSearch};
use query::{Matched, Matcher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const NO_PARENT: u32 = u32::MAX;
const FLAG_DIR: u8 = 1;
const FLAG_ASCII: u8 = 2;

/// 1つの索引(ユーザーフォルダー, Box, WSLなど). 根の名前はパス全体.
#[derive(Default)]
pub struct FileTable {
    names: String,
    name_end: Vec<u32>,
    lower: String,
    lower_end: Vec<u32>,
    parent: Vec<u32>,
    flags: Vec<u8>,
    /// 更新日時(UNIX秒. 分からないものは0)
    mtime: Vec<u32>,
    /// パスの区切り(`\`か`/`)
    sep: char,
}

/// 一致したファイル
#[derive(Debug, Clone, PartialEq)]
pub struct FileHit {
    pub id: u32,
    pub score: i32,
    pub positions: Vec<u32>,
}

impl FileTable {
    pub fn new(sep: char) -> FileTable {
        FileTable { sep, ..Default::default() }
    }

    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// 項目を足して番号を返す. 根は`parent = None`で, 名前にパス全体を入れる. `mtime`は更新日時(UNIX秒. 分からなければ0).
    pub fn push(&mut self, parent: Option<u32>, name: &str, is_dir: bool, mtime: u32) -> u32 {
        let id = self.parent.len() as u32;
        self.names.push_str(name);
        self.name_end.push(self.names.len() as u32);
        let l = lower_str(name);
        self.lower.push_str(&l);
        self.lower_end.push(self.lower.len() as u32);
        self.parent.push(parent.unwrap_or(NO_PARENT));
        self.mtime.push(mtime);
        self.flags.push(if is_dir { FLAG_DIR } else { 0 } | if name.is_ascii() { FLAG_ASCII } else { 0 });
        id
    }

    pub fn name(&self, id: u32) -> &str {
        let i = id as usize;
        let s = if i == 0 { 0 } else { self.name_end[i - 1] as usize };
        &self.names[s..self.name_end[i] as usize]
    }

    fn lower(&self, id: u32) -> &str {
        let i = id as usize;
        let s = if i == 0 { 0 } else { self.lower_end[i - 1] as usize };
        &self.lower[s..self.lower_end[i] as usize]
    }

    pub fn is_dir(&self, id: u32) -> bool {
        self.flags[id as usize] & FLAG_DIR != 0
    }

    pub fn parent(&self, id: u32) -> Option<u32> {
        let p = self.parent[id as usize];
        (p != NO_PARENT).then_some(p)
    }

    /// パス全体を組み立てる
    pub fn path(&self, id: u32) -> String {
        let mut parts = vec![self.name(id)];
        let mut cur = id;
        while let Some(p) = self.parent(cur) {
            parts.push(self.name(p));
            cur = p;
        }
        parts.reverse();
        let mut out = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 && !out.ends_with(self.sep) {
                out.push(self.sep);
            }
            out.push_str(part);
        }
        out
    }

    /// 親のパス(2行目に出す)
    pub fn parent_path(&self, id: u32) -> String {
        self.parent(id).map(|p| self.path(p)).unwrap_or_default()
    }

    /// メモリの目安(バイト)
    pub fn memory(&self) -> usize {
        self.names.capacity() + self.lower.capacity() + (self.name_end.capacity() + self.lower_end.capacity() + self.parent.capacity() + self.mtime.capacity()) * 4 + self.flags.capacity()
    }

    pub fn shrink(&mut self) {
        self.names.shrink_to_fit();
        self.lower.shrink_to_fit();
        self.name_end.shrink_to_fit();
        self.lower_end.shrink_to_fit();
        self.parent.shrink_to_fit();
        self.flags.shrink_to_fit();
        self.mtime.shrink_to_fit();
    }

    fn search_range(&self, matcher: Matcher<'_>, range: std::ops::Range<usize>, k: usize, cancel: &AtomicBool, dirs_only: bool) -> TopK<FileHit> {
        let mut top = TopK::new(k);
        for i in range {
            if i % 8192 == 0 && cancel.load(Ordering::Relaxed) {
                break;
            }
            let id = i as u32;
            let fl = self.flags[i];
            if self.parent[i] == NO_PARENT || (dirs_only && fl & FLAG_DIR == 0) {
                continue;
            }
            let Some(m) = matcher.matches(self, id, fl) else { continue };
            let (score, positions) = match m {
                // 条件だけに一致したときは更新が新しい順.
                Matched::Filter => (self.mtime[i].min(i32::MAX as u32) as i32, vec![]),
                Matched::Name(m) => (fuzzy::rank_score(&m, self.name(id)), m.positions),
            };
            top.push(score, FileHit { id, score, positions });
        }
        top
    }

    /// 親をたどって, 名前にこの語を含むフォルダーがあるか調べる.
    fn in_folder(&self, id: u32, text: &str) -> bool {
        let mut cur = self.parent(id);
        while let Some(parent) = cur {
            if self.lower(parent).contains(text) {
                return true;
            }
            cur = self.parent(parent);
        }
        false
    }

    /// 上位k件を探す. 件数が多いときはCPUの数に分けて並べて探す. `cancel`が立ったら途中でやめる(J1: 新しい入力で古い検索を捨てる).
    pub fn search(&self, p: &Prepared, k: usize, cancel: &AtomicBool) -> Vec<FileHit> {
        self.search_matching(Matcher::Name(p), k, cancel, false)
    }

    pub fn search_query(&self, search: &FileSearch, k: usize, cancel: &AtomicBool, dirs_only: bool) -> Vec<FileHit> {
        self.search_matching(search.matcher(), k, cancel, dirs_only)
    }

    fn search_matching(&self, matcher: Matcher<'_>, k: usize, cancel: &AtomicBool, dirs_only: bool) -> Vec<FileHit> {
        if matcher.is_empty() {
            return vec![];
        }
        let n = self.len();
        let threads = std::thread::available_parallelism().map(|t| t.get()).unwrap_or(1).min(8);
        if n < 20_000 || threads == 1 {
            return self.search_range(matcher, 0..n, k, cancel, dirs_only).into_sorted().into_iter().map(|(_, h)| h).collect();
        }
        let chunk = n.div_ceil(threads);
        let mut all = TopK::new(k);
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let range = t * chunk..((t + 1) * chunk).min(n);
                    s.spawn(move || self.search_range(matcher, range, k, cancel, dirs_only))
                })
                .collect();
            for h in handles {
                if let Ok(top) = h.join() {
                    all.merge(top);
                }
            }
        });
        all.into_sorted().into_iter().map(|(_, h)| h).collect()
    }
}

/// `*`で区切った名前の形. 区切りごとに, どれかに合えばよい語を持つ(ローマ字なら日本語にも広げる. K51)
#[derive(Debug, Clone, PartialEq)]
pub struct Glob {
    segs: Vec<Vec<String>>,
}

impl Glob {
    fn new(pattern: &str) -> Glob {
        Glob { segs: pattern.split('*').map(|s| vec![s.to_string()]).collect() }
    }

    fn matches(&self, name: &str) -> bool {
        self.rest(0, name)
    }

    fn rest(&self, i: usize, name: &str) -> bool {
        let last = i + 1 == self.segs.len();
        self.segs[i].iter().any(|w| {
            if i == 0 {
                name.strip_prefix(w.as_str()).is_some_and(|r| if last { r.is_empty() } else { self.rest(1, r) })
            } else if last {
                name.ends_with(w.as_str())
            } else {
                name.find(w.as_str()).is_some_and(|at| self.rest(i + 1, &name[at + w.len()..]))
            }
        })
    }
    /// ローマ字を日本語の名前にも広げる.
    fn expand_romaji(&mut self, dict: Option<&Dict>) {
        for seg in &mut self.segs {
            let e = migemo::expand(&seg[0], dict);
            seg.extend(e.words().iter().map(|w| w.to_lowercase()));
        }
    }
}

/// 走査で除く名前(D1). 大文字小文字は区別しない. `*`は何文字にでも合う(K44)
#[derive(Debug, Clone)]
pub struct Excludes {
    names: Vec<String>,
    patterns: Vec<String>,
}

impl Excludes {
    pub fn new(names: &[String]) -> Excludes {
        let (patterns, names) = names.iter().map(|n| n.to_lowercase()).partition(|n| n.contains('*'));
        Excludes { names, patterns }
    }

    pub fn contains(&self, name: &str) -> bool {
        let l = name.to_lowercase();
        self.names.iter().any(|n| *n == l) || self.patterns.iter().any(|p| glob(p, &l))
    }
}

/// `*`(何文字にでも合う)だけの形に合うか
fn glob(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    let Some(mut rest) = name.strip_prefix(first).and_then(|r| r.strip_suffix(last)) else { return false };
    for p in &parts[1..parts.len() - 1] {
        match rest.find(p) {
            Some(i) => rest = &rest[i + p.len()..],
            None => return false,
        }
    }
    true
}

/// 同じ場所か(大文字小文字と末尾の区切りを無視する. Windowsのパスは大文字小文字を区別しないため)
pub fn same_path(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
    n(a) == n(b)
}

/// 走査の結果の数え上げ
#[derive(Debug, Default, Clone)]
pub struct ScanStats {
    pub files: usize,
    pub dirs: usize,
    pub errors: usize,
}

/// フォルダーを走査して索引に足す. シンボリックリンクとジャンクションはたどらない(ループを防ぐ).
/// `skip_paths`に入ったフォルダー(例: ユーザーフォルダーの中のBox)は別の索引で扱うので入らない.
pub fn scan_into(table: &mut FileTable, root: &Path, excludes: &Excludes, skip_paths: &[std::path::PathBuf], cancel: &AtomicBool) -> ScanStats {
    let mut stats = ScanStats::default();
    let root_id = table.push(None, &root.to_string_lossy(), true, 0);
    let mut stack: Vec<(std::path::PathBuf, u32)> = vec![(root.to_path_buf(), root_id)];
    while let Some((dir, id)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            stats.errors += 1;
            continue;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let name = e.file_name();
            let name = name.to_string_lossy();
            // Windowsでは列挙のときに得た値なので追加の読み出しはない
            let mtime = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs().min(u32::MAX as u64) as u32);
            if excludes.contains(&name) {
                continue;
            }
            if ft.is_dir() {
                let path = e.path();
                if skip_paths.iter().any(|s| same_path(s, &path)) {
                    continue;
                }
                let cid = table.push(Some(id), &name, true, mtime);
                stats.dirs += 1;
                stack.push((path, cid));
            } else {
                table.push(Some(id), &name, false, mtime);
                stats.files += 1;
            }
        }
    }
    stats
}

/// WSL側の`find`の出力(1行に`d 相対パス`か`f 相対パス`)を索引にする(D4).
/// 根はWindowsから開ける形(`\\wsl.localhost\<distro>\home\me`)にし, 区切りは`\`にする.
pub fn table_from_wsl_list(root_unc: &str, list: &str) -> FileTable {
    let mut t = FileTable::new('\\');
    let root = t.push(None, root_unc, true, 0);
    // 相対パス → 番号. findは親を先に出すので, 親の番号はすでにある
    let mut dirs: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for line in list.lines() {
        let Some((kind, rel)) = line.split_once(' ') else { continue };
        let rel = rel.trim_end_matches('/');
        if rel.is_empty() || rel == "." {
            continue;
        }
        let (parent_rel, name) = match rel.rfind('/') {
            Some(i) => (&rel[..i], &rel[i + 1..]),
            None => ("", rel),
        };
        let pid = if parent_rel.is_empty() { root } else {
            match dirs.get(parent_rel) {
                Some(&p) => p,
                None => continue,
            }
        };
        let is_dir = kind == "d";
        let id = t.push(Some(pid), name, is_dir, 0);
        if is_dir {
            dirs.insert(rel.to_string(), id);
        }
    }
    t.shrink();
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FileTable {
        let mut t = FileTable::new('\\');
        let root = t.push(None, r"C:\Users\me", true, 0);
        let docs = t.push(Some(root), "Documents", true, 0);
        t.push(Some(docs), "報告書.docx", false, 0);
        t.push(Some(docs), "proposal.docx", false, 0);
        let box_ = t.push(Some(root), "Box", true, 0);
        t.push(Some(box_), "会議資料.xlsx", false, 0);
        t
    }

    #[test]
    fn excludes_match_names_and_stars() {
        let ex = Excludes::new(&["Node_Modules".into(), "*.tmp".into(), "~$*".into(), "a*b*c".into()]);
        for n in ["node_modules", "x.TMP", "~$book.xlsx", "abc", "a1b2c", ".tmp"] {
            assert!(ex.contains(n), "{n}");
        }
        for n in ["node_modules2", "x.tmp2", "book.xlsx", "ac", "a1c", "ab"] {
            assert!(!ex.contains(n), "{n}");
        }
    }

    #[test]
    fn paths() {
        let t = sample();
        assert_eq!(t.path(2), r"C:\Users\me\Documents\報告書.docx");
        assert_eq!(t.parent_path(2), r"C:\Users\me\Documents");
        assert!(t.is_dir(1));
        assert!(!t.is_dir(2));
    }

    #[test]
    fn search_files() {
        let t = sample();
        let cancel = AtomicBool::new(false);
        let h = t.search(&Prepared::new("prop", None), 8, &cancel);
        assert_eq!(t.name(h[0].id), "proposal.docx");
        let h = t.search(&Prepared::new("doc", None), 8, &cancel);
        // 名前の先頭が一致したフォルダーが上
        assert_eq!(t.name(h[0].id), "Documents");
        let h = t.search(&Prepared::new("会議", None), 8, &cancel);
        assert_eq!(t.name(h[0].id), "会議資料.xlsx");
        let h = t.search_query(&FileQuery::parse("o").prepare(None, 0), 8, &cancel, true);
        assert!(h.iter().all(|x| t.is_dir(x.id)));
    }

    #[test]
    fn parallel_search_matches_serial() {
        let mut t = FileTable::new('\\');
        let root = t.push(None, r"C:\r", true, 0);
        for i in 0..50_000 {
            t.push(Some(root), &format!("file_{i:05}_{}.txt", if i % 997 == 0 { "target" } else { "x" }), false, 0);
        }
        let cancel = AtomicBool::new(false);
        let h = t.search(&Prepared::new("target", None), 100, &cancel);
        assert_eq!(h.len(), 51);
    }

    #[test]
    fn scan_real_dir() {
        let dir = std::env::temp_dir().join(format!("hikidashi_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/node_modules/x")).unwrap();
        std::fs::create_dir_all(dir.join("Box/team")).unwrap();
        std::fs::write(dir.join("a/report.txt"), "").unwrap();
        std::fs::write(dir.join("a/node_modules/x/skip.js"), "").unwrap();
        std::fs::write(dir.join("Box/team/b.txt"), "").unwrap();
        let mut t = FileTable::new(std::path::MAIN_SEPARATOR);
        let ex = Excludes::new(&["node_modules".to_string()]);
        let cancel = AtomicBool::new(false);
        let st = scan_into(&mut t, &dir, &ex, &[dir.join("Box")], &cancel);
        assert_eq!(st.files, 1);
        assert_eq!(st.dirs, 1);
        let names: Vec<&str> = (0..t.len() as u32).map(|i| t.name(i)).collect();
        assert!(names.contains(&"report.txt"));
        assert!(!names.contains(&"skip.js"));
        assert!(!names.contains(&"b.txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wsl_list() {
        let t = table_from_wsl_list(r"\\wsl.localhost\Ubuntu\home\me", "d project\nd project/work\nf project/work/a.rs\nf .zshrc\n");
        assert_eq!(t.len(), 5);
        assert_eq!(t.path(3), r"\\wsl.localhost\Ubuntu\home\me\project\work\a.rs");
    }

    /// 絞り込みの確認用の表. 更新日時は`NOW`から何日前かで決める.
    const NOW: u32 = 1_800_000_000;
    const DAY: u32 = 86_400;

    fn filter_table() -> FileTable {
        let mut t = FileTable::new('\\');
        let root = t.push(None, r"C:\Users\me", true, 0);
        let anken = t.push(Some(root), "案件A", true, NOW - 30 * DAY);
        let sub = t.push(Some(anken), "Reports", true, NOW - 30 * DAY);
        let other = t.push(Some(root), "private", true, NOW - 30 * DAY);
        t.push(Some(sub), "見積.pdf", false, NOW - 2 * DAY); // 1
        t.push(Some(sub), "memo.docx", false, NOW - 20 * DAY); // 2
        t.push(Some(other), "見積.PDF", false, NOW - 10 * DAY); // 3
        t.push(Some(other), "backup.tar.gz", false, NOW - DAY / 2); // 4
        t.push(Some(anken), "notes.pdf", false, 0); // 5(日時なし)
        t.push(Some(other), "report.pdf", false, NOW - 5 * DAY); // 6
        t
    }

    fn names(t: &FileTable, q: &str) -> Vec<String> {
        let query = FileQuery::parse(q).prepare(None, NOW);
        let cancel = AtomicBool::new(false);
        t.search_query(&query, 50, &cancel, false).iter().map(|h| t.name(h.id).to_string()).collect()
    }

    #[test]
    fn only_complete_search_expressions_are_structured() {
        for q in ["*.pdf", "EXT:PDF,.docx", "modified:12H", "modified:2w", "(見積)", "in:\"Project Alpha\"", "!in:private"] {
            assert!(FileQuery::parse(q).is_structured(), "{q}");
        }
        for q in [
            "ext:", "in:", "modified:", "modified:abc", "modified:7", "ext:,", "report ext:",
            "*.pdf in:案件", "見積 2026", "R&D", "report(2026)", "2 ** 3", "*", "a &", "a |",
            "()", "(a | b", "a )", "in:\"Project Alpha",
        ] {
            assert!(!FileQuery::parse(q).is_structured(), "{q}");
        }
        let too_deep = format!("{}*.pdf{}", "(".repeat(64), ")".repeat(64));
        assert!(!FileQuery::parse(&too_deep).is_structured());
    }

    #[test]
    fn filter_by_ext() {
        let t = filter_table();
        let mut v = names(&t, "ext:pdf");
        v.sort();
        assert_eq!(v, vec!["notes.pdf", "report.pdf", "見積.PDF", "見積.pdf"]);
        assert_eq!(names(&t, "ext:tar.gz"), vec!["backup.tar.gz"]);
        assert_eq!(names(&t, "ext:gz"), vec!["backup.tar.gz"]);
        assert_eq!(names(&t, "memo & ext:docx"), vec!["memo.docx"]);
        assert!(names(&t, "memo & ext:pdf").is_empty());
        assert_eq!(names(&t, "ext:docx,gz").len(), 2);
    }

    #[test]
    fn filter_by_glob() {
        let mut t = filter_table();
        let mut v = names(&t, "*.PDF");
        v.sort();
        assert_eq!(v, vec!["notes.pdf", "report.pdf", "見積.PDF", "見積.pdf"]);
        assert_eq!(names(&t, "re*.pdf"), vec!["report.pdf"]);
        assert_eq!(names(&t, "見積* & in:reports"), vec!["見積.pdf"]);
        assert_eq!(names(&t, "*port*.pdf"), vec!["report.pdf"]);
        assert!(names(&t, "*.pdf & memo").is_empty());
        t.push(Some(0), "報告(最終)版.pdf", false, NOW);
        assert_eq!(names(&t, "(報告(最終)*.pdf | *.gz) & !in:private"), vec!["報告(最終)版.pdf"]);
        // ローマ字は日本語の名前にも合う
        let mut glob = Glob::new("kaigi*.pdf");
        let dict = Dict::parse("かいぎ /会議/\n");
        assert!(!glob.matches("会議資料.pdf"));
        glob.expand_romaji(Some(&dict));
        assert!(glob.matches("会議資料.pdf") && glob.matches("カイギ.pdf") && glob.matches("kaigi.pdf"));
        assert!(!glob.matches("会議資料.docx") && !glob.matches("定例会議.pdf"));
    }

    #[test]
    fn filter_by_dir_and_modified() {
        let t = filter_table();
        // 親のどこかに名前が含まれるフォルダー(大文字小文字を区別しない). フォルダー自身は入らない
        let mut v = names(&t, "in:reports");
        v.sort();
        assert_eq!(v, vec!["memo.docx", "見積.pdf"]);
        assert_eq!(names(&t, "in:案件").len(), 4);
        assert_eq!(names(&t, "in:案件 & in:report").len(), 2);
        // 新しい順. 日時のない(0の)ものは外す
        assert_eq!(names(&t, "modified:7d"), vec!["backup.tar.gz", "見積.pdf", "report.pdf"]);
        assert_eq!(names(&t, "modified:12h"), vec!["backup.tar.gz"]);
        assert_eq!(names(&t, "modified:1w & ext:pdf & in:private"), vec!["report.pdf"]);
        assert_eq!(names(&t, "ext:pdf & modified:30d & in:案件"), vec!["見積.pdf"]);
        // 残りの語はあいまい一致のまま
        assert_eq!(names(&t, "mem & in:案件 & modified:30d"), vec!["memo.docx"]);
    }

    #[test]
    fn operators_and_grouping() {
        let t = filter_table();
        assert_eq!(names(&t, "ext:gz | ext:pdf & in:reports"), vec!["backup.tar.gz", "見積.pdf"]);
        assert_eq!(names(&t, "(ext:gz | ext:pdf) & in:reports"), vec!["見積.pdf"]);
        assert_eq!(names(&t, "(*.pdf | *.docx) & modified:7d & !in:private"), vec!["見積.pdf"]);
        assert_eq!(names(&t, "!(in:private | ext:pdf) & ext:docx"), vec!["memo.docx"]);
        assert_eq!(names(&t, "ext:pdf & !modified:7d & in:案件"), vec!["notes.pdf"]);
        assert_eq!(names(&t, "pdf & report"), vec!["report.pdf"]);
        assert!(names(&t, "pdf report").is_empty());
        assert!(names(&t, "*.pdf in:案件").is_empty());
    }

    #[test]
    fn quoted_names_and_filter_values() {
        let mut t = filter_table();
        let project = t.push(Some(0), "Project Alpha", true, 0);
        let id = t.push(Some(project), "会議 Plan & Review.pdf", false, NOW);
        t.push(Some(project), "Plans & Review.pdf", false, NOW);
        assert_eq!(names(&t, "in:\"project alpha\" & \"Plan & Review\""), vec!["会議 Plan & Review.pdf"]);
        let query = FileQuery::parse("\"Plan & Review\" & ext:pdf").prepare(None, NOW);
        let hits = t.search_query(&query, 50, &AtomicBool::new(false), false);
        assert_eq!(hits[0].id, id);
        assert_eq!(hits[0].positions, (3..16).collect::<Vec<_>>());
    }

    /// 検索の速さ. 50万件で, 前から1文字ずつ打つように検索する(29回). `[profile.test] opt-level = 2`(ルートのCargo.toml)で最適化している.
    /// Raspberry Piでの実測: 1回あたりp95が0.11〜0.12秒(ほかの処理と重なると0.25秒), 最大0.13〜0.28秒. 上限はp95の約3倍(0.4秒), 最大は0.8秒.
    /// 参考: 最適化なしのdebugだと同じ50万件でp95が1.9秒, releaseだと0.097秒(最大0.16秒).
    #[test]
    fn typing_speed() {
        let words = ["report", "会議資料", "proposal", "budget", "定例", "design", "spec", "テスト", "image", "data", "kaigi", "mitsumori", "pwpt", "見積書", "plan", "2026", "測定結果"];
        let exts = ["docx", "xlsx", "pptx", "pdf", "txt", "png", "csv", "rs"];
        let mut t = FileTable::new('\\');
        let root = t.push(None, r"C:\Users\me", true, 0);
        let mut dirs = vec![root];
        let mut seed: u64 = 88172645463325252;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        const COUNT: u32 = 500_000;
        for i in 0..COUNT {
            let p = dirs[(rnd() as usize) % dirs.len()];
            if i % 20 == 0 {
                let id = t.push(Some(p), &format!("{}_{}", words[(rnd() % 17) as usize], i), true, 1_700_000_000 + i);
                dirs.push(id);
            } else {
                let n = format!("{}_{}_{}.{}", words[(rnd() % 17) as usize], words[(rnd() % 17) as usize], i, exts[(rnd() % 8) as usize]);
                t.push(Some(p), &n, false, 1_700_000_000 + i);
            }
        }
        t.shrink();
        let cancel = AtomicBool::new(false);
        let mut times: Vec<f64> = Vec::new();
        for q in ["kaigi", "pwpt", "mitsumori", "report 2026"] {
            let chars: Vec<char> = q.chars().collect();
            for n in 1..=chars.len() {
                let text: String = chars[..n].iter().collect();
                let p = Prepared::new(&text, None);
                let t0 = std::time::Instant::now();
                let h = t.search(&p, 50, &cancel);
                let el = t0.elapsed().as_secs_f64();
                assert!(h.len() <= 50);
                times.push(el);
            }
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95 = times[((times.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)];
        let max = *times.last().unwrap();
        println!("typing_speed: n={}, p95={:.3}s, max={:.3}s", times.len(), p95, max);
        assert!(p95 < 0.4, "p95 {p95:.3}s");
        assert!(max < 0.8, "max {max:.3}s");
    }
}
