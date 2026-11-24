//! 裏で索引を作る: ユーザーフォルダー(変更を監視してすぐ反映), Box(Box Driveの`sync.db`から. K18), WSL(定期更新)(D1〜D6),
//! アプリの一覧(スタートメニューの変更で作り直す. J1), 設定ファイルの変更の監視(K4).
//! できたら窓にメッセージを送る. 窓の側は`Shared`から最新のものを取る.

use super::apps;
use super::storage::{self, expand};
use hikidashi_core::catalog::Item;
use hikidashi_core::boxdrive;
use hikidashi_core::config::Config;
use hikidashi_core::files::{scan_into, table_from_wsl_list, Excludes, FileTable};
use std::os::windows::process::CommandExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForMultipleObjects, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 索引の場所(固定の番号)
pub const SLOT_USER: usize = 0;
pub const SLOT_BOX: usize = 1;
pub const SLOT_WSL: usize = 2;
/// 状態の表示の場所. 0〜2は索引と同じ番号, 3は辞書を取ってくる知らせ
pub const STATUS_DICT: usize = 3;

/// 窓と裏のスレッドで共有するもの
pub struct Shared {
    pub tables: Mutex<[Option<Arc<FileTable>>; 3]>,
    pub apps: Mutex<Option<Vec<Item>>>,
    /// 状態の表示(例: `Indexing files… 12,345`)
    pub status: Mutex<[String; 4]>,
    /// 手動で作り直す(トレイの`Rebuild index`)
    pub rebuild: AtomicU64,
    /// ユーザーフォルダーの索引を作り終えた回数(手動の作り直しが終わったことを窓に知らせる)
    pub user_scans: AtomicU64,
    pub config: Mutex<Config>,
    pub quit: AtomicBool,
    /// BoxのIDから手元のパスを引く表(K18). Boxの一覧と同じものを指す
    pub box_lookup: Mutex<Option<Arc<BoxLookup>>>,
    /// WSLの一覧を読んでほしいとき(`w `を打った, WSLのパスを貼った)に増やす(K24)
    pub wsl_request: AtomicU64,
}

/// BoxのID → Boxの一覧の番号(K18). URLを貼ったときに手元のファイルを開くのに使う
pub struct BoxLookup {
    pub table: Arc<FileTable>,
    pub files: HashMap<u64, u32>,
    pub folders: HashMap<u64, u32>,
    /// 読むのにかかった時間(写すのも含む)
    pub took: Duration,
}

impl BoxLookup {
    /// BoxのIDの手元のパスと, フォルダーか(`folder`はURLが`/folder/`のとき)
    pub fn path_of(&self, box_id: &str, folder: bool) -> Option<(String, bool)> {
        let id: u64 = box_id.trim().parse().ok()?;
        let n = *if folder { self.folders.get(&id) } else { self.files.get(&id) }?;
        Some((self.table.path(n), self.table.is_dir(n)))
    }
}

impl Shared {
    pub fn new(config: Config) -> Arc<Shared> {
        Arc::new(Shared {
            tables: Mutex::new([None, None, None]),
            apps: Mutex::new(None),
            status: Mutex::new(Default::default()),
            rebuild: AtomicU64::new(0),
            user_scans: AtomicU64::new(0),
            config: Mutex::new(config),
            quit: AtomicBool::new(false),
            box_lookup: Mutex::new(None),
            wsl_request: AtomicU64::new(0),
        })
    }

