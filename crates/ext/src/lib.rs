//! JavaScriptの拡張(1ファイル1拡張)を動かす. Windowsに依存しない.
//!
//! 拡張の形:
//! ```js
//! export const meta = { title: "MIPI RFFE", keyword: "rffe", clipboard: true };
//! export function search(query, ctx) { return [{ title: "…", subtitle: "…", copy: "…", url: "…" }]; }
//! ```
//! `rquickjs`の`Runtime`はスレッドをまたげないので, 専用のスレッドを1本立ててその中で全部行う.
//! 呼ぶ側は`ExtHost`にメッセージを送り, 結果を`Receiver`で受ける.

use rquickjs::{Context, Ctx, Function, Module, Object, Runtime, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

/// 返す行の最大数
pub const MAX_ROWS: usize = 50;
/// 1回の`search`の制限時間
pub const SEARCH_TIMEOUT: Duration = Duration::from_millis(200);
/// ファイルを読み込む(モジュールの最上位を実行する)ときの制限時間
const LOAD_TIMEOUT: Duration = Duration::from_millis(1000);
const MEMORY_LIMIT: usize = 32 * 1024 * 1024;

/// 拡張の情報(`meta`)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtMeta {
    /// ファイル名から拡張子を除いたもの
    pub name: String,
    pub keyword: String,
    pub title: String,
    /// trueなら`search`の`ctx.clipboard`にクリップボードの文字を渡す
    pub clipboard: bool,
    /// trueなら計算のモード(`= `)の中でも使える
    pub calc: bool,
}

/// 結果の1行
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtRow {
    pub title: String,
    pub subtitle: String,
    /// Enterでコピーする文字(省くとtitle)
    pub copy: String,
    /// あればEnterでURLを開く(Tabのメニューでコピー)
    pub url: Option<String>,
}

impl ExtRow {
    fn error(name: &str, msg: &str) -> ExtRow {
        let title = format!("{}: {}", name, msg);
        ExtRow { copy: title.clone(), title, subtitle: String::new(), url: None }
    }
}

/// `search`の結果. `generation`は呼んだときに渡した値
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub generation: u64,
    pub rows: Vec<ExtRow>,
}

enum Cmd {
    Reload(Option<Sender<()>>),
    Search { keyword: String, query: String, clipboard: Option<String>, generation: u64, reply: Sender<SearchResult> },
    Quit,
}

#[derive(Default)]
struct Shared {
    metas: Vec<ExtMeta>,
    /// (ファイル名, 理由)
    errors: Vec<(String, String)>,
}

pub struct ExtHost {
    tx: Sender<Cmd>,
    shared: Arc<Mutex<Shared>>,
    latest: Arc<AtomicU64>,
    join: Option<JoinHandle<()>>,
}

impl ExtHost {
    /// 専用のスレッドを立て, フォルダーの`*.js`を読む(読み終わるまで待つ).
    /// フォルダーは先のものほど優先(同じファイル名は先のものを使う). フォルダーがなくてもよい(拡張が0個になる).
    pub fn start(dirs: Vec<PathBuf>) -> ExtHost {
        let (tx, rx) = channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let latest = Arc::new(AtomicU64::new(0));
        let (ready_tx, ready_rx) = channel();
        let (s, l) = (shared.clone(), latest.clone());
        let join = std::thread::Builder::new()
            .name("hikidashi-ext".into())
            .spawn(move || worker(dirs, rx, s, l, ready_tx))
            .expect("拡張のスレッドを立てられない");
        let _ = ready_rx.recv();
        ExtHost { tx, shared, latest, join: Some(join) }
    }

    /// 読み込めた拡張の一覧(ファイル名順)
    pub fn list(&self) -> Vec<ExtMeta> {
        self.shared.lock().unwrap().metas.clone()
    }

    /// 読めなかったファイルと理由
    pub fn errors(&self) -> Vec<(String, String)> {
        self.shared.lock().unwrap().errors.clone()
    }

