//! Tabのメニューの「〜で開く」と, `Alt+Enter`のターミナル(Q5, Q6). どのアプリに何を渡すかを組み立てる.
//! exeを探すのとプロセスを起こすのは画面の側(`actions.rs`)が行う.

use crate::config::{Config, OpenWith};

/// アプリの種類. `args`が空のときの渡し方が変わる
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    VsCode,
    WezTerm,
    WindowsTerminal,
    Other,
}

impl ToolKind {
    /// exeの名前から決める. exeが空なら名前から(自動で探すとき)
    pub fn of(ow: &OpenWith) -> ToolKind {
        let exe = ow.exe.trim();
        if !exe.is_empty() {
            let file = exe.rsplit(['\\', '/']).next().unwrap_or(exe).to_ascii_lowercase();
            return match file.trim_end_matches(".exe") {
                "code" | "code.cmd" | "code-insiders" => ToolKind::VsCode,
                "wezterm-gui" | "wezterm" => ToolKind::WezTerm,
                "wt" | "windowsterminal" => ToolKind::WindowsTerminal,
                _ => ToolKind::Other,
            };
        }
        match ow.name.trim().to_ascii_lowercase().as_str() {
            "vs code" | "vscode" | "visual studio code" | "code" => ToolKind::VsCode,
            "wezterm" => ToolKind::WezTerm,
            "windows terminal" | "terminal" | "wt" => ToolKind::WindowsTerminal,
            _ => ToolKind::Other,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, ToolKind::WezTerm | ToolKind::WindowsTerminal)
    }
}

/// 初めの一覧. 入っているものだけメニューに出る(Q5)
pub fn defaults() -> Vec<OpenWith> {
    ["VS Code", "WezTerm", "Windows Terminal"].iter().map(|n| OpenWith { name: n.to_string(), exe: String::new(), args: String::new() }).collect()
}

/// `Alt+Enter`で使うターミナル(Q6). 設定の`[tools] terminal`の名前. 空なら一覧の中のWezTerm, Windows Terminalの順で,
/// 入っているもの(`installed`)
pub fn terminal<'a>(cfg: &'a Config, installed: impl Fn(&OpenWith) -> bool) -> Option<&'a OpenWith> {
    let want = cfg.tools.terminal.trim();
    if !want.is_empty() {
        return cfg.open_with.iter().find(|o| o.name.eq_ignore_ascii_case(want)).filter(|o| installed(o));
    }
    for k in [ToolKind::WezTerm, ToolKind::WindowsTerminal] {
        if let Some(o) = cfg.open_with.iter().find(|o| ToolKind::of(o) == k && installed(o)) {
            return Some(o);
        }
    }
    None
}

/// 開く場所. `wsl`はWSLの中のパスとディストリ(`\\wsl.localhost\…`のとき)
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// ファイルかフォルダー(Windowsのパス)
    pub path: String,
    /// フォルダー(ファイルならその親)
    pub dir: String,
    pub is_dir: bool,
    /// (ディストリ, WSLの中のパス, WSLの中のフォルダー)
    pub wsl: Option<(String, String, String)>,
}

impl Target {
    pub fn new(path: &str, is_dir: bool) -> Target {
        let path = path.trim_end_matches('\\').to_string();
        let path = if path.ends_with(':') { format!("{path}\\") } else { path };
        let dir = if is_dir { path.clone() } else { crate::pathlike::split_name(&path).1.to_string() };
        let wsl = wsl_parts(&path).map(|(d, p)| {
            let pd = if is_dir { p.clone() } else { p.rsplit_once('/').map(|x| if x.0.is_empty() { "/".to_string() } else { x.0.to_string() }).unwrap_or_else(|| "/".into()) };
            (d, p, pd)
        });
        Target { path, dir, is_dir, wsl }
    }
}

/// `\\wsl.localhost\<distro>\home\me` → (`<distro>`, `/home/me`)
fn wsl_parts(p: &str) -> Option<(String, String)> {
    let l = p.to_ascii_lowercase();
    for pre in ["\\\\wsl.localhost\\", "\\\\wsl$\\"] {
        if l.starts_with(pre) {
            let rest = &p[pre.len()..];
            let (distro, tail) = rest.split_once('\\').unwrap_or((rest, ""));
            if distro.is_empty() {
                return None;
            }
            return Some((distro.to_string(), format!("/{}", tail.replace('\\', "/"))));
        }
    }
    None
}

/// 引数1つを, 空白や引用符があってもそのまま届く形にする(Windowsのコマンドラインの決まり)
pub fn quote(s: &str) -> String {
    if !s.is_empty() && !s.chars().any(|c| c == ' ' || c == '\t' || c == '"') {
        return s.to_string();
    }
    let mut o = String::from("\"");
    let mut bs = 0;
    for c in s.chars() {
        match c {
            '\\' => bs += 1,
            '"' => {
                o.push_str(&"\\".repeat(bs * 2 + 1));
                o.push('"');
                bs = 0;
            }
            _ => {
                o.push_str(&"\\".repeat(bs));
                o.push(c);
                bs = 0;
            }
        }
    }
    o.push_str(&"\\".repeat(bs * 2));
    o.push('"');
    o
}

