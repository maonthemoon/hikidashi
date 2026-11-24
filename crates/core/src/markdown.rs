//! 長い質問を書く窓(K22)のMarkdown. 行ごとに, 整えて見せる範囲(見出し, 太字など)と記号の範囲を返す.
//! 位置はUTF-16の単位で, 文の頭からの位置(RichEditの文字位置と同じ). 改行は`\r`か`\n`の1文字とする.
//! 画面(RichEdit)は, カーソルのない行の記号を隠すか薄くし, 範囲に合わせて字を変える.
//! 箇条書き・引用・チェックボックスの印(`marker`, `check`)は, カーソルが印の中になければ, 行にカーソルがあっても絵(•, 縦線, □)にする(Obsidianと同じ).

/// 行の種類
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Plain,
    /// `#`〜`######`(1〜6)
    Heading(u8),
    /// `- `, `* `, `+ `
    Bullet,
    /// `- [ ] `, `- [x] `(済みならtrue)
    Task(bool),
    /// `1. `, `1) `
    Ordered,
    /// `> `
    Quote,
    /// ```` ``` ````で囲んだ中
    Code,
    /// ```` ``` ````の行
    Fence,
    /// `---`, `***`, `___`
    Rule,
}

/// 行の中の飾り
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inline {
    Bold,
    Italic,
    Strike,
    Code,
    /// `[文字](URL)`の文字
    Link,
    /// `https://…`
    Url,
    /// `==文字==`(蛍光ペン)
    Highlight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: Inline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// 行の頭の位置
    pub start: usize,
    /// 行の長さ(改行を含まない)
    pub len: usize,
    pub block: Block,
    /// 箇条書きの深さ(頭の空白2つで1段. タブは1段)
    pub level: usize,
    /// 頭の空白の終わり(箇条書きと引用のとき. 空白は字下げにして見せる)
    pub lead: usize,
    /// 箇条書き・引用の印(`- `, `1. `, `> `. チェックボックスは`- `だけ)
    pub marker: Option<(usize, usize)>,
    /// チェックボックスの`[ ]`(3文字)
    pub check: Option<(usize, usize)>,
    /// 見出しと行の中の記号の範囲(カーソルのない行では隠すか薄くする)
    pub marks: Vec<(usize, usize)>,
    /// 飾りの範囲(記号を含まない)
    pub spans: Vec<Span>,
}

impl Line {
    pub fn end(&self) -> usize {
        self.start + self.len
    }

    /// 印(頭の空白から, チェックボックスなら`[ ] `まで)の終わり. 印がなければ行の頭
    pub fn marker_end(&self) -> usize {
        match (self.check, self.marker) {
            (Some((_, e)), _) => (e + 1).min(self.end()),
            (None, Some((_, e))) => e,
            _ => self.start,
        }
    }
}

fn u16len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// 文全体を行に分けて読む
pub fn parse(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let mut in_code = false;
    for raw in text.split(['\r', '\n']) {
        let len = u16len(raw);
        out.push(parse_line(raw, pos, &mut in_code));
        pos += len + 1;
    }
    out
}

/// 行の頭の空白(箇条書きの深さ)
fn leading(raw: &str) -> (usize, usize) {
    let mut bytes = 0;
    let mut cols = 0;
    for c in raw.chars() {
        match c {
            ' ' => cols += 1,
            '\t' => cols += 2,
            _ => break,
        }
        bytes += 1;
    }
    (bytes, cols / 2)
}

fn is_rule(t: &str) -> bool {
    let s: String = t.chars().filter(|c| *c != ' ').collect();
    s.len() >= 3 && (s.chars().all(|c| c == '-') || s.chars().all(|c| c == '*') || s.chars().all(|c| c == '_'))
}

/// 箇条書きの印の長さ(頭の空白は含まない). (印の長さ, 種類)
fn list_marker(t: &str) -> Option<(usize, Block)> {
    let b = t.as_bytes();
    if b.len() >= 2 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ' {
        // チェックボックス
        if b.len() >= 6 && b[2] == b'[' && matches!(b[3], b' ' | b'x' | b'X') && b[4] == b']' && b[5] == b' ' {
            return Some((2, Block::Task(b[3] != b' ')));
        }
        return Some((2, Block::Bullet));
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits) && b.len() >= digits + 2 && matches!(b[digits], b'.' | b')') && b[digits + 1] == b' ' {
        return Some((digits + 2, Block::Ordered));
    }
    None
}