    /// WSLの一覧を読んでもらう. 読んでから5分たっていなければ何もしない(K24)
    pub fn request_wsl(&self) {
        self.wsl_request.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Vec<Arc<FileTable>> {
        self.tables.lock().unwrap().iter().flatten().cloned().collect()
    }

    /// 検索に使う索引(1つ). Boxは`bx `, WSLは`w `のときだけ(K20, K24). ふだんはユーザーフォルダー
    pub fn snapshot_slot(&self, slot: usize) -> Vec<Arc<FileTable>> {
        self.tables.lock().unwrap()[slot].iter().cloned().collect()
    }

    pub fn set_status(&self, slot: usize, s: String) {
        self.status.lock().unwrap()[slot] = s;
    }
}

pub const WM_APP_INDEX: u32 = 0x8000 + 10;
pub const WM_APP_APPS: u32 = 0x8000 + 11;
pub const WM_APP_CONFIG: u32 = 0x8000 + 12;

#[derive(Clone, Copy)]
struct Hwnd(isize);
unsafe impl Send for Hwnd {}

/// 窓(`HWND`の値)に知らせる. ほかのスレッドから使う
pub fn post_to(h: isize, msg: u32) {
    post(Hwnd(h), msg);
}

fn post(h: Hwnd, msg: u32) {
    unsafe {
        let _ = PostMessageW(Some(HWND(h.0 as *mut _)), msg, WPARAM(0), LPARAM(0));
    }
}

/// Box Driveのデータのフォルダー(`sync.db`と`sync_root_folder.txt`がある)
fn box_data_dir() -> PathBuf {
    PathBuf::from(storage::env("LOCALAPPDATA")).join(r"Box\Box\data")
}

/// そのフォルダーの下を浅く(深さ`depth`まで)探す. 名前は大文字小文字を区別しない
fn find_shallow(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_file() && e.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
            return Some(e.path());
        }
        if ft.is_dir() {
            subdirs.push(e.path());
        }
    }
    subdirs.into_iter().find_map(|d| find_shallow(&d, name, depth - 1))
}

/// Box Driveの`sync.db`の場所(K18). 設定の`box_db`か, `%LOCALAPPDATA%\Box\Box\data`の下
pub fn box_db_path(cfg: &Config) -> Option<PathBuf> {
    if !cfg.files.box_db.trim().is_empty() {
        let p = PathBuf::from(expand(cfg.files.box_db.trim()));
        return p.is_file().then_some(p);
    }
    find_shallow(&box_data_dir(), "sync.db", 2)
}

/// `sync_root_folder.txt`に書かれたBox Driveの根. UTF-16で書かれていても読む
fn sync_root_from_data() -> Option<PathBuf> {
    let f = find_shallow(&box_data_dir(), "sync_root_folder.txt", 2)?;
    let bytes = std::fs::read(f).ok()?;
    let text = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let u: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let line = text.trim_start_matches('\u{feff}').lines().map(|l| l.trim().trim_matches('"')).find(|l| !l.is_empty())?.to_string();
    Some(PathBuf::from(expand(&line)))
}

/// Boxのフォルダー(手元の根). 設定の`box_dir`, なければ`sync_root_folder.txt`, なければ`%USERPROFILE%\Box`
pub fn box_dir(cfg: &Config) -> Option<PathBuf> {
    if !cfg.files.box_dir.trim().is_empty() {
        let p = PathBuf::from(expand(cfg.files.box_dir.trim()));
        return p.is_dir().then_some(p);
    }
    sync_root_from_data()
        .filter(|p| p.is_dir())
        .or_else(|| Some(PathBuf::from(storage::env("USERPROFILE")).join("Box")).filter(|p| p.is_dir()))
}

/// 場所で除くフォルダー(K16)
fn skip_dirs(cfg: &Config) -> Vec<PathBuf> {
    cfg.files.skip_dirs.iter().filter(|d| !d.trim().is_empty()).map(|d| PathBuf::from(expand(d))).collect()
}

fn scan_user(shared: &Shared, cfg: &Config) -> FileTable {
    let mut t = FileTable::new('\\');
    let ex = Excludes::new(&cfg.files.exclude);
    let mut skip: Vec<PathBuf> = box_dir(cfg).into_iter().collect();
    skip.extend(skip_dirs(cfg));
    let never = AtomicBool::new(false);
    for r in &cfg.files.roots {
        let root = PathBuf::from(expand(r));
        if root.is_dir() {
            scan_into(&mut t, &root, &ex, &skip, &never);
        }
    }
    t.shrink();
    let _ = shared;
    t
}

