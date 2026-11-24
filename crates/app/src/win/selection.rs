//! 選んだ文字で開く(2つ目のホットキー)と, クリップボードの履歴から貼る操作で使う, キーの送信.

use super::clipboard;
use super::winlist;
use hikidashi_core::selection::is_terminal;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU,
    VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

fn key(vk: VIRTUAL_KEY, up: bool, extended: bool) -> INPUT {
    let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

const MODS: [VIRTUAL_KEY; 5] = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN];

fn held(vk: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(vk.0 as i32) }) < 0
}

/// ホットキーの修飾キーが離されるのを待つ(最大0.5秒). まだ押されていたら, キーを離したことにして送る
fn release_modifiers() {
    for _ in 0..50 {
        if !MODS.iter().any(|k| held(*k)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ups: Vec<INPUT> = MODS.iter().filter(|k| held(**k)).map(|k| key(*k, true, false)).collect();
    send(&ups);
    std::thread::sleep(std::time::Duration::from_millis(30));
}

/// 前面の窓のプロセスのexe
pub fn foreground_exe() -> String {
    unsafe {
        let h = GetForegroundWindow();
        if h.is_invalid() {
            return String::new();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        winlist::process_path(pid)
    }
}

/// 前面のアプリで選んでいる文字を, Ctrl+Cで読む. クリップボードの文字とファイルは退避して, 読んだら戻す.
/// 返すのは(読んだ文字, 戻したあとのクリップボードの通し番号). 番号は履歴に残さない目印.
/// ターミナルでは, Ctrl+Cが中断になるので送らない
pub fn capture(hwnd: HWND) -> (Option<String>, Option<u32>) {
    let exe = foreground_exe();
    if exe.is_empty() || is_terminal(&exe) {
        return (None, None);
    }
    let saved = clipboard::save(hwnd);
    let before = clipboard::sequence();
    release_modifiers();
    send(&[key(VK_CONTROL, false, false), key(VK_C, false, false), key(VK_C, true, false), key(VK_CONTROL, true, false)]);
    // クリップボードが変わるのを待つ(最大0.4秒)
    let mut changed = false;
    for _ in 0..40 {
        if clipboard::sequence() != before {
            changed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let mut text = None;
    if changed {
        // アプリは「空にする→入れる」の順に書くので, 空のところを読まないよう, 読めるまで少しやり直す
        for _ in 0..8 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            text = clipboard::read_text(hwnd).filter(|t| !t.is_empty());
            if text.is_some() {
                break;
            }
        }
    }
    let ignore = match (&saved, changed) {
        (Some(s), true) => Some(clipboard::restore(hwnd, s)),
        (None, true) => Some(clipboard::sequence()),
        // 変わらなかった(選んでいなかった). 何もしていないので目印は要らない
        (_, false) => None,
    };
    (text, ignore)
}

/// 貼る(Shift+Insert). 窓を閉じて, 前のアプリが前面に戻ってから呼ぶ
pub fn press_shift_insert() {
    send(&[key(VK_SHIFT, false, false), key(VK_INSERT, false, true), key(VK_INSERT, true, true), key(VK_SHIFT, true, false)]);
}

/// 自分の窓(`own`)が前面でなくなる(前のアプリが前に戻る)のを待って, 貼る. 別のスレッドで行う
pub fn paste_when_back(own: isize) {
    let _ = std::thread::Builder::new().name("paste".into()).spawn(move || {
        for _ in 0..100 {
            let fg = unsafe { GetForegroundWindow() };
            if !fg.is_invalid() && fg.0 as isize != own {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // 前のアプリの入力欄にフォーカスが戻るまで少し待つ
        std::thread::sleep(std::time::Duration::from_millis(120));
        // Enterの修飾キーなどが押されたままなら離れるのを待つ
        for _ in 0..30 {
            if !MODS.iter().any(|k| held(*k)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        press_shift_insert();
    });
}
