//! 描画(GDI. GPUで描く仕組みは使わない. I1). 一覧, Tabのメニュー, 入力欄の左の印, 下の線の光り(J2b), 状態の行.

use super::icons::glyph;
use super::system::pick_font;
use super::theme::*;
use super::util::{mix, rgb, wide};
use super::App;
use hikidashi_core::builtin::SysAction;
use hikidashi_core::catalog::Kind;
use hikidashi_core::router::{Action, Icon, Row};
use std::sync::OnceLock;
use windows::Win32::Foundation::{RECT, SIZE};
use windows::Win32::Graphics::Gdi::*;

fn s(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

/// 使うフォント(本文はNoto Sans JPがなければYu Gothic UI. wakudoriと同じ)
pub struct Fonts {
    pub input: HFONT,
    pub name: HFONT,
    pub name_bold: HFONT,
    pub sub: HFONT,
    pub tag: HFONT,
    pub icon: HFONT,
    pub icon_small: HFONT,
}

pub(super) fn body_face() -> &'static str {
    static F: OnceLock<String> = OnceLock::new();
    F.get_or_init(|| pick_font(&["Noto Sans JP", "Yu Gothic UI", "Meiryo UI", "Segoe UI"]))
}

fn icon_face() -> &'static str {
    static F: OnceLock<String> = OnceLock::new();
    F.get_or_init(|| pick_font(&["Segoe Fluent Icons", "Segoe MDL2 Assets"]))
}

pub(super) fn font(face: &str, px: i32, weight: i32) -> HFONT {
    let mut lf = LOGFONTW { lfHeight: -px, lfWeight: weight, lfCharSet: DEFAULT_CHARSET, lfQuality: CLEARTYPE_QUALITY, ..Default::default() };
    let w = wide(face);
    let n = w.len().min(lf.lfFaceName.len());
    lf.lfFaceName[..n].copy_from_slice(&w[..n]);
    unsafe { CreateFontIndirectW(&lf) }
}

impl Fonts {
    pub fn new(dpi: u32) -> Fonts {
        let b = body_face();
        Fonts {
            input: font(b, s(INPUT_FONT, dpi), 400),
            name: font(b, s(NAME_FONT, dpi), 400),
            name_bold: font(b, s(NAME_FONT, dpi), 700),
            sub: font(b, s(SUB_FONT, dpi), 400),
            tag: font(b, s(TAG_FONT, dpi), 400),
            icon: font(icon_face(), s(18, dpi), 400),
            icon_small: font(icon_face(), s(15, dpi), 400),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for f in [self.input, self.name, self.name_bold, self.sub, self.tag, self.icon, self.icon_small] {
                let _ = DeleteObject(f.into());
            }
        }
    }
}

/// 入力欄の地の色の筆. ライトとダークで1本ずつ作って使い回す(K39)
fn brush_bg() -> HBRUSH {
    static LIGHT_B: OnceLock<isize> = OnceLock::new();
    static DARK_B: OnceLock<isize> = OnceLock::new();
    let b = if is_dark() { &DARK_B } else { &LIGHT_B };
    HBRUSH(*b.get_or_init(|| unsafe { CreateSolidBrush(rgb(pal().bg)).0 as isize }) as *mut _)
}

/// 入力欄の色(WM_CTLCOLOREDIT)
pub fn edit_colors(hdc: HDC) -> HBRUSH {
    unsafe {
        SetTextColor(hdc, rgb(pal().text));
        SetBkColor(hdc, rgb(pal().bg));
    }
    brush_bg()
}

fn fill(hdc: HDC, rc: &RECT, color: u32) {
    unsafe {
        let b = CreateSolidBrush(rgb(color));
        FillRect(hdc, rc, b);
        let _ = DeleteObject(b.into());
    }
}

fn round_fill(hdc: HDC, rc: &RECT, r: i32, color: u32) {
    unsafe {
        let b = CreateSolidBrush(rgb(color));
        let p = CreatePen(PS_SOLID, 1, rgb(color));
        let ob = SelectObject(hdc, b.into());
        let op = SelectObject(hdc, p.into());
        let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, r, r);
        SelectObject(hdc, ob);
        SelectObject(hdc, op);
        let _ = DeleteObject(b.into());
        let _ = DeleteObject(p.into());
    }
}

