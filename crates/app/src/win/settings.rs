//! 設定画面(K16, K44). トレイの`Settings`で開く. 左に区分, 右に中身(カードの並び).
//! 部品の見た目は`parts.rs`でそろえる. 位置はどれも96dpiの値で, 区分を作るときに決める.
//! 変えるとすぐ`config.toml`を書き直す(K45)(Q7. 手で書いた`#`の行は残らない). 書いたファイルは監視が読み直す.

use super::i18n::tr;
use super::storage;
use super::system;
use super::theme::*;
use super::util::{from_wide, hiword, loword, rgb, WStr};
use super::{actions, dark, paint, parts};
use parts::{brush, drop_sel, s, set_drop, surface, tint, Btn, Deco, Kind};
use hikidashi_core::builtin;
use hikidashi_core::config::{self, parse_enter_key, Command, CommandKind, Config, Files, OpenWith, ENTER_KEY_CHOICES};
use std::cell::RefCell;
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::{SS_CENTER, SS_CENTERIMAGE, SS_LEFT, SS_NOPREFIX};
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
/// 中身の幅. 右はスクロールの帯の分を空ける
const BODY_W: i32 = 552;
const FONT_PX: i32 = 14;
const HINT_PX: i32 = 13;
const HEAD_PX: i32 = 20;
/// 入力欄, プルダウン, ボタンの高さ. 選ぶものの幅
const CTL_H: i32 = 32;
const CTL_W: i32 = 220;
/// 一覧の1行の高さ, 下の帯の高さ, 見出しの高さ
const ITEM_H: i32 = 30;
const BAR_H: i32 = 44;
const LIST_HEAD_H: i32 = 28;

/// 設定ファイルが変わったので, 開いている設定画面と比べる
const WM_SYNC: u32 = WM_APP + 1;
/// 下の帯の知らせを出す, 消す
const TIMER_SHOW: usize = 1;
const TIMER_HIDE: usize = 2;

// 部品の番号. 1と2はEnterとEscで押される(IsDialogMessage)
const ID_SAVE: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_OPENFILE: i32 = 10;
const ID_PROMPT: i32 = 11;
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
const ID_PFX: i32 = 700; // 700〜719(K62で12個にしたので, 110〜119から移した)
const ID_PFX_EX: i32 = 720; // 720〜739
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
const ID_PROFILE: i32 = 502;
const ID_BROWSER: i32 = 503;
const ID_OPEN_IN: i32 = 504;
const ID_EDGE_PROFILE: i32 = 505;
const ID_GPT_MODE: i32 = 510;
const ID_GPT_SEND: i32 = 513;
const ID_GPT_DELAY: i32 = 514;
const ID_GPT_HIDE: i32 = 515;
const ID_GPT_IMAGE_WAIT: i32 = 516;
const ID_ALIASES: i32 = 600;
const ID_ALIAS_ADD: i32 = 601;
const ID_ALIAS_EDIT: i32 = 602;
const ID_ALIAS_DEL: i32 = 603;
// 項目を直す小さな窓
const E_NAME: i32 = 10;
const E_TITLE: i32 = 11;
const E_KIND: i32 = 12;
const E_VALUE: i32 = 13;
const E_ERROR: i32 = 14;
const E_VALUE_LABEL: i32 = 15;
const E_APP: i32 = 16;
const E_APPS: i32 = 17;
const E_BROWSE: i32 = 18;

const SECTIONS: [&str; 8] = ["General", "Hotkeys", "Keywords", "Search", "Apps", "Actions", "Web", "Aliases"];
const PREFIX_LABELS: [&str; 12] = ["Apps", "Files", "Bookmarks", "End a process", "WSL", "Box", "Google", "ChatGPT", "Clipboard history", "Calculator", "Saved results", "Calendar"];
/// キーワードで探すものの説明(K52)
const PREFIX_NOTES: [&str; 12] = [
    "Apps in the Start menu",
    "File and folder names",
    "Chrome and Edge bookmarks",
    "Running apps",
    "Files in your WSL home",
    "Files in Box Drive",
    "Search on Google",
    "A long question to ChatGPT",
    "Text and files you copied",
    "Numbers, formulas, RF, MIPI",
    "Results saved with Ctrl+S",
    "Outlook appointments",
];
const LANGS: [(&str, &str); 3] = [("auto", "Same as Windows"), ("ja", "日本語"), ("en", "English")];
/// ランチャーの窓, 書く窓, 設定画面, トレイのメニューの色(K39, K43)
const THEMES: [(&str, &str); 3] = [("auto", "Same as Windows"), ("light", "Light"), ("dark", "Dark")];
/// Tabのメニューの項目(`config::MENU_ITEMS`の順)と種類
const MENU_LABELS: [&str; 10] = ["Copy file", "Open new window", "Open in another app", "Open the folder it is in", "Copy path", "Copy WSL path (only with WSL)", "Run as administrator", "Open in Explorer", "Open with... (list of apps)", "Attach to new mail (Outlook)"];
const MENU_KINDS: [&str; 3] = ["app", "file", "folder"];
/// Enterに修飾キーを足したときの操作(2026-11-24. 設定の`[keys]`の順)
const KEY_LABELS: [&str; 5] = ["Ask ChatGPT", "Search Google", "Long question", "Run as admin", "Open in terminal"];
const BROWSERS: [(&str, &str); 3] = [("default", "Windows default"), ("chrome", "Chrome"), ("edge", "Edge")];
const OPEN_IN: [(&str, &str); 2] = [("window", "New window"), ("tab", "New tab")];
const GPT_MODES: [(&str, &str); 2] = [("url", "Open with the question"), ("clipboard", "Copy, then open")];
/// 短い語で開くもの(エイリアス. K44)と, 「別のアプリで開く」の渡し方
const ALIAS_KINDS: [&str; 5] = ["App", "Web page", "Command", "File", "Folder"];
const PASS_KINDS: [&str; 3] = ["The selected item", "Its folder", "Custom"];
const PASS_FOLDER: &str = "{dir}";

/// 開くときに渡す値
pub struct OpenArgs {
    pub config: Config,
    pub error: Option<String>,
    pub hotkey: String,
    pub exe: String,
    pub apps: Vec<String>,
}

// ---------------------------------------------------------------- 並べ方

/// 部品の位置(96dpi)
#[derive(Clone, Copy)]
struct Pos {
    hwnd: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Default)]
struct Section {
    pos: Vec<Pos>,
    deco: Vec<Deco>,
    /// 中身の高さ
    h: i32,
}

