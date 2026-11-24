//! JavaScriptの拡張(`hikidashi-ext`)をランチャーにつなぐ. 同梱のものは起動時に書き出し, 利用者のものが優先.
//! 検索は専用のスレッドで動き, 結果は`WM_APP_EXT`で窓に知らせる(画面のスレッドでは待たない).

use super::{clipboard, storage};
use hikidashi_core::router::{Action, Icon, Row};
use hikidashi_core::scripts as cscripts;
use hikidashi_ext::{ExtHost, ExtMeta, ExtRow};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 拡張の結果が届いた
pub const WM_APP_EXT: u32 = 0x8000 + 43;

/// exeに埋め込んで配る拡張(ファイル名, 中身)
const BUNDLED: &[(&str, &str)] = &[("rffe.js", include_str!("../../../../extensions/rffe.js"))];

/// 利用者の拡張のフォルダー(作らない)
fn user_dir() -> PathBuf {
    storage::data_dir().join("extensions")
}

/// 同梱の拡張の書き出し先
fn bundled_dir() -> PathBuf {
    storage::install_dir().join("extensions")
}

/// 同梱の拡張を書き出す(中身が違うときだけ)
fn write_bundled() {
    let dir = bundled_dir();
    let _ = std::fs::create_dir_all(&dir);
    for (name, text) in BUNDLED {
        let path = dir.join(name);
        if std::fs::read(&path).ok().as_deref() != Some(text.as_bytes()) {
            if let Err(e) = std::fs::write(&path, text) {
                storage::log(&format!("ext: could not write {name}: {e}"));
            }
        }
    }
}

type Slot = Arc<Mutex<Option<Arc<ExtHost>>>>;

pub struct Ext {
    host: Slot,
    result: Arc<Mutex<Option<(u64, Vec<ExtRow>)>>>,
    /// 入っているkeyword(小文字)と, そのとき読んだクリップボードの文字(入った最初に1回だけ読む)
    active: Option<(String, Option<String>)>,
    /// 今の一覧が拡張の行
    pub view: bool,
    prev_view: bool,
    /// 行ごとのEnterでコピーする文字(`view`のとき)
    pub copies: Vec<String>,
    /// 結果を待っている世代
    pending: Option<u64>,
    /// ログに書いた内容(同じものは1回だけ)
    logged: Arc<Mutex<HashSet<String>>>,
}

fn log_once(logged: &Mutex<HashSet<String>>, msg: String) {
    if logged.lock().unwrap().insert(msg.clone()) {
        storage::log(&msg);
    }
}

fn log_errors(host: &ExtHost, logged: &Mutex<HashSet<String>>) {
    for (name, reason) in host.errors() {
        log_once(logged, format!("ext: could not load {name}: {reason}"));
    }
}

/// 開いてよいURLか(`https://`と`http://`だけ)
pub fn url_ok(u: &str) -> bool {
    let l = u.trim_start().to_ascii_lowercase();
    l.starts_with("https://") || l.starts_with("http://")
}