fn parse_line(raw: &str, start: usize, in_code: &mut bool) -> Line {
    let len = u16len(raw);
    let mut line = Line { start, len, block: Block::Plain, level: 0, lead: start, marker: None, check: None, marks: vec![], spans: vec![] };
    let trimmed = raw.trim_start_matches([' ', '\t']);
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        *in_code = !*in_code;
        line.block = Block::Fence;
        return line;
    }
    if *in_code {
        line.block = Block::Code;
        return line;
    }
    let (lead_bytes, level) = leading(raw);
    let lead = u16len(&raw[..lead_bytes]);
    let mut content = lead_bytes;
    // 見出し(頭の空白は3つまで)
    let hashes = trimmed.bytes().take_while(|c| *c == b'#').count();
    if lead_bytes <= 3 && (1..=6).contains(&hashes) && (trimmed.len() == hashes || trimmed.as_bytes()[hashes] == b' ') {
        line.block = Block::Heading(hashes as u8);
        let m = (hashes + 1).min(trimmed.len());
        line.marks.push((start, start + lead + m));
        content = lead_bytes + m;
    } else if is_rule(trimmed) {
        line.block = Block::Rule;
        return line;
    } else if let Some((m, block)) = list_marker(trimmed) {
        line.block = block;
        line.level = level;
        line.lead = start + lead;
        line.marker = Some((start + lead, start + lead + m));
        content = lead_bytes + m;
        if let Block::Task(_) = block {
            line.check = Some((start + lead + m, start + lead + m + 3));
            content += 4;
        }
    } else if let Some(rest) = trimmed.strip_prefix('>') {
        line.block = Block::Quote;
        let m = if rest.starts_with(' ') { 2 } else { 1 };
        line.lead = start + lead;
        line.marker = Some((start + lead, start + lead + m));
        content = lead_bytes + m;
    }
    let base = start + u16len(&raw[..content]);
    inline(&raw[content..], base, &mut line.marks, &mut line.spans);
    line
}

/// 行の中の飾りを読む. `base`は`s`の頭の位置
fn inline(s: &str, base: usize, marks: &mut Vec<(usize, usize)>, spans: &mut Vec<Span>) {
    let chars: Vec<char> = s.chars().collect();
    // 文字の番号 → UTF-16の位置
    let mut at = Vec::with_capacity(chars.len() + 1);
    let mut p = base;
    for c in &chars {
        at.push(p);
        p += c.len_utf16();
    }
    at.push(p);
    walk(&chars, &at, 0, chars.len(), marks, spans);
}

fn starts(chars: &[char], i: usize, pat: &str) -> bool {
    let mut k = i;
    for c in pat.chars() {
        if chars.get(k) != Some(&c) {
            return false;
        }
        k += 1;
    }
    true
}

fn word(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric())
}

/// 閉じの記号を探す. 中が空や空白で始まる・終わるものは飾りにしない
fn find_close(chars: &[char], from: usize, to: usize, pat: &str) -> Option<usize> {
    let n = pat.chars().count();
    if from >= to || chars[from] == ' ' {
        return None;
    }
    let mut j = from + 1;
    while j + n <= to {
        if starts(chars, j, pat) && chars[j - 1] != ' ' {
            // `*`1つのときは`**`の片方を閉じにしない
            if n == 1 && ((j + 1 < to && chars[j + 1] == chars[j]) || chars[j - 1] == chars[j]) {
                j += 1;
                continue;
            }
            // `***`のように続くときは, 続きの最後を閉じにする(`**太字の*斜体***`)
            if n == 2 {
                while j + n < to && chars[j + n] == chars[j] {
                    j += 1;
                }
            }
            return Some(j);
        }
        j += 1;
    }
    None
}