fn place(pos: &[Pos], dx: i32, dy: i32, dpi: u32) {
    for p in pos {
        unsafe {
            let _ = SetWindowPos(p.hwnd, None, s(p.x + dx, dpi), s(p.y + dy, dpi), s(p.w, dpi), s(p.h, dpi), SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
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
const F_HINT: u8 = 3;

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
        parts::forget(h);
        self.made.push((h, font));
        h
    }
    fn label(&mut self, text: &str) -> HWND {
        let text = &tr(text);
        self.raw(w!("STATIC"), text, (SS_LEFT.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_BODY)
    }
    fn hint(&mut self, text: &str) -> HWND {
        let text = &tr(text);
        let h = self.raw(w!("STATIC"), text, (SS_LEFT.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_HINT);
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
    /// 枠は`parts.rs`の飾りで描く
    fn edit(&mut self, id: i32, text: &str, style: i32) -> HWND {
        self.raw(w!("EDIT"), text, WS_TABSTOP.0 | (ES_AUTOHSCROLL | style) as u32, WINDOW_EX_STYLE(0), id, F_BODY)
    }
    fn button(&mut self, id: i32, text: &str) -> HWND {
        let text = &tr(text);
        self.raw(w!("BUTTON"), text, WS_TABSTOP.0 | BS_OWNERDRAW as u32, WINDOW_EX_STYLE(0), id, F_BODY)
    }
    fn check(&mut self, id: i32, text: &str, hint: &str, cell: bool, on: bool) -> HWND {
        let h = self.raw(w!("BUTTON"), text, WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32, WINDOW_EX_STYLE(0), id, F_BODY);
        parts::make_check(h, &tr(hint), cell);
        surface(h, |p| p.card);
        set_check(h, on);
        h
    }
    fn list(&mut self, id: i32, items: &[String], cols: &[i32]) -> HWND {
        let st = WS_TABSTOP.0 | WS_VSCROLL.0 | (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS) as u32;
        let h = self.raw(w!("LISTBOX"), "", st, WINDOW_EX_STYLE(0), id, F_BODY);
        parts::make_list(h, cols);
        fill_list(h, items);
        h
    }
    /// プルダウン. `items`は出す文字のまま
    fn drop(&mut self, id: i32, items: Vec<String>, sel: usize) -> HWND {
        let h = self.raw(parts::DROP_CLASS, "", WS_TABSTOP.0, WINDOW_EX_STYLE(0), id, F_BODY);
        set_drop(h, items, sel);
        h
    }
}

fn tr_all(v: &[&str]) -> Vec<String> {
    v.iter().map(|t| tr(t)).collect()
}

/// 決まった言葉から選ぶもの
fn key_items(keys: &[(&str, &str)], value: &str) -> (Vec<String>, usize) {
    (keys.iter().map(|(_, l)| tr(l)).collect(), keys.iter().position(|(k, _)| *k == value).unwrap_or(0))
}

/// 一覧の下の帯に置くもの
enum Bar {
    Button(i32, &'static str),
    Edit(i32),
}

/// ボタンの幅. 文字の長さに合わせる(日本語は2文字分で数える)
fn button_w(text: &str, min: i32) -> i32 {
    let n: i32 = tr(text).chars().map(|c| if c.is_ascii() { 8 } else { 14 }).sum();
    (n + 30).max(min)
}

/// 区分の中身を上から並べる
struct Page<'a> {
    m: &'a mut Maker,
    x: i32,
    w: i32,
    y: i32,
    out: Section,
    /// 今のカードの始まり(飾りの番号, 上の端)
    card: Option<(usize, i32)>,
}

impl<'a> Page<'a> {
    fn new(m: &'a mut Maker, x: i32, y: i32, w: i32) -> Page<'a> {
        Page { m, x, w, y, out: Section::default(), card: None }
    }
    fn at(&mut self, hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
        self.out.pos.push(Pos { hwnd, x, y, w, h });
    }
    fn deco(&mut self, kind: Kind, x: i32, y: i32, w: i32, h: i32) {
        self.out.deco.push(Deco { kind, x, y, w, h });
    }
    fn title(&mut self, text: &str) {
        let h = self.m.head(text);
        self.at(h, self.x, self.y, self.w, 30);
        self.y += 40;
    }
    /// まとまりの名前と説明
    fn group(&mut self, title: &str, hint: &str) {
        if !title.is_empty() {
            let h = self.m.sub(title);
            self.at(h, self.x, self.y, self.w, 22);
            self.y += 24;
        }
        if !hint.is_empty() {
            let h = self.m.hint(hint);
            self.at(h, self.x, self.y, self.w, 20);
            self.y += 22;
        }
        self.y += 4;
    }
    fn card(&mut self) {
        self.card = Some((self.out.deco.len(), self.y));
    }
    fn end_card(&mut self) {
        if let Some((i, top)) = self.card.take() {
            self.out.deco.insert(i, Deco { kind: Kind::Card, x: self.x, y: top, w: self.w, h: self.y - top });
        }
        self.y += 20;
    }
    /// カードの行を始める. 返り値は行の上の端
    fn row(&mut self, h: i32) -> i32 {
        let y = self.y;
        if self.card.is_some_and(|(_, top)| y > top) {
            self.deco(Kind::Line, self.x + 1, y, self.w - 2, 1);
        }
        self.y += h;
        y
    }
    /// 名前と説明. 返り値は行の真ん中
    fn named(&mut self, label: &str, hint: &str, ctl_w: i32) -> i32 {
        let h = if hint.is_empty() { 50 } else { 58 };
        let y = self.row(h);
        let w = self.w - 32 - 16 - ctl_w;
        let l = self.m.label(label);
        surface(l, |p| p.card);
        self.at(l, self.x + 16, if hint.is_empty() { y + 14 } else { y + 8 }, w, 22);
        if !hint.is_empty() {
            let t = self.m.hint(hint);
            surface(t, |p| p.card);
            self.at(t, self.x + 16, y + 30, w, 20);
        }
        y + h / 2
    }
    fn right(&self) -> i32 {
        self.x + self.w - 16
    }
    fn check(&mut self, id: i32, label: &str, hint: &str, on: bool) {
        let h = if hint.is_empty() { 50 } else { 58 };
        let y = self.row(h);
        let c = self.m.check(id, &tr(label), hint, false, on);
        // カードの角にかからないように, 内側に置く
        self.at(c, self.x + 4, y + 3, self.w - 8, h - 6);
    }
    fn select(&mut self, id: i32, label: &str, hint: &str, items: (Vec<String>, usize)) {
        let cy = self.named(label, hint, CTL_W);
        let d = self.m.drop(id, items.0, items.1);
        surface(d, |p| p.card);
        self.at(d, self.right() - CTL_W, cy - CTL_H / 2, CTL_W, CTL_H);
    }
    /// 枠付きの入力欄
    fn field(&mut self, id: i32, text: &str, style: i32, x: i32, y: i32, w: i32, h: i32) -> HWND {
        let e = self.m.edit(id, text, style);
        self.deco(Kind::Field(e), x, y, w, h);
        let pad = if w < 100 { 6 } else { 10 };
        self.at(e, x + pad, y + (h - 22) / 2, w - 2 * pad, 22);
        e
    }
    fn number(&mut self, id: i32, label: &str, hint: &str, text: &str) {
        let cy = self.named(label, hint, 80);
        self.field(id, text, ES_NUMBER | ES_CENTER, self.right() - 80, cy - CTL_H / 2, 80, CTL_H);
    }
    /// ホットキー. 「なし」のボタンがないものも, 位置はそろえる
    fn hotkey(&mut self, id: i32, label: &str, hint: &str, value: &str, change: i32, clear: Option<i32>) {
        let (cap, b1, b2) = (150, button_w("Change", 72), button_w("Clear", 64));
        let cy = self.named(label, hint, cap + b1 + b2 + 16);
        let y = cy - CTL_H / 2;
        let x0 = self.right() - b2 - 8 - b1 - 8 - cap;
        self.deco(Kind::Cap, x0, y, cap, CTL_H);
        let v = self.m.raw(w!("STATIC"), &tr(value), (SS_CENTER.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), id, F_BODY);
        surface(v, |p| p.bg_subtle);
        self.at(v, x0 + 6, y + 4, cap - 12, CTL_H - 8);
        let c = self.m.button(change, "Change");
        surface(c, |p| p.card);
        self.at(c, x0 + cap + 8, y, b1, CTL_H);
        if let Some(id) = clear {
            let c = self.m.button(id, "Clear");
            surface(c, |p| p.card);
            self.at(c, self.right() - b2, y, b2, CTL_H);
        }
    }
    /// 枠の中に, 見出し, 一覧, 下の帯(足す, 消すなど)をまとめる
    #[allow(clippy::too_many_arguments)]
    fn list(&mut self, id: i32, items: &[String], cols: &[i32], heads: &[&str], rows: i32, bar: Vec<Bar>) {
        let h = self.m.list(id, items, cols);
        let (x, y, w) = (self.x, self.y, self.w);
        let head = if heads.is_empty() { 0 } else { LIST_HEAD_H };
        let body = rows * ITEM_H + 8;
        let bar_h = if bar.is_empty() { 0 } else { BAR_H };
        self.deco(Kind::List { owner: h, head, bar: bar_h }, x, y, w, head + body + bar_h);
        let mut cx = x + 14;
        for (i, t) in heads.iter().enumerate() {
            let cw = cols.get(i).copied().unwrap_or(x + w - 14 - cx);
            let l = self.m.raw(w!("STATIC"), &tr(t), (SS_LEFT.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_HINT);
            tint(l, |p| p.text_tertiary);
            surface(l, |p| p.bg_subtle);
            self.at(l, cx, y + 4, cw - 8, head - 6);
            cx += cw;
        }
        self.at(h, x + 2, y + head + 4, w - 4, body - 8);
        let by = y + head + body + 8;
        // 入力欄は残りの幅
        let fixed: i32 = bar.iter().map(|b| if let Bar::Button(_, t) = b { button_w(t, 64) + 8 } else { 0 }).sum();
        let mut bx = x + 8;
        for b in &bar {
            match b {
                Bar::Button(id, t) => {
                    let bw = button_w(t, 64);
                    let c = self.m.button(*id, t);
                    surface(c, |p| p.bg_subtle);
                    self.at(c, bx, by, bw, 28);
                    bx += bw + 8;
                }
                Bar::Edit(id) => {
                    let ew = (w - 16 - fixed).min(200);
                    self.field(*id, "", 0, bx, by, ew, 28);
                    bx += ew + 8;
                }
            }
        }
        self.y = y + head + body + bar_h + 20;
    }
    /// 小さな窓の1行(左に名前). 返り値は部品を置く左の端と上の端
    fn form(&mut self, label: &str) -> (i32, i32) {
        let y = self.y;
        let l = self.m.label(label);
        self.at(l, self.x, y + 5, FORM_L - 8, 22);
        self.y += CTL_H + 8;
        (self.x + FORM_L, y)
    }
    fn form_edit(&mut self, label: &str, id: i32, text: &str, w: i32) {
        let (x, y) = self.form(label);
        self.field(id, text, 0, x, y, w, CTL_H);
    }
    fn form_drop(&mut self, label: &str, id: i32, items: Vec<String>, sel: usize) {
        let (x, y) = self.form(label);
        let d = self.m.drop(id, items, sel);
        self.at(d, x, y, CTL_W, CTL_H);
    }
    fn form_hint(&mut self, text: &str) {
        let h = self.m.hint(text);
        self.at(h, self.x + FORM_L, self.y - 4, self.w - FORM_L, 20);
        self.y += 22;
    }
    fn finish(mut self) -> Section {
        self.out.h = self.y;
        self.out
    }
}

/// 小さな窓の名前の幅
const FORM_L: i32 = 110;

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

/// 日本語にせず, そのまま出す
fn set_raw_text(h: HWND, t: &str) {
    let w = WStr::new(t);
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

fn list_text(h: HWND, i: usize) -> String {
    unsafe {
        let n = SendMessageW(h, LB_GETTEXTLEN, Some(WPARAM(i)), None).0.max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        SendMessageW(h, LB_GETTEXT, Some(WPARAM(i)), Some(LPARAM(buf.as_mut_ptr() as isize)));
        from_wide(&buf)
    }
}

fn list_sel(h: HWND) -> Option<usize> {
    let r = unsafe { SendMessageW(h, LB_GETCURSEL, None, None).0 };
    (r >= 0).then_some(r as usize)
}

/// 設定画面の中の枠. なければ(項目を直す小さな窓など)空
fn panel_of(hwnd: HWND) -> HWND {
    unsafe { FindWindowExW(Some(hwnd), None, PANEL_CLASS, PCWSTR::null()).unwrap_or_default() }
}

fn item(parent: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(Some(parent), id).or_else(|_| GetDlgItem(Some(panel_of(parent)), id)).unwrap_or_default() }
}

/// `pick`で選ぶもの
#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Folder,
    Exe,
    /// どのファイルでも(エイリアスのファイル. K71)
    File,
}

/// フォルダー, exe, ファイルを選ぶ(Windowsの「フォルダーの選択」と「開く」). 選ばなければNone
fn pick(owner: HWND, what: Pick, title: &str) -> Option<String> {
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let mut opts = dlg.GetOptions().ok()? | FOS_FORCEFILESYSTEM;
        if what == Pick::Folder {
            opts |= FOS_PICKFOLDERS;
        } else if what == Pick::Exe {
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

fn fonts_for(dpi: u32) -> [HFONT; 4] {
    let face = paint::body_face();
    [paint::font(face, s(FONT_PX, dpi), 400), paint::font(face, s(HEAD_PX, dpi), 600), paint::font(face, s(FONT_PX, dpi), 600), paint::font(face, s(HINT_PX, dpi), 400)]
}

fn apply_fonts(made: &[(HWND, u8)], fonts: &[HFONT; 4]) {
    for (h, f) in made {
        unsafe { SendMessageW(*h, WM_SETFONT, Some(WPARAM(fonts[*f as usize].0 as usize)), Some(LPARAM(1))) };
    }
}

fn delete_fonts(fonts: &[HFONT; 4]) {
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
        parts::register_drop(hinst.into());
    });
}

// ---------------------------------------------------------------- 色と自分で描く部品(両方の窓で使う)

/// 部品の色と, 自分で描くボタン・一覧. 扱ったらSome
fn common_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    unsafe {
        match msg {
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let hdc = HDC(wparam.0 as *mut _);
                let (fg, bg) = parts::colors(HWND(lparam.0 as *mut _));
                SetTextColor(hdc, rgb(fg));
                SetBkColor(hdc, rgb(bg));
                Some(LRESULT(brush(bg).0 as isize))
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let hdc = HDC(wparam.0 as *mut _);
                let h = HWND(lparam.0 as *mut _);
                let bg = if GetDlgCtrlID(h) == ID_SIDE { pal().bg_subtle } else { pal().field };
                SetTextColor(hdc, rgb(pal().text));
                SetBkColor(hdc, rgb(bg));
                Some(LRESULT(brush(bg).0 as isize))
            }
            WM_DRAWITEM => {
                let d = &*(lparam.0 as *const DRAWITEMSTRUCT);
                match (d.CtlType, d.CtlID as i32) {
                    (ODT_BUTTON, ID_SAVE) => parts::draw_button(d, Btn::Primary),
                    (ODT_BUTTON, ID_OPENFILE | ID_PROMPT) => parts::draw_button(d, Btn::Link),
                    (ODT_BUTTON, _) => parts::draw_button(d, Btn::Normal),
                    (ODT_LISTBOX, ID_SIDE) => parts::draw_side_item(d, &tr(SECTIONS.get(d.itemID as usize).copied().unwrap_or(""))),
                    (ODT_LISTBOX, _) => parts::draw_list_item(d),
                    _ => return None,
                }
                Some(LRESULT(1))
            }
            WM_LBUTTONDOWN => {
                parts::click_field(hwnd, loword(lparam.0 as usize) as i16 as i32, hiword(lparam.0 as usize) as i16 as i32, scroll_info(hwnd).nPos);
                None
            }
            WM_COMMAND => {
                let code = hiword(wparam.0) as u32;
                if lparam.0 != 0 && [EN_SETFOCUS, EN_KILLFOCUS, LBN_SETFOCUS, LBN_KILLFOCUS].contains(&code) {
                    parts::focus_changed(hwnd, HWND(lparam.0 as *mut _), scroll_info(hwnd).nPos);
                }
                None
            }
            _ => None,
        }
    }
}

/// 地の色と飾りを描く
fn paint_surface(hwnd: HWND, dy: i32) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        parts::buffered(hdc, &ps.rcPaint, |hdc| {
            parts::fill(hdc, &ps.rcPaint, pal().bg);
            parts::paint_decos(hwnd, hdc, dy);
        });
        let _ = EndPaint(hwnd, &ps);
    }
}

// ---------------------------------------------------------------- 設定画面

struct Settings {
    hwnd: HWND,
    dpi: u32,
    fonts: [HFONT; 4],
    made: Vec<(HWND, u8)>,
    sections: Vec<Section>,
    footer: Vec<Pos>,
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
    status: Status,
    /// 知らせを今出しているか(続けて変えたときは, 一度消して出し直す)
    status_on: bool,
    /// 設定ファイルに書き間違いがあり, 初期値を出している. 上書きする前に聞く
    broken: bool,
    asking: bool,
    /// 自分で書いた設定(読み直しの知らせが来ても作り直さない)
    written: Vec<Config>,
    ja: bool,
}

/// 下の帯の知らせ
#[derive(Clone, PartialEq)]
enum Status {
    None,
    Ok(String),
    Error(String),
}

/// 借りている間にはできないこと(ほかの窓を出すなど). 借りるのをやめてから行う
enum Task {
    AskClose,
    /// 書き間違いのある設定ファイルを置き換えてよいか
    AskReplace,
    CopyPrompt,
    Pick { id: i32, title: &'static str },
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
    open_at(a, 0, None, true);
}

/// 区分と窓の位置を決めて開く
fn open_at(a: OpenArgs, section: usize, at: Option<POINT>, activate: bool) {
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
    let mut r = centered(dpi, CLIENT_W, CLIENT_H, style);
    if let Some(p) = at {
        r = RECT { left: p.x, top: p.y, right: p.x + r.right - r.left, bottom: p.y + r.bottom - r.top };
    }
    let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let title = WStr::new(&tr("Hikidashi Settings"));
    let Ok(hwnd) = (unsafe {
        CreateWindowExW(WS_EX_CONTROLPARENT, CLASS, title.pcwstr(), style, r.left, r.top, r.right - r.left, r.bottom - r.top, None, None, Some(hinst.into()), None)
    }) else {
        return;
    };
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let mut st = build(hwnd, hinst.into(), a, dpi);
    st.show_section(section);
    dark::apply(hwnd);
    parts::hide_focus(hwnd);
    let side = st.side;
    SETTINGS.with(|s| *s.borrow_mut() = Some(st));
    unsafe {
        if activate {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(side));
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
}

/// 設定ファイルを読み直したら呼ぶ. ほかで変わっていれば, 開いている設定画面を作り直す
pub fn config_changed() {
    if let Some(h) = settings_hwnd() {
        unsafe {
            let _ = PostMessageW(Some(h), WM_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}

fn sync() {
    if editor_hwnd().is_some() {
        return;
    }
    let Some(Some(a)) = super::with_app(|a| Some(a.settings_args())) else { return };
    let Some((hwnd, cur, same, lang)) = with_settings(|s| (s.hwnd, s.current, a.config == s.orig || s.written.contains(&a.config), s.ja != super::i18n::is_ja())) else { return };
    if same && !lang {
        return;
    }
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut r);
    }
    // メモ帳などで直しているときは, 前に出さない
    let front = unsafe { GetForegroundWindow() } == hwnd;
    close_settings();
    open_at(a, cur, Some(POINT { x: r.left, y: r.top }), front);
    if same {
        with_settings(|s| s.say(Status::Ok(tr("Applied"))));
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

/// 枠のすぐ下の部品. 枠の中でなければNone
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

/// ホイール. 中身の上なら中身をスクロールする. 扱ったらtrue
fn wheel(hwnd: HWND, msg: &MSG) -> bool {
    let panel = panel_of(hwnd);
    if panel.is_invalid() {
        return false;
    }
    unsafe {
        let under = WindowFromPoint(msg.pt);
        if under != panel {
            let Some(c) = panel_child(panel, under) else { return false };
            // 中身が多い一覧は, それ自身をスクロールする
            let own = GetWindowLongPtrW(c, GWL_STYLE) as u32 & WS_VSCROLL.0 != 0;
            if own && dark::class_name(c) == "ListBox" {
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
    if let Some(r) = common_proc(hwnd, msg, wparam, lparam) {
        return r;
    }
    unsafe {
        match msg {
            // 部品からの知らせは設定画面で受ける
            WM_COMMAND => SendMessageW(GetAncestor(hwnd, GA_PARENT), msg, Some(wparam), Some(lparam)),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                paint_surface(hwnd, scroll_info(hwnd).nPos);
                LRESULT(0)
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

fn selected_key(h: HWND, keys: &[(&str, &str)]) -> String {
    keys.get(drop_sel(h)).unwrap_or(&keys[0]).0.to_string()
}

fn profile_label(dir: &str, name: &str) -> String {
    if dir == name {
        dir.to_string()
    } else {
        format!("{name} ({dir})")
    }
}

/// エイリアスの一覧: アプリの呼び名(`[alias]`)のあとに, Webページとコマンド(`[[command]]`)
fn alias_rows(c: &Config) -> Vec<String> {
    let apps = c.alias.iter().map(|(k, v)| format!("{k}\t{}\t{v}", tr(ALIAS_KINDS[0])));
    let cmds = c.command.iter().map(|m| {
        let (kind, target) = m.target().map_or((1, ""), |(k, v)| (k as usize, v));
        let shown = m.title.as_deref().filter(|t| !t.is_empty()).map_or_else(|| target.to_string(), |t| format!("{t} · {target}"));
        format!("{}\t{}\t{shown}", m.name, tr(ALIAS_KINDS[kind]))
    });
    apps.chain(cmds).collect()
}

fn open_with_rows(c: &Config) -> Vec<String> {
    c.open_with
        .iter()
        .map(|o| {
            let exe = if o.exe.is_empty() { tr("(find automatically)") } else { o.exe.clone() };
            let found = if actions::find_exe(o).is_some() { String::new() } else { tr("  · not installed") };
            format!("{}\t{exe}{found}", o.name)
        })
        .collect()
}

const TERMINAL_AUTO: &str = "Automatic";

/// `Alt+Enter`のターミナルの選び(初めは自動, あとは「別のアプリで開く」の名前)
fn terminals(c: &Config) -> (Vec<String>, usize) {
    let mut names = vec![tr(TERMINAL_AUTO)];
    names.extend(c.open_with.iter().map(|o| o.name.clone()));
    (names, c.open_with.iter().position(|o| o.name.eq_ignore_ascii_case(c.tools.terminal.trim())).map_or(0, |i| i + 1))
}

/// キーワードの説明. 空なら使わないこと
fn prefix_note(i: usize, v: &str) -> String {
    if v.trim().is_empty() {
        tr("Not used")
    } else {
        tr(PREFIX_NOTES[i])
    }
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
    let mut sections: Vec<Section> = vec![];
    const TOP: i32 = 16;

    // General
    {
        let mut p = Page::new(&mut m, PAD, TOP, BODY_W);
        p.title("General");
        p.group("Startup", "");
        p.card();
        p.check(ID_AUTOSTART, "Start when Windows starts", "", system::autostart_enabled());
        p.end_card();
        p.group("Display", "");
        p.card();
        p.select(ID_LANG, "Language", "", key_items(&LANGS, &c.ui.language));
        p.select(ID_THEME, "Theme", "", key_items(&THEMES, &c.ui.theme));
        p.end_card();
        sections.push(p.finish());
    }
    // Keys
    {
        let mut p = Page::new(&mut m, PAD, TOP, BODY_W);
        p.title("Hotkeys");
        p.group("Show the window", "");
        p.card();
        let hk = if a.hotkey.is_empty() { "Not set" } else { a.hotkey.as_str() };
        p.hotkey(ID_HOTKEY, "Open Hikidashi", "Works in any app", hk, ID_HOTKEY_CHANGE, None);
        let sel = if c.hotkeys.selection.is_empty() { "Not set" } else { c.hotkeys.selection.as_str() };
        p.hotkey(ID_SELHK, "Open with selected text", "Types it in for you", sel, ID_SELHK_CHANGE, Some(ID_SELHK_CLEAR));
        p.end_card();
        // Enterに修飾キーを足したときの操作(2026-11-24)
        p.group("Enter combinations", "");
        p.card();
        let k = &c.keys;
        for (i, (label, v)) in KEY_LABELS.iter().zip([&k.chatgpt, &k.google, &k.write, &k.admin, &k.terminal]).enumerate() {
            let items = ENTER_KEY_CHOICES.iter().map(|c| if c.is_empty() { tr("Not used") } else { c.replace('+', " + ") }).collect();
            let cur = parse_enter_key(v).and_then(|mods| ENTER_KEY_CHOICES.iter().position(|c| parse_enter_key(c) == Some(mods))).unwrap_or(0);
            p.select(ID_KEYS + i as i32, label, "", (items, cur));
        }
        p.end_card();
        sections.push(p.finish());
    }
    // Keywords
    {
        let mut p = Page::new(&mut m, PAD, TOP, BODY_W);
        p.title("Keywords");
        p.group("", "Type a keyword and a space to search only that kind. Leave it empty to turn it off.");
        p.card();
        let f = &c.prefix;
        let vals = [&f.app, &f.file, &f.bookmark, &f.kill, &f.wsl, &f.boxdrive, &f.google, &f.chatgpt, &f.clipboard, &f.calc, &f.snapshot, &f.calendar];
        for (i, (label, v)) in PREFIX_LABELS.iter().zip(vals).enumerate() {
            let y = p.row(44);
            let l = p.m.label(label);
            surface(l, |p| p.card);
            p.at(l, PAD + 16, y + 11, 210, 22);
            let ex = p.m.raw(w!("STATIC"), &prefix_note(i, v), (SS_LEFT.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), ID_PFX_EX + i as i32, F_HINT);
            tint(ex, |p| p.text_tertiary);
            surface(ex, |p| p.card);
            p.at(ex, PAD + 236, y + 11, 200, 22);
            p.field(ID_PFX + i as i32, v, ES_CENTER, p.right() - 80, y + 6, 80, CTL_H);
        }
        p.end_card();
        sections.push(p.finish());
    }
    // Search(BoxとWSLもここ. K44)
    {
        let mut p = Page::new(&mut m, PAD, TOP, BODY_W);
        p.title("Search");
        p.card();
        p.check(ID_RECENT, "Show recent items", "When the box is empty", c.search.recent);
        p.check(ID_CLIPHIST, "Clipboard history", "Keeps copied text and files in memory only", c.search.clipboard_history);
        p.end_card();
        p.group("Folders to search", "Box and your WSL home are found automatically, and searched with their keywords.");
        p.list(ID_ROOTS, &c.files.roots, &[], &[], 3, vec![Bar::Button(ID_ROOTS_ADD, "Add"), Bar::Button(ID_ROOTS_DEL, "Remove")]);
        p.group("Excluded folders", "Everything inside them is excluded too.");
        p.list(ID_SKIP, &c.files.skip_dirs, &[], &[], 3, vec![Bar::Button(ID_SKIP_ADD, "Add"), Bar::Button(ID_SKIP_DEL, "Remove")]);
        p.group("Excluded names", "Names to exclude anywhere, also in WSL. * matches anything.");
        let bar = vec![Bar::Edit(ID_NAME_EDIT), Bar::Button(ID_NAME_ADD, "Add"), Bar::Button(ID_NAME_DEL, "Remove"), Bar::Button(ID_NAME_RESET, "Restore defaults")];
        p.list(ID_NAMES, &c.files.exclude, &[], &[], 6, bar);
        sections.push(p.finish());
    }
    // Apps
    {
        let mut p = Page::new(&mut m, PAD, TOP, BODY_W);
        p.title("Apps");
        p.group("Folders with apps", ".exe files here are listed as apps.");
        p.list(ID_APPDIRS, &c.apps.extra_dirs, &[], &[], 3, vec![Bar::Button(ID_APPDIRS_ADD, "Add"), Bar::Button(ID_APPDIRS_DEL, "Remove")]);
        p.group("Open in another app", "Shown as \"Open in …\" in the actions menu. Apps that are not installed are not shown.");
        let bar = vec![Bar::Button(ID_OW_ADD, "Add"), Bar::Button(ID_OW_EDIT, "Edit"), Bar::Button(ID_OW_DEL, "Remove")];
        p.list(ID_OPENWITH, &open_with_rows(c), &[180], &["Name", "App"], 4, bar);
        p.group("Terminal", "");
        p.card();
        p.select(ID_TERMINAL, "Terminal to use", "For \"Open in terminal\"", terminals(c));
        p.end_card();
        sections.push(p.finish());
    }
    // Tab menu(K24): 項目×種類のチェック. 種類に合わないところは空ける
    {
        let mut p = Page::new(&mut m, PAD, TOP, BODY_W);
        p.title("Actions");
        p.group("", "Choose what appears when you press the right arrow key on a row.");
        p.card();
        const CW: i32 = 88;
        let cx = |k: usize| PAD + BODY_W - 12 - CW * (3 - k as i32);
        let y = p.row(36);
        for (k, t) in ["Apps", "Files", "Folders"].iter().enumerate() {
            let l = p.m.raw(w!("STATIC"), &tr(t), (SS_CENTER.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_HINT);
            tint(l, |p| p.text_tertiary);
            surface(l, |p| p.card);
            p.at(l, cx(k), y + 7, CW, 22);
        }
        for (i, (name, label)) in config::MENU_ITEMS.iter().zip(MENU_LABELS).enumerate() {
            let y = p.row(42);
            let l = p.m.label(label);
            surface(l, |p| p.card);
            p.at(l, PAD + 16, y + 10, BODY_W - 40 - CW * 3, 22);
            for (k, kind) in MENU_KINDS.iter().enumerate() {
                let id = ID_MENU + (i * 3 + k) as i32;
                if config::menu_item_fits(kind, name) {
                    let text = format!("{} ({})", tr(label), tr(["Apps", "Files", "Folders"][k]));
                    let h = p.m.check(id, &text, "", true, c.menu.has(kind, name));
                    p.at(h, cx(k) + (CW - 36) / 2, y + 4, 36, 34);
                } else {
                    let h = p.m.raw(w!("STATIC"), "–", (SS_CENTER.0 | SS_CENTERIMAGE.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), -1, F_HINT);
                    tint(h, |p| p.text_tertiary);
                    surface(h, |p| p.card);
                    p.at(h, cx(k), y + 10, CW, 22);
                }
            }
        }
        p.end_card();
        sections.push(p.finish());
    }
    // Web
    let profiles = local_state_profiles(actions::Browser::Chrome, c);
    let edge_profiles = local_state_profiles(actions::Browser::Edge, c);
    {
        let mut p = Page::new(&mut m, PAD, TOP, BODY_W);
        p.title("Web");
        p.group("Browser", "Bookmarks are read from both browsers, in these profiles.");
        p.card();
        p.select(ID_BROWSER, "Browser", "", key_items(&BROWSERS, &c.browser.browser));
        p.select(ID_OPEN_IN, "Open pages in", "", key_items(&OPEN_IN, &c.browser.open_in));
        for (id, list, cur, label) in [(ID_PROFILE, &profiles, &c.browser.chrome_profile, "Chrome profile"), (ID_EDGE_PROFILE, &edge_profiles, &c.browser.edge_profile, "Edge profile")] {
            let names = list.iter().map(|(d, n)| profile_label(d, n)).collect();
            p.select(id, label, "", (names, list.iter().position(|(d, _)| d == cur).unwrap_or(0)));
        }
        p.end_card();
        p.group("ChatGPT", "");
        p.card();
        p.select(ID_GPT_MODE, "How to send a question", "If it fails, use \"Copy, then open\"", key_items(&GPT_MODES, &c.chatgpt.mode));
        p.check(ID_GPT_SEND, "Press Enter automatically", "Only when ChatGPT is in front", c.chatgpt.send_enter);
        p.number(ID_GPT_DELAY, "Wait before Enter", "Milliseconds", &c.chatgpt.send_delay_ms.to_string());
        p.number(ID_GPT_IMAGE_WAIT, "Wait per image", "Milliseconds, added before Enter for each attached image", &c.chatgpt.image_wait_ms.to_string());
        p.check(ID_GPT_HIDE, "Hide Markdown marks", "In long questions, except on the line you edit", c.chatgpt.hide_marks);
        p.end_card();
        sections.push(p.finish());
    }
    // Aliases(自作コマンドもここ. K44)
    {
        let mut p = Page::new(&mut m, PAD, TOP, BODY_W);
        p.title("Aliases");
        p.group("", "Short words that open an app, a web page or a command.");
        let bar = vec![Bar::Button(ID_ALIAS_ADD, "Add"), Bar::Button(ID_ALIAS_EDIT, "Edit"), Bar::Button(ID_ALIAS_DEL, "Remove")];
        p.list(ID_ALIASES, &alias_rows(c), &[110, 110], &["Word", "Kind", "Opens"], 12, bar);
        sections.push(p.finish());
    }
    // 下の帯
    m.parent = hwnd;
    let open_file = m.button(ID_OPENFILE, "Open config file");
    let prompt = m.button(ID_PROMPT, "Copy setup prompt");
    let close = m.button(ID_CANCEL, "Close");
    // 下の帯. 縦は`layout`で決める. 知らせは`FOOT_STATUS`に描く
    let footer = [(open_file, 20, 2, 138, 28), (prompt, 172, 2, 168, 28), (close, SIDE_W + PAD + BODY_W - 100, 0, 100, 32)].map(|(hwnd, x, y, w, h)| Pos { hwnd, x, y, w, h }).to_vec();
    for h in [open_file, prompt, close] {
        unsafe {
            let _ = ShowWindow(h, SW_SHOW);
        }
    }
    // ホットキーが使えないなどの知らせもあるので, ファイルの書き間違いかを確かめる
    let broken = a.error.is_some() && std::fs::read_to_string(storage::config_path()).is_ok_and(|t| Config::parse(&t).is_err());
    let fonts = fonts_for(dpi);
    apply_fonts(&m.made, &fonts);
    parts::set_fonts(fonts[F_HINT as usize], fonts[F_BOLD as usize]);
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
        status: match &a.error {
            Some(e) if broken => Status::Error(tr(&format!("{e}. Defaults are shown."))),
            Some(e) => Status::Error(tr(e)),
            None => Status::None,
        },
        status_on: true,
        broken,
        asking: false,
        written: vec![],
        ja: super::i18n::is_ja(),
    }
}

/// 下の帯の知らせの左と右(96dpi)
const FOOT_STATUS: (i32, i32) = (352, SIDE_W + PAD + BODY_W - 112);

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
        let mut bottom = 0;
        for (i, sec) in self.sections.iter().enumerate() {
            place(&sec.pos, 0, 0, d);
            if i == self.current {
                bottom = sec.h;
            }
        }
        for id in [ID_ROOTS, ID_SKIP, ID_NAMES, ID_APPDIRS, ID_OPENWITH, ID_ALIASES] {
            unsafe { SendMessageW(item(self.hwnd, id), LB_SETITEMHEIGHT, Some(WPARAM(0)), Some(LPARAM(s(ITEM_H, d) as isize))) };
        }
        place(&self.footer, 0, client_h - FOOT_H + 14, d);
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
        for (k, sec) in self.sections.iter().enumerate() {
            for p in &sec.pos {
                unsafe {
                    let _ = ShowWindow(p.hwnd, if k == i { SW_SHOW } else { SW_HIDE });
                }
            }
        }
        if let Some(sec) = self.sections.get(i) {
            parts::set_decos(self.panel, sec.deco.clone());
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
        parts::set_fonts(self.fonts[F_HINT as usize], self.fonts[F_BOLD as usize]);
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
            self.paint_status(hdc, &rc);
            let _ = EndPaint(self.hwnd, &ps);
        }
    }

    fn status_rect(&self, client: &RECT) -> RECT {
        let d = self.dpi;
        RECT { left: s(FOOT_STATUS.0, d), top: client.bottom - s(FOOT_H - 4, d), right: s(FOOT_STATUS.1, d), bottom: client.bottom - s(2, d) }
    }

    /// 反映できたら緑の帯, 間違いがあれば赤い文字
    fn paint_status(&self, hdc: HDC, client: &RECT) {
        if !self.status_on {
            return;
        }
        let d = self.dpi;
        let r = self.status_rect(client);
        let hint = self.fonts[F_HINT as usize];
        match &self.status {
            Status::None => {}
            Status::Error(t) => parts::draw_lines(hdc, t, hint, pal().danger, r),
            Status::Ok(t) => {
                let bold = self.fonts[F_BOLD as usize];
                let w = (parts::text_width(hdc, t, bold) + s(50, d)).min(r.right - r.left);
                let cy = (r.top + r.bottom) / 2;
                let b = RECT { left: r.right - w, top: cy - s(16, d), right: r.right, bottom: cy + s(16, d) };
                parts::round(hdc, &b, s(16, d), None, pal().ok_bg);
                let k = d as f32 / 96.0;
                let (x, y) = (b.left as f32 + 14.0 * k, cy as f32);
                parts::stroke(hdc, &[(x, y + 0.5 * k), (x + 4.0 * k, y + 4.5 * k), (x + 11.0 * k, y - 4.5 * k)], 2.0 * k, pal().ok);
                parts::draw_text(hdc, t, bold, pal().ok, RECT { left: b.left + s(34, d), right: b.right - s(12, d), ..b }, DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS);
            }
        }
    }

    fn redraw_status(&self) {
        unsafe {
            let mut rc = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut rc);
            let r = self.status_rect(&rc);
            let _ = InvalidateRect(Some(self.hwnd), Some(&r), false);
        }
    }

    /// 下の帯に出す. できた知らせは少し消してから出し(続けて変えても気づくように), しばらくで消える
    fn say(&mut self, st: Status) {
        let ok = matches!(st, Status::Ok(_));
        self.status = st;
        self.status_on = !ok;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_SHOW);
            let _ = KillTimer(Some(self.hwnd), TIMER_HIDE);
            if ok {
                SetTimer(Some(self.hwnd), TIMER_SHOW, 100, None);
            }
        }
        self.redraw_status();
    }

    fn timer(&mut self, id: usize) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), id);
            if id == TIMER_SHOW {
                self.status_on = true;
                SetTimer(Some(self.hwnd), TIMER_HIDE, 4000, None);
            } else if matches!(self.status, Status::Ok(_)) {
                self.status = Status::None;
            }
        }
        self.redraw_status();
    }

    /// 画面の値から設定を作る. 間違いのある欄は前の値のままにし, 間違いの文も返す
    fn collect(&self) -> (Config, Option<String>) {
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
        c.prefix.snapshot = t(ID_PFX + 10);
        c.prefix.calendar = t(ID_PFX + 11);
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
        let key = |i: i32| ENTER_KEY_CHOICES.get(drop_sel(item(h, ID_KEYS + i))).copied().unwrap_or("").to_string();
        c.keys.chatgpt = key(0);
        c.keys.google = key(1);
        c.keys.write = key(2);
        c.keys.admin = key(3);
        c.keys.terminal = key(4);
        c.tools.terminal = self.terminal_name();
        c.chatgpt.send_enter = checked(item(h, ID_GPT_SEND));
        c.chatgpt.hide_marks = checked(item(h, ID_GPT_HIDE));
        let mut err = None;
        match t(ID_GPT_DELAY).parse() {
            Ok(v) => c.chatgpt.send_delay_ms = v,
            Err(_) => err = Some("Wait before Enter must be a number of ms.".to_string()),
        }
        match t(ID_GPT_IMAGE_WAIT).parse() {
            Ok(v) => c.chatgpt.image_wait_ms = v,
            Err(_) => {
                err.get_or_insert("Wait per image must be a number of ms.".to_string());
            }
        }
        c.browser.browser = selected_key(item(h, ID_BROWSER), &BROWSERS);
        c.browser.open_in = selected_key(item(h, ID_OPEN_IN), &OPEN_IN);
        let dir = |list: &[(String, String)], id: i32| list.get(drop_sel(item(h, id))).map(|(d, _)| d.clone()).unwrap_or_else(|| "Default".into());
        c.browser.chrome_profile = dir(&self.profiles, ID_PROFILE);
        c.browser.edge_profile = dir(&self.edge_profiles, ID_EDGE_PROFILE);
        c.chatgpt.mode = selected_key(item(h, ID_GPT_MODE), &GPT_MODES);
        if let Err(e) = c.validate() {
            err.get_or_insert(e);
            c = self.without_mistakes(c);
        }
        (c, err)
    }

    /// 間違いのある欄だけを前の値に戻す(ほかの欄は反映できるように)
    fn without_mistakes(&self, c: Config) -> Config {
        let o = &self.orig;
        let reverts: [&dyn Fn(&mut Config); 4] = [
            &|c| c.prefix = o.prefix.clone(),
            &|c| c.chatgpt.send_delay_ms = o.chatgpt.send_delay_ms,
            &|c| c.chatgpt.image_wait_ms = o.chatgpt.image_wait_ms,
            &|c| c.keys = o.keys.clone(),
        ];
        for f in reverts {
            let mut t = c.clone();
            f(&mut t);
            if t.validate().is_ok() {
                return t;
            }
        }
        let mut t = c;
        for f in reverts {
            f(&mut t);
        }
        if t.validate().is_ok() { t } else { o.clone() }
    }

    /// 変わっていれば, すぐ設定ファイルに書く(K45)
    fn apply(&mut self) -> Vec<Task> {
        if self.asking {
            return vec![];
        }
        let (c, err) = self.collect();
        let on = checked(item(self.hwnd, ID_AUTOSTART));
        let changed = c != self.orig;
        if changed {
            if self.broken {
                self.asking = true;
                return vec![Task::AskReplace];
            }
            let text = config::to_toml(&c);
            if let Err(e) = storage::write_config(&text) {
                self.say(Status::Error(tr(&format!("Cannot save config.toml: {e}"))));
                return vec![];
            }
            // ファイルを読み直した形で持つ(読むだけの古い項目の違いで, 作り直さないように)
            let c = Config::parse(&text).unwrap_or(c);
            if self.written.len() >= 8 {
                self.written.remove(0);
            }
            self.written.push(c.clone());
            self.orig = c.clone();
            self.work = c;
        }
        let auto = on != self.autostart;
        if auto {
            system::set_autostart(on, &self.exe);
            self.autostart = on;
        }
        match err {
            Some(e) => self.say(Status::Error(tr(&e))),
            None if changed || auto => self.say(Status::Ok(tr("Applied"))),
            None if !self.broken && matches!(self.status, Status::Error(_)) => self.say(Status::None),
            None => {}
        }
        vec![]
    }

    fn list_for(&mut self, id: i32) -> Option<&mut Vec<String>> {
        match id {
            ID_ROOTS => Some(&mut self.work.files.roots),
            ID_SKIP => Some(&mut self.work.files.skip_dirs),
            ID_NAMES => Some(&mut self.work.files.exclude),
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
    /// 実際のパス(`C:\Users\…`)のまま書く(K72. 前は`%USERPROFILE%\…`に縮めていた)
    fn picked(&mut self, id: i32, p: String) {
        match id {
            ID_ROOTS_ADD => self.list_add(ID_ROOTS, p),
            ID_SKIP_ADD => self.list_add(ID_SKIP, p),
            ID_APPDIRS_ADD => self.list_add(ID_APPDIRS, p),
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

    /// 一覧の番号から, アプリの呼び名か, Webページ・コマンドか
    fn alias_at(&self, i: usize) -> Option<ShortRef> {
        let keys = self.alias_keys();
        match keys.get(i) {
            Some(k) => Some(ShortRef::App(k.clone())),
            None => (i - keys.len() < self.work.command.len()).then(|| ShortRef::Command(i - keys.len())),
        }
    }

    fn edit_selected(&self, id: i32) -> Option<Task> {
        let i = list_sel(item(self.hwnd, id))?;
        match id {
            ID_ALIASES => {
                let orig = self.alias_at(i)?;
                let mut sh = Short::default();
                match &orig {
                    ShortRef::App(k) => {
                        sh.word = k.clone();
                        sh.value = self.work.alias.get(k)?.clone();
                    }
                    ShortRef::Command(n) => {
                        let c = self.work.command.get(*n)?;
                        sh.word = c.name.clone();
                        let (k, v) = c.target().unwrap_or((CommandKind::Url, ""));
                        sh.kind = k as usize;
                        sh.value = v.to_string();
                        sh.title = c.title.clone().unwrap_or_default();
                    }
                }
                Some(Task::Editor(EditKind::Alias { orig: Some(orig), short: sh }))
            }
            ID_OPENWITH => {
                let o = self.work.open_with.get(i)?.clone();
                Some(Task::Editor(EditKind::OpenWith { index: Some(i), ow: o }))
            }
            _ => None,
        }
    }

    /// 項目を直す窓のOK. 問題があれば文を返す
    fn apply_edit(&mut self, kind: &EditKind) -> Result<(), String> {
        match kind {
            EditKind::Alias { orig, short } => {
                let word = &short.word;
                if word.is_empty() || word.chars().any(char::is_whitespace) {
                    return Err("Type a word without spaces.".into());
                }
                if short.value.is_empty() {
                    return Err(["Choose or type the app name.", "Type the web address.", "Type the command.", "Choose or type the file.", "Choose or type the folder."][short.kind.min(4)].into());
                }
                let same_app = self.work.alias.keys().any(|k| k.eq_ignore_ascii_case(word) && orig.as_ref() != Some(&ShortRef::App(k.clone())));
                let same_cmd = self.work.command.iter().enumerate().any(|(i, c)| c.name.eq_ignore_ascii_case(word) && orig.as_ref() != Some(&ShortRef::Command(i)));
                if same_app || same_cmd {
                    return Err(format!("\"{word}\" is already an alias."));
                }
                // 種類が変わったときは, 元の場所から消して移す
                let mut at = None;
                match orig {
                    Some(ShortRef::App(k)) => {
                        self.work.alias.remove(k);
                    }
                    Some(ShortRef::Command(i)) if *i < self.work.command.len() => {
                        self.work.command.remove(*i);
                        at = Some(*i);
                    }
                    _ => {}
                }
                let row = if short.kind == 0 {
                    self.work.alias.insert(word.clone(), short.value.clone());
                    self.alias_keys().iter().position(|k| k == word).unwrap_or(0)
                } else {
                    let v = Some(short.value.clone());
                    let mut cmd = Command { name: word.clone(), title: (!short.title.is_empty()).then(|| short.title.clone()), ..Default::default() };
                    match short.kind {
                        1 => cmd.url = v,
                        3 => cmd.file = v,
                        4 => cmd.folder = v,
                        _ => cmd.run = v,
                    }
                    let i = at.unwrap_or(self.work.command.len());
                    self.work.command.insert(i, cmd);
                    self.work.alias.len() + i
                };
                self.refresh_aliases(row);
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
        let i = drop_sel(item(self.hwnd, ID_TERMINAL));
        if i >= 1 { self.work.open_with.get(i - 1).map(|o| o.name.clone()).unwrap_or_default() } else { String::new() }
    }

    fn refresh_aliases(&self, sel: usize) {
        let h = item(self.hwnd, ID_ALIASES);
        let rows = alias_rows(&self.work);
        fill_list(h, &rows);
        if !rows.is_empty() {
            unsafe { SendMessageW(h, LB_SETCURSEL, Some(WPARAM(sel.min(rows.len() - 1))), None) };
        }
    }

    /// 「別のアプリで開く」の一覧と, Alt+Enterの選びを作り直す
    fn refresh_open_with(&mut self, sel: Option<usize>) {
        let t = item(self.hwnd, ID_OPENWITH);
        fill_list(t, &open_with_rows(&self.work));
        if let Some(i) = sel.filter(|_| !self.work.open_with.is_empty()) {
            unsafe { SendMessageW(t, LB_SETCURSEL, Some(WPARAM(i.min(self.work.open_with.len() - 1))), None) };
        }
        if !self.work.open_with.iter().any(|o| o.name.eq_ignore_ascii_case(&self.work.tools.terminal)) {
            self.work.tools.terminal.clear();
        }
        let (items, sel) = terminals(&self.work);
        set_drop(item(self.hwnd, ID_TERMINAL), items, sel);
    }

    fn remove_selected(&mut self, id: i32) {
        let Some(i) = list_sel(item(self.hwnd, id)) else { return };
        if id == ID_OPENWITH {
            self.work.tools.terminal = self.terminal_name();
            if i < self.work.open_with.len() {
                self.work.open_with.remove(i);
            }
            self.refresh_open_with(Some(i));
            return;
        }
        match self.alias_at(i) {
            Some(ShortRef::App(k)) => {
                self.work.alias.remove(&k);
            }
            Some(ShortRef::Command(n)) => {
                self.work.command.remove(n);
            }
            None => {}
        }
        self.refresh_aliases(i);
    }

    /// 部品からの知らせ. 押した, 選んだ, 打ち終えて離れたときは, 変わっていればすぐ反映する
    fn command(&mut self, id: i32, code: u32) -> Vec<Task> {
        let mut tasks = self.handle(id, code);
        if [BN_CLICKED, CBN_SELCHANGE, EN_KILLFOCUS].contains(&code) {
            let first = self.apply();
            tasks.splice(0..0, first);
        }
        tasks
    }

    fn handle(&mut self, id: i32, code: u32) -> Vec<Task> {
        match id {
            ID_SIDE if code == LBN_SELCHANGE => {
                if let Some(i) = list_sel(self.side) {
                    self.show_section(i);
                }
                vec![]
            }
            ID_SAVE => {
                // Enterで押されたとき. 名前の入力欄なら足すだけ, 一覧なら直す
                let f = unsafe { GetFocus() };
                if f == item(self.hwnd, ID_NAME_EDIT) {
                    self.add_name(ID_NAME_EDIT, ID_NAMES);
                    return vec![];
                }
                for t in [ID_ALIASES, ID_OPENWITH] {
                    if f == item(self.hwnd, t) {
                        return self.edit_selected(t).into_iter().collect();
                    }
                }
                // `Enter`では, 打っている欄を反映する(このあと`apply`)
                vec![]
            }
            ID_PROMPT => vec![Task::CopyPrompt],
            ID_CANCEL => vec![Task::AskClose],
            ID_OPENFILE => vec![Task::OpenFile],
            ID_HOTKEY_CHANGE => vec![Task::ChangeHotkey],
            ID_SELHK_CHANGE => vec![Task::ChangeSelectionHotkey],
            ID_SELHK_CLEAR => vec![Task::ClearSelectionHotkey],
            ID_ROOTS_ADD => vec![Task::Pick { id, title: "Choose a folder to search" }],
            ID_SKIP_ADD => vec![Task::Pick { id, title: "Choose a folder to skip" }],
            ID_APPDIRS_ADD => vec![Task::Pick { id, title: "Choose a folder with apps" }],
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
            ID_NAME_DEL => {
                self.list_remove(ID_NAMES);
                vec![]
            }
            ID_NAME_RESET => {
                self.work.files.exclude = Files::default().exclude;
                fill_list(item(self.hwnd, ID_NAMES), &self.work.files.exclude);
                vec![]
            }
            ID_ALIAS_ADD => vec![Task::Editor(EditKind::Alias { orig: None, short: Short::default() })],
            ID_ALIAS_EDIT => self.edit_selected(ID_ALIASES).into_iter().collect(),
            ID_ALIAS_DEL => {
                self.remove_selected(ID_ALIASES);
                vec![]
            }
            // 一覧をダブルクリック
            ID_ALIASES | ID_OPENWITH if code == LBN_DBLCLK => self.edit_selected(id).into_iter().collect(),
            // キーワードの例を, 打った文字に合わせる
            _ if (ID_PFX..ID_PFX + PREFIX_LABELS.len() as i32).contains(&id) && code == EN_CHANGE => {
                let i = (id - ID_PFX) as usize;
                let h = item(self.hwnd, ID_PFX_EX + i as i32);
                set_raw_text(h, &prefix_note(i, &text(item(self.hwnd, id))));
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
                // 打っている途中の欄も反映してから閉じる(書き間違いのあるファイルは, 聞かずに置き換えない)
                let left = with_settings(|s| {
                    if s.broken {
                        s.collect().0 != s.orig
                    } else {
                        s.apply();
                        s.collect().1.is_some()
                    }
                });
                if left != Some(true) || system::message_box_yes_no(hwnd, "Some changes are not applied. Close anyway?", "Hikidashi Settings") {
                    close_settings();
                }
            }
            Task::AskReplace => {
                let yes = system::message_box_yes_no(hwnd, "config.toml has a mistake. Replace it with the settings shown here? The old file is kept as config.toml.bak.", "Hikidashi Settings");
                let more = with_settings(|s| {
                    s.asking = false;
                    if !yes {
                        return vec![];
                    }
                    let p = storage::config_path();
                    if let Err(e) = std::fs::copy(&p, p.with_extension("toml.bak")) {
                        s.say(Status::Error(tr(&format!("Cannot save config.toml: {e}"))));
                        return vec![];
                    }
                    s.broken = false;
                    s.apply()
                });
                run_tasks(more.unwrap_or_default());
            }
            Task::CopyPrompt => {
                if !system::message_box_yes_no(hwnd, "Copy a prompt for an AI, with all your current settings? It includes folder names and web addresses.", "Hikidashi Settings") {
                    continue;
                }
                with_settings(|s| {
                    // 拡張の設定(公開されていない値など)は, AIに渡す文に入れない(K52)
                    let shown = Config { ext: Default::default(), ..s.orig.clone() };
                    let p = hikidashi_core::prompt::config_prompt(&config::to_toml(&shown), super::i18n::is_ja());
                    s.say(match actions::copy_text(s.hwnd, &p) {
                        Ok(()) => Status::Ok(tr("Copied")),
                        Err(e) => Status::Error(e),
                    });
                });
            }
            Task::Pick { id, title } => {
                if let Some(p) = pick(hwnd, Pick::Folder, title) {
                    let more = with_settings(|s| {
                        s.picked(id, p);
                        s.apply()
                    });
                    run_tasks(more.unwrap_or_default());
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
                // 開いたままでよい. 直して保存すると, この画面も作り直す(`sync`)
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
        parts::clear();
    }
}

extern "system" fn settings_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| settings_inner(hwnd, msg, wparam, lparam))) {
        Ok(r) => r,
        Err(_) => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn settings_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(r) = common_proc(hwnd, msg, wparam, lparam) {
        return r;
    }
    if msg == WM_SYNC {
        sync();
        return LRESULT(0);
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
            WM_TIMER => {
                st.timer(wparam.0);
                Some(LRESULT(0))
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

// ---------------------------------------------------------------- エイリアスと「別のアプリで開く」を直す小さな窓

/// 一覧の中の場所
#[derive(Clone, PartialEq)]
enum ShortRef {
    App(String),
    Command(usize),
}

/// エイリアス. `kind`は`ALIAS_KINDS`の番号
#[derive(Clone, Default)]
struct Short {
    word: String,
    kind: usize,
    value: String,
    title: String,
}

#[derive(Clone)]
enum EditKind {
    Alias { orig: Option<ShortRef>, short: Short },
    OpenWith { index: Option<usize>, ow: OpenWith },
}

/// 種類によって出す部品. `modes`は出す種類の番号
struct Group {
    modes: &'static [usize],
    section: Section,
}

struct Editor {
    hwnd: HWND,
    owner: HWND,
    kind: EditKind,
    fonts: [HFONT; 4],
    base: Vec<Deco>,
    groups: Vec<Group>,
    apps: Vec<String>,
}

const ED_W: i32 = 520;
const APP_ROWS: i32 = 5;

/// 種類に合わせて, 部品を出したり隠したりする
fn editor_show_mode(hwnd: HWND) {
    let mode = drop_sel(item(hwnd, E_KIND));
    // 隠すときにフォーカスが動いて呼び戻されるので, 借りたままにしない
    let Some((mut decos, groups)) = EDITOR.with(|e| {
        e.borrow().as_ref().map(|e| (e.base.clone(), e.groups.iter().map(|g| (g.modes.contains(&mode), g.section.pos.clone(), g.section.deco.clone())).collect::<Vec<_>>()))
    }) else {
        return;
    };
    for (on, pos, deco) in groups {
        for p in pos {
            unsafe {
                let _ = ShowWindow(p.hwnd, if on { SW_SHOW } else { SW_HIDE });
            }
        }
        if on {
            decos.extend(deco);
        }
    }
    set_text(item(hwnd, E_VALUE_LABEL), ["Web address", "Web address", "Command", "File", "Folder"][mode.min(4)]);
    parts::set_decos(hwnd, decos);
}

/// 打った文字を含むアプリだけを一覧に出す
fn editor_filter(hwnd: HWND) {
    let Some(apps) = EDITOR.with(|e| e.borrow().as_ref().map(|e| e.apps.clone())) else { return };
    let t = text(item(hwnd, E_APP)).trim().to_lowercase();
    let list = item(hwnd, E_APPS);
    let exact = apps.iter().any(|a| a.to_lowercase() == t);
    let shown: Vec<String> = apps.iter().filter(|a| exact || t.is_empty() || a.to_lowercase().contains(&t)).cloned().collect();
    fill_list(list, &shown);
    if let Some(i) = shown.iter().position(|a| a.to_lowercase() == t) {
        unsafe { SendMessageW(list, LB_SETCURSEL, Some(WPARAM(i)), None) };
    }
}

fn open_editor(owner: HWND, kind: EditKind) {
    if editor_hwnd().is_some() {
        return;
    }
    let dpi = unsafe { GetDpiForWindow(owner) }.max(96);
    let title = match &kind {
        EditKind::Alias { orig: None, .. } => "Add alias",
        EditKind::Alias { .. } => "Edit alias",
        EditKind::OpenWith { index: None, .. } => "Add app",
        EditKind::OpenWith { .. } => "Edit app",
    };
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let t = WStr::new(&tr(title));
    let Ok(hwnd) = (unsafe { CreateWindowExW(WS_EX_CONTROLPARENT | WS_EX_DLGMODALFRAME, EDITOR_CLASS, t.pcwstr(), style, 0, 0, 100, 100, Some(owner), None, Some(hinst.into()), None) })
    else {
        return;
    };
    let mut apps = with_settings(|s| s.apps.clone()).unwrap_or_default();
    apps.sort_by_key(|a| a.to_lowercase());
    apps.dedup();
    let mut m = Maker { parent: hwnd, hinst: hinst.into(), made: vec![] };
    const X: i32 = 20;
    const FW: i32 = ED_W - 2 * X;
    let full = FW - FORM_L;
    let mut p = Page::new(&mut m, X, 20, FW);
    let mut groups = vec![];
    let mut bottom;
    let base;
    match &kind {
        EditKind::Alias { short, .. } => {
            p.form_edit("Word", E_NAME, &short.word, 160);
            p.form_hint("What you type, for example ppt");
            p.form_drop("Kind", E_KIND, tr_all(&ALIAS_KINDS), short.kind);
            let y = p.y;
            base = p.finish();
            let (app, other) = if short.kind == 0 { (short.value.as_str(), "") } else { ("", short.value.as_str()) };
            let mut p = Page::new(&mut m, X, y, FW);
            p.form_edit("App", E_APP, app, full);
            let h = p.m.list(E_APPS, &apps, &[]);
            let lh = APP_ROWS * ITEM_H + 12;
            p.deco(Kind::List { owner: h, head: 0, bar: 0 }, X + FORM_L, p.y, full, lh);
            p.at(h, X + FORM_L + 2, p.y + 4, full - 4, lh - 10);
            p.y += lh + 12;
            bottom = p.y;
            groups.push(Group { modes: &[0], section: p.finish() });
            // Webページ, コマンド, ファイル, フォルダー: 値の欄の下に, {q}の説明か参照のボタン(K71)
            let mut p = Page::new(&mut m, X, y, FW);
            let (x, vy) = (X + FORM_L, p.y);
            let l = p.m.label("Web address");
            unsafe { SetWindowLongPtrW(l, GWLP_ID, E_VALUE_LABEL as isize) };
            p.at(l, X, vy + 5, FORM_L - 8, 22);
            p.field(E_VALUE, other, 0, x, vy, full, CTL_H);
            p.y += CTL_H + 8;
            let y2 = p.y;
            groups.push(Group { modes: &[1, 2, 3, 4], section: p.finish() });
            let mut p = Page::new(&mut m, X, y2, FW);
            p.form_hint("{q} is replaced with what you type after the word.");
            let mut y3 = p.y;
            groups.push(Group { modes: &[1, 2], section: p.finish() });
            let mut p = Page::new(&mut m, X, y2, FW);
            let bw = button_w("Browse…", 72);
            let b = p.m.button(E_BROWSE, "Browse…");
            p.at(b, X + FORM_L, p.y, bw, CTL_H);
            p.y += CTL_H + 8;
            y3 = y3.max(p.y);
            groups.push(Group { modes: &[3, 4], section: p.finish() });
            let mut p = Page::new(&mut m, X, y3, FW);
            p.form_edit("Shown as", E_TITLE, &short.title, full);
            p.form_hint("Optional, for example Open Jira issue");
            bottom = bottom.max(p.y);
            groups.push(Group { modes: &[1, 2, 3, 4], section: p.finish() });
        }
        EditKind::OpenWith { ow, .. } => {
            p.form_edit("Name", E_NAME, &ow.name, full);
            p.form_hint("Shown as \"Open in <name>\"");
            let bw = button_w("Browse…", 72);
            p.form_edit("App (.exe)", E_VALUE, &ow.exe, full - bw - 8);
            let b = p.m.button(E_BROWSE, "Browse…");
            p.at(b, X + FW - bw, p.y - CTL_H - 8, bw, CTL_H);
            p.form_hint("Empty: VS Code, WezTerm and so on are found by name.");
            let args = ow.args.trim();
            let pass = if args.is_empty() { 0 } else if args == PASS_FOLDER { 1 } else { 2 };
            p.form_drop("What to pass", E_KIND, tr_all(&PASS_KINDS), pass);
            let y = p.y;
            base = p.finish();
            let mut p = Page::new(&mut m, X, y, FW);
            p.form_edit("Arguments", E_TITLE, if pass == 2 { args } else { "" }, full);
            p.form_hint("{path}: the selected item, {dir}: its folder");
            bottom = p.y;
            groups.push(Group { modes: &[2], section: p.finish() });
        }
    }
    let err = m.raw(w!("STATIC"), "", (SS_LEFT.0 | SS_NOPREFIX.0) as u32, WINDOW_EX_STYLE(0), E_ERROR, F_HINT);
    tint(err, |p| p.danger);
    let cancel = m.button(ID_CANCEL, "Cancel");
    let ok = m.button(ID_SAVE, "OK");
    let foot = [(err, X, bottom + 4, FW - 220, 36), (cancel, X + FW - 208, bottom + 6, 100, 32), (ok, X + FW - 100, bottom + 6, 100, 32)].map(|(hwnd, x, y, w, h)| Pos { hwnd, x, y, w, h });
    bottom += 58;
    let fonts = fonts_for(dpi);
    apply_fonts(&m.made, &fonts);
    dark::apply(hwnd);
    parts::hide_focus(hwnd);
    place(&base.pos, 0, 0, dpi);
    place(&foot, 0, 0, dpi);
    for p in base.pos.iter().chain(&foot) {
        unsafe {
            let _ = ShowWindow(p.hwnd, SW_SHOW);
        }
    }
    for g in &groups {
        place(&g.section.pos, 0, 0, dpi);
    }
    unsafe {
        SendMessageW(item(hwnd, E_APPS), LB_SETITEMHEIGHT, Some(WPARAM(0)), Some(LPARAM(s(ITEM_H, dpi) as isize)));
        let mut r = RECT { left: 0, top: 0, right: s(ED_W, dpi), bottom: s(bottom, dpi) };
        let _ = AdjustWindowRectExForDpi(&mut r, style, false, WS_EX_DLGMODALFRAME, dpi);
        let mut or = RECT::default();
        let _ = GetWindowRect(owner, &mut or);
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let (x, y) = (or.left + ((or.right - or.left) - w) / 2, or.top + ((or.bottom - or.top) - h) / 2);
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    EDITOR.with(|e| *e.borrow_mut() = Some(Editor { hwnd, owner, kind, fonts, base: base.deco, groups, apps }));
    editor_show_mode(hwnd);
    if !item(hwnd, E_APPS).is_invalid() {
        editor_filter(hwnd);
    }
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
    let k = drop_sel(item(hwnd, E_KIND));
    match kind {
        EditKind::Alias { orig, .. } => EditKind::Alias {
            orig: orig.clone(),
            short: Short { word: t(E_NAME), kind: k, value: if k == 0 { t(E_APP) } else { t(E_VALUE) }, title: t(E_TITLE) },
        },
        EditKind::OpenWith { index, .. } => {
            let args = [String::new(), PASS_FOLDER.to_string(), t(E_TITLE)][k.min(2)].clone();
            EditKind::OpenWith { index: *index, ow: OpenWith { name: t(E_NAME), exe: t(E_VALUE), args } }
        }
    }
}

fn editor_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(r) = common_proc(hwnd, msg, wparam, lparam) {
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
                            _ => {
                                close_editor();
                                run_tasks(with_settings(|s| s.apply()).unwrap_or_default());
                            }
                        }
                    }
                }
                ID_CANCEL => close_editor(),
                E_KIND if code == CBN_SELCHANGE => editor_show_mode(hwnd),
                E_BROWSE => {
                    // エイリアスの窓ではファイルかフォルダー(K71), アプリの窓ではexe
                    let alias = EDITOR.with(|e| e.borrow().as_ref().is_some_and(|e| matches!(e.kind, EditKind::Alias { .. })));
                    let (what, title) = match (alias, drop_sel(item(hwnd, E_KIND))) {
                        (true, 4) => (Pick::Folder, "Choose a folder"),
                        (true, _) => (Pick::File, "Choose a file"),
                        _ => (Pick::Exe, "Choose an app"),
                    };
                    if let Some(p) = pick(hwnd, what, title) {
                        set_raw_text(item(hwnd, E_VALUE), &p);
                    }
                }
                // 一覧で選んだ文字を入れたときは, 一覧を作り直さない
                E_APP if code == EN_CHANGE && unsafe { GetFocus() } == item(hwnd, E_APP) => editor_filter(hwnd),
                E_APPS if code == LBN_SELCHANGE || code == LBN_DBLCLK => {
                    let list = item(hwnd, E_APPS);
                    if let Some(i) = list_sel(list) {
                        set_raw_text(item(hwnd, E_APP), &list_text(list, i));
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            close_editor();
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint_surface(hwnd, 0);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
