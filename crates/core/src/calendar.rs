//! Outlookの予定(`cal `, K62). 読むのと開くのは画面の側(アプリ)がCOMで行い,
//! ここは予定の持ち方, 絞り込み, 並べ方, 行の文字(日時と長さの書き方)だけ.
//!
//! 時刻は「現地時間をUTCとみなしたUNIX秒」(以下, 現地の秒)で持つ. OutlookのStartとEndは現地時間で来るので,
//! そのまま秒にすれば夏時間の切り替わりをまたいでもずれない. 今の時刻はUTCのUNIX秒と, 現地時間とUTCの差(秒)で受け取る.

use crate::fuzzy::lower_str;
use crate::router::{Action, Icon, Row, MAX_ROWS};

/// 何も打っていないときに出す日数(今日から)
pub const UPCOMING_DAYS: i64 = 7;
/// 何も打っていないときに出す件数の上限
pub const UPCOMING_MAX: usize = 30;
/// 読む範囲: 今日の何日前から
pub const PAST_DAYS: i64 = 30;
/// 読む範囲: 今日の何日後まで
pub const FUTURE_DAYS: i64 = 90;
/// 読む件数の上限
pub const MAX_EVENTS: usize = 500;
/// 右端の文字
const TAG: &str = "Outlook";
const DAY: i64 = 86_400;

/// 予定の1件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub subject: String,
    pub location: String,
    /// 始まりと終わり(現地の秒)
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    /// Outlookで開くときの鍵. 繰り返しの予定では, どの回も同じ(シリーズの)ID
    pub entry_id: String,
}

/// 今の現地の秒
pub fn local_now(now: u64, tz: i64) -> i64 {
    now as i64 + tz
}

/// 現地の秒の日(1970-01-01からの日数)
fn day_of(t: i64) -> i64 {
    t.div_euclid(DAY)
}

/// UNIX日から(年, 月, 日)
pub fn civil(days: i64) -> (i64, u32, u32) {
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

/// 現地の秒から(年, 月, 日, 時, 分)
pub fn parts(t: i64) -> (i64, u32, u32, u32, u32) {
    let (y, m, d) = civil(day_of(t));
    let s = t.rem_euclid(DAY);
    (y, m, d, (s / 3600) as u32, ((s % 3600) / 60) as u32)
}

/// OLEの日時(1899-12-30からの日数. 現地時間)を現地の秒にする
pub fn from_ole_date(d: f64) -> i64 {
    ((d - 25_569.0) * DAY as f64).round() as i64
}

/// 読む範囲(現地の秒): 今日の`PAST_DAYS`日前の0時から, `FUTURE_DAYS`日後の終わり(その次の日の0時)まで
pub fn read_window(now: u64, tz: i64) -> (i64, i64) {
    let today = day_of(local_now(now, tz));
    ((today - PAST_DAYS) * DAY, (today + FUTURE_DAYS + 1) * DAY)
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// 曜日(英語の3文字). 1970-01-01は木曜日
fn weekday(day: i64) -> &'static str {
    WEEKDAYS[(day + 4).rem_euclid(7) as usize]
}

/// `11/28 (Fri)`
fn date_label(day: i64) -> String {
    let (_, m, d) = civil(day);
    format!("{m}/{d} ({})", weekday(day))
}

/// 日の書き方: `Today`, `Tomorrow`, それ以外は`11/28 (Fri)`
pub fn day_label(start: i64, now: u64, tz: i64) -> String {
    let (d, today) = (day_of(start), day_of(local_now(now, tz)));
    if d == today {
        "Today".into()
    } else if d == today + 1 {
        "Tomorrow".into()
    } else {
        date_label(d)
    }
}

/// `10:00`(時は0を付けない)
fn hm(t: i64) -> String {
    let (_, _, _, h, m) = parts(t);
    format!("{h}:{m:02}")
}

/// 長さ: 60分までは`30 min`, `60 min`. それより長ければ`1 h 30 min`, `2 h`. 0以下ならNone
pub fn duration_label(secs: i64) -> Option<String> {
    let min = secs / 60;
    if min <= 0 {
        return None;
    }
    if min <= 60 {
        return Some(format!("{min} min"));
    }
    let (h, m) = (min / 60, min % 60);
    Some(if m == 0 { format!("{h} h") } else { format!("{h} h {m} min") })
}

/// 1行目: `10:00 定例`, 終日なら`All day 定例`
pub fn first_line(e: &Event) -> String {
    let when = if e.all_day { "All day".to_string() } else { hm(e.start) };
    let subject = e.subject.trim();
    if subject.is_empty() {
        when
    } else {
        format!("{when} {subject}")
    }
}

/// 2行目: `Today · 会議室A · 60 min`. 場所がなければ省き, 終日なら長さを省く
pub fn second_line(e: &Event, now: u64, tz: i64) -> String {
    let mut v = vec![day_label(e.start, now, tz)];
    let loc = e.location.trim();
    if !loc.is_empty() {
        v.push(loc.to_string());
    }
    if !e.all_day {
        if let Some(d) = duration_label(e.end - e.start) {
            v.push(d);
        }
    }
    v.join(" · ")
}

/// コピーする1行: `11/28 (Fri) 10:00-11:00 定例 @ 会議室A`. 終日なら`11/28 (Fri) All day 定例`
pub fn copy_line(e: &Event) -> String {
    let when = if e.all_day { "All day".to_string() } else { format!("{}-{}", hm(e.start), hm(e.end)) };
    let mut s = format!("{} {when}", date_label(day_of(e.start)));
    let subject = e.subject.trim();
    if !subject.is_empty() {
        s.push(' ');
        s.push_str(subject);
    }
    let loc = e.location.trim();
    if !loc.is_empty() {
        s.push_str(" @ ");
        s.push_str(loc);
    }
    s
}

/// 始まる順(同じなら件名の順)に並べる
fn sort(v: &mut [&Event]) {
    v.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.subject.cmp(&b.subject)));
}

