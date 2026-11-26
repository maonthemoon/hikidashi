//! ファイル検索の条件を読み取り, 索引の各項目に照合する.

use super::{FileTable, Glob, FLAG_ASCII, FLAG_DIR};
use crate::fuzzy::{self, lower_str, Match};
use crate::migemo::Dict;
use crate::search::{match_name, Prepared};

pub struct FileQuery<'a> {
    input: &'a str,
    expression: Option<Expr<Atom<'a>>>,
}

impl<'a> FileQuery<'a> {
    pub fn parse(input: &'a str) -> Self {
        if !input.bytes().any(|b| matches!(b, b':' | b'*' | b'"' | b'!' | b'&' | b'|' | b'(' | b')')) {
            return Self { input, expression: None };
        }
        let expression = tokenize(input).and_then(|tokens| {
            let multiple = tokens.len() > 1;
            let mut parser = Parser { tokens: tokens.into_iter().peekable() };
            let expr = parser.or(0)?;
            let structured = multiple || !matches!(expr, Expr::Term(Atom::Name(_)));
            (structured && parser.tokens.next().is_none()).then_some(expr)
        });
        Self { input, expression }
    }

    /// フィルターや式は索引だけにかける. 単なる文字列はアプリなどの検索にも使う.
    pub fn is_structured(&self) -> bool {
        self.expression.is_some()
    }

    pub fn prepare(self, dict: Option<&Dict>, now: u32) -> FileSearch {
        match self.expression {
            Some(expr) => FileSearch(Expr::map(expr, &|atom| atom.prepare(dict, now))),
            None => FileSearch(Expr::Term(Term::Name(Prepared::new(self.input, dict)))),
        }
    }
}

pub struct FileSearch(Expr<Term>);

impl FileSearch {
    pub(super) fn matcher(&self) -> Matcher<'_> {
        match &self.0 {
            Expr::Term(Term::Name(p)) => Matcher::Name(p),
            _ => Matcher::Expression(self),
        }
    }
}

enum Expr<T> {
    Term(T),
    Not(Box<Self>),
    And(Vec<Self>),
    Or(Vec<Self>),
}

impl<T> Expr<T> {
    fn map<U>(self, f: &impl Fn(T) -> U) -> Expr<U> {
        match self {
            Self::Term(term) => Expr::Term(f(term)),
            Self::Not(expr) => Expr::Not(Box::new(expr.map(f))),
            Self::And(exprs) => Expr::And(exprs.into_iter().map(|expr| expr.map(f)).collect()),
            Self::Or(exprs) => Expr::Or(exprs.into_iter().map(|expr| expr.map(f)).collect()),
        }
    }
}

