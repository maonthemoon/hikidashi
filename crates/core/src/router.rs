//! 入力から一覧の行を作る. 接頭辞(B1, K3), パスの貼り付け(H1), 自作コマンド(H2), 計算(H5, K6),
//! 各一覧の検索と学習(B4), ヒットなしのChatGPT(B5)をまとめる. 窓に見えるのは8行(A4)で, 9行目からはスクロール(C2).

use crate::calc;
use crate::catalog::{Catalog, Hit, Item, Kind};
use crate::config::Config;
use crate::files::{FileHit, FileQuery, FileTable};
use crate::help;
use crate::learn::Learn;
use crate::pathlike::{self, Pasted};
use crate::search::Prepared;
use std::collections::BTreeMap;

/// 窓に一度に見える行の数(A4)
pub const VISIBLE_ROWS: usize = 8;
/// 一覧に持つ行の数. 9行目からはスクロールで見る(C2)
pub const MAX_ROWS: usize = 50;

/// 種類の絞り込み
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    All,
    App,
    File,
    Bookmark,
    Kill,
    /// WSLのホームのファイルとフォルダーだけ(K24. 前の版の窓の切り替えをやめた)
    Wsl,
    /// Boxのファイルとフォルダーだけ(K20). Boxはこのときだけ探す
    Box,
    /// Google検索(`g 天気`でEnter. 2026-11-24)
    Google,
    /// ChatGPT(K24). 画面はすぐ長い質問の窓にする. 窓を作れないときだけ, この1行を出す
    ChatGpt,
    /// クリップボードの履歴(`cb `). 行は画面の側が`clip::rows`で作る
    Clipboard,
    /// 計算(`= 0x1F`, `= 10dBm`, `= (12+3)*4`). 計算の行はこのときだけ出す(2026-11-25)
    Calc,
    /// 保存した結果(K62. `snap `). 行は画面の側が作る
    Snapshot,
    /// Outlookの予定(K62. `cal `). 行は画面の側が作る
    Calendar,
}

impl Mode {
    /// 入力欄の左に出す印(英語. K1)
    pub fn label(self) -> &'static str {
        match self {
            Mode::All => "",
            Mode::App => "app",
            Mode::File => "file",
            Mode::Bookmark => "bookmark",
            Mode::Kill => "kill",
            Mode::Wsl => "wsl",
            Mode::Box => "box",
            Mode::Google => "google",
            Mode::ChatGpt => "ChatGPT",
            Mode::Clipboard => "clipboard",
            Mode::Calc => "calc",
            Mode::Snapshot => "snap",
            Mode::Calendar => "calendar",
        }
    }
}

/// 先頭の接頭辞(`f `など)を読み取る. 返り値は(種類, 残りの文字).
pub fn parse<'a>(input: &'a str, prefixes: &[(String, Mode)]) -> (Mode, &'a str) {
    for (p, mode) in prefixes {
        // 日本語などで文字の途中を切らないよう, get で取る
        if let (Some(head), Some(rest)) = (input.get(..p.len()), input.get(p.len()..)) {
            if head.eq_ignore_ascii_case(p) {
                if let Some(r) = rest.strip_prefix(' ') {
                    return (*mode, r.trim_start());
                }
            }
        }
    }
    (Mode::All, input)
}

/// 行の元になった一覧
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Apps,
    Bookmarks,
    Processes,
    Settings,
    System,
    Commands,
    /// `%APPDATA%`などのフォルダー(2026-11-24)
    Folders,
}

/// `Enter`で行うこと
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// 一覧の項目(アプリ, ブックマーク, 窓, プロセス, 設定, 操作, 自作コマンド)
    Item { source: Source, index: usize },
    /// 索引のファイルかフォルダー
    File { table: usize, id: u32 },
    /// 貼り付けたパス(環境変数は展開済み)
    Path(String),
    /// BoxのWebのURL. `local`はBox Driveの手元のパス(`sync.db`で引けたとき. K18)
    BoxUrl { url: String, id: String, folder: bool, local: Option<String> },
    /// 計算の結果をコピー
    Copy(String),
    /// 入力欄に入れる(使い方の例. K49)
    Fill(String),
    /// マニュアルを指定のidで開く
    Manual(String),
    ChatGpt(String),
    Google(String),
    /// 自作コマンドのURL
    OpenUrl(String),
    /// 自作コマンドの実行
    Run(String),
    /// 保存した結果の1件(入力). `Enter`で保存した行を出す
    SnapOpen(String),
    /// クリップボードの履歴の1件(番号). `Enter`で貼る
    Clip(u64),
    /// スクリプト(`scripts`の番号)を実行する. `arg`は渡す文字(なければ空)
    ScriptRun { script: usize, arg: String },
    /// スクリプトの出力の1行. `Enter`でその行をコピー
    ScriptLine(String),
    /// Outlookの予定(K62). `Enter`でOutlookで開く. `copy`は`→`のメニューのCopyで入れる1行
    Appointment { entry_id: String, copy: String },
}

/// アイコンの手がかり(K2). 画面の側でWindowsのアイコンを引く.
#[derive(Debug, Clone, PartialEq)]
pub enum Icon {
    /// このパスのアイコン(アプリ, ファイル, フォルダー)
    Path(String),
    /// 種類ごとの決まったアイコン
    Kind(Kind),
    Calc,
    ChatGpt,
    Google,
    Web,
    /// ブックマークのサイトのアイコン(favicon. URL). 読めなければブックマークの印
    Favicon(String),
    /// クリップボード
    Clip,
    /// 予定(Outlook)
    Calendar,
}

