//! 設定画面(K16). トレイの`Settings`で開く. 左に区分, 右に中身, 下にSaveとCancel.
//! 部品はWindows標準(入力欄, リスト, 表, チェックボックス)で, 色だけランチャーに合わせる(Q6).
//! Saveで`config.toml`を書き直す(Q7. 手で書いた`#`の行は残らない). 書いたファイルは監視が読み直す.

use super::i18n::tr;
use super::storage;
use super::system;
use super::theme::*;
use super::util::{from_wide, hiword, loword, rgb, wide, WStr};
use super::{actions, dark, paint};
use hikidashi_core::builtin;
use hikidashi_core::config::{self, parse_enter_key, Command, Config, Files, OpenWith, Wsl, ENTER_KEY_CHOICES};
use hikidashi_core::pathlike;
use std::cell::RefCell;
use std::sync::OnceLock;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::{SS_CENTERIMAGE, SS_LEFT, SS_NOPREFIX};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("HikidashiSettings");
/// 区分の中身を入れる枠. 入りきらなければスクロールする(K43)
const PANEL_CLASS: PCWSTR = w!("HikidashiPanel");
const EDITOR_CLASS: PCWSTR = w!("HikidashiSettingsItem");

// 大きさ(96dpiのとき)
const CLIENT_W: i32 = 780;
const CLIENT_H: i32 = 660;
const SIDE_W: i32 = 168;
const PAD: i32 = 24;
const FOOT_H: i32 = 60;
const ROW_GAP: i32 = 8;
const FONT_PX: i32 = 14;
const HEAD_PX: i32 = 18;

// 部品の番号. 1と2はEnterとEscで押される(IsDialogMessage)
const ID_SAVE: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_OPENFILE: i32 = 10;
const ID_ERROR: i32 = 11;
const ID_SIDE: i32 = 20;
const ID_HOTKEY: i32 = 100;
const ID_HOTKEY_CHANGE: i32 = 101;
const ID_AUTOSTART: i32 = 102;
const ID_LANG: i32 = 103;
const ID_THEME: i32 = 104;
const ID_SELHK: i32 = 105;
const ID_SELHK_CHANGE: i32 = 106;
const ID_SELHK_CLEAR: i32 = 107;
const ID_RECENT: i32 = 230;
const ID_CLIPHIST: i32 = 231;
const ID_PFX: i32 = 110; // 110〜119
const ID_KEYS: i32 = 120; // 120〜124
const ID_ROOTS: i32 = 200;
const ID_ROOTS_ADD: i32 = 201;
const ID_ROOTS_DEL: i32 = 202;
const ID_SKIP: i32 = 210;
const ID_SKIP_ADD: i32 = 211;
const ID_SKIP_DEL: i32 = 212;
const ID_NAMES: i32 = 220;
const ID_NAME_EDIT: i32 = 221;
const ID_NAME_ADD: i32 = 222;
const ID_NAME_DEL: i32 = 223;
const ID_NAME_RESET: i32 = 224;
const ID_WSL_ON: i32 = 310;
const ID_WSL_DISTRO: i32 = 311;
// WSLの外す名前(K26. 検索の外す名前と同じ形)
const ID_WSL_NAMES: i32 = 313;
const ID_WSL_NAME_EDIT: i32 = 314;
const ID_WSL_NAME_ADD: i32 = 315;
const ID_WSL_NAME_DEL: i32 = 316;
const ID_WSL_NAME_RESET: i32 = 317;
const ID_APPDIRS: i32 = 400;
const ID_APPDIRS_ADD: i32 = 401;
const ID_APPDIRS_DEL: i32 = 402;
const ID_OPENWITH: i32 = 410;
const ID_OW_ADD: i32 = 411;
const ID_OW_EDIT: i32 = 412;
const ID_OW_DEL: i32 = 413;
const ID_TERMINAL: i32 = 414;
/// Tabのメニューのチェック(K24). 420 + 項目の番号×3 + 種類(アプリ, ファイル, フォルダー)
const ID_MENU: i32 = 420;
const ID_CHROME: i32 = 500;
const ID_CHROME_BROWSE: i32 = 501;
const ID_PROFILE: i32 = 502;
const ID_BROWSER: i32 = 503;
const ID_OPEN_IN: i32 = 504;
const ID_EDGE_PROFILE: i32 = 505;
const ID_GPT_MODE: i32 = 510;
const ID_GPT_URL: i32 = 511;
const ID_GOOGLE: i32 = 512;
const ID_GPT_SEND: i32 = 513;
const ID_GPT_DELAY: i32 = 514;
const ID_GPT_HIDE: i32 = 515;
const ID_ALIASES: i32 = 600;
const ID_ALIAS_ADD: i32 = 601;
const ID_ALIAS_EDIT: i32 = 602;
const ID_ALIAS_DEL: i32 = 603;
const ID_CMDS: i32 = 700;
const ID_CMD_ADD: i32 = 701;
const ID_CMD_EDIT: i32 = 702;
const ID_CMD_DEL: i32 = 703;
// 項目を直す小さな窓
const E_NAME: i32 = 10;
const E_TITLE: i32 = 11;
const E_KIND: i32 = 12;
const E_VALUE: i32 = 13;
const E_ERROR: i32 = 14;
const E_VALUE_LABEL: i32 = 15;

const SECTIONS: [&str; 8] = ["General", "Search", "Box and WSL", "Apps", "Tab menu", "Web", "Aliases", "Commands"];
const PREFIX_LABELS: [&str; 10] = ["Apps", "Files", "Bookmarks", "Kill", "WSL", "Box", "Google", "ChatGPT", "Clipboard", "Calc"];
/// 言語(K24). どちらの言語でも同じ文字で出す
const LANGS: [(&str, &str); 3] = [("auto", "Same as Windows / Windowsに合わせる"), ("ja", "日本語"), ("en", "English")];
/// ランチャーの窓, 書く窓, 設定画面, トレイのメニューの色(K39, K43)
const THEMES: [(&str, &str); 3] = [("auto", "Same as the app mode of Windows"), ("light", "Light"), ("dark", "Dark")];
/// Tabのメニューの項目(`config::MENU_ITEMS`の順)と種類
const MENU_LABELS: [&str; 8] = ["Copy file", "Open new window", "Open in … (Open with)", "Open the folder it is in", "Copy path", "Copy WSL path (only with WSL)", "Run as administrator", "Open in Explorer"];
const MENU_KINDS: [&str; 3] = ["app", "file", "folder"];
/// Enterに修飾キーを足したときの操作(2026-11-24. 設定の`[keys]`の順)
const KEY_LABELS: [&str; 5] = ["Ask ChatGPT", "Search Google", "Long question", "Run as admin", "Open in terminal"];
const BROWSERS: [(&str, &str); 3] = [("default", "Default browser of Windows"), ("chrome", "Chrome"), ("edge", "Edge")];
const OPEN_IN: [(&str, &str); 2] = [("window", "New window"), ("tab", "New tab")];
const GPT_MODES: [(&str, &str); 2] = [("url", "Open chatgpt.com/?q= and send the question"), ("clipboard", "Copy the question and open ChatGPT")];

/// 開くときに渡す値
pub struct OpenArgs {
    pub config: Config,
    pub error: Option<String>,
    pub hotkey: String,
    pub exe: String,
    pub apps: Vec<String>,
}

// ---------------------------------------------------------------- 並べ方

#[derive(Clone, Copy)]
enum W {
    Px(i32),
    Fill,
}

struct It {
    hwnd: Option<HWND>,
    w: W,
    h: i32,
    /// コンボボックスの開いたときの高さ(見た目の高さとは別)
    drop: i32,
}

struct Row {
    gap: i32,
    items: Vec<It>,
}

