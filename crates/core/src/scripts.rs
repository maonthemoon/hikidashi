//! スクリプトのフォルダー(`%APPDATA%\Hikidashi\scripts`). 先頭の見出し(`@hikidashi.title`など)の読み取りと,
//! 出力の行の読み取り. ファイルの読み込みとプロセスの実行は画面の側(アプリ)が行う.

use crate::catalog::{Catalog, Item, Kind};
use crate::router::{Action, Icon, Row, MAX_ROWS};
use crate::search::Prepared;

/// 見出しを探す行数
pub const HEADER_LINES: usize = 30;
/// 出力の行の数の上限(`run`)
pub const MAX_OUTPUT_ROWS: usize = 50;
/// `list`の件数の上限
pub const MAX_LIST_ENTRIES: usize = 5000;
/// 実行を打ち切る秒数
pub const TIMEOUT_SECS: u64 = 30;
/// `refresh`の既定(秒)
pub const DEFAULT_REFRESH_SECS: u64 = 30 * 60;

/// スクリプトの言語(拡張子で決める)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    PowerShell,
    Batch,
    Python,
}

impl Lang {
    pub fn of(file_name: &str) -> Option<Lang> {
        let ext = file_name.rsplit_once('.')?.1.to_ascii_lowercase();
        match ext.as_str() {
            "ps1" => Some(Lang::PowerShell),
            "bat" | "cmd" => Some(Lang::Batch),
            "py" => Some(Lang::Python),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptMode {
    Run,
    List,
}

/// 先頭の見出し
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    pub title: Option<String>,
    pub keyword: Option<String>,
    /// 引数を取る(値は案内の語. 空のこともある)
    pub argument: Option<String>,
    /// `run`か`list`(書いていなければNone)
    pub mode: Option<String>,
    pub refresh: Option<String>,
}

/// 先頭の30行から`@hikidashi.<名前> <値>`を読む. コメントの記号は問わず, 行のどこにあってもよい
pub fn parse_header(text: &str) -> Header {
    let mut h = Header::default();
    for line in text.lines().take(HEADER_LINES) {
        let Some(at) = line.find("@hikidashi.") else { continue };
        let rest = &line[at + "@hikidashi.".len()..];
        let (name, value) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim()),
            None => (rest, ""),
        };
        // `<# ... #>`や`/* ... */`の閉じる記号を除く
        let value = value.trim_end_matches("#>").trim_end_matches("*/").trim().to_string();
        match name.to_ascii_lowercase().as_str() {
            "title" if !value.is_empty() => h.title = Some(value),
            "keyword" if !value.is_empty() => h.keyword = Some(value),
            "argument" => h.argument = Some(value),
            "mode" if !value.is_empty() => h.mode = Some(value.to_ascii_lowercase()),
            "refresh" if !value.is_empty() => h.refresh = Some(value),
            _ => {}
        }
    }
    h
}

/// `30m`, `2h`, `90s`(単位なしは分)を秒にする. 最低1分
pub fn parse_refresh(s: &str) -> Option<u64> {
    let s = s.trim().to_ascii_lowercase();
    let (num, unit) = match s.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => (&s[..i], s[i..].trim()),
        None => (s.as_str(), "m"),
    };
    let n: u64 = num.parse().ok()?;
    let secs = match unit {
        "s" | "sec" => n,
        "m" | "min" => n * 60,
        "h" | "hour" => n * 3600,
        _ => return None,
    };
    Some(secs.max(60))
}

/// 読み込んだスクリプト
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    /// フルパス
    pub path: String,
    /// ファイル名(拡張子なし)
    pub stem: String,
    pub title: String,
    pub keyword: Option<String>,
    pub argument: Option<String>,
    pub mode: ScriptMode,
    pub refresh_secs: u64,
    pub lang: Lang,
}