/// 一覧の1行
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub sub: String,
    /// 右端の文字(K9)
    pub tag: &'static str,
    /// 太字にする文字の位置(J7)
    pub positions: Vec<u32>,
    pub action: Action,
    /// 選んだときに学習する鍵(B4)
    pub learn_key: Option<String>,
    /// 右端をクレイ色にする(kill. K9)
    pub warn: bool,
    pub icon: Icon,
    pub score: i32,
}

/// パスがあるかどうか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    File,
    Dir,
    Missing,
}

/// OSに頼る部分. 画面の側が実装する(テストでは偽物を使う).
pub trait Env {
    /// `%USERPROFILE%`などを展開する
    fn expand(&self, path: &str) -> String;
    fn path_kind(&self, path: &str) -> PathKind;
    /// WSLのパスをWindowsから開ける形にする. WSLが使えなければNone
    fn wsl_unc(&self, path: &str) -> Option<String>;
    /// BoxのIDの, Box Driveの手元のパスとフォルダーか(K18). `folder`はURLが`/folder/`のとき. 引けなければNone
    fn box_path(&self, _box_id: &str, _folder: bool) -> Option<(String, bool)> {
        None
    }
    /// フォルダーの中身(名前, フォルダーか). パスの補完に使う. 読めなければ空
    fn list_dir(&self, _dir: &str) -> Vec<(String, bool)> {
        vec![]
    }
    /// 小文字にしたパス(学習の鍵)から, 実際の綴りのパスとフォルダーか. なければNone(最近のものから外す)
    fn resolve_path(&self, _lower: &str) -> Option<(String, bool)> {
        None
    }
}

/// 検索に使う一覧
pub struct Sources<'a> {
    pub apps: &'a Catalog,
    pub bookmarks: &'a Catalog,
    pub processes: &'a Catalog,
    pub settings: &'a Catalog,
    pub system: &'a Catalog,
    pub commands: &'a Catalog,
    pub folders: &'a Catalog,
}

impl<'a> Sources<'a> {
    pub fn get(&self, s: Source) -> &'a Catalog {
        match s {
            Source::Apps => self.apps,
            Source::Bookmarks => self.bookmarks,
            Source::Processes => self.processes,
            Source::Settings => self.settings,
            Source::System => self.system,
            Source::Commands => self.commands,
            Source::Folders => self.folders,
        }
    }

    pub fn item(&self, s: Source, index: usize) -> Option<&'a Item> {
        self.get(s).items.get(index)
    }
}

/// 一覧を作る材料
pub struct Ctx<'a> {
    pub config: &'a Config,
    pub sources: Sources<'a>,
    pub tables: &'a [&'a FileTable],
    pub learn: &'a Learn,
    pub now: u64,
    pub env: &'a dyn Env,
}

/// 種類ごとの小さな加点. 同じくらい一致したら, アプリ > フォルダー > ファイル > 設定の順にする.
fn kind_weight(k: Kind) -> i32 {
    match k {
        Kind::App => 12,
        Kind::Command => 6,
        Kind::Folder => 2,
        Kind::Bookmark | Kind::File | Kind::Window | Kind::Process => 0,
        Kind::Setting | Kind::System => -6,
    }
}

pub(crate) fn item_icon(it: &Item) -> Icon {
    match it.kind {
        Kind::App | Kind::File | Kind::Folder => Icon::Path(it.target.clone()),
        Kind::Window | Kind::Process if !it.args.is_empty() => Icon::Path(it.args.clone()),
        Kind::Bookmark => Icon::Favicon(it.target.clone()),
        k => Icon::Kind(k),
    }
}

/// コマンド(設定のページ, Windowsの操作, 自作コマンド)の名前か別名に, 打った文字がぴったり一致したときの加点.
/// `lock`でLockをファイルより上に出す(C2).
fn exact_command_bonus(it: &Item, query: &str) -> i32 {
    if !matches!(it.kind, Kind::Setting | Kind::System | Kind::Command) {
        return 0;
    }
    let q = query.trim();
    if it.name.eq_ignore_ascii_case(q) || it.keys.iter().any(|k| k.eq_ignore_ascii_case(q)) {
        200
    } else {
        0
    }
}

pub(crate) fn catalog_row(src: Source, index: usize, it: &Item) -> Row {
    Row {
        name: it.name.clone(),
        sub: it.sub.clone(),
        tag: it.kind.tag(),
        positions: vec![],
        action: Action::Item { source: src, index },
        learn_key: Some(it.learn_key()),
        warn: it.kind == Kind::Process,
        icon: item_icon(it),
        score: 0,
    }
}

fn item_row(src: Source, index: usize, it: &Item, hit: &Hit, ctx: &Ctx, query: &str) -> Row {
    let mut row = catalog_row(src, index, it);
    let key = row.learn_key.as_deref().unwrap_or_default();
    row.score = hit.score + kind_weight(it.kind) + exact_command_bonus(it, query) + ctx.learn.boost(query, key, ctx.now);
    row.positions.clone_from(&hit.positions);
    // 別名で一致したときは2行目にその語を出す(なぜ出たかがわかるように)
    if let Some(v) = hit.via.as_ref().filter(|v| !v.eq_ignore_ascii_case(&it.name)) {
        row.sub = if it.sub.is_empty() { v.clone() } else { format!("{} · {}", v, it.sub) };
    }
    row
}

