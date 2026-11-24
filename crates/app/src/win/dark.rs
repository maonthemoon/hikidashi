//! Windows標準の部品をダークの見た目にする(K43. 設定画面とトレイのメニュー).
//! 部品のダークは, 公開されていない`uxtheme.dll`の番号の関数で入れる(Notepad++なども使っている形).
//! Windows 10の1809(17763)より古いものや, 関数がないときは何もしない.

use super::theme::is_dark;
use super::theme::pal;
use super::util::rgb;
use std::sync::OnceLock;
use windows::core::{s, w, BOOL, PCSTR, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
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
        "ComboBox" => {
            set_theme(h, w!("DarkMode_CFD"));
            // 開いたときの一覧(スクロールの帯)
            let mut info = COMBOBOXINFO { cbSize: std::mem::size_of::<COMBOBOXINFO>() as u32, ..Default::default() };
            if unsafe { GetComboBoxInfo(h, &mut info) }.is_ok() && !info.hwndList.is_invalid() {
                set_theme(info.hwndList, w!("DarkMode_Explorer"));
            }
        }
        "ListBox" | "Button" | "HikidashiPanel" => set_theme(h, w!("DarkMode_Explorer")),
        "SysListView32" => {
            set_theme(h, w!("DarkMode_Explorer"));
            let (bg, fg) = (rgb(pal().field).0 as isize, rgb(pal().text).0 as isize);
            unsafe {
                SendMessageW(h, LVM_SETBKCOLOR, None, Some(LPARAM(bg)));
                SendMessageW(h, LVM_SETTEXTBKCOLOR, None, Some(LPARAM(bg)));
                SendMessageW(h, LVM_SETTEXTCOLOR, None, Some(LPARAM(fg)));
                let head = HWND(SendMessageW(h, LVM_GETHEADER, None, None).0 as *mut _);
                set_theme(head, w!("DarkMode_ItemsView"));
                // 見出しの文字は標準では黒のままなので, 自分で描く
                let _ = SetWindowSubclass(h, Some(list_proc), 3, 0);
            }
        }
        _ => {}
    }
    BOOL(1)
}

fn fill(hdc: HDC, rc: &RECT, color: u32) {
    unsafe {
        let b = CreateSolidBrush(rgb(color));
        FillRect(hdc, rc, b);
        let _ = DeleteObject(b.into());
    }
}