enum Atom<'a> {
    Name(&'a str),
    QuotedName(&'a str),
    Extension(Vec<String>),
    Folder(&'a str),
    Modified(u64),
    Glob(&'a str),
}

fn quoted(s: &str) -> Option<&str> {
    let value = s.strip_prefix('"')?.strip_suffix('"')?;
    (!value.trim().is_empty() && !value.contains('"')).then_some(value)
}

impl<'a> Atom<'a> {
    fn parse(text: &'a str) -> Self {
        if let Some(value) = quoted(text) {
            return Self::QuotedName(value);
        }
        if let Some((key, rest)) = text.split_once(':') {
            let value = quoted(rest).or_else(|| {
                (!rest.is_empty() && !rest.contains('"') && !rest.contains(char::is_whitespace)).then_some(rest)
            });
            if key.eq_ignore_ascii_case("ext") {
                let exts = value.into_iter().flat_map(|v| v.split(','))
                    .map(|ext| ext.trim_start_matches('.'))
                    .filter(|ext| !ext.is_empty())
                    .map(|ext| format!(".{}", lower_str(ext)))
                    .collect::<Vec<_>>();
                return if exts.is_empty() { Self::Name(text) } else { Self::Extension(exts) };
            }
            if key.eq_ignore_ascii_case("in") {
                return value.map_or(Self::Name(text), Self::Folder);
            }
            if key.eq_ignore_ascii_case("modified") {
                return value.and_then(parse_age).map_or(Self::Name(text), Self::Modified);
            }
        }
        if !text.contains(char::is_whitespace) && !text.contains('"')
            && text.contains('*') && text.chars().any(|c| c != '*')
        {
            return Self::Glob(text);
        }
        Self::Name(text)
    }

    fn prepare(self, dict: Option<&Dict>, now: u32) -> Term {
        match self {
            Self::Name(text) => Term::Name(Prepared::new(text, dict)),
            Self::QuotedName(text) => Term::QuotedName(lower_str(text)),
            Self::Extension(exts) => Term::Extension(exts),
            Self::Folder(text) => Term::Folder(lower_str(text)),
            // 更新日時が分からない項目(0)を除く.
            Self::Modified(secs) => Term::Modified(now.saturating_sub(secs.min(u32::MAX as u64) as u32).max(1)),
            Self::Glob(text) => {
                let mut glob = Glob::new(&lower_str(text));
                glob.expand_romaji(dict);
                Term::Glob(glob)
            }
        }
    }
}

/// `7d`(日), `12h`(時間), `2w`(週)を秒にする.
fn parse_age(value: &str) -> Option<u64> {
    let (digits, unit) = value.split_at_checked(value.len().checked_sub(1)?)?;
    let n: u64 = digits.parse().ok()?;
    let hours = match unit.as_bytes()[0].to_ascii_lowercase() {
        b'h' => 1,
        b'd' => 24,
        b'w' => 24 * 7,
        _ => return None,
    };
    Some(n.saturating_mul(hours * 3600))
}

#[derive(Clone, Copy)]
enum Token<'a> {
    Word(&'a str),
    Not,
    And,
    Or,
    Open,
    Close,
}

fn tokenize(input: &str) -> Option<Vec<Token<'_>>> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut name_parens = 0;
    for (i, c) in input.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        if quoted {
            continue;
        }
        let token = match c {
            '(' if input[start..i].trim().is_empty() => Token::Open,
            '(' => {
                name_parens += 1;
                continue;
            }
            ')' if name_parens > 0 => {
                name_parens -= 1;
                continue;
            }
            ')' => Token::Close,
            '!' if input[start..i].trim().is_empty() => Token::Not,
            '&' if separated(input, i) => Token::And,
            '|' if separated(input, i) => Token::Or,
            _ => continue,
        };
        let word = input[start..i].trim();
        if !word.is_empty() {
            tokens.push(Token::Word(word));
        }
        tokens.push(token);
        start = i + 1;
        name_parens = 0;
    }
    if quoted {
        return None;
    }
    let word = input[start..].trim();
    if !word.is_empty() {
        tokens.push(Token::Word(word));
    }
    Some(tokens)
}

/// `R&D`のような名前をそのまま探せるよう, 演算子の前後を区切りで見分ける.
fn separated(input: &str, i: usize) -> bool {
    let boundary = |c: char| c.is_whitespace() || matches!(c, '(' | ')' | '"');
    input[..i].chars().next_back().is_none_or(boundary)
        || input[i + 1..].chars().next().is_none_or(|c| boundary(c) || c == '!')
}

struct Parser<'a> {
    tokens: std::iter::Peekable<std::vec::IntoIter<Token<'a>>>,
}

