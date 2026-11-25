//! 長い質問を書く窓(K22). Windows標準のRichEditにMarkdownを書き, Obsidianのライブプレビューに寄せて見せる.
//! - 見出しと行の中の記号(`#`, `**`など)は, カーソルのある行だけ見せ, ほかの行は隠す(Q3のA).
//! - 箇条書き・引用・チェックボックス・区切り線の印は, カーソルが印の中になければ絵にする(•, 縦線, □, 横線).
//!   印の字は消さずに地の色にし, RichEditが描いたあとに上から絵を描く(字の位置が変わらない. Obsidianと同じ).
//! 文字の位置はRichEditの位置(UTF-16の単位. 段落の区切りは`\r`の1文字)で扱う.

use super::paint::{body_face, font};
use super::system::pick_font;
use super::theme::*;
use super::util::{rgb, wide};
use hikidashi_core::markdown::{self, Block, Inline, Line};
use std::cell::RefCell;
use std::sync::OnceLock;
use windows::core::{w, IUnknown, Interface};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, POINTL, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::LoadLibraryW;
use windows::Win32::UI::Controls::RichEdit::*;
use windows::Win32::UI::Controls::{EM_LINEINDEX, EM_POSFROMCHAR, EM_REPLACESEL};
use windows::Win32::UI::WindowsAndMessaging::*;

/// 本文の字の大きさ(96dpiのときのピクセル). 検索の入力欄と同じ
const BODY_PX: i32 = INPUT_FONT;
/// 見出しの大きさ(本文に対する倍率. Obsidianと同じ)
const HEADING: [f32; 6] = [1.802, 1.602, 1.424, 1.266, 1.125, 1.0];
/// コードの大きさ(本文に対する倍率. Obsidianと同じ)
const MONO: f32 = 0.875;
/// 行の高さ(RichEditの「行」に対する倍率×20. 23で1.15行. 日本語の字の行の高さは字の1.3倍ほどなので, 字の1.5倍ほどになる)
const LINE_SPACING: i32 = 23;

fn mono_face() -> &'static str {
    static F: OnceLock<String> = OnceLock::new();
    F.get_or_init(|| pick_font(&["Cascadia Mono", "Consolas", "MS Gothic"]))
}

/// RichEditが描いたあとに上から描く絵
#[derive(Clone, Copy)]
enum Deco {
    /// 箇条書きの「•」(`-`の字の位置)
    Dot(usize),
    /// 引用の縦線(`>`の字の位置, 段落の頭と終わり)
    Bar(usize, usize, usize),
    /// チェックボックス(`[`の字の位置, 済みか)
    Check(usize, bool),
    /// 区切り線(段落の頭)
    Rule(usize),
}

#[derive(Default)]
struct Decos {
    items: Vec<Deco>,
    /// 最後の行など, 次の行から高さを測れないときの1行の高さ(ピクセル)
    line_h: i32,
    dpi: u32,
    /// 字の位置を聞く(行の高さを推定しないため. K50)
    doc: Option<ITextDocument>,
    /// 整えたときの文字の数とカーソル. 変換中など, 整える前に文字が増減したら, カーソルより後ろの絵をその分ずらす
    len: usize,
    at: usize,
}

thread_local! {
    static DECOS: RefCell<Decos> = RefCell::new(Decos::default());
}

/// 最後に整えたときの行(カーソルが動いたときに整え直すか決める)
#[derive(Clone, Copy)]
struct LineInfo {
    start: usize,
    end: usize,
    marker_end: usize,
}

/// 行の見た目を決めるもの. 同じなら整え直さない
#[derive(Clone, PartialEq)]
struct LineKey {
    text: Vec<u16>,
    block: Block,
    active: bool,
    in_marker: bool,
    block_active: bool,
    first: bool,
}

pub struct Compose {
    pub hwnd: HWND,
    /// 整えるときに元に戻す(Ctrl+Z)の記録を止めるため
    doc: Option<ITextDocument>,
    /// 書く窓を出している
    pub active: bool,
    /// 日本語入力で変換している間は整えない
    pub ime: bool,
    pub hide_marks: bool,
    /// 最後に整えたときのカーソル(行, 印の中か)
    shown: Option<(usize, bool)>,
    lines: Vec<LineInfo>,
    /// 前に整えたときの行(変わった行だけ整える. K51)と, そのときの大きさ, 設定, テーマ
    keys: Vec<LineKey>,
    env: (u32, bool, bool),
    /// 絵の位置だけを集め, 字は変えない
    dry: std::cell::Cell<bool>,
    /// 画面の拡大率(字の大きさに使う)
    pub dpi: u32,
    /// RichEditがtwipをピクセルにするときの拡大率
    dc_dpi: i32,
}

