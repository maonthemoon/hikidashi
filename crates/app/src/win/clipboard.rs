//! クリップボードの読み書き(文字とファイル). 履歴の記録(`WM_CLIPBOARDUPDATE`), 選んだ文字の取り込み(Ctrl+C),
//! ファイルのコピー(Explorerと同じ形)に使う. 履歴の持ち方と絞り込みは`hikidashi_core::clip`.

use super::util::{wide, WStr};
use hikidashi_core::clip::{ClipKind, MAX_TEXT_BYTES};
use std::sync::OnceLock;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, CountClipboardFormats, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT};
use windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

const CF_TEXT_ID: u32 = CF_UNICODETEXT.0 as u32;
const CF_HDROP_ID: u32 = CF_HDROP.0 as u32;
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
        let hd = HDROP(h.0);
        let n = DragQueryFileW(hd, u32::MAX, None);
        let mut out = vec![];
        for i in 0..n.min(1000) {
            let len = DragQueryFileW(hd, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            let got = DragQueryFileW(hd, i, Some(&mut buf)) as usize;
            out.push(String::from_utf16_lossy(&buf[..got.min(buf.len())]));
        }
        Some(out)
    }
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

/// 退避したクリップボード(文字とファイルだけ)
pub struct Saved {
    text: Option<String>,
    files: Option<Vec<String>>,
    /// 何も入っていなかった
    empty: bool,
}

/// 今の文字とファイルを退避する. 開けなければ`None`(あとで戻さない)
pub fn save(hwnd: HWND) -> Option<Saved> {
    if !open(hwnd) {
        return None;
    }
    let empty = unsafe { CountClipboardFormats() } == 0;
    let text = text_open().and_then(|r| r.ok());
    let files = files_open();
    unsafe {
        let _ = CloseClipboard();
    }
    Some(Saved { text, files, empty })
}

/// 退避したものを戻す. 戻したあとの通し番号を返す(履歴に残さない目印)
pub fn restore(hwnd: HWND, s: &Saved) -> u32 {
    let _ = match (&s.files, &s.text) {
        (Some(f), _) => set_files(hwnd, f),
        (None, Some(t)) => set_text(hwnd, t),
        // 退避するものがなかった. 空だったなら空に戻す(Ctrl+Cで入った文字を残さない)
        (None, None) if s.empty => empty(hwnd),
        (None, None) => Ok(()),
    };
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
