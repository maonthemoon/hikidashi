//! 入力欄が空のときに出す, 最近開いたもの. 学習の記録(`Learn`)の「最後に使った時刻」の新しい順に, 今も存在するものだけ.

use crate::catalog::Kind;
use crate::learn::Learn;
use crate::pathlike;
use crate::router::{item_icon, Action, Ctx, Icon, Row, Source};
use std::collections::{HashMap, HashSet};

/// 出す件数の上限
pub const RECENT_ROWS: usize = 10;
/// 存在を確かめるために読む記録の数(消えたものを飛ばす余裕)
const POOL: usize = 60;

const SOURCES: [Source; 6] = [Source::Apps, Source::Bookmarks, Source::Settings, Source::System, Source::Commands, Source::Folders];

/// 最近開いたものの行(新しい順). 窓の一覧(`win:`)とプロセス(`proc:`)は出さない
pub fn rows(ctx: &Ctx) -> Vec<Row> {
    rows_from(ctx.learn, ctx, RECENT_ROWS)
}

fn rows_from(learn: &Learn, ctx: &Ctx, limit: usize) -> Vec<Row> {
    let pool: Vec<(String, u64)> = learn.recent(POOL).into_iter().filter(|(k, _)| !k.starts_with("win:") && !k.starts_with("proc:")).collect();
    if pool.is_empty() {
        return vec![];
    }
    // 一覧の項目は鍵で引く. 鍵は項目から作る(学習のときと同じ)
    let mut by_key: HashMap<String, (Source, usize)> = HashMap::new();
    let wanted: HashSet<&str> = pool.iter().map(|(k, _)| k.as_str()).collect();
    for s in SOURCES {
        for (i, it) in ctx.sources.get(s).items.iter().enumerate() {
            let k = it.learn_key();
            if wanted.contains(k.as_str()) {
                by_key.entry(k).or_insert((s, i));
            }
        }
    }
    let mut out: Vec<Row> = vec![];
    for (key, _) in pool {
        if out.len() >= limit {
            break;
        }
        if out.iter().any(|r| r.learn_key.as_deref() == Some(key.as_str())) {
            continue;
        }
        if let Some(&(src, index)) = by_key.get(&key) {
            let it = &ctx.sources.get(src).items[index];
            out.push(Row {
                name: it.name.clone(),
                sub: it.sub.clone(),
                tag: it.kind.tag(),
                positions: vec![],
                action: Action::Item { source: src, index },
                learn_key: Some(key),
                warn: false,
                icon: item_icon(it),
                score: 0,
            });
        } else if let Some(lower) = key.strip_prefix("path:") {
            let Some((full, is_dir)) = ctx.env.resolve_path(lower) else { continue };
            let (name, parent) = pathlike::split_name(&full);
            let name = if name.is_empty() { full.clone() } else { name.to_string() };
            out.push(Row {
                name,
                sub: parent.to_string(),
                tag: if is_dir { "folder" } else { "file" },
                positions: vec![],
                action: Action::Path(full.clone()),
                learn_key: Some(key),
                warn: false,
                icon: if is_dir { Icon::Kind(Kind::Folder) } else { Icon::Path(full) },
                score: 0,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, Item};
    use crate::config::Config;
    use crate::router::{Env, PathKind, Sources};

    struct FakeEnv;
    impl Env for FakeEnv {
        fn expand(&self, p: &str) -> String {
            p.to_string()
        }
        fn path_kind(&self, _p: &str) -> PathKind {
            PathKind::File
        }
        fn wsl_unc(&self, _p: &str) -> Option<String> {
            None
        }
        fn resolve_path(&self, lower: &str) -> Option<(String, bool)> {
            match lower {
                r"c:\docs\report.xlsx" => Some((r"C:\Docs\Report.xlsx".into(), false)),
                r"c:\docs" => Some((r"C:\Docs".into(), true)),
                _ => None,
            }
        }
    }

    #[test]
    fn newest_first_and_only_existing() {
        let cfg = Config::default();
        let apps = Catalog::new(vec![Item::new(Kind::App, "Notepad", "", r"C:\Start\Notepad.lnk"), Item::new(Kind::App, "Word", "", r"C:\Start\Word.lnk")]);
        let bms = Catalog::new(vec![Item::new(Kind::Bookmark, "Board", "Chrome", "https://example.com/board")]);
        let empty = Catalog::new(vec![]);
        let mut l = Learn::default();
        l.record("", "app:c:\\start\\word.lnk", 10);
        l.record("", "path:c:\\docs\\report.xlsx", 20);
        l.record("", "path:c:\\gone\\x.txt", 30);
        l.record("", "bm:https://example.com/board", 40);
        l.record("", "win:Some window", 50);
        l.record("", "proc:chrome.exe", 60);
        l.record("", "path:c:\\docs", 5);
        let ctx = Ctx {
            config: &cfg,
            sources: Sources { apps: &apps, bookmarks: &bms, processes: &empty, settings: &empty, system: &empty, commands: &empty, folders: &empty },
            tables: &[],
            learn: &l,
            now: 100,
            env: &FakeEnv,
        };
        let r = rows(&ctx);
        let names: Vec<&str> = r.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["Board", "Report.xlsx", "Word", "Docs"]);
        assert_eq!(r[0].tag, "bookmark");
        assert_eq!(r[1].sub, r"C:\Docs");
        assert_eq!(r[1].tag, "file");
        assert_eq!(r[3].tag, "folder");
        assert!(matches!(r[2].action, Action::Item { source: Source::Apps, index: 1 }));
        assert_eq!(r[2].learn_key.as_deref(), Some("app:c:\\start\\word.lnk"));
    }

    #[test]
    fn at_most_ten() {
        let cfg = Config::default();
        let items: Vec<Item> = (0..30).map(|i| Item::new(Kind::App, format!("App{i}"), "", format!(r"C:\a\{i}.lnk"))).collect();
        let apps = Catalog::new(items.clone());
        let empty = Catalog::new(vec![]);
        let mut l = Learn::default();
        for (i, it) in items.iter().enumerate() {
            l.record("", &it.learn_key(), i as u64);
        }
        let ctx = Ctx {
            config: &cfg,
            sources: Sources { apps: &apps, bookmarks: &empty, processes: &empty, settings: &empty, system: &empty, commands: &empty, folders: &empty },
            tables: &[],
            learn: &l,
            now: 100,
            env: &FakeEnv,
        };
        let r = rows(&ctx);
        assert_eq!(r.len(), RECENT_ROWS);
        assert_eq!(r[0].name, "App29");
    }
}