fn text_width(hdc: HDC, f: HFONT, t: &str) -> i32 {
    if t.is_empty() {
        return 0;
    }
    let w: Vec<u16> = t.encode_utf16().collect();
    let mut sz = SIZE::default();
    unsafe {
        let old = SelectObject(hdc, f.into());
        let _ = GetTextExtentPoint32W(hdc, &w, &mut sz);
        SelectObject(hdc, old);
    }
    sz.cx
}

fn text_height(hdc: HDC, f: HFONT) -> i32 {
    let mut tm = TEXTMETRICW::default();
    unsafe {
        let old = SelectObject(hdc, f.into());
        let _ = GetTextMetricsW(hdc, &mut tm);
        SelectObject(hdc, old);
    }
    tm.tmHeight
}

/// 空の文字は渡さない(空のVecの先頭は無効な場所で, DrawTextWの中で落ちる. K51)
fn draw_text(hdc: HDC, f: HFONT, t: &str, color: u32, rc: &mut RECT, flags: DRAW_TEXT_FORMAT) {
    if t.is_empty() {
        return;
    }
    let mut w: Vec<u16> = t.encode_utf16().collect();
    unsafe {
        let old = SelectObject(hdc, f.into());
        SetTextColor(hdc, rgb(color));
        DrawTextW(hdc, &mut w, rc, flags | DT_NOPREFIX | DT_SINGLELINE);
        SelectObject(hdc, old);
    }
}

/// 一致した文字を太字にして名前を描く(J7). 入りきらなければ末尾を「…」にする
fn draw_name(hdc: HDC, app: &App, name: &str, positions: &[u32], x: i32, y: i32, max_w: i32, color: u32, all_bold: bool) {
    let chars: Vec<char> = name.chars().collect();
    let bold_at = |i: usize| all_bold || positions.contains(&(i as u32));
    // 同じ太さの文字をまとめる
    let mut runs: Vec<(String, bool)> = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        let b = bold_at(i);
        match runs.last_mut() {
            Some((t, rb)) if *rb == b => t.push(*c),
            _ => runs.push((c.to_string(), b)),
        }
    }
    let f = |b: bool| if b { app.fonts.name_bold } else { app.fonts.name };
    let total: i32 = runs.iter().map(|(t, b)| text_width(hdc, f(*b), t)).sum();
    let ell = "…";
    let ell_w = text_width(hdc, app.fonts.name, ell);
    let mut limit = if total > max_w { max_w - ell_w } else { i32::MAX };
    let mut cx = x;
    unsafe {
        SetTextColor(hdc, rgb(color));
        for (t, b) in &runs {
            let font = f(*b);
            let mut part = t.clone();
            let mut wpart = text_width(hdc, font, &part);
            if wpart > limit {
                while !part.is_empty() && wpart > limit {
                    part.pop();
                    wpart = text_width(hdc, font, &part);
                }
            }
            if !part.is_empty() {
                let old = SelectObject(hdc, font.into());
                let wv: Vec<u16> = part.encode_utf16().collect();
                let _ = TextOutW(hdc, cx, y, &wv);
                SelectObject(hdc, old);
            }
            cx += wpart;
            limit -= wpart;
            if part.len() < t.len() {
                let old = SelectObject(hdc, app.fonts.name.into());
                let wv: Vec<u16> = ell.encode_utf16().collect();
                let _ = TextOutW(hdc, cx, y, &wv);
                SelectObject(hdc, old);
                break;
            }
        }
    }
}

fn kind_glyph(k: Kind) -> char {
    match k {
        Kind::App => glyph::OPEN,
        Kind::File => glyph::FILE,
        Kind::Folder => glyph::FOLDER,
        Kind::Bookmark => glyph::BOOKMARK,
        Kind::Window => glyph::WINDOW,
        Kind::Process => glyph::KILL,
        Kind::Setting => glyph::SETTING,
        Kind::System => glyph::POWER,
        Kind::Command => glyph::COMMAND,
    }
}

fn draw_glyph(hdc: HDC, app: &App, g: char, x: i32, y: i32, size: i32, color: u32, small: bool) {
    let mut rc = RECT { left: x, top: y, right: x + size, bottom: y + size };
    draw_text(hdc, if small { app.fonts.icon_small } else { app.fonts.icon }, &g.to_string(), color, &mut rc, DT_CENTER | DT_VCENTER);
}

