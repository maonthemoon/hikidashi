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

    /// 項目を足して番号を返す. 根は`parent = None`で, 名前にパス全体を入れる.
    pub fn push(&mut self, parent: Option<u32>, name: &str, is_dir: bool) -> u32 {
        let id = self.parent.len() as u32;
        self.names.push_str(name);
        self.name_end.push(self.names.len() as u32);
        let l = lower_str(name);
        self.lower.push_str(&l);
        self.lower_end.push(self.lower.len() as u32);
        self.parent.push(parent.unwrap_or(NO_PARENT));
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
        self.names.capacity() + self.lower.capacity() + (self.name_end.capacity() + self.lower_end.capacity() + self.parent.capacity()) * 4 + self.flags.capacity()
    }

    pub fn shrink(&mut self) {
        self.names.shrink_to_fit();
        self.lower.shrink_to_fit();
        self.name_end.shrink_to_fit();
        self.lower_end.shrink_to_fit();
        self.parent.shrink_to_fit();
        self.flags.shrink_to_fit();
    }

    fn search_range(&self, p: &Prepared, range: std::ops::Range<usize>, k: usize, cancel: &AtomicBool, dirs_only: bool) -> TopK<FileHit> {
        let mut top = TopK::new(k);
        for i in range {
            if i % 8192 == 0 && cancel.load(Ordering::Relaxed) {
                break;
            }
            let id = i as u32;
            let f = self.flags[i];
            if self.parent[i] == NO_PARENT || (dirs_only && f & FLAG_DIR == 0) {
                continue;
            }
            let name = self.name(id);
            if let Some(m) = match_name(p, name, self.lower(id), f & FLAG_ASCII != 0) {
                let score = fuzzy::rank_score(&m, name);
                top.push(score, FileHit { id, score, positions: m.positions });
            }
        }
        top
    }

    /// 上位k件を探す. 件数が多いときはCPUの数に分けて並べて探す. `cancel`が立ったら途中でやめる(J1: 新しい入力で古い検索を捨てる).
    pub fn search(&self, p: &Prepared, k: usize, cancel: &AtomicBool) -> Vec<FileHit> {
        self.search_opt(p, k, cancel, false)
    }

    pub fn search_opt(&self, p: &Prepared, k: usize, cancel: &AtomicBool, dirs_only: bool) -> Vec<FileHit> {
        if p.is_empty() {
            return vec![];
        }
        let n = self.len();
        let threads = std::thread::available_parallelism().map(|t| t.get()).unwrap_or(1).min(8);
        if n < 20_000 || threads == 1 {
            return self.search_range(p, 0..n, k, cancel, dirs_only).into_sorted().into_iter().map(|(_, h)| h).collect();
        }
        let chunk = n.div_ceil(threads);
        let mut all = TopK::new(k);
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let range = t * chunk..((t + 1) * chunk).min(n);
                    s.spawn(move || self.search_range(p, range, k, cancel, dirs_only))
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
    let root_id = table.push(None, &root.to_string_lossy(), true);
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
            if excludes.contains(&name) {
                continue;
            }
            if ft.is_dir() {
                let path = e.path();
                if skip_paths.iter().any(|s| same_path(s, &path)) {
                    continue;
                }
                let cid = table.push(Some(id), &name, true);
                stats.dirs += 1;
                stack.push((path, cid));
            } else {
                table.push(Some(id), &name, false);
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
    let root = t.push(None, root_unc, true);
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
        let id = t.push(Some(pid), name, is_dir);
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
        let root = t.push(None, r"C:\Users\me", true);
        let docs = t.push(Some(root), "Documents", true);
        t.push(Some(docs), "報告書.docx", false);
        t.push(Some(docs), "proposal.docx", false);
        let box_ = t.push(Some(root), "Box", true);
        t.push(Some(box_), "会議資料.xlsx", false);
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
        let h = t.search_opt(&Prepared::new("o", None), 8, &cancel, true);
        assert!(h.iter().all(|x| t.is_dir(x.id)));
    }

    #[test]
    fn parallel_search_matches_serial() {
        let mut t = FileTable::new('\\');
        let root = t.push(None, r"C:\r", true);
        for i in 0..50_000 {
            t.push(Some(root), &format!("file_{i:05}_{}.txt", if i % 997 == 0 { "target" } else { "x" }), false);
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
}
