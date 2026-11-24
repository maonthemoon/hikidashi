//! 設定画面, 知らせ, トレイのメニューの日本語(K24のQ5). ランチャーの画面は英語のまま(K1).
//! 英語の文をそのまま鍵にして日本語を引く. 表にない文は英語のまま出す.
//! `{}`を含む鍵は形で合わせ, 日本語の`{0}`, `{1}`に中身を入れる(中身も表にあれば日本語にする).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static JA: AtomicBool = AtomicBool::new(false);

/// 設定の`[ui] language`(`auto`, `ja`, `en`)から言語を決める. `auto`はWindowsの表示言語
pub fn set_language(code: &str) {
    let ja = match code {
        "ja" => true,
        "en" => false,
        _ => (unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() } & 0x3ff) == 0x11,
    };
    JA.store(ja, Ordering::Relaxed);
}

pub fn is_ja() -> bool {
    JA.load(Ordering::Relaxed)
}

/// 今の言語の文にする
pub fn tr(s: &str) -> String {
    if !is_ja() || s.is_empty() {
        return s.to_string();
    }
    let (exact, patterns) = table();
    if let Some(j) = exact.get(s) {
        return j.to_string();
    }
    for (en, ja) in patterns {
        if let Some(caps) = capture(en, s) {
            let mut out = ja.to_string();
            for (i, c) in caps.iter().enumerate() {
                out = out.replace(&format!("{{{i}}}"), &tr(c));
            }
            return out;
        }
    }
    s.to_string()
}

/// `{}`を含む形`en`に`s`が合えば, `{}`の中身を返す
fn capture(en: &str, s: &str) -> Option<Vec<String>> {
    let parts: Vec<&str> = en.split("{}").collect();
    let mut rest = s.strip_prefix(parts[0])?;
    let mut caps = vec![];
    for (i, p) in parts.iter().enumerate().skip(1) {
        if i == parts.len() - 1 {
            let c = rest.strip_suffix(p)?;
            if c.is_empty() {
                return None;
            }
            caps.push(c.to_string());
        } else {
            let at = rest.find(p).filter(|&a| a > 0)?;
            caps.push(rest[..at].to_string());
            rest = &rest[at + p.len()..];
        }
    }
    Some(caps)
}

type Table = (HashMap<&'static str, &'static str>, Vec<(&'static str, &'static str)>);

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| {
        let mut exact = HashMap::new();
        let mut patterns = vec![];
        for &(en, ja) in PAIRS {
            if en.contains("{}") {
                patterns.push((en, ja));
            } else {
                exact.insert(en, ja);
            }
        }
        (exact, patterns)
    })
}