/// 入力欄の左の印(K3)の幅
pub fn chip_width(app: &App) -> i32 {
    let label = chip_label(app);
    if label.is_empty() {
        return 0;
    }
    unsafe {
        let hdc = GetDC(Some(app.hwnd));
        let w = text_width(hdc, app.fonts.tag, &label) + s(16, app.dpi);
        ReleaseDC(Some(app.hwnd), hdc);
        w
    }
}

fn chip_label(app: &App) -> String {
    if app.capture {
        return "hotkey".into();
    }
    app.mode.label().to_string()
}

/// 入力欄の下の線の場所. 書く窓(K22)では下の案内の上
fn line_rect(app: &App, width: i32, height: i32) -> RECT {
    let y = if app.composing() { height - s(STATUS_H, app.dpi) - s(2, app.dpi) } else { s(INPUT_H, app.dpi) - s(2, app.dpi) };
    RECT { left: 0, top: y, right: width, bottom: y + s(2, app.dpi) }
}

pub fn invalidate_line(app: &App) {
    unsafe {
        let mut rc = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(app.hwnd, &mut rc);
        let lr = line_rect(app, rc.right, rc.bottom);
        let _ = InvalidateRect(Some(app.hwnd), Some(&lr), false);
    }
}

fn icon_of(row: &Row) -> (Option<String>, char) {
    match &row.icon {
        Icon::Path(p) => (Some(p.clone()), if row.tag == "folder" { glyph::FOLDER } else { glyph::FILE }),
        Icon::Kind(Kind::Folder) => (Some(super::icons::FOLDER_KEY.to_string()), glyph::FOLDER),
        Icon::Kind(k) => (None, kind_glyph(*k)),
        Icon::Calc => (None, glyph::CALC),
        Icon::ChatGpt => (None, glyph::CHAT),
        Icon::Google => (None, glyph::GLOBE),
        Icon::Web => (None, glyph::LINK),
        Icon::Favicon(_) => (None, glyph::BOOKMARK),
        Icon::Clip => (None, glyph::COPY),
    }
}

