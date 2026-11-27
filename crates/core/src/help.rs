//! ?の一覧と, 名前・説明・種類・別名での絞り込み(K69).

use crate::catalog::{Catalog, Kind};
use crate::config::Config;
use crate::router::{self, Action, Icon, Mode, Row, Source};

const COMMAND_ICON: Icon = Icon::Kind(Kind::Command);

pub struct Entry {
    pub row: Row,
    pub alias: String,
}

impl Entry {
    fn new(name: &str, sub: &str, tag: &'static str, icon: Icon, action: Action, alias: &str) -> Self {
        Self {
            row: Row {
                name: name.to_string(),
                sub: sub.to_string(),
                tag,
                positions: vec![],
                action,
                learn_key: None,
                warn: false,
                icon,
                score: 0,
            },
            alias: alias.to_string(),
        }
    }

    fn matches(&self, words: &[String]) -> bool {
        if words.is_empty() {
            return true;
        }
        let fields = [
            self.row.name.to_lowercase(),
            self.row.sub.to_lowercase(),
            self.row.tag.to_lowercase(),
            self.alias.to_lowercase(),
        ];
        words.iter().all(|word| fields.iter().any(|field| field.contains(word)))
    }
}

/// ?の一覧なら, ?のあとに打った絞り込みの語を返す.
pub fn filter_words(mode: Mode, text: &str) -> Option<&str> {
    if mode != Mode::All {
        return None;
    }
    match text.trim() {
        "?" => Some(""),
        text => text.strip_prefix("? ").map(str::trim),
    }
}

/// キーワードを入れる行. 2行目は「キーワード  説明」.
pub fn help_row(keyword: &str, name: &str, desc: &str, tag: &'static str) -> Entry {
    let sub = if desc.is_empty() { keyword.to_string() } else { format!("{keyword}  {desc}") };
    Entry::new(name, &sub, tag, COMMAND_ICON, Action::Fill(format!("{keyword} ")), "")
}

fn keyword_entries(cfg: &Config) -> Vec<Entry> {
    let p = &cfg.prefix;
    let list: [(&str, &str, &str, &str); 12] = [
        (&p.app, "Apps", "Search only apps", "アプリ"),
        (&p.file, "Files and folders", "Search only files and folders", "ファイル フォルダー"),
        (&p.bookmark, "Bookmarks", "Search Chrome and Edge bookmarks", "ブックマーク"),
        (&p.kill, "End a process", "Choose a running app to end", "プロセス 終了 kill"),
        (&p.wsl, "WSL", "Search files in your WSL home", "wsl"),
        (&p.boxdrive, "Box", "Search files in Box Drive", "box"),
        (&p.google, "Google", "Search on Google", "検索"),
        (&p.chatgpt, "ChatGPT", "Write a long question to ChatGPT", "質問 chatgpt"),
        (&p.clipboard, "Clipboard history", "Text and files you copied", "クリップボード 履歴"),
        (&p.calc, "Calculator", "Numbers, formulas, RF", "計算 電卓 rf"),
        (&p.snapshot, "Saved results", "Results saved with Ctrl+S", "保存"),
        (&p.calendar, "Calendar", "Outlook appointments", "予定 カレンダー outlook"),
    ];
    list.into_iter()
        .filter(|(keyword, _, _, _)| !keyword.is_empty())
        .map(|(keyword, name, desc, alias)| {
            let mut entry = help_row(keyword, name, desc, "keyword");
            entry.alias = alias.to_string();
            entry
        })
        .collect()
}

fn calc_entries(prefix: &str) -> impl Iterator<Item = Entry> + '_ {
    let examples = [
        ("0x1F", "Hex, binary and decimal", "進数 16進 2進"),
        ("10dBm", "dBm to watts and volts", "電力 rf"),
        ("3dB", "dB to power and voltage ratios", "rf"),
        ("x2", "Ratio to dB", "倍 rf"),
        ("vswr 1.5", "VSWR to return loss, |Γ| and mismatch loss", "反射 定在波比 リターンロス rf"),
        ("vswr 10 il 2dB", "VSWR seen through a 2 dB loss", "反射 損失 rf"),
        ("gamma 0.2+j0.4", "Γ to impedance and VSWR", "反射係数 rf"),
        ("50+j50@2G", "Impedance to Γ and equivalent circuits", "インピーダンス rf"),
        ("100||7.958nH@2G", "Parallel parts at a frequency", "並列 インピーダンス rf"),
        ("10nH@2.4G", "Reactance at a frequency", "リアクタンス rf"),
        ("10nH 1pF", "Resonant frequency", "共振 rf"),
        ("(1+2)*3/4", "Arithmetic", "四則演算 電卓"),
        ("mipi sa=6 addr=1C data=38", "MIPI RFFE bit sequence (extension)", "rffe rf"),
    ];
    examples.into_iter().map(move |(name, sub, alias)| {
        Entry::new(name, sub, "calc", Icon::Calc, Action::Fill(format!("{prefix} {name}")), &format!("計算 {alias}"))
    })
}

