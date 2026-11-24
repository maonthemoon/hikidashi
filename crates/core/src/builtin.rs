//! 最初から入っている項目: Chromeのブックマークの読み取り(E1, E2), Windowsの設定のページ(H5-2), Windowsの操作(H5-1, K7).

use crate::catalog::{Item, Kind};
use serde_json::Value;

/// ChromeのBookmarksファイル(JSON)からブックマークを取り出す. 名前だけで探す(E2)ので別名は付けない.
/// Chromeのプロフィールの一覧(フォルダー名, 表示名). `User Data\\Local State`のJSONから. Defaultを先に
pub fn chrome_profiles(local_state: &str) -> Vec<(String, String)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(local_state) else { return vec![] };
    let Some(cache) = v.pointer("/profile/info_cache").and_then(|c| c.as_object()) else { return vec![] };
    let mut out: Vec<(String, String)> =
        cache.iter().map(|(dir, info)| (dir.clone(), info.get("name").and_then(|n| n.as_str()).unwrap_or(dir).to_string())).collect();
    out.sort_by_key(|(dir, _)| (dir != "Default", dir.len(), dir.clone()));
    out
}

pub fn parse_bookmarks(json: &str) -> Vec<Item> {
    let mut out = Vec::new();
    let Ok(v) = serde_json::from_str::<Value>(json) else { return out };
    let Some(roots) = v.get("roots").and_then(Value::as_object) else { return out };
    for key in ["bookmark_bar", "other", "synced"] {
        if let Some(node) = roots.get(key) {
            let name = node.get("name").and_then(Value::as_str).unwrap_or(key).to_string();
            walk(node, &name, &mut out);
        }
    }
    out
}

fn walk(node: &Value, folder: &str, out: &mut Vec<Item>) {
    let Some(children) = node.get("children").and_then(Value::as_array) else { return };
    for c in children {
        let name = c.get("name").and_then(Value::as_str).unwrap_or("");
        match c.get("type").and_then(Value::as_str) {
            Some("url") => {
                let url = c.get("url").and_then(Value::as_str).unwrap_or("");
                if url.is_empty() || url.starts_with("javascript:") {
                    continue;
                }
                let shown = if name.trim().is_empty() { url } else { name };
                out.push(Item::new(Kind::Bookmark, shown, folder, url));
            }
            Some("folder") => walk(c, &format!("{folder} / {name}"), out),
            _ => {}
        }
    }
}