    /// ファイルの更新日時が変わったものを読み直す(待たない. 更新がなければ`read_dir`と`metadata`だけ).
    /// 直後の`list()`にはまだ反映されていないことがある. 終わるまで待つなら`reload_wait`.
    pub fn reload(&self) {
        let _ = self.tx.send(Cmd::Reload(None));
    }

    /// `reload`して, 終わるまで待つ
    pub fn reload_wait(&self) {
        let (t, r) = channel();
        if self.tx.send(Cmd::Reload(Some(t))).is_ok() {
            let _ = r.recv();
        }
    }

    /// 非同期に`search`する. 結果は返り値の`Receiver`に1回だけ届く.
    /// 新しい`generation`の要求が来て実行されなかった古い要求は, `Receiver`が切れる(`recv`が`Err`).
    /// keywordに合う拡張がなければ空の行が届く.
    pub fn search(&self, keyword: &str, query: &str, clipboard: Option<String>, generation: u64) -> Receiver<SearchResult> {
        let (reply, rx) = channel();
        self.latest.fetch_max(generation, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Search {
            keyword: keyword.to_string(),
            query: query.trim().to_string(),
            clipboard,
            generation,
            reply,
        });
        rx
    }
}

impl Drop for ExtHost {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// 制限時間の見張り. 割り込みの関数(`set_interrupt_handler`)から読まれる
struct Guard {
    base: Instant,
    /// 期限(`base`からのナノ秒). 0は期限なし
    deadline: AtomicU64,
    fired: AtomicBool,
}

impl Guard {
    fn arm(&self, d: Duration) {
        self.fired.store(false, Ordering::SeqCst);
        let t = (self.base.elapsed() + d).as_nanos() as u64;
        self.deadline.store(t.max(1), Ordering::SeqCst);
    }
    fn disarm(&self) {
        self.deadline.store(0, Ordering::SeqCst);
    }
    fn check(&self) -> bool {
        let d = self.deadline.load(Ordering::Relaxed);
        if d != 0 && self.base.elapsed().as_nanos() as u64 > d {
            self.fired.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
}

type Stamp = (SystemTime, u64);

struct Loaded {
    meta: ExtMeta,
    stamp: Stamp,
    ctx: Context,
}

/// スレッドの中の状態
#[derive(Default)]
struct State {
    exts: HashMap<String, Loaded>,
    /// 読み込みに失敗したファイル(変わるまで読み直さない)
    failed: HashMap<String, (Stamp, String)>,
}

fn worker(dirs: Vec<PathBuf>, rx: Receiver<Cmd>, shared: Arc<Mutex<Shared>>, latest: Arc<AtomicU64>, ready: Sender<()>) {
    let guard = Arc::new(Guard { base: Instant::now(), deadline: AtomicU64::new(0), fired: AtomicBool::new(false) });
    let rt = Runtime::new().expect("QuickJSを初期化できない");
    rt.set_memory_limit(MEMORY_LIMIT);
    let g = guard.clone();
    rt.set_interrupt_handler(Some(Box::new(move || g.check())));
    // stateはrtより後に作る(= 先に捨てる)
    let mut st = State::default();

    reload_all(&rt, &guard, &dirs, &mut st, &shared);
    let _ = ready.send(());

    loop {
        let Ok(first) = rx.recv() else { break };
        let mut cmds = vec![first];
        while let Ok(c) = rx.try_recv() {
            cmds.push(c);
        }
        // 読み直しを先に済ませ, 検索は最新の1件だけ実行する
        let mut last_search = None;
        let mut quit = false;
        let mut acks = Vec::new();
        let mut want_reload = false;
        for c in cmds {
            match c {
                Cmd::Quit => quit = true,
                Cmd::Reload(a) => {
                    want_reload = true;
                    acks.extend(a);
                }
                s @ Cmd::Search { .. } => last_search = Some(s),
            }
        }
        if quit {
            break;
        }
        if want_reload {
            reload_all(&rt, &guard, &dirs, &mut st, &shared);
        }
        for a in acks {
            let _ = a.send(());
        }
        if let Some(Cmd::Search { keyword, query, clipboard, generation, reply }) = last_search {
            if generation < latest.load(Ordering::SeqCst) {
                continue; // もっと新しい要求がある
            }
            let rows = run_search(&guard, &st.exts, &keyword, &query, clipboard);
            let _ = reply.send(SearchResult { generation, rows });
        }
    }
    st.exts.clear();
}

fn reload_all(rt: &Runtime, guard: &Guard, dirs: &[PathBuf], st: &mut State, shared: &Mutex<Shared>) {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    // 先のフォルダーから順に見て, 同じ名前は最初に見つけたものだけ使う
    for dir in dirs {
        let mut found: Vec<(String, PathBuf)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                let is_js = p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("js")) == Some(true);
                if let (true, Some(stem)) = (is_js, p.file_stem().and_then(|s| s.to_str())) {
                    found.push((stem.to_string(), p.clone()));
                }
            }
        }
        found.sort();
        for (n, p) in found {
            if !files.iter().any(|(m, _)| *m == n) {
                files.push((n, p));
            }
        }
    }
    files.sort();
    let names: Vec<&String> = files.iter().map(|(n, _)| n).collect();
    st.exts.retain(|k, _| names.contains(&k));
    st.failed.retain(|k, _| names.contains(&k));
    for (name, path) in &files {
        let stamp = match std::fs::metadata(path).and_then(|m| Ok((m.modified()?, m.len()))) {
            Ok(s) => s,
            Err(e) => {
                st.exts.remove(name);
                st.failed.insert(name.clone(), ((SystemTime::UNIX_EPOCH, 0), format!("読めない: {}", e)));
                continue;
            }
        };
        let unchanged = st.exts.get(name).map(|l| l.stamp == stamp).unwrap_or(false)
            || st.failed.get(name).map(|f| f.0 == stamp).unwrap_or(false);
        if unchanged {
            continue;
        }
        st.exts.remove(name);
        st.failed.remove(name);
        match load_file(rt, guard, name, path, stamp) {
            Ok(l) => {
                st.exts.insert(name.clone(), l);
            }
            Err(e) => {
                st.failed.insert(name.clone(), (stamp, e));
            }
        }
    }
    let mut metas: Vec<ExtMeta> = st.exts.values().map(|l| l.meta.clone()).collect();
    metas.sort_by(|a, b| a.name.cmp(&b.name));
    let mut errors: Vec<(String, String)> = st.failed.iter().map(|(n, (_, r))| (n.clone(), r.clone())).collect();
    errors.sort();
    let mut sh = shared.lock().unwrap();
    sh.metas = metas;
    sh.errors = errors;
}

