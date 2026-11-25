//! 設定画面の部品と飾り(K44). 見た目をそろえるため, 入力欄の文字と一覧の中身のほかは自分で描く.
//! 角の丸い形はGDI+(Windowsの標準. CPUで描く)でなめらかに描く.

use super::i18n::tr;
use super::theme::*;
use super::util::{rgb, WStr};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Instant;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::GdiPlus::{
    FillModeAlternate, GdipAddPathArc, GdipClosePathFigure, GdipCreateFromHDC, GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath,
    GdipDeletePen, GdipDrawLines, GdipFillPath, GdipSetPenEndCap, GdipSetPenLineJoin, GdipSetPenStartCap, GdipSetPixelOffsetMode, GdipSetSmoothingMode, GdiplusStartup, GdiplusStartupInput,
    GpBrush, GpGraphics, GpPath, GpPen, GpSolidFill, LineCapRound, LineJoinRound, PixelOffsetModeHalf, PointF, SmoothingModeAntiAlias, Status, UnitPixel,
};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus, VK_DOWN, VK_F4, VK_SPACE, VK_UP};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const DROP_CLASS: PCWSTR = w!("HikidashiDrop");

pub fn s(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

fn dpi_of(h: HWND) -> u32 {
    unsafe { GetDpiForWindow(h) }.max(96)
}

// ---------------------------------------------------------------- 形(GDI+)

fn gdiplus() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| unsafe {
        let mut token = 0usize;
        let input = GdiplusStartupInput { GdiplusVersion: 1, ..Default::default() };
        GdiplusStartup(&mut token, &input, std::ptr::null_mut()) == Status(0)
    })
}

/// GDI+で描く. 使えなければfalse
fn with_graphics(hdc: HDC, f: impl FnOnce(*mut GpGraphics)) -> bool {
    if !gdiplus() {
        return false;
    }
    unsafe {
        let mut g: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut g) != Status(0) || g.is_null() {
            return false;
        }
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        GdipSetPixelOffsetMode(g, PixelOffsetModeHalf);
        f(g);
        GdipDeleteGraphics(g);
    }
    true
}

fn argb(c: u32) -> u32 {
    0xFF00_0000 | c
}

unsafe fn fill_path(g: *mut GpGraphics, rc: &RECT, inset: f32, radius: f32, color: u32) {
    let (x, y) = (rc.left as f32 + inset, rc.top as f32 + inset);
    let (wd, ht) = ((rc.right - rc.left) as f32 - inset * 2.0, (rc.bottom - rc.top) as f32 - inset * 2.0);
    if wd <= 0.0 || ht <= 0.0 {
        return;
    }
    let d = ((radius - inset).max(0.5) * 2.0).min(wd).min(ht);
    let mut path: *mut GpPath = std::ptr::null_mut();
    let mut b: *mut GpSolidFill = std::ptr::null_mut();
    GdipCreatePath(FillModeAlternate, &mut path);
    GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
    GdipAddPathArc(path, x + wd - d, y, d, d, 270.0, 90.0);
    GdipAddPathArc(path, x + wd - d, y + ht - d, d, d, 0.0, 90.0);
    GdipAddPathArc(path, x, y + ht - d, d, d, 90.0, 90.0);
    GdipClosePathFigure(path);
    GdipCreateSolidFill(argb(color), &mut b);
    GdipFillPath(g, b as *mut GpBrush, path);
    GdipDeleteBrush(b as *mut GpBrush);
    GdipDeletePath(path);
}

/// 角の丸い四角. `border`は枠の色と太さ(px)
pub fn round(hdc: HDC, rc: &RECT, radius: i32, border: Option<(u32, i32)>, fill: u32) {
    let ok = with_graphics(hdc, |g| unsafe {
        if let Some((c, bw)) = border {
            fill_path(g, rc, 0.0, radius as f32, c);
            fill_path(g, rc, bw as f32, radius as f32, fill);
        } else {
            fill_path(g, rc, 0.0, radius as f32, fill);
        }
    });
    if !ok {
        // GDI+が使えないときは, ぎざぎざの形で描く
        unsafe {
            let pen = CreatePen(PS_SOLID, 1, rgb(border.map_or(fill, |b| b.0)));
            let b = CreateSolidBrush(rgb(fill));
            let (op, ob) = (SelectObject(hdc, pen.into()), SelectObject(hdc, b.into()));
            let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius * 2, radius * 2);
            SelectObject(hdc, op);
            SelectObject(hdc, ob);
            let _ = DeleteObject(pen.into());
            let _ = DeleteObject(b.into());
        }
    }
}

