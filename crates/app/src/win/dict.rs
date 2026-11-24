//! ローマ字で日本語を探す辞書(SKK-JISYO.L. GPL)を用意する(K5, K31).
//! exeには入れない(GPLのため). ZIPにはexeとは別のファイルとして同梱し(K35), 起動したときにexeと一緒に
//! `%LOCALAPPDATA%\Hikidashi\SKK-JISYO.L`へ写す. そこを読み, なければexeの隣, それもなければネットから取ってくる.
//! 取ってくる間と取れなかったときは, ローマ字→ひらがな・カタカナだけで探す(辞書なしでも動く).

use super::indexer::{post_to, Shared, STATUS_DICT, WM_APP_INDEX};
use super::storage;
use hikidashi_core::migemo::Dict;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use windows::Win32::Foundation::HWND;
use windows::Win32::Globalization::{MultiByteToWideChar, MULTI_BYTE_TO_WIDE_CHAR_FLAGS};

/// 取ってくる元(SKK辞書の公開リポジトリ)
pub const HOST: &str = "raw.githubusercontent.com";
pub const PATH: &str = "/skk-dev/dict/master/SKK-JISYO.L";
pub const FILE_NAME: &str = "SKK-JISYO.L";
/// これより少なければ辞書ではないとみなす(SKK-JISYO.Lは約147,000語)
const MIN_ENTRIES: usize = 50_000;
/// EUC-JP(JIS X 0208と0212)
const CP_EUC_JP: u32 = 20932;

pub fn path() -> PathBuf {
    storage::install_dir().join(FILE_NAME)
}

/// exeの隣に同梱した辞書があれば, 写す先(`%LOCALAPPDATA%\Hikidashi`)へ写す. 大きさが同じなら写さない
pub fn copy_bundled(exe: &std::path::Path) {
    let Some(src) = exe.parent().map(|d| d.join(FILE_NAME)) else { return };
    let Ok(meta) = std::fs::metadata(&src) else { return };
    let dst = path();
    if std::fs::metadata(&dst).map(|m| m.len() == meta.len()).unwrap_or(false) {
        return;
    }
    match std::fs::read(&src) {
        Ok(bytes) => save(&dst, &bytes),
        Err(e) => storage::log(&format!("dictionary: could not read {} ({e})", src.display())),
    }
}

/// 裏で辞書を読む. なければ取ってきて保存する. 済んだら窓に知らせる(漢字でも探し直す)
pub fn start(hwnd: HWND, shared: Arc<Shared>, dict: Arc<OnceLock<Dict>>) {
    let h = hwnd.0 as isize;
    let _ = std::thread::Builder::new().name("dict".into()).spawn(move || {
        if let Some(d) = prepare(&shared) {
            let _ = dict.set(d);
        }
        post_to(h, WM_APP_INDEX);
    });
}

fn prepare(shared: &Shared) -> Option<Dict> {
    let p = path();
    // 写した先, exeの隣(写せずに元の場所で動いているとき)の順に読む
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(FILE_NAME)));
    for cand in std::iter::once(p.clone()).chain(beside.filter(|b| *b != p)) {
        let Ok(bytes) = std::fs::read(&cand) else { continue };
        match load(&bytes) {
            Ok(d) => {
                storage::log(&format!("dictionary: {} entries from {}", d.len(), cand.display()));
                return Some(d);
            }
            Err(e) => storage::log(&format!("dictionary: {} could not be used ({e})", cand.display())),
        }
    }
    shared.set_status(STATUS_DICT, "Downloading the dictionary for Japanese names…".into());
    let started = std::time::Instant::now();
    let got = super::net::get(HOST, PATH).and_then(|bytes| load(&bytes).map(|d| (bytes, d)));
    match got {
        Ok((bytes, d)) => {
            save(&p, &bytes);
            storage::log(&format!(
                "dictionary: downloaded {} bytes in {} ms, {} entries",
                bytes.len(),
                started.elapsed().as_millis(),
                d.len()
            ));
            shared.set_status(STATUS_DICT, String::new());
            Some(d)
        }
        Err(e) => {
            storage::log(&format!("dictionary: download failed ({e}). Put SKK-JISYO.L at {}", p.display()));
            shared.set_status(STATUS_DICT, "Couldn't download the dictionary, so romaji finds kana only. See Manual".into());
            None
        }
    }
}

/// 辞書の本文(EUC-JPのまま, または手で置いたUTF-8)を読む
fn load(bytes: &[u8]) -> Result<Dict, String> {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => decode_euc_jp(bytes)?,
    };
    let d = Dict::parse(&text);
    if d.len() < MIN_ENTRIES {
        return Err(format!("only {} entries", d.len()));
    }
    Ok(d)
}

fn decode_euc_jp(bytes: &[u8]) -> Result<String, String> {
    let flags = MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0);
    unsafe {
        let n = MultiByteToWideChar(CP_EUC_JP, flags, bytes, None);
        if n <= 0 {
            return Err(format!("EUC-JP: {}", windows::core::Error::from_thread()));
        }
        let mut buf = vec![0u16; n as usize];
        let n = MultiByteToWideChar(CP_EUC_JP, flags, bytes, Some(&mut buf));
        buf.truncate(n.max(0) as usize);
        Ok(String::from_utf16_lossy(&buf))
    }
}

/// 書きかけのファイルが残らないよう, 別の名前で書いてから置き換える
fn save(p: &PathBuf, bytes: &[u8]) {
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = p.with_extension("L.part");
    let ok = std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, p).is_ok();
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        storage::log(&format!("dictionary: could not save {}", p.display()));
    }
}