/// エラーを1行の短い文にする
fn describe(ctx: &Ctx<'_>, guard: &Guard, e: rquickjs::Error, timeout: Duration) -> String {
    if guard.fired.load(Ordering::SeqCst) {
        let _ = ctx.catch();
        return format!("時間切れ({}ms)", timeout.as_millis());
    }
    let msg = match e {
        rquickjs::Error::Exception => {
            let v = ctx.catch();
            match v.as_exception().and_then(|x| x.message()) {
                Some(m) => m,
                None => match v.as_string().and_then(|s| s.to_string().ok()) {
                    Some(s) => s,
                    None => "例外".to_string(),
                },
            }
        }
        other => other.to_string(),
    };
    let line = msg.lines().next().unwrap_or("").trim().to_string();
    let mut out: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        out.push('…');
    }
    out
}

fn load_file(rt: &Runtime, guard: &Guard, name: &str, path: &Path, stamp: Stamp) -> Result<Loaded, String> {
    let src = std::fs::read(path).map_err(|e| format!("読めない: {}", e))?;
    let src = String::from_utf8(src).map_err(|_| "UTF-8ではない".to_string())?;
    let ctx = Context::full(rt).map_err(|e| e.to_string())?;
    guard.arm(LOAD_TIMEOUT);
    let r = ctx.with(|c| -> Result<ExtMeta, String> {
        let d = |e| describe(&c, guard, e, LOAD_TIMEOUT);
        let m = Module::declare(c.clone(), format!("{}.js", name), src).map_err(d)?;
        let (m, p) = m.eval().map_err(d)?;
        p.finish::<()>().map_err(d)?;
        let meta: Value = m.get("meta").map_err(|_| "metaがexportされていない".to_string())?;
        if meta.is_undefined() {
            return Err("metaがexportされていない".into());
        }
        let search: Value = m.get("search").map_err(|_| "searchがexportされていない".to_string())?;
        let Some(search) = search.into_function() else {
            return Err("searchが関数ではない".into());
        };
        c.globals().set("__hikidashi_search", search).map_err(d)?;
        let json = c.json_stringify(meta).map_err(d)?;
        let json = json.map(|s| s.to_string().unwrap_or_default()).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&json).map_err(|_| "metaがオブジェクトではない".to_string())?;
        let keyword = v.get("keyword").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if keyword.is_empty() {
            return Err("meta.keywordがない".into());
        }
        let title = v.get("title").and_then(|x| x.as_str()).unwrap_or(name).to_string();
        let clipboard = v.get("clipboard").and_then(|x| x.as_bool()).unwrap_or(false);
        let calc = v.get("calc").and_then(|x| x.as_bool()).unwrap_or(false);
        Ok(ExtMeta { name: name.to_string(), keyword, title, clipboard, calc })
    });
    guard.disarm();
    Ok(Loaded { meta: r?, stamp, ctx })
}