/// 絞り込み. 何も打っていなければ, 今日から7日間の予定のうち, まだ終わっていないものを最大30件.
/// 語を打ったら, 件名と場所に語が全部含まれるもの(大文字小文字と全角半角の英数字は区別しない)を, 読んだ範囲から全部(終わったものも含む).
/// どちらも始まる順
pub fn filter<'a>(events: &'a [Event], query: &str, now: u64, tz: i64) -> Vec<&'a Event> {
    let words: Vec<String> = query.split_whitespace().map(lower_str).collect();
    let mut v: Vec<&Event> = if words.is_empty() {
        let now_l = local_now(now, tz);
        let until = (day_of(now_l) + UPCOMING_DAYS) * DAY;
        events.iter().filter(|e| e.start < until && e.end.max(e.start) > now_l).collect()
    } else {
        events
            .iter()
            .filter(|e| {
                let hay = lower_str(&format!("{}\n{}", e.subject, e.location));
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .collect()
    };
    sort(&mut v);
    v.truncate(if words.is_empty() { UPCOMING_MAX } else { MAX_ROWS });
    v
}

fn row(name: String, sub: String, tag: &'static str, action: Action) -> Row {
    Row { name, sub, tag, positions: vec![], action, learn_key: None, warn: false, icon: Icon::Calendar, score: 0 }
}

/// 何もしない行(`Enter`を押しても何も起きない)
fn note_row(name: &str, sub: &str, warn: bool) -> Row {
    let mut r = row(name.into(), sub.into(), "", Action::Copy(String::new()));
    r.warn = warn;
    r
}

/// `cal `のあとの語で絞った行. 何も打っていないのに1件もなければ, そう書いた1行
pub fn rows(events: &[Event], query: &str, now: u64, tz: i64) -> Vec<Row> {
    let v = filter(events, query, now, tz);
    if v.is_empty() && query.trim().is_empty() {
        return vec![note_row("No appointments in the next 7 days", "", false)];
    }
    v.into_iter()
        .map(|e| row(first_line(e), second_line(e, now, tz), TAG, Action::Appointment { entry_id: e.entry_id.clone(), copy: copy_line(e) }))
        .collect()
}

/// 初めて読んでいる間の1行
pub fn reading_row() -> Row {
    note_row("Reading Outlook…", "", false)
}

/// 読めなかったときの1行. `reason`は理由の短い文
pub fn error_row(reason: &str) -> Row {
    note_row("Outlook is not available", reason, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3600;
    /// 2026-11-27(金)の0時(現地の秒)
    const D: i64 = 20_784 * DAY;
    const JST: i64 = 9 * H;

    /// 現地の2026-11-27 10:30のときの, UTCのUNIX秒
    fn now() -> u64 {
        (D + 10 * H + 30 * 60 - JST) as u64
    }

    fn ev(subject: &str, location: &str, start: i64, mins: i64) -> Event {
        Event { subject: subject.into(), location: location.into(), start, end: start + mins * 60, all_day: false, entry_id: format!("id-{subject}") }
    }

    fn all_day(subject: &str, day: i64) -> Event {
        Event { subject: subject.into(), location: String::new(), start: day, end: day + DAY, all_day: true, entry_id: format!("id-{subject}") }
    }

    #[test]
    fn dates() {
        assert_eq!(civil(20_784), (2026, 11, 27));
        assert_eq!(weekday(20_784), "Fri");
        assert_eq!(parts(D + 9 * H + 5 * 60), (2026, 11, 27, 9, 5));
        // 2026-11-27 0:00 = OLEの46353
        assert_eq!(from_ole_date(46_353.0), D);
        assert_eq!(from_ole_date(46_353.0 + 10.5 / 24.0), D + 10 * H + 30 * 60);
        assert_eq!(read_window(now(), JST), (D - 30 * DAY, D + 91 * DAY));
    }

    #[test]
    fn day_labels() {
        assert_eq!(day_label(D + 23 * H, now(), JST), "Today");
        assert_eq!(day_label(D + DAY, now(), JST), "Tomorrow");
        assert_eq!(day_label(D + 2 * DAY + H, now(), JST), "11/29 (Sun)");
        assert_eq!(day_label(D - DAY, now(), JST), "11/26 (Thu)");
        // UTCでは同じ時刻でも, 現地の日で決める(UTCの11/27 20:00は現地の11/28 5:00)
        let utc_evening = (D + 20 * H) as u64;
        assert_eq!(day_label(D + DAY + 6 * H, utc_evening, JST), "Today");
        assert_eq!(day_label(D + DAY + 6 * H, utc_evening, 0), "Tomorrow");
    }

    #[test]
    fn durations() {
        assert_eq!(duration_label(0), None);
        assert_eq!(duration_label(30 * 60).as_deref(), Some("30 min"));
        assert_eq!(duration_label(60 * 60).as_deref(), Some("60 min"));
        assert_eq!(duration_label(90 * 60).as_deref(), Some("1 h 30 min"));
        assert_eq!(duration_label(120 * 60).as_deref(), Some("2 h"));
    }

    #[test]
    fn lines() {
        let e = ev("定例", "会議室A", D + 10 * H, 60);
        assert_eq!(first_line(&e), "10:00 定例");
        assert_eq!(second_line(&e, now(), JST), "Today · 会議室A · 60 min");
        assert_eq!(copy_line(&e), "11/27 (Fri) 10:00-11:00 定例 @ 会議室A");
        let e = ev("朝会", "", D + DAY + 9 * H, 15);
        assert_eq!(first_line(&e), "9:00 朝会");
        assert_eq!(second_line(&e, now(), JST), "Tomorrow · 15 min");
        assert_eq!(copy_line(&e), "11/28 (Sat) 9:00-9:15 朝会");
        let e = all_day("休み", D + 3 * DAY);
        assert_eq!(first_line(&e), "All day 休み");
        assert_eq!(second_line(&e, now(), JST), "11/30 (Mon)");
        assert_eq!(copy_line(&e), "11/30 (Mon) All day 休み");
    }

    #[test]
    fn upcoming_skips_ended_and_far() {
        let events = vec![
            ev("later", "", D + DAY + 9 * H, 30),
            ev("ended", "", D + 9 * H, 60),
            ev("now", "", D + 10 * H, 60),
            all_day("today", D),
            ev("week", "", D + 6 * DAY + 23 * H, 30),
            ev("next week", "", D + 7 * DAY, 30),
            ev("last week", "", D - 7 * DAY, 30),
        ];
        let names: Vec<&str> = filter(&events, "", now(), JST).iter().map(|e| e.subject.as_str()).collect();
        assert_eq!(names, ["today", "now", "later", "week"]);
        let r = rows(&events, "  ", now(), JST);
        assert_eq!(r.len(), 4);
        assert_eq!(r[1].name, "10:00 now");
        assert_eq!(r[1].tag, "Outlook");
        assert_eq!(r[1].icon, Icon::Calendar);
        assert_eq!(r[1].action, Action::Appointment { entry_id: "id-now".into(), copy: "11/27 (Fri) 10:00-11:00 now".into() });
    }

    #[test]
    fn upcoming_caps() {
        let events: Vec<Event> = (0..40).map(|i| ev(&format!("e{i:02}"), "", D + DAY + i * 600, 10)).collect();
        let v = filter(&events, "", now(), JST);
        assert_eq!(v.len(), UPCOMING_MAX);
        assert_eq!(v[0].subject, "e00");
    }

    #[test]
    fn search_subject_and_location() {
        let events = vec![
            ev("定例 Design", "会議室A", D + 2 * DAY, 60),
            ev("past design review", "Room B", D - 10 * DAY, 30),
            ev("lunch", "会議室A", D + 12 * H, 60),
        ];
        let names = |q: &str| -> Vec<String> { filter(&events, q, now(), JST).iter().map(|e| e.subject.clone()).collect() };
        // 終わったものも出し, 始まる順
        assert_eq!(names("design"), ["past design review", "定例 Design"]);
        // 全角の英字も, 大文字小文字も区別しない
        assert_eq!(names("ＤＥＳＩＧＮ　定例"), ["定例 Design"]);
        assert_eq!(names("会議室a"), ["lunch", "定例 Design"]);
        assert_eq!(names("room design"), ["past design review"]);
        assert!(names("zzz").is_empty());
        // 合うものがなければ空(何もしない行は出さない)
        assert!(rows(&events, "zzz", now(), JST).is_empty());
    }

    #[test]
    fn empty_and_status_rows() {
        let r = rows(&[], "", now(), JST);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].action, Action::Copy(String::new()));
        assert_eq!(reading_row().name, "Reading Outlook…");
        let e = error_row("Outlook is not installed");
        assert_eq!((e.name.as_str(), e.sub.as_str(), e.warn), ("Outlook is not available", "Outlook is not installed", true));
    }
}