/// 表の見出しをダークで描く
extern "system" fn list_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_NOTIFY && is_dark() && lparam.0 != 0 {
        unsafe {
            let hdr = std::ptr::read_unaligned(lparam.0 as *const NMHDR);
            let head = HWND(SendMessageW(hwnd, LVM_GETHEADER, None, None).0 as *mut _);
            if hdr.code == NM_CUSTOMDRAW && hdr.hwndFrom == head {
                let cd = std::ptr::read_unaligned(lparam.0 as *const NMCUSTOMDRAW);
                if cd.dwDrawStage == CDDS_PREPAINT {
                    let mut rc = RECT::default();
                    let _ = GetClientRect(head, &mut rc);
                    fill(cd.hdc, &rc, pal().bg_muted);
                    return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
                }
                if cd.dwDrawStage == CDDS_ITEMPREPAINT {
                    draw_header_item(head, &cd);
                    return LRESULT(CDRF_SKIPDEFAULT as isize);
                }
            }
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

unsafe fn draw_header_item(head: HWND, cd: &NMCUSTOMDRAW) {
    fill(cd.hdc, &cd.rc, pal().bg_muted);
    let line = RECT { left: cd.rc.right - 1, ..cd.rc };
    fill(cd.hdc, &line, pal().border);
    let mut buf = [0u16; 128];
    let mut it = HDITEMW { mask: HDI_TEXT, pszText: PWSTR(buf.as_mut_ptr()), cchTextMax: buf.len() as i32, ..Default::default() };
    SendMessageW(head, HDM_GETITEMW, Some(WPARAM(cd.dwItemSpec)), Some(LPARAM(&mut it as *mut _ as isize)));
    let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    let font = HFONT(SendMessageW(head, WM_GETFONT, None, None).0 as *mut _);
    let old = SelectObject(cd.hdc, font.into());
    SetBkMode(cd.hdc, TRANSPARENT);
    SetTextColor(cd.hdc, rgb(pal().text_secondary));
    let mut rc = RECT { left: cd.rc.left + 6, right: cd.rc.right - 4, ..cd.rc };
    DrawTextW(cd.hdc, &mut buf[..n], &mut rc, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX);
    SelectObject(cd.hdc, old);
}

/// チェックボックスをダークで描く(標準のものは文字が黒のままになる). 扱ったらSome
pub fn check_custom_draw(lparam: LPARAM) -> Option<LRESULT> {
    if !is_dark() || lparam.0 == 0 {
        return None;
    }
    unsafe {
        let hdr = std::ptr::read_unaligned(lparam.0 as *const NMHDR);
        if hdr.code != NM_CUSTOMDRAW || class_name(hdr.hwndFrom) != "Button" {
            return None;
        }
        let cd = std::ptr::read_unaligned(lparam.0 as *const NMCUSTOMDRAW);
        if cd.dwDrawStage != CDDS_PREPAINT {
            return None;
        }
        let h = hdr.hwndFrom;
        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(h).max(96) as i32;
        let px = |v: i32| v * dpi / 96;
        let has = |f: NMCUSTOMDRAW_DRAW_STATE_FLAGS| cd.uItemState.0 & f.0 != 0;
        let on = SendMessageW(h, BM_GETCHECK, None, None).0 as u32 == BST_CHECKED.0;
        fill(cd.hdc, &cd.rc, pal().bg);
        let size = px(14);
        let top = cd.rc.top + (cd.rc.bottom - cd.rc.top - size) / 2;
        let left = cd.rc.left + 1;
        let edge = if has(CDIS_FOCUS) { 0x2C84DB } else if on { pal().accent } else { pal().text_tertiary };
        let face = if on { pal().accent } else if has(CDIS_HOT) || has(CDIS_SELECTED) { pal().btn_down } else { pal().field };
        let b = CreateSolidBrush(rgb(face));
        let p = CreatePen(PS_SOLID, 1, rgb(edge));
        let ob = SelectObject(cd.hdc, b.into());
        let op = SelectObject(cd.hdc, p.into());
        let _ = RoundRect(cd.hdc, left, top, left + size, top + size, px(4), px(4));
        if on {
            let mark = CreatePen(PS_SOLID, px(2).max(2), rgb(pal().bg));
            SelectObject(cd.hdc, mark.into());
            let _ = MoveToEx(cd.hdc, left + px(3), top + px(7), None);
            let _ = LineTo(cd.hdc, left + px(6), top + px(10));
            let _ = LineTo(cd.hdc, left + px(11), top + px(4));
            SelectObject(cd.hdc, op);
            let _ = DeleteObject(mark.into());
        }
        SelectObject(cd.hdc, ob);
        SelectObject(cd.hdc, op);
        let _ = DeleteObject(b.into());
        let _ = DeleteObject(p.into());
        let font = HFONT(SendMessageW(h, WM_GETFONT, None, None).0 as *mut _);
        let old = SelectObject(cd.hdc, font.into());
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, &mut buf).max(0) as usize;
        SetBkMode(cd.hdc, TRANSPARENT);
        SetTextColor(cd.hdc, rgb(pal().text));
        let mut rc = RECT { left: left + size + px(6), ..cd.rc };
        DrawTextW(cd.hdc, &mut buf[..n], &mut rc, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_NOPREFIX);
        SelectObject(cd.hdc, old);
        Some(LRESULT(CDRF_SKIPDEFAULT as isize))
    }
}
