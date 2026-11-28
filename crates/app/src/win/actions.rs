//! 開く, コピー, kill などの実際の操作.

use super::storage::{env, expand, log};
use super::util::{wide, WStr};
use hikidashi_core::attach::{self, SendWait, SendWaitStep};
use hikidashi_core::builtin::SysAction;
use hikidashi_core::config::{Config, OpenWith};
use hikidashi_core::manual;
use hikidashi_core::openwith::{self, Target, ToolKind};
use hikidashi_core::router::url_encode;
use std::ffi::OsString;
use std::os::windows::{ffi::OsStringExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows::core::{w, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HWND, LPARAM, WPARAM, S_FALSE, S_OK};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
use windows::Win32::UI::Shell::{AssocQueryStringW, SHEmptyRecycleBinW, ShellExecuteW, ASSOCF_IS_PROTOCOL, ASSOCF_NOTRUNCATE, ASSOCSTR_EXECUTABLE, SHERB_NOCONFIRMATION, SHERB_NOSOUND};
use windows::Win32::UI::Accessibility::{
    IUIAutomation, IUIAutomationElement, IUIAutomationElementArray, TreeScope_Descendants, UIA_CONTROLTYPE_ID, UIA_ButtonControlTypeId, UIA_ControlTypePropertyId,
    UIA_DocumentControlTypeId, UIA_EditControlTypeId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_BACK, VK_END, VK_RETURN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextW, IsIconic, PostMessageW, SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, SW_SHOWNORMAL,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// ShellExecuteで開く. 失敗したら文を返す.
pub fn shell(verb: &str, file: &str, params: &str, dir: &str, show_hidden: bool) -> Result<(), String> {
    let v = WStr::new(verb);
    let f = WStr::new(file);
    let p = WStr::new(params);
    let d = WStr::new(dir);
    let r = unsafe {
        ShellExecuteW(
            None,
            if verb.is_empty() { PCWSTR::null() } else { v.pcwstr() },
            f.pcwstr(),
            if params.is_empty() { PCWSTR::null() } else { p.pcwstr() },
            if dir.is_empty() { PCWSTR::null() } else { d.pcwstr() },
            if show_hidden { SW_HIDE } else { SW_SHOWNORMAL },
        )
    };
    if r.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("Could not open {file} (error {})", r.0 as isize))
    }
}

/// マニュアルの節を開く. `ShellExecuteW`はfile URLの`#`以降を落とすことがあるので, URLをブラウザーに直接渡す.
pub fn open_manual(path: &Path, id: Option<&str>) -> Result<(), String> {
    if let Some(url) = id.and_then(|id| manual::section_url(path, id)) {
        if default_browser_exe().is_some_and(|exe| std::process::Command::new(exe).arg(url).spawn().is_ok()) {
            return Ok(());
        }
    }
    shell("open", &path.to_string_lossy(), "", "", false)
}

/// Windowsで`https`の`open`に関連付けられた実行ファイルを調べる.
fn default_browser_exe() -> Option<OsString> {
    let flags = ASSOCF_IS_PROTOCOL | ASSOCF_NOTRUNCATE;
    let mut len = 0;
    let result = unsafe { AssocQueryStringW(flags, ASSOCSTR_EXECUTABLE, w!("https"), w!("open"), None, &mut len) };
    if result != S_FALSE || len <= 1 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    let result = unsafe { AssocQueryStringW(flags, ASSOCSTR_EXECUTABLE, w!("https"), w!("open"), Some(PWSTR(buf.as_mut_ptr())), &mut len) };
    if result != S_OK {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0)?;
    let exe = OsString::from_wide(&buf[..end]);
    Path::new(&exe).is_absolute().then_some(exe)
}

/// 既定のアプリで開く(フォルダーはExplorer. D7). `is_dir`は一覧から分かっていれば渡す
/// (WSLのパスは調べるとアクセスで失敗することがあるため. B2)
pub fn open_path(path: &str, cfg: &Config, is_dir: Option<bool>) -> Result<(), String> {
    if !is_wsl_path(path) {
        return shell("open", path, "", "", false);
    }
    // WSLのフォルダーもExplorerで開く. ターミナルで入るのはAlt+Enter(Q6)
    let _ = (cfg, is_dir);
    open_wsl(path)
}

/// `\\wsl.localhost\…`を`\\wsl$\…`の形にする
fn wsl_dollar(path: &str) -> String {
    let l = path.to_ascii_lowercase();
    if l.starts_with("\\\\wsl.localhost\\") {
        format!("\\\\wsl$\\{}", &path["\\\\wsl.localhost\\".len()..])
    } else {
        path.to_string()
    }
}

/// WSLのファイルを開く. 会社PCでShellExecuteが`error 5`(アクセス拒否)になったので,
/// `\\wsl$\`の形でもう一度, それでもだめならExplorerに渡す(B2)
fn open_wsl(path: &str) -> Result<(), String> {
    let first = match shell("open", path, "", "", false) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    super::storage::log(&format!("open retry: {first}"));
    let alt = wsl_dollar(path);
    if alt != path && shell("open", &alt, "", "", false).is_ok() {
        return Ok(());
    }
    std::process::Command::new("explorer.exe").arg(path).spawn().map(|_| ()).map_err(|e| format!("{first}; explorer: {e}"))
}

/// アプリを起動する. ストアのアプリ(`shell:AppsFolder\…`)はExplorerに渡す
/// (会社PCでShellExecuteの`open`が`error 31`になったため)
pub fn launch_app(target: &str) -> Result<(), String> {
    if target.to_ascii_lowercase().starts_with("shell:") {
        return std::process::Command::new("explorer.exe").arg(target).spawn().map(|_| ()).map_err(|e| format!("Could not open {target}: {e}"));
    }
    shell("open", target, "", "", false)
}