pub fn paint(app: &mut App) {
    let _d = super::diag::doing("paint");
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(app.hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(app.hwnd, &mut rc);
        let (w, h) = (rc.right, rc.bottom);
        // ちらつかないよう, 裏で描いてから写す
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old_bmp = SelectObject(mem, bmp.into());
        SetBkMode(mem, TRANSPARENT);
        fill(mem, &rc, pal().bg);
        let d = app.dpi;

        if app.composing() {
            paint_compose(mem, app, w, h);
            let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
            SelectObject(mem, old_bmp);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            let _ = EndPaint(app.hwnd, &ps);
            return;
        }

        // 入力欄の左の印
        let label = chip_label(app);
        if !label.is_empty() {
            let cw = chip_width(app);
            let ch = s(26, d);
            let top = (s(INPUT_H, d) - ch) / 2;
            let crc = RECT { left: s(PAD_X, d), top, right: s(PAD_X, d) + cw, bottom: top + ch };
            round_fill(mem, &crc, s(6, d), pal().bg_muted);
            let mut trc = crc;
            draw_text(mem, app.fonts.tag, &label, pal().text, &mut trc, DT_CENTER | DT_VCENTER);
        }

        // 入力欄の下の線. 打つたびに一瞬クレイ色に光る(J2b)
        let lr = line_rect(app, w, h);
        let base = RECT { left: 0, top: lr.bottom - s(1, d).max(1), right: w, bottom: lr.bottom };
        fill(mem, &lr, pal().bg);
        fill(mem, &base, pal().border_faint);
        if let Some(g) = app.glow {
            let e = g.elapsed().as_secs_f32() * 1000.0;
            let k = if e < 80.0 { e / 80.0 } else { 1.0 - (e - 80.0) / 600.0 };
            if k > 0.0 {
                fill(mem, &lr, mix(pal().border_faint, pal().accent, k));
            }
        }

        let mut y = s(INPUT_H, d);
        let pad = s(LIST_PAD, d);
        if let Some(menu) = &app.menu {
            let mut trc = RECT { left: s(PAD_X, d), top: y + s(6, d), right: w - s(PAD_X, d), bottom: y + s(MENU_TITLE_H, d) };
            draw_text(mem, app.fonts.sub, &menu.title, pal().text_tertiary, &mut trc, DT_VCENTER | DT_END_ELLIPSIS);
            y += s(MENU_TITLE_H, d) + pad;
            for (i, it) in menu.items.iter().enumerate() {
                let rr = RECT { left: pad, top: y, right: w - pad, bottom: y + s(ROW_H, d) };
                if i == menu.sel {
                    round_fill(mem, &rr, s(8, d), pal().bg_muted);
                }
                let isz = s(ICON, d);
                draw_glyph(mem, app, it.glyph, pad + s(12, d), y + (s(ROW_H, d) - isz) / 2, isz, pal().text_secondary, true);
                let fh = text_height(mem, app.fonts.name);
                draw_name(mem, app, &it.label, &[], pad + s(12 + ICON + 12, d), y + (s(ROW_H, d) - fh) / 2, w - s(120, d), pal().text, false);
                y += s(ROW_H, d);
            }
            y += pad;
            let mut frc = RECT { left: s(PAD_X, d), top: y, right: w - s(PAD_X, d), bottom: y + s(STATUS_H, d) };
            fill(mem, &RECT { left: 0, top: y, right: w, bottom: y + 1 }, pal().border_faint);
            draw_text(mem, app.fonts.sub, "Enter Run    ← Back", pal().text_tertiary, &mut frc, DT_VCENTER);
        } else if !app.rows.is_empty() {
            y += pad;
            let vis = app.visible_rows();
            let top = app.top.min(app.rows.len().saturating_sub(vis));
            let rows: Vec<Row> = app.rows[top..top + vis].to_vec();
            let list_top = y;
            for (i, row) in rows.iter().enumerate() {
                draw_row(mem, app, row, top + i, top + i == app.sel && !app.nothing_selected(), pad, y, w);
                y += s(ROW_H, d);
            }
            // 9行目からあるときは, 右端に細いスクロールの印を描く(C2)
            if app.rows.len() > vis && vis > 0 {
                let track = (vis as i32) * s(ROW_H, d);
                let thumb = (track * vis as i32 / app.rows.len() as i32).max(s(16, d));
                let max_top = (app.rows.len() - vis) as i32;
                let ty = list_top + (track - thumb) * top as i32 / max_top.max(1);
                let bw = s(3, d).max(2);
                let x = w - s(4, d) - bw / 2;
                round_fill(mem, &RECT { left: x, top: ty, right: x + bw, bottom: ty + thumb }, bw, pal().border);
            }
            y += pad;
        }

        // 状態の行(索引を作っている, 設定の書き間違い, エラーなど)
        if app.menu.is_none() {
            if let Some((t, err)) = app.status_text() {
                let top = h - s(STATUS_H, d);
                fill(mem, &RECT { left: 0, top, right: w, bottom: h }, pal().bg_subtle);
                let mut src = RECT { left: s(PAD_X, d), top, right: w - s(PAD_X, d), bottom: h };
                draw_text(mem, app.fonts.sub, &t, if err { pal().danger } else { pal().text_tertiary }, &mut src, DT_VCENTER | DT_END_ELLIPSIS);
            }
        }
        let _ = y;

        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old_bmp);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        let _ = EndPaint(app.hwnd, &ps);
    }
}

/// 下の線(J2b)を描く. 打つたびに一瞬クレイ色に光る
fn draw_line(hdc: HDC, app: &App, w: i32, h: i32) {
    let d = app.dpi;
    let lr = line_rect(app, w, h);
    let base = RECT { left: 0, top: lr.bottom - s(1, d).max(1), right: w, bottom: lr.bottom };
    fill(hdc, &lr, pal().bg);
    fill(hdc, &base, pal().border_faint);
    if let Some(g) = app.glow {
        let e = g.elapsed().as_secs_f32() * 1000.0;
        let k = if e < 80.0 { e / 80.0 } else { 1.0 - (e - 80.0) / 600.0 };
        if k > 0.0 {
            fill(hdc, &lr, mix(pal().border_faint, pal().accent, k));
        }
    }
}

