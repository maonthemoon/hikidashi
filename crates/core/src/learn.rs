//! 並び順の学習(B4)と入力の履歴(J5). ファイルへの読み書きは呼ぶ側(アプリ)が行い, ここは文字列との変換だけ.

use std::collections::{HashMap, VecDeque};

/// 「この入力でこれを選んだ」を1位にする加点. 名前の先頭の一致(64点)よりずっと大きい.
pub const BOOST_EXACT: i32 = 10_000;
/// よく使う・最近使ったものの加点の上限. 名前の先頭の一致には勝たない大きさ(B4).
pub const BOOST_MAX_USAGE: i32 = 40;
/// 1つの入力について覚える項目の数
const PER_QUERY: usize = 5;
/// 覚える入力の数
const MAX_QUERIES: usize = 2000;
const MAX_ITEMS: usize = 5000;

#[derive(Debug, Clone, Default, PartialEq)]
struct Usage {
    count: u32,
    last: u64,
}

/// 学習の記録
#[derive(Debug, Default, Clone)]
pub struct Learn {
    /// 入力(小文字) → 選んだ項目の鍵(新しい順)
    by_query: HashMap<String, Vec<String>>,
    /// 項目の鍵 → 使った回数と最後に使った時刻(秒)
    usage: HashMap<String, Usage>,
}

fn norm(q: &str) -> String {
    q.trim().to_lowercase()
}

impl Learn {
    /// `query`で`key`の項目を選んだことを覚える. `now`はUNIX時刻(秒).
    pub fn record(&mut self, query: &str, key: &str, now: u64) {
        let q = norm(query);
        if !q.is_empty() {
            let list = self.by_query.entry(q).or_default();
            list.retain(|k| k != key);
            list.insert(0, key.to_string());
            list.truncate(PER_QUERY);
        }
        let u = self.usage.entry(key.to_string()).or_default();
        u.count = u.count.saturating_add(1);
        u.last = now;
        self.trim();
    }

    fn trim(&mut self) {
        if self.usage.len() > MAX_ITEMS {
            let mut v: Vec<(String, u64)> = self.usage.iter().map(|(k, u)| (k.clone(), u.last)).collect();
            v.sort_by_key(|(_, l)| *l);
            for (k, _) in v.iter().take(self.usage.len() - MAX_ITEMS) {
                self.usage.remove(k);
            }
        }
        if self.by_query.len() > MAX_QUERIES {
            let keys: Vec<String> = self.by_query.keys().take(self.by_query.len() - MAX_QUERIES).cloned().collect();
            for k in keys {
                self.by_query.remove(&k);
            }
        }
    }

    /// 項目の加点. 同じ入力で選んだことがあれば大きく, 使った回数と新しさで少し.
    pub fn boost(&self, query: &str, key: &str, now: u64) -> i32 {
        let mut b = 0;
        if let Some(list) = self.by_query.get(&norm(query)) {
            if let Some(pos) = list.iter().position(|k| k == key) {
                b += BOOST_EXACT - pos as i32 * 100;
            }
        }
        if let Some(u) = self.usage.get(key) {
            let freq = ((u.count as f64).ln_1p() * 8.0) as i32; // 1回で5, 10回で19, 100回で36
            let age_days = now.saturating_sub(u.last) / 86_400;
            let recent = match age_days {
                0 => 12,
                1..=6 => 8,
                7..=29 => 4,
                _ => 0,
            };
            b += (freq + recent).min(BOOST_MAX_USAGE);
        }
        b
    }

    /// 保存用の文字列. 1行に1件, タブ区切り. `q\t入力\t鍵1\t鍵2…`, `u\t鍵\t回数\t時刻`
    pub fn to_text(&self) -> String {
        let mut out = String::from("# Hikidashi learn v1\n");
        let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
        for (q, keys) in &self.by_query {
            out.push_str("q\t");
            out.push_str(&clean(q));
            for k in keys {
                out.push('\t');
                out.push_str(&clean(k));
            }
            out.push('\n');
        }
        for (k, u) in &self.usage {
            out.push_str(&format!("u\t{}\t{}\t{}\n", clean(k), u.count, u.last));
        }
        out
    }