/// 字の幅を測る(整えている間だけ使う)
struct Measure {
    dc: HDC,
    hwnd: HWND,
    font: HFONT,
    old: HGDIOBJ,
    dc_dpi: i32,
}

impl Measure {
    fn new(hwnd: HWND, px: i32, dc_dpi: i32) -> Measure {
        unsafe {
            let dc = GetDC(Some(hwnd));
            let f = font(body_face(), px, 400);
            let old = SelectObject(dc, f.into());
            Measure { dc, hwnd, font: f, old, dc_dpi }
        }
    }

    /// 本文の字での幅(twip)
    fn twips(&self, s: &str) -> i32 {
        let w: Vec<u16> = s.encode_utf16().collect();
        if w.is_empty() {
            return 0;
        }
        let mut sz = SIZE::default();
        unsafe {
            let _ = GetTextExtentPoint32W(self.dc, &w, &mut sz);
        }
        sz.cx * 1440 / self.dc_dpi
    }

    fn line_h(&self) -> i32 {
        let mut tm = TEXTMETRICW::default();
        unsafe {
            let _ = GetTextMetricsW(self.dc, &mut tm);
        }
        tm.tmHeight * LINE_SPACING / 20
    }
}

impl Drop for Measure {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.font.into());
            ReleaseDC(Some(self.hwnd), self.dc);
        }
    }
}

