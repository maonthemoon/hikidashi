//! Box Driveの手元のデータ(`sync.db`. SQLite)からBoxの一覧を作る(K18).
//! フォルダーを1つずつ開いて数えると, Box Driveがサーバーに聞くので数時間かかる(B1).
//! `sync.db`の`box_item`表には, ファイルとフォルダーの名前, 親, BoxのIDが入っているので, それを読む.
//! 表の形はBoxが公開している仕様ではないので, 列は実行時に`PRAGMA table_info`で見て選ぶ(`Columns::detect`).

use crate::files::{same_path, Excludes, FileTable};
use rusqlite::types::Value;
pub use rusqlite::Connection;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// 使う列. 選び方は`detect`の1か所にまとめる(列名が分かったらここを直す)
#[derive(Debug, Clone, PartialEq)]
pub struct Columns {
    /// 表の中のID(親が指すのがこちらのとき). `rowid`のこともある
    pub key: String,
    /// BoxのID(URLの`/file/<id>`)
    pub box_id: String,
    pub parent: String,
    pub name: String,
    /// ファイルかフォルダーかの列. なければ子を持つものをフォルダーとする
    pub kind: Option<String>,
    /// 消えた項目やゴミ箱の項目の印の列
    pub deleted: Vec<String>,
    /// 表にある列の名前全部(ログ用)
    pub all: Vec<String>,
}

impl Columns {
    /// 列の名前から使う列を選ぶ. 合う列がなければ理由を返す
    pub fn detect(names: &[String]) -> Result<Columns, String> {
        let lower: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
        let find_exact = |cands: &[&str]| cands.iter().find_map(|c| lower.iter().position(|n| n == c));
        let find_contains = |pred: &dyn Fn(&str) -> bool| lower.iter().position(|n| pred(n));
        let pick = |i: usize| names[i].clone();

        let name = find_exact(&["name", "item_name", "file_name", "filename"])
            .or_else(|| find_contains(&|n| n.contains("name") && !n.contains("parent") && !n.contains("path")))
            .map(pick)
            .ok_or("box_item has no name column")?;
        let box_id = find_exact(&["box_id", "item_box_id", "boxid", "item_id"])
            .or_else(|| find_contains(&|n| n.contains("box_id") && !n.contains("parent")))
            .or_else(|| find_exact(&["id"]))
            .map(pick)
            .ok_or("box_item has no box_id column")?;
        let parent = find_exact(&["parent_id", "parent_box_id", "parent_item_id", "parent"])
            .or_else(|| find_contains(&|n| n.contains("parent")))
            .map(pick)
            .ok_or("box_item has no parent column")?;
        let key = match find_exact(&["id", "item_id", "local_id", "_id"]).map(pick) {
            Some(k) if k != box_id => k,
            _ => "rowid".to_string(),
        };
        let kind = find_exact(&["item_type", "type", "kind", "is_folder", "is_dir", "is_directory", "folder"])
            .or_else(|| find_contains(&|n| n.contains("folder") || n.contains("is_dir") || n.ends_with("_type") && n.contains("item")))
            .map(pick);
        let deleted = names
            .iter()
            .zip(&lower)
            .filter(|(_, l)| l.contains("deleted") || l.contains("trash") || l.contains("removed"))
            .map(|(n, _)| n.clone())
            .collect();
        Ok(Columns { key, box_id, parent, name, kind, deleted, all: names.to_vec() })
    }

    /// ログに書く1行(列の選び方)
    pub fn describe(&self) -> String {
        format!(
            "columns [{}]; using key={} box_id={} parent={} name={} kind={} deleted=[{}]",
            self.all.join(", "),
            self.key,
            self.box_id,
            self.parent,
            self.name,
            self.kind.as_deref().unwrap_or("-"),
            self.deleted.join(", ")
        )
    }
}

/// 値を文字にする(IDは数か文字). NULLや空は空
fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => s.trim().to_string(),
        Value::Blob(b) => String::from_utf8_lossy(b).trim().to_string(),
    }
}

/// 印が立っているか(0, 空, NULL, false以外)
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Integer(i) => *i != 0,
        Value::Real(f) => *f != 0.0,
        Value::Text(s) => {
            let l = s.trim().to_ascii_lowercase();
            !(l.is_empty() || l == "0" || l == "false" || l == "no")
        }
        Value::Blob(b) => !b.is_empty(),
    }
}

/// 読んだ1項目
struct Raw {
    key: String,
    box_id: String,
    parent: String,
    name: String,
    kind: Option<Value>,
}

