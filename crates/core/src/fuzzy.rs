//! あいまい一致(B2). 安いふるい(飛び飛びに全部含むか)を先にかけ, 通ったものだけ点数を付ける(J1).
//! 点数は 先頭 > 単語の先頭 > 連続 > 飛び飛び. 同じ点なら短い名前が上(呼ぶ側で名前の長さを引く).

/// 1文字一致したときの基本点
const BASE: i32 = 16;
/// 名前の先頭で一致
const BONUS_PREFIX: i32 = 64;
/// 単語の先頭(区切り文字の次)で一致
const BONUS_WORD: i32 = 32;
/// 小文字→大文字の切れ目(PowerPoint の P)で一致
const BONUS_CAMEL: i32 = 28;
/// 1つ前の文字の直後で一致
const BONUS_CONSEC: i32 = 24;
/// 飛ばし始めの減点と, 1文字飛ばすごとの減点
const GAP_START: i32 = -6;
const GAP_EXT: i32 = -1;
/// 点数を付ける名前の最大の長さ(これより長い部分は見ない)
const MAX_LEN: usize = 256;
const NEG: i32 = i32::MIN / 4;

/// 一致の結果. `positions`は名前の中の文字の位置(charの番号). 太字(J7)に使う.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    pub positions: Vec<u32>,
}

/// 小文字にした1文字. 英字以外もUnicodeの小文字にする.
#[inline]
pub fn lower_char(c: char) -> char {
    if c.is_ascii() {
        c.to_ascii_lowercase()
    } else {
        // 全角英数字は半角にそろえる
        let c = match c as u32 {
            0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        };
        c.to_lowercase().next().unwrap_or(c)
    }
}

pub fn lower_str(s: &str) -> String {
    s.chars().map(lower_char).collect()
}

/// 検索語を一致に使う形にする: 小文字にし, 空白を除く.
pub fn prepare_query(q: &str) -> Vec<char> {
    q.chars().filter(|c| !c.is_whitespace()).map(lower_char).collect()
}

/// 安いふるい: `lower`(小文字にした名前)が`query`を飛び飛びに全部含むか.
#[inline]
pub fn is_subsequence(query: &[char], lower: &str) -> bool {
    let mut it = query.iter();
    let mut want = match it.next() {
        Some(c) => *c,
        None => return true,
    };
    for c in lower.chars() {
        if c == want {
            match it.next() {
                Some(n) => want = *n,
                None => return true,
            }
        }
    }
    false
}

/// ASCIIだけの検索語とASCIIだけの名前のための, バイト単位のふるい(速い).
#[inline]
pub fn is_subsequence_ascii(query: &[u8], lower: &[u8]) -> bool {
    let mut qi = 0;
    if query.is_empty() {
        return true;
    }
    for &b in lower {
        if b == query[qi] {
            qi += 1;
            if qi == query.len() {
                return true;
            }
        }
    }
    false
}

fn is_sep(c: char) -> bool {
    matches!(
        c,
        ' ' | '_' | '-' | '.' | '\\' | '/' | ':' | '(' | ')' | '[' | ']' | '　' | '・' | '「' | '」' | '+' | ','
    )
}

fn bonus_at(orig: &[char], j: usize) -> i32 {
    if j == 0 {
        return BONUS_PREFIX;
    }
    let a = orig[j - 1];
    let b = orig[j];
    if is_sep(a) {
        BONUS_WORD
    } else if a.is_lowercase() && b.is_uppercase() {
        BONUS_CAMEL
    } else if !a.is_ascii_digit() && b.is_ascii_digit() {
        BONUS_CAMEL / 2
    } else if a.is_ascii() != b.is_ascii() {
        // 英字と日本語の切れ目
        BONUS_WORD / 2
    } else {
        0
    }
}

/// 引用符で囲んだ語は, 空白も含めて連続して一致する範囲を返す.
pub(crate) fn match_contiguous(text: &str, name: &str, lower: &str) -> Option<Match> {
    let offset = lower.find(text)?;
    let start = lower[..offset].chars().count();
    let len = text.chars().count();
    let original: Vec<char> = name.chars().take(start + 1).collect();
    let score = len as i32 * BASE + (len as i32 - 1) * BONUS_CONSEC + bonus_at(&original, start);
    Some(Match { score, positions: (start as u32..(start + len) as u32).collect() })
}

