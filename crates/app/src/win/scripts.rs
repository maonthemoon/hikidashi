//! スクリプトのフォルダー(`%APPDATA%\Hikidashi\scripts`)の読み込みと実行. 見出しや出力の読み取りは`hikidashi_core::scripts`.

use super::storage;
use hikidashi_core::scripts::{self, Lang, RunOutcome, Script, ScriptMode, TIMEOUT_SECS};
use std::collections::HashMap;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Globalization::{MultiByteToWideChar, CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// 読む出力の大きさの上限(バイト)
const MAX_OUTPUT: u64 = 4 * 1024 * 1024;
/// 見出しを読むために読むファイルの大きさの上限(バイト)
const MAX_SOURCE: u64 = 256 * 1024;

/// スクリプトが終わった(`run`の結果). wparamは仕事の番号
pub const WM_APP_SCRIPT: u32 = 0x8000 + 41;
/// `list`の結果がそろった
pub const WM_APP_SCRIPT_LIST: u32 = 0x8000 + 42;

pub fn dir() -> PathBuf {
    storage::data_dir().join("scripts")
}

/// フォルダーの見た目(ファイル名と更新日時). 変わったら読み直す. フォルダーがなければ空(作らない)
pub fn signature() -> Vec<(String, Option<SystemTime>)> {
    let Ok(rd) = std::fs::read_dir(dir()) else { return vec![] };
    let mut v: Vec<(String, Option<SystemTime>)> = rd
        .filter_map(|e| e.ok())
        .filter(|e| Lang::of(&e.file_name().to_string_lossy()).is_some())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().and_then(|m| m.modified()).ok()))
        .collect();
    v.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    v
}

/// フォルダーのスクリプトを読む. 使えないもの(`list`でキーワードがないなど)はログに書いて飛ばす
pub fn load() -> Vec<Script> {
    let mut out = vec![];
    for (name, _) in signature() {
        let path = dir().join(&name);
        let mut bytes = vec![];
        if std::fs::File::open(&path).and_then(|f| f.take(MAX_SOURCE).read_to_end(&mut bytes)).is_err() {
            continue;
        }
        let text = scripts::decode(&bytes, system_cp);
        match Script::from_source(&path.to_string_lossy(), &text) {
            Ok(s) => out.push(s),
            Err(e) => storage::log(&format!("scripts: skipped {e}")),
        }
    }
    out
}

/// システムのコードページ(ANSI)で読む
fn system_cp(bytes: &[u8]) -> String {
    unsafe {
        let n = MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
        if n <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut buf = vec![0u16; n as usize];
        let m = MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, Some(&mut buf));
        String::from_utf16_lossy(&buf[..m.max(0) as usize])
    }
}

/// cmdで特別な意味を持つ文字を`^`で打ち消す
fn cmd_escape(arg: &str) -> String {
    let mut o = String::new();
    for c in arg.chars() {
        if "&|<>^()".contains(c) {
            o.push('^');
        }
        o.push(c);
    }
    o
}

fn base(mut c: Command) -> Command {
    c.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).env("PYTHONIOENCODING", "utf-8");
    // 作業フォルダーはユーザーのホーム
    let home = storage::env("USERPROFILE");
    if !home.is_empty() && std::path::Path::new(&home).is_dir() {
        c.current_dir(home);
    }
    c
}

fn spawn(s: &Script, arg: &str) -> Result<Child, String> {
    match s.lang {
        Lang::PowerShell => {
            let mut c = Command::new("powershell.exe");
            c.args(["-NoProfile", "-File"]).arg(&s.path);
            if !arg.is_empty() {
                c.arg(arg);
            }
            base(c).spawn().map_err(|e| e.to_string())
        }
        Lang::Batch => {
            // `cmd.exe /c "<path>" [引数]`. 外側をもう1組の引用符で包み, パスの引用符が残るようにする
            let tail = if arg.is_empty() { String::new() } else { format!(" {}", cmd_escape(arg)) };
            let mut c = Command::new("cmd.exe");
            c.raw_arg(format!("/c \"\"{}\"{}\"", s.path, tail));
            base(c).spawn().map_err(|e| e.to_string())
        }
        Lang::Python => {
            let mut c = Command::new("py");
            c.args(["-3"]).arg(&s.path);
            if !arg.is_empty() {
                c.arg(arg);
            }
            match base(c).spawn() {
                Ok(ch) => Ok(ch),
                // `py`がなければ`python`
                Err(_) => {
                    let mut c = Command::new("python");
                    c.arg(&s.path);
                    if !arg.is_empty() {
                        c.arg(arg);
                    }
                    base(c).spawn().map_err(|e| format!("py and python were not found ({e})"))
                }
            }
        }
    }
}