fn s(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

/// 行を上から並べる. `x`と`width`は96dpiの値. 返り値は下の端(96dpi)
fn place(rows: &[Row], x: i32, y: i32, width: i32, dpi: u32) -> i32 {
    let mut y = y;
    for r in rows {
        y += r.gap;
        let h = r.items.iter().map(|i| i.h).max().unwrap_or(0);
        let fixed: i32 = r.items.iter().map(|i| if let W::Px(p) = i.w { p } else { 0 }).sum();
        let fills = r.items.iter().filter(|i| matches!(i.w, W::Fill)).count() as i32;
        let gaps = (r.items.len() as i32 - 1).max(0) * ROW_GAP;
        let fill_w = if fills > 0 { ((width - fixed - gaps) / fills).max(20) } else { 0 };
        let mut cx = x;
        for it in &r.items {
            let iw = match it.w {
                W::Px(p) => p,
                W::Fill => fill_w,
            };
            if let Some(hwnd) = it.hwnd {
                let top = y + (h - it.h) / 2;
                let real_h = if it.drop > 0 { it.drop } else { it.h };
                unsafe {
                    let _ = SetWindowPos(hwnd, None, s(cx, dpi), s(top, dpi), s(iw, dpi), s(real_h, dpi), SWP_NOZORDER | SWP_NOACTIVATE);
                }
            }
            cx += iw + ROW_GAP;
        }
        y += h + ROW_GAP;
    }
    y
}

// ---------------------------------------------------------------- 部品を作る

struct Maker {
    parent: HWND,
    hinst: windows::Win32::Foundation::HINSTANCE,
    /// 作った部品(フォントを付けるため). (部品, 見出しか)
    made: Vec<(HWND, u8)>,
}

const F_BODY: u8 = 0;
const F_HEAD: u8 = 1;
const F_BOLD: u8 = 2;

impl Maker {
    fn raw(&mut self, class: PCWSTR, text: &str, style: u32, ex: WINDOW_EX_STYLE, id: i32, font: u8) -> HWND {
        let t = WStr::new(text);
        let h = unsafe {
            CreateWindowExW(
                ex,
                class,
                t.pcwstr(),
                WS_CHILD | WINDOW_STYLE(style),
                0,
                0,
                10,
                10,
                Some(self.parent),
                Some(HMENU(id as isize as *mut _)),
                Some(self.hinst),
                None,
            )
            .unwrap_or_default()
        };
        self.made.push((h, font));
        h
    }
    fn label(&mut self, text: &str) -> HWND {
        let text = &tr(text);
        self.raw(w!("STATIC"), text, (SS_LEFT.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_BODY)
    }
    fn hint(&mut self, text: &str) -> HWND {
        let text = &tr(text);
        let h = self.raw(w!("STATIC"), text, (SS_LEFT.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_BODY);
        tint(h, |p| p.text_tertiary);
        h
    }
    fn head(&mut self, text: &str) -> HWND {
        let text = &tr(text);
        self.raw(w!("STATIC"), text, (SS_LEFT.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_HEAD)
    }
    fn sub(&mut self, text: &str) -> HWND {
        let text = &tr(text);
        self.raw(w!("STATIC"), text, (SS_LEFT.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_BOLD)
    }
    fn edit(&mut self, id: i32, text: &str, number: bool) -> HWND {
        let mut st = WS_TABSTOP.0 | ES_AUTOHSCROLL as u32;
        if number {
            st |= ES_NUMBER as u32;
        }
        self.raw(w!("EDIT"), text, st, WS_EX_CLIENTEDGE, id, F_BODY)
    }
    fn button(&mut self, id: i32, text: &str) -> HWND {
        let text = &tr(text);
        self.raw(w!("BUTTON"), text, WS_TABSTOP.0 | BS_OWNERDRAW as u32, WINDOW_EX_STYLE(0), id, F_BODY)
    }
    fn check(&mut self, id: i32, text: &str, on: bool) -> HWND {
        let text = &tr(text);
        let h = self.raw(w!("BUTTON"), text, WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32, WINDOW_EX_STYLE(0), id, F_BODY);
        set_check(h, on);
        h
    }
    fn list(&mut self, id: i32, items: &[String]) -> HWND {
        let h = self.raw(w!("LISTBOX"), "", WS_TABSTOP.0 | WS_VSCROLL.0 | (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32, WS_EX_CLIENTEDGE, id, F_BODY);
        fill_list(h, items);
        h
    }
    fn combo(&mut self, id: i32, items: &[String], editable: bool) -> HWND {
        let st = WS_TABSTOP.0 | WS_VSCROLL.0 | if editable { CBS_DROPDOWN | CBS_AUTOHSCROLL } else { CBS_DROPDOWNLIST } as u32;
        let h = self.raw(w!("COMBOBOX"), "", st, WINDOW_EX_STYLE(0), id, F_BODY);
        for it in items {
            // 選ぶだけの一覧は日本語にする. 打てる一覧(アプリの名前など)は中身なのでそのまま
            let t = WStr::new(&if editable { it.clone() } else { tr(it) });
            unsafe { SendMessageW(h, CB_ADDSTRING, None, Some(LPARAM(t.pcwstr().0 as isize))) };
        }
        h
    }
    fn table(&mut self, id: i32, cols: &[(&str, i32)]) -> HWND {
        let h = self.raw(
            WC_LISTVIEWW,
            "",
            WS_TABSTOP.0 | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER,
            WS_EX_CLIENTEDGE,
            id,
            F_BODY,
        );
        unsafe {
            SendMessageW(h, LVM_SETEXTENDEDLISTVIEWSTYLE, Some(WPARAM(LVS_EX_FULLROWSELECT as usize)), Some(LPARAM(LVS_EX_FULLROWSELECT as isize)));
            for (i, (name, cx)) in cols.iter().enumerate() {
                let mut t = wide(&tr(name));
                let col = LVCOLUMNW { mask: LVCF_TEXT | LVCF_WIDTH, cx: *cx, pszText: PWSTR(t.as_mut_ptr()), ..Default::default() };
                SendMessageW(h, LVM_INSERTCOLUMNW, Some(WPARAM(i)), Some(LPARAM(&col as *const _ as isize)));
            }
        }
        h
    }
}

fn it(h: HWND, w: W, ht: i32) -> It {
    It { hwnd: Some(h), w, h: ht, drop: 0 }
}
fn gap(px: i32) -> It {
    It { hwnd: None, w: W::Px(px), h: 0, drop: 0 }
}
fn combo_it(h: HWND, w: W) -> It {
    It { hwnd: Some(h), w, h: 26, drop: 240 }
}
fn row(items: Vec<It>) -> Row {
    Row { gap: 0, items }
}
fn row_gap(gap: i32, items: Vec<It>) -> Row {
    Row { gap, items }
}

// ---------------------------------------------------------------- 小さな道具

pub fn text(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h);
        let mut buf = vec![0u16; n as usize + 1];
        GetWindowTextW(h, &mut buf);
        from_wide(&buf)
    }
}

/// 文字を出す. 知らせや案内は日本語にする(パスなどの中身は表にないのでそのまま)
fn set_text(h: HWND, t: &str) {
    let w = WStr::new(&tr(t));
    unsafe {
        let _ = SetWindowTextW(h, w.pcwstr());
    }
}

fn checked(h: HWND) -> bool {
    unsafe { SendMessageW(h, BM_GETCHECK, None, None).0 as u32 == BST_CHECKED.0 }
}

fn set_check(h: HWND, on: bool) {
    unsafe { SendMessageW(h, BM_SETCHECK, Some(WPARAM(if on { BST_CHECKED.0 } else { BST_UNCHECKED.0 } as usize)), None) };
}

fn fill_list(h: HWND, items: &[String]) {
    unsafe {
        SendMessageW(h, LB_RESETCONTENT, None, None);
        for it in items {
            let t = WStr::new(it);
            SendMessageW(h, LB_ADDSTRING, None, Some(LPARAM(t.pcwstr().0 as isize)));
        }
    }
}

fn list_sel(h: HWND) -> Option<usize> {
    let r = unsafe { SendMessageW(h, LB_GETCURSEL, None, None).0 };
    (r >= 0).then_some(r as usize)
}

fn table_fill(h: HWND, rows: &[Vec<String>]) {
    unsafe {
        SendMessageW(h, LVM_DELETEALLITEMS, None, None);
        for (i, cells) in rows.iter().enumerate() {
            for (j, c) in cells.iter().enumerate() {
                let mut t = wide(c);
                let item = LVITEMW { mask: LVIF_TEXT, iItem: i as i32, iSubItem: j as i32, pszText: PWSTR(t.as_mut_ptr()), ..Default::default() };
                let msg = if j == 0 { LVM_INSERTITEMW } else { LVM_SETITEMTEXTW };
                SendMessageW(h, msg, Some(WPARAM(i)), Some(LPARAM(&item as *const _ as isize)));
            }
        }
    }
}

fn table_sel(h: HWND) -> Option<usize> {
    let r = unsafe { SendMessageW(h, LVM_GETNEXTITEM, Some(WPARAM(usize::MAX)), Some(LPARAM(LVNI_SELECTED as isize))).0 };
    (r >= 0).then_some(r as usize)
}

fn table_select(h: HWND, i: usize) {
    unsafe {
        let f = LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0);
        let item = LVITEMW { stateMask: f, state: f, ..Default::default() };
        SendMessageW(h, LVM_SETITEMSTATE, Some(WPARAM(i)), Some(LPARAM(&item as *const _ as isize)));
        SendMessageW(h, LVM_ENSUREVISIBLE, Some(WPARAM(i)), Some(LPARAM(0)));
    }
}

/// 設定画面の中の枠. なければ(項目を直す小さな窓など)空
fn panel_of(hwnd: HWND) -> HWND {
    unsafe { FindWindowExW(Some(hwnd), None, PANEL_CLASS, PCWSTR::null()).unwrap_or_default() }
}

fn item(parent: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(Some(parent), id).or_else(|_| GetDlgItem(Some(panel_of(parent)), id)).unwrap_or_default() }
}

fn brush(c: u32) -> HBRUSH {
    static B: OnceLock<std::sync::Mutex<Vec<(u32, isize)>>> = OnceLock::new();
    let m = B.get_or_init(|| std::sync::Mutex::new(vec![]));
    let mut v = m.lock().unwrap();
    if let Some((_, b)) = v.iter().find(|(k, _)| *k == c) {
        return HBRUSH(*b as *mut _);
    }
    let b = unsafe { CreateSolidBrush(rgb(c)) };
    v.push((c, b.0 as isize));
    b
}

thread_local! {
    /// 文字の色を変える部品(説明の行, エラー)
    static TINT: RefCell<Vec<(isize, Color)>> = const { RefCell::new(Vec::new()) };
}

/// テーマが変わっても付いていくよう, 色そのものではなく選び方で持つ
type Color = fn(&Palette) -> u32;

fn tint(h: HWND, c: Color) {
    TINT.with(|t| {
        let mut t = t.borrow_mut();
        t.retain(|(k, _)| *k != h.0 as isize);
        t.push((h.0 as isize, c));
    });
}

fn tint_of(h: HWND) -> u32 {
    TINT.with(|t| t.borrow().iter().find(|(k, _)| *k == h.0 as isize).map(|(_, c)| c(pal())).unwrap_or(pal().text))
}

/// フォルダーやexeを選ぶ(Windowsの「フォルダーの選択」). 選ばなければNone
fn pick(owner: HWND, folder: bool, title: &str) -> Option<String> {
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let mut opts = dlg.GetOptions().ok()? | FOS_FORCEFILESYSTEM;
        if folder {
            opts |= FOS_PICKFOLDERS;
        } else {
            let name = WStr::new(&tr("Programs"));
            let spec = [COMDLG_FILTERSPEC { pszName: name.pcwstr(), pszSpec: w!("*.exe") }];
            let _ = dlg.SetFileTypes(&spec);
        }
        let _ = dlg.SetOptions(opts);
        let t = WStr::new(&tr(title));
        let _ = dlg.SetTitle(t.pcwstr());
        dlg.Show(Some(owner)).ok()?;
        let item = dlg.GetResult().ok()?;
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

fn short_path(p: &str) -> String {
    pathlike::contract_home(p, &storage::env("USERPROFILE"))
}

fn fonts_for(dpi: u32) -> [HFONT; 3] {
    let face = paint::body_face();
    [paint::font(face, s(FONT_PX, dpi), 400), paint::font(face, s(HEAD_PX, dpi), 600), paint::font(face, s(FONT_PX, dpi), 600)]
}

fn apply_fonts(made: &[(HWND, u8)], fonts: &[HFONT; 3]) {
    for (h, f) in made {
        unsafe { SendMessageW(*h, WM_SETFONT, Some(WPARAM(fonts[*f as usize].0 as usize)), Some(LPARAM(1))) };
    }
}

fn delete_fonts(fonts: &[HFONT; 3]) {
    for f in fonts {
        unsafe {
            let _ = DeleteObject((*f).into());
        }
    }
}

/// マウスがあるモニターの中央に置く窓の位置と大きさ
fn centered(dpi: u32, cw: i32, ch: i32, style: WINDOW_STYLE) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: s(cw, dpi), bottom: s(ch, dpi) };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut r, style, false, WINDOW_EX_STYLE(0), dpi);
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let w = r.right - r.left;
        let wa = mi.rcWork;
        // 画面が低ければ窓も低くする(中身はスクロールする)
        let h = (r.bottom - r.top).min(wa.bottom - wa.top);
        let x = wa.left + ((wa.right - wa.left) - w) / 2;
        let y = wa.top + ((wa.bottom - wa.top) - h) / 2;
        RECT { left: x, top: y.max(wa.top), right: x + w, bottom: y.max(wa.top) + h }
    }
}

fn dpi_at_cursor() -> u32 {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let (mut x, mut y) = (96u32, 96u32);
        let _ = windows::Win32::UI::HiDpi::GetDpiForMonitor(mon, windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI, &mut x, &mut y);
        x.max(96)
    }
}

fn register_classes() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let procs: [(PCWSTR, WNDPROC); 3] = [(CLASS, Some(settings_proc)), (EDITOR_CLASS, Some(editor_proc)), (PANEL_CLASS, Some(panel_proc))];
        for (class, proc_) in procs {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: proc_,
                hInstance: hinst.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: class,
                hIcon: system::app_icon(),
                ..Default::default()
            };
            RegisterClassExW(&wc);
        }
        let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_LISTVIEW_CLASSES };
        let _ = InitCommonControlsEx(&icc);
    });
}

// ---------------------------------------------------------------- 色と自分で描く部品(両方の窓で使う)