/// exeに渡すコマンドラインの引数. `args`が空なら, アプリごとのいつもの渡し方(WSLのパスならWSLの中で開く).
/// `{path}`(ファイルかフォルダー), `{dir}`(フォルダー)は引用符付きで置き換える.
/// `folder_only`はターミナルで入るとき(`Alt+Enter`)で, ファイルでもそのフォルダーを開く
pub fn command_line(ow: &OpenWith, t: &Target, folder_only: bool) -> String {
    if !ow.args.trim().is_empty() {
        return ow.args.replace("{path}", &quote(&t.path)).replace("{dir}", &quote(&t.dir));
    }
    let kind = ToolKind::of(ow);
    match (kind, &t.wsl) {
        // 既定の起動先をWSLにしていても, Windowsの側からwsl.exeを起こす(B3. `--domain local`がないとWSLの中でwsl.exeを探して落ちる)
        (ToolKind::WezTerm, Some((d, _, pd))) => format!("start --domain local -- wsl.exe -d {} --cd {}", quote(d), quote(pd)),
        (ToolKind::WezTerm, None) => format!("start --domain local --cwd {}", quote(&t.dir)),
        (ToolKind::WindowsTerminal, Some((d, _, pd))) => format!("wsl.exe -d {} --cd {}", quote(d), quote(pd)),
        (ToolKind::WindowsTerminal, None) => format!("-d {}", quote(&t.dir)),
        // VS CodeはWSLの中のものをRemote - WSLで開く(UNCのパスだと確認が出るため)
        (ToolKind::VsCode, Some((d, p, pd))) => {
            if t.is_dir || folder_only {
                format!("--folder-uri {}", quote(&format!("vscode-remote://wsl+{d}{}", if folder_only { pd } else { p })))
            } else {
                format!("--file-uri {}", quote(&format!("vscode-remote://wsl+{d}{p}")))
            }
        }
        _ => quote(if folder_only { &t.dir } else { &t.path }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ow(name: &str, exe: &str, args: &str) -> OpenWith {
        OpenWith { name: name.into(), exe: exe.into(), args: args.into() }
    }

    #[test]
    fn kinds() {
        assert_eq!(ToolKind::of(&ow("VS Code", "", "")), ToolKind::VsCode);
        assert_eq!(ToolKind::of(&ow("Editor", r"C:\Program Files\WezTerm\wezterm-gui.exe", "")), ToolKind::WezTerm);
        assert_eq!(ToolKind::of(&ow("x", "wt.exe", "")), ToolKind::WindowsTerminal);
        assert_eq!(ToolKind::of(&ow("Notepad++", r"C:\npp\notepad++.exe", "")), ToolKind::Other);
    }

    #[test]
    fn wezterm_wsl_uses_local_domain() {
        let t = Target::new(r"\\wsl.localhost\Ubuntu\home\me\proj", true);
        assert_eq!(command_line(&ow("WezTerm", "", ""), &t, true), "start --domain local -- wsl.exe -d Ubuntu --cd /home/me/proj");
        // ファイルならそのフォルダー
        let t = Target::new(r"\\wsl.localhost\Ubuntu-22.04\home\me\a b\x.md", false);
        assert_eq!(command_line(&ow("WezTerm", "", ""), &t, true), "start --domain local -- wsl.exe -d Ubuntu-22.04 --cd \"/home/me/a b\"");
    }

    #[test]
    fn windows_paths() {
        let t = Target::new(r"C:\Users\me\My Docs\a.pptx", false);
        assert_eq!(t.dir, r"C:\Users\me\My Docs");
        assert_eq!(command_line(&ow("WezTerm", "", ""), &t, true), "start --domain local --cwd \"C:\\Users\\me\\My Docs\"");
        assert_eq!(command_line(&ow("Windows Terminal", "", ""), &t, true), "-d \"C:\\Users\\me\\My Docs\"");
        assert_eq!(command_line(&ow("VS Code", "", ""), &t, false), "\"C:\\Users\\me\\My Docs\\a.pptx\"");
        // ドライブの直下
        let t = Target::new(r"D:\", true);
        assert_eq!(command_line(&ow("Windows Terminal", "", ""), &t, true), "-d D:\\");
    }

    #[test]
    fn vscode_wsl_remote() {
        let t = Target::new(r"\\wsl.localhost\Ubuntu\home\me\notes.md", false);
        assert_eq!(command_line(&ow("VS Code", "", ""), &t, false), "--file-uri vscode-remote://wsl+Ubuntu/home/me/notes.md");
        let t = Target::new(r"\\wsl$\Ubuntu\home\me", true);
        assert_eq!(command_line(&ow("VS Code", "", ""), &t, false), "--folder-uri vscode-remote://wsl+Ubuntu/home/me");
    }

    #[test]
    fn custom_args() {
        let t = Target::new(r"C:\a b\c.txt", false);
        let o = ow("nvim", r"C:\WezTerm\wezterm-gui.exe", "start -- nvim {path}");
        assert_eq!(command_line(&o, &t, false), "start -- nvim \"C:\\a b\\c.txt\"");
        let o = ow("x", "x.exe", "--in {dir}");
        assert_eq!(command_line(&o, &t, false), "--in \"C:\\a b\"");
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("abc"), "abc");
        assert_eq!(quote(r"C:\a b\"), "\"C:\\a b\\\\\"");
        assert_eq!(quote(""), "\"\"");
    }

    #[test]
    fn terminal_choice() {
        let mut c = Config::default();
        let t = terminal(&c, |o| o.name != "WezTerm").map(|o| o.name.clone());
        assert_eq!(t.as_deref(), Some("Windows Terminal"));
        let t = terminal(&c, |_| true).map(|o| o.name.clone());
        assert_eq!(t.as_deref(), Some("WezTerm"));
        c.tools.terminal = "windows terminal".into();
        assert_eq!(terminal(&c, |_| true).map(|o| o.name.as_str()), Some("Windows Terminal"));
        assert!(terminal(&c, |_| false).is_none());
    }
}