/// Windowsの設定のページ. 名前は英語(K1), 日本語の名前も一致に使う.
/// targetが`ms-settings:`や`windowsdefender:`ならURIとして開き, それ以外はコマンドとして実行する.
const SETTINGS: &[(&str, &str, &str, &str)] = &[
    // (名前, 一致に使う語(空白区切り), 開くもの, 引数)
    ("Display", "ディスプレイ 画面 monitor screen resolution 解像度", "ms-settings:display", ""),
    ("Night light", "夜間モード nightlight", "ms-settings:nightlight", ""),
    ("Sound", "サウンド 音 volume audio speaker スピーカー", "ms-settings:sound", ""),
    ("Notifications", "通知", "ms-settings:notifications", ""),
    ("Power & battery", "電源 バッテリー スリープ power sleep", "ms-settings:powersleep", ""),
    ("Storage", "ストレージ ディスク disk", "ms-settings:storagesense", ""),
    ("Multitasking", "マルチタスク snap スナップ", "ms-settings:multitasking", ""),
    ("Clipboard", "クリップボード", "ms-settings:clipboard", ""),
    ("Remote Desktop", "リモートデスクトップ rdp", "ms-settings:remotedesktop", ""),
    ("About this PC", "バージョン情報 PC名 about", "ms-settings:about", ""),
    ("Bluetooth & devices", "bluetooth ブルートゥース デバイス devices", "ms-settings:bluetooth", ""),
    ("Printers & scanners", "プリンター スキャナー printer", "ms-settings:printers", ""),
    ("Mouse", "マウス", "ms-settings:mousetouchpad", ""),
    ("Touchpad", "タッチパッド", "ms-settings:devices-touchpad", ""),
    ("Typing", "入力 キーボード keyboard", "ms-settings:typing", ""),
    ("AutoPlay", "自動再生", "ms-settings:autoplay", ""),
    ("Network & internet", "ネットワーク インターネット network", "ms-settings:network", ""),
    ("Wi-Fi", "wifi 無線", "ms-settings:network-wifi", ""),
    ("Ethernet", "イーサネット 有線 lan", "ms-settings:network-ethernet", ""),
    ("VPN", "vpn", "ms-settings:network-vpn", ""),
    ("Proxy", "プロキシ", "ms-settings:network-proxy", ""),
    ("Personalization", "個人用設定", "ms-settings:personalization", ""),
    ("Background", "背景 壁紙 wallpaper", "ms-settings:personalization-background", ""),
    ("Colors", "色 カラー ダーク dark", "ms-settings:colors", ""),
    ("Themes", "テーマ", "ms-settings:themes", ""),
    ("Lock screen", "ロック画面", "ms-settings:lockscreen", ""),
    ("Start menu", "スタート", "ms-settings:personalization-start", ""),
    ("Taskbar", "タスクバー", "ms-settings:taskbar", ""),
    ("Fonts", "フォント", "ms-settings:fonts", ""),
    ("Installed apps", "アプリ アンインストール uninstall programs", "ms-settings:appsfeatures", ""),
    ("Default apps", "既定のアプリ", "ms-settings:defaultapps", ""),
    ("Startup apps", "スタートアップ startup", "ms-settings:startupapps", ""),
    ("Optional features", "オプション機能", "ms-settings:optionalfeatures", ""),
    ("Your info", "アカウント account", "ms-settings:yourinfo", ""),
    ("Sign-in options", "サインイン pin 顔認証", "ms-settings:signinoptions", ""),
    ("Date & time", "日付と時刻 時計 time", "ms-settings:dateandtime", ""),
    ("Language & region", "言語 地域 language", "ms-settings:regionlanguage", ""),
    ("Japanese IME", "ime 日本語入力", "ms-settings:regionlanguage-jpnime", ""),
    ("Accessibility", "アクセシビリティ", "ms-settings:easeofaccess", ""),
    ("Text size", "文字のサイズ 文字サイズ", "ms-settings:easeofaccess-display", ""),
    ("Mouse pointer", "マウスポインター カーソル", "ms-settings:easeofaccess-mousepointer", ""),
    ("Privacy & security", "プライバシー", "ms-settings:privacy", ""),
    ("Camera privacy", "カメラ webcam", "ms-settings:privacy-webcam", ""),
    ("Microphone privacy", "マイク mic", "ms-settings:privacy-microphone", ""),
    ("Location", "位置情報", "ms-settings:privacy-location", ""),
    ("Windows Security", "セキュリティ defender ウイルス", "windowsdefender:", ""),
    ("Windows Update", "更新 アップデート update", "ms-settings:windowsupdate", ""),
    ("Recovery", "回復", "ms-settings:recovery", ""),
    ("For developers", "開発者 developer", "ms-settings:developers", ""),
    ("Control Panel", "コントロールパネル", "control.exe", ""),
    ("Device Manager", "デバイスマネージャー", "devmgmt.msc", ""),
    ("Environment variables", "環境変数 path env", "rundll32.exe", "sysdm.cpl,EditEnvironmentVariables"),
    ("Network connections", "ネットワーク接続 アダプター adapter ncpa", "ncpa.cpl", ""),
    ("Programs and Features", "プログラムと機能 appwiz", "appwiz.cpl", ""),
    ("Services", "サービス", "services.msc", ""),
    ("Task Manager", "タスクマネージャー taskmgr", "taskmgr.exe", ""),
    ("Disk Management", "ディスクの管理 diskmgmt", "diskmgmt.msc", ""),
    ("Event Viewer", "イベントビューアー eventvwr", "eventvwr.msc", ""),
    ("Registry Editor", "レジストリ regedit", "regedit.exe", ""),
    ("System properties", "システムのプロパティ sysdm", "sysdm.cpl", ""),
];

pub fn settings_items() -> Vec<Item> {
    SETTINGS
        .iter()
        .map(|(name, keys, target, args)| {
            let mut it = Item::new(Kind::Setting, *name, "Windows settings", *target);
            it.keys = keys.split_whitespace().map(str::to_string).collect();
            it.args = args.to_string();
            it
        })
        .collect()
}

/// Windowsの操作(H5-1, K7)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysAction {
    Lock,
    Sleep,
    EmptyRecycleBin,
    Restart,
    ShutDown,
    SignOut,
    /// Hikidashiの設定画面を開く(K16, G1)
    HikidashiSettings,
}

impl SysAction {
    /// `Enter`を2回押したときだけ実行するか(K7)
    pub fn needs_confirm(self) -> bool {
        matches!(self, SysAction::Restart | SysAction::ShutDown | SysAction::SignOut)
    }

