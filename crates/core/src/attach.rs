//! 長い質問を書く窓(K22)に添付する画像. 貼り付けで何をするかの判定, 下の帯の文字, 画像の大きさの読み取り,
//! ChatGPTに送るときの開き方と待ち時間. クリップボードやWICは使わない(`crates/app`が使う).

/// 添付できる画像の数
pub const MAX_IMAGES: usize = 4;

/// 添付にするファイルの拡張子(小文字)
pub const IMAGE_EXTS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

/// 下の帯の, 外す方法の案内
pub const REMOVE_HINT: &str = "Ctrl+Shift+Backspace to remove";

/// 送ったときにChatGPTの窓にならず, 画像を貼れなかったときの知らせ
pub const NOT_ATTACHED: &str = "Couldn't attach the image. It is on the clipboard.";

/// 添付した画像. クリップボードに入れるときの形で持つ(どちらか一方はある)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// `CF_DIB`の中身(`BITMAPINFOHEADER`から)
    pub dib: Option<Vec<u8>>,
    /// 登録形式`PNG`の中身(PNGのファイルのバイト列)
    pub png: Option<Vec<u8>>,
}

/// 画像として添付するファイルか(拡張子で決める)
pub fn is_image_path(p: &str) -> bool {
    let name = p.rsplit(['\\', '/']).next().unwrap_or(p);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str()),
        _ => false,
    }
}

/// 書く窓での貼り付け(`Ctrl+V`, `Shift+Insert`)で何をするか
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Paste {
    /// 画像のファイルを添付する(画像でないファイルは除いたもの)
    Files(Vec<String>),
    /// クリップボードの画像を添付する
    Image,
    /// 今まで通り文字を貼る(文字がなければ何もしない)
    Text,
}

/// 貼り付けで何をするか決める. ファイル(`CF_HDROP`)の画像を先に見て, 次に文字, 文字がなければ画像.
/// 文字と画像が両方あるもの(Officeでコピーした表など)は文字を貼る
pub fn paste_kind(has_text: bool, has_image: bool, files: &[String]) -> Paste {
    let imgs: Vec<String> = files.iter().filter(|f| is_image_path(f)).cloned().collect();
    if !imgs.is_empty() {
        Paste::Files(imgs)
    } else if !has_text && has_image {
        Paste::Image
    } else {
        Paste::Text
    }
}

/// あと何枚添付できるか
pub fn room(have: usize) -> usize {
    MAX_IMAGES.saturating_sub(have)
}

/// 下の帯の左に出す文字. 1枚なら`Image 1280×720`, 2枚以上なら`2 images`. なければNone
pub fn label(sizes: &[(u32, u32)]) -> Option<String> {
    match sizes {
        [] => None,
        [(w, h)] => Some(format!("Image {w}×{h}")),
        _ => Some(format!("{} images", sizes.len())),
    }
}

/// 画像を貼ったあと, Enterを押すまで待つ時間(1枚ごとに`per_ms`を足す)
pub fn wait_ms(images: usize, per_ms: u32) -> u64 {
    images as u64 * per_ms as u64
}

/// 画像つきで送るときの, 文字の入れ方
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fill {
    /// `?q=`に入れて開く(符号にした文字)
    Url(String),
    /// 長いので`?q=.`で開き, `.`を消して貼る
    Paste,
    /// 文字がない. `?q=.`で開き, `.`を消すだけ(`?q=`なしで開くと入力欄にカーソルがなく, 貼り付けが空振りする. K22)
    Dot,
}

/// 文字の入れ方を決める. `encoded`は`?q=`に入れる形にした文字, `max`はその長さの上限
pub fn fill(text: &str, encoded: String, max: usize) -> Fill {
    if text.trim().is_empty() {
        Fill::Dot
    } else if encoded.len() <= max {
        Fill::Url(encoded)
    } else {
        Fill::Paste
    }
}

/// PNGの大きさ(IHDRから)
pub fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    Some((be(16), be(20)))
}

/// `CF_DIB`(`BITMAPINFOHEADER`か`BITMAPCOREHEADER`で始まる)の大きさ. 上から下に並ぶものは高さが負なので, 絶対値にする
pub fn dib_size(b: &[u8]) -> Option<(u32, u32)> {
    let size = u32::from_le_bytes(b.get(..4)?.try_into().ok()?);
    if size == 12 {
        let w = u16::from_le_bytes(b.get(4..6)?.try_into().ok()?);
        let h = u16::from_le_bytes(b.get(6..8)?.try_into().ok()?);
        return Some((w as u32, h as u32));
    }
    if size < 40 {
        return None;
    }
    let w = i32::from_le_bytes(b.get(4..8)?.try_into().ok()?);
    let h = i32::from_le_bytes(b.get(8..12)?.try_into().ok()?);
    (w > 0 && h != 0).then(|| (w as u32, h.unsigned_abs()))
}