pub fn is_wsl_path(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    l.starts_with("\\\\wsl.localhost\\") || l.starts_with("\\\\wsl$\\")
}

/// 管理者として実行(C2)
pub fn run_as_admin(path: &str, args: &str) -> Result<(), String> {
    shell("runas", path, args, "", false)
}

/// 入っているフォルダーを開き, そのファイルを選んだ状態にする(D7)
pub fn reveal(path: &str) -> Result<(), String> {
    shell("open", "explorer.exe", &format!("/select,\"{path}\""), "", false)
}

pub fn copy_text(hwnd: HWND, text: &str) -> Result<(), String> {
    let data = wide(text);
    unsafe {
        OpenClipboard(Some(hwnd)).map_err(|e| e.message())?;
        let _ = EmptyClipboard();
        let bytes = data.len() * 2;
        let h = match GlobalAlloc(GMEM_MOVEABLE, bytes) {
            Ok(h) => h,
            Err(e) => {
                let _ = CloseClipboard();
                return Err(e.message());
            }
        };
        let p = GlobalLock(h) as *mut u16;
        if !p.is_null() {
            std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
            let _ = GlobalUnlock(h);
        }
        if SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(h.0))).is_err() {
            let _ = GlobalFree(Some(h));
        }
        let _ = CloseClipboard();
    }
    Ok(())
}

fn first_existing(cands: &[PathBuf]) -> Option<PathBuf> {
    cands.iter().find(|p| p.is_file()).cloned()
}

/// ページを開くブラウザー(ChromeとEdgeは窓かタブか, プロフィールを選べる)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Edge,
}

impl Browser {
    /// ブックマークの`args`に入れる名前
    pub fn key(self) -> &'static str {
        match self {
            Browser::Chrome => "chrome",
            Browser::Edge => "edge",
        }
    }

    pub fn from_key(k: &str) -> Option<Browser> {
        match k {
            "chrome" => Some(Browser::Chrome),
            "edge" => Some(Browser::Edge),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Browser::Chrome => "Chrome",
            Browser::Edge => "Edge",
        }
    }

    /// User Dataのフォルダー(ブックマークとプロフィールの一覧がある)
    pub fn user_data(self, cfg: &Config) -> PathBuf {
        match self {
            Browser::Chrome if !cfg.browser.chrome_user_data.is_empty() => PathBuf::from(expand(&cfg.browser.chrome_user_data)),
            Browser::Chrome => PathBuf::from(env("LOCALAPPDATA")).join(r"Google\Chrome\User Data"),
            Browser::Edge => PathBuf::from(env("LOCALAPPDATA")).join(r"Microsoft\Edge\User Data"),
        }
    }

    pub fn profile(self, cfg: &Config) -> &str {
        match self {
            Browser::Chrome => &cfg.browser.chrome_profile,
            Browser::Edge => &cfg.browser.edge_profile,
        }
    }

    /// exeの場所. 見つからなければNone
    pub fn exe(self, cfg: &Config) -> Option<PathBuf> {
        let (set, rel) = match self {
            Browser::Chrome => (&cfg.browser.chrome_exe, r"Google\Chrome\Application\chrome.exe"),
            Browser::Edge => (&cfg.browser.edge_exe, r"Microsoft\Edge\Application\msedge.exe"),
        };
        if !set.is_empty() {
            let p = PathBuf::from(expand(set));
            return p.is_file().then_some(p);
        }
        first_existing(&[
            PathBuf::from(env("ProgramFiles(x86)")).join(rel),
            PathBuf::from(env("ProgramFiles")).join(rel),
            PathBuf::from(env("LOCALAPPDATA")).join(rel),
        ])
    }
}

/// Windowsの既定のブラウザー(httpsを開くアプリ). ChromeとEdge以外はNone
pub fn default_browser() -> Option<Browser> {
    let id = super::system::reg_string(r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice", "ProgId")?;
    let l = id.to_ascii_lowercase();
    if l.starts_with("chrome") {
        Some(Browser::Chrome)
    } else if l.starts_with("msedge") {
        Some(Browser::Edge)
    } else {
        None
    }
}

/// 設定で選んだブラウザー. `default`なら既定のブラウザー(ChromeかEdgeでなければNone)
pub fn chosen_browser(cfg: &Config) -> Option<Browser> {
    match cfg.browser.browser.as_str() {
        "chrome" => Some(Browser::Chrome),
        "edge" => Some(Browser::Edge),
        _ => default_browser(),
    }
}

/// URLを設定のブラウザーで開く(E1, E4). 新しい窓か新しいタブかは設定で選ぶ
pub fn open_url(url: &str, cfg: &Config) -> Result<(), String> {
    open_url_with(url, cfg, chosen_browser(cfg))
}

/// URLをそのブラウザーで開く(ブックマークは読んだブラウザーで開く). Noneや見つからなければ既定のブラウザー
pub fn open_url_with(url: &str, cfg: &Config, browser: Option<Browser>) -> Result<(), String> {
    if let Some(b) = browser {
        if let Some(exe) = b.exe(cfg) {
            let mut c = std::process::Command::new(exe);
            if cfg.browser.open_in != "tab" {
                c.arg("--new-window");
            }
            let profile = b.profile(cfg);
            if !profile.is_empty() {
                c.arg(format!("--profile-directory={profile}"));
            }
            if c.arg(url).creation_flags(CREATE_NO_WINDOW).spawn().is_ok() {
                return Ok(());
            }
        }
    }
    shell("open", url, "", "", false)
}

/// ChatGPTに質問する(F2): `?q=`で開くか, コピーしてChatGPTを開く
pub fn ask_chatgpt(hwnd: HWND, q: &str, cfg: &Config) -> Result<(), String> {
    if cfg.chatgpt.mode == "clipboard" {
        copy_text(hwnd, q)?;
        open_url(&cfg.chatgpt.home, cfg)
    } else {
        let previous_pages = cfg.chatgpt.send_enter.then(chatgpt_pages);
        let started = Instant::now();
        open_url(&cfg.chatgpt.url.replace("{q}", &url_encode(q)), cfg)?;
        if let Some(previous_pages) = previous_pages {
            send_enter_to_chatgpt(started, cfg.chatgpt.send_delay_ms, previous_pages);
        }
        Ok(())
    }
}

/// クリップボードの文字(なければ, 文字でなければNone)
pub fn clipboard_text(hwnd: HWND) -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return None;
        }
        OpenClipboard(Some(hwnd)).ok()?;
        let mut out = None;
        if let Ok(h) = GetClipboardData(CF_UNICODETEXT.0 as u32) {
            let hg = windows::Win32::Foundation::HGLOBAL(h.0);
            let p = GlobalLock(hg) as *const u16;
            if !p.is_null() {
                let mut n = 0usize;
                while *p.add(n) != 0 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                let _ = GlobalUnlock(hg);
            }
        }
        let _ = CloseClipboard();
        out
    }
}