impl Compose {
    pub fn create(parent: HWND) -> Option<Compose> {
        unsafe {
            LoadLibraryW(w!("Msftedit.dll")).ok()?;
            let style = WS_CHILD | WS_VSCROLL | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_NOHIDESEL) as u32);
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), w!("RICHEDIT50W"), w!(""), style, 0, 0, 100, 100, Some(parent), None, None, None).ok()?;
            SendMessageW(hwnd, EM_SETBKGNDCOLOR, Some(WPARAM(0)), Some(LPARAM(rgb(pal().bg).0 as isize)));
            SendMessageW(hwnd, EM_EXLIMITTEXT, Some(WPARAM(0)), Some(LPARAM(200_000)));
            SendMessageW(hwnd, EM_SETEVENTMASK, Some(WPARAM(0)), Some(LPARAM((ENM_CHANGE | ENM_SELCHANGE | ENM_REQUESTRESIZE) as isize)));
            // コードのかたまりの背景を行の右端まで伸ばす
            SendMessageW(hwnd, EM_SETEDITSTYLE, Some(WPARAM(SES_EXTENDBACKCOLOR as usize)), Some(LPARAM(SES_EXTENDBACKCOLOR as isize)));
            // 外から文書や画像を落とされないようにする(貼り付けも字だけにする)
            let _ = windows::Win32::System::Ole::RevokeDragDrop(hwnd);
            // スクロールの帯もダークに合わせる
            super::dark::scroll_bars(hwnd);
            let mut raw: *mut core::ffi::c_void = std::ptr::null_mut();
            SendMessageW(hwnd, EM_GETOLEINTERFACE, Some(WPARAM(0)), Some(LPARAM(&mut raw as *mut _ as isize)));
            let doc = if raw.is_null() { None } else { IUnknown::from_raw(raw).cast::<ITextDocument>().ok() };
            let dc = GetDC(Some(hwnd));
            let dc_dpi = GetDeviceCaps(Some(dc), LOGPIXELSY).max(1);
            ReleaseDC(Some(hwnd), dc);
            Some(Compose { hwnd, doc, active: false, ime: false, hide_marks: true, shown: None, lines: vec![], keys: vec![], env: (0, false, false), dry: std::cell::Cell::new(false), dpi: 96, dc_dpi })
        }
    }

    // ---- 文字 ----

    /// 中の文字(段落の区切りは`\r`)
    pub fn text(&self) -> String {
        unsafe {
            let n = GetWindowTextLengthW(self.hwnd).max(0) as usize + 1;
            let mut buf = vec![0u16; n];
            let gt = GETTEXTEX { cb: (n * 2) as u32, flags: GT_DEFAULT, codepage: 1200, lpDefaultChar: windows::core::PCSTR::null(), lpUsedDefChar: std::ptr::null_mut() };
            let got = SendMessageW(self.hwnd, EM_GETTEXTEX, Some(WPARAM(&gt as *const _ as usize)), Some(LPARAM(buf.as_mut_ptr() as isize))).0.max(0) as usize;
            String::from_utf16_lossy(&buf[..got.min(n)])
        }
    }

    /// 文字を入れ替える(元に戻す記録は消える). 改行は`\r`にする
    pub fn set_text(&mut self, s: &str) {
        let s = normalize(s);
        let w = wide(&s);
        let st = SETTEXTEX { flags: ST_DEFAULT, codepage: 1200 };
        unsafe {
            SendMessageW(self.hwnd, EM_SETTEXTEX, Some(WPARAM(&st as *const _ as usize)), Some(LPARAM(w.as_ptr() as isize)));
        }
        self.shown = None;
        self.keys.clear();
    }

    /// 選んでいる範囲(なければカーソルの位置)を置き換える. 元に戻せる
    pub fn replace_sel(&self, s: &str) {
        let w = wide(&normalize(s));
        unsafe {
            SendMessageW(self.hwnd, EM_REPLACESEL, Some(WPARAM(1)), Some(LPARAM(w.as_ptr() as isize)));
        }
    }

    pub fn sel(&self) -> (usize, usize) {
        let mut cr = CHARRANGE::default();
        unsafe {
            SendMessageW(self.hwnd, EM_EXGETSEL, Some(WPARAM(0)), Some(LPARAM(&mut cr as *mut _ as isize)));
        }
        (cr.cpMin.max(0) as usize, cr.cpMax.max(0) as usize)
    }

    pub fn set_sel(&self, a: usize, b: usize) {
        let cr = CHARRANGE { cpMin: a as i32, cpMax: b as i32 };
        unsafe {
            SendMessageW(self.hwnd, EM_EXSETSEL, Some(WPARAM(0)), Some(LPARAM(&cr as *const _ as isize)));
        }
    }

    pub fn chars(&self) -> usize {
        self.text().chars().filter(|c| *c != '\r').count()
    }

    /// カーソルの行と, カーソルが印の中にあるか
    fn caret_state(&self, pos: usize) -> Option<(usize, bool)> {
        let i = self.lines.iter().position(|l| pos >= l.start && pos <= l.end)?;
        let l = self.lines[i];
        Some((i, pos >= l.start && pos < l.marker_end))
    }

    /// カーソルが別の行や印の中に移ったか(見せる記号を変えるため)
    pub fn caret_moved_line(&self) -> bool {
        let (a, b) = self.sel();
        a == b && self.caret_state(b) != self.shown
    }

    // ---- 整える ----

    /// 96dpiのピクセルを, RichEditのtwipにする. RichEditは自分の画面の拡大率でピクセルにするので, 窓の拡大率に合わせる
    fn twips(&self, px: f32) -> i32 {
        let px_at = px * self.dpi as f32 / 96.0;
        (px_at * 1440.0 / self.dc_dpi as f32).round() as i32
    }

    fn cf(&self) -> CHARFORMAT2W {
        CHARFORMAT2W { Base: CHARFORMATW { cbSize: std::mem::size_of::<CHARFORMAT2W>() as u32, ..Default::default() }, ..Default::default() }
    }

    fn color(&self, c: u32) -> CHARFORMAT2W {
        let mut f = self.cf();
        f.Base.dwMask = CFM_COLOR;
        f.Base.crTextColor = rgb(c);
        f
    }

    fn hidden(&self) -> CHARFORMAT2W {
        let mut f = self.cf();
        f.Base.dwMask = CFM_HIDDEN;
        f.Base.dwEffects = CFE_HIDDEN;
        f
    }

    fn face(cf: &mut CHARFORMAT2W, name: &str) {
        let w: Vec<u16> = name.encode_utf16().collect();
        let n = w.len().min(31);
        cf.Base.szFaceName = [0; 32];
        cf.Base.szFaceName[..n].copy_from_slice(&w[..n]);
        cf.Base.dwMask |= CFM_FACE;
    }

    fn apply(&self, a: usize, b: usize, cf: &CHARFORMAT2W) {
        if a >= b || self.dry.get() {
            return;
        }
        self.set_sel(a, b);
        unsafe {
            SendMessageW(self.hwnd, EM_SETCHARFORMAT, Some(WPARAM(SCF_SELECTION as usize)), Some(LPARAM(cf as *const _ as isize)));
        }
    }

    fn para(&self, at: usize, pf: &PARAFORMAT2) {
        if self.dry.get() {
            return;
        }
        self.set_sel(at, at);
        unsafe {
            SendMessageW(self.hwnd, EM_SETPARAFORMAT, Some(WPARAM(0)), Some(LPARAM(pf as *const _ as isize)));
        }
    }

    fn pf(&self) -> PARAFORMAT2 {
        let mut pf = PARAFORMAT2::default();
        pf.Base.cbSize = std::mem::size_of::<PARAFORMAT2>() as u32;
        pf.Base.dwMask = PFM_NUMBERING | PFM_STARTINDENT | PFM_OFFSET | PFM_SPACEBEFORE | PFM_SPACEAFTER | PFM_ALIGNMENT | PFM_LINESPACING;
        pf.Base.wAlignment = PFA_LEFT;
        pf.bLineSpacingRule = 5;
        pf.dyLineSpacing = LINE_SPACING;
        pf
    }

    /// ライトとダークが変わったとき(K39): 地の色を変えて, 全体を整え直す
    pub fn apply_theme(&mut self) {
        unsafe {
            SendMessageW(self.hwnd, EM_SETBKGNDCOLOR, Some(WPARAM(0)), Some(LPARAM(rgb(pal().bg).0 as isize)));
        }
        super::dark::scroll_bars(self.hwnd);
        self.restyle();
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }

    /// 全体を整えて見せる. 中の文字は変えない. 戻り値は整えにかかった時間(ミリ秒)
    pub fn restyle(&mut self) -> f64 {
        if self.ime {
            return 0.0;
        }
        let t0 = std::time::Instant::now();
        let text = self.text();
        let lines = markdown::parse(&text);
        let (s0, s1) = self.sel();
        let caret_line = lines.iter().position(|l| s1 >= l.start && s1 <= l.end());
        // カーソルがあるコードのかたまり(```から```まで)は, ```を見せる
        let mut block_of = vec![None; lines.len()];
        let mut open = None;
        for (i, l) in lines.iter().enumerate() {
            match l.block {
                Block::Fence if open.is_none() => {
                    open = Some(i);
                    block_of[i] = Some(i);
                }
                Block::Fence => {
                    block_of[i] = open;
                    open = None;
                }
                Block::Code => block_of[i] = open,
                _ => {}
            }
        }
        let caret_block = caret_line.and_then(|i| block_of[i]);
        let units: Vec<u16> = text.encode_utf16().collect();
        let state: Vec<(bool, bool, bool)> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let active = Some(i) == caret_line;
                (active, active && s0 == s1 && s1 >= l.start && s1 < l.marker_end(), block_of[i].is_some() && block_of[i] == caret_block)
            })
            .collect();
        let keys: Vec<LineKey> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                // カーソルがあるかどうかで見た目が変わるのは, 記号のある行だけ
                let cares = !l.marks.is_empty() || l.marker.is_some() || l.block != Block::Plain;
                let (active, in_marker, block_active) = state[i];
                LineKey { text: units[l.start.min(units.len())..l.end().min(units.len())].to_vec(), block: l.block, active: active && cares, in_marker, block_active, first: i == 0 }
            })
            .collect();
        let env = (self.dpi, self.hide_marks, super::theme::is_dark());
        let full = self.keys.is_empty() || env != self.env || units.is_empty();
        // 変わった行(前と後ろの同じところを除く)
        let (from, to) = if full {
            (0, lines.len())
        } else {
            let max = keys.len().min(self.keys.len());
            let p = keys.iter().zip(&self.keys).take_while(|(a, b)| a == b).count();
            let q = keys.iter().rev().zip(self.keys.iter().rev()).take(max - p).take_while(|(a, b)| a == b).count();
            let (mut f, mut t) = (p, keys.len() - q);
            // 行が増減したら, 段落の書式が隣に移ることがあるので, 前後の1行も整える
            if keys.len() != self.keys.len() {
                f = f.saturating_sub(1);
                t = (t + 1).min(keys.len());
            }
            (f, t)
        };
        if from >= to && !full {
            DECOS.with(|d| {
                let mut d = d.borrow_mut();
                d.at = s0;
                d.len = units.len();
            });
            self.keys = keys;
            self.lines = lines.iter().map(|l| LineInfo { start: l.start, end: l.end(), marker_end: l.marker_end() }).collect();
            self.shown = self.caret_state(s1).filter(|_| s0 == s1).or(caret_line.map(|i| (i, false)));
            return t0.elapsed().as_secs_f64() * 1000.0;
        }
        let m = Measure::new(self.hwnd, BODY_PX * self.dpi as i32 / 96, self.dc_dpi);
        let mut decos = Vec::new();
        let h = self.hwnd;
        unsafe {
            let shown = GetWindowLongW(h, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0;
            let mask = SendMessageW(h, EM_SETEVENTMASK, Some(WPARAM(0)), Some(LPARAM(0)));
            if let Some(d) = &self.doc {
                let _ = d.Undo(tomSuspend.0);
            }
            SendMessageW(h, WM_SETREDRAW, Some(WPARAM(0)), Some(LPARAM(0)));
            let mut scroll = POINT::default();
            SendMessageW(h, EM_GETSCROLLPOS, Some(WPARAM(0)), Some(LPARAM(&mut scroll as *mut _ as isize)));

            // 全体を普通の字に戻す(貼り付けやRichEditのキーで付いた飾りも消す)
            let mut base = self.cf();
            base.Base.dwMask = CFM_BOLD | CFM_ITALIC | CFM_STRIKEOUT | CFM_UNDERLINE | CFM_HIDDEN | CFM_COLOR | CFM_SIZE | CFM_BACKCOLOR | CFM_OFFSET;
            base.Base.dwEffects = CFE_AUTOBACKCOLOR;
            base.Base.crTextColor = rgb(pal().text);
            base.Base.yHeight = self.twips(BODY_PX as f32);
            Self::face(&mut base, body_face());
            if full {
                SendMessageW(h, EM_SETCHARFORMAT, Some(WPARAM(SCF_ALL as usize)), Some(LPARAM(&base as *const _ as isize)));
                // 空のときに打つ字も同じにする
                SendMessageW(h, EM_SETCHARFORMAT, Some(WPARAM(SCF_DEFAULT as usize)), Some(LPARAM(&base as *const _ as isize)));
                self.set_sel(0, units.len());
            } else {
                self.set_sel(lines[from].start, (lines[to - 1].end() + 1).min(units.len()));
                SendMessageW(h, EM_SETCHARFORMAT, Some(WPARAM(SCF_SELECTION as usize)), Some(LPARAM(&base as *const _ as isize)));
            }
            let pf = self.pf();
            SendMessageW(h, EM_SETPARAFORMAT, Some(WPARAM(0)), Some(LPARAM(&pf as *const _ as isize)));

            for (i, l) in lines.iter().enumerate() {
                let (active, in_marker, block_active) = state[i];
                self.dry.set(i < from || i >= to);
                self.style_line(l, &units, active, in_marker, block_active, i == 0, &m, &mut decos);
            }
            self.dry.set(false);

            self.set_sel(s0, s1);
            SendMessageW(h, EM_SETSCROLLPOS, Some(WPARAM(0)), Some(LPARAM(&scroll as *const _ as isize)));
            SendMessageW(h, WM_SETREDRAW, Some(WPARAM(1)), Some(LPARAM(0)));
            // WM_SETREDRAWは, 隠れている窓を見える状態にしてしまう(検索の窓を覆っていた)
            if !shown {
                let _ = ShowWindow(h, SW_HIDE);
            }
            if let Some(d) = &self.doc {
                let _ = d.Undo(tomResume.0);
            }
            SendMessageW(h, EM_SETEVENTMASK, Some(WPARAM(0)), Some(LPARAM(mask.0)));
        }
        let line_h = m.line_h();
        drop(m);
        let dpi = self.dpi;
        DECOS.with(|d| *d.borrow_mut() = Decos { items: decos, line_h, dpi, doc: self.doc.clone(), len: units.len(), at: s0 });
        self.keys = keys;
        self.env = env;
        unsafe {
            let _ = InvalidateRect(Some(h), None, false);
        }
        self.lines = lines.iter().map(|l| LineInfo { start: l.start, end: l.end(), marker_end: l.marker_end() }).collect();
        self.shown = self.caret_state(s1).filter(|_| s0 == s1).or(caret_line.map(|i| (i, false)));
        t0.elapsed().as_secs_f64() * 1000.0
    }

    #[allow(clippy::too_many_arguments)]
    fn style_line(&self, l: &Line, units: &[u16], active: bool, in_marker: bool, block_active: bool, first: bool, m: &Measure, decos: &mut Vec<Deco>) {
        let (a, b) = (l.start, l.end());
        let sub = |x: usize, y: usize| String::from_utf16_lossy(&units[x.min(units.len())..y.min(units.len())]);
        let hide_inline = self.hide_marks && !active;
        // 箇条書きの印を絵にするか(カーソルが印の中になければ. 記号を隠さない設定なら絵にしない)
        let picture = self.hide_marks && !in_marker;
        let mut pf = self.pf();
        let mut para = false;

        match l.block {
            Block::Heading(n) => {
                let mut c = self.cf();
                c.Base.dwMask = CFM_BOLD | CFM_SIZE;
                c.Base.dwEffects = CFE_BOLD;
                let size = BODY_PX as f32 * HEADING[(n as usize - 1).min(5)];
                c.Base.yHeight = self.twips(size);
                self.apply(a, b, &c);
                // 見出しは1行の高さ(1.2倍ほど)で, 上に少し空ける
                pf.dyLineSpacing = 20;
                if !first {
                    pf.dySpaceBefore = self.twips(size * 0.45);
                }
                para = true;
            }
            Block::Code | Block::Fence => {
                let mut c = self.cf();
                c.Base.dwMask = CFM_SIZE | CFM_BACKCOLOR | CFM_COLOR;
                c.Base.yHeight = self.twips(BODY_PX as f32 * MONO);
                c.crBackColor = rgb(pal().bg_subtle);
                c.Base.crTextColor = rgb(pal().text_secondary);
                Self::face(&mut c, mono_face());
                // 段落の区切りまで背景を付けると, 行の右端まで伸びる
                self.apply(a, b + 1, &c);
                if l.block == Block::Fence {
                    if self.hide_marks && !block_active {
                        self.apply(a, b, &self.hidden());
                    } else {
                        self.apply(a, b, &self.color(pal().faint));
                    }
                }
            }
            Block::Rule => {
                if self.hide_marks && !active {
                    self.apply(a, b, &self.color(pal().bg));
                    decos.push(Deco::Rule(a));
                } else {
                    self.apply(a, b, &self.color(pal().faint));
                }
            }
            _ => {}
        }

        if let Some((ma, mb)) = l.marker {
            if picture {
                // 頭の空白は字下げにする. 下の段の印は, 上の段の字の頭にそろえる(Obsidianと同じ)
                self.apply(a, l.lead, &self.hidden());
                let dash = m.twips("- ");
                // 2行目からは印のうしろ(字の頭)にそろえる. チェックボックスは`[ ] `の幅
                let hang_text = match (l.block, l.check) {
                    (Block::Task(_), Some((ca, _))) => sub(ca, l.marker_end()),
                    _ => sub(ma, mb),
                };
                let hang = m.twips(&hang_text);
                pf.Base.dxStartIndent = l.level as i32 * dash;
                pf.Base.dxOffset = hang;
                para = true;
                match l.block {
                    Block::Bullet => {
                        self.apply(ma, ma + 1, &self.color(pal().bg));
                        decos.push(Deco::Dot(ma));
                    }
                    Block::Task(done) => {
                        self.apply(ma, mb, &self.hidden());
                        if let Some((ca, cb)) = l.check {
                            self.apply(ca, cb, &self.color(pal().bg));
                            decos.push(Deco::Check(ca, done));
                        }
                    }
                    Block::Quote => {
                        self.apply(ma, ma + 1, &self.color(pal().bg));
                        decos.push(Deco::Bar(ma, a, b));
                    }
                    // 番号は字のまま見せる
                    _ => {}
                }
            } else {
                self.apply(ma, mb, &self.color(pal().faint));
                if let Some((ca, cb)) = l.check {
                    self.apply(ca, cb, &self.color(pal().faint));
                }
            }
            if let Block::Task(true) = l.block {
                // 済みの行は取り消し線で薄く(Obsidianと同じ)
                let mut c = self.color(pal().faint);
                c.Base.dwMask |= CFM_STRIKEOUT;
                c.Base.dwEffects = CFE_STRIKEOUT;
                self.apply(l.marker_end(), b, &c);
            }
        }
        if para {
            self.para(a, &pf);
        }

        for s in &l.spans {
            let mut c = self.cf();
            match s.style {
                Inline::Bold => {
                    c.Base.dwMask = CFM_BOLD;
                    c.Base.dwEffects = CFE_BOLD;
                }
                Inline::Italic => {
                    c.Base.dwMask = CFM_ITALIC;
                    c.Base.dwEffects = CFE_ITALIC;
                }
                Inline::Strike => {
                    c.Base.dwMask = CFM_STRIKEOUT;
                    c.Base.dwEffects = CFE_STRIKEOUT;
                }
                Inline::Code => {
                    c.Base.dwMask = CFM_SIZE | CFM_BACKCOLOR | CFM_COLOR;
                    c.Base.yHeight = self.twips(BODY_PX as f32 * MONO);
                    c.crBackColor = rgb(pal().bg_muted);
                    c.Base.crTextColor = rgb(pal().text_secondary);
                    Self::face(&mut c, mono_face());
                }
                Inline::Highlight => {
                    c.Base.dwMask = CFM_BACKCOLOR;
                    c.crBackColor = rgb(pal().highlight);
                }
                Inline::Link | Inline::Url => {
                    c.Base.dwMask = CFM_UNDERLINE | CFM_COLOR;
                    c.Base.dwEffects = CFE_UNDERLINE;
                    c.Base.crTextColor = rgb(pal().accent_text);
                }
            }
            self.apply(s.start, s.end, &c);
        }
        for (ma, mb) in &l.marks {
            if hide_inline {
                self.apply(*ma, *mb, &self.hidden());
            } else {
                let mut c = self.color(pal().faint);
                c.Base.dwMask |= CFM_BOLD | CFM_ITALIC | CFM_STRIKEOUT | CFM_UNDERLINE | CFM_BACKCOLOR;
                c.Base.dwEffects = CFE_AUTOBACKCOLOR;
                self.apply(*ma, *mb, &c);
            }
        }
    }

    /// 中身の高さを聞く(EN_REQUESTRESIZEで親の窓に届く)
    pub fn request_resize(&self) {
        unsafe {
            SendMessageW(self.hwnd, EM_REQUESTRESIZE, Some(WPARAM(0)), Some(LPARAM(0)));
        }
    }

    pub fn set_font_dpi(&mut self, dpi: u32) {
        self.dpi = dpi;
    }
}

