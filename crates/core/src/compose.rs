//! 長い質問を書く窓(K22)の書きかけと, 送った文の履歴. `%APPDATA%\Hikidashi\compose.json`に置く.

use serde::{Deserialize, Serialize};

/// 送った文を覚える数
pub const SENT_MAX: usize = 20;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Drafts {
    /// 書きかけ(閉じても残す. Q5のA)
    pub draft: String,
    /// 送った文(新しい順)
    pub sent: Vec<String>,
}

impl Drafts {
    pub fn from_json(s: &str) -> Drafts {
        serde_json::from_str(s).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// 送った文を履歴の先頭に足す(同じ文は1つにする)
    pub fn push_sent(&mut self, s: &str) {
        let s = s.trim();
        if s.is_empty() {
            return;
        }
        self.sent.retain(|x| x != s);
        self.sent.insert(0, s.to_string());
        self.sent.truncate(SENT_MAX);
    }

    /// 新しく書き始める前に, 今の文を履歴に移す. 書く窓が閉じていれば保存済みの文を使う
    pub fn archive_draft(&mut self, current: Option<&str>) -> bool {
        let saved = std::mem::take(&mut self.draft);
        let draft = current.unwrap_or(&saved);
        self.push_sent(draft);
        !saved.is_empty() || !draft.trim().is_empty()
    }
}

/// 窓を出し直すとき, 書きかけの文と添付をまとめて選ぶ
#[derive(Debug, Default)]
pub enum DraftSelection {
    #[default]
    None,
    Reopen,
    All { draft: String, text_len: usize },
}

pub enum ReplaceAll {
    TextChanged,
    ImagePasted,
    Backspace,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AfterReplace {
    KeepEditing,
    ClearText,
    Search,
}

impl DraftSelection {
    pub fn on_hide(&mut self, draft: &str, image_count: usize) {
        *self = if draft.trim().is_empty() && image_count == 0 { Self::None } else { Self::Reopen };
    }

    pub fn take_reopen(&mut self) -> bool {
        if matches!(self, Self::Reopen) {
            self.clear();
            return true;
        }
        false
    }

    pub fn select_all(&mut self, draft: String, text_len: usize) {
        *self = Self::All { draft, text_len };
    }

    pub fn is_all(&self) -> bool {
        matches!(self, Self::All { .. })
    }

    pub fn clear(&mut self) {
        *self = Self::None;
    }

    /// 文と添付の全選択を解除し, 添付を残したまま続けて書く
    pub fn keep_editing(&mut self) -> bool {
        if !self.is_all() {
            return false;
        }
        self.clear();
        true
    }

    /// 修飾キーなしの上下キーで履歴をたどるか
    pub fn uses_plain_history(&self, older: bool, empty: bool, recalled: bool) -> bool {
        !self.is_all() && (recalled || (older && empty))
    }

    pub fn on_selection_changed(&mut self, range: (usize, usize), leave_empty: bool) -> bool {
        let Self::All { text_len, .. } = self else { return false };
        if range == (0, *text_len) && !(*text_len == 0 && leave_empty) {
            return false;
        }
        self.keep_editing()
    }

    /// 全選択を置き換えたら, 元の文を履歴に移す. 戻り値があれば添付も消す
    pub fn on_replace(&mut self, action: ReplaceAll, drafts: &mut Drafts) -> Option<AfterReplace> {
        let Self::All { draft, .. } = self else { return None };
        drafts.archive_draft(Some(draft));
        self.clear();
        Some(match action {
            ReplaceAll::TextChanged => AfterReplace::KeepEditing,
            ReplaceAll::ImagePasted => AfterReplace::ClearText,
            ReplaceAll::Backspace => AfterReplace::Search,
        })
    }
}

/// `Ctrl+↑`/`Ctrl+↓`で送った文を呼び出す. 呼び出す前の書きかけは覚えておき, 戻ったら出す
#[derive(Debug, Clone, Default)]
pub struct Browse {
    pos: Option<usize>,
    saved: String,
}

impl Browse {
    pub fn is_browsing(&self) -> bool {
        self.pos.is_some()
    }