impl<'a> Parser<'a> {
    fn or(&mut self, depth: usize) -> Option<Expr<Atom<'a>>> {
        let first = self.and(depth)?;
        if !matches!(self.tokens.peek(), Some(Token::Or)) {
            return Some(first);
        }
        let mut exprs = vec![first];
        while matches!(self.tokens.peek(), Some(Token::Or)) {
            self.tokens.next();
            exprs.push(self.and(depth)?);
        }
        Some(Expr::Or(exprs))
    }

    fn and(&mut self, depth: usize) -> Option<Expr<Atom<'a>>> {
        let first = self.unary(depth)?;
        if !matches!(self.tokens.peek(), Some(Token::And)) {
            return Some(first);
        }
        let mut exprs = vec![first];
        while matches!(self.tokens.peek(), Some(Token::And)) {
            self.tokens.next();
            exprs.push(self.unary(depth)?);
        }
        Some(Expr::And(exprs))
    }

    fn unary(&mut self, depth: usize) -> Option<Expr<Atom<'a>>> {
        if depth >= 32 {
            return None;
        }
        match self.tokens.next()? {
            Token::Word(text) => Some(Expr::Term(Atom::parse(text))),
            Token::Not => Some(Expr::Not(Box::new(self.unary(depth + 1)?))),
            Token::Open => {
                let expr = self.or(depth + 1)?;
                matches!(self.tokens.next(), Some(Token::Close)).then_some(expr)
            }
            _ => None,
        }
    }
}

enum Term {
    Name(Prepared),
    QuotedName(String),
    Extension(Vec<String>),
    Folder(String),
    Modified(u32),
    Glob(Glob),
}

pub(super) enum Matched {
    Filter,
    Name(Match),
}

impl Expr<Term> {
    fn evaluate(&self, term_match: &impl Fn(&Term) -> Option<Matched>) -> Option<Matched> {
        match self {
            Self::Term(term) => term_match(term),
            Self::Not(expr) => expr.evaluate(term_match).is_none().then_some(Matched::Filter),
            Self::And(exprs) => {
                let mut total = Matched::Filter;
                for expr in exprs {
                    if let Matched::Name(m) = expr.evaluate(term_match)? {
                        match &mut total {
                            Matched::Filter => total = Matched::Name(m),
                            Matched::Name(total) => {
                                total.score = total.score.saturating_add(m.score);
                                total.positions.extend(m.positions);
                            }
                        }
                    }
                }
                if let Matched::Name(m) = &mut total {
                    m.positions.sort_unstable();
                    m.positions.dedup();
                }
                Some(total)
            }
            Self::Or(exprs) => {
                let mut best = None;
                for expr in exprs {
                    match expr.evaluate(term_match) {
                        Some(Matched::Name(m)) => {
                            if !matches!(&best, Some(Matched::Name(prev)) if prev.score >= m.score) {
                                best = Some(Matched::Name(m));
                            }
                        }
                        Some(Matched::Filter) if best.is_none() => best = Some(Matched::Filter),
                        _ => {}
                    }
                }
                best
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Matcher<'a> {
    Name(&'a Prepared),
    Expression(&'a FileSearch),
}

impl Matcher<'_> {
    pub(super) fn is_empty(self) -> bool {
        matches!(self, Self::Name(p) if p.is_empty())
    }

    pub(super) fn matches(self, table: &FileTable, id: u32, flags: u8) -> Option<Matched> {
        let name = table.name(id);
        let lower = table.lower(id);
        let name_match = |p: &Prepared| match_name(p, name, lower, flags & FLAG_ASCII != 0).map(Matched::Name);
        match self {
            Self::Name(p) => name_match(p),
            Self::Expression(search) => search.0.evaluate(&|term| {
                let matches = match term {
                    Term::Name(p) => return name_match(p),
                    Term::QuotedName(text) => return fuzzy::match_contiguous(text, name, lower).map(Matched::Name),
                    Term::Extension(exts) => flags & FLAG_DIR == 0 && exts.iter().any(|ext| lower.ends_with(ext.as_str())),
                    Term::Folder(text) => table.in_folder(id, text),
                    Term::Modified(min) => table.mtime[id as usize] >= *min,
                    Term::Glob(glob) => glob.matches(lower),
                };
                matches.then_some(Matched::Filter)
            }),
        }
    }
}
