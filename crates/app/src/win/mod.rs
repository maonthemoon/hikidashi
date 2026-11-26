//! Windowsの画面とOSの部分. 起動, 窓, キー操作, Tabのメニュー, トレイ.

mod actions;
mod apps;
mod clipboard;
mod compose;
mod dark;
mod diag;
mod dict;
mod ext;
mod icons;
mod i18n;
mod indexer;
mod net;
mod paint;
mod parts;
mod scripts;
mod searcher;
mod selection;
mod settings;
mod shellmenu;
mod storage;
mod system;
mod theme;
mod util;
mod winlist;

use hikidashi_core::builtin::{self, SysAction};
use hikidashi_core::clip::{ClipHistory, ClipKind};
use hikidashi_core::scripts::{self as cscripts, RunOutcome, Script};
use hikidashi_core::selection::{self as csel, Picked};
use hikidashi_core::catalog::{Catalog, Item, Kind};
use hikidashi_core::compose::{Browse, Drafts};
use hikidashi_core::markdown::{self, NewLine};
use hikidashi_core::config::{self, Config, KeyAction, KEY_ALT, KEY_CTRL, KEY_SHIFT};
use hikidashi_core::files::FileTable;
use hikidashi_core::hotkey::{Hotkey, State, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
use hikidashi_core::learn::{History, Learn};
use hikidashi_core::migemo::Dict;
use hikidashi_core::pathlike;
use hikidashi_core::router::{self, Action, Ctx, Env, Mode, PathKind, Row, Source, Sources};
use hikidashi_core::search::Prepared;
use indexer::Shared;
use searcher::{Job, Searcher};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Instant, SystemTime};
use util::{from_wide, hiword, loword, WStr};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows::Win32::Graphics::Gdi::{InvalidateRect, MonitorFromPoint, MonitorFromWindow, GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST, HBRUSH};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener, COPYDATASTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Controls::RichEdit::{EN_REQUESTRESIZE, EN_SELCHANGE, REQRESIZE};
use windows::Win32::UI::Controls::{EM_GETRECT, EM_GETSEL, EM_SETLIMITTEXT, EM_SETSEL, NMHDR};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_BACK, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_INSERT, VK_LEFT, VK_RIGHT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RWIN, VK_SHIFT, VK_TAB, VK_UP};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

pub use paint::Fonts;

const CLASS: PCWSTR = w!("HikidashiWindow");
const WM_APP_SHOW: u32 = 0x8000 + 2;
const WM_APP_QUIT: u32 = 0x8000 + 3;
const TIMER_GLOW: usize = 2;
const TIMER_QUICK: usize = 3;
const TIMER_SELECTION: usize = 77;
/// 書く窓を整える頼み(K22. RichEditの通知の中では整えず, あとで行う)
const WM_APP_COMPOSE: u32 = 0x8000 + 5;
/// ブックマークのアイコンを読み終えた
const WM_APP_FAVICONS: u32 = 0x8000 + 7;
/// `--query`で動いているものに文字を渡す`WM_COPYDATA`の目印
const QUERY_MAGIC: usize = 0x4B51_0001;
/// ファイルの結果を待つ時間. これより遅ければ先にほかの結果を出す
const QUICK_MS: u32 = 60;


/// Tabのメニューの1行
#[derive(Clone)]
pub struct MenuItem {
    pub label: String,
    pub glyph: char,
    pub act: MenuAct,
}

#[derive(Clone, PartialEq)]
pub enum MenuAct {
    Open,
    Reveal,
    CopyPath,
    CopyWslPath,
    /// フォルダーをExplorerで開く(Q6)
    Explorer,
    /// 「〜で開く」(Q5)
    OpenWith(hikidashi_core::config::OpenWith),
    Admin,
    NewInstance,
    /// ファイルそのものをクリップボードへ(Explorerでコピーしたのと同じ形)
    CopyFile,
    /// 「プログラムから開く」の一覧にメニューの中身を替える
    OpenWithList,
    /// 一覧の1件(`App::assoc`の番号)で開く
    AssocInvoke(usize),
    /// Windowsの「別のアプリを選ぶ」画面
    ChooseApp,
    /// Windowsの共有のウィンドウ
    Share,
    /// Outlookの新しいメールに添付
    Mail,
    CopyText(String),
    /// クリップボードの履歴の1件をクリップボードに入れるだけ
    ClipCopy(u64),
    ClipDelete(u64),
    Kill,
    ChatGpt,
    Google,
}

pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
    pub sel: usize,
    pub row: Row,
}

#[derive(Default)]
pub struct Stats {
    pub last_search_ms: f64,
    pub last_show_ms: f64,
    hotkey_at: Option<Instant>,
}

pub struct App {
    pub hwnd: HWND,
    pub edit: HWND,
    shared: Arc<Shared>,
    searcher: Searcher,
    dict: Arc<OnceLock<Dict>>,
    pub config: Config,
    pub config_error: Option<String>,
    /// 塗ってある色がダークか
    themed_dark: bool,
    apps: Catalog,
    bookmarks: Catalog,
    /// ChromeとEdgeのBookmarksの更新日時(変わったときだけ読み直す)
    bookmarks_mtime: Vec<Option<SystemTime>>,
    windows: Catalog,
    processes: Catalog,
    processes_fresh: bool,
    settings: Catalog,
    system: Catalog,
    commands: Catalog,
    /// `%APPDATA%`などのフォルダー(2026-11-24)
    folders: Catalog,
    /// ブックマークのアイコン(URL → PNG). 別のスレッドで読む
    pub favicons: Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    favicons_at: Option<Instant>,
    learn: Learn,
    history: History,
    state: State,
    pub mode: Mode,
    pub rows: Vec<Row>,
    pub sel: usize,
    /// 見えている最初の行(9行目からのスクロール. C2)
    pub top: usize,
    /// マウスの最後の位置(画面の座標). 動いたときだけ選びを動かす
    last_mouse: (i32, i32),
    /// 左ボタンを押した行(離したときに同じ行なら開く)
    press_row: Option<usize>,
    tables: Vec<Arc<FileTable>>,
    gen: u64,
    pending: Option<(u64, Vec<Row>)>,
    pub menu: Option<Menu>,
    /// 「プログラムから開く」の一覧(メニューの`AssocInvoke`の番号で引く)
    assoc: Vec<shellmenu::Handler>,
    pub confirm: Option<String>,
    pub capture: bool,
    pub message: Option<(String, bool)>,
    hotkey: Option<Hotkey>,
    pub glow: Option<Instant>,
    animations: bool,
    pub dpi: u32,
    pub fonts: Fonts,
    pub icons: icons::Icons,
    pub stats: Stats,
    from_history: bool,
    /// ↑↓で選びを動かした(ファイルの結果が後から来ても選んだものを保つ)
    user_moved: bool,
    /// トレイの`Rebuild index`を押したときの, ユーザーフォルダーの索引を作り終えた回数. 増えたら知らせる
    rebuild_wait: Option<u64>,
    taskbar_msg: u32,
    exe_path: String,
    /// 長い質問を書く窓(K22). RichEditを作れなければNone(Shift+Enterは前の版と同じくすぐ送る)
    pub compose: Option<compose::Compose>,
    /// 書きかけと送った文
    drafts: Drafts,
    browse: Browse,
    /// 呼び出して書く窓に入れた文(これと同じ間は, 呼び出しを続ける)
    recalled: String,
    /// 書く窓の中身の高さ(EN_REQUESTRESIZE)
    compose_h: i32,
    /// 選んだ文字で開くホットキー(設定の`[hotkeys] selection`)と, 登録できているか
    sel_hotkey: Option<Hotkey>,
    sel_registered: bool,
    /// ホットキーの登録で, 選んだ文字で開くホットキーを変えているところ
    capture_sel: bool,
    /// クリップボードの履歴(メモリだけ)
    clips: ClipHistory,
    clip_listening: bool,
    /// この通し番号のクリップボードの更新は履歴に残さない(選んだ文字の取り込みで一時的に変えたもの)
    clip_ignore: Option<u32>,
    /// 窓を出す前に前面にあった窓. 窓が出ているときに選んだ文字を読むとき, ここへ戻す
    prev_fg: isize,
    /// 窓が出ているときに選んだ文字のホットキーが押された時刻. 修飾キーが離れるのを待っている
    selection_wait: Option<Instant>,
    /// スクリプトのフォルダー
    scripts: Vec<Script>,
    scripts_sig: Vec<(String, Option<SystemTime>)>,
    lists: scripts::Lists,
    /// `run`のスクリプトの仕事の番号と結果. 結果の行を出している間はtrue
    script_job: u64,
    script_view: bool,
    /// 空の入力の「最近のもの」を出している(↓かクリックで選ぶまで, 何も選ばれていない形)
    recent_view: bool,
    script_all: String,
    script_result: Arc<std::sync::Mutex<Option<(u64, RunOutcome)>>>,
    /// JavaScriptの拡張
    ext: ext::Ext,
    /// 整える頼みが来ている(文字が変わった, カーソルが動いた)
    restyle_text: bool,
    restyle_sel: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// 書く窓の中身の高さ. EN_REQUESTRESIZEは`App`を使っている間にも届くので, ここに置いて読む
    static REQ_H: Cell<Option<i32>> = const { Cell::new(None) };
    /// 書く窓で処理したキーの文字(WM_CHAR)を捨てる
    static EAT_CHAR: Cell<bool> = const { Cell::new(false) };
}

/// 窓のプロシージャなどから`App`を使う. すでに使用中(呼び出しの途中で同じ窓にメッセージが来た)ならNone
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| match a.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(f),
        Err(_) => None,
    })
}

struct WinEnv<'a> {
    distro: &'a str,
    /// BoxのIDから手元のパスを引く表(K18)
    box_lookup: Option<Arc<indexer::BoxLookup>>,
    /// WSLのホーム(`/home/me`). `~/`を展開する
    home: Option<String>,
}

impl Env for WinEnv<'_> {
    fn expand(&self, p: &str) -> String {
        storage::expand(p)
    }
    fn path_kind(&self, p: &str) -> PathKind {
        let _d = diag::doing(format!("check path {p}"));
        // 共有フォルダーやWSLのパスは, 届かないと長く待たされるので見に行かず, 拡張子で決める
        if p.starts_with("\\\\") {
            let (name, _) = pathlike::split_name(p);
            return if name.contains('.') { PathKind::File } else { PathKind::Dir };
        }
        match std::fs::metadata(p) {
            Ok(m) if m.is_dir() => PathKind::Dir,
            Ok(_) => PathKind::File,
            Err(_) => PathKind::Missing,
        }
    }
    fn box_path(&self, box_id: &str, folder: bool) -> Option<(String, bool)> {
        self.box_lookup.as_ref()?.path_of(box_id, folder)
    }
    fn wsl_unc(&self, p: &str) -> Option<String> {
        if self.distro.is_empty() {
            return None;
        }
        Some(pathlike::wsl_to_unc(p, self.distro, self.home.as_deref()))
    }
    fn resolve_path(&self, lower: &str) -> Option<(String, bool)> {
        // 共有フォルダーやWSLは, 届かないと待たされるので最近のものには出さない
        if lower.starts_with("\\\\") {
            return None;
        }
        let m = std::fs::metadata(lower).ok()?;
        // 学習の鍵は小文字なので, 実際の綴りに直す(GetLongPathNameWは各部分を実際の名前にする)
        let w = WStr::new(lower);
        let mut buf = vec![0u16; 1024];
        let n = unsafe { windows::Win32::Storage::FileSystem::GetLongPathNameW(w.pcwstr(), Some(&mut buf)) } as usize;
        let full = if n > 0 && n < buf.len() { from_wide(&buf[..n]) } else { lower.to_string() };
        Some((full, m.is_dir()))
    }
    fn list_dir(&self, dir: &str) -> Vec<(String, bool)> {
        let _d = diag::doing(format!("list folder {dir}"));
        let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
        rd.take(5000)
            .filter_map(|e| e.ok())
            .map(|e| {
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false) || e.path().is_dir();
                (e.file_name().to_string_lossy().into_owned(), is_dir)
            })
            .collect()
    }
}

/// `cb `の一覧(履歴を, 打った語で絞って新しい順に). ファイルが今もあるかも見る(共有フォルダーは見に行かない)
fn clip_rows(h: &ClipHistory, query: &str) -> Vec<Row> {
    hikidashi_core::clip::rows(h, query, util::now_secs(), util::tz_offset_secs(), |p| p.starts_with("\\\\") || Path::new(p).exists())
}