/// 名前`name`に検索語`query`(prepare_query済み)が一致すれば点数と位置を返す.
/// `lower`は`lower_str(name)`(呼ぶ側で覚えておけば毎回作らずに済む).
pub fn score(query: &[char], name: &str, lower: &str) -> Option<Match> {
    if query.is_empty() {
        return Some(Match { score: 0, positions: vec![] });
    }
    if !is_subsequence(query, lower) {
        return None;
    }
    let orig: Vec<char> = name.chars().take(MAX_LEN).collect();
    let low: Vec<char> = lower.chars().take(MAX_LEN).collect();
    let n = low.len().min(orig.len());
    let m = query.len();
    if m > n {
        return None;
    }
    let bonus: Vec<i32> = (0..n).map(|j| bonus_at(&orig, j)).collect();

    // prev[j]: 1つ前の検索文字を名前のjで一致させたときの最高点
    let mut prev = vec![NEG; n];
    let mut cur = vec![NEG; n];
    // from[i*n+j]: 検索文字iをjで一致させたとき, 検索文字i-1を一致させた位置
    let mut from = vec![u16::MAX; m * n];

    for j in 0..n {
        if low[j] == query[0] {
            // 後ろで始まるほど少し減点(同じ条件なら前で一致した方が上)
            prev[j] = BASE + bonus[j] - (j as i32).min(8);
        }
    }
    for i in 1..m {
        let mut run = NEG; // 1文字以上飛ばして来たときの最高点
        let mut run_k = u16::MAX;
        for j in 0..n {
            cur[j] = NEG;
            if j >= 2 {
                let cand = prev[j - 2] + GAP_START;
                if run != NEG {
                    run += GAP_EXT;
                }
                if cand > run {
                    run = cand;
                    run_k = (j - 2) as u16;
                }
            }
            if low[j] != query[i] || j < i {
                continue;
            }
            let consec = if j >= 1 && prev[j - 1] > NEG { prev[j - 1] + BONUS_CONSEC } else { NEG };
            let (best, k) = if consec >= run { (consec, (j.wrapping_sub(1)) as u16) } else { (run, run_k) };
            if best <= NEG {
                continue;
            }
            cur[j] = best + BASE + bonus[j];
            from[i * n + j] = k;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let (mut j, best) = prev
        .iter()
        .enumerate()
        .filter(|(_, s)| **s > NEG)
        .max_by_key(|(j, s)| (**s, std::cmp::Reverse(*j)))
        .map(|(j, s)| (j, *s))?;
    let mut positions = vec![0u32; m];
    for i in (0..m).rev() {
        positions[i] = j as u32;
        if i > 0 {
            j = from[i * n + j] as usize;
        }
    }
    Some(Match { score: best, positions })
}

/// 並べるときの点数: 一致の点数から名前の長さを引く(同じ一致なら短い名前が上).
pub fn rank_score(m: &Match, name: &str) -> i32 {
    m.score - name.chars().count().min(MAX_LEN) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(q: &str, name: &str) -> Option<i32> {
        score(&prepare_query(q), name, &lower_str(name)).map(|m| m.score)
    }

    #[test]
    fn scattered_match() {
        let m = score(&prepare_query("pwpt"), "PowerPoint", &lower_str("PowerPoint")).unwrap();
        assert_eq!(m.positions, vec![0, 2, 5, 9]);
    }

    #[test]
    fn no_match() {
        assert!(s("xyz", "PowerPoint").is_none());
        assert!(s("pp", "P").is_none());
    }

    #[test]
    fn prefix_beats_word_start_beats_middle() {
        let prefix = s("rep", "report.docx").unwrap();
        let word = s("rep", "weekly report.docx").unwrap();
        let middle = s("rep", "prepare.docx").unwrap();
        assert!(prefix > word, "{prefix} {word}");
        assert!(word > middle, "{word} {middle}");
    }

    #[test]
    fn consecutive_beats_scattered() {
        let r = |q: &str, n: &str| rank_score(&score(&prepare_query(q), n, &lower_str(n)).unwrap(), n);
        let a = r("note", "Notepad");
        let b = r("note", "No Time Edit");
        assert!(a > b, "{a} {b}");
    }

    #[test]
    fn camel_case_bonus() {
        let m = score(&prepare_query("pp"), "PowerPoint", &lower_str("PowerPoint")).unwrap();
        assert_eq!(m.positions, vec![0, 5]);
    }

    #[test]
    fn best_alignment_is_chosen() {
        // 1つ目の"c"ではなく, 単語の先頭の"c"から"code"を取る
        let m = score(&prepare_query("code"), "acme Code", &lower_str("acme Code")).unwrap();
        assert_eq!(m.positions, vec![5, 6, 7, 8]);
    }

    #[test]
    fn japanese_and_fullwidth() {
        assert!(s("会議", "定例会議.xlsx").is_some());
        assert!(s("abc", "ＡＢＣ.txt").is_some());
    }

    #[test]
    fn ascii_prefilter() {
        assert!(is_subsequence_ascii(b"pwpt", b"powerpoint"));
        assert!(!is_subsequence_ascii(b"pwpz", b"powerpoint"));
    }
}
