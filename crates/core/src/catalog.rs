//! 数の少ない一覧(アプリ, ブックマーク, 窓, プロセス, 設定のページ, Windowsの操作, 自作コマンド).
//! ファイルは数十万件になるので`files`で別に持つ.

use crate::fuzzy::{self, lower_str};
use crate::search::{match_name, Prepared, TopK};

/// 項目の種類. 右端に出す文字(K9)もここで決める.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    App,
    File,
    Folder,
    Bookmark,
    Window,
    Process,
    Setting,
    System,
    Command,
}

impl Kind {
    /// 行の右端に出す文字(K2, K9). 開く以外のことが起きる種類は動作の名前にする.
    pub fn tag(self) -> &'static str {
        match self {
            Kind::App => "app",
            Kind::File => "file",
            Kind::Folder => "folder",
            Kind::Bookmark => "bookmark",
            Kind::Window => "window",
            Kind::Process => "kill",
            Kind::Setting => "setting",
            Kind::System => "system",
            Kind::Command => "run",
        }
    }

    /// 学習(B4)の鍵の頭
    pub fn key_prefix(self) -> &'static str {
        match self {
            Kind::App => "app",
            Kind::File | Kind::Folder => "path",
            Kind::Bookmark => "bm",
            Kind::Window => "win",
            Kind::Process => "proc",
            Kind::Setting => "set",
            Kind::System => "sys",
            Kind::Command => "cmd",
        }
    }
}

/// 1つの項目
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: Kind,
    /// 1行目に出す名前
    pub name: String,
    /// 2行目に小さく出す文字(パスなど)
    pub sub: String,
    /// 開く対象(パス, URL, `ms-settings:`, コマンドなど)
    pub target: String,
    /// 名前のほかに一致に使う語(exe名, 別名, 日本語の名前など. J6)
    pub keys: Vec<String>,
    /// 種類ごとの追加の値(窓のハンドル, プロセスID, 引数など)
    pub data: u64,
    pub args: String,
}

impl Item {
    pub fn new(kind: Kind, name: impl Into<String>, sub: impl Into<String>, target: impl Into<String>) -> Item {
        Item { kind, name: name.into(), sub: sub.into(), target: target.into(), keys: vec![], data: 0, args: String::new() }
    }

    pub fn with_keys(mut self, keys: &[&str]) -> Item {
        self.keys.extend(keys.iter().map(|k| k.to_string()));
        self
    }

    /// 学習(B4)に使う, 項目を区別する鍵
    pub fn learn_key(&self) -> String {
        let t = match self.kind {
            Kind::Window | Kind::Process => self.name.as_str(),
            _ => self.target.as_str(),
        };
        // ブックマークは, 同じURLでもChromeとEdgeを別のものにする(両方出すため. 2026-11-24)
        if self.kind == Kind::Bookmark && !self.args.is_empty() {
            return format!("{}:{}:{}", self.kind.key_prefix(), self.args, t.to_lowercase());
        }
        format!("{}:{}", self.kind.key_prefix(), t.to_lowercase())
    }
}

/// 一致した項目. `positions`は名前の中で一致した文字(J7の太字). 別名で一致したときは空.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub score: i32,
    pub positions: Vec<u32>,
    /// 別名などで一致したとき, その語
    pub via: Option<String>,
}

struct Prepped {
    lower: String,
    ascii: bool,
    keys: Vec<(String, bool)>,
}

pub struct Catalog {
    pub items: Vec<Item>,
    prepped: Vec<Prepped>,
}

impl Catalog {
    pub fn new(items: Vec<Item>) -> Catalog {
        let prepped = items
            .iter()
            .map(|it| Prepped {
                lower: lower_str(&it.name),
                ascii: it.name.is_ascii(),
                keys: it.keys.iter().map(|k| (lower_str(k), k.is_ascii())).collect(),
            })
            .collect();
        Catalog { items, prepped }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 上位k件. `filter`がfalseを返す項目は飛ばす.
    pub fn search(&self, p: &Prepared, k: usize, filter: impl Fn(&Item) -> bool) -> Vec<Hit> {
        let mut top = TopK::new(k);
        for (i, (it, pp)) in self.items.iter().zip(&self.prepped).enumerate() {
            if !filter(it) {
                continue;
            }
            let mut best: Option<Hit> = None;
            if let Some(m) = match_name(p, &it.name, &pp.lower, pp.ascii) {
                let score = fuzzy::rank_score(&m, &it.name);
                best = Some(Hit { index: i, score, positions: m.positions, via: None });
            }
            for (key, (kl, ka)) in it.keys.iter().zip(&pp.keys) {
                if let Some(m) = match_name(p, key, kl, *ka) {
                    // 別名は名前より少しだけ下げる. ただし別名にちょうど一致したら上げる(`calc`で電卓)
                    let exact = kl.as_str() == p.text.trim().to_lowercase();
                    let score = fuzzy::rank_score(&m, key) - 4 + if exact { 40 } else { 0 };
                    if best.as_ref().map_or(true, |b| score > b.score) {
                        best = Some(Hit { index: i, score, positions: vec![], via: Some(key.clone()) });
                    }
                }
            }
            if let Some(h) = best {
                top.push(h.score, h);
            }
        }
        top.into_sorted().into_iter().map(|(_, h)| h).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Catalog {
        Catalog::new(vec![
            Item::new(Kind::App, "PowerPoint", "", r"C:\ppt.lnk").with_keys(&["powerpnt", "ppt"]),
            Item::new(Kind::App, "Word", "", r"C:\word.lnk").with_keys(&["winword"]),
            Item::new(Kind::App, "電卓", "", "calc").with_keys(&["calc", "calculator"]),
            Item::new(Kind::Bookmark, "Project board", "", "https://example.com"),
        ])
    }

    #[test]
    fn name_and_alias() {
        let c = cat();
        let p = Prepared::new("pwpt", None);
        let h = c.search(&p, 8, |_| true);
        assert_eq!(c.items[h[0].index].name, "PowerPoint");
        assert_eq!(h[0].positions, vec![0, 2, 5, 9]);

        let p = Prepared::new("winword", None);
        let h = c.search(&p, 8, |_| true);
        assert_eq!(c.items[h[0].index].name, "Word");
        assert_eq!(h[0].via.as_deref(), Some("winword"));

        let p = Prepared::new("calc", None);
        let h = c.search(&p, 8, |_| true);
        assert_eq!(c.items[h[0].index].name, "電卓");
    }

    #[test]
    fn filter_by_kind() {
        let c = cat();
        let p = Prepared::new("p", None);
        let h = c.search(&p, 8, |it| it.kind == Kind::Bookmark);
        assert_eq!(h.len(), 1);
        assert_eq!(c.items[h[0].index].name, "Project board");
    }
}