/// つないだ線(チェックの印, プルダウンの印)
fn stroke(hdc: HDC, pts: &[(f32, f32)], width: f32, color: u32) {
    let ok = with_graphics(hdc, |g| unsafe {
        let mut pen: *mut GpPen = std::ptr::null_mut();
        GdipCreatePen1(argb(color), width, UnitPixel, &mut pen);
        GdipSetPenStartCap(pen, LineCapRound);
        GdipSetPenEndCap(pen, LineCapRound);
        GdipSetPenLineJoin(pen, LineJoinRound);
        let p: Vec<PointF> = pts.iter().map(|(x, y)| PointF { X: *x, Y: *y }).collect();
        GdipDrawLines(g, pen, p.as_ptr(), p.len() as i32);
        GdipDeletePen(pen);
    });
    if !ok {
        unsafe {
            let pen = CreatePen(PS_SOLID, width.round().max(1.0) as i32, rgb(color));
            let op = SelectObject(hdc, pen.into());
            for (i, (x, y)) in pts.iter().enumerate() {
                if i == 0 {
                    let _ = MoveToEx(hdc, *x as i32, *y as i32, None);
                } else {
                    let _ = LineTo(hdc, *x as i32, *y as i32);
                }
            }
            SelectObject(hdc, op);
            let _ = DeleteObject(pen.into());
        }
    }
}

pub fn brush(c: u32) -> HBRUSH {
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

pub fn fill(hdc: HDC, rc: &RECT, c: u32) {
    unsafe { FillRect(hdc, rc, brush(c)) };
}

fn font_of(h: HWND) -> HFONT {
    HFONT(unsafe { SendMessageW(h, WM_GETFONT, None, None) }.0 as *mut _)
}

fn draw_text(hdc: HDC, text: &str, font: HFONT, color: u32, rc: RECT, flags: DRAW_TEXT_FORMAT) {
    let mut t: Vec<u16> = text.encode_utf16().collect();
    if t.is_empty() {
        return;
    }
    let mut rc = rc;
    unsafe {
        let old = SelectObject(hdc, font.into());
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, rgb(color));
        DrawTextW(hdc, &mut t, &mut rc, DT_SINGLELINE | DT_NOPREFIX | flags);
        SelectObject(hdc, old);
    }
}

