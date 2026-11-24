//! ローマ字で日本語の名前を探す(Migemo. B3). 辞書はSKK辞書(UTF-8に変えたもの. K5).
//! `kaigi` → [かいぎ, カイギ, 会議, 回議, …] のように広げ, 名前にそのどれかが含まれれば一致とする.

use crate::romaji;
use std::collections::HashMap;

/// 1つの入力から広げる語の上限(多すぎると遅く, 一致もでたらめになる)
const MAX_WORDS: usize = 400;

/// SKK辞書. 読みの順に並べた行の位置だけを持ち, 本文は1つの文字列に置く(メモリを節約する).
pub struct Dict {
    text: String,
    /// (読みの開始, 読みの終わり, 候補の終わり). 読みの順に並ぶ.
    lines: Vec<(u32, u32, u32)>,
}

impl Dict {
    /// SKK辞書の本文(UTF-8)を読む. `;;`の行は注釈. 送りありの読み(`かえr`)は末尾の英字を取る.
    pub fn parse(src: &str) -> Dict {
        let mut entries: Vec<(&str, &str)> = Vec::new();
        for line in src.lines() {
            if line.starts_with(";;") || line.is_empty() {
                continue;
            }
            let Some((reading, cands)) = line.split_once(" /") else { continue };
            let reading = reading.trim_end_matches(|c: char| c.is_ascii_alphabetic());
            if reading.is_empty() || !reading.chars().all(|c| ('ぁ'..='ゖ').contains(&c) || c == 'ー') {
                continue;
            }
            entries.push((reading, cands));
        }
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut text = String::with_capacity(src.len() / 2);
        let mut lines = Vec::with_capacity(entries.len());
        for (reading, cands) in entries {
            let start = text.len() as u32;
            text.push_str(reading);
            let rend = text.len() as u32;
            for c in cands.split('/') {
                let word = c.split(';').next().unwrap_or("");
                // (concat ...)などの式や, 英字だけの候補は名前探しに使わない
                if word.is_empty() || word.starts_with('(') || word.is_ascii() {
                    continue;
                }
                text.push('/');
                text.push_str(word);
            }
            lines.push((start, rend, text.len() as u32));
        }
        text.shrink_to_fit();
        Dict { text, lines }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn reading(&self, i: usize) -> &str {
        let (s, e, _) = self.lines[i];
        &self.text[s as usize..e as usize]
    }

    fn words(&self, i: usize) -> impl Iterator<Item = &str> {
        let (_, e, c) = self.lines[i];
        self.text[e as usize..c as usize].split('/').filter(|w| !w.is_empty())
    }

    /// 読みが`prefix`で始まる行の範囲
    fn prefix_range(&self, prefix: &str) -> std::ops::Range<usize> {
        let start = self.lines.partition_point(|&(s, e, _)| &self.text[s as usize..e as usize] < prefix);
        let mut end = start;
        while end < self.lines.len() && self.reading(end).starts_with(prefix) {
            end += 1;
        }
        start..end
    }
}

/// 1つの入力から広げた語の集まり. 名前のどこに含まれるかを速く調べるため, 先頭の文字で引けるようにしてある.
pub struct Expansion {
    words: Vec<String>,
    by_first: HashMap<char, Vec<usize>>,
}

impl Expansion {
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// 名前に広げた語のどれかが含まれれば, 一番長く一致した語の位置(charの番号)を返す.
    pub fn find(&self, name: &str) -> Option<Vec<u32>> {
        let mut best: Option<(usize, usize)> = None; // (charの位置, charの長さ)
        for (ci, (bi, ch)) in name.char_indices().enumerate() {
            let Some(list) = self.by_first.get(&ch) else { continue };
            for &w in list {
                let word = &self.words[w];
                if name[bi..].starts_with(word.as_str()) {
                    let len = word.chars().count();
                    if best.map_or(true, |(_, l)| len > l) {
                        best = Some((ci, len));
                    }
                }
            }
        }
        best.map(|(ci, len)| (ci as u32..(ci + len) as u32).collect())
    }
}

/// ローマ字の入力を, かな・カタカナ・辞書の漢字に広げる. ローマ字として読めなければ空.
/// 辞書がなくても, かなとカタカナには広げる.
pub fn expand(query: &str, dict: Option<&Dict>) -> Expansion {
    let q: String = query.chars().filter(|c| !c.is_whitespace()).collect();
    let mut words: Vec<String> = Vec::new();
    // 2文字以下は広げすぎるのでやめる. 英字以外を含むときも広げない
    if q.len() >= 3 && q.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '\'') {
        let prefixes = romaji::hiragana_prefixes(&q);
        for p in &prefixes {
            push(&mut words, p.clone());
            push(&mut words, romaji::to_katakana(p));
        }
        if let Some(d) = dict {
            // 読みがちょうど同じものを先に, 前方一致はその後(短い読みから)
            let mut exact = Vec::new();
            let mut longer = Vec::new();
            for p in &prefixes {
                for i in d.prefix_range(p) {
                    if d.reading(i) == p {
                        exact.push(i);
                    } else {
                        longer.push(i);
                    }
                }
            }
            longer.sort_by_key(|&i| d.reading(i).chars().count());
            'fill: for i in exact.into_iter().chain(longer) {
                for w in d.words(i) {
                    if words.len() >= MAX_WORDS {
                        break 'fill;
                    }
                    push(&mut words, w.to_string());
                }
            }
        }
    }
    let mut by_first: HashMap<char, Vec<usize>> = HashMap::new();
    for (i, w) in words.iter().enumerate() {
        if let Some(c) = w.chars().next() {
            by_first.entry(c).or_default().push(i);
        }
    }
    Expansion { words, by_first }
}

fn push(words: &mut Vec<String>, w: String) {
    if !w.is_empty() && !words.contains(&w) {
        words.push(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = ";; okuri-ari entries.\nかえr /帰/返/\n;; okuri-nasi entries.\nかいぎ /会議/回議;注釈/\nかいぎしつ /会議室/\nかいしゃ /会社/\nしりょう /資料/試料/\nてい /低/\nていれい /定例/\nいんたーねっと /(concat \"x\")/インターネット/\n";

    #[test]
    fn parse_dict() {
        let d = Dict::parse(SAMPLE);
        assert_eq!(d.len(), 8);
        let r = d.prefix_range("かいぎ");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn expand_and_find() {
        let d = Dict::parse(SAMPLE);
        let e = expand("kaigi", Some(&d));
        assert!(e.words().contains(&"会議".to_string()));
        assert!(e.words().contains(&"会議室".to_string()));
        assert!(e.words().contains(&"カイギ".to_string()));
        assert_eq!(e.find("定例会議.xlsx"), Some(vec![2, 3]));
        assert_eq!(e.find("会議室の予約"), Some(vec![0, 1, 2]));
        assert_eq!(e.find("report.docx"), None);
    }

    #[test]
    fn partial_input() {
        let d = Dict::parse(SAMPLE);
        let e = expand("kaig", Some(&d));
        assert!(e.find("定例会議.xlsx").is_some());
        let e = expand("shiryo", Some(&d));
        assert!(e.find("会議資料.xlsx").is_some());
    }

    #[test]
    fn katakana_without_dict() {
        let e = expand("pawa-", None);
        assert_eq!(e.find("パワーポイント"), Some(vec![0, 1, 2]));
        assert!(expand("pw", None).is_empty());
        assert!(expand("pwpt", None).is_empty());
    }
}