    pub fn reset(&mut self) {
        self.pos = None;
        self.saved.clear();
    }

    /// 1つ古いもの. なければNone
    pub fn older(&mut self, sent: &[String], current: &str) -> Option<String> {
        let next = match self.pos {
            None => 0,
            Some(p) => p + 1,
        };
        let s = sent.get(next)?.clone();
        if self.pos.is_none() {
            self.saved = current.to_string();
        }
        self.pos = Some(next);
        Some(s)
    }

    /// 1つ新しいもの. 一番新しいものの先は書きかけに戻る
    pub fn newer(&mut self, sent: &[String]) -> Option<String> {
        match self.pos {
            None => None,
            Some(0) => {
                self.pos = None;
                Some(std::mem::take(&mut self.saved))
            }
            Some(p) => {
                self.pos = Some(p - 1);
                sent.get(p - 1).cloned()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopens_with_text_or_images_selected() {
        for (draft, images) in [("前の文", 0), ("", 1), ("前の文", 1)] {
            let mut selection = DraftSelection::default();
            selection.on_hide(draft, images);
            assert!(selection.take_reopen());
            selection.select_all(draft.into(), draft.encode_utf16().count());
            assert!(selection.is_all());
            assert!(!selection.take_reopen());
            assert!(selection.is_all());
            selection.on_hide(draft, images);
            assert!(selection.take_reopen());
        }
    }

    #[test]
    fn empty_or_sent_draft_opens_search() {
        let mut selection = DraftSelection::default();
        selection.on_hide("前の文", 1);
        selection.on_hide(" \n", 0);
        assert!(!selection.take_reopen());
        selection.on_hide("前の文", 1);
        selection.clear();
        assert!(!selection.take_reopen());
    }

    #[test]
    fn replacing_selection_archives_draft_and_removes_attachments() {
        for (action, after) in [
            (ReplaceAll::TextChanged, AfterReplace::KeepEditing),
            (ReplaceAll::ImagePasted, AfterReplace::ClearText),
            (ReplaceAll::Backspace, AfterReplace::Search),
        ] {
            let mut selection = DraftSelection::default();
            let mut drafts = Drafts { draft: "前の文\n質問".into(), ..Default::default() };
            selection.select_all(drafts.draft.clone(), 6);
            assert_eq!(selection.on_replace(action, &mut drafts), Some(after));
            assert!(!selection.is_all());
            assert!(drafts.draft.is_empty());
            assert_eq!(Browse::default().older(&drafts.sent, "").as_deref(), Some("前の文\n質問"));
        }
    }

    #[test]
    fn leaving_selection_preserves_draft_and_attachments() {
        for (draft, range, leave_empty) in [("前の文", (3, 3), false), ("", (0, 0), true)] {
            let mut selection = DraftSelection::default();
            let mut drafts = Drafts { draft: draft.into(), ..Default::default() };
            let text_len = draft.encode_utf16().count();
            selection.select_all(drafts.draft.clone(), text_len);
            assert!(!selection.on_selection_changed((0, text_len), false));
            assert!(selection.on_selection_changed(range, leave_empty));
            assert!(!selection.is_all());
            assert_eq!(selection.on_replace(ReplaceAll::TextChanged, &mut drafts), None);
            assert_eq!(drafts.draft, draft);
            assert!(drafts.sent.is_empty());
        }
    }

    #[test]
    fn undo_or_redo_without_a_change_keeps_everything_selected() {
        for draft in ["前の文", ""] {
            let mut drafts = Drafts { draft: draft.into(), ..Default::default() };
            let mut selection = DraftSelection::default();
            let text_len = draft.encode_utf16().count();
            selection.select_all(draft.into(), text_len);

            assert!(!selection.on_selection_changed((0, text_len), false));
            assert!(selection.is_all());
            assert!(drafts.sent.is_empty());

            assert_eq!(selection.on_replace(ReplaceAll::TextChanged, &mut drafts), Some(AfterReplace::KeepEditing));
            assert_eq!(drafts.sent.first().map(String::as_str), (!draft.is_empty()).then_some(draft));
        }
    }

    #[test]
    fn clicking_the_bar_preserves_full_text_selection() {
        let mut selection = DraftSelection::default();
        let draft = "前の文";
        let text_len = draft.encode_utf16().count();
        selection.select_all(draft.into(), text_len);

        assert!(!selection.on_selection_changed((0, text_len), true));
        assert!(selection.is_all());
    }

    #[test]
    fn archiving_before_new_query_uses_live_draft() {
        let mut drafts = Drafts { draft: "保存した文".into(), ..Default::default() };
        assert!(drafts.archive_draft(Some("書きかけの文\n続き")));
        assert!(drafts.draft.is_empty());
        assert_eq!(Browse::default().older(&drafts.sent, "").as_deref(), Some("書きかけの文\n続き"));
        assert_eq!(drafts.sent.len(), 1);

        drafts.draft = "消した文".into();
        assert!(drafts.archive_draft(Some("")));
        assert!(drafts.draft.is_empty());
        assert_eq!(drafts.sent.len(), 1);

        drafts.draft = "閉じた窓の文".into();
        assert!(drafts.archive_draft(None));
        assert_eq!(drafts.sent[0], "閉じた窓の文");
    }

    #[test]
    fn selecting_only_text_or_formatting_preserves_attachments_on_edit() {
        for (draft, text_len) in [("前の文", 3), ("", 0)] {
            let mut drafts = Drafts { draft: draft.into(), ..Default::default() };
            let mut selection = DraftSelection::default();
            selection.select_all(drafts.draft.clone(), text_len);
            assert!(selection.keep_editing());
            assert!(!selection.on_selection_changed((0, text_len), false));
            assert_eq!(selection.on_replace(ReplaceAll::TextChanged, &mut drafts), None);
            assert_eq!(drafts.draft, draft);
            assert!(drafts.sent.is_empty());
        }
    }

    #[test]
    fn plain_arrows_move_selection_even_in_recalled_text() {
        let mut selection = DraftSelection::default();
        selection.on_hide("呼び出した文", 1);
        assert!(selection.take_reopen());
        selection.select_all("呼び出した文".into(), 6);
        assert!(!selection.uses_plain_history(true, false, true));
        assert!(!selection.uses_plain_history(false, false, true));
        assert!(selection.on_selection_changed((6, 6), false));
        assert!(selection.uses_plain_history(true, false, true));
        assert!(selection.uses_plain_history(false, false, true));
    }

    #[test]
    fn json_roundtrip_and_push() {
        let mut d = Drafts::default();
        d.draft = "# 書きかけ\n- a".into();
        d.push_sent("一つ目");
        d.push_sent("二つ目");
        d.push_sent("一つ目");
        assert_eq!(d.sent, vec!["一つ目", "二つ目"]);
        let back = Drafts::from_json(&d.to_json());
        assert_eq!(back, d);
        assert_eq!(Drafts::from_json("壊れた"), Drafts::default());
        for i in 0..30 {
            d.push_sent(&i.to_string());
        }
        assert_eq!(d.sent.len(), SENT_MAX);
        assert_eq!(d.sent[0], "29");
    }

    #[test]
    fn browse() {
        let sent = vec!["新".to_string(), "古".to_string()];
        let mut b = Browse::default();
        assert_eq!(b.newer(&sent), None);
        assert_eq!(b.older(&sent, "書きかけ").as_deref(), Some("新"));
        assert_eq!(b.older(&sent, "新").as_deref(), Some("古"));
        assert_eq!(b.older(&sent, "古"), None);
        assert_eq!(b.newer(&sent).as_deref(), Some("新"));
        assert_eq!(b.newer(&sent).as_deref(), Some("書きかけ"));
        assert!(!b.is_browsing());
    }
}