/// 長い質問を書く窓(K22). 中身はRichEditが描くので, 下の線と案内だけ
fn paint_compose(hdc: HDC, app: &App, w: i32, h: i32) {
    let d = app.dpi;
    draw_line(hdc, app, w, h);
    let top = h - s(STATUS_H, d);
    fill(hdc, &RECT { left: 0, top, right: w, bottom: h }, pal().bg_subtle);
    let n = app.compose.as_ref().map(|c| c.chars()).unwrap_or(0);
    let count = format!("{} chars", n);
    let cw = text_width(hdc, app.fonts.sub, &count);
    let mut crc = RECT { left: w - s(PAD_X, d) - cw, top, right: w - s(PAD_X, d), bottom: h };
    draw_text(hdc, app.fonts.sub, &count, pal().text_tertiary, &mut crc, DT_VCENTER | DT_RIGHT);
    // 添付した画像(左に小さな箱). 知らせがなければ, 続けて外す方法を出す
    let sizes: Vec<(u32, u32)> = app.images.iter().map(|i| (i.width, i.height)).collect();
    let mut left = s(PAD_X, d);
    if let Some(label) = hikidashi_core::attach::label(&sizes) {
        let lw = text_width(hdc, app.fonts.sub, &label) + s(14, d);
        let lh = s(20, d);
        let lt = top + (h - top - lh) / 2;
        let lrc = RECT { left, top: lt, right: (left + lw).min(crc.left - s(12, d)), bottom: lt + lh };
        round_fill(hdc, &lrc, s(6, d), pal().bg_muted);
        let mut trc = lrc;
        draw_text(hdc, app.fonts.sub, &label, pal().text, &mut trc, DT_CENTER | DT_VCENTER | DT_END_ELLIPSIS);
        left = lrc.right + s(10, d);
    }
    let hint = if sizes.is_empty() { "Ctrl+↑  Earlier questions" } else { hikidashi_core::attach::REMOVE_HINT };
    let (t, err) = app.status_text().unwrap_or_else(|| (hint.into(), false));
    let mut src = RECT { left, top, right: crc.left - s(12, d), bottom: h };
    draw_text(hdc, app.fonts.sub, &t, if err { pal().danger } else { pal().text_tertiary }, &mut src, DT_VCENTER | DT_END_ELLIPSIS);
    // 上の帯(K24のQ2): ChatGPTに送る窓だと分かるよう, 左に印(接頭辞の印と同じ形), 右にキーの案内
    let bh = s(COMPOSE_BAND, d);
    fill(hdc, &RECT { left: 0, top: 0, right: w, bottom: bh }, pal().bg_subtle);
    fill(hdc, &RECT { left: 0, top: bh - s(1, d).max(1), right: w, bottom: bh }, pal().border_faint);
    let label = "ChatGPT";
    let cw = text_width(hdc, app.fonts.tag, label) + s(16, d);
    let ch = s(24, d);
    let ct = (bh - ch) / 2;
    let crc = RECT { left: s(PAD_X, d), top: ct, right: s(PAD_X, d) + cw, bottom: ct + ch };
    round_fill(hdc, &crc, s(6, d), pal().bg_muted);
    let mut trc = crc;
    draw_text(hdc, app.fonts.tag, label, pal().text, &mut trc, DT_CENTER | DT_VCENTER);
    let keys = "Enter  Send     Shift+Enter  New line     Esc  Back";
    let mut krc = RECT { left: crc.right + s(12, d), top: 0, right: w - s(PAD_X, d), bottom: bh };
    draw_text(hdc, app.fonts.sub, keys, pal().text_tertiary, &mut krc, DT_VCENTER | DT_RIGHT | DT_END_ELLIPSIS);
}

/// 帯(K52): 区切りごとの小さな箱. 上に名前, 下にビット
fn draw_band(hdc: HDC, app: &App, band: &[hikidashi_ext::Seg], pad: i32, y: i32, w: i32) {
    let d = app.dpi;
    let p = pal();
    let (top, bottom) = (y + s(5, d), y + s(ROW_H, d) - s(5, d));
    let mut x = pad + s(12, d);
    let right = w - pad - s(8, d);
    for (i, g) in band.iter().enumerate() {
        if i > 0 {
            x += s(if g.gap { 8 } else { 2 }, d);
        }
        let bw = text_width(hdc, app.fonts.sub, &g.label).max(text_width(hdc, app.fonts.tag, &g.bits)) + s(12, d);
        if x + bw > right {
            break;
        }
        let rc = RECT { left: x, top, right: x + bw, bottom };
        let edge = g.kind == "b";
        let c = match g.kind.as_str() {
            "sa" => mix(p.bg, p.accent, 0.30),
            "addr" => mix(p.bg, p.accent, 0.16),
            "data" => p.ok_bg,
            "mask" => mix(p.bg, p.text, 0.22),
            "b" => p.border,
            _ => mix(p.bg, p.text, 0.10),
        };
        round_fill(hdc, &rc, s(6, d), if g.bad { p.danger } else { c });
        // SSCとB, 誤りのあるフレームは枠だけ
        if edge || g.bad {
            let t = s(if g.bad { 2 } else { 1 }, d).max(1);
            let inner = RECT { left: rc.left + t, top: rc.top + t, right: rc.right - t, bottom: rc.bottom - t };
            round_fill(hdc, &inner, s(5, d), if edge { p.bg } else { c });
        }
        let mid = if g.bits.is_empty() { bottom } else { (top + bottom) / 2 };
        let mut lrc = RECT { left: rc.left, top: rc.top, right: rc.right, bottom: mid };
        draw_text(hdc, app.fonts.sub, &g.label, p.text_secondary, &mut lrc, DT_CENTER | DT_VCENTER);
        let mut brc = RECT { left: rc.left, top: mid, right: rc.right, bottom: rc.bottom };
        draw_text(hdc, app.fonts.tag, &g.bits, p.text, &mut brc, DT_CENTER | DT_VCENTER);
        x += bw;
    }
}

