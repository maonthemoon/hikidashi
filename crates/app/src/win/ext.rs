//! JavaScriptの拡張(`hikidashi-ext`)をランチャーにつなぐ. 同梱のものは起動時に書き出し, 利用者のものが優先.
//! 検索は専用のスレッドで動き, 結果は`WM_APP_EXT`で窓に知らせる(画面のスレッドでは待たない).

use super::{clipboard, storage};
use hikidashi_core::router::{Action, Icon, Row};
use hikidashi_core::scripts as cscripts;
use hikidashi_ext::{ExtHost, ExtMeta, ExtRow, Options, Seg};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 拡張の結果が届いた
pub const WM_APP_EXT: u32 = 0x8000 + 43;

/// exeに埋め込んで配る拡張(ファイル名, 中身)
const BUNDLED: &[(&str, &str)] = &[("mipi.js", include_str!("../../../../extensions/mipi.js"))];

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
    // 前の版の名前(K50)
    let _ = std::fs::remove_file(dir.join("rffe.js"));
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
    /// 行ごとの帯, 誤りか, Tabで入れる文字(`view`のとき)
    pub bands: Vec<Vec<Seg>>,
    pub bads: Vec<bool>,
    pub completes: Vec<Option<String>>,
    options: Arc<Mutex<Options>>,
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

/// 0と1(と空白, `_`)だけで`min`桁以上か. 1000000000000のような数と分けるため, 1が3つ以上あるもの
fn is_bits(text: &str, min: usize) -> bool {
    let t: Vec<char> = text.chars().filter(|c| !c.is_whitespace() && *c != '_').collect();
    min > 0 && t.len() >= min && t.iter().all(|c| matches!(c, '0' | '1')) && t.iter().filter(|c| **c == '1').count() >= 3
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
            bands: vec![],
            bads: vec![],
            completes: vec![],
            options: Arc::new(Mutex::new(Options::new())),
            pending: None,
            logged: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// 読み込みを待つので, 起動を遅らせないよう裏のスレッドで始める
    pub fn start(&self) {
        let (slot, logged, options) = (self.host.clone(), self.logged.clone(), self.options.clone());
        let _ = std::thread::Builder::new().name("ext-start".into()).spawn(move || {
            write_bundled();
            let host = Arc::new(ExtHost::start(vec![user_dir(), bundled_dir()]));
            log_errors(&host, &logged);
            host.set_options(options.lock().unwrap().clone());
            *slot.lock().unwrap() = Some(host);
        });
    }

    /// 設定ファイルの`[ext.…]`を渡す
    pub fn set_options(&self, o: &Options) {
        *self.options.lock().unwrap() = o.clone();
        if let Some(h) = self.host() {
            h.set_options(o.clone());
        }
    }

    /// 計算のモードで, 打ちかけの語から拡張の名前を補う行(`mi`→`mipi`)
    pub fn hints(&self, text: &str, taken: &[String]) -> Vec<Row> {
        let t = text.trim().to_lowercase();
        let Some(h) = self.host().filter(|_| !t.is_empty()) else { return vec![] };
        h.list()
            .into_iter()
            .filter(|m| m.calc && m.keyword.to_lowercase().starts_with(&t) && !taken.contains(&m.keyword.to_lowercase()))
            .map(|m| Row { name: m.keyword.clone(), sub: m.title.clone(), tag: "try", positions: vec![], action: Action::Fill(format!("{} ", m.keyword)), learn_key: None, warn: false, icon: Icon::Calc, score: 0 })
            .collect()
    }

    /// `?`の一覧に足す, 拡張のキーワード(K62). 計算のモードの中だけで使うものも出す
    pub fn help_rows(&self, taken: &[String]) -> Vec<Row> {
        let Some(h) = self.host() else { return vec![] };
        h.list()
            .into_iter()
            .filter(|m| !m.keyword.is_empty() && !taken.contains(&m.keyword.to_lowercase()))
            .map(|m| hikidashi_core::router::help_row(&m.keyword, &m.title, &m.name, "extension"))
            .collect()
    }

    fn clear_rows(&mut self) {
        self.copies.clear();
        self.bands.clear();
        self.bads.clear();
        self.completes.clear();
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
    /// `calc`(計算のモード)なら`meta.calc`のものだけ
    fn find(&self, text: &str, taken: &[String], calc: bool) -> Option<(ExtMeta, String)> {
        let host = self.host()?;
        let text = text.trim_start();
        host.list().into_iter().find_map(|m| {
            let kw = m.keyword.to_lowercase();
            if taken.contains(&kw) || (calc && !m.calc) {
                return None;
            }
            // 計算のモードでは, 長いビット列をそのまま受け, keywordだけでも使い方を出す
            if calc && is_bits(text, m.calc_bits) {
                return Some((m.clone(), text.trim().to_string()));
            }
            let head = text.get(..m.keyword.len())?;
            let rest = text.get(m.keyword.len()..)?;
            if head.to_lowercase() == kw && (rest.starts_with(char::is_whitespace) || calc && rest.is_empty()) {
                Some((m.clone(), rest.trim_start().to_string()))
            } else {
                None
            }
        })
    }

    /// 拡張の行を出す入力なら, 検索を頼んで(待たない)今の行を返す. 届くまでは前の行か`Loading…`
    pub fn rows(&mut self, text: &str, taken: &[String], calc: bool, hwnd: HWND, gen: u64, current: &[Row]) -> Option<Vec<Row>> {
        let Some((meta, rest)) = self.find(text, taken, calc) else {
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
            self.clear_rows();
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
        self.clear_rows();
        for r in &rows {
            self.copies.push(r.copy.clone());
            self.bands.push(r.band.clone());
            self.bads.push(r.bad);
            self.completes.push(r.complete.clone());
        }
        Some(rows.iter().map(make_row).collect())
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
    let (tag, action, icon) = match (r.url.as_deref().filter(|u| url_ok(u)), &r.fill) {
        (_, Some(t)) => ("try", Action::Fill(t.clone()), Icon::Calc),
        (Some(u), None) => ("url", Action::OpenUrl(u.trim().to_string()), Icon::Web),
        (None, None) => (if r.copy.is_empty() { "" } else { "copy" }, Action::Copy(r.copy.clone()), Icon::Calc),
    };
    Row { name: r.title.clone(), sub: r.subtitle.clone(), tag, positions: vec![], action, learn_key: None, warn: false, icon, score: 0 }
}