/// 貼り付けたパスの行(H1, K9: 普通のファイルと同じ見た目)
fn path_row(p: Pasted, ctx: &Ctx) -> Row {
    match p {
        Pasted::WinPath(raw) => {
            let full = ctx.env.expand(&raw);
            let kind = ctx.env.path_kind(&full);
            path_like_row(full, kind)
        }
        Pasted::WslPath(raw) => match ctx.env.wsl_unc(&raw) {
            Some(unc) => {
                let kind = ctx.env.path_kind(&unc);
                path_like_row(unc, kind)
            }
            None => Row {
                name: raw.clone(),
                sub: "WSL is not available".into(),
                tag: "file",
                positions: vec![],
                action: Action::Path(raw),
                learn_key: None,
                warn: false,
                icon: Icon::Kind(Kind::File),
                score: 0,
            },
        },
        Pasted::Url(url) => Row {
            name: url.clone(),
            sub: "Open in browser".into(),
            tag: "url",
            positions: vec![],
            action: Action::OpenUrl(url),
            learn_key: None,
            warn: false,
            icon: Icon::Web,
            score: 0,
        },
        Pasted::BoxUrl { url, id, folder } => match ctx.env.box_path(&id, folder) {
            // Box Driveの手元のファイルに引けたら, 普通のファイルと同じ見た目にする(Enterで既定のアプリ)
            Some((local, is_dir)) => {
                let (name, parent) = pathlike::split_name(&local);
                Row {
                    name: if name.is_empty() { local.clone() } else { name.to_string() },
                    sub: parent.to_string(),
                    tag: if is_dir { "folder" } else { "file" },
                    positions: vec![],
                    action: Action::BoxUrl { url, id, folder: is_dir, local: Some(local.clone()) },
                    learn_key: None,
                    warn: false,
                    icon: if is_dir { Icon::Kind(Kind::Folder) } else { Icon::Path(local) },
                    score: 0,
                }
            }
            None => Row {
                name: format!("Box {} {}", if folder { "folder" } else { "file" }, id),
                sub: "Not found in Box Drive · Open in browser".into(),
                tag: "url",
                positions: vec![],
                action: Action::BoxUrl { url, id, folder, local: None },
                learn_key: None,
                warn: false,
                icon: Icon::Web,
                score: 0,
            },
        },
    }
}

fn path_like_row(full: String, kind: PathKind) -> Row {
    let (name, parent) = pathlike::split_name(&full);
    let name = if name.is_empty() { full.clone() } else { name.to_string() };
    let (tag, sub) = match kind {
        PathKind::Dir => ("folder", parent.to_string()),
        PathKind::File => ("file", parent.to_string()),
        PathKind::Missing => ("file", format!("Not found · {}", parent)),
    };
    Row {
        name,
        sub,
        tag,
        positions: vec![],
        action: Action::Path(full.clone()),
        learn_key: None,
        warn: false,
        icon: if kind == PathKind::Dir { Icon::Kind(Kind::Folder) } else { Icon::Path(full) },
        score: 0,
    }
}

/// 自作コマンドの一覧(H2). 名前で探せるようにする.
pub fn command_items(cfg: &Config) -> Vec<Item> {
    cfg.command
        .iter()
        .map(|c| {
            let title = c.title.clone().unwrap_or_else(|| c.name.clone());
            let (target, sub) = match (&c.url, &c.run) {
                (Some(u), _) => (u.clone(), u.clone()),
                (None, Some(r)) => (r.clone(), r.clone()),
                _ => (String::new(), String::new()),
            };
            let mut it = Item::new(Kind::Command, title, sub, target);
            it.keys = vec![c.name.clone()];
            it.args = if c.url.is_some() { "url".into() } else { "run".into() };
            it
        })
        .collect()
}

/// アプリの別名(J6)を足す. `ppt = "PowerPoint"`ならPowerPointに`ppt`で一致させる.
pub fn apply_aliases(apps: &mut [Item], alias: &BTreeMap<String, String>) {
    for (a, target) in alias {
        for it in apps.iter_mut().filter(|it| it.name.eq_ignore_ascii_case(target)) {
            if !it.keys.iter().any(|k| k.eq_ignore_ascii_case(a)) {
                it.keys.push(a.clone());
            }
        }
    }
}

/// `jira ABC-123`のように, 自作コマンドの名前の後ろに文字があれば, そのコマンドの行を作る.
fn command_with_arg(text: &str, cfg: &Config) -> Option<Row> {
    let (head, rest) = text.trim().split_once(char::is_whitespace)?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    let c = cfg.command.iter().find(|c| c.name.eq_ignore_ascii_case(head))?;
    let title = c.title.clone().unwrap_or_else(|| c.name.clone());
    let (action, tag, icon) = if let Some(u) = &c.url {
        (Action::OpenUrl(u.replace("{q}", &url_encode(rest))), "url", Icon::Web)
    } else {
        let r = c.run.clone().unwrap_or_default();
        let cmd = if r.contains("{q}") { r.replace("{q}", rest) } else { format!("{r} {rest}") };
        (Action::Run(cmd), "run", Icon::Kind(Kind::Command))
    };
    Some(Row { name: rest.to_string(), sub: title, tag, positions: vec![], action, learn_key: None, warn: false, icon, score: 0 })
}

/// URLに入れる文字の変換(英数字と一部の記号以外を%XXにする)
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}


fn calc_rows(c: &calc::Calc) -> Vec<Row> {
    c.rows
        .iter()
        .map(|r| Row {
            name: r.value.clone(),
            sub: r.note.clone(),
            tag: "copy",
            positions: vec![],
            action: Action::Copy(r.copy.clone()),
            learn_key: None,
            warn: false,
            icon: Icon::Calc,
            score: 0,
        })
        .collect()
}

/// ヒットなしのときの行(B5): `Enter`でChatGPT. 2行目にGoogleで探すキーを出す
pub fn chatgpt_row(text: &str, cfg: &Config) -> Row {
    let key = cfg.keys.label(crate::config::KeyAction::Google);
    let sub = if !key.is_empty() {
        format!("No results · {key} for Google")
    } else if !cfg.prefix.google.is_empty() {
        format!("No results · Type \"{} \" first for Google", cfg.prefix.google)
    } else {
        "No results".into()
    };
    Row {
        name: text.trim().to_string(),
        sub,
        tag: "ChatGPT",
        positions: vec![],
        action: Action::ChatGpt(text.trim().to_string()),
        learn_key: None,
        warn: false,
        icon: Icon::ChatGpt,
        score: 0,
    }
}