fn syntax_entries() -> impl Iterator<Item = Entry> {
    let kinds = [
        ("excel", "Excel and CSV files"),
        ("word", "Word files"),
        ("ppt", "PowerPoint files"),
        ("pdf", "PDF files"),
        ("image", "Images"),
        ("video", "Videos"),
        ("audio", "Audio files"),
        ("zip", "Archives"),
        ("text", "Text files"),
        ("code", "Source code"),
        ("folder", "Folders only"),
        ("word,pdf", "Word or PDF files"),
    ];
    let examples = [
        ("*.pdf", "Names that match a pattern (* is any text)", "*.pdf ", "ワイルドカード"),
        ("ext:pdf", "Files with this extension", "ext:pdf ", "拡張子"),
        ("in:資料", "Files whose folder path contains the word", "in:", "フォルダー 場所"),
        ("modified:7d", "Changed within 7 days (h, d, w)", "modified:7d ", "更新 日付"),
        ("report & 2026", "Both words (AND)", "report & 2026", "かつ and"),
        ("*.pdf | *.xlsx", "Either one (OR)", "*.pdf | *.xlsx", "または or"),
        ("report & !draft", "Leave out a word (NOT)", "report & !draft", "除く not"),
        ("(*.pdf | *.docx) & in:2026", "Group conditions with parentheses", "(*.pdf | *.docx) & in:2026", "括弧"),
        ("%APPDATA%", "Folders such as %APPDATA% and %TEMP%", "%APPDATA%\\", "環境変数 フォルダー"),
        (":stats", "File counts, memory and search times", ":stats", "統計 計測"),
    ];
    let kind_entries = kinds.into_iter().map(|(kind, sub)| {
        let name = format!("kind:{kind}");
        Entry::new(&name, sub, "syntax", COMMAND_ICON, Action::Fill(format!("{name} ")), "検索 書き方 種類")
    });
    kind_entries.chain(examples.into_iter().map(|(name, sub, fill, alias)| {
        Entry::new(name, sub, "syntax", COMMAND_ICON, Action::Fill(fill.to_string()), &format!("検索 書き方 {alias}"))
    }))
}

fn key_entries(cfg: &Config) -> impl Iterator<Item = Entry> + '_ {
    let keys: [(&str, &str, &str, &str); 10] = [
        (&cfg.hotkeys.selection, "Search the text selected in any app", "selection", "選択 選んだ文字"),
        (&cfg.keys.write, "Write a long question to ChatGPT", "longq", "長い質問 改行 chatgpt"),
        (&cfg.keys.chatgpt, "Ask ChatGPT even if something matches", "keys", "質問 chatgpt"),
        (&cfg.keys.google, "Search the words on Google", "web", "検索 google"),
        (&cfg.keys.admin, "Run as administrator", "keys", "管理者"),
        ("Tab", "Complete the selected row", "keys", "補完"),
        ("→", "Actions for the selected row", "tab", "メニュー 操作 右"),
        ("Ctrl+S", "Save the result (see snap)", "snap", "保存"),
        ("↑", "With an empty box, bring back what you typed", "keys", "履歴 前 上"),
        ("Backspace", "After \\ in a path, go up to the parent folder", "paste", "親 フォルダー 戻る"),
    ];
    keys.into_iter().filter_map(|(name, sub, id, alias)| {
        let name = name.trim();
        (!name.is_empty()).then(|| Entry::new(name, sub, "key", COMMAND_ICON, Action::Manual(id.to_string()), alias))
    })
}

