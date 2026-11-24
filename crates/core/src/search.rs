//! 検索語の準備と, 1つの名前との一致. あいまい一致(B2)とMigemo(B3)をまとめる.

use crate::fuzzy::{self, Match};
use crate::migemo::{self, Dict, Expansion};

/// 1回の入力について一度だけ作る, 検索語の準備.
pub struct Prepared {
    pub text: String,
    pub chars: Vec<char>,
    /// 検索語がASCIIだけなら, そのバイト列(速いふるいに使う)
    pub ascii: Option<Vec<u8>>,
    /// ローマ字として読めるときの広げた語(日本語の名前に使う)
    pub migemo: Option<Expansion>,
}

impl Prepared {
    pub fn new(text: &str, dict: Option<&Dict>) -> Prepared {
        let chars = fuzzy::prepare_query(text);
        let ascii = if chars.iter().all(|c| c.is_ascii()) { Some(chars.iter().map(|c| *c as u8).collect()) } else { None };
        let e = migemo::expand(text, dict);
        Prepared { text: text.to_string(), chars, ascii, migemo: if e.is_empty() { None } else { Some(e) } }
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }
}

/// 名前`name`(小文字にしたもの`lower`, 名前がASCIIだけか`ascii`)と検索語の一致.
pub fn match_name(p: &Prepared, name: &str, lower: &str, ascii: bool) -> Option<Match> {
    if p.chars.is_empty() {
        return None;
    }
    if let (Some(q), true) = (&p.ascii, ascii) {
        if !fuzzy::is_subsequence_ascii(q, lower.as_bytes()) {
            return None;
        }
        return fuzzy::score(&p.chars, name, lower);
    }
    let direct = if p.ascii.is_some() || !ascii { fuzzy::score(&p.chars, name, lower) } else { None };
    if !ascii {
        if let Some(e) = &p.migemo {
            if let Some(pos) = e.find(name) {
                let start = pos[0] as usize;
                let at_start = start == 0;
                let after_sep = start > 0 && name.chars().nth(start - 1).is_some_and(|c| !c.is_alphanumeric() || c.is_ascii());
                let score = 150 + if at_start { 64 } else if after_sep { 32 } else { 0 } + 8 * pos.len() as i32;
                let m = Match { score, positions: pos };
                return Some(match direct {
                    Some(d) if d.score >= m.score => d,
                    _ => m,
                });
            }
        }
    }
    direct
}

/// 上位k件だけを残す入れ物. 点数の高い順に取り出せる.
pub struct TopK<T> {
    k: usize,
    items: Vec<(i32, T)>,
    min: i32,
}

impl<T> TopK<T> {
    pub fn new(k: usize) -> Self {
        TopK { k, items: Vec::with_capacity(k + 1), min: i32::MIN }
    }

    /// これより低い点数は入らない(ふるいの前に比べて, 無駄な計算を省く)
    pub fn threshold(&self) -> i32 {
        if self.items.len() < self.k { i32::MIN } else { self.min }
    }

    pub fn push(&mut self, score: i32, item: T) {
        if self.items.len() >= self.k && score <= self.min {
            return;
        }
        self.items.push((score, item));
        if self.items.len() > self.k {
            // 一番低いものを捨てる
            let (i, _) = self.items.iter().enumerate().min_by_key(|(_, (s, _))| *s).unwrap();
            self.items.swap_remove(i);
        }
        if self.items.len() >= self.k {
            self.min = self.items.iter().map(|(s, _)| *s).min().unwrap_or(i32::MIN);
        }
    }

    pub fn merge(&mut self, other: TopK<T>) {
        for (s, t) in other.items {
            self.push(s, t);
        }
    }

    pub fn into_sorted(mut self) -> Vec<(i32, T)> {
        self.items.sort_by(|a, b| b.0.cmp(&a.0));
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzzy::lower_str;

    fn m(q: &str, name: &str) -> Option<Match> {
        let p = Prepared::new(q, None);
        match_name(&p, name, &lower_str(name), name.is_ascii())
    }

    #[test]
    fn ascii_and_japanese() {
        assert!(m("pwpt", "PowerPoint").is_some());
        assert!(m("pwpt", "メモ").is_none());
        assert!(m("pawa-", "パワーポイント").is_some());
        assert!(m("会議", "定例会議.xlsx").is_some());
        // 英字の名前にはMigemoを使わない
        assert!(m("pawa-", "power").is_none());
    }

    #[test]
    fn topk_keeps_best() {
        let mut t = TopK::new(3);
        for (s, n) in [(5, "a"), (9, "b"), (1, "c"), (7, "d"), (3, "e")] {
            t.push(s, n);
        }
        let v: Vec<&str> = t.into_sorted().into_iter().map(|(_, n)| n).collect();
        assert_eq!(v, vec!["b", "d", "a"]);
    }
}