fn text_of(h: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(h, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}

/// ちらつかないよう, 裏に描いてから写す. GDI+のために原点をずらさない(左上からの大きさで裏を作る)
pub fn buffered(hdc: HDC, rc: &RECT, f: impl FnOnce(HDC)) {
    let (wd, ht) = (rc.right - rc.left, rc.bottom - rc.top);
    if wd <= 0 || ht <= 0 {
        return;
    }
    if rc.left < 0 || rc.top < 0 {
        f(hdc);
        return;
    }
    unsafe {
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
        if mem.is_invalid() || bmp.is_invalid() {
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            f(hdc);
            return;
        }
        let old = SelectObject(mem, bmp.into());
        f(mem);
        let _ = BitBlt(hdc, rc.left, rc.top, wd, ht, Some(mem), rc.left, rc.top, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
    }
}

// ---------------------------------------------------------------- 部品の色

/// テーマが変わっても付いていくよう, 色そのものではなく選び方で持つ
pub type Color = fn(&Palette) -> u32;

#[derive(Clone, Copy)]
struct Look {
    fg: Color,
    bg: Color,
}

thread_local! {
    static LOOK: RefCell<HashMap<isize, Look>> = RefCell::new(HashMap::new());
    static DECOS: RefCell<HashMap<isize, Vec<Deco>>> = RefCell::new(HashMap::new());
    static CHECKS: RefCell<HashMap<isize, Check>> = RefCell::new(HashMap::new());
    static DROPS: RefCell<HashMap<isize, Drop>> = RefCell::new(HashMap::new());
    static COLS: RefCell<HashMap<isize, Vec<i32>>> = RefCell::new(HashMap::new());
    static HINT_FONT: Cell<isize> = const { Cell::new(0) };
    static BOLD_FONT: Cell<isize> = const { Cell::new(0) };
}

fn look(h: HWND) -> Look {
    LOOK.with(|l| l.borrow().get(&(h.0 as isize)).copied().unwrap_or(Look { fg: |p| p.text, bg: |p| p.bg }))
}

/// 文字の色
pub fn tint(h: HWND, fg: Color) {
    let l = Look { fg, ..look(h) };
    LOOK.with(|m| m.borrow_mut().insert(h.0 as isize, l));
}

/// 部品の後ろの色(カードの上か, 一覧の下の帯か)
pub fn surface(h: HWND, bg: Color) {
    let l = Look { bg, ..look(h) };
    LOOK.with(|m| m.borrow_mut().insert(h.0 as isize, l));
}

pub fn colors(h: HWND) -> (u32, u32) {
    let l = look(h);
    ((l.fg)(pal()), (l.bg)(pal()))
}

/// 説明の小さい字と, 選んだ区分の太字
pub fn set_fonts(hint: HFONT, bold: HFONT) {
    HINT_FONT.with(|f| f.set(hint.0 as isize));
    BOLD_FONT.with(|f| f.set(bold.0 as isize));
}

fn extra_font(c: &'static std::thread::LocalKey<Cell<isize>>, h: HWND) -> HFONT {
    match c.with(|f| f.get()) {
        0 => font_of(h),
        f => HFONT(f as *mut _),
    }
}

/// 部品を作ったとき, 前に同じ番号だった部品の値を捨てる
pub fn forget(h: HWND) {
    let k = h.0 as isize;
    LOOK.with(|m| m.borrow_mut().remove(&k));
    CHECKS.with(|m| m.borrow_mut().remove(&k));
    DROPS.with(|m| m.borrow_mut().remove(&k));
    COLS.with(|m| m.borrow_mut().remove(&k));
}

/// マウスで使っている間は, 選んでいる部品の枠を出さない(`Tab`などを押すと出る)
pub fn hide_focus(hwnd: HWND) {
    unsafe { SendMessageW(hwnd, WM_CHANGEUISTATE, Some(WPARAM((UIS_SET | UISF_HIDEFOCUS << 16) as usize)), None) };
}

pub fn clear() {
    set_fonts(HFONT::default(), HFONT::default());
    LOOK.with(|m| m.borrow_mut().clear());
    DECOS.with(|m| m.borrow_mut().clear());
    CHECKS.with(|m| m.borrow_mut().clear());
    DROPS.with(|m| m.borrow_mut().clear());
    COLS.with(|m| m.borrow_mut().clear());
}

// ---------------------------------------------------------------- 飾り(カード, 枠, 区切り)

#[derive(Clone, Copy)]
pub enum Kind {
    Card,
    Line,
    /// 入力欄の枠. 部品にフォーカスがあると枠が青くなる
    Field(HWND),
    /// キーの枠
    Cap,
    /// 一覧の枠. 見出しと下の帯の高さ
    List { owner: HWND, head: i32, bar: i32 },
}

/// 位置は96dpiの値
#[derive(Clone, Copy)]
pub struct Deco {
    pub kind: Kind,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub fn set_decos(hwnd: HWND, v: Vec<Deco>) {
    DECOS.with(|m| m.borrow_mut().insert(hwnd.0 as isize, v));
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

fn decos(hwnd: HWND) -> Vec<Deco> {
    DECOS.with(|m| m.borrow().get(&(hwnd.0 as isize)).cloned().unwrap_or_default())
}

fn deco_rect(d: &Deco, dy: i32, dpi: u32) -> RECT {
    RECT { left: s(d.x, dpi), top: s(d.y, dpi) - dy, right: s(d.x + d.w, dpi), bottom: s(d.y + d.h, dpi) - dy }
}

/// 飾りを描く. `dy`はスクロールした量(px)
pub fn paint_decos(hwnd: HWND, hdc: HDC, dy: i32) {
    let dpi = dpi_of(hwnd);
    let p = pal();
    let one = s(1, dpi).max(1);
    let focus = unsafe { GetFocus() };
    let frame = |owner: HWND| if focus == owner { (FOCUS, s(2, dpi).max(2)) } else { (p.border, one) };
    for d in decos(hwnd) {
        let r = deco_rect(&d, dy, dpi);
        match d.kind {
            Kind::Card => round(hdc, &r, s(8, dpi), Some((p.border_faint, one)), p.card),
            Kind::Line => fill(hdc, &RECT { bottom: r.top + one, ..r }, p.border_faint),
            Kind::Field(owner) => round(hdc, &r, s(6, dpi), Some(frame(owner)), p.field),
            Kind::Cap => round(hdc, &r, s(6, dpi), Some((p.border, one)), p.bg_subtle),
            Kind::List { owner, head, bar } => {
                let (c, bw) = frame(owner);
                if head == 0 && bar == 0 {
                    round(hdc, &r, s(8, dpi), Some((c, bw)), p.field);
                    continue;
                }
                round(hdc, &r, s(8, dpi), Some((c, bw)), p.bg_subtle);
                let body = RECT { left: r.left + bw, top: r.top + if head > 0 { s(head, dpi) } else { bw }, right: r.right - bw, bottom: r.bottom - s(bar, dpi) };
                if head > 0 {
                    fill(hdc, &body, p.field);
                } else {
                    round(hdc, &body, s(7, dpi), None, p.field);
                    fill(hdc, &RECT { top: (body.top + body.bottom) / 2, ..body }, p.field);
                }
                fill(hdc, &RECT { top: body.bottom, bottom: body.bottom + one, ..body }, p.border_faint);
            }
        }
    }
}

/// フォーカスが変わった部品の枠を描き直す
pub fn focus_changed(hwnd: HWND, ctl: HWND, dy: i32) {
    let dpi = dpi_of(hwnd);
    for d in decos(hwnd) {
        if matches!(d.kind, Kind::Field(o) | Kind::List { owner: o, .. } if o == ctl) {
            let r = deco_rect(&d, dy, dpi);
            unsafe {
                let _ = InvalidateRect(Some(hwnd), Some(&r), true);
            }
        }
    }
}

/// 入力欄の枠の余白を押したら, その入力欄に入る
pub fn click_field(hwnd: HWND, x: i32, y: i32, dy: i32) {
    let dpi = dpi_of(hwnd);
    for d in decos(hwnd) {
        let r = deco_rect(&d, dy, dpi);
        if let Kind::Field(o) = d.kind {
            if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                unsafe {
                    let _ = SetFocus(Some(o));
                }
            }
        }
    }
}

// ---------------------------------------------------------------- ボタン

/// 普通のボタンと, 塗りのボタン, 文字だけのボタン
#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    Normal,
    Primary,
    Link,
}

pub unsafe fn draw_button(d: &DRAWITEMSTRUCT, style: Btn) {
    let hdc = d.hDC;
    let r = d.rcItem;
    let dpi = dpi_of(d.hwndItem);
    let pressed = d.itemState.0 & ODS_SELECTED.0 != 0;
    let focus = d.itemState.0 & ODS_FOCUS.0 != 0 && d.itemState.0 & ODS_NOFOCUSRECT.0 == 0;
    let disabled = d.itemState.0 & ODS_DISABLED.0 != 0;
    let p = pal();
    let font = font_of(d.hwndItem);
    let label = text_of(d.hwndItem);
    buffered(hdc, &r, |hdc| {
        fill(hdc, &r, colors(d.hwndItem).1);
        if style == Btn::Link {
            draw_text(hdc, &label, font, if pressed { p.text } else { p.accent_text }, r, DT_VCENTER | DT_LEFT);
            if focus {
                fill(hdc, &RECT { top: r.bottom - s(3, dpi), bottom: r.bottom - s(1, dpi), ..r }, p.accent_text);
            }
            return;
        }
        let (face, edge, fg) = if style == Btn::Primary {
            let c = if pressed { FILL_DOWN } else { FILL };
            (c, c, 0xFFFFFF)
        } else {
            (if pressed { p.btn_down } else { p.btn }, p.border, if disabled { p.text_tertiary } else { p.text })
        };
        let border = if focus { (FOCUS, s(2, dpi).max(2)) } else { (edge, s(1, dpi).max(1)) };
        round(hdc, &r, s(6, dpi), Some(border), face);
        draw_text(hdc, &label, font, fg, r, DT_VCENTER | DT_CENTER);
    });
}

// ---------------------------------------------------------------- チェックボックス

struct Check {
    /// 名前の下の説明
    hint: String,
    /// 表の中の1つ(箱だけ)
    cell: bool,
}

/// 標準のチェックボックスの見た目を付け替える(押し方と読み上げは標準のまま)
pub fn make_check(h: HWND, hint: &str, cell: bool) {
    CHECKS.with(|m| m.borrow_mut().insert(h.0 as isize, Check { hint: hint.to_string(), cell }));
    unsafe {
        let _ = SetWindowSubclass(h, Some(check_proc), 1, 0);
    }
}

extern "system" fn check_proc(h: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(h, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(h, &mut rc);
                buffered(hdc, &rc, |hdc| paint_check(h, hdc, &rc));
                let _ = EndPaint(h, &ps);
                return LRESULT(0);
            }
            _ => {}
        }
        let r = DefSubclassProc(h, msg, wparam, lparam);
        // 標準の部品が直に描くことがあるので, 状態が変わったら描き直す
        if matches!(msg, BM_SETCHECK | BM_SETSTATE | WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | WM_SETTEXT | WM_UPDATEUISTATE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_KEYDOWN | WM_KEYUP) && IsWindow(Some(h)).as_bool() {
            let _ = InvalidateRect(Some(h), None, false);
        }
        r
    }
}

