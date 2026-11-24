//! 画面の色と大きさ(A5. wakudoriの`Theme.cs`と同じ色. ライトテーマだけ).

pub const BG: u32 = 0xFAF9F5;
pub const BG_SUBTLE: u32 = 0xF0EEE6;
pub const BG_MUTED: u32 = 0xE8E6DC;
pub const TEXT: u32 = 0x141413;
pub const TEXT_SECONDARY: u32 = 0x3D3D3A;
pub const TEXT_TERTIARY: u32 = 0x5E5D59;
pub const BORDER: u32 = 0xD1CFC5;
pub const BORDER_FAINT: u32 = 0xE8E6DC;
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