/// 上の行から並んだ32bitの画素(B, G, R, A)を, `CF_DIB`の形(`BITMAPINFOHEADER`と, 下の行から並んだ画素)にする
pub fn dib_from_bgra(width: u32, height: u32, pixels: &[u8]) -> Option<Vec<u8>> {
    let stride = width as usize * 4;
    let bytes = stride.checked_mul(height as usize)?;
    if width == 0 || height == 0 || pixels.len() < bytes || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return None;
    }
    let mut out = Vec::with_capacity(40 + bytes);
    // biSize, biWidth, biHeight(正なので下の行から), biPlanes, biBitCount, biCompression(BI_RGB), biSizeImage,
    // biXPelsPerMeter, biYPelsPerMeter, biClrUsed, biClrImportant
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(bytes as u32).to_le_bytes());
    for _ in 0..4 {
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    for row in pixels[..bytes].chunks_exact(stride).rev() {
        out.extend_from_slice(row);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_paths() {
        assert!(is_image_path(r"C:\a\b.PNG"));
        assert!(is_image_path(r"C:\a\写真.jpeg"));
        assert!(is_image_path("x.webp") && is_image_path("x.gif") && is_image_path("x.jpg"));
        assert!(!is_image_path(r"C:\a\b.pdf"));
        assert!(!is_image_path(r"C:\a.png\b"));
        assert!(!is_image_path(r"C:\a\.png"));
        assert!(!is_image_path("png"));
    }

    #[test]
    fn what_to_paste() {
        let files = vec![r"C:\a.txt".to_string(), r"C:\b.png".to_string()];
        assert_eq!(paste_kind(false, false, &files), Paste::Files(vec![r"C:\b.png".into()]));
        // 画像のファイルは, 文字があっても添付
        assert_eq!(paste_kind(true, true, &files), Paste::Files(vec![r"C:\b.png".into()]));
        // 画像でないファイルだけなら今まで通り
        assert_eq!(paste_kind(false, false, &files[..1]), Paste::Text);
        // 文字と画像が両方あれば文字
        assert_eq!(paste_kind(true, true, &[]), Paste::Text);
        assert_eq!(paste_kind(false, true, &[]), Paste::Image);
        assert_eq!(paste_kind(true, false, &[]), Paste::Text);
        assert_eq!(paste_kind(false, false, &[]), Paste::Text);
    }

    #[test]
    fn labels_and_waits() {
        assert_eq!(label(&[]), None);
        assert_eq!(label(&[(1280, 720)]).as_deref(), Some("Image 1280×720"));
        assert_eq!(label(&[(1, 2), (3, 4)]).as_deref(), Some("2 images"));
        assert_eq!(room(0), 4);
        assert_eq!(room(3), 1);
        assert_eq!(room(5), 0);
        assert_eq!(wait_ms(0, 3000), 0);
        assert_eq!(wait_ms(3, 3000), 9000);
        assert_eq!(wait_ms(4, u32::MAX), 4 * u32::MAX as u64);
    }

    #[test]
    fn how_to_fill() {
        assert_eq!(fill("  \n", String::new(), 10), Fill::Dot);
        assert_eq!(fill("abc", "abc".into(), 10), Fill::Url("abc".into()));
        assert_eq!(fill("abc", "%E3%81%82".into(), 5), Fill::Paste);
    }

    #[test]
    fn sizes() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&1280u32.to_be_bytes());
        png.extend_from_slice(&720u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((1280, 720)));
        assert_eq!(png_size(&png[..20]), None);
        assert_eq!(png_size(b"GIF89a................................"), None);
        let dib = dib_from_bgra(3, 2, &[0u8; 24]).unwrap();
        assert_eq!(dib_size(&dib), Some((3, 2)));
        // 上から下に並ぶもの(高さが負)
        let mut td = dib.clone();
        td[8..12].copy_from_slice(&(-2i32).to_le_bytes());
        assert_eq!(dib_size(&td), Some((3, 2)));
        let mut core = vec![12, 0, 0, 0];
        core.extend_from_slice(&5u16.to_le_bytes());
        core.extend_from_slice(&6u16.to_le_bytes());
        assert_eq!(dib_size(&core), Some((5, 6)));
        assert_eq!(dib_size(&[1, 2]), None);
    }

    #[test]
    fn dib_rows_are_flipped() {
        // 上の行が赤, 下の行が青(2×2)
        let red = [0u8, 0, 255, 255];
        let blue = [255u8, 0, 0, 255];
        let px: Vec<u8> = [red, red, blue, blue].concat();
        let dib = dib_from_bgra(2, 2, &px).unwrap();
        assert_eq!(dib.len(), 40 + 16);
        assert_eq!(u16::from_le_bytes([dib[14], dib[15]]), 32);
        assert_eq!(u32::from_le_bytes(dib[20..24].try_into().unwrap()), 16);
        // DIBは下の行から並ぶ
        assert_eq!(&dib[40..44], &blue);
        assert_eq!(&dib[48..52], &red);
        assert_eq!(dib_from_bgra(2, 2, &px[..8]), None);
        assert_eq!(dib_from_bgra(0, 2, &px), None);
    }
}