/// クリップボードを元に戻す頼み(貼り付けたあと. K22). `lparam`は`Box<ClipRestore>`
pub const WM_APP_CLIP_RESTORE: u32 = 0x8000 + 6;

pub struct ClipRestore {
    /// 貼り付けに使った文字. クリップボードがまだこれなら戻す(その間にほかの物をコピーしていたら戻さない)
    pub ours: String,
    pub prev: String,
}

/// `?q=`に入れる長さの上限(URLを符号にした長さ). 長すぎるURLはサーバーに断られることがあるので, これを超えたら貼り付ける
const URL_Q_MAX: usize = 12000;

/// 長い質問(K22)を送る(Q4のA). `mode = "clipboard"`ならコピーして開くだけ. `send_enter = false`ならEnterを押さない.
/// - URLに入る長さなら, 1行の質問と同じく`?q=`で開き, ChatGPTの窓を確かめてEnterを押す(改行は`%0A`).
/// - 入らなければ, `?q=.`(1文字)で開いて入力欄にカーソルを置かせ, 文の終わりに移って(Ctrl+End)`.`を消し(Backspace), 全体を貼って(Ctrl+V)Enterを押す.
///   `?q=`なしで開くと入力欄にカーソルがなく, 貼り付けが空振りした. 頭の300字を`?q=`に入れてCtrl+Aで貼り替える形は,
///   Ctrl+Aで全部が選ばれず, 頭の300字の途中に全体が入って重なって送られた(どちらも2026-11-24の会社PCでの確認).
///   1文字なら, うまくいかなくても`.`が残るだけで済む.
pub fn ask_chatgpt_long(hwnd: HWND, text: &str, cfg: &Config) -> Result<(), String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if cfg.chatgpt.mode == "clipboard" {
        copy_text(hwnd, &text.replace('\n', "\r\n"))?;
        return open_url(&cfg.chatgpt.home, cfg);
    }
    let enc = url_encode(&text);
    if enc.len() <= URL_Q_MAX {
        let previous_pages = cfg.chatgpt.send_enter.then(chatgpt_pages);
        let started = Instant::now();
        open_url(&cfg.chatgpt.url.replace("{q}", &enc), cfg)?;
        super::storage::log(&format!("chatgpt: long question by URL ({} chars)", text.chars().count()));
        if let Some(previous_pages) = previous_pages {
            send_enter_to_chatgpt(started, cfg.chatgpt.send_delay_ms, previous_pages);
        }
        return Ok(());
    }
    let prev = clipboard_text(hwnd);
    let ours = text.replace('\n', "\r\n");
    copy_text(hwnd, &ours)?;
    let previous_pages = chatgpt_pages();
    let started = Instant::now();
    open_url(&cfg.chatgpt.url.replace("{q}", "."), cfg)?;
    let (h, delay, enter) = (hwnd.0 as isize, cfg.chatgpt.send_delay_ms, cfg.chatgpt.send_enter);
    std::thread::spawn(move || {
        let not_pasted = |front| {
            log(&format!("chatgpt: not pasted (window in front: {front}). The question is left on the clipboard"));
        };
        let gpt = match wait_chatgpt_open(started, delay, &previous_pages) {
            Ok(gpt) => gpt,
            Err(front) => return not_pasted(front),
        };
        focus_chatgpt_input(gpt);
        // `.`を消して, 全体を貼る
        for (keys, pause_ms) in [(&[VK_CONTROL, VK_END][..], 0), (&[VK_BACK][..], 150), (&[VK_CONTROL, VK_V][..], 150)] {
            if pause_ms > 0 {
                sleep_ms(pause_ms);
            }
            if let Err(front) = chatgpt_in_front(Some(gpt)) {
                not_pasted(front);
                return;
            }
            press(keys);
        }
        let pasted = Instant::now();
        log(&format!("chatgpt: pasted a long question ({} chars) after {} ms", ours.chars().count(), elapsed_ms(started)));
        if enter {
            wait_send_button(gpt, pasted, SendWait::after_paste(), 5000, 800, 800);
            send_chatgpt_enter(gpt, started);
        }
        // 貼り付けが終わってから戻す
        sleep_ms(1500);
        if let Some(prev) = prev {
            let b = Box::new(ClipRestore { ours, prev });
            unsafe {
                // 届かなければ捨てる(戻さない)
                let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_CLIP_RESTORE, WPARAM(0), LPARAM(Box::into_raw(b) as isize));
            }
        }
    });
    Ok(())
}

/// 画像つきの質問を送り終えた知らせ(K22). `lparam`は`Box<ImagesDone>`
pub const WM_APP_IMAGES_DONE: u32 = 0x8000 + 8;

pub struct ImagesDone {
    /// 送る前のクリップボード. クリップボードがまだ`ours`なら戻す
    pub saved: Option<super::clipboard::Saved>,
    pub ours: u32,
    /// ChatGPTの窓にならず, 画像を貼れなかった(最後の画像をクリップボードに残した)
    pub failed: bool,
}