impl Script {
    /// ファイルの名前と中身から作る. 使えなければ理由(ログに書く)を返す
    pub fn from_source(path: &str, text: &str) -> Result<Script, String> {
        let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
        let lang = Lang::of(name).ok_or_else(|| format!("{name}: not a script"))?;
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name).to_string();
        let h = parse_header(text);
        let mode = match h.mode.as_deref() {
            None | Some("run") => ScriptMode::Run,
            Some("list") => ScriptMode::List,
            Some(m) => return Err(format!("{name}: unknown mode \"{m}\" (use run or list)")),
        };
        if mode == ScriptMode::List && h.keyword.is_none() {
            return Err(format!("{name}: a list script needs @hikidashi.keyword"));
        }
        let refresh_secs = match &h.refresh {
            None => DEFAULT_REFRESH_SECS,
            Some(r) => parse_refresh(r).ok_or_else(|| format!("{name}: bad @hikidashi.refresh \"{r}\" (use 30m or 2h)"))?,
        };
        Ok(Script { path: path.to_string(), stem: stem.clone(), title: h.title.unwrap_or(stem), keyword: h.keyword, argument: h.argument, mode, refresh_secs, lang })
    }
}

/// `run`のスクリプトを, ふだんの検索の一覧(自作コマンドと同じ`Kind::Command`)の項目にする. `data`はスクリプトの番号
pub fn run_items(scripts: &[Script]) -> Vec<Item> {
    scripts
        .iter()
        .enumerate()
        .filter(|(_, s)| s.mode == ScriptMode::Run)
        .map(|(i, s)| {
            let mut it = Item::new(Kind::Command, s.title.clone(), s.path.clone(), s.path.clone());
            it.keys = s.keyword.iter().cloned().chain(std::iter::once(s.stem.clone())).collect();
            it.args = "script".into();
            it.data = i as u64;
            it
        })
        .collect()
}

fn starts_with_word<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let head = text.get(..name.len())?;
    let rest = text.get(name.len()..)?;
    if head.eq_ignore_ascii_case(name) && rest.starts_with(char::is_whitespace) {
        Some(rest.trim())
    } else {
        None
    }
}

fn plain_row(name: &str, sub: &str, tag: &'static str, action: Action, icon: Icon, warn: bool) -> Row {
    Row { name: name.to_string(), sub: sub.to_string(), tag, positions: vec![], action, learn_key: None, warn, icon, score: 0 }
}

/// 引数を取る`run`のスクリプトの名前(キーワード, ファイル名, タイトル)のあとに文字があれば, 実行の行を作る
pub fn arg_row(scripts: &[Script], text: &str) -> Option<Row> {
    let text = text.trim_start();
    for (i, s) in scripts.iter().enumerate() {
        if s.mode != ScriptMode::Run || s.argument.is_none() {
            continue;
        }
        let names = s.keyword.iter().map(String::as_str).chain([s.stem.as_str(), s.title.as_str()]);
        // 名前が長いものを優先する(`find`より`Find a file`)
        let best = names.filter_map(|n| starts_with_word(text, n).map(|rest| (n.len(), rest))).filter(|(_, rest)| !rest.is_empty()).max_by_key(|(l, _)| *l);
        if let Some((_, rest)) = best {
            let action = Action::ScriptRun { script: i, arg: rest.to_string() };
            return Some(plain_row(rest, &s.title, "run", action, Icon::Kind(Kind::Command), false));
        }
    }
    None
}

/// `list`のスクリプトのキーワード(`wiki `)で始まっていれば(スクリプトの番号, あとの文字)
pub fn list_target<'a>(scripts: &[Script], text: &'a str) -> Option<(usize, &'a str)> {
    let text = text.trim_start();
    scripts.iter().enumerate().find_map(|(i, s)| {
        if s.mode != ScriptMode::List {
            return None;
        }
        starts_with_word(text, s.keyword.as_deref()?).map(|rest| (i, rest))
    })
}

/// 出力の文字にする. UTF-8として読み, 不正なら`fallback`(システムのコードページ)で読む. 先頭のBOMは除く
pub fn decode(bytes: &[u8], fallback: impl Fn(&[u8]) -> String) -> String {
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => fallback(bytes),
    };
    s.trim_start_matches('\u{feff}').to_string()
}

/// 出力を行にする(空行は除く. 行末の空白と`\r`も除く)
pub fn output_lines(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim_end().to_string()).filter(|l| !l.trim().is_empty()).collect()
}

/// プロセスの結果
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    /// 起動できなかった理由
    pub spawn_error: Option<String>,
}

/// `Running…`の行
pub fn running_row(title: &str) -> Row {
    plain_row("Running…", title, "", Action::Copy(String::new()), Icon::Kind(Kind::Command), false)
}