fn draw_row(hdc: HDC, app: &mut App, row: &Row, index: usize, selected: bool, pad: i32, y: i32, w: i32) {
    let d = app.dpi;
    let rh = s(ROW_H, d);
    let rr = RECT { left: pad, top: y, right: w - pad, bottom: y + rh };
    if selected {
        round_fill(hdc, &rr, s(8, d), pal().bg_muted);
    }
    if app.ext.view {
        if let Some(b) = app.ext.bands.get(index).filter(|b| !b.is_empty()) {
            draw_band(hdc, app, b, pad, y, w);
            return;
        }
    }
    let bad = app.ext.view && app.ext.bads.get(index).copied().unwrap_or(false);
    // アイコン(K2)
    let isz = s(ICON, d);
    let ix = pad + s(12, d);
    let iy = y + (rh - isz) / 2;
    let (path, g) = icon_of(row);
    let drawn = match (&path, &row.icon) {
        (Some(p), _) => app.icons.draw(hdc, p, ix, iy),
        // ブックマークはサイトのアイコン(favicon. 2026-11-24)
        (None, Icon::Favicon(url)) => {
            let png = app.favicons.lock().unwrap().get(url).cloned();
            app.icons.draw_png(hdc, url, png.as_deref(), ix, iy)
        }
        _ => false,
    };
    if !drawn {
        draw_glyph(hdc, app, g, ix, iy, isz, pal().text_secondary, false);
    }
    // 右端の種類(K9). killはクレイ色
    let confirm = match &row.action {
        Action::Item { .. } => app.confirm.is_some() && selected && row.tag == "system",
        _ => false,
    };
    let tag = row.tag;
    let tw = text_width(hdc, app.fonts.tag, tag);
    let tx = w - pad - s(12, d) - tw;
    let mut trc = RECT { left: tx, top: y, right: w - pad - s(12, d), bottom: y + rh };
    draw_text(hdc, app.fonts.tag, tag, if row.warn || confirm { pal().accent_text } else { pal().text_tertiary }, &mut trc, DT_VCENTER | DT_RIGHT);

    let nx = ix + isz + s(12, d);
    let max_w = tx - s(12, d) - nx;
    let name_h = text_height(hdc, app.fonts.name);
    let sub_h = text_height(hdc, app.fonts.sub);
    let (name, sub, color, bold) = if confirm {
        let a = app.confirm.as_deref().and_then(SysAction::from_id);
        (a.map(|a| a.confirm_text().to_string()).unwrap_or_default(), "Esc to cancel".to_string(), pal().accent_text, true)
    } else {
        (row.name.clone(), row.sub.clone(), if bad { pal().danger } else { pal().text }, false)
    };
    if sub.is_empty() {
        draw_name(hdc, app, &name, &row.positions, nx, y + (rh - name_h) / 2, max_w, color, bold);
    } else {
        let top = y + (rh - name_h - sub_h - s(1, d)) / 2;
        draw_name(hdc, app, &name, if confirm { &[] } else { &row.positions }, nx, top, max_w, color, bold);
        let mut src = RECT { left: nx, top: top + name_h + s(1, d), right: nx + max_w, bottom: top + name_h + s(1, d) + sub_h };
        let path_like = sub.contains('\\');
        draw_text(hdc, app.fonts.sub, &sub, pal().text_tertiary, &mut src, if path_like { DT_PATH_ELLIPSIS } else { DT_END_ELLIPSIS });
    }
}
