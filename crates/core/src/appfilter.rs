//! アプリの一覧に入れないショートカットの見分け方(C1, K30).
//! スタートメニューには, 自分で開くアプリでないもの(説明書, Webページ, インストーラー, スタートアップ)も多い.
//! 会社PCのアプリの一覧(191件)で, はっきりノイズだったものから決めた.

/// 名前でわかるもの(アンインストーラー, 説明など)
pub fn is_noise_name(name: &str) -> bool {
    let l = name.to_lowercase();
    ["uninstall", "アンインストール", "readme", "release notes", "website", "ウェブサイト", "ホームページ", "homepage", "help and support"]
        .iter()
        .any(|w| l.contains(w))
}

/// リンク先でわかるもの: 説明書・文書(`.chm`, `.pdf`, `.html`など), インストーラー(`setup.exe`, `\Uninstall\`の中)
pub fn is_noise_target(target: &str) -> bool {
    let l = target.to_lowercase().replace('/', "\\");
    let name = l.rsplit('\\').next().unwrap_or(&l);
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    if ["chm", "pdf", "html", "htm", "txt", "rtf", "hlp", "md", "doc", "docx", "xps", "url"].contains(&ext) {
        return true;
    }
    if ext == "exe" && (stem == "setup" || stem.starts_with("uninst") || l.contains("\\uninstall\\")) {
        return true;
    }
    false
}

/// スタートメニューの中のスタートアップ(Windowsの起動時に動くもの. 自分で開くものではない).
/// `rel`はスタートメニューの`Programs`からの相対パス
pub fn in_startup(rel: &str) -> bool {
    rel.replace('/', "\\").split('\\').next().is_some_and(|first| first.eq_ignore_ascii_case("startup"))
}

/// `.url`(インターネットのショートカット)の中身から, Webページへのリンクか(`URL=http...`)
pub fn url_file_is_web(content: &str) -> bool {
    content.lines().any(|line| {
        let t = line.trim();
        t.len() > 4 && t[..4].eq_ignore_ascii_case("url=") && {
            let u = t[4..].trim().to_ascii_lowercase();
            u.starts_with("http://") || u.starts_with("https://")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_and_installers_are_noise() {
        assert!(is_noise_target(r"C:\Program Files\7-Zip\7-zip.chm"));
        assert!(is_noise_target(r"C:\Program Files (x86)\Example\Keymap Tool\License_jpn.pdf"));
        assert!(is_noise_target(r"C:\Python314\Doc\html\index.html"));
        assert!(is_noise_target(r"C:\Program Files\ExampleCAD\v242\Win64\Uninstall\setup.exe"));
        assert!(is_noise_target(r"C:\ExampleDB\db12\oui\bin\setup.exe"));
        assert!(!is_noise_target(r"C:\Program Files\WezTerm\wezterm-gui.exe"));
        assert!(!is_noise_target(r"C:\WINDOWS\system32\services.msc"));
        assert!(!is_noise_target(""));
    }

    #[test]
    fn startup_and_names() {
        assert!(in_startup(r"Startup\電源設定.lnk"));
        assert!(in_startup(r"StartUp\NI Error Reporting.lnk"));
        assert!(!in_startup(r"Accessories\Notepad.lnk"));
        assert!(is_noise_name("Eulerのホームページ"));
        assert!(is_noise_name("Uninstall Foo"));
        assert!(!is_noise_name("Euler"));
    }

    #[test]
    fn web_url_files() {
        assert!(url_file_is_web("[InternetShortcut]\r\nURL=https://example.com/\r\n"));
        assert!(!url_file_is_web("[InternetShortcut]\nURL=steam://rungameid/1\n"));
    }
}