fn scale(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

impl App {
    fn prefix_of(&self, m: Mode) -> &str {
        let p = &self.config.prefix;
        match m {
            Mode::App => &p.app,
            Mode::File => &p.file,
            Mode::Bookmark => &p.bookmark,
            Mode::Kill => &p.kill,
            Mode::Wsl => &p.wsl,
            Mode::Box => &p.boxdrive,
            Mode::Google => &p.google,
            Mode::ChatGpt => &p.chatgpt,
            Mode::Clipboard => &p.clipboard,
            Mode::Calc => &p.calc,
            Mode::All => "",
        }
    }

    fn edit_text(&self) -> String {
        unsafe {
            let len = GetWindowTextLengthW(self.edit);
            let mut buf = vec![0u16; len as usize + 1];
            GetWindowTextW(self.edit, &mut buf);
            from_wide(&buf)
        }
    }

    /// 接頭辞の印と入力欄の文字を合わせた, 中心部に渡す入力
    fn input(&self) -> String {
        let t = self.edit_text();
        if self.mode == Mode::All {
            t
        } else {
            format!("{} {}", self.prefix_of(self.mode), t)
        }
    }

    fn wsl_distro(&self) -> String {
        if !self.config.wsl.distro.is_empty() {
            return self.config.wsl.distro.clone();
        }
        // WSLの索引の根(\\wsl.localhost\<distro>\...)から取る
        self.wsl_root()
            .and_then(|root| root.trim_start_matches('\\').split('\\').nth(1).map(str::to_string))
            .unwrap_or_default()
    }

    /// WSLの索引の根(`\\wsl.localhost\<distro>\home\me`)
    fn wsl_root(&self) -> Option<String> {
        let t = self.shared.tables.lock().unwrap();
        t[indexer::SLOT_WSL].as_ref().filter(|t| !t.is_empty()).map(|t| t.name(0).to_string())
    }

    /// 一覧を作る材料(`Ctx`)を作って`f`に渡す
    fn with_ctx<R>(&self, tables: &[Arc<FileTable>], f: impl FnOnce(&Ctx) -> R) -> R {
        let distro = self.wsl_distro();
        let home = self.wsl_root().and_then(|r| pathlike::to_wsl(&r));
        let box_lookup = self.shared.box_lookup.lock().unwrap().clone();
        let env = WinEnv { distro: &distro, home, box_lookup };
        let refs: Vec<&FileTable> = tables.iter().map(|t| t.as_ref()).collect();
        let ctx = Ctx {
            config: &self.config,
            sources: Sources {
                apps: &self.apps,
                bookmarks: &self.bookmarks,
                processes: &self.processes,
                settings: &self.settings,
                system: &self.system,
                commands: &self.commands,
                folders: &self.folders,
            },
            tables: &refs,
            learn: &self.learn,
            now: util::now_secs(),
            env: &env,
        };
        f(&ctx)
    }

    fn build_rows(&self, input: &str, hits: &[(usize, hikidashi_core::files::FileHit)], tables: &[Arc<FileTable>]) -> (Mode, Vec<Row>) {
        let (_, text) = router::parse(input, &self.config.prefixes());
        let prep = Prepared::new(text.trim(), self.dict.get());
        let (mode, mut rows) = self.with_ctx(tables, |ctx| router::build(input, ctx, &prep, hits));
        // 引数を取るスクリプト(`ff 語`)の行は一番上に出す
        if mode == Mode::All {
            if let Some(r) = cscripts::arg_row(&self.scripts, text) {
                rows.insert(0, r);
            }
        }
        (mode, rows)
    }

    /// 入力が変わったら一覧を作り直す. ファイルの検索は別スレッドに頼み, 速いものを先に出す(J1)
    fn refresh(&mut self) {
        let _d = diag::doing(format!("search ({:?}, {} chars)", self.mode, self.edit_text().chars().count()));
        self.gen += 1;
        self.user_moved = false;
        self.menu = None;
        self.confirm = None;
        self.script_view = false;
        self.recent_view = false;
        self.ext.begin_refresh();
        let input = self.input();
        let text = self.edit_text();
        if text.trim() == ":stats" {
            self.rows = self.stats_rows();
            self.sel = 0;
            self.top = 0;
            self.pending = None;
            self.layout();
            return;
        }
        // 履歴, 最近のもの, スクリプトの一覧は, ファイルの検索を使わず, すぐ出す
        if let Some(rows) = self.special_rows(&text) {
            self.searcher.cancel();
            self.pending = None;
            self.set_rows(rows, vec![]);
            return;
        }
        if self.mode == Mode::Kill && !self.processes_fresh {
            let wins = winlist::windows(self.hwnd.0 as isize);
            self.processes = Catalog::new(winlist::process_items(&wins));
            self.processes_fresh = true;
        }
        let (_, quick) = self.build_rows(&input, &[], &[]);
        let qlen = text.trim().chars().count();
        let filtered = hikidashi_core::files::FileQuery::parse(text.trim()).is_structured();
        let want_files = match self.mode {
            Mode::All => (qlen >= 2 || filtered) && pathlike::detect(&input).is_none(),
            Mode::File | Mode::Box | Mode::Wsl => qlen >= 1,
            _ => false,
        };
        // WSLは`w `のときと, WSLのパスを貼ったのにWSLのホームがまだ分からないときに読んでもらう(K24)
        if self.mode == Mode::Wsl || (matches!(pathlike::detect(&input), Some(pathlike::Pasted::WslPath(_))) && self.wsl_root().is_none()) {
            self.shared.request_wsl();
        }
        if want_files {
            // Boxは`bx `, WSLは`w `のときだけ探す(K20, K24)
            let slot = match self.mode {
                Mode::Box => indexer::SLOT_BOX,
                Mode::Wsl => indexer::SLOT_WSL,
                _ => indexer::SLOT_USER,
            };
            let tables = self.shared.snapshot_slot(slot);
            if !tables.is_empty() {
                self.searcher.submit(Job { gen: self.gen, text: text.trim().to_string(), dirs_only: false, tables });
                self.pending = Some((self.gen, quick));
                unsafe {
                    SetTimer(Some(self.hwnd), TIMER_QUICK, QUICK_MS, None);
                }
                return;
            }
        }
        self.searcher.cancel();
        self.pending = None;
        self.set_rows(quick, vec![]);
    }

    /// ふつうの検索ではない一覧: クリップボードの履歴(`cb `), 空の入力の最近のもの, `list`のスクリプト(`wiki `). なければNone
    fn special_rows(&mut self, text: &str) -> Option<Vec<Row>> {
        if self.mode == Mode::Clipboard {
            return Some(clip_rows(&self.clips, text));
        }
        // 計算のモードの中でも, 計算の拡張(`mipi `)は使える(K43)
        if self.mode == Mode::Calc {
            if let Some(r) = self.ext_rows(text) {
                return Some(r);
            }
            let hints = self.ext.hints(text, &self.ext_taken());
            return if hints.is_empty() { None } else { Some(hints) };
        }
        if self.mode != Mode::All {
            return None;
        }
        if text.trim().is_empty() {
            if !self.config.search.recent {
                return None;
            }
            self.recent_view = true;
            return Some(self.with_ctx(&[], |ctx| hikidashi_core::recent::rows(ctx)));
        }
        let Some((i, rest)) = cscripts::list_target(&self.scripts, text) else { return self.ext_rows(text) };
        let rest = rest.to_string();
        let s = self.scripts.get(i)?.clone();
        scripts::ensure_list(&self.lists, &s, self.hwnd);
        let g = self.lists.lock().unwrap();
        let st = g.get(&s.path)?;
        Some(match (&st.cache, &st.error) {
            (Some(c), _) => c.rows(&rest, self.dict.get()),
            (None, Some(e)) => vec![cscripts::error_row(e)],
            (None, None) => vec![cscripts::loading_row()],
        })
    }

    /// 新しい行にする. 選んでいた項目がまだあれば選んだままにする
    fn set_rows(&mut self, rows: Vec<Row>, tables: Vec<Arc<FileTable>>) {
        // 同じ入力のまま(先に出した一覧にファイルの結果が足された)で, 自分で選びを動かしていたときだけ保つ
        let keep = if self.user_moved { self.rows.get(self.sel).and_then(|r| r.learn_key.clone()) } else { None };
        self.rows = rows;
        self.tables = tables;
        self.sel = keep.and_then(|k| self.rows.iter().position(|r| r.learn_key.as_deref() == Some(&k))).unwrap_or(0);
        self.top = 0;
        self.ensure_visible();
        self.layout();
    }

    fn on_files(&mut self, gen: u64) {
        if gen != self.gen {
            return;
        }
        let Some(done) = self.searcher.result.lock().unwrap().take() else { return };
        if done.gen != self.gen {
            return;
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_QUICK);
        }
        self.stats.last_search_ms = done.elapsed_ms;
        self.pending = None;
        let input = self.input();
        let (_, rows) = self.build_rows(&input, &done.hits, &done.tables);
        // 先に出した一覧で選びを動かしていなければ1行目を選ぶ
        self.set_rows(rows, done.tables);
    }

    /// 打ってすぐEnterを押したとき, 今の入力の結果がまだなら少し(最大0.3秒)待つ. 前の入力の結果を開かないように
    fn wait_pending(&mut self) {
        let Some((g, _)) = &self.pending else { return };
        let g = *g;
        if g != self.gen {
            return;
        }
        let until = Instant::now() + std::time::Duration::from_millis(300);
        while Instant::now() < until {
            let ready = self.searcher.result.lock().unwrap().as_ref().is_some_and(|d| d.gen == g);
            if ready {
                self.on_files(g);
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // 間に合わなければ, ファイル以外の結果で決める
        if let Some((_, rows)) = self.pending.take() {
            self.set_rows(rows, vec![]);
        }
    }

    fn on_quick_timer(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_QUICK);
        }
        if let Some((g, rows)) = self.pending.take() {
            if g == self.gen {
                self.set_rows(rows, vec![]);
                // ファイルの結果はまだ来る. 待っている印として世代は残す
            }
        }
    }

    pub fn status_text(&self) -> Option<(String, bool)> {
        if let Some(e) = &self.config_error {
            return Some((e.clone(), true));
        }
        if let Some(m) = &self.message {
            return Some(m.clone());
        }
        if self.composing() {
            return None;
        }
        let st = self.shared.status.lock().unwrap();
        let parts: Vec<&str> = st.iter().filter(|s| !s.is_empty()).map(String::as_str).collect();
        if !parts.is_empty() {
            return Some((parts.join("  "), false));
        }
        if self.capture {
            return Some(("Esc to cancel".into(), false));
        }
        if self.compose.is_some() && !self.drafts.draft.trim().is_empty() && self.mode == Mode::All && self.edit_text().is_empty() {
            let k = self.config.keys.label(KeyAction::Write);
            if !k.is_empty() {
                return Some((format!("Draft kept. {k} to continue writing"), false));
            }
        }
        None
    }

    /// 窓の高さと入力欄の位置を決めて描き直す
    pub fn layout(&mut self) {
        if self.composing() {
            self.layout_compose();
            return;
        }
        let d = self.dpi;
        let n = if let Some(m) = &self.menu { m.items.len() } else { self.visible_rows() };
        let mut h = theme::INPUT_H;
        if n > 0 || self.menu.is_some() {
            h += theme::LIST_PAD * 2 + n as i32 * theme::ROW_H;
            if self.menu.is_some() {
                h += theme::MENU_TITLE_H + theme::STATUS_H;
            }
        }
        if self.status_text().is_some() && self.menu.is_none() {
            h += theme::STATUS_H;
        }
        let height = scale(h, d);
        unsafe {
            let mut rc = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut rc);
            if rc.bottom - rc.top != height {
                let _ = SetWindowPos(self.hwnd, None, 0, 0, rc.right - rc.left, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
            self.position_edit();
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn chip_width(&self) -> i32 {
        if self.mode == Mode::All && !self.capture {
            0
        } else {
            paint::chip_width(self)
        }
    }

    fn position_edit(&self) {
        if !self.capture {
            let t = cue_for(self.mode);
            if CUE.get() != t {
                set_cue(self.edit, t);
            }
        }
        let d = self.dpi;
        let x = scale(theme::PAD_X, d) + if self.chip_width() > 0 { self.chip_width() + scale(10, d) } else { 0 };
        let eh = scale(theme::INPUT_FONT + 10, d);
        let y = (scale(theme::INPUT_H, d) - eh) / 2;
        let w = scale(theme::WIDTH - theme::PAD_X, d) - x;
        unsafe {
            let mut rc = RECT::default();
            let _ = GetWindowRect(self.edit, &mut rc);
            let mut pt = [POINT { x: rc.left, y: rc.top }];
            windows::Win32::Graphics::Gdi::MapWindowPoints(None, Some(self.hwnd), &mut pt);
            if (pt[0].x, pt[0].y, rc.right - rc.left, rc.bottom - rc.top) != (x, y, w, eh) {
                // 左の印が付いたり消えたりして動いたときは, 前の絵を写さずに全部描き直す(案内の文字の左が欠けたため. K43)
                let _ = SetWindowPos(self.edit, None, x, y, w, eh, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS);
                let _ = InvalidateRect(Some(self.edit), None, true);
            }
        }
    }

    fn set_dpi(&mut self, dpi: u32) {
        if dpi == self.dpi {
            return;
        }
        self.dpi = dpi;
        self.fonts = Fonts::new(dpi);
        self.icons.set_size(scale(theme::ICON, dpi));
        if let Some(c) = &mut self.compose {
            c.set_font_dpi(dpi);
        }
        unsafe {
            SendMessageW(self.edit, WM_SETFONT, Some(WPARAM(self.fonts.input.0 as usize)), Some(LPARAM(1)));
        }
    }

    /// 窓を出す(A2: マウスカーソルがあるモニターの中央やや上)
    fn show(&mut self) {
        self.show_inner(true);
    }

    /// `select_all`なら前回の入力を全選択で残す(A3). 選んだ文字や`--query`の文字を入れて出すときは, カーソルを末尾に置く
    fn show_inner(&mut self, select_all: bool) {
        let _d = diag::doing("show the window");
        self.sync_theme();
        let t0 = self.stats.hotkey_at.take().unwrap_or_else(Instant::now);
        unsafe {
            let fg = GetForegroundWindow();
            if fg != self.hwnd && !self.visible() {
                self.prev_fg = fg.0 as isize;
            }
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            // 窓が出たときにマウスの下の行が選ばれないよう, いまの位置を覚える
            self.last_mouse = (pt.x, pt.y);
            let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            self.set_dpi(dx);
            let wa = mi.rcWork;
            let w = scale(theme::WIDTH, dx);
            let x = wa.left + (wa.right - wa.left - w) / 2;
            let y = wa.top + (wa.bottom - wa.top) * 22 / 100;
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, scale(theme::INPUT_H, dx), SWP_NOACTIVATE);
        }
        // 窓の一覧は出すたびに取り直す(H4). プロセスはkillのときだけ取る
        let wins = winlist::windows(self.hwnd.0 as isize);
        self.windows = Catalog::new(winlist::window_items(&wins));
        self.processes_fresh = false;
        self.reload_bookmarks();
        self.check_scripts();
        self.reload_ext();
        // サイトのアイコンは, ブラウザーで開くと増えるので30分ごとに読み直す
        if self.favicons_at.is_none_or(|t| t.elapsed().as_secs() > 30 * 60) {
            self.load_favicons();
        }
        self.history.reset();
        // 印(モード)は引き継がない. 出したときは, いつも全部を探す(文字は残す. K51)
        if select_all && self.mode != Mode::All {
            self.mode = Mode::All;
            self.position_edit();
        }
        self.refresh();
        if self.composing() {
            // 書きかけのまま閉じたので, 書く窓で開く(Q5のA). 日本語で書くことが多いのでIMEは変えない
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOW);
                let _ = SetForegroundWindow(self.hwnd);
            }
            if let Some(c) = &self.compose {
                unsafe {
                    let _ = SetFocus(Some(c.hwnd));
                }
            }
            self.compose_sync();
            self.stats.last_show_ms = t0.elapsed().as_secs_f64() * 1000.0;
            return;
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.edit));
            if select_all {
                // 前回の入力を全選択で残す(A3)
                SendMessageW(self.edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            }
        }
        system::ime_off(self.edit);
        self.stats.last_show_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    fn hide(&mut self) {
        self.script_view = false;
        self.ext.reset();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            let _ = KillTimer(Some(self.hwnd), TIMER_QUICK);
        }
        self.searcher.cancel();
        self.menu = None;
        self.confirm = None;
        self.message = None;
        // 書く窓で閉じても, 次は検索から始める(書きかけは残る. `Shift+Enter`で続き. K51)
        self.save_draft();
        if let Some(c) = &mut self.compose {
            if c.active {
                c.active = false;
                let ch = c.hwnd;
                unsafe {
                    let _ = ShowWindow(ch, SW_HIDE);
                    let _ = ShowWindow(self.edit, SW_SHOW);
                }
            }
        }
        if self.capture {
            self.end_capture(false);
        }
    }

    fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    fn toggle(&mut self) {
        if self.visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    /// ChromeとEdgeのブックマークを読む(E1). ある方だけ. 2行目の頭にどちらのものかを出す
    fn reload_bookmarks(&mut self) {
        let _d = diag::doing("read bookmarks");
        let browsers = [actions::Browser::Chrome, actions::Browser::Edge];
        let paths: Vec<PathBuf> = browsers.iter().map(|b| b.user_data(&self.config).join(b.profile(&self.config)).join("Bookmarks")).collect();
        let mtimes: Vec<Option<SystemTime>> = paths.iter().map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok()).collect();
        if mtimes.iter().any(Option::is_some) && mtimes == self.bookmarks_mtime {
            return;
        }
        self.bookmarks_mtime = mtimes;
        let mut items = Vec::new();
        for (b, path) in browsers.iter().zip(&paths) {
            let Ok(text) = std::fs::read_to_string(path) else { continue };
            for mut it in builtin::parse_bookmarks(&text) {
                it.sub = format!("{} · {}", b.label(), it.sub);
                it.args = b.key().to_string();
                items.push(it);
            }
        }
        self.bookmarks = Catalog::new(items);
        self.load_favicons();
    }

    /// ブックマークのアイコン(favicon)を, ChromeとEdgeの`Favicons`から読む(2026-11-24). ブックマークを読み直したときと, 30分ごと.
    /// ブラウザーが使っている間は鍵がかかるので, 一時フォルダーに写して別のスレッドで読み, 終わったら知らせる
    fn load_favicons(&mut self) {
        self.favicons_at = Some(Instant::now());
        let mut jobs = vec![];
        for b in [actions::Browser::Chrome, actions::Browser::Edge] {
            let path = b.user_data(&self.config).join(b.profile(&self.config)).join("Favicons");
            let urls: Vec<String> = self.bookmarks.items.iter().filter(|it| it.args == b.key()).map(|it| it.target.clone()).collect();
            if !urls.is_empty() && path.is_file() {
                jobs.push((b.key().to_string(), path, urls));
            }
        }
        if jobs.is_empty() {
            return;
        }
        let want = scale(theme::ICON, self.dpi.max(96)) as i64;
        let out = self.favicons.clone();
        let hwnd = self.hwnd.0 as isize;
        let _ = std::thread::Builder::new().name("favicons".into()).spawn(move || {
            let mut all = std::collections::HashMap::new();
            let tmp = std::env::temp_dir().join("Hikidashi");
            let _ = std::fs::create_dir_all(&tmp);
            let mut total = 0;
            for (key, path, urls) in jobs {
                total += urls.len();
                let copy = tmp.join(format!("favicons-{key}.db"));
                if let Err(e) = std::fs::copy(&path, &copy) {
                    storage::log(&format!("favicons: could not copy {}: {e}", path.display()));
                    continue;
                }
                match hikidashi_core::boxdrive::Connection::open(&copy) {
                    Ok(conn) => {
                        for (k, v) in hikidashi_core::favicon::read(&conn, &urls, want) {
                            all.entry(k).or_insert(v);
                        }
                    }
                    Err(e) => storage::log(&format!("favicons: could not open {}: {e}", path.display())),
                }
                let _ = std::fs::remove_file(&copy);
            }
            storage::log(&format!("favicons: {} of {} bookmarks", all.len(), total));
            *out.lock().unwrap() = all;
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_APP_FAVICONS, WPARAM(0), LPARAM(0));
            }
        });
    }

    /// 塗ってある色と今のテーマが違えば塗り直す
    fn sync_theme(&mut self) {
        if self.themed_dark != theme::is_dark() {
            self.apply_theme();
        }
    }

    /// ライトとダークが変わったとき(K39): 枠, 書く窓, 入力欄を塗り直す
    fn apply_theme(&mut self) {
        self.themed_dark = theme::is_dark();
        set_border_color(self.hwnd);
        settings::apply_theme();
        if let Some(c) = &mut self.compose {
            c.apply_theme();
        }
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, true);
            let _ = InvalidateRect(Some(self.edit), None, true);
        }
    }

    fn reload_config(&mut self) {
        let (cfg, err) = storage::load_config();
        self.sync_theme();
        let files_changed = cfg.files != self.config.files || cfg.wsl != self.config.wsl || cfg.apps != self.config.apps;
        self.config_error = err.map(|e| e.0);
        self.config = cfg.clone();
        *self.shared.config.lock().unwrap() = cfg;
        self.rebuild_commands();
        self.ext.set_options(&self.config.ext);
        self.apply_apps();
        self.sync_clip_listener();
        self.apply_selection_hotkey();
        self.bookmarks_mtime.clear();
        self.reload_bookmarks();
        if files_changed {
            self.shared.rebuild.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(c) = &mut self.compose {
            c.hide_marks = self.config.chatgpt.hide_marks;
        }
        if self.visible() {
            if self.composing() {
                self.compose_sync();
            } else if !self.script_view {
                self.refresh();
            }
        }
        settings::config_changed();
    }

    fn apply_apps(&mut self) {
        let mut items = self.shared.apps.lock().unwrap().clone().unwrap_or_default();
        router::apply_aliases(&mut items, &self.config.alias);
        self.apps = Catalog::new(items);
    }

    fn stats_rows(&self) -> Vec<Row> {
        let t = self.shared.snapshot();
        let files: usize = t.iter().map(|x| x.len()).sum();
        let mem: usize = t.iter().map(|x| x.memory()).sum();
        let (ws, private) = system::memory();
        let lines = vec![
            (format!("Files: {}", group(files)), format!("{} indexes", t.len())),
            (format!("Index memory: {:.1} MB", mem as f64 / 1e6), "Names and parents".to_string()),
            (format!("Process memory: {:.1} MB", private as f64 / 1e6), format!("Working set {:.1} MB", ws as f64 / 1e6)),
            (format!("Last file search: {:.1} ms", self.stats.last_search_ms), "Target: 30 ms".to_string()),
            (format!("Last show: {:.1} ms", self.stats.last_show_ms), "Target: 50 ms".to_string()),
            (format!("Apps: {}  Bookmarks: {}", self.apps.len(), self.bookmarks.len()), format!("Dictionary: {}", self.dict.get().map(|d| d.len()).unwrap_or(0))),
            match self.shared.box_lookup.lock().unwrap().as_ref() {
                Some(b) => (
                    format!("Box: {}", group(b.table.len().saturating_sub(1))),
                    format!("Read from Box Drive in {:.1} s · {} IDs", b.took.as_secs_f64(), group(b.files.len() + b.folders.len())),
                ),
                None => ("Box: not read".to_string(), "Box Drive data (sync.db) was not found or could not be read. See hikidashi.log".to_string()),
            },
        ];
        let all: Vec<String> = lines.iter().map(|(a, b)| format!("{a} ({b})")).collect();
        let all = all.join("\n");
        lines
            .into_iter()
            .map(|(name, sub)| Row {
                name,
                sub,
                tag: "copy",
                positions: vec![],
                action: Action::Copy(all.clone()),
                learn_key: None,
                warn: false,
                icon: router::Icon::Calc,
                score: 0,
            })
            .collect()
    }

    // ---- 入力 ----

    fn on_text_changed(&mut self) {
        let text = self.edit_text();
        // 履歴から入れた文字はEN_CHANGEを通らないので, ここに来るのは自分で打ったとき
        self.history.reset();
        self.message = None;
        // 先頭に接頭辞とスペースを打ったら, 入力欄の左の印にする(K3)
        if self.mode == Mode::All {
            let (mode, rest) = router::parse(&text, &self.config.prefixes());
            if mode == Mode::ChatGpt && self.compose.is_some() {
                // `c `はすぐ長い質問の窓にする(K24のQ1)
                let rest = rest.to_string();
                self.start_chatgpt(&rest);
                return;
            }
            if mode != Mode::All {
                self.mode = mode;
                let rest = rest.to_string();
                self.set_edit_text(&rest);
            }
        }
        if self.animations {
            self.glow = Some(Instant::now());
            unsafe {
                SetTimer(Some(self.hwnd), TIMER_GLOW, 16, None);
            }
        }
        self.refresh();
    }

    /// 入力欄の文字を変える(EN_CHANGEはwith_appが使用中なので無視される)
    fn set_edit_text(&mut self, s: &str) {
        let w = WStr::new(s);
        unsafe {
            let _ = SetWindowTextW(self.edit, w.pcwstr());
            let n = s.encode_utf16().count();
            SendMessageW(self.edit, EM_SETSEL, Some(WPARAM(n)), Some(LPARAM(n as isize)));
        }
        self.position_edit();
    }

    fn set_input(&mut self, input: &str) {
        let (mode, rest) = router::parse(input, &self.config.prefixes());
        if mode == Mode::ChatGpt && self.compose.is_some() {
            let rest = rest.to_string();
            self.start_chatgpt(&rest);
            return;
        }
        self.mode = mode;
        let rest = rest.to_string();
        self.set_edit_text(&rest);
        self.refresh();
    }

    /// 窓に見えている行の数(最大8行. C2)
    pub fn visible_rows(&self) -> usize {
        self.rows.len().min(router::VISIBLE_ROWS)
    }

    /// 選んでいる行が見えるようにスクロールする
    fn ensure_visible(&mut self) {
        let vis = router::VISIBLE_ROWS;
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + vis {
            self.top = self.sel + 1 - vis;
        }
        self.top = self.top.min(self.rows.len().saturating_sub(vis));
    }

    /// 選びをその行に移す(PageUp/PageDown, マウス)
    fn select(&mut self, i: usize) {
        if let Some(m) = &mut self.menu {
            if i < m.items.len() && m.sel != i {
                m.sel = i;
                unsafe {
                    let _ = InvalidateRect(Some(self.hwnd), None, false);
                }
            }
            return;
        }
        if i >= self.rows.len() || i == self.sel {
            return;
        }
        self.sel = i;
        self.user_moved = true;
        self.confirm = None;
        self.ensure_visible();
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// ホイールで見える範囲を動かす. 選びは見えている行の中に収める(C2)
    fn scroll(&mut self, lines: i32) {
        if self.composing() || self.menu.is_some() || self.rows.len() <= router::VISIBLE_ROWS {
            return;
        }
        let max_top = (self.rows.len() - router::VISIBLE_ROWS) as i32;
        let top = (self.top as i32 + lines).clamp(0, max_top) as usize;
        if top == self.top {
            return;
        }
        self.top = top;
        let last = top + router::VISIBLE_ROWS - 1;
        if self.sel < top || self.sel > last {
            self.sel = self.sel.clamp(top, last);
            self.user_moved = true;
            self.confirm = None;
        }
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// クライアントの座標にある行(一覧かTabのメニュー)
    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        let d = self.dpi;
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let pad = scale(theme::LIST_PAD, d);
        if x < pad || x >= rc.right - pad {
            return None;
        }
        let rh = scale(theme::ROW_H, d);
        let (y0, n, off) = if let Some(m) = &self.menu {
            (scale(theme::INPUT_H, d) + scale(theme::MENU_TITLE_H, d) + pad, m.items.len(), 0)
        } else {
            (scale(theme::INPUT_H, d) + pad, self.visible_rows(), self.top)
        };
        if y < y0 {
            return None;
        }
        let i = ((y - y0) / rh) as usize;
        (i < n).then_some(i + off)
    }

    fn on_mouse_move(&mut self, x: i32, y: i32) {
        let mut pt = POINT { x, y };
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(self.hwnd, &mut pt);
        }
        // 窓が出たり大きさが変わったりしたときにも届くので, マウスが本当に動いたときだけ
        if (pt.x, pt.y) == self.last_mouse || self.composing() {
            return;
        }
        self.last_mouse = (pt.x, pt.y);
        if let Some(i) = self.row_at(x, y) {
            self.select(i);
        }
    }

    /// 1回のクリックで開く(押した行と離した行が同じとき)
    fn on_click(&mut self, x: i32, y: i32, down: bool) {
        if self.composing() {
            return;
        }
        let hit = self.row_at(x, y);
        if down {
            self.press_row = hit;
            if let Some(i) = hit {
                self.select(i);
            }
            return;
        }
        let pressed = self.press_row.take();
        if hit.is_some() && hit == pressed {
            self.user_moved = true;
            if let Some(i) = hit {
                self.select(i);
            }
            self.on_enter(0);
        }
    }

    /// 最近のものを出していて, まだ何も選んでいない
    pub fn nothing_selected(&self) -> bool {
        self.recent_view && !self.user_moved && self.menu.is_none()
    }

    fn move_sel(&mut self, delta: i32) {
        // 何も選ばれていない形からの最初の↓は1行目を選ぶ
        if self.nothing_selected() && !self.rows.is_empty() {
            self.sel = if delta > 0 { 0 } else { self.rows.len() - 1 };
            self.user_moved = true;
            self.ensure_visible();
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
            return;
        }
        if let Some(m) = &mut self.menu {
            let n = m.items.len() as i32;
            if n > 0 {
                m.sel = ((m.sel as i32 + delta).rem_euclid(n)) as usize;
            }
        } else {
            let n = self.rows.len() as i32;
            if n > 0 {
                self.sel = ((self.sel as i32 + delta).rem_euclid(n)) as usize;
                self.user_moved = true;
                self.ensure_visible();
            }
            self.confirm = None;
        }
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// キーの処理. 処理したらtrue(入力欄には渡さない)
    fn on_key(&mut self, vk: u16, sys: bool) -> bool {
        let down = |k: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe { GetKeyState(k.0 as i32) < 0 };
        let ctrl = down(VK_CONTROL);
        let shift = down(VK_SHIFT);
        let alt = sys || down(VK_MENU);
        if self.capture {
            return self.capture_key(vk, ctrl, shift, alt, down(VK_LWIN) || down(VK_RWIN));
        }
        match vk {
            v if v == VK_RETURN.0 => {
                let mut mods = 0;
                if ctrl {
                    mods |= KEY_CTRL;
                }
                if shift {
                    mods |= KEY_SHIFT;
                }
                if alt {
                    mods |= KEY_ALT;
                }
                self.on_enter(mods);
                true
            }
            // 入力欄の終わりで→: 操作のメニュー. ←で戻る(K52. Flow Launcherと同じ)
            v if v == VK_RIGHT.0 && !ctrl && !shift && !alt => {
                if self.menu.is_none() {
                    if !self.caret_at_end() {
                        return false;
                    }
                    self.open_menu();
                }
                true
            }
            v if v == VK_LEFT.0 && self.menu.is_some() => {
                self.menu = None;
                self.layout();
                true
            }
            // Tab: 選んだ行を入力欄に入れて, 続きを打てるようにする(補完)
            v if v == VK_TAB.0 => {
                if self.menu.is_none() {
                    self.complete();
                }
                true
            }
            v if v == VK_ESCAPE.0 => {
                if self.menu.is_some() {
                    self.menu = None;
                    self.layout();
                } else {
                    self.hide();
                }
                true
            }
            v if v == VK_UP.0 || (ctrl && v == b'P' as u16) => {
                let empty = self.edit_text().is_empty() && self.mode == Mode::All;
                if self.menu.is_none() && (empty || self.history.is_browsing()) {
                    // 入力欄が空なら前に打った文字を呼び出す(J5)
                    if let Some(s) = self.history.older().map(str::to_string) {
                        self.from_history = true;
                        self.set_input(&s);
                    }
                } else {
                    self.move_sel(-1);
                }
                true
            }
            v if v == VK_DOWN.0 || (ctrl && v == b'N' as u16) => {
                if self.menu.is_none() && self.history.is_browsing() {
                    let s = self.history.newer().map(str::to_string).unwrap_or_default();
                    self.from_history = true;
                    self.set_input(&s);
                } else {
                    self.move_sel(1);
                }
                true
            }
            v if v == VK_PRIOR.0 => {
                if self.menu.is_none() {
                    self.select(self.sel.saturating_sub(router::VISIBLE_ROWS));
                }
                true
            }
            v if v == VK_NEXT.0 => {
                if self.menu.is_none() && !self.rows.is_empty() {
                    self.select((self.sel + router::VISIBLE_ROWS).min(self.rows.len() - 1));
                }
                true
            }
            v if v == VK_BACK.0 => {
                if ctrl {
                    self.delete_word();
                    return true;
                }
                // 入力欄が空なら左の印を消す
                if self.mode != Mode::All && self.edit_text().is_empty() {
                    self.mode = Mode::All;
                    self.refresh();
                    return true;
                }
                false
            }
            v if ctrl && v == b'A' as u16 => {
                unsafe {
                    SendMessageW(self.edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
                }
                true
            }
            // 2行以上の文を貼ったら書く窓にする(K22)
            v if (ctrl && !shift && v == b'V' as u16) || (shift && !ctrl && v == VK_INSERT.0) => self.paste_multiline(),
            _ => false,
        }
    }

    /// Ctrl+Backspace: カーソルの前の1語を消す
    fn delete_word(&mut self) {
        let text = self.edit_text();
        let mut start = 0u32;
        let mut end = 0u32;
        unsafe {
            SendMessageW(self.edit, EM_GETSEL, Some(WPARAM(&mut start as *mut _ as usize)), Some(LPARAM(&mut end as *mut _ as isize)));
        }
        let units: Vec<u16> = text.encode_utf16().collect();
        let caret = (end as usize).min(units.len());
        let before = String::from_utf16_lossy(&units[..caret]);
        let after = String::from_utf16_lossy(&units[caret..]);
        let trimmed = before.trim_end_matches(|c: char| c == ' ' || c == '\\' || c == '/');
        let cut = trimmed.rfind([' ', '\\', '/', '.', '_', '-']).map(|i| i + 1).unwrap_or(0);
        let new = format!("{}{}", &trimmed[..cut], after);
        self.set_edit_text(&new);
        let n = trimmed[..cut].encode_utf16().count();
        unsafe {
            SendMessageW(self.edit, EM_SETSEL, Some(WPARAM(n)), Some(LPARAM(n as isize)));
        }
        self.refresh();
    }

    // ---- 長い質問を書く窓(K22) ----

    pub fn composing(&self) -> bool {
        self.compose.as_ref().is_some_and(|c| c.active)
    }

    /// 書く窓にする. `caret`がNoneなら最後にカーソルを置く
    fn enter_compose(&mut self, text: String, caret: Option<usize>) {
        let dpi = self.dpi;
        let hide_marks = self.config.chatgpt.hide_marks;
        let Some(c) = &mut self.compose else { return };
        c.hide_marks = hide_marks;
        c.set_font_dpi(dpi);
        c.set_text(&text);
        let n = caret.unwrap_or_else(|| compose::normalize(&text).encode_utf16().count());
        c.set_sel(n, n);
        c.active = true;
        let ch = c.hwnd;
        self.browse.reset();
        self.menu = None;
        self.confirm = None;
        self.message = None;
        self.mode = Mode::All;
        self.searcher.cancel();
        self.pending = None;
        self.set_edit_text("");
        unsafe {
            let _ = ShowWindow(self.edit, SW_HIDE);
            let _ = ShowWindow(ch, SW_SHOW);
            let _ = SetFocus(Some(ch));
        }
        self.compose_sync();
    }

    /// `c `(K24): 書く窓にする. 続きの文字がなければ書きかけを開き, あれば前の書きかけを履歴に移してその文字から書く
    fn start_chatgpt(&mut self, rest: &str) {
        if rest.trim().is_empty() {
            let d = self.drafts.draft.clone();
            self.enter_compose(d, None);
        } else {
            self.keep_old_draft();
            self.enter_compose(rest.trim_end().to_string(), None);
        }
    }

    /// 書く窓から検索に戻る(Esc). 書きかけは残す
    fn leave_compose(&mut self) {
        self.save_draft();
        let Some(c) = &mut self.compose else { return };
        c.active = false;
        let ch = c.hwnd;
        unsafe {
            let _ = ShowWindow(ch, SW_HIDE);
            let _ = ShowWindow(self.edit, SW_SHOW);
            let _ = SetFocus(Some(self.edit));
        }
        self.refresh();
    }

    /// 書く窓の書きかけを覚えてファイルに書く(閉じたときなど)
    fn save_draft(&mut self) {
        let Some(c) = &self.compose else { return };
        if !c.active {
            return;
        }
        let t = c.text();
        self.drafts.draft = if t.trim().is_empty() { String::new() } else { t.replace('\r', "\n") };
        storage::save_drafts(&self.drafts);
    }

    /// 新しく書き始めるとき, 前の書きかけは`Ctrl+↑`で呼び出せるように履歴に移す
    fn keep_old_draft(&mut self) {
        let d = std::mem::take(&mut self.drafts.draft);
        if !d.trim().is_empty() {
            self.drafts.push_sent(&d);
            storage::save_drafts(&self.drafts);
        }
    }

    /// 検索の入力欄に2行以上の文を貼ったら, 書く窓にして続きを書けるようにする. 処理したらtrue
    fn paste_multiline(&mut self) -> bool {
        if self.compose.is_none() || self.capture {
            return false;
        }
        let Some(t) = actions::clipboard_text(self.hwnd) else { return false };
        let body = t.trim_end_matches(['\r', '\n']);
        if !body.contains(['\r', '\n']) {
            return false;
        }
        let text = self.edit_text();
        let (mut s, mut e) = (0u32, 0u32);
        unsafe {
            SendMessageW(self.edit, EM_GETSEL, Some(WPARAM(&mut s as *mut _ as usize)), Some(LPARAM(&mut e as *mut _ as isize)));
        }
        let units: Vec<u16> = text.encode_utf16().collect();
        let (s, e) = ((s as usize).min(units.len()), (e as usize).min(units.len()));
        let before = String::from_utf16_lossy(&units[..s.min(e)]);
        let after = String::from_utf16_lossy(&units[e.max(s)..]);
        let pasted = compose::normalize(body);
        let caret = before.encode_utf16().count() + pasted.encode_utf16().count();
        self.keep_old_draft();
        self.enter_compose(format!("{before}{pasted}{after}"), Some(caret));
        true
    }

    /// 貼る1行(改行の前まで. 入力欄に入る長さまで). 文字がなければNone
    fn paste_line(&mut self) -> Option<String> {
        if self.capture {
            return None;
        }
        let t = actions::clipboard_text(self.hwnd)?;
        let line = t.split(['\r', '\n', '\0']).next().unwrap_or("");
        let room = EDIT_LIMIT.saturating_sub(self.edit_text().encode_utf16().count());
        let mut used = 0;
        Some(line.chars().take_while(|c| { used += c.len_utf16(); used <= room }).collect())
    }

    /// 整えて, 中身の高さを測り, 窓の大きさを合わせる
    fn compose_sync(&mut self) {
        let Some(c) = &mut self.compose else { return };
        let ms = c.restyle();
        if ms > 50.0 {
            storage::log(&format!("compose: restyle took {ms:.0} ms ({} chars)", c.chars()));
        }
        REQ_H.set(None);
        c.request_resize();
        if let Some(h) = REQ_H.take() {
            self.compose_h = h;
        }
        self.layout();
    }

    /// 整える頼み. RichEditの通知の中では整えず, あとで(WM_APP_COMPOSE)まとめて行う
    fn schedule_restyle(&mut self, text_changed: bool) {
        let first = !self.restyle_text && !self.restyle_sel;
        if text_changed {
            self.restyle_text = true;
        } else {
            self.restyle_sel = true;
        }
        if first {
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_APP_COMPOSE, WPARAM(0), LPARAM(0));
            }
        }
    }

    fn on_compose_posted(&mut self) {
        let text = std::mem::take(&mut self.restyle_text);
        self.restyle_sel = false;
        let Some(c) = &self.compose else { return };
        if !c.active || c.ime {
            return;
        }
        // カーソルが同じ行の中で動いただけなら整え直さない
        if text || c.caret_moved_line() {
            self.compose_sync();
        }
    }

    /// 書く窓の文字が変わった(自分で打った. Hikidashiが入れたときも呼ぶ)
    fn compose_changed(&mut self) {
        let _d = diag::doing("long question: restyle");
        // 呼び出した文を入れたときの通知は, RichEditから遅れて届くことがある. 中身が呼び出した文のままなら続ける
        if !self.recalled_unchanged() {
            self.browse.reset();
        }
        if self.animations {
            self.glow = Some(Instant::now());
            unsafe {
                SetTimer(Some(self.hwnd), TIMER_GLOW, 16, None);
            }
        }
        self.schedule_restyle(true);
    }

    /// 書く窓の窓の高さ. ランチャーと同じ幅で, 中身に合わせて下に伸び, 画面の高さの6割で止まる(Q6のA)
    fn layout_compose(&mut self) {
        let d = self.dpi;
        let work_h = unsafe {
            let mon = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            mi.rcWork.bottom - mi.rcWork.top
        };
        let band = scale(theme::COMPOSE_BAND, d);
        let (top, bottom, foot) = (band + scale(theme::COMPOSE_TOP, d), scale(theme::COMPOSE_BOTTOM, d), scale(theme::STATUS_H, d));
        let fixed = top + bottom + foot;
        let max_body = (work_h * 60 / 100 - fixed).max(scale(80, d));
        let body = self.compose_h.max(scale(theme::COMPOSE_MIN_BODY, d)).min(max_body);
        let height = fixed + body;
        let Some(c) = &self.compose else { return };
        unsafe {
            let mut rc = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut rc);
            if rc.bottom - rc.top != height {
                let _ = SetWindowPos(self.hwnd, None, 0, 0, rc.right - rc.left, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
            let x = scale(theme::PAD_X, d);
            let _ = SetWindowPos(c.hwnd, None, x, top, scale(theme::WIDTH, d) - x - scale(8, d), body, SWP_NOZORDER | SWP_NOACTIVATE);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// 書く窓のキー. 処理したらtrue(RichEditには渡さない)
    fn compose_key(&mut self, vk: u16, sys: bool) -> bool {
        let down = |k: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| unsafe { GetKeyState(k.0 as i32) < 0 };
        let ctrl = down(VK_CONTROL);
        let shift = down(VK_SHIFT);
        let alt = sys || down(VK_MENU);
        if alt {
            return vk == VK_RETURN.0;
        }
        let empty = self.compose.as_ref().is_some_and(|c| c.text().is_empty());
        match vk {
            // Enterで送る, Shift+Enterで改行(Q2のB). Ctrl+Enterも送る
            v if v == VK_RETURN.0 => {
                if shift && !ctrl {
                    self.compose_newline();
                } else if !shift {
                    self.compose_send();
                }
                true
            }
            v if v == VK_ESCAPE.0 => {
                self.leave_compose();
                true
            }
            v if ctrl && (v == VK_UP.0 || v == VK_DOWN.0) => {
                self.compose_history(v == VK_UP.0);
                true
            }
            // 空なら↑だけでも前に送った文を呼び出す. 呼び出した文のままなら, 続けて↑でさらに前, ↓で戻る(検索の入力欄のJ5と同じ)
            v if v == VK_UP.0 && !shift && (empty || self.recalled_unchanged()) => {
                self.compose_history(true);
                true
            }
            v if v == VK_DOWN.0 && !shift && self.recalled_unchanged() => {
                self.compose_history(false);
                true
            }
            v if v == VK_TAB.0 && !ctrl => self.compose_indent(shift),
            // 空でBackspaceなら検索に戻る
            v if v == VK_BACK.0 && !ctrl && empty => {
                self.leave_compose();
                true
            }
            v if ctrl && v == b'B' as u16 => {
                self.compose_wrap("**");
                true
            }
            v if ctrl && v == b'I' as u16 => {
                self.compose_wrap("*");
                true
            }
            v if (ctrl && !shift && v == b'V' as u16) || (shift && !ctrl && v == VK_INSERT.0) => {
                self.compose_paste();
                true
            }
            // RichEditの飾りのキー(Ctrl+U, Ctrl+E, Ctrl+Shift+Lなど)は使わない. 全選択, コピー, 切り取り, 元に戻すだけ通す
            v if ctrl && ((0x30..=0x5A).contains(&v) || (0xBA..=0xC0).contains(&v) || (0xDB..=0xDE).contains(&v)) => {
                let pass = matches!(v as u8, b'C' | b'X' | b'Z' | b'Y') || (v == b'A' as u16 && !shift);
                !pass
            }
            _ => false,
        }
    }

    /// カーソルの行の頭と終わり(UTF-16)と, 文全体
    fn compose_line(&self, at: usize) -> Option<(Vec<u16>, usize, usize)> {
        let c = self.compose.as_ref()?;
        let units: Vec<u16> = c.text().encode_utf16().collect();
        let at = at.min(units.len());
        let ls = units[..at].iter().rposition(|u| *u == 13 || *u == 10).map(|i| i + 1).unwrap_or(0);
        let le = units[at..].iter().position(|u| *u == 13 || *u == 10).map(|i| at + i).unwrap_or(units.len());
        Some((units, ls, le))
    }

    /// Shift+Enter: 改行. 箇条書きなら次の印を付け, 印だけの行なら印を消して箇条書きを終える
    fn compose_newline(&mut self) {
        let Some(c) = &self.compose else { return };
        let (a, b) = c.sel();
        let Some((units, ls, le)) = self.compose_line(a) else { return };
        let what = if a == b {
            let whole = String::from_utf16_lossy(&units[ls..le]);
            let before = String::from_utf16_lossy(&units[ls..a]);
            markdown::new_line(&before, &whole)
        } else {
            NewLine::Plain
        };
        let Some(c) = &self.compose else { return };
        match what {
            NewLine::Continue(p) => c.replace_sel(&format!("\r{p}")),
            NewLine::EndList(n) => {
                c.set_sel(ls, ls + n);
                c.replace_sel("");
            }
            NewLine::Plain => c.replace_sel("\r"),
        }
        self.compose_changed();
    }

    /// Tab, Shift+Tab: 箇条書きの行を字下げする. 箇条書きでなければTabはそのまま入れる
    fn compose_indent(&mut self, outdent: bool) -> bool {
        let Some(c) = &self.compose else { return false };
        let (a, b) = c.sel();
        let Some((units, ls, le)) = self.compose_line(a) else { return false };
        let whole = String::from_utf16_lossy(&units[ls..le]);
        if !markdown::is_list_line(&whole) {
            return outdent;
        }
        let Some(c) = &self.compose else { return false };
        if outdent {
            let k = whole.chars().take(2).take_while(|ch| *ch == ' ').count();
            if k == 0 {
                return true;
            }
            c.set_sel(ls, ls + k);
            c.replace_sel("");
            c.set_sel(a.saturating_sub(k).max(ls), b.saturating_sub(k).max(ls));
        } else {
            c.set_sel(ls, ls);
            c.replace_sel("  ");
            c.set_sel(a + 2, b + 2);
        }
        self.compose_changed();
        true
    }

    /// Ctrl+B, Ctrl+I: 選んだ文字を`**`や`*`で囲む
    fn compose_wrap(&mut self, m: &str) {
        let Some(c) = &self.compose else { return };
        let (a, b) = c.sel();
        let units: Vec<u16> = c.text().encode_utf16().collect();
        let inner = String::from_utf16_lossy(&units[a.min(units.len())..b.min(units.len())]);
        c.replace_sel(&format!("{m}{inner}{m}"));
        let ml = m.len();
        c.set_sel(a + ml, b + ml);
        self.compose_changed();
    }

    /// 貼り付けは字だけにする(書式や画像は入れない)
    fn compose_paste(&mut self) {
        let Some(t) = actions::clipboard_text(self.hwnd) else { return };
        let Some(c) = &self.compose else { return };
        c.replace_sel(&t);
        self.compose_changed();
    }

    /// Ctrl+↑/Ctrl+↓: 送った文を呼び出す
    /// 呼び出した文のまま(手を加えていない)か
    fn recalled_unchanged(&self) -> bool {
        self.browse.is_browsing() && self.compose.as_ref().is_some_and(|c| c.text() == self.recalled)
    }

    fn compose_history(&mut self, older: bool) {
        let Some(c) = &mut self.compose else { return };
        let cur = c.text();
        let r = if older { self.browse.older(&self.drafts.sent, &cur) } else { self.browse.newer(&self.drafts.sent) };
        storage::log(&format!("compose: Ctrl+{} ({} sent kept, found: {})", if older { "Up" } else { "Down" }, self.drafts.sent.len(), r.is_some()));
        let Some(t) = r else { return };
        c.set_text(&t);
        self.recalled = compose::normalize(&t);
        let n = self.recalled.encode_utf16().count();
        c.set_sel(n, n);
        self.schedule_restyle(true);
    }

    /// Enter: ChatGPTに送る. 長いのでURLではなく貼り付けで入れる(Q4のA)
    fn compose_send(&mut self) {
        let Some(c) = &self.compose else { return };
        let text = c.text();
        let q = text.trim();
        if q.is_empty() {
            return;
        }
        let q = q.replace('\r', "\n");
        match actions::ask_chatgpt_long(self.hwnd, &q.replace('\n', "\r\n"), &self.config) {
            Ok(()) => {
                self.drafts.push_sent(&q);
                self.drafts.draft.clear();
                storage::save_drafts(&self.drafts);
                self.browse.reset();
                if let Some(c) = &mut self.compose {
                    c.set_text("");
                    c.active = false;
                    let ch = c.hwnd;
                    unsafe {
                        let _ = ShowWindow(ch, SW_HIDE);
                        let _ = ShowWindow(self.edit, SW_SHOW);
                        let _ = SetFocus(Some(self.edit));
                    }
                }
                self.hide();
            }
            Err(e) => self.fail(e),
        }
    }

    /// 貼り付けに使ったあと, クリップボードを元に戻す(その間にほかの物をコピーしていなければ)
    fn restore_clipboard(&mut self, r: actions::ClipRestore) {
        if actions::clipboard_text(self.hwnd).as_deref() == Some(r.ours.as_str()) {
            let _ = actions::copy_text(self.hwnd, &r.prev);
            // 元に戻しただけなので, 履歴の先頭に出し直さない
            self.clip_ignore = Some(clipboard::sequence());
        }
    }

    // ---- ホットキーの登録(A1) ----

    /// ホットキーを登録しなおす. `selection`なら選んだ文字で開くホットキー, でなければメインのホットキー
    fn start_capture(&mut self, selection: bool) {
        system::unregister_hotkey(self.hwnd);
        if self.sel_registered {
            system::unregister_hotkey_id(self.hwnd, system::HOTKEY_SELECTION_ID);
            self.sel_registered = false;
        }
        self.capture_sel = selection;
        self.capture = true;
        self.mode = Mode::All;
        self.set_edit_text("");
        self.rows.clear();
        self.top = 0;
        self.message = Some((
            if selection { "Press the hotkey that opens Hikidashi with the selected text, for example Ctrl+Alt+Space" } else { "Press the hotkey you want to use, for example Alt+Space" }.into(),
            false,
        ));
        if !self.visible() {
            self.show();
        }
        set_cue(self.edit, "Press a hotkey…");
        self.layout();
    }

    fn end_capture(&mut self, keep_message: bool) {
        self.capture = false;
        if !keep_message {
            self.message = None;
        }
        if let Some(h) = self.hotkey {
            system::register_hotkey(self.hwnd, h);
        }
        self.capture_sel = false;
        self.apply_selection_hotkey();
        set_cue(self.edit, CUE_SEARCH);
        self.layout();
    }

    fn capture_key(&mut self, vk: u16, ctrl: bool, shift: bool, alt: bool, win: bool) -> bool {
        if [VK_SHIFT.0, VK_CONTROL.0, VK_MENU.0, VK_LWIN.0, VK_RWIN.0, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5].contains(&vk) {
            return true;
        }
        if vk == VK_ESCAPE.0 && !(ctrl || shift || alt || win) {
            let was_sel = self.capture_sel;
            self.end_capture(false);
            if self.hotkey.is_none() && !was_sel {
                self.message = Some(("No hotkey yet. Use the tray icon to set one.".into(), false));
            }
            self.layout();
            return true;
        }
        let mut mods = 0;
        if ctrl {
            mods |= MOD_CONTROL;
        }
        if alt {
            mods |= MOD_ALT;
        }
        if shift {
            mods |= MOD_SHIFT;
        }
        if win {
            mods |= MOD_WIN;
        }
        let h = Hotkey { mods, vk: vk as u32 };
        if !h.is_usable() {
            self.message = Some((format!("{} would block normal typing. Add Ctrl, Alt, Shift or Win.", h.format()), true));
            self.layout();
            return true;
        }
        if self.capture_sel {
            return self.capture_selection_key(h);
        }
        if system::register_hotkey(self.hwnd, h) {
            self.hotkey = Some(h);
            self.capture = false;
            self.apply_selection_hotkey();
            self.state.hotkey = Some(h.format());
            storage::save_state(&self.state);
            self.capture = false;
            self.message = Some((format!("Hotkey set: {}", h.format()), false));
            settings::set_hotkey(&h.format());
            set_cue(self.edit, CUE_SEARCH);
            self.layout();
            if !self.state.autostart_asked {
                self.state.autostart_asked = true;
                storage::save_state(&self.state);
                let hwnd = self.hwnd;
                let exe = self.exe_path.clone();
                // メッセージボックスの間に窓が隠れないよう, 後で聞く
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_ASK_AUTOSTART, WPARAM(0), LPARAM(0));
                }
                let _ = exe;
            }
        } else {
            self.message = Some((format!("{} is used by another app. Try another key.", h.format()), true));
            self.layout();
        }
        true
    }

    // ---- 実行 ----

    fn finish(&mut self, learn_key: Option<String>) {
        let input = self.input();
        let (_, text) = router::parse(&input, &self.config.prefixes());
        if let Some(k) = learn_key {
            self.learn.record(text, &k, util::now_secs());
            storage::save_learn(&self.learn);
        }
        self.history.push(&input);
        storage::save_history(&self.history);
        self.hide();
    }

    fn fail(&mut self, msg: String) {
        storage::log(&format!("error: {msg}"));
        self.message = Some((msg, true));
        self.layout();
    }

    fn item(&self, src: Source, index: usize) -> Option<Item> {
        let c = match src {
            Source::Apps => &self.apps,
            Source::Bookmarks => &self.bookmarks,
            Source::Processes => &self.processes,
            Source::Settings => &self.settings,
            Source::System => &self.system,
            Source::Commands => &self.commands,
            Source::Folders => &self.folders,
        };
        c.items.get(index).cloned()
    }

    /// 行が指すパス(ファイル, フォルダー, アプリのショートカット)
    fn row_path(&self, row: &Row) -> Option<String> {
        match &row.action {
            Action::File { table, id } => self.tables.get(*table).map(|t| t.path(*id)),
            Action::Path(p) => Some(p.clone()),
            Action::BoxUrl { local: Some(p), .. } => Some(p.clone()),
            Action::Item { source, index } => {
                let it = self.item(*source, *index)?;
                match it.kind {
                    Kind::App if !it.target.starts_with("shell:") => Some(it.target),
                    Kind::Window | Kind::Process if !it.args.is_empty() => Some(it.args),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// 行がフォルダーか(ファイルやフォルダーの行だけ)
    fn row_is_dir(&self, row: &Row) -> bool {
        match &row.action {
            Action::File { table, id } => self.tables.get(*table).is_some_and(|t| t.is_dir(*id)),
            Action::Path(p) => Path::new(p).is_dir(),
            Action::BoxUrl { local: Some(_), folder, .. } => *folder,
            Action::Item { source, index } => self.item(*source, *index).is_some_and(|it| it.kind == Kind::Folder),
            _ => false,
        }
    }

    /// 入力欄のカーソルが終わりにあり, 何も選んでいないか
    fn caret_at_end(&self) -> bool {
        let (mut s, mut e) = (0u32, 0u32);
        unsafe {
            SendMessageW(self.edit, EM_GETSEL, Some(WPARAM(&mut s as *mut _ as usize)), Some(LPARAM(&mut e as *mut _ as isize)));
        }
        s == e && e == self.edit_text().encode_utf16().count() as u32
    }

    /// `Tab`: 選んだ行を入力欄に入れる(補完. K52). 拡張の候補, パス, フォルダー, アプリなどの名前
    fn complete(&mut self) {
        self.wait_pending();
        if !self.wait_ext() {
            return;
        }
        let text = self.edit_text();
        let Some(row) = self.rows.get(self.sel).cloned() else { return };
        let mut mode = self.mode;
        let t = if let Action::Fill(t) = &row.action {
            Some(t.clone())
        } else if self.ext.view {
            self.ext.completes.get(self.sel).cloned().flatten()
        } else if matches!(self.mode, Mode::Calc | Mode::Clipboard | Mode::Google) || self.script_view {
            None
        } else {
            let is_dir = self.row_is_dir(&row) || row.tag == "folder";
            let path = match &row.action {
                Action::File { .. } | Action::Path(_) => self.row_path(&row),
                _ => None,
            };
            let done = if self.mode == Mode::All { router::completion(&text, &row, is_dir) } else { None };
            done.or_else(|| {
                // ファイルとフォルダーはパス(フォルダーなら中身が続けて出る), ほかは名前
                path.map(|p| {
                    mode = Mode::All;
                    if is_dir && !p.ends_with('\\') { format!("{p}\\") } else { p }
                })
            })
            .or_else(|| matches!(row.action, Action::Item { .. }).then(|| row.name.clone()))
        };
        let Some(t) = t.filter(|t| *t != text) else { return };
        self.mode = mode;
        self.set_edit_text(&t);
        self.refresh();
    }

    /// `Alt+Enter`: ファイルやフォルダーの行を, ターミナルでそのフォルダーに入って開く(Q6)
    fn on_alt_enter(&mut self) {
        if self.menu.is_some() {
            return;
        }
        self.wait_pending();
        let Some(row) = self.rows.get(self.sel).cloned() else { return };
        let file_like = match &row.action {
            Action::File { .. } | Action::Path(_) | Action::BoxUrl { local: Some(_), .. } => true,
            Action::Item { source, index } => self.item(*source, *index).is_some_and(|it| matches!(it.kind, Kind::File | Kind::Folder)),
            _ => false,
        };
        let Some(p) = self.row_path(&row).filter(|_| file_like) else { return };
        let r = match actions::terminal(&self.config) {
            Some(t) => actions::open_with(&t, &p, self.row_is_dir(&row), true),
            None => Err("No terminal found. Install WezTerm or Windows Terminal, or add one in Settings > Apps".into()),
        };
        match r {
            Ok(()) => self.finish(row.learn_key.clone()),
            Err(e) => self.fail(e),
        }
    }

    /// Enter(修飾キーなし)と, 修飾キーを足したEnter. 修飾キーで何をするかは設定の`[keys]`で決める(2026-11-24)
    fn on_enter(&mut self, mods: u8) {
        let _d = diag::doing("Enter (open)");
        if let Some(m) = self.menu.take() {
            if let Some(mi) = m.items.get(m.sel).cloned() {
                self.run_menu(&m.row, &mi.act);
            }
            return;
        }
        let text = self.edit_text();
        let q = text.trim().to_string();
        let action = if mods == 0 { None } else {
            match self.config.keys.action_of(mods) {
                Some(a) => Some(a),
                // 決めていない組み合わせは何もしない
                None => return,
            }
        };
        match action {
            // 改行して書く窓にする(K22). RichEditを作れなかったときは, 前の版と同じくすぐChatGPTに送る
            Some(KeyAction::Write) if self.compose.is_some() => {
                if q.is_empty() {
                    let d = self.drafts.draft.clone();
                    self.enter_compose(d, None);
                } else {
                    self.keep_old_draft();
                    self.enter_compose(format!("{}\r", text.trim_end()), None);
                }
                return;
            }
            Some(KeyAction::Write) | Some(KeyAction::ChatGpt) => {
                if q.is_empty() {
                    return;
                }
                let hwnd = self.hwnd;
                match actions::ask_chatgpt(hwnd, &q, &self.config) {
                    Ok(()) => self.finish(None),
                    Err(e) => self.fail(e),
                }
                return;
            }
            Some(KeyAction::Google) => {
                if q.is_empty() {
                    return;
                }
                match actions::google(&q, &self.config) {
                    Ok(()) => self.finish(None),
                    Err(e) => self.fail(e),
                }
                return;
            }
            Some(KeyAction::Terminal) => {
                self.on_alt_enter();
                return;
            }
            Some(KeyAction::Admin) | None => {}
        }
        let admin = action == Some(KeyAction::Admin);
        self.wait_pending();
        if !self.wait_ext() {
            return;
        }
        // 最近のものは, ↓かクリックで選ぶまで開かない
        if self.nothing_selected() {
            return;
        }
        let Some(row) = self.rows.get(self.sel).cloned() else { return };
        self.run_row(&row, admin);
    }

    fn run_row(&mut self, row: &Row, admin: bool) {
        let cfg = self.config.clone();
        let hwnd = self.hwnd;
        match &row.action {
            // 履歴の1件: 貼る
            Action::Clip(id) => {
                self.paste_clip(*id);
                return;
            }
            // 使い方の例: 入力欄に入れる(モードはそのまま)
            Action::Fill(t) => {
                let t = t.clone();
                self.set_edit_text(&t);
                self.refresh();
                return;
            }
            // スクリプト: 窓は閉じずに動かして, 出力を行にする
            Action::ScriptRun { script, arg } => {
                self.start_script(*script, arg.clone());
                return;
            }
            // `Running…`などの, 何もしない行
            Action::Copy(s) if s.is_empty() => return,
            _ => {}
        }
        let r: Result<(), String> = match &row.action {
            Action::Item { source, index } => {
                let Some(it) = self.item(*source, *index) else { return };
                match it.kind {
                    Kind::App => {
                        if admin {
                            actions::run_as_admin(&it.target, "")
                        } else if let Some(h) = self.running_window(&it) {
                            // 起動中なら窓を前に出す(C3)
                            actions::switch_to(h);
                            Ok(())
                        } else {
                            actions::launch_app(&it.target)
                        }
                    }
                    Kind::Bookmark => actions::open_url_with(&it.target, &cfg, actions::Browser::from_key(&it.args)),
                    Kind::Process => {
                        let pids: Vec<u32> = it.target.split(',').filter_map(|p| p.parse().ok()).collect();
                        let r = actions::kill(&pids);
                        self.processes_fresh = false;
                        r
                    }
                    Kind::Setting => {
                        if admin && !it.target.contains(':') {
                            actions::run_as_admin(&it.target, &it.args)
                        } else {
                            actions::open_setting(&it.target, &it.args)
                        }
                    }
                    Kind::System => {
                        let Some(a) = SysAction::from_id(&it.target) else { return };
                        if a.needs_confirm() && self.confirm.as_deref() != Some(a.id()) {
                            // 1回目のEnter: 確かめる文を出す(K7)
                            self.confirm = Some(a.id().to_string());
                            unsafe {
                                let _ = InvalidateRect(Some(self.hwnd), None, false);
                            }
                            return;
                        }
                        self.confirm = None;
                        // 先に窓を隠してから(ロックやスリープの前に)
                        let key = row.learn_key.clone();
                        self.finish(key);
                        if a == SysAction::HikidashiSettings {
                            self.open_settings();
                            return;
                        }
                        if let Err(e) = actions::system(a, hwnd) {
                            storage::log(&e);
                        }
                        return;
                    }
                    Kind::Command => {
                        if it.args == "script" {
                            self.start_script(it.data as usize, String::new());
                            return;
                        }
                        if it.args == "url" {
                            actions::open_url(&it.target.replace("{q}", ""), &cfg)
                        } else {
                            actions::run_command(&it.target.replace("{q}", ""))
                        }
                    }
                    Kind::File | Kind::Folder => actions::open_path(&it.target, &cfg, Some(it.kind == Kind::Folder)),
                    // 窓の一覧は起動中のアプリを前に出すのにだけ使う(K24. 窓の行は出さない)
                    Kind::Window => Ok(()),
                }
            }
            Action::File { .. } | Action::Path(_) => {
                let Some(p) = self.row_path(row) else { return };
                if !Path::new(&p).exists() && !actions::is_wsl_path(&p) {
                    self.fail(format!("Not found: {p}"));
                    return;
                }
                let is_dir = match &row.action {
                    Action::File { table, id } => self.tables.get(*table).map(|t| t.is_dir(*id)),
                    _ => None,
                };
                if admin {
                    actions::run_as_admin(&p, "")
                } else {
                    actions::open_path(&p, &cfg, is_dir)
                }
            }
            // BoxのURL: Box Driveの手元のファイルに引けたら既定のアプリ(Excelなど)で開く(K18). 引けなければブラウザー
            Action::BoxUrl { local: Some(p), folder, .. } => {
                if admin {
                    actions::run_as_admin(p, "")
                } else {
                    actions::open_path(p, &cfg, Some(*folder))
                }
            }
            Action::BoxUrl { url, .. } => actions::open_url(url, &cfg),
            Action::Copy(s) | Action::ScriptLine(s) => actions::copy_text(hwnd, s),
            Action::Clip(_) | Action::ScriptRun { .. } | Action::Fill(_) => Ok(()),
            Action::ChatGpt(q) => actions::ask_chatgpt(hwnd, q, &cfg),
            Action::Google(q) => actions::google(q, &cfg),
            Action::OpenUrl(u) => actions::open_url(u, &cfg),
            Action::Run(c) => actions::run_command(c),
        };
        match r {
            Ok(()) => self.finish(row.learn_key.clone()),
            Err(e) => self.fail(e),
        }
    }

    /// アプリがすでに動いていれば, その窓(C3). exeのパスで比べる
    fn running_window(&self, it: &Item) -> Option<isize> {
        let exe = it.args.to_lowercase();
        if exe.is_empty() || exe == "store" || !exe.ends_with(".exe") {
            return None;
        }
        // エクスプローラーなどは常に動いているので, 新しく開く
        if exe.ends_with("\\explorer.exe") || exe.ends_with("\\cmd.exe") || exe.ends_with("\\wezterm-gui.exe") {
            return None;
        }
        self.windows.items.iter().find(|w| w.args.to_lowercase() == exe).map(|w| w.data as isize)
    }

    // ---- Tabのメニュー(D7, C2, C3, K8) ----

    fn open_menu(&mut self) {
        self.wait_pending();
        if !self.wait_ext() {
            return;
        }
        if self.nothing_selected() {
            return;
        }
        let Some(row) = self.rows.get(self.sel).cloned() else { return };
        use icons::glyph as g;
        let mi = |label: &str, glyph: char, act: MenuAct| MenuItem { label: label.into(), glyph, act };
        let mut items = Vec::new();
        let path = self.row_path(&row);
        match &row.action {
            // 拡張の行: Copy, urlがあれば Open in browser と Copy URL
            _ if self.ext.view => {
                if let Some(c) = self.ext.copies.get(self.sel).filter(|c| !c.is_empty()) {
                    items.push(mi("Copy", g::COPY, MenuAct::CopyText(c.clone())));
                }
                if let Action::OpenUrl(u) = &row.action {
                    items.push(mi("Open in browser", g::GLOBE, MenuAct::Open));
                    items.push(mi("Copy URL", g::COPY, MenuAct::CopyText(u.clone())));
                }
            }
            Action::File { .. } | Action::Path(_) | Action::BoxUrl { local: Some(_), .. } => {
                let is_dir = self.row_is_dir(&row);
                // 出すものは設定の`[menu]`で種類ごとに選ぶ(K24). Copy WSL pathはWSLが入っているときだけ
                let kind = if is_dir { "folder" } else { "file" };
                let on = |item: &str| self.config.menu.has(kind, item) && config::menu_item_fits(kind, item);
                // ファイルそのものをコピー(Explorerでコピーしたのと同じ形). 先頭に出す
                if on("copy_file") && path.is_some() {
                    items.push(mi("Copy file", g::COPY, MenuAct::CopyFile));
                }
                // 「プログラムから開く」(Explorerの右クリックと同じ一覧). ファイルだけ
                if on("choose_app") && path.is_some() {
                    items.push(mi("Open with…", g::OPEN, MenuAct::OpenWithList));
                }
                if let Action::BoxUrl { url, .. } = &row.action {
                    items.push(mi("Open in browser", g::GLOBE, MenuAct::Open));
                    items.push(mi("Copy URL", g::COPY, MenuAct::CopyText(url.clone())));
                }
                if on("explorer") {
                    items.push(mi("Open in Explorer", g::FOLDER, MenuAct::Explorer));
                }
                // 入っているものだけ(Q5). ターミナルは`Alt+Enter`と同じ(Q6)
                let term = actions::terminal(&self.config).map(|t| t.name);
                let open_with = if on("open_with") { actions::installed_open_with(&self.config) } else { vec![] };
                for ow in open_with {
                    let kind = hikidashi_core::openwith::ToolKind::of(&ow);
                    let glyph = if kind == hikidashi_core::openwith::ToolKind::VsCode { g::CODE } else if kind.is_terminal() { g::COMMAND } else { g::OPEN };
                    let key = self.config.keys.label(KeyAction::Terminal);
                    let label = if term.as_deref() == Some(ow.name.as_str()) && !key.is_empty() { format!("Open in {}  ({key})", ow.name) } else { format!("Open in {}", ow.name) };
                    items.push(MenuItem { label, glyph, act: MenuAct::OpenWith(ow) });
                }
                if on("reveal") {
                    items.push(mi(if is_dir { "Open parent folder" } else { "Open folder" }, g::FOLDER, MenuAct::Reveal));
                }
                if on("copy_path") {
                    items.push(mi("Copy path", g::COPY, MenuAct::CopyPath));
                }
                if on("copy_wsl_path") && system::wsl_installed() {
                    items.push(mi("Copy WSL path", g::COPY, MenuAct::CopyWslPath));
                }
                if on("admin") {
                    items.push(mi("Run as administrator", g::ADMIN, MenuAct::Admin));
                }
                if on("share") && path.is_some() {
                    items.push(mi("Share…", g::LINK, MenuAct::Share));
                }
                // Outlookが見つからないときは出さない
                if on("mail") && path.is_some() && shellmenu::outlook_exe().is_some() {
                    items.push(mi("Attach to new mail", g::FILE, MenuAct::Mail));
                }
            }
            Action::Item { source, index } => {
                let Some(it) = self.item(*source, *index) else { return };
                match it.kind {
                    Kind::App => {
                        let on = |item: &str| self.config.menu.has("app", item);
                        if on("new_window") {
                            items.push(mi("Open new window", g::OPEN, MenuAct::NewInstance));
                        }
                        if path.is_some() && on("reveal") {
                            items.push(mi("Open file location", g::FOLDER, MenuAct::Reveal));
                        }
                        if path.is_some() && on("copy_path") {
                            items.push(mi("Copy path", g::COPY, MenuAct::CopyPath));
                        }
                        if on("admin") {
                            items.push(mi("Run as administrator", g::ADMIN, MenuAct::Admin));
                        }
                    }
                    Kind::Bookmark => {
                        items.push(mi("Open", g::GLOBE, MenuAct::Open));
                        items.push(mi("Copy URL", g::COPY, MenuAct::CopyText(it.target.clone())));
                    }
                    Kind::Process => {
                        items.push(mi("Kill", g::KILL, MenuAct::Kill));
                        if path.is_some() {
                            items.push(mi("Open file location", g::FOLDER, MenuAct::Reveal));
                            items.push(mi("Copy path", g::COPY, MenuAct::CopyPath));
                        }
                    }
                    Kind::Setting => {
                        items.push(mi("Open", g::SETTING, MenuAct::Open));
                        if !it.target.contains(':') {
                            items.push(mi("Run as administrator", g::ADMIN, MenuAct::Admin));
                        }
                    }
                    _ => {}
                }
            }
            Action::ChatGpt(q) | Action::Google(q) => {
                items.push(mi("Ask ChatGPT", g::CHAT, MenuAct::ChatGpt));
                items.push(mi("Search Google", g::GLOBE, MenuAct::Google));
                items.push(mi("Copy text", g::COPY, MenuAct::CopyText(q.clone())));
            }
            Action::Copy(s) => {
                if !s.is_empty() {
                    items.push(mi("Copy", g::COPY, MenuAct::CopyText(s.clone())));
                }
            }
            Action::ScriptLine(l) => {
                items.push(mi("Copy line", g::COPY, MenuAct::CopyText(l.clone())));
                items.push(mi("Copy all", g::COPY, MenuAct::CopyText(self.script_all.clone())));
            }
            Action::Clip(id) => {
                items.push(mi("Copy", g::COPY, MenuAct::ClipCopy(*id)));
                items.push(mi("Delete from history", g::KILL, MenuAct::ClipDelete(*id)));
            }
            Action::ScriptRun { .. } | Action::Fill(_) => {}
            Action::BoxUrl { url, .. } => {
                items.push(mi("Open in browser", g::GLOBE, MenuAct::Open));
                items.push(mi("Copy URL", g::COPY, MenuAct::CopyText(url.clone())));
            }
            Action::OpenUrl(u) => {
                items.push(mi("Open in browser", g::GLOBE, MenuAct::Open));
                items.push(mi("Copy URL", g::COPY, MenuAct::CopyText(u.clone())));
            }
            Action::Run(_) => {}
        }
        if items.is_empty() {
            return;
        }
        self.menu = Some(Menu { title: format!("Actions: {}", row.name), items, sel: 0, row });
        self.layout();
    }

    fn run_menu(&mut self, row: &Row, act: &MenuAct) {
        let _d = diag::doing("Tab menu: run");
        let cfg = self.config.clone();
        let hwnd = self.hwnd;
        // 履歴から消すだけは, 窓を閉じずに一覧を作り直す
        if let MenuAct::ClipDelete(id) = act {
            self.clips.remove(*id);
            self.refresh();
            return;
        }
        let path = self.row_path(row);
        // 一覧に替えるだけは, 窓を閉じない
        if let MenuAct::OpenWithList = act {
            self.show_open_with_list(row, path);
            return;
        }
        let item = match &row.action {
            Action::Item { source, index } => self.item(*source, *index),
            _ => None,
        };
        let r: Result<(), String> = match act {
            MenuAct::OpenWithList => Ok(()),
            MenuAct::AssocInvoke(i) => match (self.assoc.get(*i), &path) {
                (Some(h), Some(p)) => {
                    storage::log(&format!("open with {}: {p}", h.name));
                    shellmenu::invoke_handler(&h.handler, p)
                }
                _ => Ok(()),
            },
            MenuAct::ChooseApp => match &path {
                Some(p) => shellmenu::choose_another_app(hwnd, p),
                None => Ok(()),
            },
            MenuAct::Share => match &path {
                Some(p) => shellmenu::share(hwnd, p),
                None => Ok(()),
            },
            MenuAct::Mail => match &path {
                Some(p) => shellmenu::attach_to_new_mail(p),
                None => Ok(()),
            },
            MenuAct::Open => match (&row.action, &item) {
                (Action::BoxUrl { url, .. }, _) | (Action::OpenUrl(url), _) => actions::open_url(url, &cfg),
                (_, Some(it)) if it.kind == Kind::Bookmark => actions::open_url_with(&it.target, &cfg, actions::Browser::from_key(&it.args)),
                (_, Some(it)) if it.kind == Kind::Setting => actions::open_setting(&it.target, &it.args),
                _ => Ok(()),
            },
            MenuAct::NewInstance => match &item {
                Some(it) => actions::launch_app(&it.target),
                None => Ok(()),
            },
            MenuAct::Reveal => match (&item, &path) {
                (Some(it), _) if it.kind == Kind::App && !it.args.is_empty() && it.args != "store" => actions::reveal(&it.args),
                (_, Some(p)) => actions::reveal(p),
                _ => Ok(()),
            },
            MenuAct::CopyPath => match &path {
                Some(p) => actions::copy_text(hwnd, p),
                None => Ok(()),
            },
            MenuAct::CopyWslPath => match &path {
                Some(p) => actions::copy_text(hwnd, &pathlike::to_wsl(p).unwrap_or_else(|| p.clone())),
                None => Ok(()),
            },
            MenuAct::Explorer => match &path {
                Some(p) => actions::open_path(p, &cfg, Some(true)),
                None => Ok(()),
            },
            MenuAct::OpenWith(ow) => match &path {
                Some(p) => actions::open_with(ow, p, self.row_is_dir(row), false),
                None => Ok(()),
            },
            MenuAct::Admin => match (&item, &path) {
                (Some(it), _) if it.kind == Kind::Setting => actions::run_as_admin(&it.target, &it.args),
                (Some(it), _) if it.kind == Kind::App => actions::run_as_admin(&it.target, ""),
                (_, Some(p)) => actions::run_as_admin(p, ""),
                _ => Ok(()),
            },
            MenuAct::CopyText(s) => actions::copy_text(hwnd, s),
            MenuAct::CopyFile => match &path {
                Some(p) => clipboard::set_files(hwnd, std::slice::from_ref(p)),
                None => Ok(()),
            },
            MenuAct::ClipCopy(id) => match self.clips.get(*id).cloned() {
                Some(e) => clipboard::set_kind(hwnd, &e.kind),
                None => Ok(()),
            },
            MenuAct::ClipDelete(_) => Ok(()),
            MenuAct::Kill => {
                let r = match &item {
                    Some(it) => actions::kill(&it.target.split(',').filter_map(|p| p.parse().ok()).collect::<Vec<u32>>()),
                    None => Ok(()),
                };
                self.processes_fresh = false;
                r
            }
            MenuAct::ChatGpt => actions::ask_chatgpt(hwnd, self.edit_text().trim(), &cfg),
            MenuAct::Google => actions::google(self.edit_text().trim(), &cfg),
        };
        match r {
            Ok(()) => self.finish(row.learn_key.clone()),
            Err(e) => self.fail(e),
        }
    }

    /// メニューの中身を, そのファイルの拡張子に関連付いたアプリの一覧にする.
    /// 既定のアプリが先頭, おすすめが続き, 最後に`Choose another app…`
    fn show_open_with_list(&mut self, row: &Row, path: Option<String>) {
        use icons::glyph as g;
        let Some(p) = path else { return };
        let list = match shellmenu::open_with_handlers(&p) {
            Ok(l) => l,
            Err(e) => {
                self.fail(e);
                return;
            }
        };
        let mut items: Vec<MenuItem> = list.iter().enumerate().map(|(i, h)| MenuItem { label: h.name.clone(), glyph: g::OPEN, act: MenuAct::AssocInvoke(i) }).collect();
        items.push(MenuItem { label: "Choose another app…".into(), glyph: g::SETTING, act: MenuAct::ChooseApp });
        self.assoc = list;
        self.menu = Some(Menu { title: format!("Open with: {}", row.name), items, sel: 0, row: row.clone() });
        self.layout();
    }

    /// 設定画面(K16)
    fn settings_args(&self) -> settings::OpenArgs {
        let mut apps: Vec<String> = self.apps.items.iter().map(|i| i.name.clone()).collect();
        apps.sort_by_key(|a| a.to_lowercase());
        apps.dedup();
        settings::OpenArgs {
            config: self.config.clone(),
            error: self.config_error.clone(),
            hotkey: self.hotkey.map(|h| h.format()).unwrap_or_default(),
            exe: self.exe_path.clone(),
            apps,
        }
    }

    fn open_settings(&mut self) {
        settings::open(self.settings_args());
    }

    /// `docs\manual.html`を既定のブラウザーで開く. Webのマニュアルは見にいかない(K46)
    fn open_manual(&mut self) {
        let Some(path) = storage::manual_path() else {
            system::notice("The manual was not found. The docs folder (docs\\manual.html) that comes with Hikidashi.exe is needed. If you moved only the exe, move the docs folder next to it.");
            return;
        };
        if let Err(e) = actions::shell("open", &path.to_string_lossy(), "", "", false) {
            system::notice(&format!("Cannot open the manual: {e}"));
        }
    }

    fn tray_menu(&mut self) {
        unsafe {
            let Ok(m) = CreatePopupMenu() else { return };
            // 文字は設定の言語に合わせる(K24)
            let add = |id: usize, text: &str, flags: MENU_ITEM_FLAGS| {
                let t = WStr::new(&i18n::tr(text));
                let _ = AppendMenuW(m, flags, id, if text.is_empty() { PCWSTR::null() } else { t.pcwstr() });
            };
            add(1, "Show Hikidashi", MF_STRING);
            add(2, "Change hotkey…", MF_STRING);
            add(3, "Start with Windows", MF_STRING | if system::autostart_enabled() { MF_CHECKED } else { MF_UNCHECKED });
            add(4, "Rebuild index", MF_STRING);
            add(5, "Settings", MF_STRING);
            add(7, "Manual", MF_STRING);
            add(0, "", MF_SEPARATOR);
            add(6, "Quit", MF_STRING);
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);
            let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), self.hwnd, None);
            let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(m);
            match cmd.0 {
                1 => self.show(),
                2 => self.start_capture(false),
                3 => system::set_autostart(!system::autostart_enabled(), &self.exe_path),
                4 => {
                    self.rebuild_wait = Some(self.shared.user_scans.load(std::sync::atomic::Ordering::Relaxed));
                    self.shared.rebuild.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                5 => self.open_settings(),
                7 => self.open_manual(),
                6 => {
                    self.shared.quit.store(true, std::sync::atomic::Ordering::Relaxed);
                    system::tray_remove(self.hwnd);
                    PostQuitMessage(0);
                }
                _ => {}
            }
        }
    }
}

impl App {
    // ---- 選んだ文字で開く, --query ----

    /// 選んだ文字で開くホットキーを登録しなおす(設定を読み直したとき, 登録の画面を終えたとき). 設定の書き間違いは`config_error`に出す
    fn apply_selection_hotkey(&mut self) {
        if self.sel_registered {
            system::unregister_hotkey_id(self.hwnd, system::HOTKEY_SELECTION_ID);
            self.sel_registered = false;
        }
        self.sel_hotkey = None;
        let text = self.config.hotkeys.selection.trim().to_string();
        if text.is_empty() || self.capture {
            return;
        }
        let Some(h) = Hotkey::parse(&text).filter(|h| h.is_usable()) else {
            self.config_error.get_or_insert(format!("config.toml: selection hotkey \"{text}\" is not a hotkey. Write it like Ctrl+Alt+Space"));
            return;
        };
        if self.hotkey == Some(h) {
            self.config_error.get_or_insert(format!("config.toml: selection hotkey {} is the same as the hotkey. Change one of them", h.format()));
            return;
        }
        if system::register_hotkey_id(self.hwnd, system::HOTKEY_SELECTION_ID, h) {
            self.sel_registered = true;
            self.sel_hotkey = Some(h);
        } else {
            self.config_error.get_or_insert(format!("Selection hotkey {} is used by another app. Change it in Settings", h.format()));
        }
    }

    /// ホットキーの登録の画面で, 選んだ文字で開くホットキーが押された
    fn capture_selection_key(&mut self, h: Hotkey) -> bool {
        if self.hotkey == Some(h) {
            self.message = Some((format!("{} is already the hotkey that opens Hikidashi. Try another key.", h.format()), true));
            self.layout();
            return true;
        }
        // 先に使えるか試す(登録の画面の間は, 自分のホットキーはどちらも外してある)
        if !system::register_hotkey_id(self.hwnd, system::HOTKEY_SELECTION_ID, h) {
            self.message = Some((format!("{} is used by another app. Try another key.", h.format()), true));
            self.layout();
            return true;
        }
        system::unregister_hotkey_id(self.hwnd, system::HOTKEY_SELECTION_ID);
        self.set_selection_hotkey(Some(h));
        self.message = Some((format!("Selection hotkey set: {}", h.format()), false));
        self.layout();
        true
    }

    /// 選んだ文字で開くホットキーを設定ファイルに書いて, 登録しなおす. `None`なら使わない
    fn set_selection_hotkey(&mut self, h: Option<Hotkey>) {
        let text = h.map(|h| h.format()).unwrap_or_default();
        // 今のファイルを読み直して, ホットキーだけ変えて書く. 書き間違いのあるファイルは, 初期値で置き換えないよう書かない
        let (mut cfg, err) = storage::load_config();
        if let Some(e) = err {
            self.fail(format!("Fix config.toml first: {}", e.0));
            return;
        }
        cfg.hotkeys.selection = text.clone();
        if let Err(e) = storage::write_config(&config::to_toml(&cfg)) {
            self.fail(format!("Cannot save config.toml: {e}"));
            return;
        }
        self.config.hotkeys.selection = text.clone();
        settings::set_selection_hotkey(&text);
        if self.capture {
            self.end_capture(true);
        } else {
            self.apply_selection_hotkey();
        }
    }

    /// 選んだ文字で開くホットキー(2つ目)が押された. 前面のアプリの選択を読んで, その文字で窓を出す
    fn on_selection_hotkey(&mut self) {
        if self.capture {
            return;
        }
        // 窓が出ているときは, いったん隠して前のアプリに戻ってから読む(K50).
        // 修飾キーを離すまでは隠さない. 隠したあとに離すと, Altが前のアプリに届いてメニューに入り, Ctrl+Cが効かない(確認15 N1)
        if self.visible() {
            self.selection_wait = Some(Instant::now());
            unsafe {
                SetTimer(Some(self.hwnd), TIMER_SELECTION, 20, None);
            }
            return;
        }
        self.stats.hotkey_at = Some(Instant::now());
        let _d = diag::doing("selection: copy the selected text");
        let (text, ignore) = selection::capture(self.hwnd);
        if ignore.is_some() {
            self.clip_ignore = ignore;
        }
        let picked = text.map(|t| csel::classify(&t)).unwrap_or(Picked::Empty);
        let what = match &picked {
            Picked::Empty => "no text",
            Picked::Line(_) => "one line",
            Picked::Multi(_) => "several lines",
        };
        storage::log(&format!("selection hotkey: {what}"));
        self.show_with(picked);
    }

    /// 窓が出ているときに押された選んだ文字のホットキーの続き.
    /// 修飾キーが離れたら(最大1.5秒), 前のアプリを前面に戻して窓を隠し, 0.15秒後に読む
    fn on_selection_timer(&mut self) {
        if let Some(t) = self.selection_wait {
            if selection::modifiers_held() && t.elapsed().as_millis() < 1500 {
                return;
            }
            self.selection_wait = None;
            if !self.visible() {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), TIMER_SELECTION);
                }
                return;
            }
            unsafe {
                let prev = HWND(self.prev_fg as *mut _);
                if self.prev_fg != 0 && IsWindow(Some(prev)).as_bool() && IsWindowVisible(prev).as_bool() {
                    let _ = SetForegroundWindow(prev);
                }
            }
            self.hide();
            unsafe {
                SetTimer(Some(self.hwnd), TIMER_SELECTION, 150, None);
            }
            return;
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_SELECTION);
        }
        if !self.visible() {
            self.on_selection_hotkey();
        }
    }

    /// `--query`で別のHikidashiから渡された文字
    fn on_query(&mut self, text: &str) {
        if self.capture {
            return;
        }
        self.show_with(csel::classify(text));
    }

    /// 文字を入れて窓を出す. 1行なら入力欄(カーソルは末尾), 2行以上なら長い質問を書く窓, 空なら空の窓
    fn show_with(&mut self, picked: Picked) {
        self.mode = Mode::All;
        match picked {
            Picked::Multi(t) if self.compose.is_some() => {
                self.keep_old_draft();
                self.enter_compose(t, None);
                self.show_inner(false);
            }
            // 書く窓を作れなかったときは, 1行にして入力欄に入れる
            Picked::Multi(t) => {
                let one = t.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
                self.show_text(&one);
            }
            Picked::Line(t) => self.show_text(&t),
            Picked::Empty => self.show_text(""),
        }
    }

    fn show_text(&mut self, t: &str) {
        if self.composing() && !t.is_empty() {
            self.leave_compose();
        }
        if !self.composing() {
            self.set_edit_text(t);
        }
        self.show_inner(false);
    }

    // ---- クリップボードの履歴 ----

    /// 設定に合わせて, クリップボードの変化の監視を入れ切りする. 切ったら記録も消す
    fn sync_clip_listener(&mut self) {
        let on = self.config.search.clipboard_history;
        if on && !self.clip_listening {
            self.clip_listening = unsafe { AddClipboardFormatListener(self.hwnd) }.is_ok();
            if !self.clip_listening {
                storage::log("clipboard history: could not listen to the clipboard");
            }
        } else if !on && self.clip_listening {
            unsafe {
                let _ = RemoveClipboardFormatListener(self.hwnd);
            }
            self.clip_listening = false;
        }
        if !on {
            self.clips.clear();
        }
    }

    /// クリップボードが変わった(`WM_CLIPBOARDUPDATE`)
    fn on_clipboard_update(&mut self) {
        if !self.config.search.clipboard_history {
            return;
        }
        // 選んだ文字の取り込みで一時的に変えたもの(と, 戻したもの)は残さない
        if self.clip_ignore == Some(clipboard::sequence()) {
            return;
        }
        let _d = diag::doing("clipboard history: read");
        if let Some(k) = clipboard::read_for_history(self.hwnd) {
            self.clips.add(k, util::now_secs());
        }
    }

    /// 履歴の1件を貼る: 窓を閉じ, クリップボードに入れ, 前のアプリが前面に戻ってからShift+Insertを送る
    fn paste_clip(&mut self, id: u64) {
        let Some(e) = self.clips.get(id).cloned() else { return };
        // 消えたファイルは何もしない
        if let ClipKind::Files(f) = &e.kind {
            if f.iter().any(|p| !p.starts_with("\\\\") && !Path::new(p).exists()) {
                return;
            }
        }
        if let Err(msg) = clipboard::set_kind(self.hwnd, &e.kind) {
            self.fail(msg);
            return;
        }
        self.finish(None);
        selection::paste_when_back(self.hwnd.0 as isize);
    }

    // ---- スクリプトのフォルダー ----

    /// 自作コマンドの一覧(設定の`[[command]]`と, `run`のスクリプト)を作り直す
    fn rebuild_commands(&mut self) {
        let mut items = router::command_items(&self.config);
        items.extend(cscripts::run_items(&self.scripts));
        self.commands = Catalog::new(items);
    }

    /// フォルダーの中身が変わっていたら読み直す(窓を出すたびに見る)
    fn check_scripts(&mut self) {
        let sig = scripts::signature();
        if sig == self.scripts_sig {
            return;
        }
        self.scripts_sig = sig;
        self.scripts = scripts::load();
        storage::log(&format!("scripts: {} loaded", self.scripts.len()));
        let paths: Vec<String> = self.scripts.iter().map(|s| s.path.clone()).collect();
        self.lists.lock().unwrap().retain(|k, _| paths.contains(k));
        self.rebuild_commands();
    }

    /// `run`のスクリプトを裏で動かす. 窓は閉じず, 実行中は`Running…`の行を出す
    fn start_script(&mut self, index: usize, arg: String) {
        let Some(s) = self.scripts.get(index).cloned() else { return };
        self.script_job += 1;
        let job = self.script_job;
        self.script_view = true;
        self.script_all.clear();
        self.menu = None;
        self.rows = vec![cscripts::running_row(&s.title)];
        self.sel = 0;
        self.top = 0;
        self.layout();
        let (slot, h) = (self.script_result.clone(), self.hwnd.0 as isize);
        let _ = std::thread::Builder::new().name("script-run".into()).spawn(move || {
            let o = scripts::run(&s, &arg);
            *slot.lock().unwrap() = Some((job, o));
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), scripts::WM_APP_SCRIPT, WPARAM(0), LPARAM(0));
            }
        });
    }

    /// スクリプトが終わった. まだその結果を待っていれば, 出力を行にして出す
    fn on_script_done(&mut self) {
        let Some((job, o)) = self.script_result.lock().unwrap().take() else { return };
        if job != self.script_job || !self.script_view || !self.visible() {
            return;
        }
        let title = self.rows.first().map(|r| r.sub.clone()).unwrap_or_default();
        let (rows, all) = cscripts::outcome_rows(&title, &o);
        self.script_all = all;
        self.set_rows(rows, vec![]);
    }

    /// 拡張のkeywordにできない名前(接頭辞とスクリプトのkeyword. 小文字)
    fn ext_taken(&self) -> Vec<String> {
        let mut v: Vec<String> = self.config.prefixes().into_iter().map(|(p, _)| p.trim().to_lowercase()).collect();
        v.extend(self.scripts.iter().filter_map(|s| s.keyword.as_ref().map(|k| k.to_lowercase())));
        v
    }

    /// 窓を出すたびに拡張を読み直す
    fn reload_ext(&mut self) {
        let taken = self.ext_taken();
        self.ext.reload(&taken);
    }

    /// `keyword `で始まる入力なら拡張の行(結果が届くまでは前の行か`Loading…`)
    fn ext_rows(&mut self, text: &str) -> Option<Vec<Row>> {
        let taken = self.ext_taken();
        let (hwnd, gen) = (self.hwnd, self.gen);
        let calc = self.mode == Mode::Calc;
        self.ext.rows(text, &taken, calc, hwnd, gen, &self.rows)
    }

    /// 拡張の結果が届いた. 今の入力の結果なら出す
    fn on_ext(&mut self) {
        if !self.visible() || self.composing() || !matches!(self.mode, Mode::All | Mode::Calc) {
            return;
        }
        if let Some(rows) = self.ext.take(self.gen) {
            self.set_rows(rows, vec![]);
        }
    }

    /// 打ってすぐEnterを押したとき, 拡張の結果がまだなら少し待つ. 届かなければfalse(何もしない)
    fn wait_ext(&mut self) -> bool {
        let Some(g) = self.ext.pending() else { return true };
        if g != self.gen {
            return false;
        }
        match self.ext.wait(g) {
            Some(rows) => {
                self.set_rows(rows, vec![]);
                true
            }
            None => false,
        }
    }

    /// `list`のスクリプトの結果がそろった. その一覧を見ているなら出し直す
    fn on_script_list(&mut self) {
        if self.visible() && !self.composing() && !self.script_view && self.menu.is_none() && self.mode == Mode::All {
            let text = self.edit_text();
            if cscripts::list_target(&self.scripts, &text).is_some() {
                self.refresh();
            }
        }
    }
}

