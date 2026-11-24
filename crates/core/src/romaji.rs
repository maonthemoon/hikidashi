//! ローマ字→ひらがな. Migemo(B3)で使う. 打ちかけの末尾(`kaig`の`g`)は, 続きうるかなを全部出す.

/// (ローマ字, かな). 長いものから試す.
const TABLE: &[(&str, &str)] = &[
    // 3文字
    ("kya", "きゃ"), ("kyu", "きゅ"), ("kyo", "きょ"), ("kye", "きぇ"), ("kyi", "きぃ"),
    ("gya", "ぎゃ"), ("gyu", "ぎゅ"), ("gyo", "ぎょ"), ("gye", "ぎぇ"), ("gyi", "ぎぃ"),
    ("sha", "しゃ"), ("shi", "し"), ("shu", "しゅ"), ("she", "しぇ"), ("sho", "しょ"),
    ("sya", "しゃ"), ("syu", "しゅ"), ("syo", "しょ"), ("sye", "しぇ"), ("syi", "しぃ"),
    ("zya", "じゃ"), ("zyu", "じゅ"), ("zyo", "じょ"), ("zye", "じぇ"), ("zyi", "じぃ"),
    ("jya", "じゃ"), ("jyu", "じゅ"), ("jyo", "じょ"), ("jye", "じぇ"), ("jyi", "じぃ"),
    ("cha", "ちゃ"), ("chi", "ち"), ("chu", "ちゅ"), ("che", "ちぇ"), ("cho", "ちょ"),
    ("tya", "ちゃ"), ("tyu", "ちゅ"), ("tyo", "ちょ"), ("tye", "ちぇ"), ("tyi", "ちぃ"),
    ("cya", "ちゃ"), ("cyu", "ちゅ"), ("cyo", "ちょ"),
    ("dya", "ぢゃ"), ("dyu", "ぢゅ"), ("dyo", "ぢょ"),
    ("tsu", "つ"), ("tsa", "つぁ"), ("tsi", "つぃ"), ("tse", "つぇ"), ("tso", "つぉ"),
    ("thi", "てぃ"), ("thu", "てゅ"), ("dhi", "でぃ"), ("dhu", "でゅ"),
    ("twu", "とぅ"), ("dwu", "どぅ"),
    ("nya", "にゃ"), ("nyu", "にゅ"), ("nyo", "にょ"), ("nye", "にぇ"), ("nyi", "にぃ"),
    ("hya", "ひゃ"), ("hyu", "ひゅ"), ("hyo", "ひょ"), ("hye", "ひぇ"), ("hyi", "ひぃ"),
    ("bya", "びゃ"), ("byu", "びゅ"), ("byo", "びょ"), ("bye", "びぇ"), ("byi", "びぃ"),
    ("pya", "ぴゃ"), ("pyu", "ぴゅ"), ("pyo", "ぴょ"), ("pye", "ぴぇ"), ("pyi", "ぴぃ"),
    ("mya", "みゃ"), ("myu", "みゅ"), ("myo", "みょ"), ("mye", "みぇ"), ("myi", "みぃ"),
    ("rya", "りゃ"), ("ryu", "りゅ"), ("ryo", "りょ"), ("rye", "りぇ"), ("ryi", "りぃ"),
    ("lya", "ゃ"), ("lyu", "ゅ"), ("lyo", "ょ"), ("xya", "ゃ"), ("xyu", "ゅ"), ("xyo", "ょ"),
    ("ltu", "っ"), ("xtu", "っ"), ("lwa", "ゎ"), ("xwa", "ゎ"),
    ("wha", "うぁ"), ("whi", "うぃ"), ("whe", "うぇ"), ("who", "うぉ"),
    ("fya", "ふゃ"), ("fyu", "ふゅ"), ("fyo", "ふょ"),
    ("vya", "ゔゃ"), ("vyu", "ゔゅ"), ("vyo", "ゔょ"),
    // 2文字
    ("ka", "か"), ("ki", "き"), ("ku", "く"), ("ke", "け"), ("ko", "こ"),
    ("ca", "か"), ("ci", "し"), ("cu", "く"), ("ce", "せ"), ("co", "こ"),
    ("qa", "くぁ"), ("qi", "くぃ"), ("qu", "く"), ("qe", "くぇ"), ("qo", "くぉ"),
    ("ga", "が"), ("gi", "ぎ"), ("gu", "ぐ"), ("ge", "げ"), ("go", "ご"),
    ("sa", "さ"), ("si", "し"), ("su", "す"), ("se", "せ"), ("so", "そ"),
    ("za", "ざ"), ("zi", "じ"), ("zu", "ず"), ("ze", "ぜ"), ("zo", "ぞ"),
    ("ja", "じゃ"), ("ji", "じ"), ("ju", "じゅ"), ("je", "じぇ"), ("jo", "じょ"),
    ("ta", "た"), ("ti", "ち"), ("tu", "つ"), ("te", "て"), ("to", "と"),
    // diはぢではなくでぃにする(名前ではディスプレイのような外来語の方がずっと多い)
    ("da", "だ"), ("di", "でぃ"), ("du", "づ"), ("de", "で"), ("do", "ど"),
    ("na", "な"), ("ni", "に"), ("nu", "ぬ"), ("ne", "ね"), ("no", "の"),
    ("ha", "は"), ("hi", "ひ"), ("hu", "ふ"), ("he", "へ"), ("ho", "ほ"),
    ("fa", "ふぁ"), ("fi", "ふぃ"), ("fu", "ふ"), ("fe", "ふぇ"), ("fo", "ふぉ"),
    ("ba", "ば"), ("bi", "び"), ("bu", "ぶ"), ("be", "べ"), ("bo", "ぼ"),
    ("pa", "ぱ"), ("pi", "ぴ"), ("pu", "ぷ"), ("pe", "ぺ"), ("po", "ぽ"),
    ("ma", "ま"), ("mi", "み"), ("mu", "む"), ("me", "め"), ("mo", "も"),
    ("ya", "や"), ("yi", "い"), ("yu", "ゆ"), ("ye", "いぇ"), ("yo", "よ"),
    ("ra", "ら"), ("ri", "り"), ("ru", "る"), ("re", "れ"), ("ro", "ろ"),
    ("la", "ら"), ("li", "り"), ("lu", "る"), ("le", "れ"), ("lo", "ろ"),
    ("wa", "わ"), ("wi", "うぃ"), ("wu", "う"), ("we", "うぇ"), ("wo", "を"),
    ("va", "ゔぁ"), ("vi", "ゔぃ"), ("vu", "ゔ"), ("ve", "ゔぇ"), ("vo", "ゔぉ"),
    ("xa", "ぁ"), ("xi", "ぃ"), ("xu", "ぅ"), ("xe", "ぇ"), ("xo", "ぉ"),
    ("nn", "ん"), ("n'", "ん"), ("xn", "ん"),
    // 1文字
    ("a", "あ"), ("i", "い"), ("u", "う"), ("e", "え"), ("o", "お"),
    ("-", "ー"),
];

