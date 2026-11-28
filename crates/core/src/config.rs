//! 設定ファイル(`%APPDATA%\Hikidashi\config.toml`. K4). 書いていない項目は初期値を使う.

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub prefix: Prefix,
    /// アプリの別名(J6). `ppt = "PowerPoint"`
    pub alias: BTreeMap<String, String>,
    /// 自作コマンド(H2)
    pub command: Vec<Command>,
    pub apps: Apps,
    pub files: Files,
    pub wsl: Wsl,
    /// ページを開くブラウザー(既定のブラウザー, Chrome, Edge)とブックマーク
    pub browser: Browser,
    /// 前の版の`[chrome]`. 読んだら`browser`に移す(書くときは`[browser]`だけ)
    pub chrome: Option<Chrome>,
    pub chatgpt: ChatGpt,
    pub google: Google,
    /// Enterに修飾キーを足したときにすること(2026-11-24)
    pub keys: Keys,
    pub tools: Tools,
    /// 操作のメニュー(→)の「〜で開く」(Q5). 入っているものだけ出す
    pub open_with: Vec<OpenWith>,
    /// 操作のメニュー(→)に出すもの(種類ごと. K24)
    pub menu: Menu,
    /// 設定画面とトレイの言語(K24)
    pub ui: Ui,
    /// 検索まわりの入れ切り(最近のもの, クリップボードの履歴)
    pub search: Search,
    /// メインのほかのホットキー(選んだ文字で開く)
    pub hotkeys: Hotkeys,
    /// 拡張ごとの設定(`[ext.mipi] masked_write = "…"`. K52). 中身は拡張に渡すだけ
    pub ext: BTreeMap<String, BTreeMap<String, String>>,
}

/// 検索まわりの入れ切り
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Search {
    /// 入力欄が空のとき, 最近開いたものを出す(初めはオフ. K43)
    pub recent: bool,
    /// クリップボードの履歴を記録する(`cb `で呼ぶ)
    pub clipboard_history: bool,
}

impl Default for Search {
    fn default() -> Self {
        Search { recent: false, clipboard_history: true }
    }
}

/// メインのホットキー(`state.txt`)のほかのホットキー. 空なら使わない
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Hotkeys {
    /// 選んでいる文字で窓を開く
    pub selection: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys { selection: "Ctrl+Alt+Space".into() }
    }
}

/// 操作のメニュー(→)の項目の名前(K24). 並び順はこの順
pub const MENU_ITEMS: [&str; 10] = ["copy_file", "new_window", "open_with", "reveal", "copy_path", "copy_wsl_path", "admin", "explorer", "choose_app", "mail"];

/// 操作のメニュー(→)に出すもの(K24). 種類ごとに`MENU_ITEMS`の名前を並べる. 種類に合わないものは出さない
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Menu {
    pub app: Vec<String>,
    pub file: Vec<String>,
    pub folder: Vec<String>,
}

impl Default for Menu {
    fn default() -> Self {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect();
        Menu {
            app: v(&["new_window", "reveal", "copy_path", "admin"]),
            file: v(&["copy_file", "reveal", "copy_path", "copy_wsl_path", "choose_app", "mail"]),
            folder: v(&["copy_file", "copy_path", "copy_wsl_path"]),
        }
    }
}

impl Menu {
    /// その種類(`app`, `file`, `folder`)で項目を出すか
    pub fn has(&self, kind: &str, item: &str) -> bool {
        let list = match kind {
            "app" => &self.app,
            "file" => &self.file,
            _ => &self.folder,
        };
        list.iter().any(|s| s == item)
    }
}

/// 種類ごとに意味のある項目(K24). 設定画面の表の「—」はここにないもの
pub fn menu_item_fits(kind: &str, item: &str) -> bool {
    match item {
        "new_window" => kind == "app",
        "copy_file" => kind != "app",
        "open_with" | "copy_wsl_path" => kind != "app",
        // 「プログラムから開く」「メールに添付」はファイルだけ
        "choose_app" | "mail" => kind == "file",
        "admin" => kind != "folder",
        "explorer" => kind == "folder",
        _ => true,
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    /// `auto`(Windowsの言語), `ja`, `en`
    pub language: String,
    /// ランチャーの窓と書く窓の色(K39): `auto`(Windowsのアプリのモード), `light`, `dark`
    pub theme: String,
}

impl Default for Ui {
    fn default() -> Self {
        Ui { language: "auto".into(), theme: "auto".into() }
    }
}

/// 「〜で開く」の1つ(Q5)
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpenWith {
    /// メニューに出す名前(`Open in <name>`)
    pub name: String,
    /// exeの場所. 空なら名前から探す(VS Code, WezTerm, Windows Terminal)
    #[serde(default)]
    pub exe: String,
    /// 渡すもの. `{path}`はファイルかフォルダー, `{dir}`はフォルダー. 空ならアプリごとのいつもの渡し方
    #[serde(default)]
    pub args: String,
}

/// 種類を絞る接頭辞(B1, K3). 先頭にこの文字とスペースを打つ.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Prefix {
    pub app: String,
    pub file: String,
    pub bookmark: String,
    pub kill: String,
    /// WSLのホームだけを探す(K24. 前の版の窓の切り替え`window`は読んでこれに移す)
    #[serde(alias = "window")]
    pub wsl: String,
    /// Boxだけを探す(K20). Boxは件数が多いので, ふだんの検索には混ぜない. `box`はRustの予約語なので名前を変える
    #[serde(rename = "box")]
    pub boxdrive: String,
    /// Google検索(2026-11-24. `g 天気`でEnter)
    pub google: String,
    /// すぐ長い質問の窓にする(K24)
    pub chatgpt: String,
    /// クリップボードの履歴(`cb `)
    pub clipboard: String,
    /// 計算(2026-11-25. `= 0x1F`). 計算の行はこの接頭辞のときだけ出す
    pub calc: String,
    /// 保存した結果(K62. `snap `)
    pub snapshot: String,
    /// Outlookの予定(K62. `cal `)
    pub calendar: String,
}