// ---- 上から描く絵 ----

fn pos(hwnd: HWND, i: usize) -> (i32, i32) {
    let mut p = POINTL::default();
    unsafe {
        SendMessageW(hwnd, EM_POSFROMCHAR, Some(WPARAM(&mut p as *mut _ as usize)), Some(LPARAM(i as isize)));
    }
    (p.x, p.y)
}

/// その字の下端の線(ベースライン)の高さ. 聞けなければNone
fn baseline(doc: &Option<ITextDocument>, i: usize) -> Option<i32> {
    let kind = tomConstants(tomStart.0 | tomClientCoord.0 | tomAllowOffClient.0 | TA_BASELINE.0 as i32);
    unsafe {
        let r = doc.as_ref()?.Range(i as i32, i as i32 + 1).ok()?;
        let (mut x, mut y) = (0i32, 0i32);
        r.GetPoint(kind, &mut x, &mut y).ok()?;
        Some(y)
    }
}

/// 中の文字の数(段落の区切りは1つ)
fn text_len(hwnd: HWND) -> usize {
    let g = GETTEXTLENGTHEX { flags: GTL_PRECISE | GTL_NUMCHARS, codepage: 1200 };
    unsafe { SendMessageW(hwnd, EM_GETTEXTLENGTHEX, Some(WPARAM(&g as *const _ as usize)), Some(LPARAM(0))).0.max(0) as usize }
}

