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
    ("Keywords", "キーワード"),
    ("Search", "検索"),
    ("Apps", "アプリ"),
    ("Actions", "操作メニュー"),
    ("Aliases", "エイリアス"),
    ("Files", "ファイル"),
    ("Folders", "フォルダー"),
    ("Bookmarks", "ブックマーク"),
    ("End a process", "プロセスを終了"),
    ("Clipboard history", "クリップボードの履歴"),
    ("Calculator", "計算"),
    ("Saved results", "保存した結果"),
    ("Saved: {}", "保存しました: {0}"),
    ("Calendar", "予定"),
    ("Nothing here yet", "まだありません"),
    // 全般
    ("Startup", "起動"),
    ("Start when Windows starts", "Windowsの起動時に開始"),
    ("Display", "表示"),
    ("Language", "言語 (Language)"),
    ("Theme", "テーマ"),
    ("Same as Windows", "Windowsに合わせる"),
    ("Light", "ライト"),
    ("Dark", "ダーク"),
    // キー
    ("Hotkeys", "ホットキー"),
    ("Show the window", "窓を出す"),
    ("Open Hikidashi", "Hikidashiを開く"),
    ("Works in any app", "どのアプリからでも開きます"),
    ("Open with selected text", "選択した文字で開く"),
    ("Types it in for you", "選択中の文字を入れて開きます"),
    ("Not set", "未設定"),
    ("Change", "変更"),
    ("Clear", "解除"),
    ("Selection hotkey must look like \"Ctrl+Alt+Space\" (with Ctrl, Alt, Shift or Win).", "「選択した文字で開く」のホットキーは「Ctrl+Alt+Space」のように書いてください(Ctrl, Alt, Shift, Winのどれかと一緒に)."),
    ("Enter combinations", "Enterの組み合わせ"),
    ("Not used", "使わない"),
    ("Ask ChatGPT", "ChatGPTに聞く"),
    ("Search Google", "Googleで検索"),
    ("Long question", "長い質問を書く"),
    ("Run as admin", "管理者として実行"),
    ("Open in terminal", "ターミナルで開く"),
    // キーワード
    ("Type a keyword and a space to search only that kind. Leave it empty to turn it off.", "キーワードと空白に続けて打つと, その種類だけを検索します. 空にすると使いません."),
    // 検索
    ("Show recent items", "最近使ったものを表示"),
    ("When the box is empty", "入力欄が空のときに出します"),
    ("Keeps copied text and files in memory only", "コピーした文字とファイルを, メモリの中だけに残します"),
    ("Folders to search", "検索するフォルダー"),
    ("Box and your WSL home are found automatically, and searched with their keywords.", "BoxとWSLのホームは自動で見つけ, キーワードを打ったときだけ検索します."),
    ("Excluded folders", "除外するフォルダー"),
    ("Everything inside them is excluded too.", "中のフォルダーとファイルも除外します."),
    ("Excluded names", "除外する名前"),
    ("Names to exclude anywhere, also in WSL. * matches anything.", "どこにあっても, この名前のものを除外します. WSLにも効き, *も使えます."),
    ("Add", "追加"),
    ("Remove", "削除"),
    ("Edit", "編集"),
    ("Restore defaults", "初期値に戻す"),
    ("Browse…", "参照"),
    // アプリ
    ("Folders with apps", "アプリを探すフォルダー"),
    (".exe files here are listed as apps.", "ここにある.exeをアプリとして出します."),
    ("Open in another app", "別のアプリで開く"),
    ("Shown as \"Open in …\" in the actions menu. Apps that are not installed are not shown.", "操作メニューの「Open in …」に出ます. 入っていないアプリは出ません."),
    ("Name", "名前"),
    ("App", "アプリ"),
    ("Terminal", "ターミナル"),
    ("Terminal to use", "使うターミナル"),
    ("For \"Open in terminal\"", "「ターミナルで開く」で使います"),
    ("Automatic", "自動"),
    ("  · not installed", "  · 入っていない"),
    ("(find automatically)", "(自動)"),
    // 操作メニュー
    ("Choose what appears when you press the right arrow key on a row.", "行を選んで→を押したときに出すものを選びます."),
    ("Apps in the Start menu", "スタートメニューのアプリ"),
    ("File and folder names", "ファイルとフォルダーの名前"),
    ("Chrome and Edge bookmarks", "ChromeとEdgeのブックマーク"),
    ("Running apps", "動いているアプリ"),
    ("Files in your WSL home", "WSLのホームのファイル"),
    ("Files in Box Drive", "Box Driveのファイル"),
    ("Search on Google", "Googleで検索"),
    ("A long question to ChatGPT", "ChatGPTへの長い質問"),
    ("Text and files you copied", "コピーした文字とファイル"),
    ("Numbers, formulas, RF, MIPI", "進数, 式, RF, MIPI"),
    ("Results saved with Ctrl+S", "Ctrl+Sで保存した結果"),
    ("Outlook appointments", "Outlookの予定"),
    ("Copy file", "ファイルをコピー"),
    ("Open new window", "新しく起動"),
    ("Open the folder it is in", "場所を開く"),
    ("Copy path", "パスをコピー"),
    ("Copy WSL path (only with WSL)", "WSLのパスをコピー"),
    ("Run as administrator", "管理者として実行"),
    ("Open in Explorer", "Explorerで開く"),
    ("Open with... (list of apps)", "プログラムから開く(アプリの一覧)"),
    ("Attach to new mail (Outlook)", "新しいメールに添付(Outlook)"),
    ("Open with…", "プログラムから開く…"),
    ("Choose another app…", "別のアプリを選ぶ…"),
    ("Attach to new mail", "新しいメールに添付"),
    ("No apps found for this file", "このファイルに関連付いたアプリがありません"),
    // Web
    ("Browser", "ブラウザー"),
    ("Bookmarks are read from both browsers, in these profiles.", "ブックマークは両方のブラウザーから, ここで選んだプロフィールのものを読みます."),
    ("Windows default", "Windowsの既定"),
    ("Open pages in", "ページの開き方"),
    ("New window", "新しいウィンドウ"),
    ("New tab", "新しいタブ"),
    ("Chrome profile", "Chromeのプロフィール"),
    ("Edge profile", "Edgeのプロフィール"),
    ("How to send a question", "質問の渡し方"),
    ("If it fails, use \"Copy, then open\"", "送れないときは「コピーして開く」"),
    ("Open with the question", "URLで開いて送る"),
    ("Copy, then open", "コピーして開く"),
    ("Press Enter automatically", "自動でEnterを押す"),
    ("Only when ChatGPT is in front", "ChatGPTが前にあるときだけ"),
    ("Wait before Enter", "Enterまでの待ち"),
    ("Wait per image", "画像1枚ごとの待ち"),
    ("Milliseconds, added before Enter for each attached image", "ミリ秒. 添付した画像1枚ごとに, Enterの前に足します"),
    ("Milliseconds", "ミリ秒"),
    ("Hide Markdown marks", "Markdownの記号を隠す"),
    ("In long questions, except on the line you edit", "長い質問で, 書いている行のほか"),
    // エイリアスと, 小さな窓
    ("Short words that open an app, a web page, a command, a file or a folder.", "短い語で, アプリ, Webページ, コマンド, ファイル, フォルダーを開きます."),
    ("Word", "語"),
    ("Kind", "種類"),
    ("Opens", "開くもの"),
    ("Web page", "Webページ"),
    ("Command", "コマンド"),
    ("File", "ファイル"),
    ("Folder", "フォルダー"),
    ("Choose a file", "ファイルを選ぶ"),
    ("Choose a folder", "フォルダーを選ぶ"),
    ("Add alias", "エイリアスを追加"),
    ("Edit alias", "エイリアスを編集"),
    ("Add app", "アプリを追加"),
    ("Edit app", "アプリを編集"),
    ("What you type, for example ppt", "打つ文字です(例: ppt)"),
    ("Web address", "アドレス"),
    ("{q} is replaced with what you type after the word.", "{q}は, 語のあとに打った文字に置き換わります."),
    ("Shown as", "表示名"),
    ("Optional, for example Open Jira issue", "なくてもよい(例: Open Jira issue)"),
    ("Shown as \"Open in <name>\"", "「Open in <名前>」と出ます"),
    ("App (.exe)", "アプリ (.exe)"),
    ("Empty: VS Code, WezTerm and so on are found by name.", "空なら, VS CodeやWezTermなどは名前から探します."),
    ("What to pass", "渡し方"),
    ("The selected item", "選んだものを渡す"),
    ("Its folder", "フォルダーを渡す"),
    ("Custom", "自分で書く"),
    ("Arguments", "引数"),
    ("{path}: the selected item, {dir}: its folder", "{path}は選んだもの, {dir}はそのフォルダー"),
    ("Choose an app", "アプリを選ぶ"),
    // 下のボタンと確かめ
    ("Open config file", "設定ファイルを開く"),
    ("Copy setup prompt", "設定プロンプトをコピー"),
    ("Cancel", "キャンセル"),
    ("Close", "閉じる"),
    ("Applied", "反映しました"),
    ("Copied", "コピーしました"),
    ("Hikidashi Settings", "Hikidashiの設定"),
    ("Some changes are not applied. Close anyway?", "反映していない変更があります. このまま閉じますか?"),
    ("config.toml has a mistake. Replace it with the settings shown here? The old file is kept as config.toml.bak.", "config.tomlに書き間違いがあります. ここに出ている設定で置き換えますか? 前のファイルはconfig.toml.bakに残します."),
    ("Copy a prompt for an AI, with all your current settings? It includes folder names and web addresses.", "AIに設定を直してもらうための文を, 今の設定ごとコピーしますか? フォルダーの場所やWebのアドレスも入ります."),
    ("Programs", "プログラム"),
    ("Choose a folder to search", "探すフォルダーを選ぶ"),
    ("Choose a folder to skip", "外すフォルダーを選ぶ"),
    ("Choose a folder with apps", "アプリのあるフォルダーを選ぶ"),
    // 知らせ
    ("Type a word without spaces.", "空白のない語を打ってください."),
    ("Choose or type the app name.", "アプリの名前を選ぶか打ってください."),
    ("\"{}\" is already an alias.", "「{0}」はもうエイリアスにあります."),
    ("Type the web address.", "アドレスを打ってください."),
    ("Type the command.", "コマンドを打ってください."),
    ("Choose or type the file.", "ファイルを選ぶか打ってください."),
    ("Choose or type the folder.", "フォルダーを選ぶか打ってください."),
    ("Type a name.", "名前を打ってください."),
    ("Type where the .exe is (only VS Code, WezTerm and Windows Terminal are found automatically).", ".exeの場所を打ってください(自動で探すのはVS Code, WezTerm, Windows Terminalだけです)."),
    ("\"{}\" is already in the list.", "「{0}」はもう一覧にあります."),
    ("Wait before Enter must be a number of ms.", "Enterまでの待ちはミリ秒の数で書いてください."),
    ("Cannot save config.toml: {}", "config.tomlを保存できません: {0}"),
    ("{}. Defaults are shown.", "{0}. 初期値を出しています."),
    ("Keyword for {} cannot contain spaces.", "{0}のキーワードに空白は使えません."),
    ("Keyword \"{}\" is used twice.", "キーワード「{0}」が2回使われています."),
    ("Alias \"{}\" cannot be empty or contain spaces.", "エイリアス「{0}」は空にできず, 空白も使えません."),
    ("Alias \"{}\" needs an app name.", "エイリアス「{0}」にアプリの名前がありません."),
    ("Command name \"{}\" cannot be empty or contain spaces.", "コマンドの名前「{0}」は空にできず, 空白も使えません."),
    ("Command \"{}\" is listed twice.", "コマンド「{0}」が2回あります."),
    ("Command \"{}\" needs either a URL or a command to run.", "コマンド「{0}」には, URLか実行するコマンドのどちらか1つが要ります."),
    ("Wait before Enter must be between 500 and 30000 ms.", "Enterまでの待ちは500〜30000ミリ秒にしてください."),
    ("Wait per image must be a number of ms.", "画像1枚ごとの待ちはミリ秒の数で書いてください."),
    ("Wait per image must be between 0 and 60000 ms.", "画像1枚ごとの待ちは0〜60000ミリ秒にしてください."),
    ("Each \"Open with\" app needs a name.", "「別のアプリで開く」のアプリには名前が要ります."),
    ("\"Open with\" app \"{}\" is listed twice.", "「別のアプリで開く」のアプリ「{0}」が2回あります."),
    ("Terminal \"{}\" is not in the \"Open with\" list.", "ターミナル「{0}」が「別のアプリで開く」の一覧にありません."),
    ("Key for {} must look like \"Ctrl+Enter\" (Ctrl, Shift, Alt and Enter).", "{0}のキーは「Ctrl+Enter」のように書いてください(Ctrl, Shift, AltとEnter)."),
    ("Key {} is used twice.", "キー{0}が2回使われています."),
    ("Unknown actions menu item \"{}\" for {}.", "{1}の操作メニューに知らない項目「{0}」があります."),
    // Outlookの予定(K62). ランチャーの行と`→`のメニューは英語のまま出す(K1). 知らせはこの表で日本語にする
    ("Search your Outlook calendar", "Outlookの予定を探す"),
    ("Reading Outlook…", "Outlookを読んでいます…"),
    ("Outlook is not available", "Outlookを使えません"),
    ("Outlook is not installed", "Outlookが入っていません"),
    ("Could not start or connect to Outlook", "Outlookを起動できないか, つなげません"),
    ("Could not read the calendar", "予定表を読めません"),
    ("Could not open the appointment", "予定を開けません"),
    ("Cannot open the appointment in Outlook: {}", "Outlookで予定を開けません: {0}"),
    ("No appointments in the next 7 days", "7日以内の予定はありません"),
    ("Open in Outlook", "Outlookで開く"),
    ("Today", "今日"),
    ("Tomorrow", "明日"),
    ("All day", "終日"),
    ("Sun", "日"),
    ("Mon", "月"),
    ("Tue", "火"),
    ("Wed", "水"),
    ("Thu", "木"),
    ("Fri", "金"),
    ("Sat", "土"),
    // トレイのメニュー
    ("Show Hikidashi", "Hikidashiを出す"),
    ("Change hotkey…", "ホットキーを変える…"),
    ("Start with Windows", "Windowsと一緒に起動"),
    ("Rebuild index", "索引を作り直す"),
    ("Settings", "設定"),
    ("Manual", "マニュアル"),
    ("The manual was not found. The docs folder (docs\\manual.html) that comes with Hikidashi.exe is needed. If you moved only the exe, move the docs folder next to it.", "マニュアルが見つかりません. Hikidashi.exeと一緒に配ったdocsフォルダー(docs\\manual.html)が要ります. exeだけを移したときは, docsフォルダーもexeの隣に置いてください."),
    ("Cannot open the manual: {}", "マニュアルを開けません: {0}"),
    ("Quit", "終了"),
];
