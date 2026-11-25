//! 設定ファイルをAIに直してもらうときに貼る文(K40の(3), K45). どのAIでも使えるよう, 書き方の決まりと今の設定を1つにまとめる.

use crate::config::{ENTER_KEY_CHOICES, MENU_ITEMS};

/// `toml`は今の`config.toml`の中身
pub fn config_prompt(toml: &str, ja: bool) -> String {
    let keys: Vec<&str> = ENTER_KEY_CHOICES.iter().copied().filter(|k| !k.is_empty()).collect();
    let (keys, menu) = (keys.join(", "), MENU_ITEMS.join(", "));
    let head = if ja {
        format!(
            "Windowsのランチャー「Hikidashi」の設定ファイル config.toml を, 私の希望に合わせて直してください.

# 進め方
1. 下に「希望」が書かれていなければ, まず何を変えたいかを私に聞く. 分からないこと(アプリの名前, フォルダーの場所, URLなど)は推測せずに聞く.
2. 直した config.toml の全文を, 1つのコードブロックで出す. 一部だけにしない.
3. 変えた所を, コードブロックの後に短く書く.

# 決まり
- 頼まれていない値は変えない. 行を消さない.
- 書き方はTOML. 下の設定にない名前の項目や区分を作らない(読み込みのエラーになる). 使い方は, 下の設定の # の行にある.
- Windowsのパスは '...' で囲む(例: 'D:\\tools'). \\ を重ねない. ユーザーフォルダーは %USERPROFILE% と書ける.
- [prefix]: キーワード. 空白を入れない. 同じものを2つに使わない. 空にするとその種類を使わない.
- [alias]: 語 = \"アプリの名前\". 語に空白を入れない. 例: ppt = \"PowerPoint\"
- [[command]]: 語でWebページかコマンドを開く. name(空白なし. 重ならない), title(なくてもよい), url か run のどちらか1つ. {{q}} は語の後に打った文字.
- [files], [apps]: 一覧は ['...', '...'] の形. exclude の * は何文字にでも合う.
- [keys]: {keys} のどれか, または空. 同じものを2つに使わない.
- [menu]: {menu} から選ぶ.
- [[open_with]]: name, exe(VS Code, WezTerm, Windows Terminalは空でよい), args({{path}} は選んだもの, {{dir}} はそのフォルダー. 空ならふつうの渡し方).
- [tools] terminal: [[open_with]] の name のどれか, または空.
- [browser] browser: default, chrome, edge. open_in: window, tab.
- [chatgpt] mode: url, clipboard. send_delay_ms: 500〜30000.
- [ui] language: auto, ja, en. theme: auto, light, dark.
- [hotkeys] selection: Ctrl+Alt+Space のような形, または空.

# 今の config.toml
"
        )
    } else {
        format!(
            "Edit config.toml, the settings file of the Windows launcher \"Hikidashi\", the way I ask.

# Steps
1. If \"What I want\" below is empty, first ask me what to change. Do not guess things you do not know (app names, folders, URLs); ask me.
2. Output the whole new config.toml in one code block, not just a part.
3. After the code block, list what you changed, briefly.

# Rules
- Do not change or remove anything I did not ask for.
- The format is TOML. Do not add names or sections that are not in the file below (they cause an error). The # lines explain each one.
- Put Windows paths in '...' (for example 'D:\\tools') and do not double the \\. The user folder can be written as %USERPROFILE%.
- [prefix]: keywords. No spaces, no duplicates. Empty turns that kind off.
- [alias]: word = \"app name\". No spaces in the word. Example: ppt = \"PowerPoint\"
- [[command]]: a word that opens a web page or runs a command. name (no spaces, no duplicates), title (optional), and either url or run. {{q}} is what is typed after the word.
- [files], [apps]: lists look like ['...', '...']. In exclude, * matches anything.
- [keys]: one of {keys}, or empty. No duplicates.
- [menu]: choose from {menu}.
- [[open_with]]: name, exe (can be empty for VS Code, WezTerm and Windows Terminal), args ({{path}} is the selected item, {{dir}} is its folder; empty is the usual way).
- [tools] terminal: the name of one of [[open_with]], or empty.
- [browser] browser: default, chrome, edge. open_in: window, tab.
- [chatgpt] mode: url, clipboard. send_delay_ms: 500 to 30000.
- [ui] language: auto, ja, en. theme: auto, light, dark.
- [hotkeys] selection: like Ctrl+Alt+Space, or empty.

# The current config.toml
"
        )
    };
    let want = if ja { "# 希望\n" } else { "# What I want\n" };
    format!("{head}```toml\n{}\n```\n\n{want}", toml.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{to_toml, Config};

    #[test]
    fn has_rules_and_the_current_file() {
        let toml = to_toml(&Config::default());
        for ja in [true, false] {
            let p = config_prompt(&toml, ja);
            assert!(p.contains(toml.trim_end()));
            assert!(p.contains("{q}") && p.contains("{path}") && p.contains("{dir}") && !p.contains("{{"));
            assert!(p.contains("Ctrl+Alt+Shift+Enter") && p.contains("copy_wsl_path"));
            // 説明した区分は, どれも今のファイルにある
            for sec in ["prefix", "files", "apps", "keys", "menu", "tools", "browser", "chatgpt", "ui", "hotkeys", "alias"] {
                assert!(p.contains(&format!("- [{sec}]")) || p.contains(&format!(", [{sec}]")), "{sec}");
                assert!(toml.contains(&format!("[{sec}]")), "{sec}");
            }
            // 中のコードブロックをそのまま読める
            let body = p.split("```toml\n").nth(1).unwrap().split("```").next().unwrap();
            assert_eq!(Config::parse(body).unwrap(), Config::default());
        }
    }
}