const CHATGPT_POLL_MS: u64 = 250;

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn chatgpt_in_front(expected: Option<HWND>) -> Result<(HWND, String), String> {
    let hwnd = unsafe { GetForegroundWindow() };
    let title = window_text(hwnd);
    let held = [VK_SHIFT, VK_CONTROL, VK_MENU].iter().any(|k| unsafe { GetAsyncKeyState(k.0 as i32) } < 0);
    if title.contains("ChatGPT") && !held && expected.is_none_or(|gpt| gpt == hwnd) {
        Ok((hwnd, title))
    } else {
        Err(title)
    }
}

/// 前の窓がChatGPTで, 修飾キーが離れるまで250ミリ秒ごとに確かめる. だめなら前の窓のタイトルを返す
fn wait_chatgpt(limit_ms: u64, expected: Option<HWND>) -> Result<HWND, String> {
    let started = Instant::now();
    loop {
        let front = match chatgpt_in_front(expected) {
            Ok((hwnd, _)) => return Ok(hwnd),
            Err(title) => title,
        };
        let remaining = limit_ms.saturating_sub(elapsed_ms(started));
        if remaining == 0 {
            return Err(front);
        }
        sleep_ms(CHATGPT_POLL_MS.min(remaining));
    }
}

/// URLを開く前に, ChatGPTの最上位の窓とその時点のページのタイトルを記録する
fn chatgpt_pages() -> Vec<(isize, String)> {
    unsafe extern "system" fn record(hwnd: HWND, param: LPARAM) -> BOOL {
        let pages = unsafe { &mut *(param.0 as *mut Vec<(isize, String)>) };
        let title = window_text(hwnd);
        if title.contains("ChatGPT") {
            pages.push((hwnd.0 as isize, title));
        }
        true.into()
    }

    let mut pages = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(record), LPARAM(&mut pages as *mut Vec<(isize, String)> as isize));
    }
    pages
}

fn wait_chatgpt_open(started: Instant, delay_ms: u32, previous_pages: &[(isize, String)]) -> Result<HWND, String> {
    let delay_ms = u64::from(delay_ms);
    let limit_ms = delay_ms + 10_000;
    loop {
        let front = chatgpt_in_front(None);
        let elapsed = elapsed_ms(started);
        let title = match front {
            Ok((hwnd, title)) if attach::chatgpt_page_ready(previous_pages, hwnd.0 as isize, &title, elapsed, delay_ms) => {
                log(&format!("chatgpt: ChatGPT in front after {elapsed} ms"));
                return Ok(hwnd);
            }
            Ok((_, title)) | Err(title) => title,
        };
        let remaining = limit_ms.saturating_sub(elapsed);
        if remaining == 0 {
            return Err(title);
        }
        sleep_ms(CHATGPT_POLL_MS.min(remaining));
    }
}

/// 画像を添付した長い質問(K22)を送る. 文字は`?q=`に入れずにChatGPTを開き(K73), 入力欄を選んで文字と画像を貼る.
/// 最後の画像のあと, 送信ボタンが押せるようになったらEnterを押す(`send_enter`がオフなら押さない).
/// 終わったらクリップボードを元に戻す. ChatGPTの窓にならなければ貼らず, 最後の画像をクリップボードに残す.
/// - `?q=`で開くと, ChatGPTによっては開いたとたんに文字だけを送ってしまう(確認17のI2. 個人のアカウント). `?q=.`も`.`が送られるので使わない.
/// - `?q=`なしで開くと入力欄にカーソルがないことがある(2026-11-24の会社PC)ので, UI Automationで入力欄(`#prompt-textarea`)を探して選んでから貼る(K76).
///   ChatGPTの「入力欄に移る」キー(`Shift+Esc`)は, Edgeでは「ブラウザー タスク マネージャー」が開いて使えなかった(確認18のI2).
/// - 貼るたびに少し待つ. ブラウザーがクリップボードを読む前に次の画像に入れ替えると, 同じ画像が2回入ったり抜けたりするため.
/// - `mode = "clipboard"`なら, 文字(なければ最後の画像)をコピーして開くだけ.
pub fn ask_chatgpt_images(hwnd: HWND, text: &str, images: &[attach::Image], cfg: &Config) -> Result<(), String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let Some(last_img) = images.last() else { return ask_chatgpt_long(hwnd, &text, cfg) };
    if cfg.chatgpt.mode == "clipboard" {
        if text.trim().is_empty() {
            super::clipboard::set_image(hwnd, last_img).ok_or("Could not copy the image to the clipboard")?;
        } else {
            copy_text(hwnd, &text.replace('\n', "\r\n"))?;
        }
        return open_url(&cfg.chatgpt.home, cfg);
    }
    // 送る前のクリップボード(画像のことも多い)を写しておく
    let saved = super::clipboard::save(hwnd);
    let previous_pages = chatgpt_pages();
    let started = Instant::now();
    open_url(&attach::url_without_q(&cfg.chatgpt.url), cfg)?;
    super::storage::log(&format!("chatgpt: question with {} image(s) ({} chars)", images.len(), text.chars().count()));
    let images = images.to_vec();
    let (h, delay, enter, per) = (hwnd.0 as isize, cfg.chatgpt.send_delay_ms, cfg.chatgpt.send_enter, cfg.chatgpt.image_wait_ms);
    std::thread::spawn(move || {
        let hwnd = HWND(h as *mut _);
        let done = |d: ImagesDone| unsafe {
            // 届かなければ捨てる(戻さない)
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_IMAGES_DONE, WPARAM(0), LPARAM(Box::into_raw(Box::new(d)) as isize));
        };
        // 貼れなかった. 最後の画像をクリップボードに残す(自分で貼れるように)
        let fail = |why: String, saved: Option<super::clipboard::Saved>| {
            super::storage::log(&format!("chatgpt: images not pasted ({why})"));
            let ours = super::clipboard::set_image(hwnd, images.last().unwrap()).unwrap_or(0);
            done(ImagesDone { saved, ours, failed: true });
        };
        // 貼ったChatGPTのウィンドウ. 待つ間にほかへ移っても, Enterの前にここへ戻す
        let gpt = match wait_chatgpt_open(started, delay, &previous_pages) {
            Ok(gpt) => gpt,
            Err(front) => return fail(format!("window in front: {front}"), saved),
        };
        focus_chatgpt_input(gpt);
        if !text.trim().is_empty() {
            if let Err(e) = copy_text(hwnd, &text.replace('\n', "\r\n")) {
                fail(e, saved);
                return;
            }
            sleep_ms(150);
            if let Err(front) = chatgpt_in_front(Some(gpt)) {
                return fail(format!("before text, window in front: {front}"), saved);
            }
            press(&[VK_CONTROL, VK_V]);
            log(&format!("chatgpt: pasted a long question ({} chars) after {} ms", text.chars().count(), elapsed_ms(started)));
            sleep_ms(800);
        }
        let mut ours = 0;
        for (i, img) in images.iter().enumerate() {
            if i > 0 {
                sleep_ms(1000);
            }
            if let Err(front) = wait_chatgpt(5000, Some(gpt)) {
                fail(format!("before image {}, window in front: {front}", i + 1), saved);
                return;
            }
            let Some(seq) = super::clipboard::set_image(hwnd, img) else {
                fail(format!("could not put image {} on the clipboard", i + 1), saved);
                return;
            };
            ours = seq;
            sleep_ms(150);
            if let Err(front) = chatgpt_in_front(Some(gpt)) {
                return fail(format!("before image {}, window in front: {front}", i + 1), saved);
            }
            press(&[VK_CONTROL, VK_V]);
        }
        let pasted = Instant::now();
        log(&format!("chatgpt: pasted {} image(s) after {} ms", images.len(), elapsed_ms(started)));
        if enter {
            let fallback_ms = attach::wait_ms(images.len(), per);
            wait_send_button(gpt, pasted, SendWait::after_paste(), fallback_ms + 5000, 2000, fallback_ms);
            // 待つ間にほかのウィンドウに移っていたら, ChatGPTを前に戻してからEnterを押す(確認16のI4のB)
            match bring_chatgpt_back(gpt) {
                Ok(()) => send_chatgpt_enter(gpt, started),
                Err(front) => log(&format!("chatgpt: Enter not sent (window in front: {front})")),
            }
        } else {
            sleep_ms(2000);
            log_send_button_check(gpt);
        }
        // 貼り付けが終わってから戻す
        sleep_ms(1500);
        done(ImagesDone { saved, ours, failed: false });
    });
    Ok(())
}