fn line_row(line: &str, sub: &str) -> Row {
    plain_row(line, sub, "copy", Action::ScriptLine(line.to_string()), Icon::Calc, false)
}

/// 実行の結果の行と, `Copy all`で写す全部の文
pub fn outcome_rows(title: &str, o: &RunOutcome) -> (Vec<Row>, String) {
    let fail = |name: String, lines: Vec<String>| {
        let mut rows = vec![plain_row(&name, title, "", Action::Copy(String::new()), Icon::Kind(Kind::Command), true)];
        rows.extend(lines.iter().map(|l| line_row(l, "stderr")));
        (rows, lines.join("\n"))
    };
    if let Some(e) = &o.spawn_error {
        return fail(format!("Could not run: {e}"), vec![]);
    }
    if o.timed_out {
        return fail(format!("Stopped after {TIMEOUT_SECS} seconds"), tail(&o.stderr, 5));
    }
    if let Some(c) = o.exit_code.filter(|c| *c != 0) {
        return fail(format!("Failed (exit code {c})"), tail(&o.stderr, 5));
    }
    let lines = output_lines(&o.stdout);
    if lines.is_empty() {
        return (vec![plain_row("Done (no output)", title, "", Action::Copy(String::new()), Icon::Kind(Kind::Command), false)], String::new());
    }
    let all = lines.join("\n");
    let rows = lines.iter().take(MAX_OUTPUT_ROWS).map(|l| line_row(l, title)).collect();
    (rows, all)
}

/// 最後の数行(空行を除く)
fn tail(s: &str, n: usize) -> Vec<String> {
    let v = output_lines(s);
    let skip = v.len().saturating_sub(n);
    v.into_iter().skip(skip).collect()
}

/// `list`の1件
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListEntry {
    pub title: String,
    pub subtitle: String,
    pub url: Option<String>,
    pub path: Option<String>,
    pub copy: Option<String>,
}

fn looks_like_url(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l.starts_with("mailto:") || l.contains("://")
}

fn looks_like_path(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')) || s.starts_with("\\\\") || s.starts_with('%')
}

/// 出力の1行を1件にする. `title<TAB>subtitle<TAB>target`か, JSONの1行. 読めなければNone
pub fn parse_list_line(line: &str) -> Option<ListEntry> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() {
        return None;
    }
    if line.trim_start().starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
        let get = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let title = get("title")?;
        return Some(ListEntry { title, subtitle: get("subtitle").unwrap_or_default(), url: get("url"), path: get("path"), copy: get("copy") });
    }
    let mut it = line.split('\t');
    let title = it.next()?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    let subtitle = it.next().unwrap_or("").trim().to_string();
    let target = it.next().unwrap_or("").trim().to_string();
    let mut e = ListEntry { title, subtitle, ..Default::default() };
    if looks_like_url(&target) {
        e.url = Some(target);
    } else if looks_like_path(&target) {
        e.path = Some(target);
    } else if !target.is_empty() {
        e.copy = Some(target);
    }
    Some(e)
}

pub fn parse_list(text: &str) -> Vec<ListEntry> {
    text.lines().filter_map(parse_list_line).take(MAX_LIST_ENTRIES).collect()
}

/// `Enter`の動き: urlがあれば開く, pathがあれば開く, なければcopy(なければtitle)をコピー
pub fn entry_action(e: &ListEntry) -> Action {
    if let Some(u) = &e.url {
        Action::OpenUrl(u.clone())
    } else if let Some(p) = &e.path {
        Action::Path(p.clone())
    } else {
        Action::Copy(e.copy.clone().unwrap_or_else(|| e.title.clone()))
    }
}

/// `list`の結果の一覧(あいまい一致で絞るための)
pub struct ListCache {
    pub entries: Vec<ListEntry>,
    catalog: Catalog,
}

impl ListCache {
    pub fn new(entries: Vec<ListEntry>) -> ListCache {
        let items = entries.iter().map(|e| Item::new(Kind::Command, e.title.clone(), e.subtitle.clone(), String::new())).collect();
        ListCache { catalog: Catalog::new(items), entries }
    }

