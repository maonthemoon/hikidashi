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