/// `g `のときの行: `Enter`でGoogle検索
pub fn google_row(text: &str) -> Row {
    Row {
        name: text.trim().to_string(),
        sub: "Search Google".into(),
        tag: "Google",
        positions: vec![],
        action: Action::Google(text.trim().to_string()),
        learn_key: None,
        warn: false,
        icon: Icon::Google,
        score: 0,
    }
}

/// パスの補完(2026-11-24): `%APPDATA%\`や`C:\Users\`のように`\`のあとに, そのフォルダーの中身を出す.
/// `\`のあとに打った文字で絞る(頭が一致するもの → 途中に含むもの, それぞれフォルダーが先).
/// 共有フォルダーとWSL(`\\`で始まる)は, 届かないと待たされるので出さない
fn child_rows(raw: &str, ctx: &Ctx) -> Vec<Row> {
    if raw.starts_with("\\\\") {
        return vec![];
    }
    let Some(i) = raw.rfind('\\') else { return vec![] };
    let (dir_raw, filter) = (&raw[..=i], &raw[i + 1..]);
    let dir = ctx.env.expand(dir_raw);
    let f = filter.to_lowercase();
    let flen = f.chars().count();
    let mut v: Vec<(u8, bool, String, Vec<u32>)> = ctx
        .env
        .list_dir(&dir)
        .into_iter()
        .filter_map(|(name, is_dir)| {
            let l = name.to_lowercase();
            if !f.is_empty() && l == f {
                // 打った名前そのものは1行目(パスの行)に出ている
                return None;
            }
            if f.is_empty() || l.starts_with(&f) {
                Some((0, is_dir, name, (0..flen as u32).collect()))
            } else {
                let at = l.find(&f)?;
                let start = l[..at].chars().count() as u32;
                Some((1, is_dir, name, (start..start + flen as u32).collect()))
            }
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase())));
    v.truncate(MAX_ROWS - 1);
    let parent = dir.trim_end_matches('\\').to_string();
    v.into_iter()
        .map(|(_, is_dir, name, positions)| {
            let full = format!("{parent}\\{name}");
            Row {
                name,
                sub: parent.clone(),
                tag: if is_dir { "folder" } else { "file" },
                positions,
                action: Action::Path(full.clone()),
                learn_key: None,
                warn: false,
                icon: if is_dir { Icon::Kind(Kind::Folder) } else { Icon::Path(full) },
                score: 0,
            }
        })
        .collect()
}

/// 補完で入力欄に入れる文字(`Tab`. K52. 前は`→`). `%APPDATA%`の行は`%APPDATA%\`, パスの中身の行は打ったフォルダー+名前(フォルダーなら`\`も)
pub fn completion(input: &str, row: &Row, is_dir: bool) -> Option<String> {
    if row.name.starts_with('%') && row.name.ends_with('%') && row.name.len() > 2 {
        return Some(format!("{}\\", row.name));
    }
    let Some(Pasted::WinPath(raw)) = pathlike::detect(input) else { return None };
    let Action::Path(p) = &row.action else { return None };
    // `\`で終わる行は打ったフォルダーそのもの(1行目). 足す名前はない(続けて→を押すと名前が重なっていた. 確認8のP5)
    if p.ends_with('\\') {
        return None;
    }
    let i = raw.rfind('\\')?;
    let text = format!("{}{}{}", &raw[..=i], row.name, if is_dir { "\\" } else { "" });
    (text != raw).then_some(text)
}

