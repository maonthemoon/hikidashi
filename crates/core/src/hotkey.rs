//! ホットキーの文字(`Alt+Space`など)と, 修飾キー・仮想キーの値を相互に変える(A1).
//! 値はWindowsの`RegisterHotKey`の`MOD_*`と仮想キーコードと同じ.

pub const MOD_ALT: u32 = 1;
pub const MOD_CONTROL: u32 = 2;
pub const MOD_SHIFT: u32 = 4;
pub const MOD_WIN: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: u32,
    pub vk: u32,
}

/// 仮想キーの名前. 文字と数字はそのまま.
pub fn key_name(vk: u32) -> Option<String> {
    let s = match vk {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Esc",
        0x1C => "Henkan",
        0x1D => "Muhenkan",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2C => "PrintScreen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x30..=0x39 | 0x41..=0x5A => return Some(char::from_u32(vk).unwrap().to_string()),
        0x60..=0x69 => return Some(format!("Num{}", vk - 0x60)),
        0x6A => "Num*",
        0x6B => "Num+",
        0x6D => "Num-",
        0x6E => "Num.",
        0x6F => "Num/",
        0x70..=0x87 => return Some(format!("F{}", vk - 0x6F)),
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        0xF3 | 0xF4 => "Hankaku",
        _ => return None,
    };
    Some(s.to_string())
}

fn vk_from_name(name: &str) -> Option<u32> {
    let alias = match name.to_ascii_lowercase().as_str() {
        "escape" => Some(0x1B),
        "return" => Some(0x0D),
        "del" => Some(0x2E),
        "ins" => Some(0x2D),
        "pgup" => Some(0x21),
        "pgdn" => Some(0x22),
        "prtsc" | "prtscn" => Some(0x2C),
        _ => None,
    };
    if alias.is_some() {
        return alias;
    }
    (0u32..256).find(|&vk| key_name(vk).is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

impl Hotkey {
    /// `Ctrl+Alt+Space`の形にする(修飾キーは Ctrl, Alt, Shift, Win の順)
    pub fn format(&self) -> String {
        let mut s = String::new();
        for (m, n) in [(MOD_CONTROL, "Ctrl+"), (MOD_ALT, "Alt+"), (MOD_SHIFT, "Shift+"), (MOD_WIN, "Win+")] {
            if self.mods & m != 0 {
                s.push_str(n);
            }
        }
        s.push_str(&key_name(self.vk).unwrap_or_else(|| format!("0x{:02X}", self.vk)));
        s
    }

    pub fn parse(s: &str) -> Option<Hotkey> {
        let mut mods = 0;
        let mut vk = None;
        for part in s.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods |= MOD_CONTROL,
                "alt" => mods |= MOD_ALT,
                "shift" => mods |= MOD_SHIFT,
                "win" | "windows" => mods |= MOD_WIN,
                "" => return None,
                _ => {
                    if vk.is_some() {
                        return None;
                    }
                    vk = Some(vk_from_name(part).or_else(|| part.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok()))?);
                }
            }
        }
        Some(Hotkey { mods, vk: vk? })
    }

    /// 修飾キーなしで使うと普段の入力を邪魔するキーか(F1〜F24などは単独でも許す)
    pub fn is_usable(&self) -> bool {
        if self.mods != 0 {
            return true;
        }
        matches!(self.vk, 0x13 | 0x2C | 0x70..=0x87 | 0x1C | 0x1D | 0xF3 | 0xF4)
    }
}

/// アプリが自分で書く小さな状態のファイル(`state.txt`. 1行に`key = value`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub hotkey: Option<String>,
    pub autostart_asked: bool,
}

impl State {
    pub fn parse(s: &str) -> State {
        let mut st = State::default();
        for line in s.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "hotkey" if !v.is_empty() => st.hotkey = Some(v.to_string()),
                "autostart_asked" => st.autostart_asked = v == "true",
                _ => {}
            }
        }
        st
    }

    pub fn to_text(&self) -> String {
        format!(
            "# Hikidashi state. Written by Hikidashi.\nhotkey = {}\nautostart_asked = {}\n",
            self.hotkey.as_deref().unwrap_or(""),
            self.autostart_asked
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in ["Alt+Space", "Ctrl+Shift+K", "Win+F1", "Ctrl+Alt+1", "F13", "Ctrl+;"] {
            let h = Hotkey::parse(s).unwrap();
            assert_eq!(h.format(), s);
        }
        assert_eq!(Hotkey::parse("alt + space"), Some(Hotkey { mods: MOD_ALT, vk: 0x20 }));
        assert_eq!(Hotkey::parse("Ctrl+"), None);
        assert_eq!(Hotkey::parse("Ctrl+A+B"), None);
    }

    #[test]
    fn usable() {
        assert!(Hotkey::parse("Alt+Space").unwrap().is_usable());
        assert!(!Hotkey::parse("A").unwrap().is_usable());
        assert!(Hotkey::parse("F13").unwrap().is_usable());
    }

    #[test]
    fn state() {
        let s = State { hotkey: Some("Alt+Space".into()), autostart_asked: true };
        assert_eq!(State::parse(&s.to_text()), s);
        assert_eq!(State::parse(""), State::default());
    }
}