const PAIRS: &[(&str, &str)] = &[
    // 区分と見出し
    ("General", "全般"),
    ("Search", "検索"),
    ("Box and WSL", "Box・WSL"),
    ("Apps", "アプリ"),
    ("Tab menu", "Tabのメニュー"),
    ("Aliases", "別名"),
    ("Commands", "自作コマンド"),
    ("Files", "ファイル"),
    ("Folders", "フォルダー"),
    ("Bookmarks", "ブックマーク"),
    ("Kill", "kill"),
    ("Clipboard", "クリップボード"),
    // 全般
    ("Selection hotkey", "選択ホットキー"),
    ("Clear", "なし"),
    ("Show recent items when the box is empty", "入力欄が空のとき, 最近開いたものを出す"),
    ("Clipboard history", "クリップボードの履歴"),
    ("Remember what you copy (text and files, in memory only). Type the Clipboard prefix (cb) and a space to see it.", "コピーしたもの(文字とファイル)を覚えます(メモリの中だけ). 接頭辞(cb)と空白を打つと見られます."),
    ("Selection hotkey must look like \"Ctrl+Alt+Space\" (with Ctrl, Alt, Shift or Win).", "選択ホットキーは「Ctrl+Alt+Space」のように書いてください(Ctrl, Alt, Shift, Winのどれかと一緒に)."),
    ("Hotkey", "ホットキー"),
    ("Theme", "テーマ"),
    ("Same as the app mode of Windows", "Windowsのアプリのモードに合わせる"),
    ("Light", "ライト"),
    ("Dark", "ダーク"),
    ("Not set", "未設定"),
    ("Change…", "変える…"),
    ("Start Hikidashi when Windows starts", "Windowsと一緒に起動する"),
    ("Prefixes", "接頭辞"),
    ("Type a prefix and a space to search only one kind, for example \"f report\".", "接頭辞と空白を打つと, 1つの種類だけを探します(例: f report)."),
    ("Enter with Ctrl, Shift or Alt", "Ctrl, Shift, Altを足したEnter"),
    ("Not used", "使わない"),
    ("Ask ChatGPT", "ChatGPTに聞く"),
    ("Search Google", "Googleで検索"),
    ("Long question", "長い質問"),
    ("Run as admin", "管理者として実行"),
    ("Open in terminal", "ターミナルで開く"),
    // 検索
    ("Folders to search", "探すフォルダー"),
    ("Add…", "追加…"),
    ("Remove", "削除"),
    ("Edit…", "編集…"),
    ("Skip these folders", "外すフォルダー"),
    ("and everything inside them", "中も全部外します"),
    ("Skip these names", "外す名前"),
    ("folders and files with this name, wherever they are", "どこにあっても, この名前のフォルダーとファイルを外します"),
    ("Restore defaults", "初めの一覧に戻す"),
    // Box・WSL
    ("Box is found automatically from Box Drive and read again whenever it changes. Type \"bx \" and a name to search it.", "BoxはBox Driveから自動で見つけ, 変わるたびに読み直します. bxと空白のあとに名前を打つと探します."),
    ("folders and files with this name in WSL, wherever they are", "WSLの中のどこにあっても, この名前のフォルダーとファイルを外します"),
    ("Add", "追加"),
    ("Browse…", "参照…"),
    ("Search the home folder in WSL", "WSLのホームフォルダーを探す"),
    ("Type the WSL prefix and a space (\"w report\"). The list is read then, and again if it is older than 5 minutes.", "WSLの接頭辞と空白を打つと探します(例: w report). 一覧はそのときに読み, 前に読んでから5分たっていれば読み直します."),
    ("Distro", "ディストリ"),
    ("empty: the default distro", "空なら既定のディストリ"),
    // アプリ
    ("More folders with apps", "アプリを足すフォルダー"),
    (".exe files here are listed as apps", "ここにある.exeをアプリとして出します"),
    ("Open with", "〜で開く"),
    ("\"Open in …\" in the Tab menu. Apps that are not installed are not shown.", "Tabのメニューの「Open in …」. 入っていないアプリは出ません."),
    ("Name", "名前"),
    ("App", "アプリ"),
    ("Passes", "渡すもの"),
    ("Terminal", "ターミナル"),
    ("\"Open in terminal\" (General, Alt+Enter at first) goes into the folder of the selected file or folder in this terminal.", "「ターミナルで開く」(全般. 初めはAlt+Enter)で, 選んだファイルやフォルダーの場所にこのターミナルで入ります."),
    ("Automatic (WezTerm, then Windows Terminal)", "自動(WezTerm, なければWindows Terminal)"),
    ("  · not installed", "  · 入っていない"),
    ("(find automatically)", "(自動で探す)"),
    ("(usual way)", "(いつもの渡し方)"),
    // Tabのメニュー
    ("Choose what the Tab menu shows for apps, files and folders. Things you can do with a key (Enter, Alt+Enter, Ctrl+Shift+Enter) can be left out.", "Tabのメニューに出すものを, アプリ, ファイル, フォルダーごとに選びます. キー(Enter, Alt+Enter, Ctrl+Shift+Enter)でできることは外してかまいません."),
    ("Copy file", "ファイルをコピー(Copy file)"),
    ("Open new window", "新しく起動する(Open new window)"),
    ("Open in … (Open with)", "〜で開く(Open in …)"),
    ("Open the folder it is in", "場所を開く(Open folder)"),
    ("Copy path", "パスをコピー(Copy path)"),
    ("Copy WSL path (only with WSL)", "WSLのパスをコピー(WSLがあるときだけ)"),
    ("Run as administrator", "管理者として実行(Run as administrator)"),
    ("Open in Explorer", "Explorerで開く(Open in Explorer)"),
    // Web
    ("Browser", "ブラウザー"),
    ("Default browser of Windows", "Windowsの既定のブラウザー"),
    ("Open pages in", "ページの開き方"),
    ("New window", "新しい窓"),
    ("New tab", "新しいタブ"),
    ("Chrome profile", "Chromeのプロフィール"),
    ("Edge profile", "Edgeのプロフィール"),
    ("Bookmarks are read from both browsers. Pages open in these profiles.", "ブックマークは両方のブラウザーから読みます. ページはこのプロフィールで開きます."),
    ("Open chatgpt.com/?q= and send the question", "chatgpt.com/?q=で開いて質問を送る"),
    ("Copy the question and open ChatGPT", "質問をコピーしてChatGPTを開く"),
    ("Use \"Copy the question\" if the question stops being sent.", "質問が送られなくなったら「質問をコピー」にしてください."),
    ("Press Enter in ChatGPT after", "開いたらChatGPTでEnterを押す. 待ち:"),
    ("ms (only when ChatGPT is in front)", "ミリ秒(ChatGPTが前にあるときだけ)"),
    ("Long questions: hide Markdown marks except on the line you edit", "長い質問: 書いている行のほかはMarkdownの記号を隠す"),
    ("ChatGPT URL", "ChatGPTのURL"),
    ("Google URL", "GoogleのURL"),
    // 別名, 自作コマンド
    ("Type the name on the left to find the app on the right, for example ppt for PowerPoint.", "左の名前を打つと右のアプリが出ます(例: pptでPowerPoint)."),
    ("Type", "打つ名前"),
    ("Type the name to open a web page or run a command. {q} is what you type after the name, for example \"jira ABC-123\".", "名前を打つと, Webのページを開くかコマンドを実行します. {q}は名前のあとに打った文字です(例: jira ABC-123)."),
    ("Shown as", "表示名"),
    ("Does", "すること"),
    ("Add alias", "別名を追加"),
    ("Edit alias", "別名を編集"),
    ("Add command", "自作コマンドを追加"),
    ("Edit command", "自作コマンドを編集"),
    ("Add app to open with", "「〜で開く」のアプリを追加"),
    ("Edit app to open with", "「〜で開く」のアプリを編集"),
    ("for example ppt", "例: ppt"),
    ("Choose from the list or type the app name.", "一覧から選ぶか, アプリの名前を打ちます."),
    ("what you type, for example jira", "打つ名前(例: jira)"),
    ("optional, for example Open Jira issue", "なくてもよい(例: Open Jira issue)"),
    ("Open a web page", "Webのページを開く"),
    ("Run a command", "コマンドを実行する"),
    ("Command", "コマンド"),
    ("Web address", "Webのアドレス"),
    ("{q} is replaced with what you type after the name.", "{q}は名前のあとに打った文字に置き換わります."),
    ("{q} is replaced with what you type.", "{q}は打った文字に置き換わります."),
    ("shown as \"Open in <name>\"", "「Open in <名前>」と出ます"),
    ("App (.exe)", "アプリ(.exe)"),
    ("Empty: find VS Code, WezTerm or Windows Terminal by the name.", "空なら, VS Code, WezTerm, Windows Terminalは名前から探します."),
    ("{path} file or folder, {dir} folder. Empty: the usual way.", "{path}はファイルかフォルダー, {dir}はフォルダー. 空ならいつもの渡し方."),
    // 下のボタンと確かめ
    ("Open config file", "設定ファイルを開く"),
    ("Cancel", "キャンセル"),
    ("Save", "保存"),
    ("Hikidashi Settings", "Hikidashiの設定"),
    ("Saved. The changes are in effect now.", "保存しました. 今から変わります."),
    ("Saved. Close and open Settings again to change the language.", "保存しました. 言語は設定画面を閉じて開き直すと変わります."),
    ("Discard your changes?", "変更を捨てますか?"),
    ("Open config.toml and discard the changes made here?", "config.tomlを開き, ここで変えた内容を捨てますか?"),
    ("Programs", "プログラム"),
    ("Choose a folder to search", "探すフォルダーを選ぶ"),
    ("Choose a folder to skip", "外すフォルダーを選ぶ"),
    ("Choose a folder with apps", "アプリのあるフォルダーを選ぶ"),
    ("Choose chrome.exe", "chrome.exeを選ぶ"),
    // 知らせ
    ("Type a name without spaces.", "空白のない名前を打ってください."),
    ("Choose or type the app name.", "アプリの名前を選ぶか打ってください."),
    ("\"{}\" is already an alias.", "「{0}」はもう別名にあります."),
    ("Type the web address or the command.", "Webのアドレスかコマンドを打ってください."),
    ("\"{}\" is already a command.", "「{0}」はもう自作コマンドにあります."),
    ("Type a name.", "名前を打ってください."),
    ("Type where the .exe is (only VS Code, WezTerm and Windows Terminal are found automatically).", ".exeの場所を打ってください(自動で探すのはVS Code, WezTerm, Windows Terminalだけです)."),
    ("\"{}\" is already in the list.", "「{0}」はもう一覧にあります."),
    ("Wait before Enter must be a number of ms.", "Enterまでの待ちはミリ秒の数で書いてください."),
    ("Choose a browser profile (Default if unsure).", "ブラウザーのプロフィールを選んでください(迷ったらDefault)."),
    ("Cannot save config.toml: {}", "config.tomlを保存できません: {0}"),
    ("{}. Defaults are shown; Save replaces the file.", "{0}. 初期値を出しています. 保存するとファイルを置き換えます."),
    ("Prefix for {} cannot contain spaces.", "{0}の接頭辞に空白は使えません."),
    ("Prefix \"{}\" is used twice.", "接頭辞「{0}」が2回使われています."),
    ("Alias \"{}\" cannot be empty or contain spaces.", "別名「{0}」は空にできず, 空白も使えません."),
    ("Alias \"{}\" needs an app name.", "別名「{0}」にアプリの名前がありません."),
    ("Command name \"{}\" cannot be empty or contain spaces.", "自作コマンドの名前「{0}」は空にできず, 空白も使えません."),
    ("Command \"{}\" is listed twice.", "自作コマンド「{0}」が2回あります."),
    ("Command \"{}\" needs either a URL or a command to run.", "自作コマンド「{0}」には, URLか実行するコマンドのどちらか1つが要ります."),
    ("Wait before Enter must be between 500 and 30000 ms.", "Enterまでの待ちは500〜30000ミリ秒にしてください."),
    ("Each \"Open with\" app needs a name.", "「〜で開く」のアプリには名前が要ります."),
    ("\"Open with\" app \"{}\" is listed twice.", "「〜で開く」のアプリ「{0}」が2回あります."),
    ("Terminal \"{}\" is not in the \"Open with\" list.", "ターミナル「{0}」が「〜で開く」の一覧にありません."),
    ("Key for {} must look like \"Ctrl+Enter\" (Ctrl, Shift, Alt and Enter).", "{0}のキーは「Ctrl+Enter」のように書いてください(Ctrl, Shift, AltとEnter)."),
    ("Key {} is used twice.", "キー{0}が2回使われています."),
    ("Unknown Tab menu item \"{}\" for {}.", "{1}のTabのメニューに知らない項目「{0}」があります."),
    // トレイのメニュー
    ("Show Hikidashi", "Hikidashiを出す"),
    ("Change hotkey…", "ホットキーを変える…"),
    ("Start with Windows", "Windowsと一緒に起動"),
    ("Rebuild index", "索引を作り直す"),
    ("Settings", "設定"),
    ("Manual", "マニュアル"),
    ("Quit", "終了"),
];