const WM_APP_ASK_AUTOSTART: u32 = 0x8000 + 4;
/// 入力欄に入る長さ
const EDIT_LIMIT: usize = 4096;

fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // パニックがWindowsの呼び出しを越えると落ちるので, ここで止めて記録する
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wndproc_inner(hwnd, msg, wparam, lparam))) {
        Ok(r) => r,
        Err(_) => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn wndproc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // 止まっていないかの合図(K29). Appを借りていても返事する
    if msg == diag::WM_APP_PING {
        diag::pong(wparam.0 as u64);
        return LRESULT(0);
    }
    // 入力欄の色. 文字を入れ替えている途中(Appを借りている間)にも聞かれるので, 先に答える(白い地に黒い字で描かれないように. K43)
    if msg == WM_CTLCOLOREDIT {
        return LRESULT(paint::edit_colors(windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut _)).0 as isize);
    }
    // 書く窓の中身の高さ. 整えている途中(Appを借りている間)にも届くので, 先にここで受け取る
    let mut notify_code = 0u32;
    if msg == WM_NOTIFY && lparam.0 != 0 {
        let hdr = unsafe { std::ptr::read_unaligned(lparam.0 as *const NMHDR) };
        notify_code = hdr.code;
        if hdr.code == EN_REQUESTRESIZE {
            let rr = unsafe { std::ptr::read_unaligned(lparam.0 as *const REQRESIZE) };
            let rc = rr.rc;
            REQ_H.set(Some(rc.bottom - rc.top));
        }
    }
    let handled: Option<Option<LRESULT>> = with_app(|app| -> Option<LRESULT> {
        match msg {
            WM_PAINT => {
                paint::paint(app);
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_HOTKEY => {
                storage::log(&format!("hotkey pressed (visible: {})", app.visible()));
                if wparam.0 as i32 == system::HOTKEY_ID {
                    app.stats.hotkey_at = Some(Instant::now());
                    app.toggle();
                } else if wparam.0 as i32 == system::HOTKEY_SELECTION_ID {
                    app.on_selection_hotkey();
                }
                Some(LRESULT(0))
            }
            WM_CLIPBOARDUPDATE => {
                app.on_clipboard_update();
                Some(LRESULT(0))
            }
            WM_COPYDATA => {
                // `Hikidashi.exe --query "文字"`から. 文字(UTF-16)を入れて窓を出す
                let cds = lparam.0 as *const COPYDATASTRUCT;
                if cds.is_null() {
                    return None;
                }
                let (magic, len, ptr) = unsafe { ((*cds).dwData, (*cds).cbData as usize, (*cds).lpData as *const u16) };
                if magic != QUERY_MAGIC || ptr.is_null() {
                    return None;
                }
                let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len / 2) });
                app.on_query(&text);
                Some(LRESULT(1))
            }
            scripts::WM_APP_SCRIPT => {
                app.on_script_done();
                Some(LRESULT(0))
            }
            ext::WM_APP_EXT => {
                app.on_ext();
                Some(LRESULT(0))
            }
            scripts::WM_APP_SCRIPT_LIST => {
                app.on_script_list();
                Some(LRESULT(0))
            }
            WM_ACTIVATE => {
                // ほかの窓をクリックしたら隠す(A3)
                if loword(wparam.0) as u32 == WA_INACTIVE && app.visible() {
                    app.hide();
                }
                None
            }
            WM_NOTIFY => {
                if !app.composing() {
                    return None;
                }
                if notify_code == EN_REQUESTRESIZE {
                    if let Some(h) = REQ_H.take() {
                        if h != app.compose_h {
                            app.compose_h = h;
                            app.layout();
                        }
                    }
                } else if notify_code == EN_SELCHANGE {
                    app.schedule_restyle(false);
                }
                Some(LRESULT(0))
            }
            WM_APP_COMPOSE => {
                app.on_compose_posted();
                Some(LRESULT(0))
            }
            WM_APP_FAVICONS => {
                app.icons.clear_favicons();
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                Some(LRESULT(0))
            }
            actions::WM_APP_CLIP_RESTORE => {
                if lparam.0 != 0 {
                    let r = unsafe { Box::from_raw(lparam.0 as *mut actions::ClipRestore) };
                    app.restore_clipboard(*r);
                }
                Some(LRESULT(0))
            }
            WM_COMMAND => {
                if app.compose.as_ref().is_some_and(|c| HWND(lparam.0 as *mut _) == c.hwnd) && hiword(wparam.0) as u32 == EN_CHANGE {
                    if app.composing() {
                        app.compose_changed();
                    }
                    return Some(LRESULT(0));
                }
                if HWND(lparam.0 as *mut _) == app.edit && hiword(wparam.0) as u32 == EN_CHANGE {
                    if !app.capture {
                        app.on_text_changed();
                    }
                    return Some(LRESULT(0));
                }
                None
            }
            WM_MOUSEMOVE => {
                app.on_mouse_move(loword(lparam.0 as usize) as i16 as i32, hiword(lparam.0 as usize) as i16 as i32);
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP => {
                app.on_click(loword(lparam.0 as usize) as i16 as i32, hiword(lparam.0 as usize) as i16 as i32, msg == WM_LBUTTONDOWN);
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                let delta = hiword(wparam.0) as i16 as i32;
                let lines = if delta > 0 { -3 } else if delta < 0 { 3 } else { 0 };
                app.scroll(lines * (delta.abs() / 120).max(1));
                Some(LRESULT(0))
            }
            WM_TIMER => {
                match wparam.0 {
                    TIMER_GLOW => {
                        let done = app.glow.map_or(true, |g| g.elapsed().as_millis() > 700);
                        if done {
                            app.glow = None;
                            unsafe {
                                let _ = KillTimer(Some(hwnd), TIMER_GLOW);
                            }
                        }
                        paint::invalidate_line(app);
                    }
                    TIMER_QUICK => app.on_quick_timer(),
                    TIMER_SELECTION => app.on_selection_timer(),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            searcher::WM_APP_FILES => {
                app.on_files(wparam.0 as u64);
                Some(LRESULT(0))
            }
            indexer::WM_APP_INDEX => {
                if let Some(n) = app.rebuild_wait {
                    if app.shared.user_scans.load(std::sync::atomic::Ordering::Relaxed) > n {
                        app.rebuild_wait = None;
                        let files: usize = app.shared.snapshot().iter().map(|t| t.len()).sum();
                        app.message = Some((format!("Index rebuilt: {} files", group(files)), false));
                    }
                }
                if app.visible() {
                    app.layout();
                }
                Some(LRESULT(0))
            }
            icons::WM_APP_ICONS => {
                if app.icons.take_loaded() && app.visible() {
                    unsafe {
                        let _ = InvalidateRect(Some(app.hwnd), None, false);
                    }
                }
                Some(LRESULT(0))
            }
            indexer::WM_APP_APPS => {
                app.apply_apps();
                // アプリのアイコンは先に裏で読んでおく(K29)
                let paths: Vec<String> = app.apps.items.iter().map(|it| it.target.clone()).collect();
                app.icons.prefetch(paths.iter().map(String::as_str));
                if app.visible() && !app.script_view {
                    app.refresh();
                }
                Some(LRESULT(0))
            }
            indexer::WM_APP_CONFIG => {
                app.reload_config();
                Some(LRESULT(0))
            }
            system::WM_APP_TRAY => {
                match loword(lparam.0 as usize) as u32 {
                    WM_LBUTTONUP => app.show(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => app.tray_menu(),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_APP_SHOW => {
                if !app.visible() {
                    app.show();
                }
                Some(LRESULT(0))
            }
            WM_APP_QUIT => {
                app.shared.quit.store(true, std::sync::atomic::Ordering::Relaxed);
                system::tray_remove(hwnd);
                unsafe { PostQuitMessage(0) };
                Some(LRESULT(0))
            }
            WM_APP_ASK_AUTOSTART => {
                let on = system::message_box_yes_no(hwnd, "Start Hikidashi when Windows starts?\nYou can change this later from the tray icon.", "Hikidashi");
                system::set_autostart(on, &app.exe_path);
                Some(LRESULT(0))
            }
            WM_SETTINGCHANGE => {
                // Windowsのアプリのモード(ライト/ダーク)が変わった. 設定が「自動」なら追いかける(K39)
                let what = if lparam.0 != 0 { unsafe { PCWSTR(lparam.0 as *const u16).to_string().unwrap_or_default() } } else { String::new() };
                if what == "ImmersiveColorSet" {
                    theme::set_theme(&app.config.ui.theme);
                    app.sync_theme();
                }
                None
            }
            WM_DPICHANGED => {
                app.set_dpi(hiword(wparam.0) as u32);
                app.layout();
                Some(LRESULT(0))
            }
            m if m == app.taskbar_msg && m != 0 => {
                system::tray_add(hwnd);
                Some(LRESULT(0))
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                Some(LRESULT(0))
            }
            _ => None,
        }
    });
    if msg == WM_HOTKEY && handled.is_none() {
        // 画面の処理の途中(Appを借りている間)にホットキーが来た
        storage::log("hotkey pressed while busy");
    }
    match handled {
        Some(Some(r)) => r,
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

const CUE_SEARCH: &str = "Search apps, files, bookmarks, or paste a path";

/// 入力欄が空のときの案内(種類ごと. K51)
fn cue_for(m: Mode) -> &'static str {
    match m {
        Mode::All | Mode::ChatGpt => CUE_SEARCH,
        Mode::App => "Search apps",
        Mode::File => "Search files and folders",
        Mode::Bookmark => "Search bookmarks",
        Mode::Kill => "Choose a process to end",
        Mode::Wsl => "Search your WSL home",
        Mode::Box => "Search Box",
        Mode::Google => "Search Google",
        Mode::Clipboard => "Search clipboard history",
        Mode::Calc => "Calculate: 0x1F, 10dBm, (12+3)*4, mipi",
    }
}

thread_local! {
    /// 入力欄が空のときの案内の文字. 標準の案内はダークで色が暗く, 欠けることもあったので自分で描く(K43)
    static CUE: std::cell::Cell<&'static str> = const { std::cell::Cell::new(CUE_SEARCH) };
    /// 前に描いたときに案内を出していたか
    static CUE_SHOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// 日本語入力で変換している間は案内を出さない
    static EDIT_IME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn set_cue(edit: HWND, text: &'static str) {
    CUE.set(text);
    unsafe {
        let _ = InvalidateRect(Some(edit), None, true);
    }
}

/// 入力欄が空で, 案内の文字を出すか
fn cue_wanted(edit: HWND) -> bool {
    !EDIT_IME.get() && unsafe { DefSubclassProc(edit, WM_GETTEXTLENGTH, WPARAM(0), LPARAM(0)) }.0 == 0
}

fn draw_cue(edit: HWND) {
    use windows::Win32::Graphics::Gdi::*;
    unsafe {
        let mut rc = RECT::default();
        SendMessageW(edit, EM_GETRECT, None, Some(LPARAM(&mut rc as *mut _ as isize)));
        let _ = HideCaret(Some(edit));
        let hdc = GetDC(Some(edit));
        let font = HFONT(SendMessageW(edit, WM_GETFONT, None, None).0 as *mut _);
        let old = SelectObject(hdc, font.into());
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, util::rgb(theme::pal().text_tertiary));
        let mut t: Vec<u16> = CUE.get().encode_utf16().collect();
        DrawTextW(hdc, &mut t, &mut rc, DT_SINGLELINE | DT_TOP | DT_LEFT | DT_NOPREFIX | DT_END_ELLIPSIS);
        SelectObject(hdc, old);
        ReleaseDC(Some(edit), hdc);
        let _ = ShowCaret(Some(edit));
    }
}

/// 入力欄のキーを横取りする
extern "system" fn edit_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, id: usize, data: usize) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| edit_proc_inner(hwnd, msg, wparam, lparam, id, data))) {
        Ok(r) => r,
        Err(_) => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

fn edit_proc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_PAINT => {
            let cue = cue_wanted(hwnd);
            // 案内を出す(出していた)ときは全体を描き直す. 一部だけ描くと欠けや重ね描きになる
            if cue || CUE_SHOWN.get() {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
            let r = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            if cue {
                draw_cue(hwnd);
            }
            CUE_SHOWN.set(cue);
            return r;
        }
        WM_IME_ENDCOMPOSITION => EDIT_IME.set(false),
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let sys = msg == WM_SYSKEYDOWN;
            if let Some(true) = with_app(|app| app.on_key(wparam.0 as u16, sys)) {
                return LRESULT(0);
            }
        }
        WM_MOUSEWHEEL => {
            // 入力欄にフォーカスがあってもホイールで一覧を動かす(C2)
            if let Ok(parent) = unsafe { GetParent(hwnd) } {
                unsafe { SendMessageW(parent, msg, Some(wparam), Some(lparam)) };
                return LRESULT(0);
            }
        }
        WM_PASTE => {
            if let Some(true) = with_app(|app| app.paste_multiline()) {
                return LRESULT(0);
            }
            // 標準の貼り付けは, クリップボードを開いたままEN_CHANGEを送る. その中で拡張に渡す文字を読むと
            // 途中で閉じてしまうので, 先に読んで閉じてから入れる(K50)
            if let Some(Some(t)) = with_app(|app| app.paste_line()) {
                let w = util::wide(&t);
                unsafe { SendMessageW(hwnd, windows::Win32::UI::Controls::EM_REPLACESEL, Some(WPARAM(1)), Some(LPARAM(w.as_ptr() as isize))) };
                redraw_cue(hwnd);
                return LRESULT(0);
            }
        }
        WM_IME_STARTCOMPOSITION => EDIT_IME.set(true),
        WM_CHAR | WM_SYSCHAR => {
            let c = wparam.0 as u32;
            // 書く窓にしたあとに届いた文字(Ctrl+Vなど)は入れない
            let capture = with_app(|app| app.capture || app.composing()).unwrap_or(false);
            // Enter, Tab, Esc, Ctrl+Backspace, Ctrl+A, Ctrl+N/P の文字は入力欄に入れない(鳴らさない)
            if capture || matches!(c, 0x0D | 0x09 | 0x1B | 0x7F | 0x01 | 0x0E | 0x10 | 0x0A) {
                return LRESULT(0);
            }
            if msg == WM_SYSCHAR {
                return LRESULT(0);
            }
        }
        _ => {}
    }
    let r = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    // 空になった, 空でなくなったときは描き直す(入力欄は変わった文字のところしか描かないことがある)
    if matches!(
        msg,
        WM_SETTEXT | WM_KEYDOWN | WM_CHAR | WM_PASTE | WM_CUT | WM_CLEAR | WM_UNDO | WM_IME_STARTCOMPOSITION | WM_IME_ENDCOMPOSITION | WM_IME_COMPOSITION | WM_IME_CHAR
    ) {
        redraw_cue(hwnd);
    }
    r
}

