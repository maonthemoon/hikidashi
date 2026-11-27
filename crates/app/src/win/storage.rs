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

/// マニュアル(`docs\manual.html`)の場所. なければNone
pub fn manual_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    hikidashi_core::manual::near(exe.parent()?).into_iter().find(|p| p.is_file())
}

/// 前の版が`%LOCALAPPDATA%\Hikidashi`に写したexeと, 一緒に置いたものを消す(K47). 名前を決めたものだけ.
/// ここで動いているときは消さない
pub fn remove_old_copy() {
    let dir = install_dir();
    let Ok(exe) = std::env::current_exe() else { return };
    if exe.parent().is_some_and(|d| d.to_string_lossy().eq_ignore_ascii_case(&dir.to_string_lossy())) || !dir.join("Hikidashi.exe").is_file() {
        return;
    }
    // 終わったばかりのexeは, 少しの間消せないことがある
    for _ in 0..20 {
        if std::fs::remove_file(dir.join("Hikidashi.exe")).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    for f in ["manual.html", "source.txt", r"docs\manual.html"] {
        let _ = std::fs::remove_file(dir.join(f));
    }
    // 空のときだけ消える
    let _ = std::fs::remove_dir(dir.join("docs"));
    // 写した辞書は, exeの隣に同じ大きさのものがあるときだけ
    let dict = dir.join("SKK-JISYO.L");
    let size = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).ok();
    if let (Some(a), Some(b)) = (size(&dict), exe.parent().and_then(|d| size(&d.join("SKK-JISYO.L")))) {
        if a == b {
            let _ = std::fs::remove_file(&dict);
        }
    }
    log("removed the old copy in LOCALAPPDATA");
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
            let mut c = Config::default();
            real_paths(&mut c);
            let t = config::to_toml(&c);
            write("config.toml", &t);
            t
        }
    };
    // メモ帳で保存すると先頭にBOMが付くことがある
    let text = text.trim_start_matches('\u{feff}');
    let (mut c, err) = match Config::parse(text) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(e)),
    };
    real_paths(&mut c);
    (c, err)
}

/// 設定画面に出すフォルダーの`%USERPROFILE%`などを, 実際のパス(`C:\Users\…`)にする(K72).
/// 設定画面で変えて書き直すと, 設定ファイルも実際のパスになる
fn real_paths(c: &mut Config) {
    for list in [&mut c.files.roots, &mut c.files.skip_dirs, &mut c.apps.extra_dirs] {
        for p in list.iter_mut() {
            *p = expand(p);
        }
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

/// 保存した結果(`snap `). 読めない・壊れていれば空として扱い, ログに書く
pub fn load_snapshots() -> hikidashi_core::snapshot::Snapshots {
    let Some(text) = read("snapshots.json") else { return Default::default() };
    let (s, err) = hikidashi_core::snapshot::Snapshots::from_json(&text);
    if let Some(e) = err {
        log(&format!("snapshots.json is broken, treated as empty: {e}"));
    }
    s
}

pub fn save_snapshots(s: &hikidashi_core::snapshot::Snapshots) {
    write("snapshots.json", &s.to_json());
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