/// Enterに修飾キーを足したときにすること. 値は`Ctrl+Enter`のような形. 空ならその操作はキーでは行わない.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Keys {
    /// 一致があっても, 打った文字をChatGPTに聞く
    pub chatgpt: String,
    /// 打った文字をGoogleで検索する
    pub google: String,
    /// 改行して, 長い質問を書く窓にする(K22)
    pub write: String,
    /// 管理者として実行する(C2)
    pub admin: String,
    /// ファイルやフォルダーをターミナルで開く(Q6)
    pub terminal: String,
}

impl Default for Keys {
    fn default() -> Self {
        Keys {
            chatgpt: "Ctrl+Enter".into(),
            google: "Ctrl+Alt+Enter".into(),
            write: "Shift+Enter".into(),
            admin: "Ctrl+Shift+Enter".into(),
            terminal: "Alt+Enter".into(),
        }
    }
}

/// Enterに足す修飾キー
pub const KEY_CTRL: u8 = 1;
pub const KEY_SHIFT: u8 = 2;
pub const KEY_ALT: u8 = 4;

/// `Ctrl+Alt+Enter`を修飾キーの組にする. Enterで終わらない, 修飾キーがない, 知らない語があればNone
pub fn parse_enter_key(s: &str) -> Option<u8> {
    let mut mods = 0u8;
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let (last, rest) = parts.split_last()?;
    if !last.eq_ignore_ascii_case("enter") {
        return None;
    }
    for p in rest {
        mods |= match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => KEY_CTRL,
            "shift" => KEY_SHIFT,
            "alt" => KEY_ALT,
            _ => return None,
        };
    }
    (mods != 0).then_some(mods)
}

/// 修飾キーの組を`Ctrl+Alt+Enter`の形にする(Ctrl, Shift, Altの順)
pub fn format_enter_key(mods: u8) -> String {
    let mut s = String::new();
    for (bit, name) in [(KEY_CTRL, "Ctrl+"), (KEY_SHIFT, "Shift+"), (KEY_ALT, "Alt+")] {
        if mods & bit != 0 {
            s.push_str(name);
        }
    }
    s + "Enter"
}

/// 選べるキーの組(設定画面の選び. 空は「使わない」)
pub const ENTER_KEY_CHOICES: [&str; 8] = ["", "Ctrl+Enter", "Shift+Enter", "Alt+Enter", "Ctrl+Shift+Enter", "Ctrl+Alt+Enter", "Alt+Shift+Enter", "Ctrl+Alt+Shift+Enter"];

impl Keys {
    /// キーの組に当たる操作
    pub fn action_of(&self, mods: u8) -> Option<KeyAction> {
        [
            (&self.chatgpt, KeyAction::ChatGpt),
            (&self.google, KeyAction::Google),
            (&self.write, KeyAction::Write),
            (&self.admin, KeyAction::Admin),
            (&self.terminal, KeyAction::Terminal),
        ]
        .into_iter()
        .find(|(k, _)| parse_enter_key(k) == Some(mods))
        .map(|(_, a)| a)
    }

