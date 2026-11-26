//! パスの貼り付け(H1)の見分け. パスの形をした入力は検索より先に1位に出す.

/// 貼り付けられた入力の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pasted {
    /// Windowsのパス(`C:\...`, `\\server\share\...`, `%USERPROFILE%\...`). 区切りは`\`にそろえてある.
    WinPath(String),
    /// WSLのパス(`/home/...`など). `/mnt/c/...`はWinPathに変える.
    WslPath(String),
    /// BoxのWebのURL. IDから手元のパスを引く(PoC待ち). 引けなければURLを開く.
    BoxUrl { url: String, folder: bool, id: String },
    /// そのほかの`http://`, `https://`のURL(Boxの共有リンクなど). ブラウザーで開く(B4)
    Url(String),
}

/// 前後の空白, 引用符, `<>`, 末尾の句読点を取る.
pub fn clean(input: &str) -> &str {
    const OPEN: &[char] = &['"', '\'', '<', '“', '‘', '「', '『', '(', '（', '`'];
    const CLOSE: &[char] = &['"', '\'', '>', '”', '’', '」', '』', ')', '）', '`'];
    const TAIL: &[char] = &['.', ',', '。', '、', ';', '；', '．', '，'];
    let mut s = input.trim();
    loop {
        let before = s.len();
        s = s.trim_start_matches(OPEN).trim_end_matches(CLOSE).trim();
        // 末尾の句読点. ただし`C:\.`のような意味のある点は残さない場面がほぼないので, まとめて取る
        s = s.trim_end_matches(TAIL).trim();
        if s.len() == before {
            break;
        }
    }
    s
}

fn is_drive_path(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 自作コマンド(`[[command]]`の`run`)が, ファイルやフォルダーを開くだけのものなら, そのパスを返す.
/// `C:\…`, `"C:\…"`, `explorer "C:\…"`, `start "" "C:\…"`の形. 環境変数はそのまま返す.
/// 呼ぶ側で, あることを確かめてから直接開く(`cmd.exe`と`explorer.exe`を起こさない分, 速い)
pub fn command_target(cmd: &str) -> Option<String> {
    let mut s = cmd.trim();
    let lower = s.to_ascii_lowercase();
    if let Some(p) = ["explorer.exe ", "explorer "].into_iter().find(|p| lower.starts_with(p)) {
        s = s[p.len()..].trim_start();
    } else if lower.starts_with("start ") {
        s = s[6..].trim_start();
        if let Some(r) = s.strip_prefix("\"\"") {
            s = r.trim_start();
        }
    }
    // 引用符で囲んだときは, その後ろに何もないこと(引数の付いたコマンドは`cmd.exe`に渡す)
    if let Some(r) = s.strip_prefix('"') {
        let end = r.find('"')?;
        if !r[end + 1..].trim().is_empty() {
            return None;
        }
        s = &r[..end];
    }
    if s.contains('"') {
        return None;
    }
    match detect(s)? {
        Pasted::WinPath(p) => Some(p),
        _ => None,
    }
}

/// 入力がパスやBoxのURLの形なら, その種類を返す. 複数行のときは1行目だけを見る.
pub fn detect(input: &str) -> Option<Pasted> {
    let first = input.lines().find(|l| !l.trim().is_empty())?;
    let s = clean(first);
    if s.is_empty() || s.chars().count() < 3 {
        return None;
    }
    // file:///C:/... の形
    if let Some(rest) = s.strip_prefix("file:///") {
        let p = percent_decode(rest).replace('/', "\\");
        if is_drive_path(&p) {
            return Some(Pasted::WinPath(p));
        }
    }
    if let Some(rest) = s.strip_prefix("file://") {
        let p = format!("\\\\{}", percent_decode(rest).replace('/', "\\"));
        return Some(Pasted::WinPath(p));
    }
    if is_drive_path(s) {
        return Some(Pasted::WinPath(s.replace('/', "\\")));
    }
    if s.starts_with("\\\\") && s.len() > 3 {
        return Some(Pasted::WinPath(s.to_string()));
    }
    if s.starts_with('%') {
        if let Some(end) = s[1..].find('%') {
            let var = &s[1..1 + end];
            let after = &s[2 + end..];
            if !var.is_empty()
                && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '(' || c == ')')
                && (after.is_empty() || after.starts_with('\\') || after.starts_with('/'))
            {
                return Some(Pasted::WinPath(s.replace('/', "\\")));
            }
        }
    }
    if let Some(rest) = s.strip_prefix("/mnt/") {
        let mut it = rest.splitn(2, '/');
        let drive = it.next().unwrap_or("");
        if drive.len() == 1 && drive.as_bytes()[0].is_ascii_alphabetic() {
            let tail = it.next().unwrap_or("").replace('/', "\\");
            return Some(Pasted::WinPath(format!("{}:\\{}", drive.to_ascii_uppercase(), tail)));
        }
    }
    if (s.starts_with("/home/") || s.starts_with("~/") || s.starts_with("/etc/") || s.starts_with("/usr/") || s.starts_with("/opt/") || s.starts_with("/tmp/"))
        && !s.contains(char::is_whitespace)
    {
        return Some(Pasted::WslPath(s.to_string()));
    }
    if let Some(b) = box_url(s) {
        return Some(b);
    }
    let l = s.to_ascii_lowercase();
    if (l.starts_with("https://") || l.starts_with("http://")) && !s.contains(char::is_whitespace) && s.split_once("://").is_some_and(|(_, r)| !r.is_empty()) {
        return Some(Pasted::Url(s.to_string()));
    }
    None
}