fn walk(chars: &[char], at: &[usize], from: usize, to: usize, marks: &mut Vec<(usize, usize)>, spans: &mut Vec<Span>) {
    let mut i = from;
    while i < to {
        let c = chars[i];
        // コード. 中は読まない
        if c == '`' {
            let n = chars[i..to].iter().take_while(|x| **x == '`').count();
            let pat: String = "`".repeat(n);
            let mut j = i + n;
            let mut found = None;
            while j + n <= to {
                if starts(chars, j, &pat) && chars.get(j + n) != Some(&'`') {
                    found = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(j) = found.filter(|j| *j > i + n) {
                marks.push((at[i], at[i + n]));
                spans.push(Span { start: at[i + n], end: at[j], style: Inline::Code });
                marks.push((at[j], at[j + n]));
                i = j + n;
                continue;
            }
            i += n;
            continue;
        }
        // リンク [文字](URL)
        if c == '[' {
            if let Some(close) = (i + 1..to).find(|k| chars[*k] == ']') {
                if close > i + 1 && chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = (close + 2..to).find(|k| chars[*k] == ')') {
                        marks.push((at[i], at[i + 1]));
                        spans.push(Span { start: at[i + 1], end: at[close], style: Inline::Link });
                        walk(chars, at, i + 1, close, marks, spans);
                        marks.push((at[close], at[end + 1]));
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        // URL
        if (c == 'h') && (starts(chars, i, "https://") || starts(chars, i, "http://")) && !word(i.checked_sub(1).and_then(|k| chars.get(k))) {
            let mut j = i;
            while j < to && !chars[j].is_whitespace() && !matches!(chars[j], '<' | '>' | '"' | '）' | '」' | '、' | '。') {
                j += 1;
            }
            // 末尾の句読点は含めない
            while j > i && matches!(chars[j - 1], '.' | ',' | ')' | ';' | ':' | '!' | '?') {
                j -= 1;
            }
            spans.push(Span { start: at[i], end: at[j], style: Inline::Url });
            i = j;
            continue;
        }
        // 太字, 取り消し線
        if let Some((pat, style)) = [("**", Inline::Bold), ("__", Inline::Bold), ("~~", Inline::Strike), ("==", Inline::Highlight)].into_iter().find(|(p, _)| starts(chars, i, p)) {
            let under = pat == "__";
            let close = if under && word(i.checked_sub(1).and_then(|k| chars.get(k))) { None } else { find_close(chars, i + 2, to, pat) };
            if let Some(j) = close.filter(|j| !(under && word(chars.get(j + 2)))) {
                marks.push((at[i], at[i + 2]));
                spans.push(Span { start: at[i + 2], end: at[j], style });
                walk(chars, at, i + 2, j, marks, spans);
                marks.push((at[j], at[j + 2]));
                i = j + 2;
            } else {
                // 閉じがない記号は字のまま
                i += 2;
            }
            continue;
        }
        // 斜体
        if c == '*' || c == '_' {
            let boundary_ok = c == '*' || !word(i.checked_sub(1).and_then(|k| chars.get(k)));
            if boundary_ok && chars.get(i + 1) != Some(&c) {
                let pat = c.to_string();
                if let Some(j) = find_close(chars, i + 1, to, &pat) {
                    if c == '*' || !word(chars.get(j + 1)) {
                        marks.push((at[i], at[i + 1]));
                        spans.push(Span { start: at[i + 1], end: at[j], style: Inline::Italic });
                        walk(chars, at, i + 1, j, marks, spans);
                        marks.push((at[j], at[j + 1]));
                        i = j + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
}

/// `Shift+Enter`で改行したときにすること(箇条書きの続き. Q2のB)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewLine {
    /// 改行のあとにこの文字を入れる(次の印)
    Continue(String),
    /// 印だけの行なので, 行の頭からこの長さ(UTF-16)の印を消して, 改行しない
    EndList(usize),
    /// 普通の改行
    Plain,
}

/// カーソルのある行(カーソルより前の部分)から, 改行したときにすることを決める
pub fn new_line(before_caret: &str, whole_line: &str) -> NewLine {
    let (lead_bytes, _) = leading(whole_line);
    let lead = &whole_line[..lead_bytes];
    let t = &whole_line[lead_bytes..];
    if before_caret.len() < lead_bytes {
        return NewLine::Plain;
    }
    let (marker, rest): (String, &str) = if let Some((m, block)) = list_marker(t) {
        match block {
            Block::Task(_) => (format!("{}[ ] ", &t[..m]), &t[m + 4..]),
            Block::Ordered => {
                let digits = t.bytes().take_while(|c| c.is_ascii_digit()).count();
                let n: u64 = t[..digits].parse().unwrap_or(0);
                (format!("{}{} ", n + 1, &t[digits..digits + 1]), &t[m..])
            }
            _ => (t[..m].to_string(), &t[m..]),
        }
    } else if let Some(r) = t.strip_prefix("> ") {
        ("> ".to_string(), r)
    } else if let Some(r) = t.strip_prefix('>') {
        ("> ".to_string(), r)
    } else {
        return NewLine::Plain;
    };
    // 印のうしろで改行したときだけ続ける(印の途中なら普通の改行)
    let marker_end = whole_line.len() - rest.len();
    if before_caret.len() < marker_end {
        return NewLine::Plain;
    }
    if rest.trim().is_empty() && before_caret.len() >= whole_line.trim_end().len() {
        return NewLine::EndList(u16len(&whole_line[..marker_end]));
    }
    NewLine::Continue(format!("{lead}{marker}"))
}

/// 箇条書きの行か(`Tab`で字下げする)
pub fn is_list_line(line: &str) -> bool {
    let (lead_bytes, _) = leading(line);
    list_marker(&line[lead_bytes..]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str, ranges: &[(usize, usize)]) -> Vec<String> {
        let u: Vec<u16> = src.encode_utf16().collect();
        ranges.iter().map(|(a, b)| String::from_utf16_lossy(&u[*a..*b])).collect()
    }

    fn span_texts(src: &str, l: &Line) -> Vec<(String, Inline)> {
        let u: Vec<u16> = src.encode_utf16().collect();
        l.spans.iter().map(|s| (String::from_utf16_lossy(&u[s.start..s.end]), s.style)).collect()
    }

    #[test]
    fn heading_and_marks() {
        let src = "# 見出し\n本文";
        let ls = parse(src);
        assert_eq!(ls.len(), 2);
        assert_eq!(ls[0].block, Block::Heading(1));
        assert_eq!(texts(src, &ls[0].marks), vec!["# "]);
        assert_eq!(ls[1].start, 6);
        assert_eq!(ls[1].block, Block::Plain);
        // #のうしろに空白がなければ見出しにしない
        assert_eq!(parse("#tag")[0].block, Block::Plain);
        assert_eq!(parse("### 3")[0].block, Block::Heading(3));
    }

    #[test]
    fn bold_italic_code() {
        let src = "2.4GHz帯で使う**LNA**を*急ぎで*探す. `3.3V`で";
        let l = &parse(src)[0];
        assert_eq!(span_texts(src, l), vec![("LNA".into(), Inline::Bold), ("急ぎで".into(), Inline::Italic), ("3.3V".into(), Inline::Code)]);
        assert_eq!(texts(src, &l.marks), vec!["**", "**", "*", "*", "`", "`"]);
    }

    #[test]
    fn nested_and_unclosed() {
        let src = "**太字の中の*斜体***";
        let l = &parse(src)[0];
        let st = span_texts(src, l);
        assert_eq!(st[0], ("太字の中の*斜体*".into(), Inline::Bold));
        assert_eq!(st[1], ("斜体".into(), Inline::Italic));
        // 閉じがないものは飾らない
        assert!(parse("2 * 3 = 6")[0].spans.is_empty());
        assert!(parse("**閉じない")[0].spans.is_empty());
        // snake_caseの_は斜体にしない
        assert!(parse("send_delay_ms を変える")[0].spans.is_empty());
        assert_eq!(span_texts("_斜体_", &parse("_斜体_")[0]), vec![("斜体".into(), Inline::Italic)]);
        let s = "~~消す~~";
        assert_eq!(span_texts(s, &parse(s)[0]), vec![("消す".into(), Inline::Strike)]);
    }

    #[test]
    fn code_inside_is_literal() {
        let src = "`a**b**c`";
        let l = &parse(src)[0];
        assert_eq!(span_texts(src, l), vec![("a**b**c".into(), Inline::Code)]);
    }

    #[test]
    fn lists() {
        let src = "- 項目\n  - 下の項目\n1. 番号\n- [ ] やる\n- [x] 済み\n> 引用\n---";
        let ls = parse(src);
        assert_eq!(ls[0].block, Block::Bullet);
        assert_eq!(texts(src, &[ls[0].marker.unwrap()]), vec!["- "]);
        assert!(ls[0].marks.is_empty());
        assert_eq!((ls[1].block, ls[1].level), (Block::Bullet, 1));
        assert_eq!(texts(src, &[(ls[1].start, ls[1].lead), ls[1].marker.unwrap()]), vec!["  ", "- "]);
        assert_eq!(ls[2].block, Block::Ordered);
        assert_eq!(texts(src, &[ls[2].marker.unwrap()]), vec!["1. "]);
        assert_eq!(ls[3].block, Block::Task(false));
        assert_eq!(texts(src, &[ls[3].marker.unwrap(), ls[3].check.unwrap()]), vec!["- ", "[ ]"]);
        assert_eq!(texts(src, &[(ls[3].start, ls[3].marker_end())]), vec!["- [ ] "]);
        assert_eq!(ls[4].block, Block::Task(true));
        assert_eq!(ls[5].block, Block::Quote);
        assert_eq!(texts(src, &[ls[5].marker.unwrap()]), vec!["> "]);
        assert_eq!(ls[6].block, Block::Rule);
        // `- `だけ打った行もすぐ箇条書き
        assert_eq!(parse("- ")[0].block, Block::Bullet);
        assert_eq!(parse("- ")[0].marker_end(), 2);
        let h = "==大事==";
        assert_eq!(span_texts(h, &parse(h)[0]), vec![("大事".into(), Inline::Highlight)]);
    }

    #[test]
    fn fences() {
        let src = "前\n```rust\nlet a = **b**;\n```\n後**太**";
        let ls = parse(src);
        let kinds: Vec<Block> = ls.iter().map(|l| l.block).collect();
        assert_eq!(kinds, vec![Block::Plain, Block::Fence, Block::Code, Block::Fence, Block::Plain]);
        assert!(ls[2].spans.is_empty());
        assert_eq!(span_texts(src, &ls[4]), vec![("太".into(), Inline::Bold)]);
    }

    #[test]
    fn links_and_urls() {
        let src = "[資料](https://example.com/a) と https://example.com/b. を見て";
        let l = &parse(src)[0];
        assert_eq!(span_texts(src, l), vec![("資料".into(), Inline::Link), ("https://example.com/b".into(), Inline::Url)]);
        assert_eq!(texts(src, &l.marks), vec!["[", "](https://example.com/a)"]);
    }

    #[test]
    fn utf16_positions() {
        // 𝑥はUTF-16で2単位
        let src = "𝑥\n**a**";
        let ls = parse(src);
        assert_eq!(ls[1].start, 3);
        assert_eq!(ls[1].spans[0], Span { start: 5, end: 6, style: Inline::Bold });
        // \rで区切っても同じ
        assert_eq!(parse("a\rb")[1].start, 2);
    }

    #[test]
    fn continue_lists() {
        assert_eq!(new_line("- 項目", "- 項目"), NewLine::Continue("- ".into()));
        assert_eq!(new_line("  * 下", "  * 下"), NewLine::Continue("  * ".into()));
        assert_eq!(new_line("3. 三", "3. 三"), NewLine::Continue("4. ".into()));
        assert_eq!(new_line("- [x] 済み", "- [x] 済み"), NewLine::Continue("- [ ] ".into()));
        assert_eq!(new_line("> 引用", "> 引用"), NewLine::Continue("> ".into()));
        // 印だけの行で改行したら印を消して終わる
        assert_eq!(new_line("- ", "- "), NewLine::EndList(2));
        assert_eq!(new_line("  2. ", "  2. "), NewLine::EndList(5));
        assert_eq!(new_line("普通の文", "普通の文"), NewLine::Plain);
        // 印の前で改行したら普通の改行
        assert_eq!(new_line("", "- 項目"), NewLine::Plain);
        assert!(is_list_line("  - a"));
        assert!(!is_list_line("a - b"));
    }
}
