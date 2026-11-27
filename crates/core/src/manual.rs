//! マニュアル(`docs\manual.html`)の場所(K46. wakudoriと同じく, 配った`docs`だけを開き, Webは見にいかない).

use crate::router::url_encode;
use std::path::{Path, PathBuf};

/// `dir`にexeがあるときのマニュアルの場所. ZIPではexeの隣の`docs`, リポジトリではexe(`bin`か`out`)の1つ上の`docs`
pub fn near(dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![dir.join("docs").join("manual.html")];
    if let Some(up) = dir.parent() {
        v.push(up.join("docs").join("manual.html"));
    }
    v
}

/// Windowsの絶対パスと節のIDから, ブラウザーに渡すfile URLを作る.
pub fn section_url(path: &Path, id: &str) -> Option<String> {
    let path = path.to_str()?.replace('\\', "/");
    let (unc, path) = if let Some(path) = path.strip_prefix("//?/") {
        (path.strip_prefix("UNC/"), path)
    } else {
        (path.strip_prefix("//"), path.as_str())
    };
    let (root, tail) = if let Some(unc) = unc {
        let (server, tail) = unc.split_once('/')?;
        (format!("file://{}/", url_encode(server)), tail)
    } else {
        let (drive, tail) = path.split_once('/')?;
        if !matches!(drive.as_bytes(), [b'A'..=b'Z' | b'a'..=b'z', b':']) {
            return None;
        }
        (format!("file:///{drive}/"), tail)
    };
    let tail = tail.split('/').map(url_encode).collect::<Vec<_>>().join("/");
    Some(format!("{root}{tail}#{}", url_encode(id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_of_places() {
        assert_eq!(near(Path::new("/repo/bin")), vec![PathBuf::from("/repo/bin/docs/manual.html"), PathBuf::from("/repo/docs/manual.html")]);
    }

    #[test]
    fn section_url_encodes_spaces() {
        assert_eq!(
            section_url(Path::new(r"C:\Users\me\My Apps\hikidashi\docs\manual.html"), "keys").as_deref(),
            Some("file:///C:/Users/me/My%20Apps/hikidashi/docs/manual.html#keys"),
        );
    }

    #[test]
    fn section_url_encodes_japanese_and_reserved_characters() {
        assert_eq!(
            section_url(Path::new(r"D:\日本語\#100%\docs\manual.html"), "keys").as_deref(),
            Some("file:///D:/%E6%97%A5%E6%9C%AC%E8%AA%9E/%23100%25/docs/manual.html#keys"),
        );
    }

    #[test]
    fn section_url_supports_extended_paths_and_unc() {
        assert_eq!(
            section_url(Path::new(r"\\?\C:\hikidashi\docs\manual.html"), "tab").as_deref(),
            Some("file:///C:/hikidashi/docs/manual.html#tab"),
        );
        for path in [r"\\server\My Apps\docs\manual.html", r"\\?\UNC\server\My Apps\docs\manual.html"] {
            assert_eq!(
                section_url(Path::new(path), "tab").as_deref(),
                Some("file://server/My%20Apps/docs/manual.html#tab"),
            );
        }
    }
}