/// `https://app.box.com/file/123`, `https://<会社>.app.box.com/folder/456`などを見分ける.
fn box_url(s: &str) -> Option<Pasted> {
    let rest = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    if !(host == "app.box.com" || host.ends_with(".app.box.com") || host == "box.com" || host.ends_with(".box.com")) {
        return None;
    }
    let mut parts = path.split(['/', '?', '#']);
    let kind = parts.next()?;
    let folder = match kind {
        "file" => false,
        "folder" => true,
        _ => return None,
    };
    let id: String = parts.next()?.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.is_empty() {
        return None;
    }
    Some(Pasted::BoxUrl { url: s.to_string(), folder, id })
}

/// WindowsのパスをWSLの形にする(`C:\a\b` → `/mnt/c/a/b`). Tabの`Copy WSL path`(D7)に使う.
pub fn to_wsl(win: &str) -> Option<String> {
    if is_drive_path(win) {
        let drive = win[..1].to_ascii_lowercase();
        let tail = win[3..].replace('\\', "/");
        return Some(if tail.is_empty() { format!("/mnt/{drive}") } else { format!("/mnt/{drive}/{tail}") });
    }
    // \\wsl.localhost\<distro>\home\... や \\wsl$\<distro>\...
    for pre in ["\\\\wsl.localhost\\", "\\\\wsl$\\"] {
        if let Some(rest) = win.strip_prefix(pre) {
            let (_, tail) = rest.split_once('\\').unwrap_or((rest, ""));
            return Some(format!("/{}", tail.replace('\\', "/")));
        }
    }
    None
}

/// WSLのパスをWindowsから開ける形にする(`/home/me/a` → `\\wsl.localhost\<distro>\home\me\a`).
pub fn wsl_to_unc(path: &str, distro: &str, home: Option<&str>) -> String {
    let p = match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(h)) => format!("{}/{}", h.trim_end_matches('/'), rest),
        _ => path.to_string(),
    };
    format!("\\\\wsl.localhost\\{}{}", distro, p.replace('/', "\\"))
}

/// ユーザーフォルダーの中のパスを`%USERPROFILE%\\…`の形にする(設定画面でフォルダーを選んだとき. K16).
/// 設定ファイルを別のPCに写してもそのまま使えるように.
pub fn contract_home(path: &str, home: &str) -> String {
    let home = home.trim_end_matches('\\');
    if home.is_empty() || path.len() < home.len() || !path.is_char_boundary(home.len()) {
        return path.to_string();
    }
    let (head, rest) = path.split_at(home.len());
    if head.to_lowercase() == home.to_lowercase() && (rest.is_empty() || rest.starts_with('\\')) {
        format!("%USERPROFILE%{rest}")
    } else {
        path.to_string()
    }
}