/// 変更の通知を待つ. `paths`のどれかが変わるか`timeout`が過ぎたら返る. 変わったらtrue
fn wait_change(handles: &[HANDLE], timeout: Duration) -> bool {
    if handles.is_empty() {
        std::thread::sleep(timeout);
        return false;
    }
    let r = unsafe { WaitForMultipleObjects(handles, false, timeout.as_millis() as u32) };
    let i = r.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
    if i < handles.len() {
        unsafe {
            let _ = FindNextChangeNotification(handles[i]);
        }
        true
    } else {
        false
    }
}

fn watch(paths: &[PathBuf], subtree: bool, filter: FILE_NOTIFY_CHANGE) -> Vec<HANDLE> {
    paths
        .iter()
        .filter_map(|p| {
            let w = super::util::WStr::new(&p.to_string_lossy());
            unsafe { FindFirstChangeNotificationW(w.pcwstr(), subtree, filter).ok() }
        })
        .collect()
}

fn close(handles: Vec<HANDLE>) {
    for h in handles {
        unsafe {
            let _ = FindCloseChangeNotification(h);
        }
    }
}


/// フォルダーの変更を, 変わったパス付きで受け取る(ReadDirectoryChangesW). 除いた場所(AppDataなど)の変更を無視するため.
struct DirWatcher {
    dir: HANDLE,
    event: HANDLE,
    ov: Box<OVERLAPPED>,
    buf: Vec<u32>,
}

unsafe impl Send for DirWatcher {}

enum Change {
    /// 変わった相対パス
    Paths(Vec<String>),
    /// 多すぎて取りこぼした(全部変わったとみなす)
    Overflow,
    None,
}

impl DirWatcher {
    fn new(path: &std::path::Path) -> Option<DirWatcher> {
        unsafe {
            let w = super::util::WStr::new(&path.to_string_lossy());
            let dir = CreateFileW(
                w.pcwstr(),
                FILE_LIST_DIRECTORY.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                None,
            )
            .ok()?;
            let event = CreateEventW(None, true, false, PCWSTR::null()).ok()?;
            let mut w = DirWatcher { dir, event, ov: Box::new(OVERLAPPED::default()), buf: vec![0u32; 16 * 1024] };
            if !w.arm() {
                return None;
            }
            Some(w)
        }
    }

    fn arm(&mut self) -> bool {
        unsafe {
            let _ = ResetEvent(self.event);
            *self.ov = OVERLAPPED::default();
            self.ov.hEvent = self.event;
            ReadDirectoryChangesW(
                self.dir,
                self.buf.as_mut_ptr() as *mut _,
                (self.buf.len() * 4) as u32,
                true,
                FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME,
                None,
                Some(&mut *self.ov),
                None,
            )
            .is_ok()
        }
    }

    /// 変更を待つ(最大`timeout`)
    fn wait(&mut self, timeout: Duration) -> Change {
        unsafe {
            let r = WaitForSingleObject(self.event, timeout.as_millis() as u32);
            if r != WAIT_OBJECT_0 {
                return Change::None;
            }
            let mut n = 0u32;
            if GetOverlappedResult(self.dir, &*self.ov, &mut n, false).is_err() || n == 0 {
                self.arm();
                return Change::Overflow;
            }
            let bytes = std::slice::from_raw_parts(self.buf.as_ptr() as *const u8, n as usize);
            let mut out = Vec::new();
            let mut off = 0usize;
            loop {
                if off + 12 > bytes.len() {
                    break;
                }
                let next = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
                let len = u32::from_le_bytes(bytes[off + 8..off + 12].try_into().unwrap()) as usize;
                let name_start = off + 12;
                if name_start + len > bytes.len() {
                    break;
                }
                let units: Vec<u16> = bytes[name_start..name_start + len].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                out.push(String::from_utf16_lossy(&units));
                if next == 0 {
                    break;
                }
                off += next;
            }
            self.arm();
            Change::Paths(out)
        }
    }
}

impl Drop for DirWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = CancelIoEx(self.dir, Some(&*self.ov));
            let mut n = 0u32;
            let _ = GetOverlappedResult(self.dir, &*self.ov, &mut n, true);
            let _ = CloseHandle(self.dir);
            let _ = CloseHandle(self.event);
        }
    }
}