/// 画像を貼ったあと, 前のウィンドウがChatGPTでなければ, 貼ったChatGPTのウィンドウを前に戻す(確認16のI4のB).
/// 打っている途中の文字がChatGPTに入らないよう, キーボードとマウスが1.5秒止まるのを待ってから戻す(最大30秒).
/// 戻したあともタイトルにChatGPTがなければ(同じウィンドウでほかのタブに移ったなど), Errで何もしない
fn bring_chatgpt_back(gpt: HWND) -> Result<(), String> {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::UI::WindowsAndMessaging::{BringWindowToTop, GetWindowThreadProcessId, IsWindow};
    if chatgpt_in_front(Some(gpt)).is_ok() {
        return Ok(());
    }
    if !unsafe { IsWindow(Some(gpt)) }.as_bool() {
        return Err("the ChatGPT window was closed".into());
    }
    let idle_ms = || unsafe {
        let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        let _ = GetLastInputInfo(&mut li);
        GetTickCount().wrapping_sub(li.dwTime)
    };
    for _ in 0..120 {
        if idle_ms() >= 1500 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    unsafe {
        // 前にいないプロセスはSetForegroundWindowが効かないので, 今前にいるウィンドウの入力に一時的につなぐ
        let me = GetCurrentThreadId();
        let other = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let attached = other != 0 && other != me && AttachThreadInput(me, other, true).as_bool();
        if IsIconic(gpt).as_bool() {
            let _ = ShowWindow(gpt, SW_RESTORE);
        }
        let _ = SetForegroundWindow(gpt);
        let _ = BringWindowToTop(gpt);
        if attached {
            let _ = AttachThreadInput(me, other, false);
        }
    }
    super::storage::log("chatgpt: brought ChatGPT back to the front");
    std::thread::sleep(std::time::Duration::from_millis(400));
    wait_chatgpt(2000, Some(gpt)).map(|_| ())
}

/// UI Automationは裏のスレッドで使い, 探し直す間は同じインスタンスを使う
fn with_chatgpt_automation<T>(run: impl FnOnce(windows::core::Result<IUIAutomation>) -> T) -> T {
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::UI::Accessibility::CUIAutomation;
    unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_MULTITHREADED).ok() {
            return run(Err(e));
        }
        let result = run(CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER));
        CoUninitialize();
        result
    }
}

fn chatgpt_page(uia: &IUIAutomation, gpt: HWND) -> windows::core::Result<Option<IUIAutomationElement>> {
    use windows::Win32::System::Variant::VARIANT;
    unsafe {
        let root = uia.ElementFromHandle(gpt)?;
        let doc = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_DocumentControlTypeId.0))?;
        match root.FindFirst(TreeScope_Descendants, &doc) {
            Ok(page) => Ok(Some(page)),
            // FindFirstが見つからなかったときのNULLは, windowsクレートでは空のエラーになる
            Err(e) if e.code() == windows::core::Error::empty().code() => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn chatgpt_page_controls(uia: &IUIAutomation, gpt: HWND, kind: UIA_CONTROLTYPE_ID) -> windows::core::Result<Option<IUIAutomationElementArray>> {
    use windows::Win32::System::Variant::VARIANT;
    unsafe {
        let Some(page) = chatgpt_page(uia, gpt)? else { return Ok(None) };
        let control = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(kind.0))?;
        page.FindAll(TreeScope_Descendants, &control).map(Some)
    }
}