/// 案内を出すか消すかが変わったら, 入力欄を描き直す
fn redraw_cue(hwnd: HWND) {
    if cue_wanted(hwnd) != CUE_SHOWN.get() {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, true);
        }
    }
}

/// 書く窓(RichEdit)のキーを横取りする(K22)
extern "system" fn compose_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, id: usize, data: usize) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compose_proc_inner(hwnd, msg, wparam, lparam, id, data))) {
        Ok(r) => r,
        Err(_) => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

fn compose_proc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        // RichEditが描いたあとに, 箇条書きの「•」などを上から描く
        WM_PAINT => {
            let r = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            compose::paint_decos(hwnd);
            return r;
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            EAT_CHAR.set(false);
            if let Some(true) = with_app(|app| app.compose_key(wparam.0 as u16, msg == WM_SYSKEYDOWN)) {
                EAT_CHAR.set(true);
                return LRESULT(0);
            }
        }
        WM_CHAR | WM_SYSCHAR => {
            if EAT_CHAR.replace(false) {
                return LRESULT(0);
            }
        }
        // 日本語入力で変換している間は整えない(整えると変換が切れる)
        WM_IME_STARTCOMPOSITION => {
            with_app(|app| {
                if let Some(c) = &mut app.compose {
                    c.ime = true;
                }
            });
        }
        WM_IME_ENDCOMPOSITION => {
            with_app(|app| {
                if let Some(c) = &mut app.compose {
                    c.ime = false;
                }
                app.schedule_restyle(true);
            });
        }
        WM_PASTE => {
            if with_app(|app| app.compose_paste()).is_some() {
                return LRESULT(0);
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// 窓の枠の色(Windows 11). ライトとダークで変える(K39)
fn set_border_color(hwnd: HWND) {
    let border: u32 = util::rgb(theme::pal().border).0;
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, windows::Win32::Graphics::Dwm::DWMWA_BORDER_COLOR, &border as *const _ as *const _, 4);
    }
}

/// 別の場所の(または前の版が写した)Hikidashiが動いていたら, 終わってもらう(K47). 終わらせたらtrue.
/// 同じexeなら何もしない(動いている方の窓を出す)
fn take_over() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    unsafe {
        let Ok(h) = FindWindowW(CLASS, PCWSTR::null()) else { return false };
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        let other = winlist::process_path(pid);
        if other.is_empty() || other.eq_ignore_ascii_case(&exe.to_string_lossy()) {
            return false;
        }
        storage::log(&format!("taking over from {other}"));
        let _ = PostMessageW(Some(h), WM_APP_QUIT, WPARAM(0), LPARAM(0));
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if FindWindowW(CLASS, PCWSTR::null()).is_err() {
                return true;
            }
        }
    }
    false
}