/// 変わったパスが, 索引に関係する場所か(除いた名前, 除いたフォルダー, Boxの中なら関係ない)
fn relevant(rel: &str, ex: &Excludes, skip_rel: &[String]) -> bool {
    let l = rel.to_lowercase();
    if skip_rel.iter().any(|b| l == *b || l.starts_with(&format!("{b}\\"))) {
        return false;
    }
    !rel.split('\\').any(|part| ex.contains(part))
}

/// ユーザーフォルダー: 作ったあと変更を監視し, 落ち着いたら(0.5秒)作り直す.
/// 作り直しの間隔は, 前回かかった時間の3倍(1秒〜60秒). 件数が少なければすぐ, 多ければ間を空ける.
fn rescan_gap(took: Duration) -> Duration {
    (took * 3).clamp(Duration::from_secs(1), Duration::from_secs(60))
}

fn user_loop(shared: Arc<Shared>, hwnd: Hwnd) {
    let mut last_rebuild = shared.rebuild.load(Ordering::Relaxed);
    loop {
        let cfg = shared.config.lock().unwrap().clone();
        let t0 = Instant::now();
        shared.set_status(SLOT_USER, "Indexing your folder…".into());
        post(hwnd, WM_APP_INDEX);
        let t = scan_user(&shared, &cfg);
        storage::log(&format!("user index: {} entries, {:?}, {:.1} MB", t.len(), t0.elapsed(), t.memory() as f64 / 1e6));
        shared.tables.lock().unwrap()[SLOT_USER] = Some(Arc::new(t));
        shared.set_status(SLOT_USER, String::new());
        shared.user_scans.fetch_add(1, Ordering::Relaxed);
        post(hwnd, WM_APP_INDEX);
        let gap = rescan_gap(t0.elapsed());
        let ex = Excludes::new(&cfg.files.exclude);
        let mut skips: Vec<PathBuf> = box_dir(&cfg).into_iter().collect();
        skips.extend(skip_dirs(&cfg));
        let mut watchers: Vec<(DirWatcher, Vec<String>)> = cfg
            .files
            .roots
            .iter()
            .map(|r| PathBuf::from(expand(r)))
            .filter(|p| p.is_dir())
            .filter_map(|root| {
                // Boxや除いたフォルダーがこの根の中にあれば, その相対パス(小文字)
                let root_l = root.to_string_lossy().trim_end_matches('\\').to_lowercase();
                let rel: Vec<String> = skips
                    .iter()
                    .filter_map(|b| {
                        let l = b.to_string_lossy().trim_end_matches('\\').to_lowercase();
                        l.strip_prefix(&root_l).and_then(|r| r.strip_prefix('\\')).map(str::to_string)
                    })
                    .collect();
                DirWatcher::new(&root).map(|w| (w, rel))
            })
            .collect();
        let scanned_at = Instant::now();
        let mut dirty_since: Option<Instant> = None;
        let mut last_change = Instant::now();
        // 関係する変更があってから0.5秒静かになったら作り直す. 作り直しは前回からgap以上空ける
        loop {
            if shared.quit.load(Ordering::Relaxed) {
                return;
            }
            let r = shared.rebuild.load(Ordering::Relaxed);
            if r != last_rebuild {
                last_rebuild = r;
                break;
            }
            let mut changed = false;
            if watchers.is_empty() {
                std::thread::sleep(Duration::from_millis(500));
            }
            let per = Duration::from_millis(500 / watchers.len().max(1) as u64);
            for (w, skip_rel) in watchers.iter_mut() {
                match w.wait(per) {
                    Change::Paths(paths) => {
                        if paths.iter().any(|p| relevant(p, &ex, skip_rel)) {
                            changed = true;
                        }
                    }
                    Change::Overflow => changed = true,
                    Change::None => {}
                }
            }
            if changed {
                last_change = Instant::now();
                dirty_since.get_or_insert(last_change);
            }
            if dirty_since.is_some() && last_change.elapsed() >= Duration::from_millis(500) && scanned_at.elapsed() >= gap {
                break;
            }
        }
    }
}

/// `sync.db`と`-wal`の更新日時(変わったときだけ読み直す)
fn box_db_stamp(db: &Path) -> (Option<SystemTime>, Option<SystemTime>) {
    let m = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    (m(db), m(&PathBuf::from(format!("{}-wal", db.display()))))
}