fn paint_check(h: HWND, hdc: HDC, rc: &RECT) {
    let dpi = dpi_of(h);
    let p = pal();
    let (fg, bg) = colors(h);
    fill(hdc, rc, bg);
    let on = unsafe { SendMessageW(h, BM_GETCHECK, None, None).0 } as u32 == BST_CHECKED.0;
    let down = unsafe { SendMessageW(h, BM_GETSTATE, None, None).0 } as u32 & BST_PUSHED != 0;
    // マウスで押しただけのときは, 枠を出さない
    let focus = unsafe { GetFocus() } == h && unsafe { SendMessageW(h, WM_QUERYUISTATE, None, None).0 } as u32 & UISF_HIDEFOCUS == 0;
    let (hint, cell) = CHECKS.with(|m| m.borrow().get(&(h.0 as isize)).map(|c| (c.hint.clone(), c.cell)).unwrap_or_default());
    let size = s(20, dpi);
    let left = if cell { (rc.right - size) / 2 } else { rc.right - s(12, dpi) - size };
    let top = (rc.bottom - size) / 2;
    let b = RECT { left, top, right: left + size, bottom: top + size };
    if focus {
        let m = s(3, dpi);
        round(hdc, &RECT { left: b.left - m, top: b.top - m, right: b.right + m, bottom: b.bottom + m }, s(6, dpi), Some((FOCUS, s(2, dpi).max(2))), bg);
    }
    if on {
        round(hdc, &b, s(4, dpi), None, if down { FILL_DOWN } else { FILL });
        let k = dpi as f32 / 96.0;
        let at = |x: f32, y: f32| (left as f32 + x * k, top as f32 + y * k);
        stroke(hdc, &[at(5.0, 10.5), at(8.7, 14.0), at(15.0, 6.5)], 2.0 * k, 0xFFFFFF);
    } else {
        round(hdc, &b, s(4, dpi), Some((p.off, s(1, dpi).max(1) + (dpi > 96) as i32)), if down { p.btn_down } else { p.field });
    }
    if cell {
        return;
    }
    let mut t = RECT { left: s(12, dpi), top: 0, right: left - s(12, dpi), bottom: rc.bottom };
    if hint.is_empty() {
        draw_text(hdc, &text_of(h), font_of(h), fg, t, DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS);
    } else {
        // 2行. 上下の余白を同じにする
        let mid = rc.bottom / 2 + s(1, dpi);
        t.bottom = mid;
        draw_text(hdc, &text_of(h), font_of(h), fg, t, DT_BOTTOM | DT_LEFT | DT_END_ELLIPSIS);
        draw_text(hdc, &hint, extra_font(&HINT_FONT, h), p.text_tertiary, RECT { top: mid, bottom: rc.bottom, ..t }, DT_TOP | DT_LEFT | DT_END_ELLIPSIS);
    }
}