/// 読んだ結果の数(ログ用)
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Stats {
    /// 表の行の数
    pub rows: usize,
    /// 消えた印やゴミ箱で除いた数
    pub deleted: usize,
    /// 一覧に入れた数(根を除く)
    pub placed: usize,
    /// 親がたどれず入れなかった数
    pub orphans: usize,
    /// 除く名前や除く場所で入れなかった数
    pub skipped: usize,
    /// 親の列が指すもの(`box_id`か表のID)
    pub parent_refers: String,
}

/// Boxの一覧と, BoxのIDから一覧の番号を引く表
pub struct BoxIndex {
    pub table: FileTable,
    /// ファイルのBoxのID → 一覧の番号
    pub files: HashMap<u64, u32>,
    /// フォルダーのBoxのID → 一覧の番号. Boxではファイルとフォルダーの番号は別々に振られ, 同じ数があり得るので分ける
    pub folders: HashMap<u64, u32>,
    pub columns: Columns,
    pub stats: Stats,
}

impl BoxIndex {
    /// BoxのIDの手元のパスと, フォルダーか. `folder`はURLが`/folder/`ならtrue(`/file/`ならfalse)
    pub fn path_of(&self, box_id: &str, folder: bool) -> Option<(String, bool)> {
        let id: u64 = box_id.trim().parse().ok()?;
        let n = *if folder { self.folders.get(&id) } else { self.files.get(&id) }?;
        Some((self.table.path(n), self.table.is_dir(n)))
    }
}

/// フォルダーとみなす種類の値か(文字で分かるとき)
fn kind_word(v: &Value) -> Option<bool> {
    if let Value::Text(s) = v {
        let l = s.to_ascii_lowercase();
        if l.contains("folder") || l.contains("dir") {
            return Some(true);
        }
        if l.contains("file") || l.contains("web_link") || l.contains("weblink") {
            return Some(false);
        }
    }
    None
}