fn run_search(guard: &Guard, exts: &HashMap<String, Loaded>, keyword: &str, query: &str, clipboard: Option<String>) -> Vec<ExtRow> {
    let kw = keyword.to_lowercase();
    let mut cands: Vec<&Loaded> = exts.values().filter(|l| l.meta.keyword.to_lowercase() == kw).collect();
    cands.sort_by(|a, b| a.meta.name.cmp(&b.meta.name));
    let Some(l) = cands.first() else { return Vec::new() };
    let label = if l.meta.title.is_empty() { l.meta.name.clone() } else { l.meta.title.clone() };
    let cb = if l.meta.clipboard { clipboard.unwrap_or_default() } else { String::new() };
    guard.arm(SEARCH_TIMEOUT);
    let r = l.ctx.with(|c| -> Result<Vec<ExtRow>, String> {
        let d = |e| describe(&c, guard, e, SEARCH_TIMEOUT);
        let f: Function = c.globals().get("__hikidashi_search").map_err(d)?;
        let o = Object::new(c.clone()).map_err(d)?;
        o.set("clipboard", cb).map_err(d)?;
        let v: Value = f.call((query.to_string(), o)).map_err(d)?;
        if !v.is_array() {
            return Err("返り値が配列ではない".into());
        }
        let json = c.json_stringify(v).map_err(d)?;
        let json = json.map(|s| s.to_string().unwrap_or_default()).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        Ok(rows_from_json(&v))
    });
    guard.disarm();
    match r {
        Ok(rows) => rows,
        Err(e) => vec![ExtRow::error(&label, &e)],
    }
}

fn rows_from_json(v: &serde_json::Value) -> Vec<ExtRow> {
    let s = |o: &serde_json::Value, k: &str| o.get(k).and_then(|x| x.as_str()).map(|x| x.to_string());
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|o| {
                    let title = s(o, "title")?;
                    let copy = s(o, "copy").unwrap_or_else(|| title.clone());
                    Some(ExtRow { subtitle: s(o, "subtitle").unwrap_or_default(), url: s(o, "url").filter(|u| !u.is_empty()), title, copy })
                })
                .take(MAX_ROWS)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
