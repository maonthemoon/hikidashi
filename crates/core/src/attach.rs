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

/// 貼り付けで何をするか
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

/// 検索の入力欄に画像を貼ったとき, 書く窓に引き継ぐ文
pub fn paste_to_compose(kind: &Paste, typed: &str) -> Option<String> {
    has_image(kind).then(|| typed.trim().to_string())
}

/// 検索の入力欄では, 画像の貼り付けだけキーの繰り返しを止める(K73)
pub fn suppress_search_paste_repeat(kind: &Paste, repeat: bool) -> bool {
    repeat && has_image(kind)
}

fn has_image(kind: &Paste) -> bool {
    match kind {
        Paste::Image => true,
        Paste::Files(paths) => paths.iter().any(|p| is_image_path(p)),
        Paste::Text => false,
    }
}

/// 検索から貼る画像を先に読む. 1枚も読めなければ知らせだけを返す
pub fn load_search_images(
    kind: &Paste,
    read_clipboard: impl FnOnce() -> Result<Image, String>,
    mut load_file: impl FnMut(&str) -> Result<Image, String>,
) -> Result<(Vec<Image>, Option<String>), String> {
    match kind {
        Paste::Image => read_clipboard().map(|img| (vec![img], None)),
        Paste::Files(paths) => {
            let mut images = Vec::new();
            let mut message = None;
            for path in paths.iter().filter(|p| is_image_path(p)) {
                if room(images.len()) == 0 {
                    message = Some(format!("Up to {MAX_IMAGES} images"));
                    break;
                }
                match load_file(path) {
                    Ok(img) => images.push(img),
                    Err(e) => message = Some(e),
                }
            }
            if images.is_empty() {
                Err(message.unwrap_or_else(|| "Couldn't read any image to attach".into()))
            } else {
                Ok((images, message))
            }
        }
        Paste::Text => Err("Couldn't read any image to attach".into()),
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

/// 画像を貼ったあと, 送信ボタンが見つからないときに待つ時間(1枚ごとに`per_ms`を足す)
pub fn wait_ms(images: usize, per_ms: u32) -> u64 {
    images as u64 * per_ms as u64
}

/// 同じ窓でもタイトルが変われば新しいページとみなす. 待っても区別できなければ, 最後は今の前面のChatGPTを使う.
pub fn chatgpt_page_ready(previous_pages: &[(isize, String)], hwnd: isize, title: &str, elapsed_ms: u64, delay_ms: u64) -> bool {
    elapsed_ms >= delay_ms || !previous_pages.iter().any(|(old_hwnd, old_title)| *old_hwnd == hwnd && old_title == title)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendWaitStep {
    Wait { until_ms: u64 },
    Ready,
    Fallback,
    TimedOut,
}

pub struct SendWait {
    found: bool,
    ready_streak: u8,
    ready_checks: u8,
    waiting_for_paste: bool,
}

impl Default for SendWait {
    fn default() -> Self {
        Self { found: false, ready_streak: 0, ready_checks: 1, waiting_for_paste: false }
    }
}

impl SendWait {
    pub fn after_paste() -> Self {
        Self { ready_checks: 2, waiting_for_paste: true, ..Self::default() }
    }

    /// ボタンが見つかったら上限まで有効になるのを待つ. 一度も見つからなければ決め打ちの待ちへ切り替える.
    pub fn step(&mut self, elapsed_ms: u64, enabled: Option<bool>, limit_ms: u64, missing_ms: u64) -> SendWaitStep {
        self.found |= enabled.is_some();
        // 貼り付け前から有効だった状態を数えない. 無効を見たか, 貼り付けから1秒たったら数え始める.
        self.waiting_for_paste &= elapsed_ms < 1000 && enabled != Some(false);
        self.ready_streak = if !self.waiting_for_paste && enabled == Some(true) { self.ready_streak.saturating_add(1) } else { 0 };
        if self.ready_streak >= self.ready_checks {
            SendWaitStep::Ready
        } else if elapsed_ms >= limit_ms {
            SendWaitStep::TimedOut
        } else if !self.found && elapsed_ms >= missing_ms {
            SendWaitStep::Fallback
        } else {
            let until_ms = if self.found { limit_ms } else { limit_ms.min(missing_ms) };
            SendWaitStep::Wait { until_ms }
        }
    }
}

/// 画像つきで送るときに開くURL. 文字は`?q=`に入れず, 開いてから画像と一緒に貼る(K73).
/// `?q=`で開くと, ChatGPTによっては開いたとたんに文字だけを送ってしまう(確認17のI2. 個人のアカウント).
/// `q={q}`の項目を消し, 残りの項目はそのまま(例: `https://chatgpt.com/?model=x&q={q}` → `https://chatgpt.com/?model=x`)
pub fn url_without_q(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else { return url.replace("{q}", "") };
    let rest: Vec<&str> = query.split('&').filter(|p| !p.contains("{q}")).collect();
    if rest.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", rest.join("&"))
    }
}

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
    fn image_paste_carries_trimmed_search_text() {
        assert_eq!(paste_to_compose(&Paste::Image, "　この画像を  説明して 🐈 \t").as_deref(), Some("この画像を  説明して 🐈"));
        assert_eq!(paste_to_compose(&Paste::Image, "").as_deref(), Some(""));
        assert_eq!(paste_to_compose(&Paste::Image, " \t　").as_deref(), Some(""));
    }

    #[test]
    fn image_files_among_other_files_open_compose() {
        let files = Paste::Files(vec![r"C:\a.txt".into(), r"C:\b.PNG".into()]);
        assert_eq!(paste_to_compose(&files, " 写真 ").as_deref(), Some("写真"));
    }

    #[test]
    fn text_and_non_image_files_do_not_open_compose() {
        assert_eq!(paste_to_compose(&Paste::Text, "検索語"), None);
        assert_eq!(paste_to_compose(&Paste::Files(vec![r"C:\a.txt".into()]), "検索語"), None);
        assert_eq!(paste_to_compose(&Paste::Files(vec![]), "検索語"), None);
    }

    #[test]
    fn search_paste_repeat_is_suppressed_only_for_images() {
        let mixed_files = Paste::Files(vec!["notes.txt".into(), "photo.PNG".into()]);
        let other_files = Paste::Files(vec!["notes.txt".into()]);
        assert!(suppress_search_paste_repeat(&Paste::Image, true));
        assert!(suppress_search_paste_repeat(&mixed_files, true));
        assert!(!suppress_search_paste_repeat(&Paste::Image, false));
        assert!(!suppress_search_paste_repeat(&mixed_files, false));
        assert!(!suppress_search_paste_repeat(&Paste::Text, true));
        assert!(!suppress_search_paste_repeat(&other_files, true));
    }

    #[test]
    fn unreadable_clipboard_image_cannot_replace_the_draft() {
        let message = "Couldn't read the image on the clipboard";
        let result = load_search_images(&Paste::Image, || Err(message.into()), |_| unreachable!());
        assert_eq!(result, Err(message.into()));
    }

    #[test]
    fn unreadable_image_files_cannot_replace_the_draft() {
        let files = Paste::Files(vec!["too-large.png".into(), "broken.jpg".into()]);
        let result = load_search_images(&files, || unreachable!(), |path| Err(format!("Couldn't read {path}")));
        assert_eq!(result, Err("Couldn't read broken.jpg".into()));
    }

    #[test]
    fn readable_image_file_can_replace_the_draft_even_if_another_file_fails() {
        let files = Paste::Files(vec!["notes.txt".into(), "too-large.png".into(), "photo.png".into()]);
        let photo = Image { width: 32, height: 16, ..Image::default() };
        let result = load_search_images(&files, || unreachable!(), |path| match path {
            "too-large.png" => Err("too-large.png is too large to attach".into()),
            "photo.png" => Ok(photo.clone()),
            _ => panic!("unexpected image path: {path}"),
        });
        assert_eq!(result, Ok((vec![photo], Some("too-large.png is too large to attach".into()))));
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
    fn chatgpt_page_waits_for_a_new_window_or_title_until_delay() {
        let pages = [(1, "First - ChatGPT".into()), (2, "Second - ChatGPT".into())];
        assert!(!chatgpt_page_ready(&pages, 1, "First - ChatGPT", 0, 5000));
        assert!(!chatgpt_page_ready(&pages, 2, "Second - ChatGPT", 4999, 5000));
        assert!(chatgpt_page_ready(&pages, 1, "Second - ChatGPT", 0, 5000));
        assert!(chatgpt_page_ready(&pages, 3, "First - ChatGPT", 0, 5000));
        assert!(chatgpt_page_ready(&pages, 2, "Second - ChatGPT", 5000, 5000));
    }

    #[test]
    fn send_wait_requires_consecutive_enabled_checks() {
        use SendWaitStep::*;
        let mut wait = SendWait::after_paste();
        let waiting = Wait { until_ms: 8000 };
        assert_eq!(wait.step(0, Some(true), 8000, 2000), waiting);
        assert_eq!(wait.step(250, Some(true), 8000, 2000), waiting);
        assert_eq!(wait.step(500, Some(false), 8000, 2000), waiting);
        assert_eq!(wait.step(550, Some(true), 8000, 2000), waiting);
        assert_eq!(wait.step(600, None, 8000, 2000), waiting);
        assert_eq!(wait.step(650, Some(true), 8000, 2000), waiting);
        assert_eq!(wait.step(900, Some(true), 8000, 2000), Ready);
    }

    #[test]
    fn send_wait_ignores_enabled_checks_for_one_second_if_never_disabled() {
        use SendWaitStep::*;
        let mut wait = SendWait::after_paste();
        let waiting = Wait { until_ms: 5000 };
        assert_eq!(wait.step(0, Some(true), 5000, 800), waiting);
        assert_eq!(wait.step(250, None, 5000, 800), waiting);
        assert_eq!(wait.step(500, Some(true), 5000, 800), waiting);
        assert_eq!(wait.step(750, Some(true), 5000, 800), waiting);
        assert_eq!(wait.step(1000, Some(true), 5000, 800), waiting);
        assert_eq!(wait.step(1250, Some(true), 5000, 800), Ready);
    }

    #[test]
    fn send_wait_falls_back_only_if_button_was_never_found() {
        use SendWaitStep::*;
        let mut missing = SendWait::after_paste();
        assert_eq!(missing.step(800, None, 5000, 800), Fallback);
        let mut missing = SendWait::after_paste();
        assert_eq!(missing.step(1999, None, 8000, 2000), Wait { until_ms: 2000 });
        assert_eq!(missing.step(2000, None, 8000, 2000), Fallback);

        let mut found = SendWait::after_paste();
        assert_eq!(found.step(1750, Some(false), 8000, 2000), Wait { until_ms: 8000 });
        assert_eq!(found.step(2000, None, 8000, 2000), Wait { until_ms: 8000 });
        assert_eq!(found.step(7999, Some(false), 8000, 2000), Wait { until_ms: 8000 });
        assert_eq!(found.step(8000, Some(false), 8000, 2000), TimedOut);
    }

    #[test]
    fn send_wait_url_can_use_one_enabled_check() {
        assert_eq!(SendWait::default().step(250, Some(true), 15000, 5000), SendWaitStep::Ready);
        assert_eq!(SendWait::default().step(5000, None, 15000, 5000), SendWaitStep::Fallback);
    }

    #[test]
    fn url_for_images() {
        assert_eq!(url_without_q("https://chatgpt.com/?q={q}"), "https://chatgpt.com/");
        assert_eq!(url_without_q("https://chatgpt.com/?model=x&q={q}"), "https://chatgpt.com/?model=x");
        assert_eq!(url_without_q("https://chatgpt.com/g/g-abc?q={q}&hints=search"), "https://chatgpt.com/g/g-abc?hints=search");
        assert_eq!(url_without_q("https://chatgpt.com/"), "https://chatgpt.com/");
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