/// `sync.db`を開いた`Connection`からBoxの一覧を作る. `root`はBox Driveの手元の根(`C:\Users\me\Box`).
/// 除く名前(`AppData`など)と除く場所はユーザーフォルダーと同じに扱う.
pub fn read_index(conn: &Connection, root: &str, excludes: &Excludes, skip_paths: &[std::path::PathBuf]) -> Result<BoxIndex, String> {
    let names: Vec<String> = {
        let mut st = conn.prepare("PRAGMA table_info(box_item)").map_err(|e| e.to_string())?;
        let rows = st.query_map([], |r| r.get::<_, String>(1)).map_err(|e| e.to_string())?;
        rows.filter_map(Result::ok).collect()
    };
    if names.is_empty() {
        return Err("sync.db has no box_item table".into());
    }
    let cols = Columns::detect(&names)?;
    let q = |c: &str| if c == "rowid" { "rowid".to_string() } else { format!("\"{}\"", c.replace('"', "\"\"")) };
    let mut select = vec![q(&cols.key), q(&cols.box_id), q(&cols.parent), q(&cols.name)];
    if let Some(k) = &cols.kind {
        select.push(q(k));
    }
    for d in &cols.deleted {
        select.push(q(d));
    }
    let sql = format!("SELECT {} FROM box_item", select.join(", "));
    let mut st = conn.prepare(&sql).map_err(|e| format!("{e} ({sql})"))?;
    let has_kind = cols.kind.is_some();
    let n_del = cols.deleted.len();
    let mut stats = Stats::default();
    let mut raws: Vec<Raw> = Vec::new();
    let mut rows = st.query([]).map_err(|e| e.to_string())?;
    while let Some(r) = rows.next().map_err(|e| e.to_string())? {
        stats.rows += 1;
        let get = |i: usize| r.get::<_, Value>(i).unwrap_or(Value::Null);
        let first_del = if has_kind { 5 } else { 4 };
        if (0..n_del).any(|j| truthy(&get(first_del + j))) {
            stats.deleted += 1;
            continue;
        }
        let name = text(&get(3));
        raws.push(Raw { key: text(&get(0)), box_id: text(&get(1)), parent: text(&get(2)), name, kind: if has_kind { Some(get(4)) } else { None } });
    }
    drop(rows);
    drop(st);

    // 親の列が指すのは, BoxのIDか表のIDか(多く一致する方)
    let box_ids: HashSet<&str> = raws.iter().map(|r| r.box_id.as_str()).filter(|s| !s.is_empty()).collect();
    let keys: HashSet<&str> = raws.iter().map(|r| r.key.as_str()).filter(|s| !s.is_empty()).collect();
    let hits_box = raws.iter().filter(|r| box_ids.contains(r.parent.as_str())).count();
    let hits_key = raws.iter().filter(|r| keys.contains(r.parent.as_str())).count();
    let by_box = hits_box >= hits_key;
    stats.parent_refers = if by_box { cols.box_id.clone() } else { cols.key.clone() };
    let ref_of = |r: &Raw| -> String { if by_box { r.box_id.clone() } else { r.key.clone() } };

    // 種類の列が0と1の数だけなら, 1がフォルダー(実際のsync.db: `item_type`は0がファイル, 1がフォルダー)
    let binary_kind = has_kind
        && raws.iter().all(|r| matches!(r.kind, Some(Value::Integer(0)) | Some(Value::Integer(1)) | Some(Value::Null)));
    let known = |i: usize| -> Option<bool> {
        let v = raws[i].kind.as_ref()?;
        if binary_kind {
            return match v {
                Value::Integer(1) => Some(true),
                Value::Integer(0) => Some(false),
                _ => None,
            };
        }
        kind_word(v)
    };

    // 子の一覧
    let mut index_of: HashMap<String, usize> = HashMap::new();
    for (i, r) in raws.iter().enumerate() {
        index_of.entry(ref_of(r)).or_insert(i);
    }
    let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in raws.iter().enumerate() {
        children.entry(r.parent.as_str()).or_default().push(i);
    }

    // フォルダーとみなす種類の値: 文字で分かればそれ, 分からなければ子を持つ項目がある値(ファイルは子を持たない)
    let has_children = |i: usize| children.get(ref_of(&raws[i]).as_str()).is_some_and(|c| !c.is_empty());
    let mut folder_values: HashSet<String> = HashSet::new();
    if has_kind {
        let mut seen: HashMap<String, (usize, usize)> = HashMap::new();
        for (i, r) in raws.iter().enumerate() {
            if let Some(v) = &r.kind {
                let e = seen.entry(text(v)).or_default();
                e.0 += 1;
                if has_children(i) {
                    e.1 += 1;
                }
            }
        }
        for (v, (_, with_child)) in seen {
            if with_child > 0 {
                folder_values.insert(v);
            }
        }
    }
    let is_folder = |i: usize| -> bool {
        // 種類が分かるときはそれだけで決める(同じ番号のフォルダーの子を, ファイルの子と取り違えないため)
        if let Some(b) = known(i) {
            return b;
        }
        if has_children(i) {
            return true;
        }
        match &raws[i].kind {
            Some(v) => match kind_word(v) {
                Some(b) => b,
                None => {
                    let name = cols.kind.as_deref().unwrap_or("").to_ascii_lowercase();
                    if name.contains("folder") || name.contains("is_dir") || name.contains("directory") {
                        truthy(v)
                    } else {
                        folder_values.contains(&text(v))
                    }
                }
            },
            None => false,
        }
    };

    // 根: BoxのIDが0の項目(すべてのファイル). なければ, 表にない親の値のうち一番多いもの
    let root_ref: String = match raws.iter().enumerate().position(|(i, r)| r.box_id == "0" && known(i) != Some(false)) {
        Some(i) => ref_of(&raws[i]),
        None => {
            let mut count: HashMap<&str, usize> = HashMap::new();
            for r in &raws {
                if !index_of.contains_key(&r.parent) {
                    *count.entry(r.parent.as_str()).or_default() += 1;
                }
            }
            count.into_iter().max_by_key(|(k, n)| (*n, k.is_empty())).map(|(k, _)| k.to_string()).unwrap_or_default()
        }
    };

    let mut table = FileTable::new('\\');
    let root_path = root.trim_end_matches('\\');
    let root_id = table.push(None, root_path, true, 0);
    let mut files: HashMap<u64, u32> = HashMap::new();
    let mut folders: HashMap<u64, u32> = HashMap::new();
    let mut visited: HashSet<usize> = HashSet::new();
    let mut queue: VecDeque<(String, u32, String)> = VecDeque::new();
    queue.push_back((root_ref.clone(), root_id, root_path.to_string()));
    while let Some((pref, tid, path)) = queue.pop_front() {
        let Some(kids) = children.get(pref.as_str()) else { continue };
        for &i in kids {
            if !visited.insert(i) {
                continue;
            }
            let r = &raws[i];
            if r.name.is_empty() || ref_of(r) == root_ref {
                continue;
            }
            let child_path = format!("{path}\\{}", r.name);
            if excludes.contains(&r.name) || skip_paths.iter().any(|s| same_path(s, Path::new(&child_path))) {
                stats.skipped += 1;
                continue;
            }
            let dir = is_folder(i);
            let id = table.push(Some(tid), &r.name, dir, 0);
            stats.placed += 1;
            if let Ok(b) = r.box_id.parse::<u64>() {
                if dir { folders.insert(b, id) } else { files.insert(b, id) };
            }
            if dir {
                queue.push_back((ref_of(r), id, child_path));
            }
        }
    }
    stats.orphans = raws.len().saturating_sub(stats.placed + stats.skipped + usize::from(raws.iter().any(|r| r.box_id == "0")));
    table.shrink();
    Ok(BoxIndex { table, files, folders, columns: cols, stats })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex() -> Excludes {
        Excludes::new(&["node_modules".to_string()])
    }

    /// 親がBoxのIDを指す形(種類は文字)
    #[test]
    fn parent_is_box_id_with_type_text() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE box_item (id INTEGER PRIMARY KEY, box_id TEXT, parent_box_id TEXT, name TEXT, item_type TEXT, is_deleted INTEGER);
             INSERT INTO box_item VALUES (1, '0', NULL, 'All Files', 'folder', 0);
             INSERT INTO box_item VALUES (2, '100', '0', 'Projects', 'folder', 0);
             INSERT INTO box_item VALUES (3, '200', '100', 'plan.xlsx', 'file', 0);
             INSERT INTO box_item VALUES (4, '300', '100', 'old.pptx', 'file', 1);
             INSERT INTO box_item VALUES (5, '400', '0', 'Empty', 'folder', 0);
             INSERT INTO box_item VALUES (6, '500', '999', 'lost.txt', 'file', 0);
             INSERT INTO box_item VALUES (7, '600', '100', 'node_modules', 'folder', 0);",
        )
        .unwrap();
        let b = read_index(&c, r"C:\Users\me\Box\", &ex(), &[]).unwrap();
        assert_eq!(b.columns.box_id, "box_id");
        assert_eq!(b.columns.parent, "parent_box_id");
        assert_eq!(b.columns.kind.as_deref(), Some("item_type"));
        assert_eq!(b.columns.deleted, vec!["is_deleted"]);
        assert_eq!(b.stats.parent_refers, "box_id");
        assert_eq!(b.path_of("200", false), Some((r"C:\Users\me\Box\Projects\plan.xlsx".to_string(), false)));
        assert_eq!(b.path_of("100", true), Some((r"C:\Users\me\Box\Projects".to_string(), true)));
        // 子のないフォルダーも, 種類の列でフォルダーと分かる
        assert_eq!(b.path_of("400", true), Some((r"C:\Users\me\Box\Empty".to_string(), true)));
        assert_eq!(b.path_of("300", false), None);
        assert_eq!(b.path_of("500", false), None);
        assert_eq!(b.path_of("600", true), None);
        assert_eq!(b.stats.deleted, 1);
        assert_eq!(b.stats.placed, 3);
        assert_eq!(b.stats.skipped, 1);
        assert_eq!(b.stats.orphans, 1);
    }

    /// 親が表のIDを指す形(種類の列は数. 根の行はない)
    #[test]
    fn parent_is_row_id_with_numeric_type() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE box_item (item_id INTEGER PRIMARY KEY, box_id INTEGER, parent_item_id INTEGER, name TEXT, type INTEGER);
             INSERT INTO box_item VALUES (10, 1000, 0, 'Team', 1);
             INSERT INTO box_item VALUES (11, 1001, 10, 'Design', 1);
             INSERT INTO box_item VALUES (12, 1002, 11, '回路図.pdf', 2);
             INSERT INTO box_item VALUES (13, 1003, 10, 'memo.docx', 2);",
        )
        .unwrap();
        let b = read_index(&c, r"D:\Box", &ex(), &[]).unwrap();
        assert_eq!(b.columns.key, "item_id");
        assert_eq!(b.stats.parent_refers, "item_id");
        assert_eq!(b.path_of("1002", false), Some((r"D:\Box\Team\Design\回路図.pdf".to_string(), false)));
        assert_eq!(b.path_of("1001", true).map(|x| x.1), Some(true));
        assert_eq!(b.path_of("1003", false).map(|x| x.1), Some(false));
        assert_eq!(b.table.len(), 5);
    }

    /// 種類の列がない形. 子を持つものをフォルダーとする. 親の輪があっても止まる. 除く場所も効く
    #[test]
    fn no_kind_column_and_loops() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE box_item (box_id TEXT, parent TEXT, name TEXT, trashed_at TEXT);
             INSERT INTO box_item VALUES ('1', '', 'A', NULL);
             INSERT INTO box_item VALUES ('2', '1', 'b.txt', NULL);
             INSERT INTO box_item VALUES ('3', '4', 'X', NULL);
             INSERT INTO box_item VALUES ('4', '3', 'Y', NULL);
             INSERT INTO box_item VALUES ('5', '', 'Skip', NULL);
             INSERT INTO box_item VALUES ('6', '5', 'c.txt', NULL);
             INSERT INTO box_item VALUES ('7', '', 'gone.txt', '2026-01-01');",
        )
        .unwrap();
        let skip = vec![std::path::PathBuf::from(r"C:\Box\skip")];
        let b = read_index(&c, r"C:\Box", &ex(), &skip).unwrap();
        assert_eq!(b.columns.key, "rowid");
        assert_eq!(b.columns.kind, None);
        assert_eq!(b.columns.deleted, vec!["trashed_at"]);
        assert_eq!(b.path_of("2", false), Some((r"C:\Box\A\b.txt".to_string(), false)));
        assert_eq!(b.path_of("1", true).map(|x| x.1), Some(true));
        assert_eq!(b.path_of("3", true), None);
        assert_eq!(b.path_of("6", false), None);
        assert_eq!(b.stats.deleted, 1);
    }

    /// 実際のsync.dbの形(2026-11-24に会社PCで調べた): 主キーは(box_id, item_type), item_typeは0がファイル, 1がフォルダー,
    /// parent_item_idは親フォルダーのbox_id, 根はbox_id='0'. ファイルとフォルダーで同じbox_idがあり得る
    #[test]
    fn real_schema_with_same_id_for_file_and_folder() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE box_item (box_id TEXT, item_type INTEGER, parent_item_id TEXT, name TEXT COLLATE NOCASE, sort_name TEXT,
               checksum TEXT, size INTEGER, version_id TEXT, content_created_at INTEGER, content_updated_at INTEGER,
               lock_id TEXT, lock_owner_id TEXT, lock_app_type TEXT, owner_id TEXT, PRIMARY KEY (box_id, item_type));
             INSERT INTO box_item (box_id, item_type, parent_item_id, name) VALUES ('0', 1, '', 'All Files');
             INSERT INTO box_item (box_id, item_type, parent_item_id, name) VALUES ('500', 1, '0', 'Team');
             INSERT INTO box_item (box_id, item_type, parent_item_id, name) VALUES ('500', 0, '0', 'readme.txt');
             INSERT INTO box_item (box_id, item_type, parent_item_id, name) VALUES ('700', 0, '500', '設計.xlsx');
             INSERT INTO box_item (box_id, item_type, parent_item_id, name) VALUES ('800', 1, '500', 'Empty');",
        )
        .unwrap();
        let b = read_index(&c, r"C:\Users\me\Box", &ex(), &[]).unwrap();
        assert_eq!(b.columns.box_id, "box_id");
        assert_eq!(b.columns.parent, "parent_item_id");
        assert_eq!(b.columns.kind.as_deref(), Some("item_type"));
        assert_eq!(b.stats.parent_refers, "box_id");
        assert_eq!(b.path_of("500", true), Some((r"C:\Users\me\Box\Team".to_string(), true)));
        assert_eq!(b.path_of("500", false), Some((r"C:\Users\me\Box\readme.txt".to_string(), false)));
        assert_eq!(b.path_of("700", false), Some((r"C:\Users\me\Box\Team\設計.xlsx".to_string(), false)));
        assert_eq!(b.path_of("700", true), None);
        assert_eq!(b.path_of("800", true).map(|x| x.1), Some(true));
        assert_eq!(b.stats.placed, 4);
        assert_eq!(b.stats.orphans, 0);
    }

    #[test]
    fn missing_table_is_error() {
        let c = Connection::open_in_memory().unwrap();
        assert!(read_index(&c, r"C:\Box", &ex(), &[]).is_err());
        c.execute_batch("CREATE TABLE box_item (foo TEXT)").unwrap();
        let e = read_index(&c, r"C:\Box", &ex(), &[]).err().unwrap();
        assert!(e.contains("name"), "{e}");
    }
}
