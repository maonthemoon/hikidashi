//! 結果の保存(`snap `). 計算, 拡張, スクリプトの結果を, そのときの入力と見えていた行ごと残す.
//! `%APPDATA%\Hikidashi\snapshots.json`に置く. 読み書きは画面の側(アプリ)が行い,
//! ここはデータの型, JSON, 足す・消す・置き換え, 一覧の行だけ.

use crate::clip::ago;
use crate::router::{Action, Icon, Row, MAX_ROWS};
use serde::{Deserialize, Serialize};

/// 残す件数の上限(古いものから消す)
pub const MAX_SNAPSHOTS: usize = 100;
/// 1行目の文字数の上限(越えたら`…`)
const NAME_CHARS: usize = 100;

/// 保存した1行
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapRow {
    pub name: String,
    pub sub: String,
    /// `Enter`でコピーする文字
    pub copy: String,
}

/// 保存した1件
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 保存したときの入力(接頭辞を含む. 例: `= vswr 1.5`)
    pub input: String,
    /// 種類(`calc`, `ext`, `script`)
    pub kind: String,
    /// 保存した時刻(UNIX秒)
    pub at: u64,
    pub rows: Vec<SnapRow>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshots {
    /// 新しい順
    items: Vec<Snapshot>,
}

impl Snapshots {
    /// JSONから. 壊れていれば空と, その理由
    pub fn from_json(s: &str) -> (Snapshots, Option<String>) {
        if s.trim().is_empty() {
            return (Snapshots::default(), None);
        }
        match serde_json::from_str::<Snapshots>(s.trim_start_matches('\u{feff}')) {
            Ok(mut v) => {
                v.items.truncate(MAX_SNAPSHOTS);
                (v, None)
            }
            Err(e) => (Snapshots::default(), Some(e.to_string())),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, input: &str) -> Option<&Snapshot> {
        self.items.iter().find(|s| s.input == input)
    }

    /// 足す. 行がなければ何もしない(false). 同じ入力があれば消して, 新しい方を先頭にする. 上限を超えた古いものは消す
    pub fn add(&mut self, input: &str, kind: &str, rows: Vec<SnapRow>, now: u64) -> bool {
        if rows.is_empty() || input.trim().is_empty() {
            return false;
        }
        self.items.retain(|s| s.input != input);
        self.items.insert(0, Snapshot { input: input.to_string(), kind: kind.to_string(), at: now, rows });
        self.items.truncate(MAX_SNAPSHOTS);
        true
    }

    pub fn remove(&mut self, input: &str) {
        self.items.retain(|s| s.input != input);
    }

    /// 打った語(空白区切り)を全部含むもの(大文字小文字は区別しない). 入力の文字で絞る. 新しい順
    pub fn filter(&self, query: &str) -> Vec<&Snapshot> {
        let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        self.items
            .iter()
            .filter(|s| {
                let hay = s.input.to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .collect()
    }

    /// 全部の行を`名前<TAB>2行目`の形で改行でつないだ文字
    pub fn copy_all(rows: &[SnapRow]) -> String {
        rows.iter().map(|r| format!("{}\t{}", r.name, r.sub)).collect::<Vec<_>>().join("\n")
    }
}

fn clip_name(s: &str) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut chars = line.chars();
    let head: String = chars.by_ref().take(NAME_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// `snap `のあとの語で絞った一覧の行. 保存がなければ案内の1行. 新しい順
pub fn rows(s: &Snapshots, query: &str, now: u64, tz: i64) -> Vec<Row> {
    if s.is_empty() {
        return vec![Row {
            name: "No saved results".into(),
            sub: "Press Ctrl+S on a result to save it".into(),
            tag: "",
            positions: vec![],
            action: Action::Copy(String::new()),
            learn_key: None,
            warn: false,
            icon: Icon::Calc,
            score: 0,
        }];
    }
    s.filter(query)
        .into_iter()
        .take(MAX_ROWS)
        .map(|e| {
            let n = e.rows.len();
            let unit = if n == 1 { "row" } else { "rows" };
            Row {
                name: clip_name(&e.input),
                sub: format!("{} · {} · {n} {unit}", e.kind, when(now, e.at, tz)),
                tag: "open",
                positions: vec![],
                action: Action::SnapOpen(e.input.clone()),
                learn_key: None,
                warn: false,
                icon: Icon::Calc,
                score: 0,
            }
        })
        .collect()
}

/// `Today 14:02`, `Yesterday 09:30`, `Nov 21 09:30`, `Oct 20, 2025 09:30`
fn when(now: u64, at: u64, tz: i64) -> String {
    let local = at as i64 + tz;
    let secs = local.rem_euclid(86_400);
    let hm = format!("{:02}:{:02}", secs / 3600, secs % 3600 / 60);
    let day = |t: u64| (t as i64 + tz).div_euclid(86_400);
    let head = if day(now) == day(at) {
        "Today".to_string()
    } else if day(at) + 1 == day(now) {
        "Yesterday".to_string()
    } else {
        // 2日以上前は, クリップボードの履歴の日付の書き方(`Nov 21`)を使う
        ago(now, at, tz)
    };
    format!("{head} {hm}")
}

/// 見返しているときの行(`Enter`でコピーする文字は保存したもの)
pub fn view_rows(e: &Snapshot) -> Vec<Row> {
    e.rows
        .iter()
        .map(|r| Row {
            name: r.name.clone(),
            sub: r.sub.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: &str) -> Vec<SnapRow> {
        vec![SnapRow { name: n.into(), sub: "s".into(), copy: format!("c{n}") }]
    }

    #[test]
    fn add_replaces_same_input_and_caps() {
        let mut s = Snapshots::default();
        assert!(s.add("= a", "calc", r("1"), 1));
        assert!(s.add("= b", "calc", r("2"), 2));
        assert!(s.add("= a", "calc", r("3"), 3));
        assert_eq!(s.len(), 2);
        let f = s.filter("");
        assert_eq!(f[0].input, "= a");
        assert_eq!(f[0].rows[0].name, "3");
        assert_eq!(f[1].input, "= b");
        for i in 0..150 {
            s.add(&format!("= x{i}"), "calc", r("x"), 10);
        }
        assert_eq!(s.len(), MAX_SNAPSHOTS);
        assert_eq!(s.filter("")[0].input, "= x149");
        assert!(s.get("= a").is_none());
    }

    #[test]
    fn add_refuses_empty() {
        let mut s = Snapshots::default();
        assert!(!s.add("= a", "calc", vec![], 1));
        assert!(!s.add("  ", "calc", r("1"), 1));
        assert!(s.is_empty());
    }

    #[test]
    fn json_roundtrip_and_broken() {
        let mut s = Snapshots::default();
        s.add("= 日本語", "ext", r("あ"), 5);
        let (back, err) = Snapshots::from_json(&s.to_json());
        assert_eq!(back, s);
        assert!(err.is_none());
        let (e, err) = Snapshots::from_json("壊れた");
        assert!(e.is_empty());
        assert!(err.is_some());
        let (e, err) = Snapshots::from_json("");
        assert!(e.is_empty() && err.is_none());
    }

    #[test]
    fn remove_and_filter() {
        let mut s = Snapshots::default();
        s.add("= vswr 1.5", "calc", r("1"), 1);
        s.add("mipi rffe", "ext", r("2"), 2);
        assert_eq!(s.filter("VSWR").len(), 1);
        assert_eq!(s.filter("= rffe").len(), 0);
        assert_eq!(s.filter("zzz").len(), 0);
        s.remove("= vswr 1.5");
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn list_rows_text() {
        let day = 86_400u64;
        let now = 20_781 * day + 14 * 3600 + 5 * 60;
        let mut s = Snapshots::default();
        let three = (0..3).map(|i| SnapRow { name: format!("n{i}"), sub: String::new(), copy: String::new() }).collect();
        s.add("= vswr 1.5", "calc", three, now - 3 * 60);
        s.add("mipi x", "ext", r("1"), now - day);
        s.add("run y", "script", r("1"), now - 3 * day);
        let list = rows(&s, "", now, 0);
        assert_eq!(list.len(), 3);
        assert_eq!(list[2].name, "= vswr 1.5");
        assert_eq!(list[2].sub, "calc · Today 14:02 · 3 rows");
        assert_eq!(list[1].sub, "ext · Yesterday 14:05 · 1 row");
        assert_eq!(list[0].sub, "script · Nov 21 14:05 · 1 row");
        assert!(matches!(&list[2].action, Action::SnapOpen(i) if i == "= vswr 1.5"));
        assert_eq!(rows(&s, "mipi", now, 0).len(), 1);
    }

    #[test]
    fn empty_list_guide() {
        let r = rows(&Snapshots::default(), "", 0, 0);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "No saved results");
        assert_eq!(r[0].sub, "Press Ctrl+S on a result to save it");
    }

    #[test]
    fn view_and_copy_all() {
        let mut s = Snapshots::default();
        s.add("= a", "calc", vec![SnapRow { name: "n".into(), sub: "s".into(), copy: "cc".into() }, SnapRow { name: "m".into(), sub: "t".into(), copy: "dd".into() }], 1);
        let e = s.get("= a").unwrap();
        let v = view_rows(e);
        assert_eq!(v[1].action, Action::Copy("dd".into()));
        assert_eq!(Snapshots::copy_all(&e.rows), "n\ts\nm\tt");
    }
}
