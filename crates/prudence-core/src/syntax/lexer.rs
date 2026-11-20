//! Tokens. Each token records whether a newline preceded it: call `(` and
//! index `[` must sit on the same line as their callee (so a match arm that
//! starts with `(` is never mistaken for a call), while binary operators may
//! continue an expression on the next line.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    /// Decimal literal, underscores removed (`250_000` -> `250000`).
    Num(String),
    /// `5%`
    Percent(String),
    Str(String),
    /// `[1.5.1:amount]`: a form-cell reference.
    Cell(String, String),
    Sym(&'static str),
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
    pub nl_before: bool,
}

const SYMS: [&str; 33] = [
    "==>", "==", "!=", "<=", ">=", "&&", "||", "=>", "->", "(", ")", "{", "}", "[", "]", ",", ":", ".", "@", "=",
    "<", ">", "+", "-", "*", "/", "!", "|", "%", ";", "?", "_", "#",
];

pub fn lex(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line, mut col) = (0usize, 1usize, 1usize);
    let mut nl = true;
    let advance = |i: &mut usize, line: &mut usize, col: &mut usize, chars: &[char]| {
        let c = chars[*i];
        *i += 1;
        if c == '\n' {
            *line += 1;
            *col = 1;
        } else {
            *col += 1;
        }
    };
    while i < chars.len() {
        let c = chars[i];
        let pos = Pos { line, col };
        if c == '\n' {
            nl = true;
            advance(&mut i, &mut line, &mut col, &chars);
            continue;
        }
        if c.is_whitespace() {
            advance(&mut i, &mut line, &mut col, &chars);
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                advance(&mut i, &mut line, &mut col, &chars);
            }
            continue;
        }
        let tok = if c == '"' {
            advance(&mut i, &mut line, &mut col, &chars);
            let mut s = String::new();
            loop {
                let Some(&ch) = chars.get(i) else { return Err(format!("{pos}: unterminated string")) };
                if ch == '\n' {
                    return Err(format!("{pos}: unterminated string"));
                }
                advance(&mut i, &mut line, &mut col, &chars);
                match ch {
                    '"' => break,
                    '\\' => {
                        let Some(&esc) = chars.get(i) else { return Err(format!("{pos}: bad escape")) };
                        advance(&mut i, &mut line, &mut col, &chars);
                        s.push(esc);
                    }
                    _ => s.push(ch),
                }
            }
            Tok::Str(s)
        } else if c == '[' && cell_ref_at(&chars, i).is_some() {
            let (item, column, len) = cell_ref_at(&chars, i).unwrap();
            for _ in 0..len {
                advance(&mut i, &mut line, &mut col, &chars);
            }
            Tok::Cell(item, column)
        } else if c.is_ascii_digit() {
            let mut s = String::new();
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                if chars[i] != '_' {
                    s.push(chars[i]);
                }
                advance(&mut i, &mut line, &mut col, &chars);
            }
            if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                s.push('.');
                advance(&mut i, &mut line, &mut col, &chars);
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                    if chars[i] != '_' {
                        s.push(chars[i]);
                    }
                    advance(&mut i, &mut line, &mut col, &chars);
                }
            }
            if chars.get(i) == Some(&'%') {
                advance(&mut i, &mut line, &mut col, &chars);
                Tok::Percent(s)
            } else {
                Tok::Num(s)
            }
        } else if c.is_alphabetic() || (c == '_' && chars.get(i + 1).is_some_and(|d| d.is_alphanumeric() || *d == '_')) {
            let mut s = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                s.push(chars[i]);
                advance(&mut i, &mut line, &mut col, &chars);
            }
            Tok::Ident(s)
        } else {
            let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
            let Some(sym) = SYMS.iter().find(|s| rest.starts_with(**s)) else {
                return Err(format!("{pos}: unexpected character {c:?}"));
            };
            for _ in 0..sym.chars().count() {
                advance(&mut i, &mut line, &mut col, &chars);
            }
            Tok::Sym(sym)
        };
        out.push(Token { tok, pos, nl_before: nl });
        nl = false;
    }
    out.push(Token { tok: Tok::Eof, pos: Pos { line, col }, nl_before: true });
    Ok(out)
}

/// `[digits(.digits)*:ident]` at `i`, returning (item, column, length).
fn cell_ref_at(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    let mut j = i + 1;
    let mut item = String::new();
    while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
        item.push(chars[j]);
        j += 1;
    }
    if item.is_empty() || !item.starts_with(|c: char| c.is_ascii_digit()) || chars.get(j) != Some(&':') {
        return None;
    }
    j += 1;
    let mut col = String::new();
    while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
        col.push(chars[j]);
        j += 1;
    }
    if col.is_empty() || chars.get(j) != Some(&']') {
        return None;
    }
    Some((item, col, j + 1 - i))
}