/// `sync.db`を一時フォルダーに写して読み, Boxの一覧を作る(K18). Box Driveの動きは邪魔しない
fn read_box(cfg: &Config, db: &Path) -> Result<(boxdrive::BoxIndex, PathBuf), String> {
    let root = box_dir(cfg).ok_or("Box folder not found (set Box folder in Settings)")?;
    let tmp = std::env::temp_dir().join("Hikidashi").join("box");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("temp: {e}"))?;
    let copy = tmp.join("sync.db");
    let wal_src = PathBuf::from(format!("{}-wal", db.display()));
    let wal_dst = tmp.join("sync.db-wal");
    let _ = std::fs::remove_file(tmp.join("sync.db-shm"));
    let _ = std::fs::remove_file(&wal_dst);
    std::fs::copy(db, &copy).map_err(|e| format!("copy sync.db: {e}"))?;
    // 書きかけの部分(-wal)も写す. 写したものなので書き込みで開き, SQLiteに取り込ませる
    if wal_src.is_file() {
        let _ = std::fs::copy(&wal_src, &wal_dst);
    }
    let conn = rusqlite_open(&copy)?;
    let ex = Excludes::new(&cfg.files.exclude);
    let idx = boxdrive::read_index(&conn, &root.to_string_lossy(), &ex, &skip_dirs(cfg))?;
    drop(conn);
    let _ = std::fs::remove_file(&copy);
    let _ = std::fs::remove_file(&wal_dst);
    let _ = std::fs::remove_file(tmp.join("sync.db-shm"));
    Ok((idx, root))
}

fn rusqlite_open(p: &Path) -> Result<hikidashi_core::boxdrive::Connection, String> {
    hikidashi_core::boxdrive::Connection::open(p).map_err(|e| format!("open sync.db: {e}"))
}

