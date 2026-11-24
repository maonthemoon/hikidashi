//! 選んだ文字で開く(2つ目のホットキー)の, 画面とOSに頼らない判断.

/// Ctrl+Cを送らずに空の窓を出すアプリ(ターミナル. Ctrl+Cが中断になるため)
pub const TERMINAL_EXES: [&str; 9] = [
    "WindowsTerminal.exe",
    "wezterm-gui.exe",
    "OpenConsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "mintty.exe",
    "alacritty.exe",
];

/// プロセスのexe(名前だけでもフルパスでもよい)がターミナルか
pub fn is_terminal(exe: &str) -> bool {
    let name = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    TERMINAL_EXES.iter().any(|t| t.eq_ignore_ascii_case(name))
}

/// 読んだ文字の扱い
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    /// 読めなかった(空の窓を出す)
    Empty,
    /// 1行(前後の空白と改行を除いたもの). 入力欄に入れる
    Line(String),
    /// 2行以上. 長い質問を書く窓に入れる(前後の空白と改行は除く)
    Multi(String),
}

pub fn classify(text: &str) -> Picked {
    let t = text.trim();
    if t.is_empty() {
        Picked::Empty
    } else if t.contains(['\n', '\r']) {
        Picked::Multi(t.to_string())
    } else {
        Picked::Line(t.to_string())
    }
}

/// 起動の引数から`--query <文字>`の文字を取る. `--query=文字`の形も読む
pub fn query_arg<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--query" {
            return it.next();
        }
        if let Some(v) = a.strip_prefix("--query=") {
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals() {
        assert!(is_terminal("WindowsTerminal.exe"));
        assert!(is_terminal(r"C:\Windows\System32\CMD.EXE"));
        assert!(is_terminal("pwsh.exe"));
        assert!(!is_terminal("chrome.exe"));
        assert!(!is_terminal("notcmd.exe"));
    }

    #[test]
    fn classify_text() {
        assert_eq!(classify(""), Picked::Empty);
        assert_eq!(classify(" \r\n "), Picked::Empty);
        assert_eq!(classify("  hello world \r\n"), Picked::Line("hello world".into()));
        assert_eq!(classify("a\r\nb\r\n"), Picked::Multi("a\r\nb".into()));
        assert_eq!(classify("\n\na\nb"), Picked::Multi("a\nb".into()));
    }

    #[test]
    fn query_argument() {
        let a = |v: &[&str]| query_arg(v.iter().map(|s| s.to_string()));
        assert_eq!(a(&["--query", "hello world"]), Some("hello world".into()));
        assert_eq!(a(&["--here", "--query=x y"]), Some("x y".into()));
        assert_eq!(a(&["--query"]), None);
        assert_eq!(a(&["--autostart"]), None);
    }
}