/// ChatGPTの入力欄を探して選ぶ(K76). ページの中で知っているIDの入力欄を選び, なければ入力欄が1つのときだけ選ぶ.
/// 2026-11-28のEdge(ログインなし)は`mobile-composer-prompt`(名前`Chat with ChatGPT`)だった. 前は`prompt-textarea`.
/// ブラウザーが読み上げ用の木を作るまで時間がかかることがあるので, 8秒まで探し直す.
/// 見つからなければ今の焦点と入力欄の一覧をログに残し, ページが入力欄を選んでいることに任せて貼る
fn focus_chatgpt_input(gpt: HWND) {
    const IDS: [&str; 2] = ["prompt-textarea", "mobile-composer-prompt"];
    let started = Instant::now();
    let result = with_chatgpt_automation(|uia| unsafe {
        let uia = uia.map_err(|e| e.to_string())?;
        let id_of = |e: &IUIAutomationElement| e.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default();
        let desc = |e: &IUIAutomationElement| {
            let name: String = e.CurrentName().map(|s| s.to_string()).unwrap_or_default().chars().take(40).collect();
            format!("type {} name '{}' id '{}'", e.CurrentControlType().map(|c| c.0).unwrap_or(0), name, id_of(e))
        };
        let mut edits: Vec<IUIAutomationElement> = vec![];
        loop {
            edits.clear();
            if let Ok(Some(all)) = chatgpt_page_controls(&uia, gpt, UIA_EditControlTypeId) {
                for i in 0..all.Length().unwrap_or(0) {
                    if let Ok(e) = all.GetElement(i) {
                        edits.push(e);
                    }
                }
            }
            let pick = edits.iter().find(|e| IDS.contains(&id_of(e).as_str())).or(if edits.len() == 1 { edits.first() } else { None });
            if let Some(el) = pick {
                chatgpt_in_front(Some(gpt)).map_err(|front| format!("window in front: {front}"))?;
                el.SetFocus().map_err(|e| e.to_string())?;
                return Ok(desc(el));
            }
            let remaining = 8000u64.saturating_sub(elapsed_ms(started));
            if remaining == 0 {
                break;
            }
            sleep_ms(CHATGPT_POLL_MS.min(remaining));
        }
        let focused = uia.GetFocusedElement().map(|e| desc(&e)).unwrap_or_default();
        let list: Vec<String> = edits.iter().take(5).map(|e| desc(e)).collect();
        Err(format!("not found. focused: {focused}. edits in the page: [{}]", list.join("; ")))
    });
    match result {
        Ok(what) => log(&format!("chatgpt: focused the input box after {} ms ({what})", elapsed_ms(started))),
        Err(e) => log(&format!("chatgpt: input box not focused after {} ms ({e})", elapsed_ms(started))),
    }
}