/// Box: Box Driveの`sync.db`から一覧を作る(K18). フォルダーを巡回すると数時間かかるので巡回はしない.
/// 1分ごとに`sync.db`の更新日時を見て, 変わっていれば読み直す. 読み直しは前回から5分空ける(K24で間隔の設定はなくした)
fn box_loop(shared: Arc<Shared>, hwnd: Hwnd) {
    let mut last_rebuild = shared.rebuild.load(Ordering::Relaxed);
    let mut last_stamp: Option<(PathBuf, (Option<SystemTime>, Option<SystemTime>))> = None;
    let mut last_read: Option<Instant> = None;
    let mut last_error = String::new();
    let mut forced = true;
    loop {
        let cfg = shared.config.lock().unwrap().clone();
        let gap = Duration::from_secs(5 * 60);
        match box_db_path(&cfg) {
            None => {
                if shared.tables.lock().unwrap()[SLOT_BOX].is_some() || last_error != "no sync.db" {
                    storage::log("box drive: sync.db not found (Box is not searched)");
                }
                last_error = "no sync.db".into();
                shared.tables.lock().unwrap()[SLOT_BOX] = None;
                *shared.box_lookup.lock().unwrap() = None;
                last_stamp = None;
                post(hwnd, WM_APP_INDEX);
            }
            Some(db) => {
                let stamp = (db.clone(), box_db_stamp(&db));
                let changed = last_stamp.as_ref() != Some(&stamp);
                let due = last_read.is_none_or(|t| t.elapsed() >= gap);
                if forced || (changed && due) {
                    let t0 = Instant::now();
                    shared.set_status(SLOT_BOX, "Reading Box Drive…".into());
                    post(hwnd, WM_APP_INDEX);
                    match read_box(&cfg, &db) {
                        Ok((idx, root)) => {
                            let st = &idx.stats;
                            storage::log(&format!(
                                "box drive: {} items, {:?}; rows {} deleted {} skipped {} orphans {}; parent -> {}; root {}; {}",
                                st.placed,
                                t0.elapsed(),
                                st.rows,
                                st.deleted,
                                st.skipped,
                                st.orphans,
                                st.parent_refers,
                                root.display(),
                                idx.columns.describe()
                            ));
                            let table = Arc::new(idx.table);
                            *shared.box_lookup.lock().unwrap() = Some(Arc::new(BoxLookup { table: table.clone(), files: idx.files, folders: idx.folders, took: t0.elapsed() }));
                            shared.tables.lock().unwrap()[SLOT_BOX] = Some(table);
                            last_error.clear();
                        }
                        Err(e) => {
                            if e != last_error {
                                storage::log(&format!("box drive: could not read {}: {e}", db.display()));
                            }
                            last_error = e;
                            shared.tables.lock().unwrap()[SLOT_BOX] = None;
                            *shared.box_lookup.lock().unwrap() = None;
                        }
                    }
                    last_stamp = Some(stamp);
                    last_read = Some(Instant::now());
                    shared.set_status(SLOT_BOX, String::new());
                    post(hwnd, WM_APP_INDEX);
                }
            }
        }
        // 1分待つ. 手動の作り直し(トレイの`Rebuild index`や設定の保存)なら, すぐ読み直す
        forced = false;
        let until = Instant::now() + Duration::from_secs(60);
        while Instant::now() < until {
            if shared.quit.load(Ordering::Relaxed) {
                return;
            }
            let r = shared.rebuild.load(Ordering::Relaxed);
            if r != last_rebuild {
                last_rebuild = r;
                forced = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

/// WSL: WSLの中で`find`して一覧をもらう(D4). `\\wsl.localhost`を走査するより速い
fn wsl_scan(cfg: &Config) -> Option<FileTable> {
    let mut prune = String::new();
    for (i, e) in cfg.wsl.exclude.iter().enumerate() {
        if i > 0 {
            prune.push_str(" -o");
        }
        prune.push_str(&format!(" -name '{}'", e.replace('\'', "")));
    }
    let script = if prune.is_empty() {
        "echo \"$WSL_DISTRO_NAME\"; cd ~ && pwd && find . -mindepth 1 \\( -type d -printf 'd %P\\n' -o -printf 'f %P\\n' \\) 2>/dev/null".to_string()
    } else {
        format!("echo \"$WSL_DISTRO_NAME\"; cd ~ && pwd && find . -mindepth 1 \\({prune} \\) -prune -o \\( -type d -printf 'd %P\\n' -o -printf 'f %P\\n' \\) 2>/dev/null")
    };
    let mut c = std::process::Command::new("wsl.exe");
    if !cfg.wsl.distro.is_empty() {
        c.args(["-d", &cfg.wsl.distro]);
    }
    c.args(["-e", "sh", "-c", &script]);
    c.creation_flags(0x0800_0000);
    let out = c.output().ok()?;
    if !out.status.success() && out.stdout.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.splitn(3, '\n');
    let distro = lines.next()?.trim().to_string();
    let home = lines.next()?.trim().to_string();
    let rest = lines.next().unwrap_or("");
    if distro.is_empty() || !home.starts_with('/') {
        return None;
    }
    let root = format!("\\\\wsl.localhost\\{}{}", distro, home.replace('/', "\\"));
    Some(table_from_wsl_list(&root, rest))
}

/// WSLの一覧(K24): 頼まれたとき(`w `を打った, WSLのパスを貼った)だけ読む. 前に読んでから5分たっていなければ読まない.
/// WSLを使わない日はWSLを起こさない. 手動の作り直しでは, 一度読んだことがあるときだけ読み直す
fn wsl_loop(shared: Arc<Shared>, hwnd: Hwnd) {
    const FRESH: Duration = Duration::from_secs(5 * 60);
    let mut last_rebuild = shared.rebuild.load(Ordering::Relaxed);
    let mut last_request = 0u64;
    let mut loaded: Option<Instant> = None;
    loop {
        if shared.quit.load(Ordering::Relaxed) {
            return;
        }
        let req = shared.wsl_request.load(Ordering::Relaxed);
        let reb = shared.rebuild.load(Ordering::Relaxed);
        let asked = req != last_request;
        let rebuilt = reb != last_rebuild && loaded.is_some();
        last_request = req;
        last_rebuild = reb;
        let stale = loaded.is_none_or(|t| t.elapsed() >= FRESH);
        if !((asked && stale) || rebuilt) {
            std::thread::sleep(Duration::from_millis(200));
            continue;
        }
        let cfg = shared.config.lock().unwrap().clone();
        let has_wsl = super::system::wsl_installed();
        if cfg.wsl.enabled && has_wsl {
            let t0 = Instant::now();
            let had = shared.tables.lock().unwrap()[SLOT_WSL].is_some();
            shared.set_status(SLOT_WSL, if had { "Updating the WSL list…".into() } else { "Reading WSL…".into() });
            post(hwnd, WM_APP_INDEX);
            let t = wsl_scan(&cfg);
            storage::log(&format!("wsl index: {:?} entries, {:?}", t.as_ref().map(|t| t.len()), t0.elapsed()));
            if let Some(t) = t {
                shared.tables.lock().unwrap()[SLOT_WSL] = Some(Arc::new(t));
            }
            loaded = Some(Instant::now());
        } else {
            shared.tables.lock().unwrap()[SLOT_WSL] = None;
            loaded = None;
        }
        shared.set_status(SLOT_WSL, String::new());
        post(hwnd, WM_APP_INDEX);
    }
}

/// アプリの一覧: 作ったあとスタートメニューの変更を監視し, 300msまとめてから作り直す(J1)
fn apps_loop(shared: Arc<Shared>, hwnd: Hwnd) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let mut cache = apps::LnkCache::default();
    let mut last_rebuild = shared.rebuild.load(Ordering::Relaxed);
    loop {
        let cfg = shared.config.lock().unwrap().clone();
        let t0 = Instant::now();
        let list = apps::build(&mut cache, &cfg.apps.extra_dirs);
        storage::log(&format!("apps: {} items, {:?}", list.len(), t0.elapsed()));
        // どのアプリがどの名前で探せるか(exeの名前が取れたか)を残す. 原因を追うため
        let lines: Vec<String> = list.iter().map(|it| format!("  {} | {} | {} | {}", it.name, it.keys.join(","), it.target, it.args)).collect();
        storage::log(&format!("apps list:\n{}", lines.join("\n")));
        *shared.apps.lock().unwrap() = Some(list);
        post(hwnd, WM_APP_APPS);
        let mut dirs = apps::start_menu_dirs();
        dirs.extend(apps::desktop_dirs());
        dirs.extend(cfg.apps.extra_dirs.iter().map(|d| PathBuf::from(expand(d))));
        let handles = watch(&dirs, true, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE);
        loop {
            if shared.quit.load(Ordering::Relaxed) {
                close(handles);
                return;
            }
            let r = shared.rebuild.load(Ordering::Relaxed);
            if r != last_rebuild {
                last_rebuild = r;
                break;
            }
            if wait_change(&handles, Duration::from_secs(2)) {
                while wait_change(&handles, Duration::from_millis(300)) {}
                break;
            }
        }
        close(handles);
    }
}

/// 設定ファイルの変更を監視する(保存したらすぐ反映. K4)
fn config_loop(shared: Arc<Shared>, hwnd: Hwnd) {
    let dir = storage::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let handles = watch(&[dir], false, FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME);
    let path = storage::config_path();
    let mtime = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut last = mtime(&path);
    while !shared.quit.load(Ordering::Relaxed) {
        if wait_change(&handles, Duration::from_secs(2)) {
            std::thread::sleep(Duration::from_millis(200));
            let now = mtime(&path);
            if now != last {
                last = now;
                post(hwnd, WM_APP_CONFIG);
            }
        }
    }
    close(handles);
}

pub fn start(shared: &Arc<Shared>, hwnd: HWND) {
    let h = Hwnd(hwnd.0 as isize);
    let spawn = |name: &str, f: fn(Arc<Shared>, Hwnd)| {
        let s = shared.clone();
        let _ = std::thread::Builder::new().name(name.into()).spawn(move || f(s, h));
    };
    spawn("apps", apps_loop);
    spawn("user-index", user_loop);
    spawn("box-index", box_loop);
    spawn("wsl-index", wsl_loop);
    spawn("config-watch", config_loop);
}