/// 動いているHikidashiの窓に`--query`の文字を`WM_COPYDATA`で送る. 送れたらtrue
fn send_query(text: &str) -> bool {
    unsafe {
        let Ok(h) = FindWindowW(CLASS, PCWSTR::null()) else { return false };
        // 動いている方が前に出られるようにする
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let data = util::wide(text);
        let cds = COPYDATASTRUCT { dwData: QUERY_MAGIC, cbData: (data.len() * 2) as u32, lpData: data.as_ptr() as *mut _ };
        // 窓が出るまで待つ(相手が忙しければ待たされる)ので, 返ってきたら届いたとみなす
        SendMessageW(h, WM_COPYDATA, Some(WPARAM(0)), Some(LPARAM(&cds as *const _ as isize))).0 != 0
    }
}

pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        storage::log(&format!("panic: {info}"));
    }));
    diag::log_crashes();
    // `--query 文字`: 動いていればその窓に渡して終わる(置き換えのための入れ替えもしない). 動いていなければ, ふつうに起動して, その文字で窓を出す
    let query = csel::query_arg(std::env::args().skip(1));
    if let Some(q) = &query {
        if send_query(q) {
            return;
        }
    }
    // 置いた場所のexeがそのまま動く(K47. wakudoriと同じ)
    let updated = take_over();
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    // 2つ目は起動せず, 動いているものの窓を出す
    let mut first = false;
    for _ in 0..30 {
        let m = unsafe { CreateMutexW(None, true, w!("Local\\HikidashiSingleInstance")) };
        if m.is_ok() && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
            first = true;
            break;
        }
        if !updated {
            break;
        }
        // 前のものが終わるのを待つ
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if !first {
        if let Some(q) = &query {
            if send_query(q) {
                return;
            }
        }
        unsafe {
            if let Ok(h) = FindWindowW(CLASS, PCWSTR::null()) {
                // 動いている方が前に出られるようにする
                let _ = AllowSetForegroundWindow(ASFW_ANY);
                let _ = PostMessageW(Some(h), WM_APP_SHOW, WPARAM(0), LPARAM(0));
            }
        }
        return;
    }
    let autostart = std::env::args().any(|a| a == "--autostart");

    let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        hbrBackground: HBRUSH::default(),
        lpszClassName: CLASS,
        hIcon: system::app_icon(),
        ..Default::default()
    };
    unsafe { RegisterClassExW(&wc) };
    // テーマを決めてから窓と書く窓を作る(後で読むと, 書く窓の地がライトのままになる. K50)
    let (config, err) = storage::load_config();
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS,
            w!("Hikidashi"),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            scale(theme::WIDTH, 96),
            scale(theme::INPUT_H, 96),
            None,
            None,
            Some(hinst.into()),
            None,
        )
    };
    let Ok(hwnd) = hwnd else { return };
    unsafe {
        // Windows 11では角を丸くする
        let pref: u32 = 2;
        let _ = DwmSetWindowAttribute(hwnd, windows::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as *const _, 4);
    }
    let edit = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            0,
            0,
            100,
            30,
            Some(hwnd),
            None,
            Some(hinst.into()),
            None,
        )
    };
    let Ok(edit) = edit else { return };
    unsafe {
        let _ = SetWindowSubclass(edit, Some(edit_proc), 1, 0);
        SendMessageW(edit, EM_SETLIMITTEXT, Some(WPARAM(EDIT_LIMIT)), Some(LPARAM(0)));
    }

    let compose = compose::Compose::create(hwnd);
    match &compose {
        Some(c) => unsafe {
            let _ = SetWindowSubclass(c.hwnd, Some(compose_proc), 2, 0);
        },
        None => storage::log("compose: could not create the RichEdit control; Shift+Enter sends to ChatGPT right away"),
    }

    set_border_color(hwnd);
    let shared = Shared::new(config.clone());
    let dict: Arc<OnceLock<Dict>> = Arc::new(OnceLock::new());
    dict::start(hwnd, shared.clone(), dict.clone());
    let searcher = Searcher::new(hwnd, dict.clone());
    let state = storage::load_state();
    let hotkey = state.hotkey.as_deref().and_then(Hotkey::parse);
    let exe_path = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let animations = {
        let mut on = windows::core::BOOL(1);
        unsafe {
            let _ = SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut on as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        }
        on.as_bool()
    };
    let taskbar_msg = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    let app = App {
        hwnd,
        edit,
        shared: shared.clone(),
        searcher,
        dict,
        commands: Catalog::new(router::command_items(&config)),
        sel_hotkey: None,
        sel_registered: false,
        capture_sel: false,
        clips: ClipHistory::default(),
        clip_listening: false,
        clip_ignore: None,
        prev_fg: 0,
        selection_wait: None,
        scripts: vec![],
        scripts_sig: vec![],
        lists: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        script_job: 0,
        script_view: false,
        recent_view: false,
        script_all: String::new(),
        script_result: Arc::new(std::sync::Mutex::new(None)),
        ext: ext::Ext::new(),
        folders: Catalog::new(builtin::folder_items(
            &builtin::FOLDER_VARS
                .iter()
                .filter_map(|v| std::env::var(v).ok().filter(|p| Path::new(p).is_dir()).map(|p| (v.to_string(), p)))
                .collect::<Vec<_>>(),
        )),
        favicons: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        favicons_at: None,
        config,
        config_error: err.map(|e| e.0),
        themed_dark: theme::is_dark(),
        apps: Catalog::new(vec![]),
        bookmarks: Catalog::new(vec![]),
        bookmarks_mtime: vec![],
        windows: Catalog::new(vec![]),
        processes: Catalog::new(vec![]),
        processes_fresh: false,
        settings: Catalog::new(builtin::settings_items()),
        system: Catalog::new(builtin::system_items()),
        learn: storage::load_learn(),
        history: storage::load_history(),
        state,
        mode: Mode::All,
        rows: vec![],
        sel: 0,
        top: 0,
        last_mouse: (i32::MIN, i32::MIN),
        press_row: None,
        tables: vec![],
        gen: 0,
        pending: None,
        menu: None,
        assoc: Vec::new(),
        confirm: None,
        capture: false,
        message: None,
        hotkey,
        glow: None,
        animations,
        dpi: 0,
        fonts: Fonts::new(96),
        icons: icons::Icons::new(scale(theme::ICON, 96)),
        stats: Stats::default(),
        from_history: false,
        user_moved: false,
        rebuild_wait: None,
        taskbar_msg,
        exe_path,
        compose,
        drafts: storage::load_drafts(),
        browse: Browse::default(),
        recalled: String::new(),
        compose_h: 0,
        restyle_text: false,
        restyle_sel: false,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));
    with_app(|app| {
        app.set_dpi(96);
        system::tray_add(hwnd);
        // フォルダーを移したときは, 自動起動の登録先を今の場所に直す
        system::fix_autostart(&app.exe_path);
        std::thread::spawn(storage::remove_old_copy);
        indexer::start(&app.shared, hwnd);
        app.ext.set_options(&app.config.ext);
        app.ext.start();
        diag::start(hwnd);
        app.icons.start(hwnd);
        app.reload_bookmarks();
        app.sync_clip_listener();
        match app.hotkey {
            Some(h) if system::register_hotkey(hwnd, h) => {
                if updated {
                    app.message = Some(("Hikidashi was updated".into(), false));
                }
                app.apply_selection_hotkey();
                if let Some(q) = &query {
                    app.on_query(q);
                } else if !autostart {
                    app.show();
                }
            }
            Some(h) => {
                app.start_capture(false);
                app.message = Some((format!("{} is used by another app. Press another hotkey.", h.format()), true));
                app.layout();
            }
            None => app.start_capture(false),
        }
    });
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if settings::pre_translate(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    with_app(|app| {
        app.save_draft();
        storage::save_learn(&app.learn);
        storage::save_history(&app.history);
        system::tray_remove(app.hwnd);
    });
    // スレッドが動いているうちに片付ける(プロセスの終わりでは, 待つ相手がもういない. K51)
    let app = APP.with(|a| a.borrow_mut().take());
    drop(app);
}
