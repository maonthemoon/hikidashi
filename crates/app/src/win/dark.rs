//! Windows標準の部品をダークの見た目にする(K43. 設定画面の一覧とスクロールの帯, トレイのメニュー).
//! ほかの部品は`parts.rs`が自分で描く(K44).
//! 部品のダークは, 公開されていない`uxtheme.dll`の番号の関数で入れる(Notepad++なども使っている形).
//! Windows 10の1809(17763)より古いものや, 関数がないときは何もしない.

use super::theme::is_dark;
use super::theme::pal;
use super::util::rgb;
use std::sync::OnceLock;
use windows::core::{s, w, BOOL, PCSTR, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::{RedrawWindow, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::UI::Controls::SetWindowTheme;
use windows::Win32::UI::WindowsAndMessaging::*;

type AllowWindowFn = unsafe extern "system" fn(HWND, BOOL) -> BOOL;
type AppModeFn = unsafe extern "system" fn(i32) -> i32;
type FlushFn = unsafe extern "system" fn();

struct Api {
    allow_window: Option<AllowWindowFn>,
    app_mode: Option<AppModeFn>,
    flush_menus: Option<FlushFn>,
}

/// Windowsのビルド番号. 分からなければ0
fn build_number() -> u32 {
    static B: OnceLock<u32> = OnceLock::new();
    *B.get_or_init(|| unsafe {
        let Ok(h) = GetModuleHandleW(w!("ntdll.dll")) else { return 0 };
        let Some(f) = GetProcAddress(h, s!("RtlGetVersion")) else { return 0 };
        let f: unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32 = std::mem::transmute(f);
        let mut v = OSVERSIONINFOW { dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
        if f(&mut v) == 0 {
            v.dwBuildNumber
        } else {
            0
        }
    })
}

fn api() -> &'static Api {
    static A: OnceLock<Api> = OnceLock::new();
    A.get_or_init(|| unsafe {
        let none = Api { allow_window: None, app_mode: None, flush_menus: None };
        if build_number() < 17763 {
            return none;
        }
        let Ok(h) = LoadLibraryExW(w!("uxtheme.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) else { return none };
        let get = |n: usize| GetProcAddress(h, PCSTR(n as *const u8));
        Api {
            allow_window: get(133).map(|f| std::mem::transmute::<_, AllowWindowFn>(f)),
            // 1809ではAllowDarkModeForApp(真偽), 1903からはSetPreferredAppMode(0: 既定, 2: いつもダーク). 0と2ならどちらでも通じる
            app_mode: get(135).map(|f| std::mem::transmute::<_, AppModeFn>(f)),
            flush_menus: get(136).map(|f| std::mem::transmute::<_, FlushFn>(f)),
        }
    })
}

/// アプリ全体のモード. トレイのメニューとファイルを選ぶ窓の色が変わる
pub fn set_app_mode(dark: bool) {
    let a = api();
    unsafe {
        if let Some(f) = a.app_mode {
            f(if dark { 2 } else { 0 });
        }
        if let Some(f) = a.flush_menus {
            f();
        }
    }
}

/// 窓のタイトルの帯. Windows 10はダークかライトだけ, 11は背景の色にする
pub fn title_bar(hwnd: HWND) {
    let on = BOOL(is_dark() as i32);
    let (bg, fg) = (rgb(pal().bg).0, rgb(pal().text).0);
    unsafe {
        if DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &on as *const _ as *const _, 4).is_err() {
            // 2004より前のWindows 10は19番
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(19), &on as *const _ as *const _, 4);
        }
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &bg as *const _ as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &fg as *const _ as *const _, 4);
    }
}

/// 窓と中の部品全部に, 今のテーマの見た目を付ける
pub fn apply(hwnd: HWND) {
    title_bar(hwnd);
    unsafe {
        let _ = EnumChildWindows(Some(hwnd), Some(apply_one), LPARAM(0));
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN);
    }
}

pub fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}

fn set_theme(h: HWND, dark_name: PCWSTR) {
    let dark = is_dark();
    unsafe {
        if let Some(f) = api().allow_window {
            let _ = f(h, BOOL(dark as i32));
        }
        let _ = SetWindowTheme(h, if dark { dark_name } else { PCWSTR::null() }, PCWSTR::null());
    }
}

extern "system" fn apply_one(h: HWND, _: LPARAM) -> BOOL {
    match class_name(h).as_str() {
        "Edit" => set_theme(h, w!("DarkMode_CFD")),
        "ListBox" | "HikidashiPanel" => set_theme(h, w!("DarkMode_Explorer")),
        _ => {}
    }
    BOOL(1)
}