/// 部品の色と, 自分で描くボタン・区分の一覧. 扱ったらSome
fn common_proc(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    unsafe {
        match msg {
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let hdc = HDC(wparam.0 as *mut _);
                let h = HWND(lparam.0 as *mut _);
                SetTextColor(hdc, rgb(tint_of(h)));
                SetBkColor(hdc, rgb(pal().bg));
                Some(LRESULT(brush(pal().bg).0 as isize))
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let hdc = HDC(wparam.0 as *mut _);
                let h = HWND(lparam.0 as *mut _);
                if GetDlgCtrlID(h) == ID_SIDE {
                    SetTextColor(hdc, rgb(pal().text));
                    SetBkColor(hdc, rgb(pal().bg_subtle));
                    return Some(LRESULT(brush(pal().bg_subtle).0 as isize));
                }
                SetTextColor(hdc, rgb(pal().text));
                SetBkColor(hdc, rgb(pal().field));
                Some(LRESULT(brush(pal().field).0 as isize))
            }
            WM_NOTIFY => dark::check_custom_draw(lparam),
            WM_DRAWITEM => {
                let d = &*(lparam.0 as *const DRAWITEMSTRUCT);
                if d.CtlType == ODT_BUTTON {
                    draw_button(d);
                    Some(LRESULT(1))
                } else if d.CtlType == ODT_LISTBOX && d.CtlID as i32 == ID_SIDE {
                    draw_side_item(d);
                    Some(LRESULT(1))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

unsafe fn draw_button(d: &DRAWITEMSTRUCT) {
    let hdc = d.hDC;
    let r = d.rcItem;
    let id = d.CtlID as i32;
    let pressed = (d.itemState.0 & ODS_SELECTED.0) != 0;
    let focus = (d.itemState.0 & ODS_FOCUS.0) != 0;
    let disabled = (d.itemState.0 & ODS_DISABLED.0) != 0;
    FillRect(hdc, &r, brush(pal().bg));
    let font = HFONT(SendMessageW(d.hwndItem, WM_GETFONT, None, None).0 as *mut _);
    let old_font = SelectObject(hdc, font.into());
    let mut buf = [0u16; 128];
    let n = GetWindowTextW(d.hwndItem, &mut buf) as usize;
    let mut label = buf[..n].to_vec();
    SetBkMode(hdc, TRANSPARENT);
    let mut tr = r;
    if id == ID_OPENFILE {
        // リンクの形
        SetTextColor(hdc, rgb(if pressed { pal().text } else { pal().accent_text }));
        DrawTextW(hdc, &mut label, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
        if focus {
            let mut fr = r;
            DrawTextW(hdc, &mut label, &mut fr, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_CALCRECT);
            fr.top = r.bottom - 3;
            fr.bottom = r.bottom - 1;
            FillRect(hdc, &fr, brush(pal().accent_text));
        }
    } else {
        let primary = id == ID_SAVE;
        let (fill, border, fg) = if primary {
            (if pressed { 0xA9472A } else { 0xB85A39 }, if pressed { 0xA9472A } else { 0xB85A39 }, 0xFFFFFF)
        } else {
            (if pressed { pal().btn_down } else { pal().btn }, pal().border, if disabled { pal().text_tertiary } else { pal().text })
        };
        let pen = CreatePen(PS_SOLID, 1, rgb(if focus { 0x2C84DB } else { border }));
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, brush(fill).into());
        let rad = ((r.bottom - r.top) / 3).max(6);
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, rad, rad);
        if focus {
            // 枠を2重にして, キーボードで選んでいるのが分かるようにする
            let _ = SelectObject(hdc, GetStockObject(NULL_BRUSH));
            let _ = RoundRect(hdc, r.left + 1, r.top + 1, r.right - 1, r.bottom - 1, rad, rad);
        }
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen.into());
        SetTextColor(hdc, rgb(fg));
        DrawTextW(hdc, &mut label, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_CENTER);
    }
    SelectObject(hdc, old_font);
}

unsafe fn draw_side_item(d: &DRAWITEMSTRUCT) {
    let hdc = d.hDC;
    let r = d.rcItem;
    FillRect(hdc, &r, brush(pal().bg_subtle));
    if d.itemID == u32::MAX {
        return;
    }
    let sel = (d.itemState.0 & ODS_SELECTED.0) != 0;
    let dpi = GetDpiForWindow(d.hwndItem).max(96);
    if sel {
        let pen = CreatePen(PS_SOLID, 1, rgb(pal().bg_muted));
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, brush(pal().bg_muted).into());
        let m = s(4, dpi);
        let _ = RoundRect(hdc, r.left + m, r.top + 2, r.right - m, r.bottom - 2, s(8, dpi), s(8, dpi));
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen.into());
    }
    let font = HFONT(SendMessageW(d.hwndItem, WM_GETFONT, None, None).0 as *mut _);
    let old_font = SelectObject(hdc, font.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, rgb(if sel { pal().text } else { pal().text_secondary }));
    let name = tr(SECTIONS.get(d.itemID as usize).copied().unwrap_or(""));
    let mut t: Vec<u16> = name.encode_utf16().collect();
    let mut tr = RECT { left: r.left + s(16, dpi), ..r };
    DrawTextW(hdc, &mut t, &mut tr, DT_SINGLELINE | DT_VCENTER | DT_LEFT);
    SelectObject(hdc, old_font);
}

// ---------------------------------------------------------------- 設定画面

struct Settings {
    hwnd: HWND,
    dpi: u32,
    fonts: [HFONT; 3],
    made: Vec<(HWND, u8)>,
    /// 区分ごとの行
    sections: Vec<Vec<Row>>,
    footer: Vec<Row>,
    side: HWND,
    panel: HWND,
    current: usize,
    orig: Config,
    /// 一覧で足し引きするもの(別名, 自作コマンド, フォルダー, 名前)はここで持つ
    work: Config,
    autostart: bool,
    exe: String,
    apps: Vec<String>,
    /// Chromeのプロフィール(フォルダー名, 表示名)
    profiles: Vec<(String, String)>,
    edge_profiles: Vec<(String, String)>,
}

/// 借りている間にはできないこと(ほかの窓を出すなど). 借りるのをやめてから行う
enum Task {
    AskClose,
    Pick { id: i32, folder: bool, title: &'static str },
    Editor(EditKind),
    ChangeHotkey,
    ChangeSelectionHotkey,
    ClearSelectionHotkey,
    OpenFile,
}

thread_local! {
    static SETTINGS: RefCell<Option<Settings>> = const { RefCell::new(None) };
    static EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

fn with_settings<R>(f: impl FnOnce(&mut Settings) -> R) -> Option<R> {
    SETTINGS.with(|a| match a.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(f),
        Err(_) => None,
    })
}

fn settings_hwnd() -> Option<HWND> {
    SETTINGS.with(|a| a.try_borrow().ok().and_then(|g| g.as_ref().map(|s| s.hwnd)))
}

fn editor_hwnd() -> Option<HWND> {
    EDITOR.with(|a| a.try_borrow().ok().and_then(|g| g.as_ref().map(|e| e.hwnd)))
}

/// 設定画面を開く. 開いていれば前に出す
pub fn open(a: OpenArgs) {
    if let Some(h) = settings_hwnd() {
        unsafe {
            let _ = ShowWindow(h, SW_RESTORE);
            let _ = SetForegroundWindow(editor_hwnd().unwrap_or(h));
        }
        return;
    }
    register_classes();
    let dpi = dpi_at_cursor();
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let r = centered(dpi, CLIENT_W, CLIENT_H, style);
    let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let title = WStr::new(&tr("Hikidashi Settings"));
    let Ok(hwnd) = (unsafe {
        CreateWindowExW(WS_EX_CONTROLPARENT, CLASS, title.pcwstr(), style, r.left, r.top, r.right - r.left, r.bottom - r.top, None, None, Some(hinst.into()), None)
    }) else {
        return;
    };
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let mut st = build(hwnd, hinst.into(), a, dpi);
    st.show_section(0);
    dark::apply(hwnd);
    let side = st.side;
    SETTINGS.with(|s| *s.borrow_mut() = Some(st));
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(side));
    }
}

/// ホットキーが変わったら表示を直す
pub fn set_hotkey(t: &str) {
    with_settings(|s| set_text(item(s.hwnd, ID_HOTKEY), t));
}

/// 選んだ文字で開くホットキーが変わったら, 表示と(保存済みとして)持っている値を直す. 設定ファイルは呼ぶ側が書く
pub fn set_selection_hotkey(t: &str) {
    with_settings(|s| {
        s.work.hotkeys.selection = t.to_string();
        s.orig.hotkeys.selection = t.to_string();
        set_text(item(s.hwnd, ID_SELHK), if t.is_empty() { "Not set" } else { t });
    });
}