/// 一覧の行を作る. `file_hits`はファイルの検索の結果(まだなら空. 遅い検索は見つかった分から足す. J1).
pub fn build(input: &str, ctx: &Ctx, prep: &Prepared, file_hits: &[(usize, FileHit)]) -> (Mode, Vec<Row>) {
    let prefixes = ctx.config.prefixes();
    let (mode, text) = parse(input, &prefixes);
    let text_trim = text.trim();
    // 計算の行は計算のモードのときだけ. 答えが出ないときは何も出さない(K50)
    if mode == Mode::Calc {
        return (mode, calc::evaluate(text_trim).map(|c| calc_rows(&c)).unwrap_or_default());
    }
    if text_trim.is_empty() {
        return (mode, vec![]);
    }
    if let Some(words) = help::filter_words(mode, text_trim) {
        return (mode, help::rows(ctx.config, ctx.sources.system, vec![], words));
    }
    if mode == Mode::Google {
        return (mode, vec![google_row(text_trim)]);
    }
    if mode == Mode::ChatGpt {
        let mut r = chatgpt_row(text_trim, ctx.config);
        r.sub = "Ask ChatGPT".into();
        return (mode, vec![r]);
    }
    if mode == Mode::All {
        if let Some(p) = pathlike::detect(input) {
            let mut out = vec![path_row(p.clone(), ctx)];
            if let Pasted::WinPath(raw) = &p {
                let kids = child_rows(raw, ctx);
                // 打ちかけの名前(まだないパス)より, 中身の候補を出す
                if !kids.is_empty() && out[0].sub.starts_with("Not found") {
                    out.clear();
                }
                out.extend(kids);
            }
            return (mode, out);
        }
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut head: Vec<Row> = Vec::new();
    // 絞り込みや式はファイルの索引にかける.
    let filtered = matches!(mode, Mode::All | Mode::File | Mode::Box | Mode::Wsl) && FileQuery::parse(text_trim).is_structured();
    if mode == Mode::All && !filtered {
        if let Some(r) = command_with_arg(text_trim, ctx.config) {
            head.push(r);
        }
    }

    let sources: &[Source] = match mode {
        _ if filtered => &[],
        Mode::All => &[Source::Apps, Source::Bookmarks, Source::Settings, Source::System, Source::Commands, Source::Folders],
        Mode::App => &[Source::Apps],
        Mode::Bookmark => &[Source::Bookmarks],
        Mode::Kill => &[Source::Processes],
        Mode::File => &[Source::Folders],
        Mode::Wsl | Mode::Box | Mode::Google | Mode::ChatGpt | Mode::Clipboard | Mode::Calc | Mode::Snapshot | Mode::Calendar => &[],
    };
    for &s in sources {
        let cat = ctx.sources.get(s);
        for h in cat.search(prep, MAX_ROWS, |_| true) {
            rows.push(item_row(s, h.index, &cat.items[h.index], &h, ctx, text_trim));
        }
    }
    // どの索引を渡すかは画面の側で選ぶ(Boxの索引は`Mode::Box`のときだけ. K20)
    if matches!(mode, Mode::All | Mode::File | Mode::Box | Mode::Wsl) {
        // アプリとして出したショートカット(デスクトップのものなど. K30)は, ファイルの行には出さない
        let app_paths: std::collections::HashSet<String> = rows
            .iter()
            .filter(|r| r.tag == "app")
            .filter_map(|r| if let Icon::Path(p) = &r.icon { Some(p.to_lowercase()) } else { None })
            .collect();
        for (t, h) in file_hits {
            let Some(table) = ctx.tables.get(*t) else { continue };
            let path = table.path(h.id);
            if app_paths.contains(&path.to_lowercase()) {
                continue;
            }
            let is_dir = table.is_dir(h.id);
            let kind = if is_dir { Kind::Folder } else { Kind::File };
            let key = format!("path:{}", path.to_lowercase());
            let score = h.score + kind_weight(kind) + ctx.learn.boost(text_trim, &key, ctx.now);
            rows.push(Row {
                name: table.name(h.id).to_string(),
                sub: table.parent_path(h.id),
                tag: kind.tag(),
                positions: h.positions.clone(),
                action: Action::File { table: *t, id: h.id },
                learn_key: Some(key),
                warn: false,
                icon: if is_dir { Icon::Kind(Kind::Folder) } else { Icon::Path(path) },
                score,
            });
        }
    }
    rows.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.len().cmp(&b.name.len())));
    // 同じものが2回出ないようにする(アプリの一覧とファイルの索引の両方にあるなど)
    let mut seen = std::collections::HashSet::new();
    rows.retain(|r| r.learn_key.as_ref().map_or(true, |k| seen.insert(k.clone())));

    let mut out = head;
    out.extend(rows);
    out.truncate(MAX_ROWS);
    if out.is_empty() && mode == Mode::All && !filtered {
        out.push(chatgpt_row(text_trim, ctx.config));
    }
    (mode, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_lists_keywords_and_commands() {
        let mut cfg = Config::parse("[[command]]\nname = \"jira\"\nurl = \"https://jira.example.com/browse/{q}\"\n").unwrap();
        cfg.prefix.wsl = String::new();
        let rows = help::rows(&cfg, &Catalog::new(builtin::system_items()), vec![], "");
        assert_eq!(rows[0].name, "Apps");
        assert_eq!(rows[0].sub, "a  Search only apps");
        assert_eq!(rows[0].action, Action::Fill("a ".into()));
        // 空にしたキーワード(WSL)は出さない
        assert!(!rows.iter().any(|r| r.name == "WSL"));
        assert!(rows.iter().any(|r| r.name == "Saved results" && r.sub.starts_with("snap ")));
        let last = rows.iter().take_while(|r| matches!(r.tag, "keyword" | "command")).last().unwrap();
        assert_eq!((last.name.as_str(), last.tag), ("jira", "command"));
        assert_eq!(last.action, Action::Fill("jira ".into()));
    }
    use crate::builtin;
    use std::sync::atomic::AtomicBool;

    struct FakeEnv;
    impl Env for FakeEnv {
        fn expand(&self, p: &str) -> String {
            p.replace("%USERPROFILE%", r"C:\Users\me").replace("%APPDATA%", r"C:\Users\me\AppData\Roaming")
        }
        fn path_kind(&self, p: &str) -> PathKind {
            if p.ends_with(".docx") { PathKind::File } else if p.ends_with("Missing") || p.ends_with("hiki") { PathKind::Missing } else { PathKind::Dir }
        }
        fn wsl_unc(&self, p: &str) -> Option<String> {
            Some(pathlike::wsl_to_unc(p, "Ubuntu", Some("/home/me")))
        }
        fn box_path(&self, id: &str, folder: bool) -> Option<(String, bool)> {
            (id == "123" && !folder).then(|| (r"C:\Users\me\Box\Team\plan.xlsx".to_string(), false))
        }
        fn list_dir(&self, dir: &str) -> Vec<(String, bool)> {
            if dir == r"C:\Users\me\AppData\Roaming\" {
                vec![("Hikidashi".into(), true), ("Code".into(), true), ("my-hikidashi.txt".into(), false), ("Microsoft".into(), true)]
            } else {
                vec![]
            }
        }
    }

    struct World {
        cfg: Config,
        apps: Catalog,
        bms: Catalog,
        procs: Catalog,
        sets: Catalog,
        sys: Catalog,
        cmds: Catalog,
        folders: Catalog,
        table: FileTable,
        learn: Learn,
    }

    fn world() -> World {
        let cfg = Config::parse("[alias]\nppt = \"PowerPoint\"\n[[command]]\nname = \"jira\"\nurl = \"https://jira.example.com/browse/{q}\"\n[[command]]\nname = \"dot\"\ntitle = \"Open dotfile\"\nrun = 'code C:\\dot'\n").unwrap();
        let mut apps = vec![
            Item::new(Kind::App, "PowerPoint", "", r"C:\ProgramData\Start Menu\PowerPoint.lnk").with_keys(&["powerpnt"]),
            Item::new(Kind::App, "Notepad", "", r"C:\Start Menu\Notepad.lnk").with_keys(&["notepad"]),
        ];
        apply_aliases(&mut apps, &cfg.alias);
        let mut table = FileTable::new('\\');
        let root = table.push(None, r"C:\Users\me", true, 0);
        let docs = table.push(Some(root), "Documents", true, 0);
        table.push(Some(docs), "proposal.docx", false, 0);
        table.push(Some(docs), "2024_budget.xlsx", false, 0);
        table.push(Some(docs), "定例会議.xlsx", false, 0);
        let mut kill = Item::new(Kind::Process, "chrome.exe ×20", "No window", "chrome.exe");
        kill.data = 1;
        World {
            apps: Catalog::new(apps),
            bms: Catalog::new(vec![Item::new(Kind::Bookmark, "Project board", "Bookmarks bar", "https://example.com/board")]),
            procs: Catalog::new(vec![kill]),
            sets: Catalog::new(builtin::settings_items()),
            sys: Catalog::new(builtin::system_items()),
            cmds: Catalog::new(command_items(&cfg)),
            folders: Catalog::new(builtin::folder_items(&[("APPDATA".into(), r"C:\Users\me\AppData\Roaming".into()), ("LOCALAPPDATA".into(), r"C:\Users\me\AppData\Local".into())])),
            cfg,
            table,
            learn: Learn::default(),
        }
    }

    fn run(w: &World, input: &str) -> (Mode, Vec<Row>) {
        let tables = [&w.table];
        let ctx = Ctx {
            config: &w.cfg,
            sources: Sources { apps: &w.apps, bookmarks: &w.bms, processes: &w.procs, settings: &w.sets, system: &w.sys, commands: &w.cmds, folders: &w.folders },
            tables: &tables,
            learn: &w.learn,
            now: 0,
            env: &FakeEnv,
        };
        let (_, text) = parse(input, &w.cfg.prefixes());
        let prep = Prepared::new(text.trim(), None);
        let query = FileQuery::parse(text.trim()).prepare(None, 0);
        let cancel = AtomicBool::new(false);
        let hits: Vec<(usize, FileHit)> = w.table.search_query(&query, 50, &cancel, false).into_iter().map(|h| (0, h)).collect();
        build(input, &ctx, &prep, &hits)
    }

    fn names(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn google_prefix_and_chatgpt_hint() {
        let w = world();
        let (m, rows) = run(&w, "g 明日の天気");
        assert_eq!(m, Mode::Google);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, Action::Google("明日の天気".into()));
        let (_, rows) = run(&w, "how to rename a git branch");
        assert_eq!(rows[0].sub, "No results · Ctrl+Alt+Enter for Google");
    }

    #[test]
    fn env_folders_and_completion() {
        let w = world();
        // %なしでも, 途中までの%でも出る
        let (_, rows) = run(&w, "appdata");
        assert!(names(&rows).contains(&"%APPDATA%"), "{:?}", names(&rows));
        assert!(names(&rows).contains(&"%LOCALAPPDATA%"), "{:?}", names(&rows));
        let (_, rows) = run(&w, "%app");
        assert_eq!(rows[0].name, "%APPDATA%");
        assert_eq!(completion("%app", &rows[0], true).as_deref(), Some(r"%APPDATA%\"));
        // \のあとに中身. フォルダーが先
        let (_, rows) = run(&w, r"%APPDATA%\");
        assert_eq!(names(&rows), vec!["Roaming", "Code", "Hikidashi", "Microsoft", "my-hikidashi.txt"]);
        // 打ちかけの名前で絞る. まだないパスの行は出さない
        let (_, rows) = run(&w, r"%APPDATA%\hiki");
        assert_eq!(names(&rows), vec!["Hikidashi", "my-hikidashi.txt"]);
        assert_eq!(rows[0].action, Action::Path(r"C:\Users\me\AppData\Roaming\Hikidashi".into()));
        assert_eq!(rows[0].positions, vec![0, 1, 2, 3]);
        assert_eq!(rows[1].positions, vec![3, 4, 5, 6]);
        assert_eq!(completion(r"%APPDATA%\hiki", &rows[0], true).as_deref(), Some(r"%APPDATA%\Hikidashi\"));
        assert_eq!(completion(r"%APPDATA%\hiki", &rows[1], false).as_deref(), Some(r"%APPDATA%\my-hikidashi.txt"));
        // 補完したあと(1行目はそのフォルダー自身)に→を続けて押しても, 名前は重ならない
        let (_, rows) = run(&w, r"%APPDATA%\Hikidashi\");
        assert_eq!(completion(r"%APPDATA%\Hikidashi\", &rows[0], true), None);
        // \なしでフォルダー名まで打ったときは\を足す
        let (_, rows) = run(&w, r"%APPDATA%\Hikidashi");
        assert_eq!(completion(r"%APPDATA%\Hikidashi", &rows[0], true).as_deref(), Some(r"%APPDATA%\Hikidashi\"));
    }

    #[test]
    fn prefixes() {
        let p = Config::default().prefixes();
        assert_eq!(parse("f report", &p), (Mode::File, "report"));
        assert_eq!(parse("k chrome", &p), (Mode::Kill, "chrome"));
        assert_eq!(parse("fo", &p), (Mode::All, "fo"));
        assert_eq!(parse("f", &p), (Mode::All, "f"));
        assert_eq!(parse("f ", &p), (Mode::File, ""));
        // Boxは`bx`. `b`(ブックマーク)とぶつからない(K20)
        assert_eq!(parse("bx report", &p), (Mode::Box, "report"));
        assert_eq!(parse("b report", &p), (Mode::Bookmark, "report"));
        assert_eq!(parse("bxreport", &p), (Mode::All, "bxreport"));
        assert_eq!(parse("会議", &p), (Mode::All, "会議"));
        assert_eq!(parse("é x", &p), (Mode::All, "é x"));
    }

    #[test]
    fn pasted_path_is_only_row() {
        let w = world();
        let (_, rows) = run(&w, r#""%USERPROFILE%\Documents\報告書.docx""#);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "報告書.docx");
        assert_eq!(rows[0].sub, r"C:\Users\me\Documents");
        assert_eq!(rows[0].tag, "file");
        assert_eq!(rows[0].action, Action::Path(r"C:\Users\me\Documents\報告書.docx".into()));
        let (_, rows) = run(&w, r"C:\Users\me\Missing");
        assert!(rows[0].sub.starts_with("Not found"));
        let (_, rows) = run(&w, "https://example.box.com/s/abc");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, Action::OpenUrl("https://example.box.com/s/abc".into()));
        assert_eq!(rows[0].sub, "Open in browser");
        // BoxのURLは, Box Driveの手元のパスに引けたらファイルの行(K18)
        let (_, rows) = run(&w, "https://example.ent.box.com/file/123");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "plan.xlsx");
        assert_eq!(rows[0].sub, r"C:\Users\me\Box\Team");
        assert_eq!(rows[0].tag, "file");
        assert!(matches!(&rows[0].action, Action::BoxUrl { local: Some(p), .. } if p.ends_with("plan.xlsx")));
        let (_, rows) = run(&w, "https://example.ent.box.com/file/999");
        assert_eq!(rows[0].tag, "url");
        assert!(matches!(&rows[0].action, Action::BoxUrl { local: None, .. }));
        let (_, rows) = run(&w, "~/project");
        assert_eq!(rows[0].action, Action::Path(r"\\wsl.localhost\Ubuntu\home\me\project".into()));
    }

    #[test]
    fn mixed_search_and_tags() {
        let w = world();
        let (_, rows) = run(&w, "pwpt");
        assert_eq!(rows[0].name, "PowerPoint");
        assert_eq!(rows[0].tag, "app");
        let (_, rows) = run(&w, "ppt");
        assert_eq!(rows[0].name, "PowerPoint");
        assert_eq!(rows[0].sub, "ppt");
        let (_, rows) = run(&w, "prop");
        assert_eq!(rows[0].name, "proposal.docx");
        assert_eq!(rows[0].tag, "file");
        // 窓とプロセスは混ぜない
        assert!(!rows.iter().any(|r| r.tag == "window" || r.tag == "kill"));
    }

    #[test]
    fn modes() {
        let w = world();
        let (m, rows) = run(&w, "k chr");
        assert_eq!(m, Mode::Kill);
        assert_eq!(rows[0].tag, "kill");
        assert!(rows[0].warn);
        // `w `はWSLのファイルだけ(K24. どの索引を渡すかは画面の側). 窓の切り替えはもうない
        let (m, rows) = run(&w, "w prop");
        assert_eq!(m, Mode::Wsl);
        assert!(rows.iter().all(|r| r.tag == "file" || r.tag == "folder"), "{:?}", names(&rows));
        // `c `はChatGPT(画面ではすぐ書く窓になる. 窓がないときの1行)
        let (m, rows) = run(&w, "c rebaseとmergeの違い");
        assert_eq!(m, Mode::ChatGpt);
        assert_eq!(rows[0].action, Action::ChatGpt("rebaseとmergeの違い".into()));
        assert_eq!(rows[0].sub, "Ask ChatGPT");
        let (_, rows) = run(&w, "f doc");
        assert!(rows.iter().all(|r| r.tag == "file" || r.tag == "folder"));
        let (_, rows) = run(&w, "b proj");
        assert_eq!(names(&rows), vec!["Project board"]);
        // Boxのときはファイルとフォルダーだけ(渡された索引から). アプリやChatGPTの行は出さない(K20)
        let (m, rows) = run(&w, "bx budget");
        assert_eq!(m, Mode::Box);
        assert_eq!(names(&rows), vec!["2024_budget.xlsx"]);
        let (_, rows) = run(&w, "bx zzzz");
        assert!(rows.is_empty());
    }

    #[test]
    fn numbers_and_calc() {
        let w = world();
        // ふだんの検索には計算の行を出さない
        let (_, rows) = run(&w, "2024");
        assert_eq!(rows[0].name, "2024_budget.xlsx");
        assert!(rows.iter().all(|r| r.tag != "copy"));
        let (_, rows) = run(&w, "0x1F");
        assert!(rows.iter().all(|r| r.tag != "copy"));
        let (_, rows) = run(&w, "(12+3)*4");
        assert!(rows.iter().all(|r| r.tag != "copy"));
    }

    #[test]
    fn calc_mode() {
        let w = world();
        let (m, rows) = run(&w, "= 0x1F");
        assert_eq!(m, Mode::Calc);
        assert_eq!(Mode::Calc.label(), "calc");
        assert_eq!(names(&rows), vec!["31", "0001 1111"]);
        let (_, rows) = run(&w, "= 10nH@2.4GHz");
        assert_eq!(names(&rows), vec!["150.8 Ω"]);
        assert_eq!(rows[0].action, Action::Copy("150.8 Ω".into()));
        let (_, rows) = run(&w, "= (12+3)*4");
        assert_eq!(names(&rows), vec!["60"]);
        // 1文字から. アプリやファイルは出さない(`doc`はファイルに一致するが出ない)
        let (_, rows) = run(&w, "= 5");
        assert_eq!(names(&rows), vec!["0x5", "0101"]);
        let (_, rows) = run(&w, "= doc");
        assert!(rows.is_empty(), "{:?}", names(&rows));
        // 空のときも例は出さない(K50)
        assert!(run(&w, "= ").1.is_empty());
    }

    #[test]
    fn commands_and_chatgpt() {
        let w = world();
        let (_, rows) = run(&w, "jira ABC-123");
        assert_eq!(rows[0].action, Action::OpenUrl("https://jira.example.com/browse/ABC-123".into()));
        let (_, rows) = run(&w, "dot");
        assert_eq!(rows[0].name, "Open dotfile");
        assert_eq!(rows[0].tag, "run");
        let (_, rows) = run(&w, "how to rename a branch");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tag, "ChatGPT");
        assert_eq!(rows[0].action, Action::ChatGpt("how to rename a branch".into()));
    }

    #[test]
    fn settings_and_system() {
        let w = world();
        let (_, rows) = run(&w, "display");
        assert_eq!(rows[0].name, "Display");
        assert_eq!(rows[0].tag, "setting");
        let (_, rows) = run(&w, "shut");
        assert_eq!(rows[0].name, "Shut down");
        assert_eq!(rows[0].tag, "system");
    }

    #[test]
    fn odd_inputs_do_not_panic() {
        let w = world();
        let samples = [
            "", " ", "f", "f ", "k ", "a  b", "会議", "ｆ ", "é", "😀", "😀 x", "\\", "\\\\", "C:", "C:\\", "%", "%%", "%A%", "0x", "0b", "0xZZ", "h", "1h",
            "@", "@GHz", "nH@", "1nH@", "vswr", "vswr 0.5", "rl", "g 2", "x", "x0", "-", "--", "/mnt/", "/mnt/c", "~/", "file://", "file:///", "https://app.box.com/file/",
            "jira", "jira ", "jira 日本", "dot x", "\u{0}", "a\nb", "　", "ｆｏｏ", "kaig", "n", "nn", "xtu", "ー", "1e999dBm", "99999999999999999999999999999999999999999",
        ];
        for s in samples {
            let _ = run(&w, s);
            for i in 0..s.len() {
                if let Some(p) = s.get(..i) {
                    let _ = run(&w, p);
                }
            }
        }
    }

    #[test]
    fn exact_command_beats_files() {
        let mut w = world();
        // `lock`を名前に含むファイルがたくさんあっても, Lockの操作が1行目
        let mut table = FileTable::new('\\');
        let root = table.push(None, r"C:\Users\me", true, 0);
        for i in 0..30 {
            table.push(Some(root), &format!("lock{i}.txt"), false, 0);
        }
        table.push(Some(root), "lock", false, 0);
        w.table = table;
        let (_, rows) = run(&w, "lock");
        assert_eq!(rows[0].name, "Lock");
        assert_eq!(rows[0].tag, "system");
        let (_, rows) = run(&w, "settings");
        assert_eq!(rows[0].name, "Hikidashi settings");
        let (_, rows) = run(&w, "hikidashi");
        assert_eq!(rows[0].name, "Hikidashi settings");
    }

    #[test]
    fn filters_show_only_files() {
        let w = world();
        // 絞り込みだけでも出る. アプリや計算は混ぜない
        let (_, rows) = run(&w, "ext:xlsx");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.tag == "file"));
        let (_, rows) = run(&w, "budget & ext:xlsx");
        assert_eq!(names(&rows), vec!["2024_budget.xlsx"]);
        let (_, rows) = run(&w, "f (*.docx | *.xlsx) & !budget");
        assert_eq!(names(&rows), vec!["proposal.docx", "定例会議.xlsx"]);
        let (_, rows) = run(&w, "*.xlsx & !in:Documents");
        assert!(rows.is_empty());
        let (_, rows) = run(&w, "budget ext:xlsx");
        assert!(rows.iter().all(|r| r.tag != "file"));
        let (_, rows) = run(&w, "g budget & ext:xlsx");
        assert_eq!(rows[0].action, Action::Google("budget & ext:xlsx".into()));
        // 値のない`ext:`は普通の語
        let (_, rows) = run(&w, "ext:");
        assert!(rows.iter().all(|r| r.tag != "file"));
    }

    #[test]
    fn keeps_more_than_visible_rows() {
        let mut w = world();
        let mut table = FileTable::new('\\');
        let root = table.push(None, r"C:\Users\me", true, 0);
        for i in 0..80 {
            table.push(Some(root), &format!("report{i}.txt"), false, 0);
        }
        w.table = table;
        let (_, rows) = run(&w, "report");
        assert!(rows.len() > VISIBLE_ROWS, "{}", rows.len());
        assert!(rows.len() <= MAX_ROWS);
    }

    #[test]
    fn learning_moves_item_up() {
        let mut w = world();
        let (_, rows) = run(&w, "no");
        let first = rows[0].name.clone();
        let other = rows.iter().find(|r| r.name != first).unwrap().clone();
        w.learn.record("no", other.learn_key.as_ref().unwrap(), 0);
        let (_, rows) = run(&w, "no");
        assert_eq!(rows[0].name, other.name);
    }

    #[test]
    fn clipboard_prefix_is_a_mode() {
        let w = world();
        let (m, rows) = run(&w, "cb report");
        assert_eq!(m, Mode::Clipboard);
        // 行は画面の側が`clip::rows`で作る. ふだんの検索の結果は混ぜない
        assert!(rows.is_empty());
        assert_eq!(Mode::Clipboard.label(), "clipboard");
        // `c `(ChatGPT)とは別
        let (m, _) = run(&w, "c hello");
        assert_eq!(m, Mode::ChatGpt);
    }
}