/// ページ内の送信ボタンをIDで探し, なければ名前で探す
fn find_send_button(uia: &IUIAutomation, gpt: HWND) -> windows::core::Result<Option<bool>> {
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{UIA_AutomationIdPropertyId, UIA_NamePropertyId};
    const ID: &str = "composer-submit-button";
    const NAMES: [&str; 4] = ["Send prompt", "Send message", "プロンプトを送信する", "送信"];
    // ボタンを全部取って1つずつ調べると, 長い会話のページでは1回に数秒かかった(確認20). 条件をまとめて1回で探す
    let Some(page) = chatgpt_page(uia, gpt)? else { return Ok(None) };
    unsafe {
        let button = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_ButtonControlTypeId.0))?;
        let mut keys = vec![Some(uia.CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(ID))?)];
        for name in NAMES {
            keys.push(Some(uia.CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(name))?));
        }
        let cond = uia.CreateAndCondition(&button, &uia.CreateOrConditionFromNativeArray(&keys)?)?;
        match page.FindFirst(TreeScope_Descendants, &cond) {
            Ok(found) => Ok(Some(found.CurrentIsEnabled()?.as_bool())),
            Err(e) if e.code() == windows::core::Error::empty().code() => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// 送信ボタンが見つからなかったとき, ページのボタンの終わりの方(入力欄の近く)をログに書く(確認20のP2)
fn log_page_buttons(uia: &IUIAutomation, gpt: HWND) {
    let list = unsafe {
        let Ok(Some(all)) = chatgpt_page_controls(uia, gpt, UIA_ButtonControlTypeId) else {
            log("chatgpt: page buttons: no page");
            return;
        };
        let n = all.Length().unwrap_or(0);
        (n.saturating_sub(20)..n)
            .filter_map(|i| all.GetElement(i).ok())
            .map(|b| {
                let name: String = b.CurrentName().map(|s| s.to_string()).unwrap_or_default().chars().take(30).collect();
                let id = b.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default();
                let on = b.CurrentIsEnabled().map(|e| e.as_bool()).unwrap_or(false);
                format!("'{name}' id '{id}'{}", if on { "" } else { " (disabled)" })
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    log(&format!("chatgpt: page buttons (last 20): {list}"));
}

/// `send_enter = false`のとき(e2e), 送信ボタンが見つかるかだけをログに書く
fn log_send_button_check(gpt: HWND) {
    with_chatgpt_automation(|uia| {
        let Ok(uia) = uia else { return };
        let t = Instant::now();
        let r = find_send_button(&uia, gpt);
        log(&format!("chatgpt: send button check: {} ({} ms)", match &r {
            Ok(Some(true)) => "ready".to_string(),
            Ok(Some(false)) => "disabled".to_string(),
            Ok(None) => "not found".to_string(),
            Err(e) => format!("error {e}"),
        }, elapsed_ms(t)));
        if !matches!(r, Ok(Some(_))) {
            log_page_buttons(&uia, gpt);
        }
    });
}

fn wait_send_button(gpt: HWND, started: Instant, mut wait: SendWait, limit_ms: u64, missing_ms: u64, fallback_ms: u64) -> SendWaitStep {
    let mut slowest = 0u64;
    with_chatgpt_automation(|uia| loop {
        let t = Instant::now();
        let enabled = uia.as_ref().ok().and_then(|uia| find_send_button(uia, gpt).ok().flatten());
        slowest = slowest.max(elapsed_ms(t));
        let elapsed = elapsed_ms(started);
        let step = wait.step(elapsed, enabled, limit_ms, missing_ms);
        match step {
            SendWaitStep::Wait { until_ms } => sleep_ms(CHATGPT_POLL_MS.min(until_ms - elapsed)),
            SendWaitStep::Ready => {
                log(&format!("chatgpt: send button ready after {elapsed} ms (slowest search {slowest} ms)"));
                return step;
            }
            SendWaitStep::Fallback => {
                let remaining = fallback_ms.saturating_sub(elapsed);
                if remaining > 0 {
                    sleep_ms(remaining);
                }
                log(&format!("chatgpt: send button not found, waited {} ms (slowest search {slowest} ms)", elapsed_ms(started)));
                if let Ok(uia) = uia.as_ref() {
                    log_page_buttons(uia, gpt);
                }
                return step;
            }
            SendWaitStep::TimedOut => {
                log(&format!("chatgpt: send button wait timed out after {elapsed} ms (slowest search {slowest} ms)"));
                if let Ok(uia) = uia.as_ref() {
                    log_page_buttons(uia, gpt);
                }
                return step;
            }
        }
    })
}

/// 修飾キーと一緒にキーを押して離す(例: Ctrl+V)
fn press(keys: &[VIRTUAL_KEY]) {
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
    };
    let mut inputs: Vec<INPUT> = keys.iter().map(|k| key(*k, false)).collect();
    inputs.extend(keys.iter().rev().map(|k| key(*k, true)));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// `?q=`で入力欄に入った質問を送る(Q4). ボタンが押せるか, 見つからないまま`delay_ms`たったらEnterを押す
fn send_enter_to_chatgpt(started: Instant, delay_ms: u32, previous_pages: Vec<(isize, String)>) {
    std::thread::spawn(move || {
        let gpt = match wait_chatgpt_open(started, delay_ms, &previous_pages) {
            Ok(gpt) => gpt,
            Err(front) => {
                log(&format!("chatgpt: Enter not sent (window in front: {front})"));
                return;
            }
        };
        let in_front = Instant::now();
        let delay_ms = u64::from(delay_ms);
        let limit_ms = delay_ms + 10_000;
        if wait_send_button(gpt, in_front, SendWait::default(), limit_ms, delay_ms, delay_ms) == SendWaitStep::TimedOut {
            return;
        }
        // Shift+Enterのあとなどで修飾キーが押されたままなら, 離すまで待つ
        match wait_chatgpt(limit_ms.saturating_sub(elapsed_ms(in_front)), Some(gpt)) {
            Ok(_) => send_chatgpt_enter(gpt, started),
            Err(front) => log(&format!("chatgpt: Enter not sent (window in front: {front})")),
        }
    });
}

fn window_text(h: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(h, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}

/// 念のため, 間を空けてEnterを2回押し直す(K27). ChatGPTがページを用意する前に押したEnterは効かず,
/// 質問が入力欄に入ったまま止まった(2026-11-24の会社PC. ログは`sent Enter`). 送れたあとの入力欄は空なので,
/// 押し直しても何も起きない. ChatGPTが前にないときや修飾キーが押されているときは押さない
fn enter_again(gpt: HWND) {
    for wait in [1500u64, 2500] {
        sleep_ms(wait);
        if let Err(front) = chatgpt_in_front(Some(gpt)) {
            log(&format!("chatgpt: Enter not repeated (window in front: {front})"));
            return;
        }
        press_enter();
    }
    super::storage::log("chatgpt: sent Enter again twice");
}

fn send_chatgpt_enter(gpt: HWND, started: Instant) {
    if let Err(front) = chatgpt_in_front(Some(gpt)) {
        log(&format!("chatgpt: Enter not sent (window in front: {front})"));
        return;
    }
    press_enter();
    log(&format!("chatgpt: sent Enter after {} ms", elapsed_ms(started)));
    enter_again(gpt);
}

fn press_enter() {
    let key = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_RETURN, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
    };
    let inputs = [key(false), key(true)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn google(q: &str, cfg: &Config) -> Result<(), String> {
    open_url(&cfg.google.url.replace("{q}", &url_encode(q)), cfg)
}

/// 自作コマンドの実行(H2). cmd.exeに渡す(窓は出さない)
pub fn run_command(cmd: &str) -> Result<(), String> {
    // ファイルやフォルダーを開くだけなら, 検索の結果と同じように直接開く(速い)
    if let Some(p) = hikidashi_core::pathlike::command_target(cmd) {
        let p = expand(&p);
        if is_wsl_path(&p) {
            return open_wsl(&p);
        }
        // ネットワークのパスは, ないときに調べると待たされるので, そのまま開く
        if p.starts_with("\\\\") || std::path::Path::new(&p).exists() {
            return shell("open", &p, "", "", false);
        }
    }
    std::process::Command::new("cmd.exe")
        .raw_arg(format!("/c {cmd}"))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not run {cmd}: {e}"))
}

/// 「〜で開く」のexe(Q5). `exe`が空なら名前から探す. 入っていなければNone(メニューに出さない)
pub fn find_exe(ow: &OpenWith) -> Option<PathBuf> {
    let exe = ow.exe.trim();
    if !exe.is_empty() {
        let p = PathBuf::from(expand(exe));
        if p.is_absolute() || exe.contains('\\') {
            return p.exists().then_some(p);
        }
        return on_path(exe);
    }
    let local = PathBuf::from(env("LOCALAPPDATA"));
    let pf = PathBuf::from(env("ProgramFiles"));
    match ToolKind::of(ow) {
        ToolKind::VsCode => first_existing(&[local.join(r"Programs\Microsoft VS Code\Code.exe"), pf.join(r"Microsoft VS Code\Code.exe")]),
        ToolKind::WezTerm => first_existing(&[pf.join(r"WezTerm\wezterm-gui.exe"), local.join(r"Programs\WezTerm\wezterm-gui.exe")]),
        // ストア版は`WindowsApps`の下に呼び出し用の`wt.exe`がある
        ToolKind::WindowsTerminal => first_existing(&[local.join(r"Microsoft\WindowsApps\wt.exe")]).or_else(|| on_path("wt.exe")),
        ToolKind::Other => None,
    }
}

/// `PATH`のフォルダーからexeを探す
fn on_path(name: &str) -> Option<PathBuf> {
    let name = if name.to_ascii_lowercase().ends_with(".exe") { name.to_string() } else { format!("{name}.exe") };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&name)).find(|p| p.exists())
}

/// 「〜で開く」で開く(Q5). `folder_only`ならファイルでもそのフォルダー(ターミナルで入るとき. Q6)
pub fn open_with(ow: &OpenWith, path: &str, is_dir: bool, folder_only: bool) -> Result<(), String> {
    let exe = find_exe(ow).ok_or_else(|| format!("{} was not found", ow.name))?;
    let t = Target::new(path, is_dir);
    let args = openwith::command_line(ow, &t, folder_only);
    super::storage::log(&format!("open with {}: {} {}", ow.name, exe.display(), args));
    let mut c = std::process::Command::new(&exe);
    c.raw_arg(&args);
    // VS Codeの`code.cmd`などは黒い窓を出さない
    if exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat")) {
        c.creation_flags(CREATE_NO_WINDOW);
    }
    // フォルダーで起こす(ターミナルの初めの場所. WSLのパスはWindowsの側では使えないので付けない)
    if t.wsl.is_none() && Path::new(&t.dir).is_dir() {
        c.current_dir(&t.dir);
    }
    c.spawn().map(|_| ()).map_err(|e| format!("Could not open {}: {e}", ow.name))
}

/// 入っている「〜で開く」の一覧(メニューの順)
pub fn installed_open_with(cfg: &Config) -> Vec<OpenWith> {
    cfg.open_with.iter().filter(|o| find_exe(o).is_some()).cloned().collect()
}

/// `Alt+Enter`のターミナル(Q6)
pub fn terminal(cfg: &Config) -> Option<OpenWith> {
    openwith::terminal(cfg, |o| find_exe(o).is_some()).cloned()
}

/// 窓を前に出す(H4, C3)
pub fn switch_to(hwnd: isize) {
    let h = HWND(hwnd as *mut _);
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let _ = SetForegroundWindow(h);
    }
}

/// プロセスを終了する(G2). 権限が足りなければUACを出してtaskkillで終了する(G3)
pub fn kill(pids: &[u32]) -> Result<(), String> {
    let mut denied = Vec::new();
    for &pid in pids {
        unsafe {
            match OpenProcess(PROCESS_TERMINATE, false, pid) {
                Ok(h) => {
                    if TerminateProcess(h, 1).is_err() {
                        denied.push(pid);
                    }
                    let _ = CloseHandle(h);
                }
                Err(_) => denied.push(pid),
            }
        }
    }
    if denied.is_empty() {
        return Ok(());
    }
    let args: Vec<String> = denied.iter().map(|p| format!("/PID {p}")).collect();
    shell("runas", "taskkill.exe", &format!("/F {}", args.join(" ")), "", true)
}

pub fn system(a: SysAction, hwnd: HWND) -> Result<(), String> {
    unsafe {
        match a {
            SysAction::Lock => LockWorkStation().map_err(|e| e.message()),
            SysAction::Sleep => {
                if SetSuspendState(false, false, false) {
                    Ok(())
                } else {
                    Err("Could not sleep".into())
                }
            }
            SysAction::EmptyRecycleBin => {
                let _ = SHEmptyRecycleBinW(Some(hwnd), PCWSTR::null(), SHERB_NOCONFIRMATION | SHERB_NOSOUND);
                Ok(())
            }
            SysAction::Restart => shell("open", "shutdown.exe", "/r /t 0", "", true),
            SysAction::ShutDown => shell("open", "shutdown.exe", "/s /t 0", "", true),
            SysAction::SignOut => shell("open", "shutdown.exe", "/l", "", true),
            // 画面の側(App::open_settings, App::restart)で行う
            SysAction::HikidashiSettings | SysAction::RestartHikidashi => Ok(()),
            SysAction::DisplayOff => {
                // すぐ送ると, Enterを離したキーで画面がまた点く. 少し待ってから自分の窓に送る(DefWindowProcが消す)
                let h = hwnd.0 as isize;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(700));
                    use windows::Win32::UI::WindowsAndMessaging::{SC_MONITORPOWER, WM_SYSCOMMAND};
                    let _ = PostMessageW(Some(HWND(h as *mut _)), WM_SYSCOMMAND, WPARAM(SC_MONITORPOWER as usize), LPARAM(2));
                });
                Ok(())
            }
        }
    }
}

/// 設定のページを開く(H5-2). `ms-settings:`はURI, ほかはコマンド
pub fn open_setting(target: &str, args: &str) -> Result<(), String> {
    shell("open", target, args, "", false)
}