// ---------------------------------------------------------------- プルダウン

struct Drop {
    items: Vec<String>,
    sel: usize,
    font: isize,
    closed: Option<Instant>,
}

pub fn register_drop(hinst: windows::Win32::Foundation::HINSTANCE) {
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(drop_proc),
        hInstance: hinst,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
        lpszClassName: DROP_CLASS,
        ..Default::default()
    };
    unsafe { RegisterClassExW(&wc) };
}

fn drop_font(h: HWND) -> HFONT {
    HFONT(DROPS.with(|m| m.borrow().get(&(h.0 as isize)).map_or(0, |d| d.font)) as *mut _)
}

/// 選ぶものと, 今の選び
pub fn set_drop(h: HWND, items: Vec<String>, sel: usize) {
    DROPS.with(|m| {
        let mut m = m.borrow_mut();
        let d = m.entry(h.0 as isize).or_insert(Drop { items: vec![], sel: 0, font: 0, closed: None });
        d.sel = sel.min(items.len().saturating_sub(1));
        d.items = items;
    });
    drop_text(h);
}

pub fn drop_sel(h: HWND) -> usize {
    DROPS.with(|m| m.borrow().get(&(h.0 as isize)).map_or(0, |d| d.sel))
}

fn drop_info(h: HWND) -> (Vec<String>, usize) {
    DROPS.with(|m| m.borrow().get(&(h.0 as isize)).map(|d| (d.items.clone(), d.sel)).unwrap_or_default())
}