    /// `query`で絞った行(点数順). 空なら先頭から
    pub fn rows(&self, query: &str, dict: Option<&crate::migemo::Dict>) -> Vec<Row> {
        let q = query.trim();
        let order: Vec<(usize, Vec<u32>)> = if q.is_empty() {
            (0..self.entries.len().min(MAX_ROWS)).map(|i| (i, vec![])).collect()
        } else {
            let p = Prepared::new(q, dict);
            self.catalog.search(&p, MAX_ROWS, |_| true).into_iter().map(|h| (h.index, h.positions)).collect()
        };
        order
            .into_iter()
            .map(|(i, positions)| {
                let e = &self.entries[i];
                let (tag, icon) = match (&e.url, &e.path) {
                    (Some(_), _) => ("url", Icon::Web),
                    (None, Some(p)) => ("file", Icon::Path(p.clone())),
                    _ => ("copy", Icon::Calc),
                };
                Row { name: e.title.clone(), sub: e.subtitle.clone(), tag, positions, action: entry_action(e), learn_key: None, warn: false, icon, score: 0 }
            })
            .collect()
    }
}

/// `Loading…`の行
pub fn loading_row() -> Row {
    plain_row("Loading…", "", "", Action::Copy(String::new()), Icon::Calc, false)
}

/// 一覧を作れなかったときの行(理由を2行目に)
pub fn error_row(msg: &str) -> Row {
    plain_row("Could not read the list", msg, "", Action::Copy(String::new()), Icon::Calc, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "# @hikidashi.title 社内Wikiを探す\n:: other\n// @hikidashi.keyword wiki\nREM @hikidashi.argument 語\n' @hikidashi.mode list\n<# @hikidashi.refresh 2h #>\n";

    #[test]
    fn header_is_read_from_any_comment() {
        let h = parse_header(SRC);
        assert_eq!(h.title.as_deref(), Some("社内Wikiを探す"));
        assert_eq!(h.keyword.as_deref(), Some("wiki"));
        assert_eq!(h.argument.as_deref(), Some("語"));
        assert_eq!(h.mode.as_deref(), Some("list"));
        assert_eq!(h.refresh.as_deref(), Some("2h"));
    }

    #[test]
    fn header_ignores_after_30_lines() {
        let text = format!("{}# @hikidashi.title late\n", "x\n".repeat(30));
        assert_eq!(parse_header(&text).title, None);
        let text = format!("{}# @hikidashi.title ok\n", "x\n".repeat(29));
        assert_eq!(parse_header(&text).title.as_deref(), Some("ok"));
    }

    #[test]
    fn refresh_units() {
        assert_eq!(parse_refresh("30m"), Some(1800));
        assert_eq!(parse_refresh("2h"), Some(7200));
        assert_eq!(parse_refresh("10"), Some(600));
        assert_eq!(parse_refresh("1s"), Some(60));
        assert_eq!(parse_refresh("x"), None);
        assert_eq!(parse_refresh("5d"), None);
    }

    #[test]
    fn script_defaults_and_errors() {
        let s = Script::from_source(r"C:\s\hello.ps1", "# nothing\n").unwrap();
        assert_eq!(s.title, "hello");
        assert_eq!(s.mode, ScriptMode::Run);
        assert_eq!(s.refresh_secs, DEFAULT_REFRESH_SECS);
        assert_eq!(s.lang, Lang::PowerShell);
        let s = Script::from_source(r"C:\s\wiki.py", SRC).unwrap();
        assert_eq!(s.mode, ScriptMode::List);
        assert_eq!(s.refresh_secs, 7200);
        assert!(Script::from_source(r"C:\s\x.bat", "REM @hikidashi.mode list\n").is_err());
        assert!(Script::from_source(r"C:\s\x.txt", "").is_err());
        assert!(Script::from_source(r"C:\s\x.bat", "REM @hikidashi.mode odd\n").is_err());
        assert_eq!(Lang::of("A.CMD"), Some(Lang::Batch));
    }

    #[test]
    fn run_items_and_arg_rows() {
        let a = Script::from_source(r"C:\s\find.ps1", "# @hikidashi.title Find a file\n# @hikidashi.keyword ff\n# @hikidashi.argument word\n").unwrap();
        let b = Script::from_source(r"C:\s\wiki.py", SRC).unwrap();
        let c = Script::from_source(r"C:\s\plain.bat", "").unwrap();
        let all = vec![a, b, c];
        let items = run_items(&all);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "Find a file");
        assert_eq!(items[0].data, 0);
        assert_eq!(items[1].data, 2);
        assert!(items[0].keys.contains(&"ff".to_string()));
        let r = arg_row(&all, "ff report 2024").unwrap();
        assert_eq!(r.name, "report 2024");
        assert_eq!(r.action, Action::ScriptRun { script: 0, arg: "report 2024".into() });
        assert!(arg_row(&all, "ff").is_none());
        assert!(arg_row(&all, "plain x").is_none());
        let r = arg_row(&all, "Find a file x").unwrap();
        assert_eq!(r.name, "x");
        assert_eq!(list_target(&all, "wiki  foo"), Some((1, "foo")));
        assert_eq!(list_target(&all, "wiki"), None);
        assert_eq!(list_target(&all, "ff foo"), None);
    }

    #[test]
    fn decode_falls_back() {
        assert_eq!(decode("あ".as_bytes(), |_| "x".into()), "あ");
        assert_eq!(decode(&[0xff, 0xfe, 0x41], |_| "fallback".into()), "fallback");
        assert_eq!(decode("\u{feff}abc".as_bytes(), |_| String::new()), "abc");
    }

    #[test]
    fn outcome_rows_cases() {
        let ok = RunOutcome { exit_code: Some(0), stdout: "a\r\n\r\nb\n".into(), ..Default::default() };
        let (rows, all) = outcome_rows("T", &ok);
        assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        assert_eq!(rows[0].action, Action::ScriptLine("a".into()));
        assert_eq!(all, "a\nb");
        let empty = RunOutcome { exit_code: Some(0), ..Default::default() };
        assert_eq!(outcome_rows("T", &empty).0[0].name, "Done (no output)");
        let many = RunOutcome { exit_code: Some(0), stdout: (0..80).map(|i| format!("l{i}\n")).collect(), ..Default::default() };
        let (rows, all) = outcome_rows("T", &many);
        assert_eq!(rows.len(), 50);
        assert_eq!(all.lines().count(), 80);
        let bad = RunOutcome { exit_code: Some(2), stderr: (0..9).map(|i| format!("e{i}\n")).collect(), ..Default::default() };
        let (rows, _) = outcome_rows("T", &bad);
        assert_eq!(rows[0].name, "Failed (exit code 2)");
        assert!(rows[0].warn);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[5].name, "e8");
        let to = RunOutcome { timed_out: true, ..Default::default() };
        assert!(outcome_rows("T", &to).0[0].name.starts_with("Stopped after 30"));
    }

    #[test]
    fn list_lines() {
        let e = parse_list_line("Top page\tMain\thttps://wiki.example/").unwrap();
        assert_eq!(e.title, "Top page");
        assert_eq!(e.subtitle, "Main");
        assert_eq!(e.url.as_deref(), Some("https://wiki.example/"));
        let e = parse_list_line("Report\t\tC:\\docs\\r.xlsx").unwrap();
        assert_eq!(e.path.as_deref(), Some("C:\\docs\\r.xlsx"));
        let e = parse_list_line("Snippet\tmail\thello there").unwrap();
        assert_eq!(e.copy.as_deref(), Some("hello there"));
        let e = parse_list_line("Only title").unwrap();
        assert_eq!(entry_action(&e), Action::Copy("Only title".into()));
        let e = parse_list_line(r#"{"title":"T","subtitle":"S","url":"https://x/","copy":"c"}"#).unwrap();
        assert_eq!((e.subtitle.as_str(), e.url.as_deref(), e.copy.as_deref()), ("S", Some("https://x/"), Some("c")));
        assert_eq!(entry_action(&e), Action::OpenUrl("https://x/".into()));
        assert!(parse_list_line(r#"{"subtitle":"no title"}"#).is_none());
        assert!(parse_list_line("{broken").is_none());
        assert!(parse_list_line("   ").is_none());
        assert!(parse_list_line("\tonly tab").is_none());
    }

    #[test]
    fn list_cache_filters_by_title() {
        let c = ListCache::new(parse_list("Release notes\t\t\nBudget 2024\t\t\nOnboarding\t\t\n"));
        assert_eq!(c.rows("", None).len(), 3);
        let r = c.rows("bud", None);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "Budget 2024");
        assert!(c.rows("zzz", None).is_empty());
    }
}
