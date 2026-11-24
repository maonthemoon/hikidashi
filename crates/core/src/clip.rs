//! クリップボードの履歴(`cb `). メモリにだけ置く. 読み書きと監視は画面の側(アプリ)が行い,
//! ここは履歴の持ち方, 絞り込み, 行の文字(時刻の表示など)だけ.

use crate::router::{Action, Icon, Row, MAX_ROWS};

/// 覚える件数の上限
pub const MAX_ENTRIES: usize = 200;
/// これを超える文字は残さない(バイト)
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
/// 1行目の文字数の上限(越えたら`…`)
const NAME_CHARS: usize = 100;

#[derive(Debug, Clone, PartialEq)]
pub enum ClipKind {
    Text(String),
    /// Explorerでコピーしたファイルとフォルダー(パスだけ)
    Files(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipEntry {
    pub id: u64,
    pub kind: ClipKind,
    /// 記録した時刻(UNIX秒)
    pub at: u64,
}

#[derive(Debug, Default, Clone)]
pub struct ClipHistory {
    /// 新しい順
    entries: Vec<ClipEntry>,
    next_id: u64,
}

impl ClipHistory {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn get(&self, id: u64) -> Option<&ClipEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
    }

    /// 記録する. 空の文字と大きすぎる文字は残さない. 同じ中身があれば先頭に移す. 残したらその番号
    pub fn add(&mut self, kind: ClipKind, now: u64) -> Option<u64> {
        match &kind {
            ClipKind::Text(t) if t.trim().is_empty() || t.len() > MAX_TEXT_BYTES => return None,
            ClipKind::Files(f) if f.is_empty() => return None,
            _ => {}
        }
        self.entries.retain(|e| e.kind != kind);
        self.next_id += 1;
        let id = self.next_id;
        self.entries.insert(0, ClipEntry { id, kind, at: now });
        self.entries.truncate(MAX_ENTRIES);
        Some(id)
    }

    /// 打った語(空白区切り)を全部含むもの(大文字小文字は区別しない). 文字は本文, ファイルは名前とパス. 新しい順
    pub fn filter(&self, query: &str) -> Vec<&ClipEntry> {
        let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        self.entries
            .iter()
            .filter(|e| {
                if words.is_empty() {
                    return true;
                }
                let hay = match &e.kind {
                    ClipKind::Text(t) => t.to_lowercase(),
                    ClipKind::Files(f) => f.join("\n").to_lowercase(),
                };
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .collect()
    }
}

/// UNIX日(1970-01-01からの日数)から(年, 月, 日)
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// 時刻の表示: `just now`, `N min ago`, `N h ago`(同じ日), `yesterday`, 日付. `tz`は現地時間とUTCの差(秒)
pub fn ago(now: u64, at: u64, tz: i64) -> String {
    let diff = now.saturating_sub(at);
    if diff < 60 {
        return "just now".into();
    }
    if diff < 3600 {
        return format!("{} min ago", diff / 60);
    }
    let day = |t: u64| (t as i64 + tz).div_euclid(86_400);
    let (dn, da) = (day(now), day(at));
    if da == dn {
        return format!("{} h ago", diff / 3600);
    }
    if da + 1 == dn {
        return "yesterday".into();
    }
    let (y, m, d) = civil(da);
    let (ny, _, _) = civil(dn);
    let mon = MONTHS[(m - 1) as usize];
    if y == ny {
        format!("{mon} {d}")
    } else {
        format!("{mon} {d}, {y}")
    }
}

/// 1行目に出す文字: 最初の空でない行. 長ければ省略
fn first_line(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut chars = line.chars();
    let head: String = chars.by_ref().take(NAME_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// 行(行の数が2以上なら`N lines`)
fn line_count(text: &str) -> usize {
    text.trim().lines().count()
}

/// パスの(名前, 親フォルダー)
fn split(path: &str) -> (&str, &str) {
    let p = path.trim_end_matches(['\\', '/']);
    match p.rfind(['\\', '/']) {
        Some(i) => (&p[i + 1..], &p[..i]),
        None => (p, ""),
    }
}

/// `cb `のあとの語で絞った行. `exists`でファイルが今もあるか確かめる. 新しい順
pub fn rows(h: &ClipHistory, query: &str, now: u64, tz: i64, exists: impl Fn(&str) -> bool) -> Vec<Row> {
    h.filter(query)
        .into_iter()
        .take(MAX_ROWS)
        .map(|e| {
            let when = ago(now, e.at, tz);
            let (name, sub, icon) = match &e.kind {
                ClipKind::Text(t) => {
                    let n = line_count(t);
                    let lines = if n > 1 { format!(" · {n} lines") } else { String::new() };
                    (first_line(t), format!("Text · {when}{lines}"), Icon::Clip)
                }
                ClipKind::Files(f) => {
                    let (first, parent) = split(&f[0]);
                    let name = if f.len() > 1 { format!("{first} +{}", f.len() - 1) } else { first.to_string() };
                    let here = f.iter().all(|p| exists(p));
                    let head = if here { "File" } else { "Unavailable" };
                    let icon = if here { Icon::Path(f[0].clone()) } else { Icon::Clip };
                    (name, format!("{head} · {when} · {parent}"), icon)
                }
            };
            Row { name, sub, tag: "paste", positions: vec![], action: Action::Clip(e.id), learn_key: None, warn: false, icon, score: 0 }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> ClipKind {
        ClipKind::Text(s.into())
    }

    #[test]
    fn add_dedupes_and_caps() {
        let mut h = ClipHistory::default();
        h.add(t("a"), 1);
        h.add(t("b"), 2);
        let id = h.add(t("a"), 3).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h.filter("")[0].id, id);
        assert_eq!(h.filter("")[0].at, 3);
        for i in 0..300 {
            h.add(t(&format!("x{i}")), 10);
        }
        assert_eq!(h.len(), MAX_ENTRIES);
        assert_eq!(h.filter("")[0].kind, t("x299"));
    }

    #[test]
    fn add_refuses_empty_and_huge() {
        let mut h = ClipHistory::default();
        assert!(h.add(t("  \n"), 1).is_none());
        assert!(h.add(t(&"a".repeat(MAX_TEXT_BYTES + 1)), 1).is_none());
        assert!(h.add(t(&"a".repeat(MAX_TEXT_BYTES)), 1).is_some());
        assert!(h.add(ClipKind::Files(vec![]), 1).is_none());
    }

    #[test]
    fn filter_all_words_case_insensitive() {
        let mut h = ClipHistory::default();
        h.add(t("Hello World"), 1);
        h.add(ClipKind::Files(vec![r"C:\docs\見積.xlsx".into()]), 2);
        h.add(t("hello there"), 3);
        assert_eq!(h.filter("hello").len(), 2);
        assert_eq!(h.filter("WORLD hello").len(), 1);
        assert_eq!(h.filter("見積 docs").len(), 1);
        assert_eq!(h.filter("zzz").len(), 0);
        assert_eq!(h.filter("  ").len(), 3);
        // 新しい順
        assert_eq!(h.filter("hello")[0].kind, t("hello there"));
    }

    #[test]
    fn remove_entry() {
        let mut h = ClipHistory::default();
        let a = h.add(t("a"), 1).unwrap();
        h.add(t("b"), 2);
        h.remove(a);
        assert_eq!(h.len(), 1);
        assert!(h.get(a).is_none());
    }

    #[test]
    fn time_labels() {
        let day = 86_400u64;
        // 2026-11-24 12:00 UTC
        let now = 20_781 * day + 12 * 3600;
        assert_eq!(ago(now, now - 10, 0), "just now");
        assert_eq!(ago(now, now - 3 * 60, 0), "3 min ago");
        assert_eq!(ago(now, now - 3600, 0), "1 h ago");
        assert_eq!(ago(now, now - 5 * 3600, 0), "5 h ago");
        assert_eq!(ago(now, now - 13 * 3600, 0), "yesterday");
        assert_eq!(ago(now, now - day, 0), "yesterday");
        assert_eq!(ago(now, now - 3 * day, 0), "Nov 21");
        assert_eq!(ago(now, now - 400 * day, 0), "Oct 20, 2025");
        // 現地時間(+9時間)では今は21時. 22時間前は前の日の23時
        assert_eq!(ago(now, now - 22 * 3600, 9 * 3600), "yesterday");
        assert_eq!(ago(now, now - 13 * 3600, 9 * 3600), "13 h ago");
        assert_eq!(ago(now, now - 5 * 3600, 9 * 3600), "5 h ago");
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_781), (2026, 11, 24));
    }

    #[test]
    fn row_text() {
        let mut h = ClipHistory::default();
        h.add(t("first line\nsecond"), 100);
        h.add(ClipKind::Files(vec![r"C:\work\見積.xlsx".into(), r"C:\work\b.txt".into(), r"C:\work\c.txt".into()]), 100);
        h.add(ClipKind::Files(vec![r"C:\gone\x.txt".into()]), 100);
        h.add(t(&format!("\n  {}", "あ".repeat(150))), 100);
        let r = rows(&h, "", 100 + 3 * 60, 0, |p| !p.contains("gone"));
        assert_eq!(r.len(), 4);
        // 新しい順. 先頭は長い1行の文字(前の空行は数えない)
        assert_eq!(r[0].name.chars().count(), NAME_CHARS + 1);
        assert!(r[0].name.ends_with('…'));
        assert_eq!(r[0].sub, "Text · 3 min ago");
        assert_eq!(r[0].tag, "paste");
        assert_eq!(r[1].name, "x.txt");
        assert_eq!(r[1].sub, r"Unavailable · 3 min ago · C:\gone");
        assert_eq!(r[2].name, "見積.xlsx +2");
        assert_eq!(r[2].sub, r"File · 3 min ago · C:\work");
        assert_eq!(r[3].name, "first line");
        assert_eq!(r[3].sub, "Text · 3 min ago · 2 lines");
        assert!(matches!(r[3].action, Action::Clip(_)));
    }
}