/// 窓の文字も今の値にする(読み上げのため)
fn drop_text(h: HWND) {
    let (items, sel) = drop_info(h);
    let t = WStr::new(items.get(sel).map_or("", |t| t.as_str()));
    unsafe {
        let _ = SetWindowTextW(h, t.pcwstr());
        let _ = InvalidateRect(Some(h), None, false);
    }
}

/// 選びを変えて, 親に知らせる(`CBN_SELCHANGE`)
fn drop_change(h: HWND, to: usize) {
    let changed = DROPS.with(|m| match m.borrow_mut().get_mut(&(h.0 as isize)) {
        Some(d) if to < d.items.len() && to != d.sel => {
            d.sel = to;
            true
        }
        _ => false,
    });
    if !changed {
        return;
    }
    drop_text(h);
    unsafe {
        let id = GetDlgCtrlID(h) as usize;
        if let Ok(p) = GetParent(h) {
            SendMessageW(p, WM_COMMAND, Some(WPARAM(id & 0xffff | (CBN_SELCHANGE as usize) << 16)), Some(LPARAM(h.0 as isize)));
        }
    }
}

fn drop_popup(h: HWND) {
    // 開いているときに同じ部品を押すと, 閉じた直後にまた押されたことになる
    let just = DROPS.with(|m| m.borrow().get(&(h.0 as isize)).and_then(|d| d.closed)).is_some_and(|t| t.elapsed().as_millis() < 150);
    if just {
        return;
    }
    let (items, sel) = drop_info(h);
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        for (i, t) in items.iter().enumerate() {
            let t = WStr::new(&t.replace('&', "&&"));
            let _ = AppendMenuW(menu, if i == sel { MF_STRING | MF_CHECKED } else { MF_STRING }, i + 1, t.pcwstr());
        }
        let mut r = RECT::default();
        let _ = GetWindowRect(h, &mut r);
        let ex = TPMPARAMS { cbSize: std::mem::size_of::<TPMPARAMS>() as u32, rcExclude: r };
        let cmd = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN | TPM_VERTICAL).0, r.left, r.bottom, h, Some(&ex));
        let _ = DestroyMenu(menu);
        DROPS.with(|m| {
            if let Some(d) = m.borrow_mut().get_mut(&(h.0 as isize)) {
                d.closed = Some(Instant::now());
            }
        });
        if cmd.0 > 0 {
            drop_change(h, cmd.0 as usize - 1);
        }
    }
}

