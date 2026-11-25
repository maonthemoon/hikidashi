//! 画面の色と大きさ(A5. wakudoriの`Theme.cs`と同じ色).
//! ランチャーの窓と書く窓は, ライトとダークの色の組(`Palette`)を`pal()`で使い分ける(K39).
//! 設定画面も同じ色の組を使う(K43. 標準の部品のダークは`dark.rs`).

use std::sync::atomic::{AtomicBool, Ordering};

/// ランチャーの窓と書く窓の色の組
pub struct Palette {
    pub bg: u32,
    pub bg_subtle: u32,
    pub bg_muted: u32,
    pub text: u32,
    pub text_secondary: u32,
    pub text_tertiary: u32,
    pub border: u32,
    pub border_faint: u32,
    pub accent: u32,
    pub accent_text: u32,
    pub danger: u32,
    /// 書く窓の薄い字(Markdownの記号)
    pub faint: u32,
    /// 書く窓の蛍光ペン(`==語==`)
    pub highlight: u32,
    /// 設定画面の入力欄と一覧の地
    pub field: u32,
    /// 設定画面のボタン, 押したとき
    pub btn: u32,
    pub btn_down: u32,
    /// 設定画面のカードの地(K44)
    pub card: u32,
    /// 設定画面で選んでいる区分と行の地(K44)
    pub selected: u32,
    /// 設定画面の外したチェックボックスの枠(K44)
    pub off: u32,
    /// 設定画面の「反映しました」の文字と地(K45)
    pub ok: u32,
    pub ok_bg: u32,
}

pub const LIGHT: Palette = Palette {
    bg: BG,
    bg_subtle: BG_SUBTLE,
    bg_muted: BG_MUTED,
    text: TEXT,
    text_secondary: TEXT_SECONDARY,
    text_tertiary: TEXT_TERTIARY,
    border: BORDER,
    border_faint: BORDER_FAINT,
    accent: ACCENT,
    accent_text: ACCENT_TEXT,
    danger: DANGER,
    faint: 0x9C9A92,
    highlight: 0xFBE38E,
    field: 0xFFFFFF,
    btn: 0xFFFFFF,
    btn_down: BG_MUTED,
    card: 0xFFFFFF,
    selected: 0xDCD8CA,
    off: 0x87867F,
    ok: 0x3F5A22,
    ok_bg: 0xE1EAD2,
};

/// ダーク(暖かい黒. 紹介のページのダークと同じ系統. K39のQ1)
pub const DARK: Palette = Palette {
    bg: 0x1F1E1D,
    bg_subtle: 0x262624,
    bg_muted: 0x30302E,
    text: 0xFAF9F5,
    text_secondary: 0xD1CFC5,
    text_tertiary: 0x9C9A92,
    border: 0x3D3D3A,
    border_faint: 0x30302E,
    accent: 0xD97757,
    accent_text: 0xE8956F,
    danger: 0xE07A70,
    faint: 0x73726C,
    highlight: 0x6B5A24,
    field: 0x2B2A28,
    btn: 0x30302E,
    btn_down: 0x3D3D3A,
    card: 0x262624,
    selected: 0x3D3D3A,
    off: 0x8A8880,
    ok: 0xC3DBA0,
    ok_bg: 0x35412A,
};

static DARK_ON: AtomicBool = AtomicBool::new(false);

/// 今の色の組
pub fn pal() -> &'static Palette {
    if is_dark() {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn is_dark() -> bool {
    DARK_ON.load(Ordering::Relaxed)
}

/// 設定の`[ui] theme`(`auto`, `light`, `dark`)から決める. `auto`はWindowsのアプリのモード. 変わったらtrue
pub fn set_theme(code: &str) -> bool {
    let dark = match code {
        "dark" => true,
        "light" => false,
        _ => windows_apps_dark(),
    };
    let changed = DARK_ON.swap(dark, Ordering::Relaxed) != dark;
    if changed {
        super::dark::set_app_mode(dark);
    }
    changed
}

/// Windowsの設定の「既定のアプリモード」がダークか
/// (`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`の`AppsUseLightTheme`が0)
fn windows_apps_dark() -> bool {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut v: u32 = 1;
    let mut size = 4u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    r.is_ok() && v == 0
}

pub const BG: u32 = 0xFAF9F5;
pub const BG_SUBTLE: u32 = 0xF0EEE6;
pub const BG_MUTED: u32 = 0xE8E6DC;
pub const TEXT: u32 = 0x141413;
pub const TEXT_SECONDARY: u32 = 0x3D3D3A;
pub const TEXT_TERTIARY: u32 = 0x5E5D59;
pub const BORDER: u32 = 0xD1CFC5;
pub const BORDER_FAINT: u32 = 0xE8E6DC;
/// 塗りのボタンと, 入れたチェックボックス(白い文字を載せる. ライトとダークで同じ)
pub const FILL: u32 = 0xB85A39;
pub const FILL_DOWN: u32 = 0xA9472A;
/// キーボードで選んでいる部品の枠
pub const FOCUS: u32 = 0x2C84DB;
/// 入力欄の下の線が光るときの色(J2b)
pub const ACCENT: u32 = 0xD97757;
/// killの右端の文字(K9)など, アクセントの文字
pub const ACCENT_TEXT: u32 = 0xA9472A;
pub const DANGER: u32 = 0xBF4D43;

// 大きさ(96dpiのときのピクセル. 画面の拡大率を掛けて使う)
pub const WIDTH: i32 = 680;
pub const INPUT_H: i32 = 60;
pub const PAD_X: i32 = 20;
pub const ROW_H: i32 = 52;
pub const LIST_PAD: i32 = 6;
pub const ICON: i32 = 28;
pub const INPUT_FONT: i32 = 20;
pub const NAME_FONT: i32 = 15;
pub const SUB_FONT: i32 = 13;
pub const TAG_FONT: i32 = 13;
pub const STATUS_H: i32 = 30;
pub const MENU_TITLE_H: i32 = 28;
/// 長い質問を書く窓(K22)の上と下の余白, 中身の最小の高さ
pub const COMPOSE_TOP: i32 = 14;
/// 書く窓の上の帯(K24のQ2): 左に`ChatGPT`の印, 右にキーの案内
pub const COMPOSE_BAND: i32 = 38;
pub const COMPOSE_BOTTOM: i32 = 12;
pub const COMPOSE_MIN_BODY: i32 = 96;