    /// その操作のキー(案内に出す). 使わないなら空
    pub fn label(&self, a: KeyAction) -> String {
        let k = match a {
            KeyAction::ChatGpt => &self.chatgpt,
            KeyAction::Google => &self.google,
            KeyAction::Write => &self.write,
            KeyAction::Admin => &self.admin,
            KeyAction::Terminal => &self.terminal,
        };
        parse_enter_key(k).map(format_enter_key).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    ChatGpt,
    Google,
    Write,
    Admin,
    Terminal,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Command {
    /// 打つ名前(例: `jira`)
    pub name: String,
    /// 一覧に出す名前. 省略すると`name`
    #[serde(default)]
    pub title: Option<String>,
    /// URLのひな形. `{q}`に名前の後ろの文字が入る(例: `jira ABC-123`)
    #[serde(default)]
    pub url: Option<String>,
    /// 実行するコマンド(例: `code "C:\Users\me\dotfile"`). `{q}`も使える
    #[serde(default)]
    pub run: Option<String>,
    /// 開くファイル(K71. 例: `C:\Users\me\Documents\見積.xlsx`)
    #[serde(default)]
    pub file: Option<String>,
    /// 開くフォルダー(K71)
    #[serde(default)]
    pub folder: Option<String>,
}

/// エイリアスの種類(K44, K71). 番号は設定画面の`Kind`の並び(0はアプリの呼び名)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Url = 1,
    Run = 2,
    File = 3,
    Folder = 4,
}

impl Command {
    /// 中身のある欄の種類と値. どれもなければ`None`
    pub fn target(&self) -> Option<(CommandKind, &str)> {
        [(CommandKind::Url, &self.url), (CommandKind::Run, &self.run), (CommandKind::File, &self.file), (CommandKind::Folder, &self.folder)]
            .into_iter()
            .find_map(|(k, v)| v.as_deref().filter(|s| !s.trim().is_empty()).map(|v| (k, v)))
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Apps {
    /// exeをアプリとして扱うフォルダー(C1. ポータブルアプリ置き場など)
    pub extra_dirs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Files {
    /// 走査するフォルダー(D1). `%USERPROFILE%`などの環境変数が使える
    pub roots: Vec<String>,
    /// 除く名前(フォルダー名もファイル名も. 大文字小文字は区別しない)
    pub exclude: Vec<String>,
    /// 除くフォルダー(場所で指定. K16). `%USERPROFILE%\Documents\old`など
    pub skip_dirs: Vec<String>,
    /// Boxのフォルダー(D2). 空ならBox Driveのデータ(`sync_root_folder.txt`)から自動で決める(K18).
    /// ユーザーフォルダーの走査からは外し, 中身は`sync.db`から読む
    pub box_dir: String,
    /// Box Driveのデータ(`sync.db`)の場所(K18). 空なら`%LOCALAPPDATA%\Box\Box\data`の下を探す
    pub box_db: String,
    /// 前の版の間隔(分). 読むだけで使わない(K24. Boxは`sync.db`の日時を見て, WSLは`w `のときに読み直す)
    pub refresh_minutes: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Wsl {
    /// 前の版の「使うかどうか」. 読むだけで使わない(K44. `false`なら`[prefix] wsl`を空にする)
    pub enabled: bool,
    /// ディストリ名. 空なら既定のディストリ. 設定画面には出さない(K44)
    pub distro: String,
    /// 前の版のWSL側で除く名前. 読むだけで使わない(K44. `[files] exclude`に足す)
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Browser {
    /// `default`: Windowsの既定のブラウザー, `chrome`, `edge`
    pub browser: String,
    /// `window`: 新しい窓で開く, `tab`: 新しいタブで開く
    pub open_in: String,
    /// chrome.exeの場所. 空なら探す
    pub chrome_exe: String,
    /// ChromeのUser Data. 空なら`%LOCALAPPDATA%\Google\Chrome\User Data`
    pub chrome_user_data: String,
    /// Chromeのプロフィールのフォルダー名
    pub chrome_profile: String,
    /// msedge.exeの場所. 空なら探す
    pub edge_exe: String,
    /// Edgeのプロフィールのフォルダー名
    pub edge_profile: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Chrome {
    /// chrome.exeの場所. 空なら探す
    pub exe: String,
    /// User Dataのフォルダー. 空なら`%LOCALAPPDATA%\Google\Chrome\User Data`
    pub user_data: String,
    /// プロフィールのフォルダー名(E1)
    pub profile: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ChatGpt {
    /// `url`: `?q=`で開いて自動送信. `clipboard`: 質問をコピーしてChatGPTを開く(F2)
    pub mode: String,
    pub url: String,
    pub home: String,
    /// `?q=`で開いたあと, 前の窓のタイトルにChatGPTが入っていればEnterを送る(Q4)
    pub send_enter: bool,
    /// `?q=`の送信ボタンが見つからないとき, ChatGPTが前に出てからEnterまで待つ時間(ミリ秒). 前に出るまでの上限はこれに10秒を足す
    pub send_delay_ms: u32,
    /// 画像を貼ったあと, 送信ボタンが見つからないときの待ち(1枚あたりのミリ秒). ボタンの上限は枚数分に5秒を足す
    pub image_wait_ms: u32,
    /// 長い質問を書く窓(K22)で, カーソルのない行のMarkdownの記号を隠す. falseなら薄く見せる
    pub hide_marks: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Google {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Tools {
    /// 前の版のVS Codeの場所. 読んだら`[[open_with]]`に移す(書かない)
    pub vscode: String,
    /// 前の版のWezTermの場所. 読んだら`[[open_with]]`に移す(書かない)
    pub wezterm: String,
    /// `Alt+Enter`で入るターミナル(`[[open_with]]`の名前. Q6). 空ならWezTerm, Windows Terminalの順で入っているもの
    pub terminal: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prefix: Prefix::default(),
            alias: BTreeMap::new(),
            command: vec![],
            apps: Apps::default(),
            files: Files::default(),
            wsl: Wsl::default(),
            browser: Browser::default(),
            chrome: None,
            chatgpt: ChatGpt::default(),
            google: Google::default(),
            keys: Keys::default(),
            tools: Tools::default(),
            open_with: crate::openwith::defaults(),
            menu: Menu::default(),
            ui: Ui::default(),
            search: Search::default(),
            hotkeys: Hotkeys::default(),
            ext: BTreeMap::new(),
        }
    }
}

impl Default for Prefix {
    fn default() -> Self {
        Prefix { app: "a".into(), file: "f".into(), bookmark: "b".into(), kill: "k".into(), wsl: "w".into(), boxdrive: "bx".into(), google: "g".into(), chatgpt: "c".into(), clipboard: "cb".into(), calc: "=".into(), snapshot: "snap".into(), calendar: "cal".into() }
    }
}

impl Default for Apps {
    fn default() -> Self {
        Apps { extra_dirs: vec![] }
    }
}

impl Default for Files {
    fn default() -> Self {
        Files {
            roots: vec!["%USERPROFILE%".into()],
            exclude: [
                // `target`, `obj`は入れない(自分で作った同じ名前のフォルダーまで外れるため. K32)
                "AppData", "node_modules", ".git", ".svn", ".hg", "__pycache__", ".venv", "venv", ".cache", ".npm", ".cargo", ".rustup", ".local",
                "$RECYCLE.BIN", "System Volume Information", "NTUSER.DAT", "ntuser.dat.LOG1", "ntuser.dat.LOG2", ".vs",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            skip_dirs: vec![],
            box_dir: String::new(),
            box_db: String::new(),
            refresh_minutes: 30,
        }
    }
}

impl Default for Wsl {
    fn default() -> Self {
        Wsl {
            enabled: true,
            distro: String::new(),
            exclude: vec![],
        }
    }
}

impl Default for Browser {
    fn default() -> Self {
        Browser {
            browser: "default".into(),
            open_in: "window".into(),
            chrome_exe: String::new(),
            chrome_user_data: String::new(),
            chrome_profile: "Default".into(),
            edge_exe: String::new(),
            edge_profile: "Default".into(),
        }
    }
}

impl Default for Chrome {
    fn default() -> Self {
        Chrome { exe: String::new(), user_data: String::new(), profile: "Default".into() }
    }
}

impl Default for ChatGpt {
    fn default() -> Self {
        ChatGpt { mode: "url".into(), url: "https://chatgpt.com/?q={q}".into(), home: "https://chatgpt.com/".into(), send_enter: true, send_delay_ms: 5000, image_wait_ms: 3000, hide_marks: true }
    }
}

impl Default for Google {
    fn default() -> Self {
        Google { url: "https://www.google.com/search?q={q}".into() }
    }
}

impl Default for Tools {
    fn default() -> Self {
        Tools { vscode: String::new(), wezterm: String::new(), terminal: String::new() }
    }
}

/// 読み取りの失敗. 画面に出す文(英語. K1)
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigError(pub String);

impl Config {
    pub fn parse(src: &str) -> Result<Config, ConfigError> {
        let mut c = toml::from_str::<Config>(src).map_err(|e| {
            let line = e.span().map(|s| src.get(..s.start.min(src.len())).unwrap_or(src).matches('\n').count() + 1);
            let msg = e.message().to_string();
            ConfigError(match line {
                Some(l) => format!("config.toml line {l}: {msg}"),
                None => format!("config.toml: {msg}"),
            })
        })?;
        c.migrate();
        Ok(c)
    }

    /// 前の版の`[chrome]`を`[browser]`に移す. 開くブラウザーは既定のブラウザーのまま(ユーザーの指示)
    fn migrate(&mut self) {
        // `Share…`は消した(K68). 前の版の設定ファイルの`share`は読み飛ばす
        for l in [&mut self.menu.app, &mut self.menu.file, &mut self.menu.folder] {
            l.retain(|i| i != "share");
        }
        // WSLのオンオフはキーワードが空かどうか, 除く名前はWindowsと1つにした(K44)
        if !std::mem::replace(&mut self.wsl.enabled, true) {
            self.prefix.wsl.clear();
        }
        for n in std::mem::take(&mut self.wsl.exclude) {
            if !self.files.exclude.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                self.files.exclude.push(n);
            }
        }
        // 前の版の`[tools] vscode`, `wezterm`を「〜で開く」の一覧に移す
        for (old, kind) in [(std::mem::take(&mut self.tools.vscode), crate::openwith::ToolKind::VsCode), (std::mem::take(&mut self.tools.wezterm), crate::openwith::ToolKind::WezTerm)] {
            if old.trim().is_empty() {
                continue;
            }
            match self.open_with.iter_mut().find(|o| crate::openwith::ToolKind::of(o) == kind) {
                Some(o) if o.exe.is_empty() => o.exe = old,
                Some(_) => {}
                None => self.open_with.push(OpenWith { name: if kind == crate::openwith::ToolKind::VsCode { "VS Code" } else { "WezTerm" }.into(), exe: old, args: String::new() }),
            }
        }
        let Some(old) = self.chrome.take() else { return };
        let b = &mut self.browser;
        if b.chrome_exe.is_empty() {
            b.chrome_exe = old.exe;
        }
        if b.chrome_user_data.is_empty() {
            b.chrome_user_data = old.user_data;
        }
        if b.chrome_profile == "Default" {
            b.chrome_profile = old.profile;
        }
    }

    /// 接頭辞とその種類. 長いものから試す
    pub fn prefixes(&self) -> Vec<(String, crate::router::Mode)> {
        use crate::router::Mode;
        let mut v = vec![
            (self.prefix.app.clone(), Mode::App),
            (self.prefix.file.clone(), Mode::File),
            (self.prefix.bookmark.clone(), Mode::Bookmark),
            (self.prefix.kill.clone(), Mode::Kill),
            (self.prefix.wsl.clone(), Mode::Wsl),
            (self.prefix.boxdrive.clone(), Mode::Box),
            (self.prefix.google.clone(), Mode::Google),
            (self.prefix.chatgpt.clone(), Mode::ChatGpt),
            (self.prefix.clipboard.clone(), Mode::Clipboard),
            (self.prefix.calc.clone(), Mode::Calc),
            (self.prefix.snapshot.clone(), Mode::Snapshot),
            (self.prefix.calendar.clone(), Mode::Calendar),
        ];
        v.retain(|(p, _)| !p.is_empty());
        v.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
        v
    }
}

/// 初めて起動したときに作る設定ファイル(初期値を書き出したもの)
pub fn template() -> String {
    to_toml(&Config::default())
}

/// 文字列をTOMLの文字列にする. Windowsのパスの`\`をそのまま書けるよう, なるべく'...'にする
fn q(s: &str) -> String {
    if !s.contains('\'') && !s.chars().any(char::is_control) {
        format!("'{s}'")
    } else {
        toml::Value::String(s.to_string()).to_string()
    }
}

fn list(v: &[String]) -> String {
    let items: Vec<String> = v.iter().map(|s| q(s)).collect();
    let one = format!("[{}]", items.join(", "));
    if one.len() <= 100 {
        return one;
    }
    let mut o = String::from("[\n");
    for it in items {
        o.push_str(&format!("  {it},\n"));
    }
    o.push(']');
    o
}

fn key(k: &str) -> String {
    if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        k.to_string()
    } else {
        q(k)
    }
}

/// 設定を設定ファイルの文にする(設定画面の保存. K16). 説明の行は毎回付け直す(Q7. 手で書いた`#`の行は残らない).
pub fn to_toml(c: &Config) -> String {
    let mut o = String::new();
    let mut line = |s: &str| {
        o.push_str(s);
        o.push('\n');
    };
    line("# Hikidashi settings. Change them from Settings in the tray menu, or edit this file.");
    line("# Save this file and Hikidashi reloads it right away. Lines starting with # are comments.");
    line("# Saving from the Settings window rewrites this file, so your own comments are not kept.");
    line("");
    if c.open_with.is_empty() {
        // 一覧を全部消したとき(書かないと初めの一覧に戻るため)
        line("open_with = []");
        line("");
    }
    line("# Keywords to search only one kind. Type the letter and a space, e.g. \"f report\". Leave one empty to turn it off.");
    line("# Box is searched only with its keyword, e.g. \"bx report\" (it has too many files to mix in).");
    line("# wsl searches the home folder in WSL. chatgpt opens the window for a long question right away.");
    line("# clipboard shows the clipboard history.");
    line("# calc is the only place where calculations are shown, e.g. \"= 0x1F\", \"= 10dBm\", \"= (12+3)*4\".");
    line("[prefix]");
    let p = &c.prefix;
    for (k, v) in [("app", &p.app), ("file", &p.file), ("bookmark", &p.bookmark), ("kill", &p.kill), ("wsl", &p.wsl), ("box", &p.boxdrive), ("google", &p.google), ("chatgpt", &p.chatgpt), ("clipboard", &p.clipboard), ("calc", &p.calc), ("snapshot", &p.snapshot), ("calendar", &p.calendar)] {
        line(&format!("{k} = {}", q(v)));
    }
    line("");
    line("# Extra names for apps. Type the left side to find the app on the right.");
    line("[alias]");
    if c.alias.is_empty() {
        line("# ppt = \"PowerPoint\"");
    }
    for (k, v) in &c.alias {
        line(&format!("{} = {}", key(k), q(v)));
    }
    line("");
    line("# Your own commands. \"url\" opens a web page ({q} is what you type after the name).");
    line("# \"run\" runs a command. \"file\" and \"folder\" open that file or folder.");
    if c.command.is_empty() {
        line("# [[command]]");
        line("# name = \"jira\"");
        line("# title = \"Open Jira issue\"");
        line("# url = \"https://jira.example.com/browse/{q}\"");
        line("#");
        line("# [[command]]");
        line("# name = \"dot\"");
        line("# title = \"Open dotfile in VS Code\"");
        line("# run = 'code \"C:\\Users\\me\\project\\work\\dotfile\"'");
    }
    for cmd in &c.command {
        line("[[command]]");
        line(&format!("name = {}", q(&cmd.name)));
        if let Some(t) = &cmd.title {
            line(&format!("title = {}", q(t)));
        }
        if let Some(u) = &cmd.url {
            line(&format!("url = {}", q(u)));
        }
        if let Some(r) = &cmd.run {
            line(&format!("run = {}", q(r)));
        }
        if let Some(f) = &cmd.file {
            line(&format!("file = {}", q(f)));
        }
        if let Some(f) = &cmd.folder {
            line(&format!("folder = {}", q(f)));
        }
        line("");
    }
    if c.command.is_empty() {
        line("");
    }
    line("[apps]");
    line("# Folders whose .exe files are listed as apps (portable apps, tools).");
    line(&format!("extra_dirs = {}", list(&c.apps.extra_dirs)));
    line("");
    line("[files]");
    line("# Folders to search.");
    line(&format!("roots = {}", list(&c.files.roots)));
    line("# Folders to skip, by place.");
    line(&format!("skip_dirs = {}", list(&c.files.skip_dirs)));
    line("# Names to skip (folders and files), wherever they are, also in WSL. * matches anything, e.g. \"*.tmp\".");
    line(&format!("exclude = {}", list(&c.files.exclude)));
    line("# Box is read from the data of Box Drive (sync.db), not by scanning the folder.");
    line("# box_dir: the Box folder. Leave empty to find it automatically.");
    line(&format!("box_dir = {}", q(&c.files.box_dir)));
    line("# box_db: sync.db of Box Drive. Leave empty to find it automatically.");
    line(&format!("box_db = {}", q(&c.files.box_db)));
    line("");
    line("# WSL is read when you type its keyword (\"w \"), and again if the list is older than 5 minutes.");
    line("[wsl]");
    line("# Leave empty for the default distro.");
    line(&format!("distro = {}", q(&c.wsl.distro)));
    line("");
    line("[browser]");
    line("# \"default\" uses the default browser of Windows. \"chrome\" or \"edge\" always uses that browser.");
    line(&format!("browser = {}", q(&c.browser.browser)));
    line("# \"window\" opens pages in a new window, \"tab\" in a new tab.");
    line(&format!("open_in = {}", q(&c.browser.open_in)));
    line("# Bookmarks are read from both profiles. Leave exe and user_data empty to find them automatically.");
    line(&format!("chrome_profile = {}", q(&c.browser.chrome_profile)));
    line(&format!("chrome_exe = {}", q(&c.browser.chrome_exe)));
    line(&format!("chrome_user_data = {}", q(&c.browser.chrome_user_data)));
    line(&format!("edge_profile = {}", q(&c.browser.edge_profile)));
    line(&format!("edge_exe = {}", q(&c.browser.edge_exe)));
    line("");
    line("[chatgpt]");
    line("# \"url\" opens chatgpt.com/?q=... and sends the question.");
    line("# \"clipboard\" copies the question and opens ChatGPT (use this if \"url\" stops working).");
    line(&format!("mode = {}", q(&c.chatgpt.mode)));
    line(&format!("url = {}", q(&c.chatgpt.url)));
    line(&format!("home = {}", q(&c.chatgpt.home)));
    line("# With \"url\", press Enter in ChatGPT after it opens (only when the window in front is ChatGPT).");
    line(&format!("send_enter = {}", c.chatgpt.send_enter));
    line("# With ?q=, wait this many milliseconds after ChatGPT comes to the front if the send button cannot be found.");
    line("# The limit for waiting for ChatGPT to come to the front is this value plus 10 seconds.");
    line(&format!("send_delay_ms = {}", c.chatgpt.send_delay_ms));
    line("# After pasting images, wait this many milliseconds per image if the send button cannot be found.");
    line("# The limit for waiting for the send button to become enabled is the per-image wait times the image count plus 5 seconds.");
    line(&format!("image_wait_ms = {}", c.chatgpt.image_wait_ms));
    line("# Long questions (Shift+Enter): hide Markdown marks such as ** and # except on the line you are editing. false shows them faintly.");
    line(&format!("hide_marks = {}", c.chatgpt.hide_marks));
    line("");
    line("[google]");
    line(&format!("url = {}", q(&c.google.url)));
    line("");
    line("# What Enter does with Ctrl, Shift or Alt. Write it like \"Ctrl+Alt+Enter\". Leave empty to turn it off.");
    line("# chatgpt: ask ChatGPT even when something matches. google: search Google.");
    line("# write: start a long question (new line). admin: run as administrator. terminal: open the folder in a terminal.");
    line("[keys]");
    let k = &c.keys;
    for (name, v) in [("chatgpt", &k.chatgpt), ("google", &k.google), ("write", &k.write), ("admin", &k.admin), ("terminal", &k.terminal)] {
        line(&format!("{name} = {}", q(v)));
    }
    line("");
    line("[tools]");
    line("# Terminal for Alt+Enter: a name from [[open_with]]. Leave empty to use WezTerm, then Windows Terminal.");
    line(&format!("terminal = {}", q(&c.tools.terminal)));
    line("");
    line("# \"Open in ...\" in the actions menu (the right arrow key). Only apps that are installed are shown.");
    line("# exe: leave empty to find VS Code, WezTerm or Windows Terminal automatically.");
    line("# args: {path} is the file or folder, {dir} is the folder. Leave empty for the usual way.");
    for ow in &c.open_with {
        line("[[open_with]]");
        line(&format!("name = {}", q(&ow.name)));
        line(&format!("exe = {}", q(&ow.exe)));
        line(&format!("args = {}", q(&ow.args)));
    }
    line("");
    line("# What the actions menu (the right arrow key) shows for apps, files and folders. Items: copy_file (copy the file itself), new_window (apps), open_with (\"Open in ...\"),");
    line("# reveal (open the folder it is in), copy_path, copy_wsl_path (only when WSL is installed), admin, explorer (folders),");
    line("# choose_app (\"Open with...\" list of apps), mail (attach to a new Outlook mail). These two are for files only.");
    line("[menu]");
    line(&format!("app = {}", list(&c.menu.app)));
    line(&format!("file = {}", list(&c.menu.file)));
    line(&format!("folder = {}", list(&c.menu.folder)));
    line("");
    line("# Language of the Settings window and the tray menu: \"auto\" (the language of Windows), \"ja\" or \"en\".");
    line("[ui]");
    line(&format!("language = {}", q(&c.ui.language)));
    line("# Colors of the launcher window: \"auto\" (follows the app mode of Windows), \"light\" or \"dark\".");
    line(&format!("theme = {}", q(&c.ui.theme)));
    line("");
    line("# recent: show recently opened items when the box is empty. clipboard_history: remember what you copy (type \"cb \" to see it).");
    line("[search]");
    line(&format!("recent = {}", c.search.recent));
    line(&format!("clipboard_history = {}", c.search.clipboard_history));
    line("");
    line("# selection: open Hikidashi with the text selected in the app in front. Leave empty to turn it off.");
    line("[hotkeys]");
    line(&format!("selection = {}", q(&c.hotkeys.selection)));
    line("");
    line("# Settings for extensions. masked_write: the 8 command bits of Masked Write in MIPI RFFE (not public, so not built in).");
    if !c.ext.contains_key("mipi") {
        line("# [ext.mipi]");
        line("# masked_write = \"xxxxxxxx\"");
    }
    for (name, opts) in &c.ext {
        line(&format!("[ext.{}]", q(name)));
        for (k, v) in opts {
            line(&format!("{} = {}", q(k), q(v)));
        }
    }
    o
}

/// 名前に使えない文字(空白)があるか
fn has_space(s: &str) -> bool {
    s.chars().any(char::is_whitespace)
}

impl Config {
    /// 設定画面で保存する前の確かめ. 問題があれば画面に出す文(英語)
    pub fn validate(&self) -> Result<(), String> {
        let p = &self.prefix;
        let prefixes = [("Apps", &p.app), ("Files", &p.file), ("Bookmarks", &p.bookmark), ("Kill", &p.kill), ("WSL", &p.wsl), ("Box", &p.boxdrive), ("Google", &p.google), ("ChatGPT", &p.chatgpt), ("Clipboard", &p.clipboard), ("Calc", &p.calc)];
        for (i, (label, v)) in prefixes.iter().enumerate() {
            if has_space(v) {
                return Err(format!("Keyword for {label} cannot contain spaces."));
            }
            if !v.is_empty() && prefixes[..i].iter().any(|(_, w)| w.eq_ignore_ascii_case(v)) {
                return Err(format!("Keyword \"{v}\" is used twice."));
            }
        }
        for (k, v) in &self.alias {
            if k.trim().is_empty() || has_space(k) {
                return Err(format!("Alias \"{k}\" cannot be empty or contain spaces."));
            }
            if v.trim().is_empty() {
                return Err(format!("Alias \"{k}\" needs an app name."));
            }
        }
        for (i, c) in self.command.iter().enumerate() {
            if c.name.trim().is_empty() || has_space(&c.name) {
                return Err(format!("Command name \"{}\" cannot be empty or contain spaces.", c.name));
            }
            if self.command[..i].iter().any(|d| d.name.eq_ignore_ascii_case(&c.name)) {
                return Err(format!("Command \"{}\" is listed twice.", c.name));
            }
            let filled = [&c.url, &c.run, &c.file, &c.folder].iter().filter(|v| v.as_deref().is_some_and(|s| !s.trim().is_empty())).count();
            if filled != 1 {
                return Err(format!("Command \"{}\" needs one of url, run, file or folder.", c.name));
            }
        }
        for (kind, list) in [("app", &self.menu.app), ("file", &self.menu.file), ("folder", &self.menu.folder)] {
            if let Some(bad) = list.iter().find(|i| !MENU_ITEMS.contains(&i.as_str())) {
                return Err(format!("Unknown actions menu item \"{bad}\" for {kind}."));
            }
        }
        let sel = self.hotkeys.selection.trim();
        if !sel.is_empty() && !crate::hotkey::Hotkey::parse(sel).is_some_and(|h| h.is_usable()) {
            return Err("Selection hotkey must look like \"Ctrl+Alt+Space\" (with Ctrl, Alt, Shift or Win).".into());
        }
        if !["auto", "ja", "en"].contains(&self.ui.language.as_str()) {
            return Err("Language must be auto, ja or en.".into());
        }
        if !["auto", "light", "dark"].contains(&self.ui.theme.as_str()) {
            return Err("Theme must be auto, light or dark.".into());
        }
        if !["default", "chrome", "edge"].contains(&self.browser.browser.as_str()) {
            return Err("Browser must be default, chrome or edge.".into());
        }
        if !["window", "tab"].contains(&self.browser.open_in.as_str()) {
            return Err("Open pages in must be window or tab.".into());
        }
        if self.chatgpt.mode != "url" && self.chatgpt.mode != "clipboard" {
            return Err("ChatGPT mode must be url or clipboard.".into());
        }
        if !(500..=30000).contains(&self.chatgpt.send_delay_ms) {
            return Err("Wait before Enter must be between 500 and 30000 ms.".into());
        }
        if self.chatgpt.image_wait_ms > 60000 {
            return Err("Wait per image must be between 0 and 60000 ms.".into());
        }
        for (i, o) in self.open_with.iter().enumerate() {
            if o.name.trim().is_empty() {
                return Err("Each \"Open with\" app needs a name.".into());
            }
            if self.open_with[..i].iter().any(|p| p.name.eq_ignore_ascii_case(&o.name)) {
                return Err(format!("\"Open with\" app \"{}\" is listed twice.", o.name));
            }
        }
        let term = self.tools.terminal.trim();
        if !term.is_empty() && !self.open_with.iter().any(|o| o.name.eq_ignore_ascii_case(term)) {
            return Err(format!("Terminal \"{term}\" is not in the \"Open with\" list."));
        }
        let k = &self.keys;
        let keys = [("ChatGPT", &k.chatgpt), ("Google", &k.google), ("Long question", &k.write), ("Run as administrator", &k.admin), ("Terminal", &k.terminal)];
        let mut used: Vec<u8> = vec![];
        for (label, v) in keys {
            if v.trim().is_empty() {
                continue;
            }
            let Some(m) = parse_enter_key(v) else {
                return Err(format!("Key for {label} must look like \"Ctrl+Enter\" (Ctrl, Shift, Alt and Enter)."));
            };
            if used.contains(&m) {
                return Err(format!("Key {} is used twice.", format_enter_key(m)));
            }
            used.push(m);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses() {
        let c = Config::parse(&template()).unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.prefix.file, "f");
        assert!(c.files.exclude.contains(&"AppData".to_string()));
        // 普通の言葉の名前は初期値に入れない(K32)
        for w in ["target", "obj"] {
            assert!(!c.files.exclude.iter().any(|n| n == w));
        }
    }

    #[test]
    fn old_wsl_settings_are_merged() {
        // WSLのオンオフと除く名前は, キーワードと`[files] exclude`に移す(K44)
        let c = Config::parse("[files]\nexclude = ['.git']\n[wsl]\nenabled = false\nexclude = ['.GIT', 'old*']\n").unwrap();
        assert_eq!(c.prefix.wsl, "");
        assert_eq!(c.files.exclude, vec![".git", "old*"]);
        assert!(!c.prefixes().iter().any(|(_, m)| *m == crate::router::Mode::Wsl));
        assert_eq!(Config::parse(&to_toml(&c)).unwrap(), c);
    }

    #[test]
    fn to_toml_round_trips() {
        let mut c = Config::default();
        c.alias.insert("ppt".into(), "PowerPoint".into());
        c.alias.insert("ワード".into(), "Word".into());
        c.command.push(Command { name: "jira".into(), title: Some("Open \"Jira\"".into()), url: Some("https://x/{q}".into()), ..Default::default() });
        c.command.push(Command { name: "dot".into(), title: None, url: None, run: Some(r#"code "C:\Users\me\dot""#.into()), ..Default::default() });
        c.command.push(Command { name: "mitsumori".into(), file: Some(r"C:\Users\me\見積.xlsx".into()), ..Default::default() });
        c.command.push(Command { name: "anken".into(), folder: Some(r"D:\案件".into()), ..Default::default() });
        c.files.roots.push(r"D:\work".into());
        c.files.skip_dirs.push(r"%USERPROFILE%\Documents\old".into());
        c.files.exclude = vec!["it's".into()];
        c.files.box_dir = r"D:\Box".into();
        c.files.box_db = r"%LOCALAPPDATA%\Box\Box\data\sync.db".into();
        c.prefix.wsl = String::new();
        c.chatgpt.mode = "clipboard".into();
        c.browser.browser = "edge".into();
        c.browser.open_in = "tab".into();
        c.browser.edge_profile = "Profile 1".into();
        c.ext.entry("mipi".into()).or_default().insert("masked_write".into(), "0101".into());
        let text = to_toml(&c);
        assert_eq!(Config::parse(&text).unwrap(), c, "{text}");
        assert!(text.contains(r"roots = ['%USERPROFILE%', 'D:\work']"), "{text}");
    }

    #[test]
    fn open_with_round_trips() {
        let mut c = Config::default();
        c.open_with.push(OpenWith { name: "nvim".into(), exe: r"C:\WezTerm\wezterm-gui.exe".into(), args: "start -- nvim {path}".into() });
        c.tools.terminal = "Windows Terminal".into();
        c.chatgpt.send_enter = false;
        c.chatgpt.send_delay_ms = 5000;
        c.chatgpt.image_wait_ms = 1500;
        c.chatgpt.hide_marks = false;
        assert_eq!(Config::parse(&to_toml(&c)).unwrap(), c);
        assert_eq!(Config::parse("[chatgpt]\nsend_delay_ms = 4000\n").unwrap().chatgpt.image_wait_ms, 3000);
        c.chatgpt.image_wait_ms = 60001;
        assert!(c.validate().is_err());
        c.chatgpt.image_wait_ms = 0;
        // 全部消したときは空のまま(初めの一覧に戻らない)
        c.open_with.clear();
        c.tools.terminal.clear();
        assert_eq!(Config::parse(&to_toml(&c)).unwrap().open_with, vec![]);
        assert!(c.validate().is_ok());
        c.tools.terminal = "nope".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn old_tools_move_to_open_with() {
        let c = Config::parse("[tools]\nvscode = 'D:\\Code\\Code.exe'\nwezterm = 'D:\\wez\\wezterm-gui.exe'\n").unwrap();
        assert_eq!(c.tools.vscode, "");
        assert_eq!(c.open_with[0].exe, r"D:\Code\Code.exe");
        assert_eq!(c.open_with[1].exe, r"D:\wez\wezterm-gui.exe");
        assert_eq!(c.open_with.len(), 3);
        assert_eq!(Config::parse(&to_toml(&c)).unwrap(), c);
    }

    #[test]
    fn old_chrome_section_moves_to_browser() {
        let c = Config::parse("[chrome]\nexe = 'C:\\c.exe'\nprofile = \"Profile 2\"\n").unwrap();
        assert_eq!(c.chrome, None);
        assert_eq!(c.browser.chrome_exe, r"C:\c.exe");
        assert_eq!(c.browser.chrome_profile, "Profile 2");
        assert_eq!(c.browser.browser, "default");
        assert_eq!(Config::parse(&to_toml(&c)).unwrap(), c);
    }

    #[test]
    fn enter_keys() {
        assert_eq!(parse_enter_key("Ctrl+Alt+Enter"), Some(KEY_CTRL | KEY_ALT));
        assert_eq!(parse_enter_key(" alt + shift + enter "), Some(KEY_ALT | KEY_SHIFT));
        assert_eq!(parse_enter_key("Enter"), None);
        assert_eq!(parse_enter_key("Ctrl+K"), None);
        assert_eq!(format_enter_key(KEY_ALT | KEY_SHIFT | KEY_CTRL), "Ctrl+Shift+Alt+Enter");
        let k = Keys::default();
        assert_eq!(k.action_of(KEY_CTRL), Some(KeyAction::ChatGpt));
        assert_eq!(k.action_of(KEY_CTRL | KEY_ALT), Some(KeyAction::Google));
        assert_eq!(k.action_of(KEY_SHIFT), Some(KeyAction::Write));
        assert_eq!(k.action_of(KEY_ALT | KEY_SHIFT), None);
        assert_eq!(k.label(KeyAction::Google), "Ctrl+Alt+Enter");
        for c in ENTER_KEY_CHOICES.iter().skip(1) {
            assert!(parse_enter_key(c).is_some(), "{c}");
        }
        let mut c = Config::default();
        c.keys.google = "Ctrl+Enter".into();
        assert!(c.validate().unwrap_err().contains("twice"));
        c.keys.google = "Ctrl+G".into();
        assert!(c.validate().is_err());
        c.keys.google = String::new();
        assert!(c.validate().is_ok());
        c.keys.chatgpt = "Alt+Shift+Enter".into();
        c.prefix.google = "gg".into();
        assert_eq!(Config::parse(&to_toml(&c)).unwrap(), c);
    }

    #[test]
    fn validate_catches_mistakes() {
        assert!(Config::default().validate().is_ok());
        let mut c = Config::default();
        c.prefix.kill = "f".into();
        assert!(c.validate().unwrap_err().contains("twice"));
        let mut c = Config::default();
        c.command.push(Command { name: "x".into(), ..Default::default() });
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.alias.insert("a b".into(), "X".into());
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.menu.file.push("open_in_notepad".into());
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.ui.language = "fr".into();
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.ui.theme = "blue".into();
        assert!(c.validate().is_err());
        let mut c = Config::default();
        c.browser.open_in = "popup".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn theme_is_written_and_read() {
        // 前の版の設定(themeがない)は自動. 書き出して読み直しても残る(K39)
        assert_eq!(Config::parse("[ui]\nlanguage = \"ja\"\n").unwrap().ui.theme, "auto");
        let mut c = Config::default();
        c.ui.theme = "dark".into();
        assert_eq!(Config::parse(&to_toml(&c)).unwrap().ui.theme, "dark");
    }

    #[test]
    fn old_window_prefix_and_refresh_are_read() {
        // 前の版の`[prefix] window`は`wsl`に, `refresh_minutes`は読むだけ(K24)
        let c = Config::parse("[prefix]\nwindow = \"x\"\n[files]\nrefresh_minutes = 10\n").unwrap();
        assert_eq!(c.prefix.wsl, "x");
        assert!(!to_toml(&c).contains("refresh_minutes"));
        assert_eq!(Config::parse(&to_toml(&c)).unwrap().prefix.wsl, "x");
    }

    #[test]
    fn menu_items() {
        let m = Menu::default();
        assert!(m.has("app", "admin"));
        assert!(!m.has("folder", "reveal"));
        assert!(menu_item_fits("folder", "explorer"));
        assert!(!menu_item_fits("folder", "admin"));
        assert!(!menu_item_fits("app", "copy_wsl_path"));
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn commands_and_aliases() {
        let c = Config::parse("[alias]\nppt = \"PowerPoint\"\n[[command]]\nname = \"jira\"\nurl = \"https://x/{q}\"\n").unwrap();
        assert_eq!(c.alias.get("ppt").map(String::as_str), Some("PowerPoint"));
        assert_eq!(c.command[0].url.as_deref(), Some("https://x/{q}"));
    }

    #[test]
    fn error_has_line() {
        let e = Config::parse("[prefix]\napp = \"a\"\nfile = \n").unwrap_err();
        assert!(e.0.starts_with("config.toml line 3"), "{}", e.0);
        let e = Config::parse("[prefix]\nunknown = \"a\"\n").unwrap_err();
        assert!(e.0.contains("line 2"), "{}", e.0);
    }

    #[test]
    fn search_and_selection_hotkey_settings() {
        let c = Config::default();
        assert!(!c.search.recent && c.search.clipboard_history);
        assert_eq!(c.hotkeys.selection, "Ctrl+Alt+Space");
        assert_eq!(c.prefix.clipboard, "cb");
        // 書き出して読み直しても残る
        let mut c = Config::default();
        c.search.recent = true;
        c.search.clipboard_history = false;
        c.hotkeys.selection = String::new();
        c.prefix.clipboard = "cl".into();
        let r = Config::parse(&to_toml(&c)).unwrap();
        assert_eq!(r, c);
        let r = Config::parse("[search]\nrecent = true\n[hotkeys]\nselection = \"Ctrl+Shift+K\"\n").unwrap();
        assert!(r.search.recent && r.search.clipboard_history);
        assert_eq!(r.hotkeys.selection, "Ctrl+Shift+K");
        assert!(r.validate().is_ok());
    }

    #[test]
    fn selection_hotkey_is_validated() {
        let mut c = Config::default();
        c.hotkeys.selection = "Space".into();
        assert!(c.validate().is_err());
        c.hotkeys.selection = "Ctrl+Alt+".into();
        assert!(c.validate().is_err());
        c.hotkeys.selection = String::new();
        assert!(c.validate().is_ok());
        // クリップボードの接頭辞も重なりを調べる
        let mut c = Config::default();
        c.prefix.clipboard = "f".into();
        assert!(c.validate().unwrap_err().contains("twice"));
    }

    #[test]
    fn copy_file_is_in_the_default_menu() {
        let m = Menu::default();
        assert!(m.has("file", "copy_file"));
        assert!(m.has("folder", "copy_file"));
        assert!(!m.has("app", "copy_file"));
        assert!(menu_item_fits("file", "copy_file"));
        assert!(menu_item_fits("folder", "copy_file"));
        assert!(!menu_item_fits("app", "copy_file"));
        assert_eq!(MENU_ITEMS[0], "copy_file");
        let c = Config::parse("[menu]\nfile = [\"copy_file\", \"reveal\"]\n").unwrap();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn shell_items_are_for_files_and_on_by_default() {
        let m = Menu::default();
        for it in ["choose_app", "mail"] {
            assert!(MENU_ITEMS.contains(&it));
            assert!(m.has("file", it));
            assert!(!m.has("folder", it));
            assert!(menu_item_fits("file", it));
            assert!(!menu_item_fits("folder", it));
            assert!(!menu_item_fits("app", it));
        }
        // 既存の設定ファイルに書いてある一覧には, 新しい項目は勝手に足されない(copy_fileと同じ)
        let c = Config::parse("[menu]\nfile = [\"copy_file\"]\n").unwrap();
        assert!(!c.menu.has("file", "mail"));
        // 消した`share`(K68)は, 前の版の設定ファイルに書いてあっても読み飛ばす
        let c = Config::parse("[menu]\nfile = [\"copy_file\", \"share\", \"mail\"]\n").unwrap();
        assert_eq!(c.menu.file, vec!["copy_file", "mail"]);
        assert!(c.validate().is_ok());
    }
}