extern "system" fn drop_proc(h: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop_inner(h, msg, wparam, lparam))) {
        Ok(r) => r,
        Err(_) => unsafe { DefWindowProcW(h, msg, wparam, lparam) },
    }
}

fn drop_inner(h: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(h, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(h, &mut rc);
                buffered(hdc, &rc, |hdc| {
                    let dpi = dpi_of(h);
                    let p = pal();
                    fill(hdc, &rc, colors(h).1);
                    let border = if GetFocus() == h { (FOCUS, s(2, dpi).max(2)) } else { (p.border, s(1, dpi).max(1)) };
                    round(hdc, &rc, s(6, dpi), Some(border), p.field);
                    let t = RECT { left: s(10, dpi), right: rc.right - s(30, dpi), ..rc };
                    draw_text(hdc, &text_of(h), drop_font(h), p.text, t, DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS);
                    let k = dpi as f32 / 96.0;
                    let (cx, cy) = (rc.right as f32 - 16.0 * k, rc.bottom as f32 / 2.0);
                    stroke(hdc, &[(cx - 4.0 * k, cy - 2.0 * k), (cx, cy + 2.0 * k), (cx + 4.0 * k, cy - 2.0 * k)], 1.6 * k, p.text_tertiary);
                });
                let _ = EndPaint(h, &ps);
                LRESULT(0)
            }
            WM_SETFONT => {
                DROPS.with(|m| {
                    if let Some(d) = m.borrow_mut().get_mut(&(h.0 as isize)) {
                        d.font = wparam.0 as isize;
                    }
                });
                let _ = InvalidateRect(Some(h), None, false);
                LRESULT(0)
            }
            WM_GETFONT => LRESULT(drop_font(h).0 as isize),
            WM_GETDLGCODE => LRESULT(DLGC_WANTARROWS as isize),
            WM_SETFOCUS | WM_KILLFOCUS => {
                let _ = InvalidateRect(Some(h), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let _ = SetFocus(Some(h));
                drop_popup(h);
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let vk = wparam.0 as u16;
                let alt = msg == WM_SYSKEYDOWN;
                if vk == VK_SPACE.0 && !alt || vk == VK_F4.0 && !alt || alt && (vk == VK_DOWN.0 || vk == VK_UP.0) {
                    drop_popup(h);
                    return LRESULT(0);
                }
                let sel = drop_sel(h);
                if vk == VK_DOWN.0 {
                    drop_change(h, sel + 1);
                } else if vk == VK_UP.0 && sel > 0 {
                    drop_change(h, sel - 1);
                } else {
                    return DefWindowProcW(h, msg, wparam, lparam);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(h, msg, wparam, lparam),
        }
    }
}

// ---------------------------------------------------------------- 一覧と区分

/// 一覧にする. `cols`は列の幅(96dpi. 最後の列は残り全部). 行の文字は`\t`で列に分ける
pub fn make_list(h: HWND, cols: &[i32]) {
    COLS.with(|m| m.borrow_mut().insert(h.0 as isize, cols.to_vec()));
    unsafe {
        let _ = SetWindowSubclass(h, Some(list_proc), 1, 0);
    }
}

/// 1つもないときは, そのことを書く
extern "system" fn list_proc(h: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let r = unsafe { DefSubclassProc(h, msg, wparam, lparam) };
    if msg == WM_PAINT && unsafe { SendMessageW(h, LB_GETCOUNT, None, None).0 } == 0 {
        unsafe {
            let hdc = GetDC(Some(h));
            let dpi = dpi_of(h);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let t = RECT { left: s(12, dpi), bottom: s(30, dpi), ..rc };
            draw_text(hdc, &tr("Nothing here yet"), extra_font(&HINT_FONT, h), pal().text_tertiary, t, DT_VCENTER | DT_LEFT);
            ReleaseDC(Some(h), hdc);
        }
    }
    r
}

pub unsafe fn draw_list_item(d: &DRAWITEMSTRUCT) {
    let r = d.rcItem;
    let p = pal();
    if d.itemID == u32::MAX {
        fill(d.hDC, &r, p.field);
        return;
    }
    let h = d.hwndItem;
    let dpi = dpi_of(h);
    let n = SendMessageW(h, LB_GETTEXTLEN, Some(WPARAM(d.itemID as usize)), None).0.max(0) as usize;
    let mut buf = vec![0u16; n + 1];
    SendMessageW(h, LB_GETTEXT, Some(WPARAM(d.itemID as usize)), Some(LPARAM(buf.as_mut_ptr() as isize)));
    let text = String::from_utf16_lossy(&buf[..n]);
    let cols = COLS.with(|m| m.borrow().get(&(h.0 as isize)).cloned().unwrap_or_default());
    let font = font_of(h);
    buffered(d.hDC, &r, |hdc| {
        fill(hdc, &r, p.field);
        if d.itemState.0 & ODS_SELECTED.0 != 0 {
            let m = s(4, dpi);
            round(hdc, &RECT { left: r.left + m, top: r.top + s(1, dpi), right: r.right - m, bottom: r.bottom - s(1, dpi) }, s(6, dpi), None, p.selected);
        }
        let mut x = r.left + s(12, dpi);
        for (i, cell) in text.split('\t').enumerate() {
            let right = cols.get(i).map_or(r.right - s(10, dpi), |cw| x + s(*cw, dpi));
            draw_text(hdc, cell, font, p.text, RECT { left: x, right: right - s(8, dpi), ..r }, DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS);
            x = right;
        }
    });
}

/// 左の区分. 選んだものは, 濃い地, 左の棒, 太字(K44)
pub unsafe fn draw_side_item(d: &DRAWITEMSTRUCT, name: &str) {
    let r = d.rcItem;
    let p = pal();
    let dpi = dpi_of(d.hwndItem);
    let sel = d.itemID != u32::MAX && d.itemState.0 & ODS_SELECTED.0 != 0;
    let focus = d.itemState.0 & ODS_FOCUS.0 != 0 && d.itemState.0 & ODS_NOFOCUSRECT.0 == 0;
    let base = font_of(d.hwndItem);
    buffered(d.hDC, &r, |hdc| {
        fill(hdc, &r, p.bg_subtle);
        if d.itemID == u32::MAX {
            return;
        }
        if sel {
            let b = RECT { top: r.top + s(1, dpi), bottom: r.bottom - s(1, dpi), ..r };
            let border = focus.then_some((FOCUS, s(1, dpi).max(1)));
            round(hdc, &b, s(6, dpi), border, p.selected);
            let x = r.left + s(4, dpi);
            round(hdc, &RECT { left: x, top: b.top + s(8, dpi), right: x + s(4, dpi), bottom: b.bottom - s(8, dpi) }, s(2, dpi), None, p.accent);
        }
        let font = if sel { extra_font(&BOLD_FONT, d.hwndItem) } else { base };
        let t = RECT { left: r.left + s(16, dpi), right: r.right - s(4, dpi), ..r };
        draw_text(hdc, name, font, if sel { p.text } else { p.text_secondary }, t, DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS);
    });
}
