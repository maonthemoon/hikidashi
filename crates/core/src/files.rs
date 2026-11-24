//! ファイルとフォルダーの索引(D1〜D6). 数十万件を少ないメモリで持ち, 1文字ごとに全件を探す.
//! 名前は1つの大きな文字列に詰め, 親の番号でパスを組み立てる.

use crate::fuzzy::{self, lower_str};
use crate::search::{match_name, Prepared, TopK};
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

    fn search_range(&self, p: &Prepared, range: std::ops::Range<usize>, k: usize, cancel: &AtomicBool, dirs_only: bool, f: &Filters, min_mtime: u32) -> TopK<FileHit> {
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
            if min_mtime > 0 && self.mtime[i] < min_mtime {
                continue;
            }
            let lower = self.lower(id);
            if !f.exts.is_empty() && (fl & FLAG_DIR != 0 || !f.exts.iter().any(|e| lower.ends_with(e.as_str()))) {
                continue;
            }
            if p.is_empty() {
                // 検索語がなく絞り込みだけのとき: 更新が新しい順
                if self.in_ok(id, f) {
                    let score = self.mtime[i].min(i32::MAX as u32) as i32;
                    top.push(score, FileHit { id, score, positions: vec![] });
                }
                continue;
            }
            let name = self.name(id);
            if let Some(m) = match_name(p, name, lower, fl & FLAG_ASCII != 0) {
                if !self.in_ok(id, f) {
                    continue;
                }
                let score = fuzzy::rank_score(&m, name);
                top.push(score, FileHit { id, score, positions: m.positions });
            }
        }
        top
    }

    /// `in:`の絞り込み: 親をたどって, 名前に語を含むフォルダーが(語ごとに)1つはあるか. 自分自身は数えない.
    fn in_ok(&self, id: u32, f: &Filters) -> bool {
        if f.dirs.is_empty() {
            return true;
        }
        let mut need: Vec<bool> = vec![false; f.dirs.len()];
        let mut cur = self.parent(id);
        while let Some(c) = cur {
            let l = self.lower(c);
            for (i, d) in f.dirs.iter().enumerate() {
                if !need[i] && l.contains(d.as_str()) {
                    need[i] = true;
                }
            }
            if need.iter().all(|x| *x) {
                return true;
            }
            cur = self.parent(c);
        }
        false
    }

    /// 上位k件を探す. 件数が多いときはCPUの数に分けて並べて探す. `cancel`が立ったら途中でやめる(J1: 新しい入力で古い検索を捨てる).
    pub fn search(&self, p: &Prepared, k: usize, cancel: &AtomicBool) -> Vec<FileHit> {
        self.search_opt(p, k, cancel, false, &Filters::default(), 0)
    }

    /// 絞り込み付きの検索. `now`は今のUNIX秒(`modified:`の基準). 検索語が空でも絞り込みがあれば, 条件に合うものを新しい順に返す.
    pub fn search_opt(&self, p: &Prepared, k: usize, cancel: &AtomicBool, dirs_only: bool, f: &Filters, now: u32) -> Vec<FileHit> {
        if p.is_empty() && f.is_empty() {
            return vec![];
        }
        // 更新日時が0(分からない)ものは`modified:`で外すので, 下限は1以上にする
        let min_mtime = f.modified_secs.map_or(0, |s| now.saturating_sub(s.min(u32::MAX as u64) as u32).max(1));
        let n = self.len();
        let threads = std::thread::available_parallelism().map(|t| t.get()).unwrap_or(1).min(8);
        if n < 20_000 || threads == 1 {
            return self.search_range(p, 0..n, k, cancel, dirs_only, f, min_mtime).into_sorted().into_iter().map(|(_, h)| h).collect();
        }
        let chunk = n.div_ceil(threads);
        let mut all = TopK::new(k);
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let range = t * chunk..((t + 1) * chunk).min(n);
                    s.spawn(move || self.search_range(p, range, k, cancel, dirs_only, f, min_mtime))
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

/// 打った文字から取り出した絞り込み(`ext:pdf`, `in:案件`, `modified:7d`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filters {
    /// 拡張子(`.pdf`, `.tar.gz`のように先頭に`.`を付けた小文字). どれかに合えばよい. フォルダーは除く
    pub exts: Vec<String>,
    /// パスの途中にあるフォルダー名の部分(小文字). すべてについて, 含む親のフォルダーがあること
    pub dirs: Vec<String>,
    /// 更新が今から何秒以内か
    pub modified_secs: Option<u64>,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        self.exts.is_empty() && self.dirs.is_empty() && self.modified_secs.is_none()
    }
}

/// `7d`(日), `12h`(時間), `2w`(週)を秒にする.
fn parse_age(v: &str) -> Option<u64> {
    let unit = v.chars().last()?;
    let n: u64 = v[..v.len() - unit.len_utf8()].parse().ok()?;
    let mul = match unit.to_ascii_lowercase() {
        'h' => 3600,
        'd' => 86_400,
        'w' => 604_800,
        _ => return None,
    };
    Some(n.saturating_mul(mul))
}