    pub fn from_text(s: &str) -> Learn {
        let mut l = Learn::default();
        for line in s.lines() {
            let mut it = line.split('\t');
            match it.next() {
                Some("q") => {
                    if let Some(q) = it.next() {
                        l.by_query.insert(q.to_string(), it.map(str::to_string).collect());
                    }
                }
                Some("u") => {
                    if let (Some(k), Some(c), Some(t)) = (it.next(), it.next(), it.next()) {
                        l.usage.insert(k.to_string(), Usage { count: c.parse().unwrap_or(0), last: t.parse().unwrap_or(0) });
                    }
                }
                _ => {}
            }
        }
        l
    }
}

/// 入力の履歴(J5). 入力欄が空のとき↑で1つ前, もう1回で2つ前. 最大100件.
#[derive(Debug, Default, Clone)]
pub struct History {
    items: VecDeque<String>,
    pos: Option<usize>,
}

pub const HISTORY_MAX: usize = 100;

impl History {
    pub fn push(&mut self, s: &str) {
        let s = s.trim();
        if s.is_empty() {
            return;
        }
        self.items.retain(|x| x != s);
        self.items.push_front(s.to_string());
        self.items.truncate(HISTORY_MAX);
        self.pos = None;
    }

    /// ↑: 1つ古いものへ
    pub fn older(&mut self) -> Option<&str> {
        let next = match self.pos {
            None => 0,
            Some(p) => (p + 1).min(self.items.len().saturating_sub(1)),
        };
        if self.items.is_empty() {
            return None;
        }
        self.pos = Some(next);
        self.items.get(next).map(String::as_str)
    }

    /// ↓: 1つ新しいものへ. 一番新しいところより先は空に戻る.
    pub fn newer(&mut self) -> Option<&str> {
        match self.pos {
            None | Some(0) => {
                self.pos = None;
                None
            }
            Some(p) => {
                self.pos = Some(p - 1);
                self.items.get(p - 1).map(String::as_str)
            }
        }
    }

    pub fn is_browsing(&self) -> bool {
        self.pos.is_some()
    }

    pub fn reset(&mut self) {
        self.pos = None;
    }

    pub fn to_text(&self) -> String {
        self.items.iter().map(|s| s.replace(['\n', '\r'], " ") + "\n").collect()
    }

    pub fn from_text(s: &str) -> History {
        History { items: s.lines().filter(|l| !l.trim().is_empty()).take(HISTORY_MAX).map(str::to_string).collect(), pos: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_query_wins() {
        let mut l = Learn::default();
        l.record("pp", "app:ppt", 1000);
        assert!(l.boost("pp", "app:ppt", 1000) >= BOOST_EXACT);
        assert!(l.boost("PP ", "app:ppt", 1000) >= BOOST_EXACT);
        let other = l.boost("p", "app:ppt", 1000);
        assert!(other > 0 && other <= BOOST_MAX_USAGE);
        assert_eq!(l.boost("pp", "app:word", 1000), 0);
    }

    #[test]
    fn usage_is_capped_and_ages() {
        let mut l = Learn::default();
        for _ in 0..1000 {
            l.record("", "app:x", 0);
        }
        assert_eq!(l.boost("q", "app:x", 0), BOOST_MAX_USAGE);
        let mut l = Learn::default();
        for _ in 0..3 {
            l.record("", "app:y", 0);
        }
        assert!(l.boost("q", "app:y", 60 * 86_400) < l.boost("q", "app:y", 0));
    }

    #[test]
    fn learn_roundtrip() {
        let mut l = Learn::default();
        l.record("kaigi", "path:c:\\a\\会議.xlsx", 5);
        let t = l.to_text();
        let l2 = Learn::from_text(&t);
        assert_eq!(l2.boost("kaigi", "path:c:\\a\\会議.xlsx", 5), l.boost("kaigi", "path:c:\\a\\会議.xlsx", 5));
    }

    #[test]
    fn history_navigation() {
        let mut h = History::default();
        h.push("a");
        h.push("b");
        h.push("c");
        h.push("b");
        assert_eq!(h.older(), Some("b"));
        assert_eq!(h.older(), Some("c"));
        assert_eq!(h.older(), Some("a"));
        assert_eq!(h.older(), Some("a"));
        assert_eq!(h.newer(), Some("c"));
        assert_eq!(h.newer(), Some("b"));
        assert_eq!(h.newer(), None);
        let h2 = History::from_text(&h.to_text());
        assert_eq!(h2.items.len(), 3);
    }
}