/// その字がある画面の行の高さ(次の行の頭との差. 最後の行は`fallback`)
fn line_h(hwnd: HWND, i: usize, fallback: i32) -> i32 {
    unsafe {
        let line = SendMessageW(hwnd, EM_EXLINEFROMCHAR, Some(WPARAM(0)), Some(LPARAM(i as isize))).0;
        let first = SendMessageW(hwnd, EM_LINEINDEX, Some(WPARAM(line as usize)), Some(LPARAM(0))).0;
        let next = SendMessageW(hwnd, EM_LINEINDEX, Some(WPARAM((line + 1) as usize)), Some(LPARAM(0))).0;
        if first >= 0 && next > first {
            let h = pos(hwnd, next as usize).1 - pos(hwnd, first as usize).1;
            if h > 0 {
                return h;
            }
        }
    }
    fallback
}

fn solid(dc: HDC, rc: &RECT, color: u32) {
    unsafe {
        let b = CreateSolidBrush(rgb(color));
        FillRect(dc, rc, b);
        let _ = DeleteObject(b.into());
    }
}

/// RichEditが描いたあとに, 箇条書きの「•」, 引用の縦線, チェックボックス, 区切り線を描く(WM_PAINTのあと)
pub fn paint_decos(hwnd: HWND) {
    DECOS.with(|d| {
        let d = d.borrow();
        if d.items.is_empty() {
            return;
        }
        let s = |v: f32| ((v * d.dpi as f32 / 96.0).round() as i32).max(1);
        unsafe {
            let dc = GetDC(Some(hwnd));
            let _ = HideCaret(Some(hwnd));
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            // RichEditのDCは親の窓まで描けてしまう(CS_PARENTDC)ので, 入力欄の中に限る(K50)
            let saved = SaveDC(dc);
            let _ = IntersectClipRect(dc, client.left, client.top, client.right, client.bottom);
            let visible = |top: i32, bottom: i32| bottom >= client.top && top <= client.bottom;
            let diff = text_len(hwnd) as isize - d.len as isize;
            let sh = |i: usize| if i > d.at { (i as isize + diff).max(0) as usize } else { i };
            // 字の真ん中の高さ. 行の高さは, 最後の行では測れず改行の前後で変わるので, 字の下端の線から決める
            let mid = |i: usize| baseline(&d.doc, i).map(|b| b - s(BODY_PX as f32 * 0.36));
            for item in &d.items {
                let item = match *item {
                    Deco::Dot(i) => Deco::Dot(sh(i)),
                    Deco::Bar(i, a, b) => Deco::Bar(sh(i), sh(a), sh(b)),
                    Deco::Check(i, v) => Deco::Check(sh(i), v),
                    Deco::Rule(i) => Deco::Rule(sh(i)),
                };
                match item {
                    Deco::Dot(i) => {
                        let (x0, y) = pos(hwnd, i);
                        let x1 = pos(hwnd, i + 1).0;
                        let h = line_h(hwnd, i, d.line_h);
                        if !visible(y, y + h) {
                            continue;
                        }
                        // Obsidianの「•」は本文の字の0.3倍ほどの丸
                        let r = s(BODY_PX as f32 * 0.15);
                        let (cx, cy) = ((x0 + x1) / 2, mid(i).unwrap_or(y + h / 2));
                        let br = CreateSolidBrush(rgb(pal().text_tertiary));
                        let pen = CreatePen(PS_SOLID, 1, rgb(pal().text_tertiary));
                        let ob = SelectObject(dc, br.into());
                        let op = SelectObject(dc, pen.into());
                        let _ = Ellipse(dc, cx - r, cy - r, cx + r + 1, cy + r + 1);
                        SelectObject(dc, ob);
                        SelectObject(dc, op);
                        let _ = DeleteObject(br.into());
                        let _ = DeleteObject(pen.into());
                    }
                    Deco::Bar(i, from, to) => {
                        let (x, top) = (pos(hwnd, i).0, pos(hwnd, from).1);
                        let last = pos(hwnd, to).1;
                        let bottom = last + line_h(hwnd, to, d.line_h);
                        // 最後の行の高さは推定なので, 字の下端から少し下までにする
                        let bottom = match baseline(&d.doc, i) {
                            Some(b) if last == top && text_len(hwnd) <= to => b + s(BODY_PX as f32 * 0.4),
                            _ => bottom,
                        };
                        if !visible(top, bottom) {
                            continue;
                        }
                        solid(dc, &RECT { left: x + s(1.0), top, right: x + s(1.0) + s(3.0), bottom }, pal().accent);
                    }
                    Deco::Check(i, done) => {
                        let (x0, y) = pos(hwnd, i);
                        let x1 = pos(hwnd, i + 3).0;
                        let h = line_h(hwnd, i, d.line_h);
                        if !visible(y, y + h) {
                            continue;
                        }
                        let size = s(BODY_PX as f32 * 0.8).min((x1 - x0).max(s(10.0)));
                        // 左端を, 引用の縦線や「•」の行の頭にそろえる
                        let (l, t) = (x0 + s(1.0), mid(i).map_or(y + (h - size) / 2, |m| m - size / 2));
                        let rr = s(3.0);
                        let (fillc, edge) = if done { (pal().accent, pal().accent) } else { (pal().bg, pal().text_tertiary) };
                        let br = CreateSolidBrush(rgb(fillc));
                        let pen = CreatePen(PS_SOLID, s(1.0), rgb(edge));
                        let ob = SelectObject(dc, br.into());
                        let op = SelectObject(dc, pen.into());
                        let _ = RoundRect(dc, l, t, l + size, t + size, rr * 2, rr * 2);
                        SelectObject(dc, ob);
                        SelectObject(dc, op);
                        let _ = DeleteObject(br.into());
                        let _ = DeleteObject(pen.into());
                        if done {
                            let pen = CreatePen(PS_SOLID, s(2.0), rgb(pal().bg));
                            let op = SelectObject(dc, pen.into());
                            let pts = [
                                POINT { x: l + size * 22 / 100, y: t + size * 52 / 100 },
                                POINT { x: l + size * 42 / 100, y: t + size * 72 / 100 },
                                POINT { x: l + size * 78 / 100, y: t + size * 30 / 100 },
                            ];
                            let _ = Polyline(dc, &pts);
                            SelectObject(dc, op);
                            let _ = DeleteObject(pen.into());
                        }
                    }
                    Deco::Rule(i) => {
                        let (_, y) = pos(hwnd, i);
                        let h = line_h(hwnd, i, d.line_h);
                        if !visible(y, y + h) {
                            continue;
                        }
                        let t = s(2.0);
                        let cy = y + h / 2 - t / 2;
                        solid(dc, &RECT { left: client.left, top: cy, right: client.right, bottom: cy + t }, pal().border);
                    }
                }
            }
            let _ = RestoreDC(dc, saved);
            let _ = ShowCaret(Some(hwnd));
            ReleaseDC(Some(hwnd), dc);
        }
    });
}

/// 改行を`\r`にそろえる(RichEditの段落の区切り)
pub fn normalize(s: &str) -> String {
    s.replace("\r\n", "\r").replace('\n', "\r")
}