/// 出力を読み続けるスレッド. 読めた分を共有の入れ物に足す(打ち切ったときも読めた分は使える)
fn reader<R: Read + Send + 'static>(r: R) -> (Arc<Mutex<Vec<u8>>>, std::thread::JoinHandle<()>) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let b = buf.clone();
    let h = std::thread::spawn(move || {
        let mut r = r.take(MAX_OUTPUT);
        let mut chunk = [0u8; 8192];
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => b.lock().unwrap().extend_from_slice(&chunk[..n]),
            }
        }
    });
    (buf, h)
}

/// 実行して, 終わるか30秒たつまで待つ(呼ぶ側は別のスレッド)
pub fn run(s: &Script, arg: &str) -> RunOutcome {
    storage::log(&format!("scripts: run {}", s.path));
    let mut child = match spawn(s, arg) {
        Ok(c) => c,
        Err(e) => return RunOutcome { spawn_error: Some(e), ..Default::default() },
    };
    let (out, out_h) = reader(child.stdout.take().unwrap());
    let (err, err_h) = reader(child.stderr.take().unwrap());
    let start = Instant::now();
    let mut o = RunOutcome::default();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                o.exit_code = Some(st.code().unwrap_or(-1));
                break;
            }
            Ok(None) if start.elapsed() > Duration::from_secs(TIMEOUT_SECS) => {
                let _ = child.kill();
                let _ = child.wait();
                o.timed_out = true;
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                o.spawn_error = Some(e.to_string());
                break;
            }
        }
    }
    // 孫のプロセスが出力の口を持ったままのことがあるので, 読み終わりを待つのは1秒まで
    let until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < until && !(out_h.is_finished() && err_h.is_finished()) {
        std::thread::sleep(Duration::from_millis(10));
    }
    o.stdout = scripts::decode(&out.lock().unwrap(), system_cp);
    o.stderr = scripts::decode(&err.lock().unwrap(), system_cp);
    o
}

/// `list`のスクリプトごとの, 結果の覚え
#[derive(Default)]
pub struct ListState {
    pub cache: Option<Arc<scripts::ListCache>>,
    pub fetched: Option<Instant>,
    pub running: bool,
    pub error: Option<String>,
}

pub type Lists = Arc<Mutex<HashMap<String, ListState>>>;

/// `list`のスクリプトの結果が古い(または一度もない)なら, 裏で動かし直す. 動かしたらtrue
pub fn ensure_list(lists: &Lists, s: &Script, hwnd: HWND) -> bool {
    debug_assert!(s.mode == ScriptMode::List);
    {
        let mut g = lists.lock().unwrap();
        let st = g.entry(s.path.clone()).or_default();
        let stale = st.fetched.is_none_or(|t| t.elapsed().as_secs() >= s.refresh_secs);
        if st.running || !stale {
            return false;
        }
        st.running = true;
    }
    let (lists, s, h) = (lists.clone(), s.clone(), hwnd.0 as isize);
    let _ = std::thread::Builder::new().name("script-list".into()).spawn(move || {
        let o = run(&s, "");
        let mut g = lists.lock().unwrap();
        let st = g.entry(s.path.clone()).or_default();
        st.running = false;
        st.fetched = Some(Instant::now());
        if let Some(e) = &o.spawn_error {
            st.error = Some(e.clone());
        } else if o.timed_out {
            st.error = Some(format!("Stopped after {TIMEOUT_SECS} seconds"));
        } else if o.exit_code.is_some_and(|c| c != 0) {
            let last = scripts::output_lines(&o.stderr).pop().unwrap_or_default();
            st.error = Some(format!("Exit code {}. {}", o.exit_code.unwrap_or(-1), last));
        } else {
            st.error = None;
            st.cache = Some(Arc::new(scripts::ListCache::new(scripts::parse_list(&o.stdout))));
        }
        if let Some(e) = &st.error {
            storage::log(&format!("scripts: list {} failed: {e}", s.path));
        }
        drop(g);
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_SCRIPT_LIST, WPARAM(0), LPARAM(0));
        }
    });
    true
}