/// パスの最後の部分(ファイル名)と, その親のパス.
pub fn split_name(path: &str) -> (&str, &str) {
    let t = path.trim_end_matches(['\\', '/']);
    match t.rfind(['\\', '/']) {
        Some(i) => (&t[i + 1..], &t[..i]),
        None => (t, ""),
    }
}

/// パスの`\`の後ろで`Backspace`を押したときに入れる, 1つ上のフォルダー(K62). `C:\Users\me\`→`C:\Users\`.
/// 区切りで終わっていないとき, ドライブや`%APPDATA%\`, 共有フォルダーの根(`\\server\share\`)より上はNone
pub fn parent_dir_input(text: &str) -> Option<String> {
    if !text.ends_with(['\\', '/']) {
        return None;
    }
    let t = &text[..text.len() - 1];
    let i = t.rfind(['\\', '/'])?;
    let parent = &t[..=i];
    if parent.starts_with("\\\\") && parent[2..].matches('\\').count() < 2 {
        return None;
    }
    Some(parent.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_of_typed_folder() {
        assert_eq!(parent_dir_input(r"C:\Users\me\Documents\").as_deref(), Some(r"C:\Users\me\"));
        assert_eq!(parent_dir_input(r"C:\Users\").as_deref(), Some(r"C:\"));
        assert_eq!(parent_dir_input(r"C:\"), None);
        assert_eq!(parent_dir_input(r"%APPDATA%\Hikidashi\").as_deref(), Some(r"%APPDATA%\"));
        assert_eq!(parent_dir_input(r"%APPDATA%\"), None);
        assert_eq!(parent_dir_input(r"\\server\share\dir\").as_deref(), Some(r"\\server\share\"));
        assert_eq!(parent_dir_input(r"\\server\share\"), None);
        assert_eq!(parent_dir_input("C:/work/a/").as_deref(), Some("C:/work/"));
        // 区切りで終わっていなければ, ふつうの1文字の削除
        assert_eq!(parent_dir_input(r"C:\Users\me"), None);
    }

    fn win(s: &str) -> Pasted {
        Pasted::WinPath(s.into())
    }

    #[test]
    fn urls_open_in_browser() {
        assert_eq!(detect("https://example.box.com/s/abc123"), Some(Pasted::Url("https://example.box.com/s/abc123".into())));
        assert_eq!(detect("  <http://example.com/a?b=1>  "), Some(Pasted::Url("http://example.com/a?b=1".into())));
        assert!(matches!(detect("https://app.box.com/file/123"), Some(Pasted::BoxUrl { .. })));
        assert_eq!(detect("https://"), None);
        assert_eq!(detect("https://example.com/a b"), None);
    }

    #[test]
    fn contract_home_works() {
        assert_eq!(contract_home(r"C:\Users\me\Documents", r"C:\Users\me"), r"%USERPROFILE%\Documents");
        assert_eq!(contract_home(r"c:\users\ME", r"C:\Users\me"), "%USERPROFILE%");
        assert_eq!(contract_home(r"C:\Users\me2\x", r"C:\Users\me"), r"C:\Users\me2\x");
        assert_eq!(contract_home(r"D:\work", r"C:\Users\me"), r"D:\work");
        assert_eq!(contract_home(r"D:\work", ""), r"D:\work");
    }

    #[test]
    fn drive_paths() {
        assert_eq!(detect(r"C:\Users\me\報告書.docx"), Some(win(r"C:\Users\me\報告書.docx")));
        assert_eq!(detect(r#""C:\Users\me\a b.txt""#), Some(win(r"C:\Users\me\a b.txt")));
        assert_eq!(detect(r"<C:\Users\me\a.txt>"), Some(win(r"C:\Users\me\a.txt")));
        assert_eq!(detect(r"「C:\Users\me\a.txt」。"), Some(win(r"C:\Users\me\a.txt")));
        assert_eq!(detect("C:/Users/me/a.txt."), Some(win(r"C:\Users\me\a.txt")));
        assert_eq!(detect(r"  c:\temp  "), Some(win(r"c:\temp")));
    }

    #[test]
    fn unc_env_file_wsl() {
        assert_eq!(detect(r"\\server\share\a"), Some(win(r"\\server\share\a")));
        assert_eq!(detect(r"%USERPROFILE%\Box\a.xlsx"), Some(win(r"%USERPROFILE%\Box\a.xlsx")));
        assert_eq!(detect("file:///C:/Users/me/a%20b.txt"), Some(win(r"C:\Users\me\a b.txt")));
        assert_eq!(detect("/mnt/c/Users/me"), Some(win(r"C:\Users\me")));
        assert_eq!(detect("/home/me/project"), Some(Pasted::WslPath("/home/me/project".into())));
        assert_eq!(detect("~/project/work"), Some(Pasted::WslPath("~/project/work".into())));
    }

    #[test]
    fn not_paths() {
        assert_eq!(detect("pwpt"), None);
        assert_eq!(detect("how to rename a branch"), None);
        assert_eq!(detect("C:"), None);
        assert_eq!(detect("100%"), None);
        // Box以外のURLはBoxのURLではなく, ただのURL(ブラウザーで開く. B4)
        assert_eq!(detect("https://example.com/file/1"), Some(Pasted::Url("https://example.com/file/1".into())));
    }

    #[test]
    fn box_urls() {
        assert_eq!(
            detect("https://app.box.com/file/123456?s=abc"),
            Some(Pasted::BoxUrl { url: "https://app.box.com/file/123456?s=abc".into(), folder: false, id: "123456".into() })
        );
        assert!(matches!(detect("https://acme.app.box.com/folder/789"), Some(Pasted::BoxUrl { folder: true, .. })));
        // 会社のサブドメイン(`<会社>.ent.box.com`)の形も見分ける. 共有リンクはただのURL
        assert_eq!(
            detect("https://example.ent.box.com/file/2001234567890"),
            Some(Pasted::BoxUrl { url: "https://example.ent.box.com/file/2001234567890".into(), folder: false, id: "2001234567890".into() })
        );
        assert!(matches!(detect("https://example.ent.box.com/s/abc123"), Some(Pasted::Url(_))));
    }

    #[test]
    fn wsl_conversions() {
        assert_eq!(to_wsl(r"C:\Users\me\a.txt").as_deref(), Some("/mnt/c/Users/me/a.txt"));
        assert_eq!(to_wsl(r"\\wsl.localhost\Ubuntu\home\me").as_deref(), Some("/home/me"));
        assert_eq!(wsl_to_unc("/home/me/a", "Ubuntu", None), r"\\wsl.localhost\Ubuntu\home\me\a");
        assert_eq!(wsl_to_unc("~/a", "Ubuntu", Some("/home/me")), r"\\wsl.localhost\Ubuntu\home\me\a");
        assert_eq!(split_name(r"C:\a\b.txt"), ("b.txt", r"C:\a"));
    }

    #[test]
    fn command_that_only_opens_a_path() {
        let t = |c: &str| command_target(c);
        assert_eq!(t(r#"C:\Users\me\会議"#).as_deref(), Some(r"C:\Users\me\会議"));
        assert_eq!(t(r#""C:\Program Files\x""#).as_deref(), Some(r"C:\Program Files\x"));
        assert_eq!(t(r#"explorer "C:\Users\me\会議""#).as_deref(), Some(r"C:\Users\me\会議"));
        assert_eq!(t(r#"Explorer.exe C:\a b"#).as_deref(), Some(r"C:\a b"));
        assert_eq!(t(r#"start "" "%USERPROFILE%\見積.xlsx""#).as_deref(), Some(r"%USERPROFILE%\見積.xlsx"));
        assert_eq!(t(r#"start \\server\share\a"#).as_deref(), Some(r"\\server\share\a"));
        // 引数のあるもの, パスでないものは, cmd.exeに任せる
        assert_eq!(t(r#""C:\tool.exe" --flag"#), None);
        assert_eq!(t(r#"code "C:\Users\me\dotfile""#), None);
        assert_eq!(t(r#"explorer /select,"C:\a.txt""#), None);
        assert_eq!(t("notepad.exe"), None);
        assert_eq!(t("https://example.com"), None);
    }
}