    pub fn id(self) -> &'static str {
        match self {
            SysAction::Lock => "sys:lock",
            SysAction::Sleep => "sys:sleep",
            SysAction::EmptyRecycleBin => "sys:emptybin",
            SysAction::Restart => "sys:restart",
            SysAction::ShutDown => "sys:shutdown",
            SysAction::SignOut => "sys:signout",
            SysAction::HikidashiSettings => "sys:hikidashi-settings",
        }
    }

    pub fn from_id(id: &str) -> Option<SysAction> {
        Some(match id {
            "sys:lock" => SysAction::Lock,
            "sys:sleep" => SysAction::Sleep,
            "sys:emptybin" => SysAction::EmptyRecycleBin,
            "sys:restart" => SysAction::Restart,
            "sys:shutdown" => SysAction::ShutDown,
            "sys:signout" => SysAction::SignOut,
            "sys:hikidashi-settings" => SysAction::HikidashiSettings,
            _ => return None,
        })
    }

    /// 1回目の`Enter`のあとに出す文
    pub fn confirm_text(self) -> &'static str {
        match self {
            SysAction::Restart => "Press Enter again to restart",
            SysAction::ShutDown => "Press Enter again to shut down",
            SysAction::SignOut => "Press Enter again to sign out",
            _ => "",
        }
    }
}

pub fn system_items() -> Vec<Item> {
    let list: &[(SysAction, &str, &str, &str)] = &[
        (SysAction::Lock, "Lock", "Lock the screen", "ロック 画面ロック"),
        (SysAction::Sleep, "Sleep", "Put the PC to sleep", "スリープ"),
        (SysAction::EmptyRecycleBin, "Empty Recycle Bin", "Delete everything in the Recycle Bin", "ごみ箱 ゴミ箱 trash"),
        (SysAction::Restart, "Restart", "Press Enter twice", "再起動 reboot"),
        (SysAction::ShutDown, "Shut down", "Press Enter twice", "シャットダウン shutdown 電源を切る"),
        (SysAction::SignOut, "Sign out", "Press Enter twice", "サインアウト ログオフ logoff logout"),
        (SysAction::HikidashiSettings, "Hikidashi settings", "Open the settings window", "settings hikidashi config 設定"),
    ];
    list.iter()
        .map(|(a, name, sub, keys)| {
            let mut it = Item::new(Kind::System, *name, *sub, a.id());
            it.keys = keys.split_whitespace().map(str::to_string).collect();
            it
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_profiles_from_local_state() {
        let json = r#"{"profile":{"info_cache":{"Profile 10":{"name":"Z"},"Profile 2":{"name":"Work"},"Default":{"name":"Person 1"}}}}"#;
        let p = chrome_profiles(json);
        let dirs: Vec<&str> = p.iter().map(|(d, _)| d.as_str()).collect();
        assert_eq!(dirs, ["Default", "Profile 2", "Profile 10"]);
        assert_eq!(p[1].1, "Work");
        assert!(chrome_profiles("{}").is_empty());
    }

    #[test]
    fn bookmarks() {
        let json = r#"{"roots":{"bookmark_bar":{"name":"Bookmarks bar","type":"folder","children":[
            {"type":"url","name":"GitHub","url":"https://github.com/"},
            {"type":"folder","name":"Work","children":[{"type":"url","name":"Jira ボード","url":"https://jira.example.com/"}]},
            {"type":"url","name":"","url":"https://example.com/"},
            {"type":"url","name":"bm","url":"javascript:alert(1)"}]},
            "other":{"name":"Other bookmarks","type":"folder","children":[]}}}"#;
        let b = parse_bookmarks(json);
        assert_eq!(b.len(), 3);
        assert_eq!(b[1].name, "Jira ボード");
        assert_eq!(b[1].sub, "Bookmarks bar / Work");
        assert_eq!(b[2].name, "https://example.com/");
        assert!(parse_bookmarks("not json").is_empty());
    }

    #[test]
    fn builtins() {
        assert!(settings_items().iter().any(|s| s.name == "Display" && s.keys.contains(&"ディスプレイ".to_string())));
        let sys = system_items();
        assert_eq!(sys.len(), 7);
        assert_eq!(SysAction::from_id(&sys[6].target), Some(SysAction::HikidashiSettings));
        assert!(SysAction::from_id(&sys[4].target).unwrap().needs_confirm());
        assert!(!SysAction::Lock.needs_confirm());
    }
}

/// `%APPDATA%`などのフォルダー(2026-11-24). `%app`や`appdata`と打っても出す.
/// `vars`は(環境変数の名前, 中身). 画面の側で, あるフォルダーだけを渡す. 名前はよく書く形(`%ProgramFiles%`など)で出す
pub fn folder_items(vars: &[(String, String)]) -> Vec<Item> {
    vars.iter()
        .map(|(name, path)| {
            let shown = FOLDER_VARS.iter().find(|v| v.eq_ignore_ascii_case(name)).copied().unwrap_or(name.as_str());
            Item::new(Kind::Folder, &format!("%{shown}%"), path, path).with_keys(&[&name.to_lowercase()])
        })
        .collect()
}

/// 一覧に出す環境変数(フォルダーを指すもの)
pub const FOLDER_VARS: [&str; 10] = ["APPDATA", "LOCALAPPDATA", "USERPROFILE", "TEMP", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "PUBLIC", "OneDrive", "WINDIR"];