const VOWELS: [&str; 5] = ["a", "i", "u", "e", "o"];

/// 打ちかけの末尾を含まない部分をかなにする. 返り値は(かな, 残り). 変換できない文字があればNone.
pub fn to_hiragana_partial(romaji: &str) -> Option<(String, String)> {
    let s = romaji.to_ascii_lowercase();
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    'outer: while i < b.len() {
        // 促音: 同じ子音が2つ続く(nn は「ん」)
        if i + 1 < b.len() && b[i] == b[i + 1] && b[i].is_ascii_alphabetic() && !b"aiueon".contains(&b[i]) {
            out.push('っ');
            i += 1;
            continue;
        }
        // nn の次に母音やyが来るなら, 1つ目のnだけが「ん」(konnichiha → こんにちは)
        if b[i] == b'n' && i + 2 < b.len() && b[i + 1] == b'n' && b"aiueoy".contains(&b[i + 2]) {
            out.push('ん');
            i += 1;
            continue;
        }
        for len in (1..=3).rev() {
            if i + len <= b.len() {
                let part = &s[i..i + len];
                if let Some((_, kana)) = TABLE.iter().find(|(r, _)| *r == part) {
                    out.push_str(kana);
                    i += len;
                    continue 'outer;
                }
            }
        }
        // 子音の前のn(または末尾以外のn)は「ん」
        if b[i] == b'n' && i + 1 < b.len() && !b"aiueoy".contains(&b[i + 1]) {
            out.push('ん');
            i += 1;
            continue;
        }
        // ここで変換できないのは打ちかけの末尾だけ(最大3文字)
        let rest = &s[i..];
        if rest.len() <= 3 && rest.bytes().all(|c| c.is_ascii_alphabetic()) {
            return Some((out, rest.to_string()));
        }
        return None;
    }
    Some((out, String::new()))
}