impl Ext {
    pub fn new() -> Ext {
        Ext {
            host: Arc::new(Mutex::new(None)),
            result: Arc::new(Mutex::new(None)),
            active: None,
            view: false,
            prev_view: false,
            copies: vec![],
            pending: None,
            logged: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// 読み込みを待つので, 起動を遅らせないよう裏のスレッドで始める
    pub fn start(&self) {
        let (slot, logged) = (self.host.clone(), self.logged.clone());
        let _ = std::thread::Builder::new().name("ext-start".into()).spawn(move || {
            write_bundled();
            let host = Arc::new(ExtHost::start(vec![user_dir(), bundled_dir()]));
            log_errors(&host, &logged);
            *slot.lock().unwrap() = Some(host);
        });
    }

    fn host(&self) -> Option<Arc<ExtHost>> {
        self.host.lock().unwrap().clone()
    }

    /// 窓を出すたびに読み直し(変わったものだけ), 読めないものと名前の重なりをログに書く
    pub fn reload(&self, taken: &[String]) {
        let Some(h) = self.host() else { return };
        log_errors(&h, &self.logged);
        for m in h.list() {
            let kw = m.keyword.to_lowercase();
            if taken.contains(&kw) {
                log_once(&self.logged, format!("ext: {} keyword \"{}\" is ignored (a prefix or a script uses it)", m.name, kw));
            }
        }
        h.reload();
    }

    /// 一覧を作り直す前に呼ぶ
    pub fn begin_refresh(&mut self) {
        self.prev_view = std::mem::take(&mut self.view);
        self.pending = None;
    }

    /// 窓を隠したとき
    pub fn reset(&mut self) {
        self.active = None;
        self.view = false;
        self.prev_view = false;
        self.pending = None;
    }

    /// 入力が`keyword `で始まる拡張と, そのあとの文字. `taken`(接頭辞とスクリプトのkeyword. 小文字)と重なるものは使わない
    fn find(&self, text: &str, taken: &[String]) -> Option<(ExtMeta, String)> {
        let host = self.host()?;
        let text = text.trim_start();
        host.list().into_iter().find_map(|m| {
            let kw = m.keyword.to_lowercase();
            if taken.contains(&kw) {
                return None;
            }
            let head = text.get(..m.keyword.len())?;
            let rest = text.get(m.keyword.len()..)?;
            if head.to_lowercase() == kw && rest.starts_with(char::is_whitespace) {
                Some((m.clone(), rest.trim().to_string()))
            } else {
                None
            }
        })
    }

    /// 拡張の行を出す入力なら, 検索を頼んで(待たない)今の行を返す. 届くまでは前の行か`Loading…`
    pub fn rows(&mut self, text: &str, taken: &[String], hwnd: HWND, gen: u64, current: &[Row]) -> Option<Vec<Row>> {
        let Some((meta, rest)) = self.find(text, taken) else {
            self.active = None;
            return None;
        };
        let host = self.host()?;
        let kw = meta.keyword.to_lowercase();
        // クリップボードは, 入力がこの拡張に入った最初に1回だけ読む
        if self.active.as_ref().map(|a| a.0.as_str()) != Some(kw.as_str()) {
            let clip = if meta.clipboard { clipboard::read_text(hwnd) } else { None };
            self.active = Some((kw.clone(), clip));
        }
        let clip = if meta.clipboard { self.active.as_ref().and_then(|a| a.1.clone()) } else { None };
        let rx = host.search(&meta.keyword, &rest, clip, gen);
        self.pending = Some(gen);
        let (slot, h) = (self.result.clone(), hwnd.0 as isize);
        let _ = std::thread::Builder::new().name("ext-wait".into()).spawn(move || {
            // 新しい要求で捨てられたときは受け取れずに終わる
            if let Ok(r) = rx.recv() {
                *slot.lock().unwrap() = Some((r.generation, r.rows));
                unsafe {
                    let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_EXT, WPARAM(0), LPARAM(0));
                }
            }
        });
        self.view = true;
        if self.prev_view {
            Some(current.to_vec())
        } else {
            self.copies.clear();
            Some(vec![cscripts::loading_row()])
        }
    }

    /// 今の世代の結果が届いていれば, 行にして返す(古い世代のものは捨てる)
    pub fn take(&mut self, gen: u64) -> Option<Vec<Row>> {
        let mut g = self.result.lock().unwrap();
        let (g0, rows) = g.take()?;
        if g0 != gen || !self.view || self.pending != Some(gen) {
            return None;
        }
        drop(g);
        self.pending = None;
        let mut copies = Vec::new();
        let out: Vec<Row> = rows
            .iter()
            .map(|r| {
                copies.push(r.copy.clone());
                make_row(r)
            })
            .collect();
        self.copies = copies;
        Some(out)
    }

    /// 結果を待っている世代. なければNone
    pub fn pending(&self) -> Option<u64> {
        self.pending
    }

    /// 結果が届くまで少し(最大0.3秒)待つ. 届かなければNone
    pub fn wait(&mut self, gen: u64) -> Option<Vec<Row>> {
        let until = Instant::now() + Duration::from_millis(300);
        loop {
            let ready = self.result.lock().unwrap().as_ref().is_some_and(|r| r.0 == gen);
            if ready {
                return self.take(gen);
            }
            if Instant::now() >= until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// 拡張の1行. urlが開けるものは`url`(Enterでブラウザー), それ以外は`copy`(Enterでコピー). 学習には残さない
fn make_row(r: &ExtRow) -> Row {
    let (tag, action, icon) = match r.url.as_deref().filter(|u| url_ok(u)) {
        Some(u) => ("url", Action::OpenUrl(u.trim().to_string()), Icon::Web),
        None => ("copy", Action::Copy(r.copy.clone()), Icon::Calc),
    };
    Row { name: r.title.clone(), sub: r.subtitle.clone(), tag, positions: vec![], action, learn_key: None, warn: false, icon, score: 0 }
}