/// ?の一覧を表示順に作り, 全部の語がいずれかの欄に含まれる行を残す.
pub fn rows(cfg: &Config, system: &Catalog, plugins: Vec<Entry>, words: &str) -> Vec<Row> {
    let mut entries = keyword_entries(cfg);
    for c in &cfg.command {
        let title = c.title.as_deref().unwrap_or(&c.name);
        let what = c.url.as_deref().or(c.run.as_deref()).unwrap_or("");
        entries.push(help_row(&c.name, title, what, "command"));
    }
    entries.extend(plugins);
    entries.extend(system.items.iter().enumerate().map(|(index, item)| Entry {
        row: router::catalog_row(Source::System, index, item),
        alias: item.keys.join(" "),
    }));
    if !cfg.prefix.calc.is_empty() {
        entries.extend(calc_entries(&cfg.prefix.calc));
    }
    entries.extend(syntax_entries());
    entries.extend(key_entries(cfg));

    let words: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
    let rows: Vec<Row> = entries.into_iter().filter(|entry| entry.matches(&words)).map(|entry| entry.row).collect();
    if rows.is_empty() {
        vec![Entry::new(
            "No matching help",
            "Clear the words after ? to see everything",
            "",
            COMMAND_ICON,
            Action::Copy(String::new()),
            "",
        ).row]
    } else {
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::{self, SysAction};
    use crate::calc;
    use crate::files::FileQuery;
    use crate::learn::Learn;
    use crate::router::{Ctx, Env, PathKind, Sources};
    use crate::search::Prepared;

    struct TestEnv;

    impl Env for TestEnv {
        fn expand(&self, path: &str) -> String {
            path.to_string()
        }

        fn path_kind(&self, _: &str) -> PathKind {
            PathKind::Missing
        }

        fn wsl_unc(&self, _: &str) -> Option<String> {
            None
        }
    }

    fn build(cfg: &Config, input: &str) -> Vec<Row> {
        let empty = Catalog::new(vec![]);
        let system = Catalog::new(builtin::system_items());
        let ctx = Ctx {
            config: cfg,
            sources: Sources {
                apps: &empty,
                bookmarks: &empty,
                processes: &empty,
                settings: &empty,
                system: &system,
                commands: &empty,
                folders: &empty,
            },
            tables: &[],
            learn: &Learn::default(),
            now: 0,
            env: &TestEnv,
        };
        let (_, text) = router::parse(input, &cfg.prefixes());
        router::build(input, &ctx, &Prepared::new(text.trim(), None), &[]).1
    }

    fn names(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|row| row.name.as_str()).collect()
    }

    #[test]
    fn all_rows_in_display_order() {
        let rows = build(&Config::default(), "?");
        assert_eq!(rows.len(), 66);
        for (range, tag) in [
            (0..12, "keyword"),
            (12..21, "system"),
            (21..34, "calc"),
            (34..56, "syntax"),
            (56..66, "key"),
        ] {
            assert!(rows[range].iter().all(|row| row.tag == tag));
        }

        let items = builtin::system_items();
        let mut confirm = Vec::new();
        for (index, (row, item)) in rows[12..21].iter().zip(&items).enumerate() {
            assert_eq!(row.name, item.name);
            assert_eq!(row.sub, item.sub);
            assert_eq!(row.action, Action::Item { source: Source::System, index });
            if SysAction::from_id(&item.target).unwrap().needs_confirm() {
                confirm.push(row.name.as_str());
            }
        }
        assert_eq!(confirm, ["Restart", "Shut down", "Sign out"]);
    }

    #[test]
    fn filters_by_all_words_across_fields() {
        let cfg = Config::default();
        let rf = build(&cfg, "? rf");
        assert_eq!(names(&rf), [
            "Calculator",
            "10dBm",
            "3dB",
            "x2",
            "vswr 1.5",
            "vswr 10 il 2dB",
            "gamma 0.2+j0.4",
            "50+j50@2G",
            "100||7.958nH@2G",
            "10nH@2.4G",
            "10nH 1pF",
            "mipi sa=6 addr=1C data=38",
        ]);
        let calc = build(&cfg, " ?  計算 ");
        assert_eq!(calc.len(), 14);
        assert_eq!(calc[0].name, "Calculator");
        assert!(calc[1..].iter().all(|row| row.tag == "calc"));
        let kind = build(&cfg, "? kind");
        assert_eq!(kind.len(), 12);
        assert!(kind.iter().all(|row| row.name.starts_with("kind:")));
        assert_eq!(names(&build(&cfg, "? ロック")), ["Lock"]);
        assert_eq!(build(&cfg, "? system"), build(&cfg, "?")[12..21]);
        assert_eq!(names(&build(&cfg, "? SYSTEM ロック\tSCREEN LOC")), ["Lock"]);

        let missing = build(&cfg, "? zzz");
        assert_eq!(names(&missing), ["No matching help"]);
        assert_eq!(missing[0].sub, "Clear the words after ? to see everything");
        assert_eq!(missing[0].action, Action::Copy(String::new()));
    }

    #[test]
    fn help_requires_a_standalone_question_mark_in_all_mode() {
        let cfg = Config::default();
        assert_eq!(filter_words(Mode::All, " \t?  \t"), Some(""));
        assert_eq!(filter_words(Mode::All, " ?  rf \t"), Some("rf"));
        assert_eq!(filter_words(Mode::All, "?\trf"), None);
        assert_eq!(filter_words(Mode::App, "? rf"), None);
        assert_eq!(build(&cfg, "?rf")[0].action, Action::ChatGpt("?rf".into()));
        assert!(build(&cfg, "a ? rf").is_empty());
    }

    #[test]
    fn plugins_follow_commands_and_participate_in_filtering() {
        let cfg = Config::parse("[[command]]\nname = 'jira'\nurl = 'https://example.com/{q}'\n").unwrap();
        let system = Catalog::new(builtin::system_items());
        let plugins = || vec![
            help_row("mipi", "MIPI", "MIPI command", "extension"),
            help_row("wiki", "Wiki", "Local notes", "script"),
        ];
        let all = rows(&cfg, &system, plugins(), "");
        assert_eq!(names(&all[12..16]), ["jira", "MIPI", "Wiki", "Lock"]);
        let filtered = rows(&cfg, &system, plugins(), "script NOTES");
        assert_eq!(names(&filtered), ["Wiki"]);
        assert_eq!(filtered[0].action, Action::Fill("wiki ".into()));
    }

    #[test]
    fn calc_examples_fill_valid_expressions() {
        let mut cfg = Config::default();
        cfg.prefix.calc = "math".into();
        let calc = build(&cfg, "? calc 計算");
        assert_eq!(calc.len(), 14);
        for row in &calc[1..] {
            assert_eq!(row.icon, Icon::Calc);
            let Action::Fill(fill) = &row.action else { panic!("{}: {:?}", row.name, row.action) };
            let (mode, expression) = router::parse(fill, &cfg.prefixes());
            assert_eq!((mode, expression), (Mode::Calc, row.name.as_str()));
            if !expression.starts_with("mipi ") {
                assert!(calc::evaluate(expression).is_some(), "{expression}");
            }
        }
        cfg.prefix.calc.clear();
        assert_eq!(names(&build(&cfg, "? 計算")), ["No matching help"]);
    }

    #[test]
    fn syntax_examples_fill_structured_queries() {
        let syntax = build(&Config::default(), "? syntax");
        assert_eq!(syntax.len(), 22);
        for row in syntax {
            let Action::Fill(fill) = row.action else { panic!("{}: {:?}", row.name, row.action) };
            match row.name.as_str() {
                "in:資料" => assert_eq!(fill, "in:"),
                "%APPDATA%" => assert_eq!(fill, "%APPDATA%\\"),
                ":stats" => assert_eq!(fill, ":stats"),
                _ => assert!(FileQuery::parse(fill.trim()).is_structured(), "{fill}"),
            }
        }
    }

    #[test]
    fn key_rows_link_to_manual_and_follow_config() {
        let manual = include_str!("../../../docs/manual.html");
        let mut cfg = Config::default();
        let all = build(&cfg, "?");
        for row in &all[56..] {
            let Action::Manual(id) = &row.action else { panic!("{}: {:?}", row.name, row.action) };
            assert!(manual.contains(&format!("id=\"{id}\"")), "{id}");
        }

        cfg.keys.chatgpt = "Alt+Q".into();
        let changed = build(&cfg, "? KEY CHATGPT matches");
        assert_eq!(names(&changed), ["Alt+Q"]);
        assert_eq!(changed[0].action, Action::Manual("keys".into()));
        cfg.keys.chatgpt.clear();
        assert_eq!(names(&build(&cfg, "? KEY CHATGPT matches")), ["No matching help"]);
        assert_eq!(build(&cfg, "?").iter().filter(|row| row.tag == "key").count(), 9);
    }

    #[test]
    fn key_rows_ignore_surrounding_whitespace() {
        let cfg = Config::parse(r#"
[hotkeys]
selection = "  \t "
[keys]
chatgpt = " \t "
google = " \tCtrl+Alt+Enter\t "
"#).unwrap();
        let rows = build(&cfg, "?");
        let keys: Vec<&str> = rows.iter().filter(|row| row.tag == "key").map(|row| row.name.as_str()).collect();
        assert_eq!(keys, ["Shift+Enter", "Ctrl+Alt+Enter", "Ctrl+Shift+Enter", "Tab", "→", "Ctrl+S", "↑", "Backspace"]);
    }
}
