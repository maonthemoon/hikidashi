//! 文字列の変換などの小さな道具.

use windows::core::PCWSTR;

/// Rustの文字列を, 末尾に0を付けたUTF-16にする.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `wide`の結果を一時的にPCWSTRとして使う
pub struct WStr(Vec<u16>);

impl WStr {
    pub fn new(s: &str) -> WStr {
        WStr(wide(s))
    }
    pub fn pcwstr(&self) -> PCWSTR {
        PCWSTR(self.0.as_ptr())
    }
}

/// UTF-16の配列(0で終わるか, 長さまで)を文字列にする
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// 今のUNIX時刻(秒)
pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// 色(0xRRGGBB)をGDIのCOLORREF(0x00BBGGRR)にする
pub const fn rgb(c: u32) -> windows::Win32::Foundation::COLORREF {
    let r = (c >> 16) & 0xFF;
    let g = (c >> 8) & 0xFF;
    let b = c & 0xFF;
    windows::Win32::Foundation::COLORREF(r | (g << 8) | (b << 16))
}

/// 2つの色を混ぜる(t=0でa, t=1でb)
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| -> u32 {
        let x = ((a >> s) & 0xFF) as f32;
        let y = ((b >> s) & 0xFF) as f32;
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// 下位と上位の16ビット
pub fn loword(v: usize) -> u16 {
    (v & 0xFFFF) as u16
}
pub fn hiword(v: usize) -> u16 {
    ((v >> 16) & 0xFFFF) as u16
}
