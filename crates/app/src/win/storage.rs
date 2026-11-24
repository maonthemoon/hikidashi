//! `%APPDATA%\Hikidashi`のファイル(設定, 状態, 学習, 履歴)と, 環境変数の展開.

use super::util::{from_wide, WStr};
use hikidashi_core::config::{self, Config, ConfigError};
use hikidashi_core::hotkey::State;
use hikidashi_core::learn::{History, Learn};
use std::path::PathBuf;
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;

/// `%USERPROFILE%`などを展開する
pub fn expand(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let src = WStr::new(s);
    let mut buf = vec![0u16; 2048];
    let n = unsafe { ExpandEnvironmentStringsW(src.pcwstr(), Some(&mut buf)) } as usize;
    if n == 0 {
        return s.to_string();
    }
    if n > buf.len() {
        buf = vec![0u16; n];
        unsafe { ExpandEnvironmentStringsW(src.pcwstr(), Some(&mut buf)) };
    }
    from_wide(&buf)
}

pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

/// 設定などを置くフォルダー(`%APPDATA%\Hikidashi`)
pub fn data_dir() -> PathBuf {
    let base = env("APPDATA");
    let p = if base.is_empty() { std::env::temp_dir() } else { PathBuf::from(base) };
    p.join("Hikidashi")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.toml")
}

fn read(name: &str) -> Option<String> {
    std::fs::read_to_string(data_dir().join(name)).ok()
}

/// 書き込みは一時ファイルに書いてから置き換える(途中で落ちても壊れないように)
fn write(name: &str, text: &str) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let tmp = dir.join(format!("{name}.tmp"));
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(name));
    }
}

/// 設定画面で保存する(K16). 一時ファイルに書いてから置き換える
pub fn write_config(text: &str) -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("config.toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join("config.toml")).map_err(|e| e.to_string())
}

/// 設定を読む. なければひな形を書く. 書き間違いがあれば初期値とエラーの文を返す.
pub fn load_config() -> (Config, Option<ConfigError>) {
    let r = load_config_inner();
    // 設定画面とトレイの言語(K24)
    super::i18n::set_language(&r.0.ui.language);
    // ランチャーの窓と書く窓のライトとダーク(K39)
    super::theme::set_theme(&r.0.ui.theme);
    r
}

fn load_config_inner() -> (Config, Option<ConfigError>) {
    let path = config_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            write("config.toml", &config::template());
            config::template()
        }
    };
    // メモ帳で保存すると先頭にBOMが付くことがある
    let text = text.trim_start_matches('\u{feff}');
    match Config::parse(text) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(e)),
    }
}

pub fn load_state() -> State {
    read("state.txt").map(|s| State::parse(&s)).unwrap_or_default()
}

pub fn save_state(s: &State) {
    write("state.txt", &s.to_text());
}

pub fn load_learn() -> Learn {
    read("learn.tsv").map(|s| Learn::from_text(&s)).unwrap_or_default()
}

pub fn save_learn(l: &Learn) {
    write("learn.tsv", &l.to_text());
}

pub fn load_history() -> History {
    read("history.txt").map(|s| History::from_text(&s)).unwrap_or_default()
}

pub fn save_history(h: &History) {
    write("history.txt", &h.to_text());
}

/// 長い質問の書きかけと送った文(K22)
pub fn load_drafts() -> hikidashi_core::compose::Drafts {
    read("compose.json").map(|s| hikidashi_core::compose::Drafts::from_json(&s)).unwrap_or_default()
}

pub fn save_drafts(d: &hikidashi_core::compose::Drafts) {
    write("compose.json", &d.to_json());
}

/// 動いている場所(`%LOCALAPPDATA%\Hikidashi`). リポジトリの`bin\Hikidashi.exe`を
/// ここに写してから動かすので, 動かしたまま`git pull`でexeを置き換えられる.
pub fn install_dir() -> PathBuf {
    let base = env("LOCALAPPDATA");
    let p = if base.is_empty() { std::env::temp_dir() } else { PathBuf::from(base) };
    p.join("Hikidashi")
}

pub fn log(msg: &str) {
    use std::io::Write;
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("hikidashi.log");
    // 大きくなりすぎたら作り直す
    if std::fs::metadata(&path).map(|m| m.len() > 512 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {}", super::util::now_secs(), msg);
    }
}
