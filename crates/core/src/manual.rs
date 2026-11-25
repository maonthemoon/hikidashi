//! マニュアル(`docs\manual.html`)の場所(K46. wakudoriと同じく, 配った`docs`だけを開き, Webは見にいかない).

use std::path::{Path, PathBuf};

/// `dir`にexeがあるときのマニュアルの場所. ZIPではexeの隣の`docs`, リポジトリではexe(`bin`か`out`)の1つ上の`docs`
pub fn near(dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![dir.join("docs").join("manual.html")];
    if let Some(up) = dir.parent() {
        v.push(up.join("docs").join("manual.html"));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_of_places() {
        assert_eq!(near(Path::new("/repo/bin")), vec![PathBuf::from("/repo/bin/docs/manual.html"), PathBuf::from("/repo/docs/manual.html")]);
    }
}
