//! クリップボードの読み書き(文字とファイル). 履歴の記録(`WM_CLIPBOARDUPDATE`), 選んだ文字の取り込み(Ctrl+C),
//! ファイルのコピー(Explorerと同じ形)に使う. 履歴の持ち方と絞り込みは`hikidashi_core::clip`.

use super::util::{wide, WStr};
use hikidashi_core::attach::{self, Image, Paste};
use hikidashi_core::clip::{ClipKind, MAX_TEXT_BYTES};
use std::sync::OnceLock;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, CountClipboardFormats, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT};
use windows::Win32::System::Ole::{CF_BITMAP, CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

const CF_TEXT_ID: u32 = CF_UNICODETEXT.0 as u32;
const CF_HDROP_ID: u32 = CF_HDROP.0 as u32;
const CF_DIB_ID: u32 = CF_DIB.0 as u32;
/// 添付する画像1枚の大きさの上限(バイト. 4Kの画面の切り取りは32MBほど)
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// `DROPEFFECT_COPY`
const DROPEFFECT_COPY: u32 = 1;

/// 現在のクリップボードの更新の通し番号
pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// 開く. ほかのアプリが使っている間は失敗するので, 少し待って数回やり直す
fn open(hwnd: HWND) -> bool {
    for _ in 0..10 {
        if unsafe { OpenClipboard(Some(hwnd)) }.is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    false
}

struct Formats {
    /// パスワード管理のアプリなどが付ける印(記録しない)
    exclude_monitor: u32,
    viewer_ignore: u32,
    /// 値が0なら記録しない
    can_include: u32,
    drop_effect: u32,
    /// 画像のPNG(Snipping Toolやブラウザーが付ける. ブラウザーは貼るときにこれを先に読む)
    png: u32,
}

fn formats() -> &'static Formats {
    static F: OnceLock<Formats> = OnceLock::new();
    F.get_or_init(|| {
        let reg = |n: &str| {
            let w = WStr::new(n);
            unsafe { RegisterClipboardFormatW(w.pcwstr()) }
        };
        Formats {
            exclude_monitor: reg("ExcludeClipboardContentFromMonitorProcessing"),
            viewer_ignore: reg("Clipboard Viewer Ignore"),
            can_include: reg("CanIncludeInClipboardHistory"),
            drop_effect: reg("Preferred DropEffect"),
            png: reg("PNG"),
        }
    })
}

fn available(fmt: u32) -> bool {
    fmt != 0 && unsafe { IsClipboardFormatAvailable(fmt) }.is_ok()
}

/// 開いている間に呼ぶ. 記録してはいけない印があるか
fn excluded_open() -> bool {
    let f = formats();
    if available(f.exclude_monitor) || available(f.viewer_ignore) {
        return true;
    }
    if available(f.can_include) {
        unsafe {
            if let Ok(h) = GetClipboardData(f.can_include) {
                let hg = HGLOBAL(h.0);
                if GlobalSize(hg) >= 4 {
                    let p = GlobalLock(hg) as *const u32;
                    if !p.is_null() {
                        let v = std::ptr::read_unaligned(p);
                        let _ = GlobalUnlock(hg);
                        return v == 0;
                    }
                }
            }
        }
    }
    false
}

/// 開いている間に呼ぶ. 文字(CF_UNICODETEXT). 大きすぎれば`Err(())`
fn text_open() -> Option<Result<String, ()>> {
    unsafe {
        if !available(CF_TEXT_ID) {
            return None;
        }
        let h = GetClipboardData(CF_TEXT_ID).ok()?;
        let hg = HGLOBAL(h.0);
        // UTF-16なので, バイト数の半分が文字数. 大きすぎるものは読まない
        if GlobalSize(hg) > MAX_TEXT_BYTES * 2 + 2 {
            return Some(Err(()));
        }
        let p = GlobalLock(hg) as *const u16;
        if p.is_null() {
            return None;
        }
        let max = GlobalSize(hg) / 2;
        let mut n = 0usize;
        while n < max && *p.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
        let _ = GlobalUnlock(hg);
        Some(Ok(s))
    }
}

/// 開いている間に呼ぶ. ファイル(CF_HDROP)のパス
fn files_open() -> Option<Vec<String>> {
    unsafe {
        if !available(CF_HDROP_ID) {
            return None;
        }
        let h = GetClipboardData(CF_HDROP_ID).ok()?;
        Some(drop_paths(HDROP(h.0)))
    }
}

/// `HDROP`(クリップボードのファイルや, 窓に落としたファイル)のパス
pub fn drop_paths(hd: HDROP) -> Vec<String> {
    unsafe {
        let n = DragQueryFileW(hd, u32::MAX, None);
        let mut out = vec![];
        for i in 0..n.min(1000) {
            let len = DragQueryFileW(hd, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            let got = DragQueryFileW(hd, i, Some(&mut buf)) as usize;
            out.push(String::from_utf16_lossy(&buf[..got.min(buf.len())]));
        }
        out
    }
}

/// 開いている間に呼ぶ. その形式の中身(大きすぎるものは読まない)
fn bytes_open(fmt: u32, max: usize) -> Option<Vec<u8>> {
    unsafe {
        if !available(fmt) {
            return None;
        }
        let h = GetClipboardData(fmt).ok()?;
        let hg = HGLOBAL(h.0);
        let size = GlobalSize(hg);
        if size == 0 || size > max {
            return None;
        }
        let p = GlobalLock(hg) as *const u8;
        if p.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(p, size).to_vec();
        let _ = GlobalUnlock(hg);
        Some(v)
    }
}

/// 書く窓での貼り付けで何をするか(文字, 画像, ファイルの有無から決める)
pub fn paste_kind(hwnd: HWND) -> Paste {
    // 画像はCF_BITMAPやCF_DIBV5だけでも, Windowsが自動でCF_DIBにして渡す
    let has_image = [CF_DIB_ID, CF_DIBV5.0 as u32, CF_BITMAP.0 as u32, formats().png].iter().any(|f| available(*f));
    let has_text = available(CF_TEXT_ID);
    let files = if available(CF_HDROP_ID) && open(hwnd) {
        let f = files_open().unwrap_or_default();
        unsafe {
            let _ = CloseClipboard();
        }
        f
    } else {
        vec![]
    };
    attach::paste_kind(has_text, has_image, &files)
}

/// クリップボードの画像を, 添付の形(CF_DIBと, あればPNG)で読む. 読めなければ`None`
pub fn read_image(hwnd: HWND) -> Option<Image> {
    if !open(hwnd) {
        return None;
    }
    let png = bytes_open(formats().png, MAX_IMAGE_BYTES).filter(|b| attach::png_size(b).is_some());
    let dib = bytes_open(CF_DIB_ID, MAX_IMAGE_BYTES).filter(|b| attach::dib_size(b).is_some());
    unsafe {
        let _ = CloseClipboard();
    }
    let (width, height) = dib.as_deref().and_then(attach::dib_size).or_else(|| png.as_deref().and_then(attach::png_size))?;
    Some(Image { width, height, dib, png })
}

/// 添付の画像をクリップボードに入れる(CF_DIBと, あればPNG). 入れたあとの通し番号を返す. 裏のスレッドからも呼ぶ
pub fn set_image(hwnd: HWND, img: &Image) -> Option<u32> {
    if !open(hwnd) {
        return None;
    }
    let ok = unsafe {
        let _ = EmptyClipboard();
        let a = img.dib.as_deref().is_some_and(|b| set_open(CF_DIB_ID, b));
        let b = img.png.as_deref().is_some_and(|b| set_open(formats().png, b));
        let _ = CloseClipboard();
        a || b
    };
    ok.then(sequence)
}

/// 履歴に記録する今の中身. Explorerでコピーしたファイルならパス, それ以外は文字. 画像だけのもの,
/// 記録してはいけない印があるもの, 大きすぎる文字は`None`
pub fn read_for_history(hwnd: HWND) -> Option<ClipKind> {
    if !open(hwnd) {
        return None;
    }
    let r = (|| {
        if excluded_open() {
            return None;
        }
        if let Some(f) = files_open() {
            return (!f.is_empty()).then_some(ClipKind::Files(f));
        }
        match text_open()? {
            Ok(t) if t.len() <= MAX_TEXT_BYTES => Some(ClipKind::Text(t)),
            _ => None,
        }
    })();
    unsafe {
        let _ = CloseClipboard();
    }
    r
}

/// 今のクリップボードの文字(なければ`None`)
pub fn read_text(hwnd: HWND) -> Option<String> {
    if !open(hwnd) {
        return None;
    }
    let r = text_open().and_then(|r| r.ok());
    unsafe {
        let _ = CloseClipboard();
    }
    r
}

/// 退避できる量の上限(バイト)
const MAX_SAVE_BYTES: usize = 32 * 1024 * 1024;

/// 退避したクリップボード(HGLOBALで持てる形式を, 形式の番号とバイト列で)
pub struct Saved {
    formats: Vec<(u32, Vec<u8>)>,
    /// 何も入っていなかった
    empty: bool,
    /// 写しきれない(32MBを超える, 写せる形式がない). 読み取りをせず, 元の中身を壊さない
    pub abort: bool,
}

/// GDIのハンドルで持つ形式(写せない). CF_BITMAP, CF_METAFILEPICT, CF_PALETTE, CF_ENHMETAFILE, CF_DSP*, GDIオブジェクトの範囲
fn is_gdi_format(f: u32) -> bool {
    matches!(f, 2 | 3 | 9 | 14 | 0x80..=0x8E | 0x0300..=0x03FF)
}

/// OLEが内部で使う形式(中身はそのアプリの中だけで意味がある. 戻すと, 貼る側がおかしくなる)
fn is_ole_private(f: u32) -> bool {
    static IDS: OnceLock<[u32; 2]> = OnceLock::new();
    IDS.get_or_init(|| {
        ["DataObject", "Ole Private Data"].map(|n| {
            let w = WStr::new(n);
            unsafe { RegisterClipboardFormatW(w.pcwstr()) }
        })
    })
    .contains(&f)
}

/// 今のクリップボードの形式をすべて写して退避する. 開けなければ`None`(あとで戻さない)
pub fn save(hwnd: HWND) -> Option<Saved> {
    if !open(hwnd) {
        return None;
    }
    let mut s = Saved { formats: vec![], empty: unsafe { CountClipboardFormats() } == 0, abort: false };
    unsafe {
        let mut list = vec![];
        let mut f = EnumClipboardFormats(0);
        while f != 0 {
            list.push(f);
            f = EnumClipboardFormats(f);
        }
        let has_text = list.contains(&CF_TEXT_ID);
        let mut total = 0usize;
        for f in list {
            // 文字から作られる形式(CF_TEXT, CF_OEMTEXT, CF_LOCALE)は, 文字を戻せば自動で付く
            if is_gdi_format(f) || is_ole_private(f) || (has_text && matches!(f, 1 | 7 | 16)) {
                continue;
            }
            let Ok(h) = GetClipboardData(f) else { continue };
            let hg = HGLOBAL(h.0);
            let size = GlobalSize(hg);
            total += size;
            if total > MAX_SAVE_BYTES {
                s.abort = true;
                break;
            }
            let p = GlobalLock(hg) as *const u8;
            if p.is_null() {
                continue;
            }
            s.formats.push((f, std::slice::from_raw_parts(p, size).to_vec()));
            let _ = GlobalUnlock(hg);
        }
        // 入っているのに, 1つも写せなかった(GDIのハンドルだけなど)
        if !s.empty && s.formats.is_empty() {
            s.abort = true;
        }
        let _ = CloseClipboard();
    }
    Some(s)
}

/// 退避したものを戻す. 戻したあとの通し番号を返す(履歴に残さない目印)
pub fn restore(hwnd: HWND, s: &Saved) -> u32 {
    if s.formats.is_empty() {
        // 空だったなら空に戻す(Ctrl+Cで入った文字を残さない)
        if s.empty {
            let _ = empty(hwnd);
        }
        return sequence();
    }
    if open(hwnd) {
        unsafe {
            let _ = EmptyClipboard();
            for (f, bytes) in &s.formats {
                set_open(*f, bytes);
            }
            let _ = CloseClipboard();
        }
    }
    sequence()
}

fn empty(hwnd: HWND) -> Result<(), String> {
    if !open(hwnd) {
        return Err("The clipboard is in use".into());
    }
    unsafe {
        let _ = EmptyClipboard();
        let _ = CloseClipboard();
    }
    Ok(())
}

/// グローバルメモリに書いて渡せる形にする
unsafe fn alloc(bytes: &[u8]) -> Option<HGLOBAL> {
    let h = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1)).ok()?;
    let p = GlobalLock(h) as *mut u8;
    if p.is_null() {
        let _ = GlobalFree(Some(h));
        return None;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    let _ = GlobalUnlock(h);
    Some(h)
}

/// 開いている間に呼ぶ. 渡せなければメモリを解放する
unsafe fn set_open(fmt: u32, bytes: &[u8]) -> bool {
    let Some(h) = alloc(bytes) else { return false };
    if SetClipboardData(fmt, Some(HANDLE(h.0))).is_err() {
        let _ = GlobalFree(Some(h));
        return false;
    }
    true
}

pub fn set_text(hwnd: HWND, text: &str) -> Result<(), String> {
    if !open(hwnd) {
        return Err("The clipboard is in use by another app".into());
    }
    let w = wide(text);
    let bytes: Vec<u8> = w.iter().flat_map(|c| c.to_le_bytes()).collect();
    let ok = unsafe {
        let _ = EmptyClipboard();
        let ok = set_open(CF_TEXT_ID, &bytes);
        let _ = CloseClipboard();
        ok
    };
    if ok { Ok(()) } else { Err("Could not copy to the clipboard".into()) }
}

/// `DROPFILES`(20バイト)と, 0で区切って2つの0で終わるUTF-16のパスの並び
fn drop_files(paths: &[String]) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    // pFiles(中身の場所), pt.x, pt.y, fNC, fWide
    for v in [20u32, 0, 0, 0, 1] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for p in paths {
        for c in p.encode_utf16() {
            b.extend_from_slice(&c.to_le_bytes());
        }
        b.extend_from_slice(&0u16.to_le_bytes());
    }
    b.extend_from_slice(&0u16.to_le_bytes());
    b
}

/// ファイルをExplorerでCtrl+Cしたのと同じ形(CF_HDROPと, コピーの`Preferred DropEffect`)でクリップボードに入れる
pub fn set_files(hwnd: HWND, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    if !open(hwnd) {
        return Err("The clipboard is in use by another app".into());
    }
    let data = drop_files(paths);
    let ok = unsafe {
        let _ = EmptyClipboard();
        let ok = set_open(CF_HDROP_ID, &data);
        if ok {
            set_open(formats().drop_effect, &DROPEFFECT_COPY.to_le_bytes());
        }
        let _ = CloseClipboard();
        ok
    };
    if ok { Ok(()) } else { Err("Could not copy the file to the clipboard".into()) }
}

/// 履歴の中身をクリップボードに入れる
pub fn set_kind(hwnd: HWND, k: &ClipKind) -> Result<(), String> {
    match k {
        ClipKind::Text(t) => set_text(hwnd, t),
        ClipKind::Files(f) => set_files(hwnd, f),
    }
}
