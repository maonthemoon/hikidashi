//! マニュアル(`docs\manual.html`)の場所(K46. wakudoriと同じく, 配った`docs`だけを開き, Webは見にいかない).
//! exeは`%LOCALAPPDATA%\Hikidashi`に写して動くので, 写す前にあった場所(`source`)も探す.

use std::path::{Path, PathBuf};

/// `dir`にexeがあるときのマニュアルの場所. ZIPではexeの隣の`docs`, リポジトリではexe(`bin`か`out`)の1つ上の`docs`
pub fn near(dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![dir.join("docs").join("manual.html")];
    if let Some(up) = dir.parent() {
        v.push(up.join("docs").join("manual.html"));
    }
    v
}

/// 探す順: 写す前の場所の`docs`(`git pull`した新しいもの), 動いているexeの`docs`.
/// 写した先では, 1つ上(`%LOCALAPPDATA%`)は探さない
pub fn candidates(source: Option<&Path>, exe_dir: &Path, install_dir: &Path) -> Vec<PathBuf> {
    let mut v = source.map(near).unwrap_or_default();
    let here = near(exe_dir);
    v.extend(here.into_iter().take(if exe_dir == install_dir { 1 } else { 2 }));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_of_places() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(candidates(Some(&p("/repo/bin")), &p("/local/Hikidashi"), &p("/local/Hikidashi")), vec![p("/repo/bin/docs/manual.html"), p("/repo/docs/manual.html"), p("/local/Hikidashi/docs/manual.html")]);
        assert_eq!(candidates(None, &p("/zip"), &p("/local/Hikidashi")), vec![p("/zip/docs/manual.html"), p("/docs/manual.html")]);
    }
}