/// ローマ字の入力から, 打ちかけも含めて考えられるかなの読み(前方一致に使う)を全部出す.
/// 例: `kaig` → [かいが, かいぎ, かいぐ, かいげ, かいご], `kaigi` → [かいぎ]
pub fn hiragana_prefixes(romaji: &str) -> Vec<String> {
    let Some((kana, rest)) = to_hiragana_partial(romaji) else {
        return vec![];
    };
    if rest.is_empty() {
        return vec![kana];
    }
    let mut out = Vec::new();
    if rest == "n" {
        out.push(format!("{kana}ん"));
    }
    let mut tails: Vec<String> = VOWELS.iter().map(|v| format!("{rest}{v}")).collect();
    for y in ["ya", "yu", "yo"] {
        tails.push(format!("{rest}{y}"));
    }
    if rest.len() == 1 {
        for h in ["ha", "hi", "hu", "he", "ho"] {
            tails.push(format!("{rest}{h}"));
        }
        tails.push(format!("{rest}su"));
    }
    for t in tails {
        if let Some((k, r)) = to_hiragana_partial(&t) {
            if r.is_empty() {
                let full = format!("{kana}{k}");
                if !out.contains(&full) {
                    out.push(full);
                }
            }
        }
    }
    out
}

/// ひらがな→カタカナ
pub fn to_katakana(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ぁ'..='ゖ' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// カタカナ→ひらがな(辞書を引くとき, 名前のカタカナを読みにそろえる)
pub fn to_hiragana(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_words() {
        assert_eq!(to_hiragana_partial("kaigi"), Some(("かいぎ".into(), "".into())));
        assert_eq!(to_hiragana_partial("shiryou"), Some(("しりょう".into(), "".into())));
        assert_eq!(to_hiragana_partial("kitte"), Some(("きって".into(), "".into())));
        assert_eq!(to_hiragana_partial("shinbun"), Some(("しんぶ".into(), "n".into())));
        assert!(hiragana_prefixes("shinbun").contains(&"しんぶん".to_string()));
        assert_eq!(to_hiragana_partial("konnichiha"), Some(("こんにちは".into(), "".into())));
        assert_eq!(to_hiragana_partial("disupurei"), Some(("でぃすぷれい".into(), "".into())));
        assert_eq!(to_hiragana_partial("pawa-"), Some(("ぱわー".into(), "".into())));
    }

    #[test]
    fn partial_tail() {
        assert_eq!(to_hiragana_partial("kaig"), Some(("かい".into(), "g".into())));
        assert_eq!(to_hiragana_partial("kaish"), Some(("かい".into(), "sh".into())));
        let p = hiragana_prefixes("kaig");
        assert!(p.contains(&"かいぎ".to_string()));
        assert!(p.contains(&"かいが".to_string()));
        let p = hiragana_prefixes("kaish");
        assert!(p.contains(&"かいし".to_string()));
        assert!(p.contains(&"かいしゃ".to_string()));
        let p = hiragana_prefixes("hon");
        assert!(p.contains(&"ほん".to_string()));
        assert!(p.contains(&"ほな".to_string()));
    }

    #[test]
    fn not_romaji() {
        assert_eq!(to_hiragana_partial("pwpt"), None);
        assert!(hiragana_prefixes("xyz1").is_empty());
    }

    #[test]
    fn kana_conversion() {
        assert_eq!(to_katakana("ぱわーぽいんと"), "パワーポイント");
        assert_eq!(to_hiragana("ディスプレイ"), "でぃすぷれい");
    }
}