/// 入力から`ext:`, `in:`, `modified:`の語を取り出す. 返すのは(残りの検索語, 絞り込み).
/// 値が空(`ext:`だけ)や読めない値(`modified:abc`)の語は絞り込みにせず, 普通の語として残す.
pub fn parse_filters(input: &str) -> (String, Filters) {
    let mut f = Filters::default();
    let mut rest: Vec<&str> = Vec::new();
    for w in input.split_whitespace() {
        let lw = w.to_lowercase();
        if let Some(v) = lw.strip_prefix("ext:") {
            let exts: Vec<String> = v.split(',').map(|e| e.trim_start_matches('.')).filter(|e| !e.is_empty()).map(|e| format!(".{e}")).collect();
            if !exts.is_empty() {
                f.exts.extend(exts);
                continue;
            }
        } else if let Some(v) = lw.strip_prefix("in:") {
            if !v.is_empty() {
                f.dirs.push(v.to_string());
                continue;
            }
        } else if let Some(v) = lw.strip_prefix("modified:") {
            if let Some(s) = parse_age(v) {
                f.modified_secs = Some(s);
                continue;
            }
        }
        rest.push(w);
    }
    (rest.join(" "), f)
}

/// 走査で除く名前(D1). 大文字小文字は区別しない.
#[derive(Debug, Clone)]
pub struct Excludes {
    names: Vec<String>,
}

impl Excludes {
    pub fn new(names: &[String]) -> Excludes {
        Excludes { names: names.iter().map(|n| n.to_lowercase()).collect() }
    }

    pub fn contains(&self, name: &str) -> bool {
        let l = name.to_lowercase();
        self.names.iter().any(|n| *n == l)
    }
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
        let h = t.search_opt(&Prepared::new("o", None), 8, &cancel, true, &Filters::default(), 0);
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
        let (rest, f) = parse_filters(q);
        let cancel = AtomicBool::new(false);
        t.search_opt(&Prepared::new(&rest, None), 50, &cancel, false, &f, NOW).iter().map(|h| t.name(h.id).to_string()).collect()
    }

    #[test]
    fn parse_filter_words() {
        let (rest, f) = parse_filters("kaigi ext:PDF,.docx in:案件 modified:7d");
        assert_eq!(rest, "kaigi");
        assert_eq!(f.exts, vec![".pdf", ".docx"]);
        assert_eq!(f.dirs, vec!["案件"]);
        assert_eq!(f.modified_secs, Some(7 * 86_400));
        assert_eq!(parse_filters("modified:12h").1.modified_secs, Some(12 * 3600));
        assert_eq!(parse_filters("modified:2w").1.modified_secs, Some(14 * 86_400));
        // 値が空や読めないときは普通の語
        for q in ["ext:", "in:", "modified:", "modified:abc", "modified:7", "ext:,", "report ext:"] {
            let (rest, f) = parse_filters(q);
            assert!(f.is_empty(), "{q}");
            assert_eq!(rest, q);
        }
        assert_eq!(parse_filters("a  ext:pdf   b").0, "a b");
    }

    #[test]
    fn filter_by_ext() {
        let t = filter_table();
        let mut v = names(&t, "ext:pdf");
        v.sort();
        assert_eq!(v, vec!["notes.pdf", "report.pdf", "見積.PDF", "見積.pdf"]);
        assert_eq!(names(&t, "ext:tar.gz"), vec!["backup.tar.gz"]);
        assert_eq!(names(&t, "ext:gz"), vec!["backup.tar.gz"]);
        assert_eq!(names(&t, "memo ext:docx"), vec!["memo.docx"]);
        assert!(names(&t, "memo ext:pdf").is_empty());
        assert_eq!(names(&t, "ext:docx,gz").len(), 2);
    }

    #[test]
    fn filter_by_dir_and_modified() {
        let t = filter_table();
        // 親のどこかに名前が含まれるフォルダー(大文字小文字を区別しない). フォルダー自身は入らない
        let mut v = names(&t, "in:reports");
        v.sort();
        assert_eq!(v, vec!["memo.docx", "見積.pdf"]);
        assert_eq!(names(&t, "in:案件").len(), 4);
        assert_eq!(names(&t, "in:案件 in:report").len(), 2);
        // 新しい順. 日時のない(0の)ものは外す
        assert_eq!(names(&t, "modified:7d"), vec!["backup.tar.gz", "見積.pdf", "report.pdf"]);
        assert_eq!(names(&t, "modified:12h"), vec!["backup.tar.gz"]);
        assert_eq!(names(&t, "modified:1w ext:pdf in:private"), vec!["report.pdf"]);
        assert_eq!(names(&t, "ext:pdf modified:30d in:案件"), vec!["見積.pdf"]);
        // 残りの語はあいまい一致のまま
        assert_eq!(names(&t, "mem in:案件 modified:30d"), vec!["memo.docx"]);
    }

    /// 検索の速さ. 10万件(debugのcargo testで30秒に収めるため)で, 前から1文字ずつ打つように検索する(29回).
    /// Raspberry Piのdebugビルドの実測: 1回あたりp95が0.19〜0.24秒, 最大0.25秒. 上限はp95の約3倍(0.7秒). 参考: 50万件はdebugでp95 1.9秒, releaseでp95 0.10秒(最大0.16秒).
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
        const COUNT: u32 = 100_000;
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
        assert!(p95 < 0.7, "p95 {p95:.3}s");
        assert!(max < 1.0, "max {max:.3}s");
    }
}