/// メッセージの前処理(Tabで次の部品へ, Enter, Esc). 扱ったらtrue
pub fn pre_translate(msg: &MSG) -> bool {
    for h in [editor_hwnd(), settings_hwnd()].into_iter().flatten() {
        unsafe {
            if msg.hwnd != h && !IsChild(h, msg.hwnd).as_bool() {
                continue;
            }
            if msg.message == WM_MOUSEWHEEL && wheel(h, msg) {
                return true;
            }
            let before = GetFocus();
            if IsDialogMessageW(h, msg).as_bool() {
                // Tabなどで見えない部品に移ったら, 見えるところまでスクロールする
                let f = GetFocus();
                if msg.message == WM_KEYDOWN && f != before && Some(h) == settings_hwnd() {
                    ensure_visible(panel_of(h), f);
                }
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------- 中身のスクロール(K43)

/// 枠のすぐ下の部品(コンボボックスの中の入力欄なら, コンボボックス). 枠の中でなければNone
fn panel_child(panel: HWND, h: HWND) -> Option<HWND> {
    let mut h = h;
    while !h.is_invalid() {
        let p = unsafe { GetAncestor(h, GA_PARENT) };
        if p == panel {
            return Some(h);
        }
        h = p;
    }
    None
}

fn scroll_info(panel: HWND) -> SCROLLINFO {
    let mut si = SCROLLINFO { cbSize: std::mem::size_of::<SCROLLINFO>() as u32, fMask: SIF_ALL, ..Default::default() };
    unsafe {
        let _ = GetScrollInfo(panel, SB_VERT, &mut si);
    }
    si
}

fn scroll_to(panel: HWND, pos: i32) {
    let si = scroll_info(panel);
    let max = (si.nMax - si.nPage as i32 + 1).max(0);
    let pos = pos.clamp(0, max);
    if pos == si.nPos {
        return;
    }
    let new = SCROLLINFO { cbSize: si.cbSize, fMask: SIF_POS, nPos: pos, ..Default::default() };
    unsafe {
        SetScrollInfo(panel, SB_VERT, &new, true);
        ScrollWindowEx(panel, 0, si.nPos - pos, None, None, None, None, SW_SCROLLCHILDREN | SW_INVALIDATE | SW_ERASE);
        let _ = UpdateWindow(panel);
    }
}

fn ensure_visible(panel: HWND, focus: HWND) {
    let Some(h) = panel_child(panel, focus) else { return };
    unsafe {
        let (mut r, mut pr) = (RECT::default(), RECT::default());
        let _ = GetWindowRect(h, &mut r);
        let _ = GetWindowRect(panel, &mut pr);
        let m = s(12, GetDpiForWindow(panel).max(96));
        let pos = scroll_info(panel).nPos;
        if r.top < pr.top {
            // 一番上の部品なら見出しまで戻す
            let to = pos - (pr.top - r.top) - m;
            scroll_to(panel, if to < m * 5 { 0 } else { to });
        } else if r.bottom > pr.bottom {
            scroll_to(panel, pos + (r.bottom - pr.bottom) + m);
        }
    }
}

/// ホイール. 中身の上なら中身をスクロールする(閉じたコンボボックスの値は変えない). 扱ったらtrue
fn wheel(hwnd: HWND, msg: &MSG) -> bool {
    let panel = panel_of(hwnd);
    if panel.is_invalid() {
        return false;
    }
    unsafe {
        // 開いているコンボボックスの一覧を動かしている
        if let Some(f) = panel_child(panel, GetFocus()) {
            if dark::class_name(f) == "ComboBox" && SendMessageW(f, CB_GETDROPPEDSTATE, None, None).0 != 0 {
                return false;
            }
        }
        let under = WindowFromPoint(msg.pt);
        if under != panel {
            let Some(c) = panel_child(panel, under) else { return false };
            // 中身が多い一覧と表は, それ自身をスクロールする
            let own = GetWindowLongPtrW(c, GWL_STYLE) as u32 & WS_VSCROLL.0 != 0;
            if own && ["ListBox", "SysListView32"].contains(&dark::class_name(c).as_str()) {
                return false;
            }
        }
        let delta = (msg.wParam.0 >> 16) as i16 as i32;
        let step = s(60, GetDpiForWindow(panel).max(96));
        scroll_to(panel, scroll_info(panel).nPos - delta * step / 120);
    }
    true
}

extern "system" fn panel_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| panel_inner(hwnd, msg, wparam, lparam))) {
        Ok(r) => r,
        Err(_) => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn panel_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(r) = common_proc(msg, wparam, lparam) {
        return r;
    }
    unsafe {
        match msg {
            // 部品からの知らせは設定画面で受ける
            WM_COMMAND | WM_NOTIFY => SendMessageW(GetAncestor(hwnd, GA_PARENT), msg, Some(wparam), Some(lparam)),
            WM_ERASEBKGND => {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                FillRect(HDC(wparam.0 as *mut _), &rc, brush(pal().bg));
                LRESULT(1)
            }
            WM_VSCROLL => {
                let si = scroll_info(hwnd);
                let line = s(34, GetDpiForWindow(hwnd).max(96));
                let pos = match SCROLLBAR_COMMAND(loword(wparam.0) as i32) {
                    SB_LINEUP => si.nPos - line,
                    SB_LINEDOWN => si.nPos + line,
                    SB_PAGEUP => si.nPos - si.nPage as i32,
                    SB_PAGEDOWN => si.nPos + si.nPage as i32,
                    SB_THUMBTRACK | SB_THUMBPOSITION => si.nTrackPos,
                    SB_TOP => 0,
                    SB_BOTTOM => si.nMax,
                    _ => si.nPos,
                };
                scroll_to(hwnd, pos);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// テーマが変わったら, 開いている設定画面の色を変える
pub fn apply_theme() {
    for h in [settings_hwnd(), editor_hwnd()].into_iter().flatten() {
        dark::apply(h);
    }
}

/// ブラウザーのプロフィールの一覧. `Local State`から名前付きで. 今の設定の値がなければ先頭に足す
fn local_state_profiles(b: actions::Browser, cfg: &Config) -> Vec<(String, String)> {
    let mut v = std::fs::read_to_string(b.user_data(cfg).join("Local State")).map(|t| builtin::chrome_profiles(&t)).unwrap_or_default();
    let cur = b.profile(cfg).to_string();
    if !v.iter().any(|(d, _)| *d == cur) {
        v.insert(0, (cur.clone(), cur));
    }
    v
}

/// 一覧から選ぶコンボボックスで, 今の値を選ぶ
fn select_key(h: HWND, keys: &[(&str, &str)], value: &str) {
    let cur = keys.iter().position(|(k, _)| *k == value).unwrap_or(0);
    unsafe { SendMessageW(h, CB_SETCURSEL, Some(WPARAM(cur)), None) };
}

fn selected_key(h: HWND, keys: &[(&str, &str)]) -> String {
    let i = unsafe { SendMessageW(h, CB_GETCURSEL, None, None).0 };
    keys.get(i.max(0) as usize).map(|(k, _)| k.to_string()).unwrap_or_else(|| keys[0].0.to_string())
}

/// プロフィールの表示名からフォルダー名に戻す(打ち込んだときはそのまま)
fn profile_dir(list: &[(String, String)], text: &str) -> String {
    list.iter().find(|(d, n)| profile_label(d, n) == text).map(|(d, _)| d.clone()).unwrap_or_else(|| text.to_string())
}

fn profile_label(dir: &str, name: &str) -> String {
    if dir == name {
        dir.to_string()
    } else {
        format!("{name} ({dir})")
    }
}

fn alias_rows(c: &Config) -> Vec<Vec<String>> {
    c.alias.iter().map(|(k, v)| vec![k.clone(), v.clone()]).collect()
}

fn command_rows(c: &Config) -> Vec<Vec<String>> {
    c.command
        .iter()
        .map(|m| {
            let what = match (&m.url, &m.run) {
                (Some(u), _) => format!("Open {u}"),
                (_, Some(r)) => format!("Run {r}"),
                _ => String::new(),
            };
            vec![m.name.clone(), m.title.clone().unwrap_or_default(), what]
        })
        .collect()
}

fn open_with_rows(c: &Config) -> Vec<Vec<String>> {
    c.open_with
        .iter()
        .map(|o| {
            let exe = if o.exe.is_empty() { tr("(find automatically)") } else { o.exe.clone() };
            let args = if o.args.is_empty() { tr("(usual way)") } else { o.args.clone() };
            let found = if actions::find_exe(o).is_some() { String::new() } else { tr("  · not installed") };
            vec![format!("{}{found}", o.name), exe, args]
        })
        .collect()
}

const TERMINAL_AUTO: &str = "Automatic (WezTerm, then Windows Terminal)";

/// `Alt+Enter`のターミナルの選び(初めは自動, あとは「〜で開く」の名前)
fn fill_terminals(h: HWND, c: &Config) {
    unsafe { SendMessageW(h, CB_RESETCONTENT, None, None) };
    let mut names = vec![tr(TERMINAL_AUTO)];
    names.extend(c.open_with.iter().map(|o| o.name.clone()));
    for n in &names {
        let t = WStr::new(n);
        unsafe { SendMessageW(h, CB_ADDSTRING, None, Some(LPARAM(t.pcwstr().0 as isize))) };
    }
    let cur = c.open_with.iter().position(|o| o.name.eq_ignore_ascii_case(c.tools.terminal.trim())).map_or(0, |i| i + 1);
    unsafe { SendMessageW(h, CB_SETCURSEL, Some(WPARAM(cur)), None) };
}

fn build(hwnd: HWND, hinst: windows::Win32::Foundation::HINSTANCE, a: OpenArgs, dpi: u32) -> Settings {
    let c = &a.config;
    let mut m = Maker { parent: hwnd, hinst, made: vec![] };
    let side = m.raw(
        w!("LISTBOX"),
        "",
        WS_TABSTOP.0 | (LBS_NOTIFY | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOINTEGRALHEIGHT) as u32,
        WINDOW_EX_STYLE(0),
        ID_SIDE,
        F_BODY,
    );
    fill_list(side, &SECTIONS.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    unsafe {
        SendMessageW(side, LB_SETCURSEL, Some(WPARAM(0)), None);
        let _ = ShowWindow(side, SW_SHOW);
    }

    let panel = unsafe {
        CreateWindowExW(WS_EX_CONTROLPARENT, PANEL_CLASS, PCWSTR::null(), WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_VSCROLL, 0, 0, 10, 10, Some(hwnd), None, Some(hinst), None).unwrap_or_default()
    };
    m.parent = panel;
    let mut sections: Vec<Vec<Row>> = vec![];
    const L: i32 = 110;

    // General
    {
        let mut rows = vec![row(vec![it(m.head("General"), W::Fill, 30)])];
        let hk = if a.hotkey.is_empty() { "Not set".to_string() } else { a.hotkey.clone() };
        rows.push(row(vec![it(m.label("Hotkey"), W::Px(L), 26), it(m.label(&hk), W::Px(160), 26), it(m.button(ID_HOTKEY_CHANGE, "Change…"), W::Px(110), 30)]));
        // 表示用の文字の部品に番号を付け直す
        if let Some(r) = rows.last() {
            if let Some(h) = r.items[1].hwnd {
                unsafe { SetWindowLongPtrW(h, GWLP_ID, ID_HOTKEY as isize) };
            }
        }
        let sel = if c.hotkeys.selection.is_empty() { "Not set".to_string() } else { c.hotkeys.selection.clone() };
        let sel_label = m.label(&sel);
        unsafe { SetWindowLongPtrW(sel_label, GWLP_ID, ID_SELHK as isize) };
        rows.push(row(vec![
            it(m.label("Selection hotkey"), W::Px(L), 26),
            it(sel_label, W::Px(160), 26),
            it(m.button(ID_SELHK_CHANGE, "Change…"), W::Px(110), 30),
            it(m.button(ID_SELHK_CLEAR, "Clear"), W::Px(80), 30),
        ]));
        rows.push(row(vec![it(m.check(ID_AUTOSTART, "Start Hikidashi when Windows starts", system::autostart_enabled()), W::Fill, 26)]));
        let lang_labels: Vec<String> = LANGS.iter().map(|(_, l)| l.to_string()).collect();
        let lc = m.combo(ID_LANG, &lang_labels, false);
        select_key(lc, &LANGS, &c.ui.language);
        rows.push(row(vec![it(m.label("Language / 言語"), W::Px(L), 26), combo_it(lc, W::Px(300))]));
        let theme_labels: Vec<String> = THEMES.iter().map(|(_, l)| l.to_string()).collect();
        let tc = m.combo(ID_THEME, &theme_labels, false);
        select_key(tc, &THEMES, &c.ui.theme);
        rows.push(row(vec![it(m.label("Theme"), W::Px(L), 26), combo_it(tc, W::Px(300))]));
        rows.push(row_gap(14, vec![it(m.sub("Prefixes"), W::Fill, 24)]));
        rows.push(row(vec![it(m.hint("Type a prefix and a space to search only one kind, for example \"f report\"."), W::Fill, 22)]));
        let p = &c.prefix;
        let vals = [&p.app, &p.file, &p.bookmark, &p.kill, &p.wsl, &p.boxdrive, &p.google, &p.chatgpt, &p.clipboard, &p.calc];
        let mut items = vec![];
        for (i, (label, v)) in PREFIX_LABELS.iter().zip(vals).enumerate() {
            if i > 0 && i % 3 == 0 {
                rows.push(row(std::mem::take(&mut items)));
            }
            items.push(it(m.label(label), W::Px(100), 26));
            items.push(it(m.edit(ID_PFX + i as i32, v, false), W::Px(56), 26));
            items.push(gap(12));
        }
        rows.push(row(items));
        // Enterに修飾キーを足したときの操作(2026-11-24)
        rows.push(row_gap(14, vec![it(m.sub("Enter with Ctrl, Shift or Alt"), W::Fill, 24)]));
        let k = &c.keys;
        let choices: Vec<String> = ENTER_KEY_CHOICES.iter().map(|c| if c.is_empty() { "Not used".to_string() } else { c.to_string() }).collect();
        for (i, (label, v)) in KEY_LABELS.iter().zip([&k.chatgpt, &k.google, &k.write, &k.admin, &k.terminal]).enumerate() {
            let combo = m.combo(ID_KEYS + i as i32, &choices, false);
            let cur = parse_enter_key(v).and_then(|mods| ENTER_KEY_CHOICES.iter().position(|c| parse_enter_key(c) == Some(mods))).unwrap_or(0);
            unsafe { SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(cur)), None) };
            rows.push(row(vec![it(m.label(label), W::Px(150), 26), combo_it(combo, W::Px(220))]));
        }
        sections.push(rows);
    }
    // Search
    {
        let mut rows = vec![row(vec![it(m.head("Search"), W::Fill, 30)])];
        rows.push(row(vec![it(m.check(ID_RECENT, "Show recent items when the box is empty", c.search.recent), W::Fill, 26)]));
        rows.push(row(vec![it(m.check(ID_CLIPHIST, "Clipboard history", c.search.clipboard_history), W::Fill, 26)]));
        rows.push(row(vec![gap(24), it(m.hint("Remember what you copy (text and files, in memory only). Type the Clipboard prefix (cb) and a space to see it."), W::Fill, 22)]));
        rows.push(row(vec![it(m.sub("Folders to search"), W::Fill, 22)]));
        rows.push(row(vec![it(m.list(ID_ROOTS, &c.files.roots), W::Fill, 74)]));
        rows.push(row(vec![it(m.button(ID_ROOTS_ADD, "Add…"), W::Px(96), 28), it(m.button(ID_ROOTS_DEL, "Remove"), W::Px(96), 28)]));
        rows.push(row_gap(10, vec![it(m.sub("Skip these folders"), W::Px(170), 22), it(m.hint("and everything inside them"), W::Fill, 20)]));
        rows.push(row(vec![it(m.list(ID_SKIP, &c.files.skip_dirs), W::Fill, 54)]));
        rows.push(row(vec![it(m.button(ID_SKIP_ADD, "Add…"), W::Px(96), 28), it(m.button(ID_SKIP_DEL, "Remove"), W::Px(96), 28)]));
        rows.push(row_gap(10, vec![it(m.sub("Skip these names"), W::Px(170), 22), it(m.hint("folders and files with this name, wherever they are"), W::Fill, 20)]));
        rows.push(row(vec![it(m.list(ID_NAMES, &c.files.exclude), W::Fill, 88)]));
        rows.push(row(vec![
            it(m.edit(ID_NAME_EDIT, "", false), W::Px(170), 26),
            it(m.button(ID_NAME_ADD, "Add"), W::Px(80), 28),
            it(m.button(ID_NAME_DEL, "Remove"), W::Px(96), 28),
            gap(0),
            it(m.button(ID_NAME_RESET, "Restore defaults"), W::Px(150), 28),
        ]));
        sections.push(rows);
    }
    // Box and WSL
    {
        let mut rows = vec![row(vec![it(m.head("Box and WSL"), W::Fill, 30)])];
        // Boxの場所は自動で決める(K26. 入力欄はなくした. 変えるときは設定ファイルの`box_dir`, `box_db`)
        rows.push(row(vec![it(m.sub("Box"), W::Fill, 22)]));
        rows.push(row(vec![it(m.hint("Box is found automatically from Box Drive and read again whenever it changes. Type \"bx \" and a name to search it."), W::Fill, 40)]));
        rows.push(row_gap(10, vec![it(m.sub("WSL"), W::Fill, 22)]));
        rows.push(row(vec![it(m.check(ID_WSL_ON, "Search the home folder in WSL", c.wsl.enabled), W::Fill, 26)]));
        rows.push(row(vec![gap(24), it(m.hint("Type the WSL prefix and a space (\"w report\"). The list is read then, and again if it is older than 5 minutes."), W::Fill, 40)]));
        rows.push(row(vec![it(m.label("Distro"), W::Px(L), 26), it(m.edit(ID_WSL_DISTRO, &c.wsl.distro, false), W::Px(200), 26), it(m.hint("empty: the default distro"), W::Fill, 20)]));
        rows.push(row_gap(10, vec![it(m.sub("Skip these names"), W::Px(170), 22), it(m.hint("folders and files with this name in WSL, wherever they are"), W::Fill, 20)]));
        rows.push(row(vec![it(m.list(ID_WSL_NAMES, &c.wsl.exclude), W::Fill, 88)]));
        rows.push(row(vec![
            it(m.edit(ID_WSL_NAME_EDIT, "", false), W::Px(170), 26),
            it(m.button(ID_WSL_NAME_ADD, "Add"), W::Px(80), 28),
            it(m.button(ID_WSL_NAME_DEL, "Remove"), W::Px(96), 28),
            gap(0),
            it(m.button(ID_WSL_NAME_RESET, "Restore defaults"), W::Px(150), 28),
        ]));
        sections.push(rows);
    }
    // Apps
    {
        let mut rows = vec![row(vec![it(m.head("Apps"), W::Fill, 30)])];
        rows.push(row(vec![it(m.sub("More folders with apps"), W::Px(200), 22), it(m.hint(".exe files here are listed as apps"), W::Fill, 20)]));
        rows.push(row(vec![it(m.list(ID_APPDIRS, &c.apps.extra_dirs), W::Fill, 88)]));
        rows.push(row(vec![it(m.button(ID_APPDIRS_ADD, "Add…"), W::Px(96), 28), it(m.button(ID_APPDIRS_DEL, "Remove"), W::Px(96), 28)]));
        rows.push(row_gap(14, vec![it(m.sub("Open with"), W::Px(200), 22), it(m.hint("\"Open in …\" in the Tab menu. Apps that are not installed are not shown."), W::Fill, 20)]));
        let t = m.table(ID_OPENWITH, &[("Name", s(170, dpi)), ("App", s(190, dpi)), ("Passes", s(150, dpi))]);
        table_fill(t, &open_with_rows(c));
        rows.push(row(vec![it(t, W::Fill, 120)]));
        rows.push(row(vec![
            it(m.button(ID_OW_ADD, "Add…"), W::Px(96), 28),
            it(m.button(ID_OW_EDIT, "Edit…"), W::Px(96), 28),
            it(m.button(ID_OW_DEL, "Remove"), W::Px(96), 28),
        ]));
        let tc = m.combo(ID_TERMINAL, &[], false);
        fill_terminals(tc, c);
        rows.push(row_gap(10, vec![it(m.label("Terminal"), W::Px(L), 26), combo_it(tc, W::Fill)]));
        rows.push(row(vec![gap(L), it(m.hint("\"Open in terminal\" (General, Alt+Enter at first) goes into the folder of the selected file or folder in this terminal."), W::Fill, 20)]));
        sections.push(rows);
    }
    // Tab menu(K24): 項目×種類のチェック. 種類に合わないところは空ける
    {
        let mut rows = vec![row(vec![it(m.head("Tab menu"), W::Fill, 30)])];
        rows.push(row(vec![it(m.hint("Choose what the Tab menu shows for apps, files and folders. Things you can do with a key (Enter, Alt+Enter, Ctrl+Shift+Enter) can be left out."), W::Fill, 40)]));
        const LW: i32 = 290;
        const CW: i32 = 80;
        rows.push(row_gap(6, vec![gap(LW), it(m.sub("Apps"), W::Px(CW), 22), it(m.sub("Files"), W::Px(CW), 22), it(m.sub("Folders"), W::Px(CW), 22)]));
        for (i, (name, label)) in config::MENU_ITEMS.iter().zip(MENU_LABELS).enumerate() {
            let mut items = vec![it(m.label(label), W::Px(LW), 26)];
            for (k, kind) in MENU_KINDS.iter().enumerate() {
                if config::menu_item_fits(kind, name) {
                    items.push(it(m.check(ID_MENU + (i * 3 + k) as i32, "", c.menu.has(kind, name)), W::Px(CW), 26));
                } else {
                    items.push(it(m.label("—"), W::Px(CW), 26));
                }
            }
            rows.push(row(items));
        }
        sections.push(rows);
    }
    // Web
    let profiles = local_state_profiles(actions::Browser::Chrome, c);
    let edge_profiles = local_state_profiles(actions::Browser::Edge, c);
    {
        let mut rows = vec![row(vec![it(m.head("Web"), W::Fill, 30)])];
        let labels = |v: &[(&str, &str)]| v.iter().map(|(_, l)| l.to_string()).collect::<Vec<_>>();
        let bc = m.combo(ID_BROWSER, &labels(&BROWSERS), false);
        select_key(bc, &BROWSERS, &c.browser.browser);
        rows.push(row(vec![it(m.label("Browser"), W::Px(L), 26), combo_it(bc, W::Px(300))]));
        let oc = m.combo(ID_OPEN_IN, &labels(&OPEN_IN), false);
        select_key(oc, &OPEN_IN, &c.browser.open_in);
        rows.push(row(vec![it(m.label("Open pages in"), W::Px(L), 26), combo_it(oc, W::Px(300))]));
        for (id, list, cur, label) in [
            (ID_PROFILE, &profiles, &c.browser.chrome_profile, "Chrome profile"),
            (ID_EDGE_PROFILE, &edge_profiles, &c.browser.edge_profile, "Edge profile"),
        ] {
            let names: Vec<String> = list.iter().map(|(d, n)| profile_label(d, n)).collect();
            let combo = m.combo(id, &names, true);
            let i = list.iter().position(|(d, _)| d == cur).unwrap_or(0);
            unsafe { SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(i)), None) };
            rows.push(row(vec![it(m.label(label), W::Px(L), 26), combo_it(combo, W::Px(300))]));
        }
        rows.push(row(vec![gap(L), it(m.hint("Bookmarks are read from both browsers. Pages open in these profiles."), W::Fill, 20)]));
        rows.push(row(vec![it(m.label("chrome.exe"), W::Px(L), 26), it(m.edit(ID_CHROME, &c.browser.chrome_exe, false), W::Fill, 26), it(m.button(ID_CHROME_BROWSE, "Browse…"), W::Px(96), 28)]));
        let modes: Vec<String> = GPT_MODES.iter().map(|(_, l)| l.to_string()).collect();
        let gm = m.combo(ID_GPT_MODE, &modes, false);
        let cur = GPT_MODES.iter().position(|(k, _)| *k == c.chatgpt.mode).unwrap_or(0);
        unsafe { SendMessageW(gm, CB_SETCURSEL, Some(WPARAM(cur)), None) };
        rows.push(row_gap(10, vec![it(m.label("ChatGPT"), W::Px(L), 26), combo_it(gm, W::Fill)]));
        rows.push(row(vec![gap(L), it(m.hint("Use \"Copy the question\" if the question stops being sent."), W::Fill, 20)]));
        rows.push(row(vec![
            gap(L),
            it(m.check(ID_GPT_SEND, "Press Enter in ChatGPT after", c.chatgpt.send_enter), W::Px(230), 26),
            it(m.edit(ID_GPT_DELAY, &c.chatgpt.send_delay_ms.to_string(), true), W::Px(70), 26),
            it(m.hint("ms (only when ChatGPT is in front)"), W::Fill, 20),
        ]));
        rows.push(row(vec![gap(L), it(m.check(ID_GPT_HIDE, "Long questions: hide Markdown marks except on the line you edit", c.chatgpt.hide_marks), W::Fill, 26)]));
        rows.push(row(vec![it(m.label("ChatGPT URL"), W::Px(L), 26), it(m.edit(ID_GPT_URL, &c.chatgpt.url, false), W::Fill, 26)]));
        rows.push(row(vec![it(m.label("Google URL"), W::Px(L), 26), it(m.edit(ID_GOOGLE, &c.google.url, false), W::Fill, 26)]));
        rows.push(row(vec![gap(L), it(m.hint("{q} is replaced with what you type."), W::Fill, 20)]));
        sections.push(rows);
    }
    // Aliases
    {
        let mut rows = vec![row(vec![it(m.head("Aliases"), W::Fill, 30)])];
        rows.push(row(vec![it(m.hint("Type the name on the left to find the app on the right, for example ppt for PowerPoint."), W::Fill, 22)]));
        let t = m.table(ID_ALIASES, &[("Type", s(150, dpi)), ("App", s(360, dpi))]);
        table_fill(t, &alias_rows(c));
        rows.push(row(vec![it(t, W::Fill, 330)]));
        rows.push(row(vec![
            it(m.button(ID_ALIAS_ADD, "Add…"), W::Px(96), 28),
            it(m.button(ID_ALIAS_EDIT, "Edit…"), W::Px(96), 28),
            it(m.button(ID_ALIAS_DEL, "Remove"), W::Px(96), 28),
        ]));
        sections.push(rows);
    }
    // Commands
    {
        let mut rows = vec![row(vec![it(m.head("Commands"), W::Fill, 30)])];
        rows.push(row(vec![it(
            m.hint("Type the name to open a web page or run a command. {q} is what you type after the name, for example \"jira ABC-123\"."),
            W::Fill,
            40,
        )]));
        let t = m.table(ID_CMDS, &[("Name", s(100, dpi)), ("Shown as", s(150, dpi)), ("Does", s(280, dpi))]);
        table_fill(t, &command_rows(c));
        rows.push(row(vec![it(t, W::Fill, 312)]));
        rows.push(row(vec![
            it(m.button(ID_CMD_ADD, "Add…"), W::Px(96), 28),
            it(m.button(ID_CMD_EDIT, "Edit…"), W::Px(96), 28),
            it(m.button(ID_CMD_DEL, "Remove"), W::Px(96), 28),
        ]));
        sections.push(rows);
    }
    // 下の帯
    m.parent = hwnd;
    let open_file = m.button(ID_OPENFILE, "Open config file");
    let err = m.raw(w!("STATIC"), "", (SS_LEFT.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), ID_ERROR, F_BODY);
    tint(err, |p| p.danger);
    if let Some(e) = &a.error {
        set_text(err, &format!("{e}. Defaults are shown; Save replaces the file."));
    }
    let cancel = m.button(ID_CANCEL, "Cancel");
    let save = m.button(ID_SAVE, "Save");
    let footer = vec![row(vec![it(open_file, W::Px(130), 28), it(err, W::Fill, 40), it(cancel, W::Px(96), 32), it(save, W::Px(96), 32)])];
    for h in [open_file, err, cancel, save] {
        unsafe {
            let _ = ShowWindow(h, SW_SHOW);
        }
    }
    let fonts = fonts_for(dpi);
    apply_fonts(&m.made, &fonts);
    Settings {
        hwnd,
        dpi,
        fonts,
        made: m.made,
        sections,
        footer,
        side,
        panel,
        current: 0,
        orig: a.config.clone(),
        work: a.config,
        autostart: system::autostart_enabled(),
        exe: a.exe,
        apps: a.apps,
        profiles,
        edge_profiles,
    }
}

impl Settings {
    fn layout(&self) {
        let d = self.dpi;
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let client_h = (rc.bottom as i64 * 96 / d as i64) as i32;
        // 下の帯の線の上まで
        let panel_h = (rc.bottom - s(FOOT_H, d)).max(0);
        unsafe {
            let _ = SetWindowPos(self.side, None, s(8, d), s(16, d), s(SIDE_W - 16, d), s(client_h - FOOT_H - 24, d), SWP_NOZORDER | SWP_NOACTIVATE);
            SendMessageW(self.side, LB_SETITEMHEIGHT, Some(WPARAM(0)), Some(LPARAM(s(36, d) as isize)));
            let _ = SetWindowPos(self.panel, None, s(SIDE_W, d), 0, rc.right - s(SIDE_W, d), panel_h, SWP_NOZORDER | SWP_NOACTIVATE);
        }
        let width = CLIENT_W - SIDE_W - 2 * PAD;
        let mut bottom = 0;
        for (i, rows) in self.sections.iter().enumerate() {
            let b = place(rows, PAD, 16, width, d);
            if i == self.current {
                bottom = b;
            }
        }
        place(&self.footer, SIDE_W + PAD - 8, client_h - FOOT_H + 12, width + 8, d);
        unsafe {
            // 入りきるときは, スクロールの帯は隠れる
            let si = SCROLLINFO {
                cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
                nMin: 0,
                nMax: s(bottom + 4, d) - 1,
                nPage: panel_h as u32,
                nPos: 0,
                ..Default::default()
            };
            SetScrollInfo(self.panel, SB_VERT, &si, true);
            let _ = RedrawWindow(Some(self.hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
        }
    }

    fn show_section(&mut self, i: usize) {
        self.current = i;
        for (k, rows) in self.sections.iter().enumerate() {
            for r in rows {
                for it in &r.items {
                    if let Some(h) = it.hwnd {
                        unsafe {
                            let _ = ShowWindow(h, if k == i { SW_SHOW } else { SW_HIDE });
                        }
                    }
                }
            }
        }
        unsafe {
            SendMessageW(self.side, LB_SETCURSEL, Some(WPARAM(i)), None);
        }
        self.layout();
    }

    fn set_dpi(&mut self, dpi: u32) {
        self.dpi = dpi;
        let old = self.fonts;
        self.fonts = fonts_for(dpi);
        apply_fonts(&self.made, &self.fonts);
        delete_fonts(&old);
        // 表の列の幅
        let cols: [(i32, &[i32]); 3] = [(ID_ALIASES, &[150, 360]), (ID_CMDS, &[100, 150, 280]), (ID_OPENWITH, &[170, 190, 150])];
        for (id, ws) in cols {
            for (i, wv) in ws.iter().enumerate() {
                unsafe { SendMessageW(item(self.hwnd, id), LVM_SETCOLUMNWIDTH, Some(WPARAM(i)), Some(LPARAM(s(*wv, dpi) as isize))) };
            }
        }
        self.layout();
    }

    fn paint(&self) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(self.hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut rc);
            FillRect(hdc, &rc, brush(pal().bg));
            let d = self.dpi;
            let side = RECT { left: 0, top: 0, right: s(SIDE_W, d), bottom: rc.bottom - s(FOOT_H, d) };
            FillRect(hdc, &side, brush(pal().bg_subtle));
            let line = RECT { left: 0, top: rc.bottom - s(FOOT_H, d), right: rc.right, bottom: rc.bottom - s(FOOT_H, d) + 1.max(s(1, d)) };
            FillRect(hdc, &line, brush(pal().border_faint));
            let _ = EndPaint(self.hwnd, &ps);
        }
    }

    fn error(&self, msg: &str) {
        self.footer_text(msg, |p| p.danger);
    }

    /// 下の帯の文字(エラーは赤, 保存できた知らせは薄い色)
    fn footer_text(&self, msg: &str, color: Color) {
        let h = item(self.hwnd, ID_ERROR);
        tint(h, color);
        set_text(h, msg);
        unsafe {
            let _ = InvalidateRect(Some(h), None, true);
        }
    }

    /// 画面の値から設定を作る
    fn collect(&self) -> Result<Config, String> {
        let h = self.hwnd;
        let mut c = self.work.clone();
        let t = |id: i32| text(item(h, id)).trim().to_string();
        c.prefix.app = t(ID_PFX);
        c.prefix.file = t(ID_PFX + 1);
        c.prefix.bookmark = t(ID_PFX + 2);
        c.prefix.kill = t(ID_PFX + 3);
        c.prefix.wsl = t(ID_PFX + 4);
        c.prefix.boxdrive = t(ID_PFX + 5);
        c.prefix.google = t(ID_PFX + 6);
        c.prefix.chatgpt = t(ID_PFX + 7);
        c.prefix.clipboard = t(ID_PFX + 8);
        c.prefix.calc = t(ID_PFX + 9);
        c.search.recent = checked(item(h, ID_RECENT));
        c.search.clipboard_history = checked(item(h, ID_CLIPHIST));
        c.ui.language = selected_key(item(h, ID_LANG), &LANGS);
        c.ui.theme = selected_key(item(h, ID_THEME), &THEMES);
        for (k, kind) in MENU_KINDS.iter().enumerate() {
            let list: Vec<String> = config::MENU_ITEMS
                .iter()
                .enumerate()
                .filter(|(i, name)| config::menu_item_fits(kind, name) && checked(item(h, ID_MENU + (i * 3 + k) as i32)))
                .map(|(_, name)| name.to_string())
                .collect();
            match k {
                0 => c.menu.app = list,
                1 => c.menu.file = list,
                _ => c.menu.folder = list,
            }
        }
        let key = |i: i32| {
            let n = unsafe { SendMessageW(item(h, ID_KEYS + i), CB_GETCURSEL, None, None).0 };
            ENTER_KEY_CHOICES.get(n.max(0) as usize).copied().unwrap_or("").to_string()
        };
        c.keys.chatgpt = key(0);
        c.keys.google = key(1);
        c.keys.write = key(2);
        c.keys.admin = key(3);
        c.keys.terminal = key(4);
        c.wsl.enabled = checked(item(h, ID_WSL_ON));
        c.wsl.distro = t(ID_WSL_DISTRO);
        let ti = unsafe { SendMessageW(item(h, ID_TERMINAL), CB_GETCURSEL, None, None).0 };
        c.tools.terminal = if ti >= 1 { c.open_with.get(ti as usize - 1).map(|o| o.name.clone()).unwrap_or_default() } else { String::new() };
        c.chatgpt.send_enter = checked(item(h, ID_GPT_SEND));
        c.chatgpt.hide_marks = checked(item(h, ID_GPT_HIDE));
        c.chatgpt.send_delay_ms = t(ID_GPT_DELAY).parse().map_err(|_| "Wait before Enter must be a number of ms.".to_string())?;
        c.browser.browser = selected_key(item(h, ID_BROWSER), &BROWSERS);
        c.browser.open_in = selected_key(item(h, ID_OPEN_IN), &OPEN_IN);
        c.browser.chrome_exe = t(ID_CHROME);
        c.browser.chrome_profile = profile_dir(&self.profiles, &t(ID_PROFILE));
        c.browser.edge_profile = profile_dir(&self.edge_profiles, &t(ID_EDGE_PROFILE));
        if c.browser.chrome_profile.is_empty() || c.browser.edge_profile.is_empty() {
            return Err("Choose a browser profile (Default if unsure).".into());
        }
        let gm = unsafe { SendMessageW(item(h, ID_GPT_MODE), CB_GETCURSEL, None, None).0 };
        c.chatgpt.mode = GPT_MODES.get(gm.max(0) as usize).map(|(k, _)| k.to_string()).unwrap_or_else(|| "url".into());
        c.chatgpt.url = t(ID_GPT_URL);
        c.google.url = t(ID_GOOGLE);
        c.validate()?;
        Ok(c)
    }

    fn dirty(&self) -> bool {
        match self.collect() {
            Ok(c) => c != self.orig || checked(item(self.hwnd, ID_AUTOSTART)) != self.autostart,
            Err(_) => true,
        }
    }

    fn save(&mut self) -> Vec<Task> {
        let c = match self.collect() {
            Ok(c) => c,
            Err(e) => {
                self.error(&e);
                return vec![];
            }
        };
        if let Err(e) = storage::write_config(&config::to_toml(&c)) {
            self.error(&format!("Cannot save config.toml: {e}"));
            return vec![];
        }
        let on = checked(item(self.hwnd, ID_AUTOSTART));
        if on != self.autostart {
            system::set_autostart(on, &self.exe);
        }
        // 保存しても閉じない(K28). 今の値を「保存した値」にして, 続けて直せるようにする
        let lang_changed = c.ui.language != self.orig.ui.language;
        self.orig = c.clone();
        self.work = c;
        self.autostart = on;
        self.footer_text(if lang_changed { "Saved. Close and open Settings again to change the language." } else { "Saved. The changes are in effect now." }, |p| p.text_secondary);
        vec![]
    }

    fn list_for(&mut self, id: i32) -> Option<&mut Vec<String>> {
        match id {
            ID_ROOTS => Some(&mut self.work.files.roots),
            ID_SKIP => Some(&mut self.work.files.skip_dirs),
            ID_NAMES => Some(&mut self.work.files.exclude),
            ID_WSL_NAMES => Some(&mut self.work.wsl.exclude),
            ID_APPDIRS => Some(&mut self.work.apps.extra_dirs),
            _ => None,
        }
    }

    fn list_add(&mut self, id: i32, value: String) {
        let value = value.trim().to_string();
        if value.is_empty() {
            return;
        }
        let h = item(self.hwnd, id);
        let Some(v) = self.list_for(id) else { return };
        let pos = match v.iter().position(|x| x.eq_ignore_ascii_case(&value)) {
            Some(p) => p,
            None => {
                v.push(value);
                v.len() - 1
            }
        };
        let items = v.clone();
        fill_list(h, &items);
        unsafe { SendMessageW(h, LB_SETCURSEL, Some(WPARAM(pos)), None) };
    }

    fn list_remove(&mut self, id: i32) {
        let h = item(self.hwnd, id);
        let Some(i) = list_sel(h) else { return };
        let Some(v) = self.list_for(id) else { return };
        if i < v.len() {
            v.remove(i);
        }
        let items = v.clone();
        let n = items.len();
        fill_list(h, &items);
        if n > 0 {
            unsafe { SendMessageW(h, LB_SETCURSEL, Some(WPARAM(i.min(n - 1))), None) };
        }
    }

    /// 選んだフォルダー・exeを入れる
    fn picked(&mut self, id: i32, path: String) {
        let p = short_path(&path);
        match id {
            ID_ROOTS_ADD => self.list_add(ID_ROOTS, p),
            ID_SKIP_ADD => self.list_add(ID_SKIP, p),
            ID_APPDIRS_ADD => self.list_add(ID_APPDIRS, p),
            ID_CHROME_BROWSE => set_text(item(self.hwnd, ID_CHROME), &p),
            _ => {}
        }
    }

    /// 名前の入力欄(`edit`)の名前を一覧(`list`)に足す. 空白で区切れば一度に何個でも
    fn add_name(&mut self, edit: i32, list: i32) {
        let e = item(self.hwnd, edit);
        let t = text(e);
        for name in t.split_whitespace() {
            self.list_add(list, name.to_string());
        }
        set_text(e, "");
        unsafe {
            let _ = SetFocus(Some(e));
        }
    }

    fn alias_keys(&self) -> Vec<String> {
        self.work.alias.keys().cloned().collect()
    }

    fn edit_selected(&self, id: i32) -> Option<Task> {
        match id {
            ID_ALIASES => {
                let i = table_sel(item(self.hwnd, ID_ALIASES))?;
                let k = self.alias_keys().get(i)?.clone();
                let v = self.work.alias.get(&k)?.clone();
                Some(Task::Editor(EditKind::Alias { orig: Some(k.clone()), name: k, app: v, apps: self.apps.clone() }))
            }
            ID_CMDS => {
                let i = table_sel(item(self.hwnd, ID_CMDS))?;
                let c = self.work.command.get(i)?.clone();
                Some(Task::Editor(EditKind::Command { index: Some(i), cmd: c }))
            }
            ID_OPENWITH => {
                let i = table_sel(item(self.hwnd, ID_OPENWITH))?;
                let o = self.work.open_with.get(i)?.clone();
                Some(Task::Editor(EditKind::OpenWith { index: Some(i), ow: o }))
            }
            _ => None,
        }
    }

    /// 項目を直す窓のOK. 問題があれば文を返す
    fn apply_edit(&mut self, kind: &EditKind) -> Result<(), String> {
        match kind {
            EditKind::Alias { orig, name, app, .. } => {
                if name.is_empty() || name.chars().any(char::is_whitespace) {
                    return Err("Type a name without spaces.".into());
                }
                if app.is_empty() {
                    return Err("Choose or type the app name.".into());
                }
                if orig.as_deref() != Some(name.as_str()) && self.work.alias.contains_key(name) {
                    return Err(format!("\"{name}\" is already an alias."));
                }
                if let Some(o) = orig {
                    self.work.alias.remove(o);
                }
                self.work.alias.insert(name.clone(), app.clone());
                let t = item(self.hwnd, ID_ALIASES);
                table_fill(t, &alias_rows(&self.work));
                if let Some(i) = self.alias_keys().iter().position(|k| k == name) {
                    table_select(t, i);
                }
            }
            EditKind::Command { index, cmd } => {
                if cmd.name.is_empty() || cmd.name.chars().any(char::is_whitespace) {
                    return Err("Type a name without spaces.".into());
                }
                if cmd.url.is_none() && cmd.run.is_none() {
                    return Err("Type the web address or the command.".into());
                }
                if self.work.command.iter().enumerate().any(|(i, c)| Some(i) != *index && c.name.eq_ignore_ascii_case(&cmd.name)) {
                    return Err(format!("\"{}\" is already a command.", cmd.name));
                }
                let i = match index {
                    Some(i) if *i < self.work.command.len() => {
                        self.work.command[*i] = cmd.clone();
                        *i
                    }
                    _ => {
                        self.work.command.push(cmd.clone());
                        self.work.command.len() - 1
                    }
                };
                let t = item(self.hwnd, ID_CMDS);
                table_fill(t, &command_rows(&self.work));
                table_select(t, i);
            }
            EditKind::OpenWith { index, ow } => {
                if ow.name.is_empty() {
                    return Err("Type a name.".into());
                }
                if ow.exe.is_empty() && hikidashi_core::openwith::ToolKind::of(ow) == hikidashi_core::openwith::ToolKind::Other {
                    return Err("Type where the .exe is (only VS Code, WezTerm and Windows Terminal are found automatically).".into());
                }
                if self.work.open_with.iter().enumerate().any(|(i, o)| Some(i) != *index && o.name.eq_ignore_ascii_case(&ow.name)) {
                    return Err(format!("\"{}\" is already in the list.", ow.name));
                }
                // 名前を変えたら, Alt+Enterの選びも付いていく
                let term = self.terminal_name();
                let i = match index {
                    Some(i) if *i < self.work.open_with.len() => {
                        let old = std::mem::replace(&mut self.work.open_with[*i], ow.clone());
                        if term.eq_ignore_ascii_case(&old.name) {
                            self.work.tools.terminal = ow.name.clone();
                        } else {
                            self.work.tools.terminal = term;
                        }
                        *i
                    }
                    _ => {
                        self.work.tools.terminal = term;
                        self.work.open_with.push(ow.clone());
                        self.work.open_with.len() - 1
                    }
                };
                self.refresh_open_with(Some(i));
            }
        }
        Ok(())
    }

    /// 画面で選んでいる`Alt+Enter`のターミナルの名前(自動なら空)
    fn terminal_name(&self) -> String {
        let ti = unsafe { SendMessageW(item(self.hwnd, ID_TERMINAL), CB_GETCURSEL, None, None).0 };
        if ti >= 1 { self.work.open_with.get(ti as usize - 1).map(|o| o.name.clone()).unwrap_or_default() } else { String::new() }
    }

    /// 「〜で開く」の表と, Alt+Enterの選びを作り直す
    fn refresh_open_with(&mut self, sel: Option<usize>) {
        let t = item(self.hwnd, ID_OPENWITH);
        table_fill(t, &open_with_rows(&self.work));
        if let Some(i) = sel.filter(|_| !self.work.open_with.is_empty()) {
            table_select(t, i.min(self.work.open_with.len() - 1));
        }
        if !self.work.open_with.iter().any(|o| o.name.eq_ignore_ascii_case(&self.work.tools.terminal)) {
            self.work.tools.terminal.clear();
        }
        fill_terminals(item(self.hwnd, ID_TERMINAL), &self.work);
    }

    fn remove_selected(&mut self, id: i32) {
        let t = item(self.hwnd, id);
        let Some(i) = table_sel(t) else { return };
        if id == ID_OPENWITH {
            self.work.tools.terminal = self.terminal_name();
            if i < self.work.open_with.len() {
                self.work.open_with.remove(i);
            }
            self.refresh_open_with(Some(i));
            return;
        }
        if id == ID_ALIASES {
            if let Some(k) = self.alias_keys().get(i) {
                self.work.alias.remove(k);
            }
            table_fill(t, &alias_rows(&self.work));
        } else {
            if i < self.work.command.len() {
                self.work.command.remove(i);
            }
            table_fill(t, &command_rows(&self.work));
        }
        let n = if id == ID_ALIASES { self.work.alias.len() } else { self.work.command.len() };
        if n > 0 {
            table_select(t, i.min(n - 1));
        }
    }

    fn command(&mut self, id: i32, code: u32) -> Vec<Task> {
        match id {
            ID_SIDE if code == LBN_SELCHANGE => {
                if let Some(i) = list_sel(self.side) {
                    self.show_section(i);
                }
                vec![]
            }
            ID_SAVE => {
                // Enterで押されたとき. 名前の入力欄なら足すだけ, 表なら直す
                let f = unsafe { GetFocus() };
                for (edit, list) in [(ID_NAME_EDIT, ID_NAMES), (ID_WSL_NAME_EDIT, ID_WSL_NAMES)] {
                    if f == item(self.hwnd, edit) {
                        self.add_name(edit, list);
                        return vec![];
                    }
                }
                for t in [ID_ALIASES, ID_CMDS, ID_OPENWITH] {
                    if f == item(self.hwnd, t) {
                        return self.edit_selected(t).into_iter().collect();
                    }
                }
                self.save()
            }
            ID_CANCEL => vec![Task::AskClose],
            ID_OPENFILE => vec![Task::OpenFile],
            ID_HOTKEY_CHANGE => vec![Task::ChangeHotkey],
            ID_SELHK_CHANGE => vec![Task::ChangeSelectionHotkey],
            ID_SELHK_CLEAR => vec![Task::ClearSelectionHotkey],
            ID_ROOTS_ADD => vec![Task::Pick { id, folder: true, title: "Choose a folder to search" }],
            ID_SKIP_ADD => vec![Task::Pick { id, folder: true, title: "Choose a folder to skip" }],
            ID_APPDIRS_ADD => vec![Task::Pick { id, folder: true, title: "Choose a folder with apps" }],
            ID_CHROME_BROWSE => vec![Task::Pick { id, folder: false, title: "Choose chrome.exe" }],
            ID_ROOTS_DEL => {
                self.list_remove(ID_ROOTS);
                vec![]
            }
            ID_SKIP_DEL => {
                self.list_remove(ID_SKIP);
                vec![]
            }
            ID_APPDIRS_DEL => {
                self.list_remove(ID_APPDIRS);
                vec![]
            }
            ID_NAME_ADD => {
                self.add_name(ID_NAME_EDIT, ID_NAMES);
                vec![]
            }
            ID_WSL_NAME_ADD => {
                self.add_name(ID_WSL_NAME_EDIT, ID_WSL_NAMES);
                vec![]
            }
            ID_WSL_NAME_DEL => {
                self.list_remove(ID_WSL_NAMES);
                vec![]
            }
            ID_WSL_NAME_RESET => {
                self.work.wsl.exclude = Wsl::default().exclude;
                fill_list(item(self.hwnd, ID_WSL_NAMES), &self.work.wsl.exclude);
                vec![]
            }
            ID_NAME_DEL => {
                self.list_remove(ID_NAMES);
                vec![]
            }
            ID_NAME_RESET => {
                self.work.files.exclude = Files::default().exclude;
                fill_list(item(self.hwnd, ID_NAMES), &self.work.files.exclude);
                vec![]
            }
            ID_ALIAS_ADD => vec![Task::Editor(EditKind::Alias { orig: None, name: String::new(), app: String::new(), apps: self.apps.clone() })],
            ID_CMD_ADD => vec![Task::Editor(EditKind::Command { index: None, cmd: Command { name: String::new(), title: None, url: Some(String::new()), run: None } })],
            ID_ALIAS_EDIT => self.edit_selected(ID_ALIASES).into_iter().collect(),
            ID_CMD_EDIT => self.edit_selected(ID_CMDS).into_iter().collect(),
            ID_ALIAS_DEL => {
                self.remove_selected(ID_ALIASES);
                vec![]
            }
            ID_CMD_DEL => {
                self.remove_selected(ID_CMDS);
                vec![]
            }
            ID_OW_ADD => vec![Task::Editor(EditKind::OpenWith { index: None, ow: OpenWith { name: String::new(), exe: String::new(), args: String::new() } })],
            ID_OW_EDIT => self.edit_selected(ID_OPENWITH).into_iter().collect(),
            ID_OW_DEL => {
                self.remove_selected(ID_OPENWITH);
                vec![]
            }
            _ => vec![],
        }
    }
}

fn run_tasks(tasks: Vec<Task>) {
    for t in tasks {
        let Some(hwnd) = settings_hwnd() else { return };
        match t {
            Task::AskClose => {
                let dirty = with_settings(|s| s.dirty()).unwrap_or(false);
                if !dirty || system::message_box_yes_no(hwnd, "Discard your changes?", "Hikidashi Settings") {
                    close_settings();
                }
            }
            Task::Pick { id, folder, title } => {
                if let Some(p) = pick(hwnd, folder, title) {
                    with_settings(|s| s.picked(id, p));
                }
            }
            Task::Editor(kind) => open_editor(hwnd, kind),
            Task::ChangeHotkey => {
                super::with_app(|a| a.start_capture(false));
            }
            Task::ChangeSelectionHotkey => {
                super::with_app(|a| a.start_capture(true));
            }
            Task::ClearSelectionHotkey => {
                super::with_app(|a| a.set_selection_hotkey(None));
            }
            Task::OpenFile => {
                let dirty = with_settings(|s| s.dirty()).unwrap_or(false);
                if dirty && !system::message_box_yes_no(hwnd, "Open config.toml and discard the changes made here?", "Hikidashi Settings") {
                    continue;
                }
                close_settings();
                let p = storage::config_path();
                let _ = actions::shell("open", "notepad.exe", &format!("\"{}\"", p.display()), "", false);
            }
        }
    }
}

fn close_settings() {
    if let Some(h) = editor_hwnd() {
        let e = EDITOR.with(|e| e.borrow_mut().take());
        unsafe {
            let _ = DestroyWindow(h);
        }
        if let Some(e) = e {
            delete_fonts(&e.fonts);
        }
    }
    let st = SETTINGS.with(|s| s.borrow_mut().take());
    if let Some(st) = st {
        unsafe {
            let _ = DestroyWindow(st.hwnd);
        }
        delete_fonts(&st.fonts);
        TINT.with(|t| t.borrow_mut().clear());
    }
}

extern "system" fn settings_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| settings_inner(hwnd, msg, wparam, lparam))) {
        Ok(r) => r,
        Err(_) => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn settings_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(r) = common_proc(msg, wparam, lparam) {
        return r;
    }
    let mut tasks = vec![];
    let r = with_settings(|st| -> Option<LRESULT> {
        match msg {
            WM_PAINT => {
                st.paint();
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_COMMAND => {
                let id = loword(wparam.0) as i32;
                let code = hiword(wparam.0) as u32;
                tasks = st.command(id, code);
                Some(LRESULT(0))
            }
            WM_NOTIFY => {
                let nm = unsafe { &*(lparam.0 as *const NMHDR) };
                if nm.code == NM_DBLCLK && [ID_ALIASES, ID_CMDS, ID_OPENWITH].contains(&(nm.idFrom as i32)) {
                    tasks = st.edit_selected(nm.idFrom as i32).into_iter().collect();
                    return Some(LRESULT(0));
                }
                None
            }
            WM_CLOSE => {
                tasks = vec![Task::AskClose];
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                st.set_dpi(hiword(wparam.0) as u32);
                let r = unsafe { &*(lparam.0 as *const RECT) };
                unsafe {
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                }
                st.layout();
                Some(LRESULT(0))
            }
            _ => None,
        }
    });
    run_tasks(tasks);
    match r {
        Some(Some(r)) => r,
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

// ---------------------------------------------------------------- 別名・自作コマンドを直す小さな窓

#[derive(Clone)]
enum EditKind {
    Alias { orig: Option<String>, name: String, app: String, apps: Vec<String> },
    Command { index: Option<usize>, cmd: Command },
    OpenWith { index: Option<usize>, ow: OpenWith },
}

struct Editor {
    hwnd: HWND,
    owner: HWND,
    kind: EditKind,
    fonts: [HFONT; 3],
}

const ED_W: i32 = 480;

fn open_editor(owner: HWND, kind: EditKind) {
    if editor_hwnd().is_some() {
        return;
    }
    let dpi = unsafe { GetDpiForWindow(owner) }.max(96);
    let (title, ch) = match &kind {
        EditKind::Alias { orig: None, .. } => ("Add alias", 210),
        EditKind::Alias { .. } => ("Edit alias", 210),
        EditKind::Command { index: None, .. } => ("Add command", 300),
        EditKind::Command { .. } => ("Edit command", 300),
        EditKind::OpenWith { index: None, .. } => ("Add app to open with", 290),
        EditKind::OpenWith { .. } => ("Edit app to open with", 290),
    };
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
    let mut r = RECT { left: 0, top: 0, right: s(ED_W, dpi), bottom: s(ch, dpi) };
    let mut or = RECT::default();
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut r, style, false, WINDOW_EX_STYLE(0), dpi);
        let _ = GetWindowRect(owner, &mut or);
    }
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let x = or.left + ((or.right - or.left) - w) / 2;
    let y = or.top + ((or.bottom - or.top) - h) / 2;
    let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let t = WStr::new(&tr(title));
    let Ok(hwnd) = (unsafe { CreateWindowExW(WS_EX_CONTROLPARENT | WS_EX_DLGMODALFRAME, EDITOR_CLASS, t.pcwstr(), style, x, y, w, h, Some(owner), None, Some(hinst.into()), None) })
    else {
        return;
    };
    let mut m = Maker { parent: hwnd, hinst: hinst.into(), made: vec![] };
    let mut rows = vec![];
    const L: i32 = 110;
    match &kind {
        EditKind::Alias { name, app, apps, .. } => {
            rows.push(row(vec![it(m.label("Type"), W::Px(L), 26), it(m.edit(E_NAME, name, false), W::Px(160), 26), it(m.hint("for example ppt"), W::Fill, 20)]));
            let mut list = apps.clone();
            list.sort_by_key(|a| a.to_lowercase());
            list.dedup();
            let c = m.combo(E_VALUE, &list, true);
            set_text(c, app);
            rows.push(row(vec![it(m.label("App"), W::Px(L), 26), combo_it(c, W::Fill)]));
            rows.push(row(vec![gap(L), it(m.hint("Choose from the list or type the app name."), W::Fill, 20)]));
        }
        EditKind::Command { cmd, .. } => {
            rows.push(row(vec![it(m.label("Name"), W::Px(L), 26), it(m.edit(E_NAME, &cmd.name, false), W::Px(160), 26), it(m.hint("what you type, for example jira"), W::Fill, 20)]));
            rows.push(row(vec![it(m.label("Shown as"), W::Px(L), 26), it(m.edit(E_TITLE, cmd.title.as_deref().unwrap_or(""), false), W::Fill, 26)]));
            rows.push(row(vec![gap(L), it(m.hint("optional, for example Open Jira issue"), W::Fill, 20)]));
            let kinds = vec!["Open a web page".to_string(), "Run a command".to_string()];
            let k = m.combo(E_KIND, &kinds, false);
            let is_run = cmd.run.is_some();
            unsafe { SendMessageW(k, CB_SETCURSEL, Some(WPARAM(is_run as usize)), None) };
            rows.push(row(vec![it(m.label("Does"), W::Px(L), 26), combo_it(k, W::Px(200))]));
            let vl = m.label(if is_run { "Command" } else { "Web address" });
            unsafe { SetWindowLongPtrW(vl, GWLP_ID, E_VALUE_LABEL as isize) };
            let value = cmd.url.clone().or(cmd.run.clone()).unwrap_or_default();
            rows.push(row(vec![it(vl, W::Px(L), 26), it(m.edit(E_VALUE, &value, false), W::Fill, 26)]));
            rows.push(row(vec![gap(L), it(m.hint("{q} is replaced with what you type after the name."), W::Fill, 20)]));
        }
        EditKind::OpenWith { ow, .. } => {
            rows.push(row(vec![it(m.label("Name"), W::Px(L), 26), it(m.edit(E_NAME, &ow.name, false), W::Fill, 26)]));
            rows.push(row(vec![gap(L), it(m.hint("shown as \"Open in <name>\""), W::Fill, 20)]));
            rows.push(row(vec![it(m.label("App (.exe)"), W::Px(L), 26), it(m.edit(E_VALUE, &ow.exe, false), W::Fill, 26)]));
            rows.push(row(vec![gap(L), it(m.hint("Empty: find VS Code, WezTerm or Windows Terminal by the name."), W::Fill, 20)]));
            rows.push(row(vec![it(m.label("Passes"), W::Px(L), 26), it(m.edit(E_TITLE, &ow.args, false), W::Fill, 26)]));
            rows.push(row(vec![gap(L), it(m.hint("{path} file or folder, {dir} folder. Empty: the usual way."), W::Fill, 20)]));
        }
    }
    let err = m.raw(w!("STATIC"), "", (SS_LEFT.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), E_ERROR, F_BODY);
    tint(err, |p| p.danger);
    rows.push(row(vec![gap(L), it(err, W::Fill, 20)]));
    let cancel = m.button(ID_CANCEL, "Cancel");
    let ok = m.button(ID_SAVE, "OK");
    rows.push(row_gap(4, vec![gap(0), It { hwnd: None, w: W::Fill, h: 0, drop: 0 }, it(cancel, W::Px(96), 32), it(ok, W::Px(96), 32)]));
    let fonts = fonts_for(dpi);
    apply_fonts(&m.made, &fonts);
    dark::apply(hwnd);
    place(&rows, PAD - 8, 16, ED_W - 2 * PAD + 8, dpi);
    for (h, _) in &m.made {
        unsafe {
            let _ = ShowWindow(*h, SW_SHOW);
        }
    }
    EDITOR.with(|e| *e.borrow_mut() = Some(Editor { hwnd, owner, kind, fonts }));
    unsafe {
        let _ = EnableWindow(owner, false);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(item(hwnd, E_NAME)));
    }
}

fn close_editor() {
    let e = EDITOR.with(|e| e.borrow_mut().take());
    if let Some(e) = e {
        unsafe {
            // 先に持ち主を戻してから閉じる(ほかのアプリが前に出ないように)
            let _ = EnableWindow(e.owner, true);
            let _ = SetForegroundWindow(e.owner);
            let _ = DestroyWindow(e.hwnd);
        }
        delete_fonts(&e.fonts);
    }
}

extern "system" fn editor_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| editor_inner(hwnd, msg, wparam, lparam))) {
        Ok(r) => r,
        Err(_) => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// OKを押したときに, 窓の値で項目を作る
fn editor_values(hwnd: HWND, kind: &EditKind) -> EditKind {
    let t = |id: i32| text(item(hwnd, id)).trim().to_string();
    match kind {
        EditKind::Alias { orig, apps, .. } => EditKind::Alias { orig: orig.clone(), name: t(E_NAME), app: t(E_VALUE), apps: apps.clone() },
        EditKind::Command { index, .. } => {
            let run = unsafe { SendMessageW(item(hwnd, E_KIND), CB_GETCURSEL, None, None).0 } == 1;
            let v = t(E_VALUE);
            let title = t(E_TITLE);
            let val = (!v.is_empty()).then_some(v);
            EditKind::Command {
                index: *index,
                cmd: Command {
                    name: t(E_NAME),
                    title: (!title.is_empty()).then_some(title),
                    url: if run { None } else { val.clone() },
                    run: if run { val } else { None },
                },
            }
        }
        EditKind::OpenWith { index, .. } => EditKind::OpenWith { index: *index, ow: OpenWith { name: t(E_NAME), exe: t(E_VALUE), args: t(E_TITLE) } },
    }
}

fn editor_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(r) = common_proc(msg, wparam, lparam) {
        return r;
    }
    match msg {
        WM_COMMAND => {
            let id = loword(wparam.0) as i32;
            let code = hiword(wparam.0) as u32;
            match id {
                ID_SAVE => {
                    let kind = EDITOR.with(|e| e.borrow().as_ref().map(|e| e.kind.clone()));
                    if let Some(kind) = kind {
                        let v = editor_values(hwnd, &kind);
                        match with_settings(|s| s.apply_edit(&v)) {
                            Some(Err(e)) => set_text(item(hwnd, E_ERROR), &e),
                            _ => close_editor(),
                        }
                    }
                }
                ID_CANCEL => close_editor(),
                E_KIND if code == CBN_SELCHANGE => {
                    let run = unsafe { SendMessageW(item(hwnd, E_KIND), CB_GETCURSEL, None, None).0 } == 1;
                    set_text(item(hwnd, E_VALUE_LABEL), if run { "Command" } else { "Web address" });
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            close_editor();
            LRESULT(0)
        }
        WM_ERASEBKGND => unsafe {
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            FillRect(HDC(wparam.0 as *mut _), &rc, brush(pal().bg));
            LRESULT(1)
        },
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
