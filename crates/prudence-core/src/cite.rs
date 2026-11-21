//! Structured `@cite`s. A cite string is one or more `;`-separated parts; each
//! part is a reference into APS 210, APG 210 or ARS 210.0, or a free-text
//! note (`sum of items ...`, `memo / of which`). References are expanded to
//! single paragraphs (`paras 62-63` -> 62, 63; `36(c)-(d)` -> 36(c), 36(d)) so
//! every spelling of the same clause compares equal.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Doc {
    /// APS 210 (the prudential standard).
    Aps210,
    /// APG 210 (the guidance).
    Apg210,
    /// ARS 210.0 (the reporting standard: forms and instructions).
    Ars2100,
}

impl Doc {
    pub fn name(self) -> &'static str {
        match self {
            Doc::Aps210 => "APS 210",
            Doc::Apg210 => "APG 210",
            Doc::Ars2100 => "ARS 210.0",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Loc {
    /// Paragraph `num`, optionally sub-paragraph `sub` (`b` in `46(b)`), in
    /// attachment `att` (`None` = the main body).
    Para { att: Option<char>, num: u32, sub: Option<String> },
    /// Table `num` of attachment `att`, optionally one of its footnotes.
    Table { att: Option<char>, num: u32, footnote: Option<u32> },
    /// A page of the ARS 210.0 PDF.
    Page(u32),
    /// An ARF form item (`9.1`, `11.x.1`).
    Item(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ref {
    pub doc: Doc,
    pub loc: Loc,
}

impl Ref {
    /// The paragraph this reference sits in (sub-paragraph dropped).
    pub fn para(&self) -> Option<Ref> {
        match &self.loc {
            Loc::Para { att, num, .. } => Some(Ref { doc: self.doc, loc: Loc::Para { att: *att, num: *num, sub: None } }),
            _ => None,
        }
    }

    /// Element id in the APRA HTML of the document (`Attachment-A-Item-46-b`).
    pub fn html_id(&self) -> Option<String> {
        match &self.loc {
            Loc::Para { att, num, sub } => {
                let mut s = match att {
                    Some(a) => format!("Attachment-{a}-Item-{num}"),
                    None => format!("Item-{num}"),
                };
                if let Some(x) = sub {
                    s.push('-');
                    s.push_str(x);
                }
                Some(s)
            }
            _ => None,
        }
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ", self.doc.name())?;
        match &self.loc {
            Loc::Para { att, num, sub } => {
                if let Some(a) = att {
                    write!(f, "Att {a} ")?;
                }
                write!(f, "para {num}")?;
                if let Some(x) = sub {
                    write!(f, "({x})")?;
                }
                Ok(())
            }
            Loc::Table { att, num, footnote } => {
                if let Some(a) = att {
                    write!(f, "Att {a} ")?;
                }
                write!(f, "Table {num}")?;
                if let Some(n) = footnote {
                    write!(f, " footnote {n}")?;
                }
                Ok(())
            }
            Loc::Page(p) => write!(f, "p.{p}"),
            Loc::Item(i) => write!(f, "item {i}"),
        }
    }
}

/// One `;`-separated part of a cite string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// References, with the text that followed them (`formula`, `: 5% unless
    /// agreed with APRA`, `(all other non-operational)`), if any.
    Refs { text: String, refs: Vec<Ref>, comment: Option<String> },
    /// Not a reference: an explanatory note.
    Note(String),
}

/// Parse a cite string. Never fails: what isn't a reference is a note.
pub fn parse(s: &str) -> Vec<Part> {
    s.split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| match parse_part(p) {
            Some((refs, comment)) => Part::Refs { text: p.to_string(), refs, comment },
            None => Part::Note(p.to_string()),
        })
        .collect()
}

/// All references in a cite string.
pub fn refs(s: &str) -> Vec<Ref> {
    parse(s)
        .into_iter()
        .flat_map(|p| match p {
            Part::Refs { refs, .. } => refs,
            Part::Note(_) => vec![],
        })
        .collect()
}

fn strip_word<'a>(s: &'a str, words: &[&str]) -> Option<&'a str> {
    // Longest first, so `paras` wins over `para`.
    let mut ws: Vec<&&str> = words.iter().collect();
    ws.sort_by_key(|w| std::cmp::Reverse(w.len()));
    ws.into_iter().find_map(|w| s.strip_prefix(*w)).map(str::trim_start)
}

fn parse_part(p: &str) -> Option<(Vec<Ref>, Option<String>)> {
    let (doc, rest) = if let Some(r) = p.strip_prefix("APS 210") {
        (Doc::Aps210, r.trim_start())
    } else if let Some(r) = p.strip_prefix("APG 210") {
        (Doc::Apg210, r.trim_start())
    } else if let Some(r) = p.strip_prefix("ARS 210.0").or_else(|| p.strip_prefix("ARS")) {
        (Doc::Ars2100, r.trim_start())
    } else if p.starts_with("Att ") {
        (Doc::Aps210, p)
    } else {
        return None;
    };
    let (locs, rest) = match doc {
        Doc::Ars2100 => ars_locs(rest)?,
        _ => standard_locs(rest, doc == Doc::Aps210)?,
    };
    let refs: Vec<Ref> = locs.into_iter().map(|loc| Ref { doc, loc }).collect();
    let comment = rest.trim().trim_start_matches(':').trim();
    let comment = comment.strip_prefix('(').and_then(|c| c.strip_suffix(')')).unwrap_or(comment).trim();
    Some((refs, (!comment.is_empty()).then(|| comment.to_string())))
}

/// `Att A paras 6-7`, `Att C Table 1`, `§§41-42`, `para 105`.
fn standard_locs(s: &str, attachments: bool) -> Option<(Vec<Loc>, &str)> {
    let (att, s) = match s.strip_prefix("Att ") {
        Some(r) if attachments => {
            let a = r.chars().next().filter(char::is_ascii_uppercase)?;
            (Some(a), r[1..].trim_start())
        }
        _ => (None, s),
    };
    if let Some(r) = strip_word(s, &["Tables", "Table"]) {
        let (nums, r) = num_list(r)?;
        let (footnote, r) = match strip_word(r.trim_start(), &["footnote"]) {
            Some(f) => {
                let (n, r) = number(f)?;
                (Some(n), r)
            }
            None => (None, r),
        };
        let locs = nums.into_iter().map(|(num, _)| Loc::Table { att, num, footnote }).collect();
        return Some((locs, r));
    }
    let r = strip_word(s, &["paras", "para", "§§", "§"])?;
    let (paras, r) = num_list(r)?;
    Some((paras.into_iter().map(|(num, sub)| Loc::Para { att, num, sub }).collect(), r))
}

/// `p.41 formula`, `pp. 48-50, 57-58`, `item 9.1`, `items 11.x.1, 19.1.x.1`,
/// `Instructions p.51, item 13.3`.
fn ars_locs(s: &str) -> Option<(Vec<Loc>, &str)> {
    let s = strip_word(s, &["Instructions"]).unwrap_or(s);
    if let Some(r) = strip_word(s, &["pp.", "p."]) {
        let (pages, r) = num_list(r)?;
        let mut locs: Vec<Loc> = pages.into_iter().map(|(n, _)| Loc::Page(n)).collect();
        // `p.51, item 13.3`: the item on that page.
        if let Some((items, rest)) = r.trim_start().strip_prefix(',').and_then(|x| item_list(x.trim_start())) {
            locs.extend(items);
            return Some((locs, rest));
        }
        return Some((locs, r));
    }
    item_list(s)
}

fn item_list(s: &str) -> Option<(Vec<Loc>, &str)> {
    let mut r = strip_word(s, &["items", "item"])?;
    let mut items = vec![];
    loop {
        let end = r.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.')).unwrap_or(r.len());
        let id = r[..end].trim_end_matches('.');
        if id.is_empty() || !id.starts_with(|c: char| c.is_ascii_digit()) {
            break;
        }
        items.push(Loc::Item(id.to_string()));
        r = r[id.len()..].trim_start();
        match r.strip_prefix(',') {
            Some(x) => r = x.trim_start(),
            None => break,
        }
    }
    (!items.is_empty()).then_some((items, r))
}

fn number(s: &str) -> Option<(u32, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    Some((s[..end].parse().ok()?, &s[end..]))
}

fn sub(s: &str) -> Option<(String, &str)> {
    let r = s.strip_prefix('(')?;
    let end = r.find(')')?;
    let x = &r[..end];
    (!x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric())).then(|| (x.to_string(), &r[end + 1..]))
}

/// A paragraph number and optional sub-paragraph letter.
type Num = (u32, Option<String>);

/// `62-63, 9(d)-(e), 36(c)-(d)` -> expanded (number, sub-paragraph) list.
fn num_list(s: &str) -> Option<(Vec<Num>, &str)> {
    let mut out = vec![];
    let mut r = s;
    loop {
        let (n, rest) = number(r)?;
        r = rest;
        let first_sub = sub(r).map(|(x, rest)| {
            r = rest;
            x
        });
        if let Some(rest) = r.strip_prefix('-').or_else(|| r.strip_prefix('–')) {
            if let Some((to, rest)) = sub(rest) {
                // 36(c)-(d)
                let from = first_sub.clone()?;
                out.extend(letter_range(&from, &to)?.into_iter().map(|x| (n, Some(x))));
                r = rest;
            } else {
                // 62-63 (a sub-paragraph on either end is dropped: the range is whole paragraphs)
                let (m, rest) = number(rest)?;
                if m < n || m - n > 200 {
                    return None;
                }
                out.extend((n..=m).map(|k| (k, None)));
                r = rest;
                if let Some((_, rest)) = sub(r) {
                    r = rest;
                }
            }
        } else {
            out.push((n, first_sub));
        }
        let t = r.trim_start();
        let next = t.strip_prefix(',').or_else(|| t.strip_prefix("and ")).map(str::trim_start);
        match next {
            Some(x) if x.starts_with(|c: char| c.is_ascii_digit()) => r = x,
            _ => return Some((out, r)),
        }
    }
}

fn letter_range(from: &str, to: &str) -> Option<Vec<String>> {
    let (a, b) = (single(from)?, single(to)?);
    (a <= b && a.is_ascii_lowercase() && b.is_ascii_lowercase()).then(|| (a..=b).map(String::from).collect())
}

fn single(s: &str) -> Option<char> {
    let mut c = s.chars();
    let x = c.next()?;
    c.next().is_none().then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(s: &str) -> Vec<String> {
        refs(s).iter().map(|r| r.to_string()).collect()
    }

    #[test]
    fn spellings_of_one_paragraph_agree() {
        for s in ["Att A §46(b)", "APS 210 Att A para 46(b)", "Att A paras 46(b)"] {
            assert_eq!(canon(s), ["APS 210 Att A para 46(b)"], "{s}");
        }
    }

    #[test]
    fn ranges_expand() {
        assert_eq!(canon("APS 210 Att A paras 62-63"), ["APS 210 Att A para 62", "APS 210 Att A para 63"]);
        assert_eq!(canon("Att A §§41-42"), ["APS 210 Att A para 41", "APS 210 Att A para 42"]);
        assert_eq!(canon("APS 210 Att C para 36(c)-(d); APS 210 Att C para 23").len(), 3);
        assert_eq!(canon("Att A para 9(d)-(e)"), ["APS 210 Att A para 9(d)", "APS 210 Att A para 9(e)"]);
        assert_eq!(canon("Att A paras 10(b), 12(a)"), ["APS 210 Att A para 10(b)", "APS 210 Att A para 12(a)"]);
        assert_eq!(canon("Att A paras 54, 60").len(), 2);
        assert_eq!(canon("ARS 210.0 pp. 48-50, 57-58"), ["ARS 210.0 p.48", "ARS 210.0 p.49", "ARS 210.0 p.50", "ARS 210.0 p.57", "ARS 210.0 p.58"]);
    }

    #[test]
    fn documents_and_tables() {
        assert_eq!(canon("APG 210 §109"), ["APG 210 para 109"]);
        assert_eq!(canon("Att A para 65; APG 210 para 105"), ["APS 210 Att A para 65", "APG 210 para 105"]);
        assert_eq!(canon("Att A Table 3 footnote 5; Att C paras 9, 27"), ["APS 210 Att A Table 3 footnote 5", "APS 210 Att C para 9", "APS 210 Att C para 27"]);
        assert_eq!(canon("Att A Tables 1-2"), ["APS 210 Att A Table 1", "APS 210 Att A Table 2"]);
        assert_eq!(canon("Att A §46; ARS item 9.1"), ["APS 210 Att A para 46", "ARS 210.0 item 9.1"]);
        assert_eq!(canon("Att A para 8; ARS items 11.x.1, 19.1.x.1"), ["APS 210 Att A para 8", "ARS 210.0 item 11.x.1", "ARS 210.0 item 19.1.x.1"]);
        assert_eq!(canon("ARS Instructions p.51, item 13.3"), ["ARS 210.0 p.51", "ARS 210.0 item 13.3"]);
        assert_eq!(canon("ARS Instructions pp. 55-56, 61-62").len(), 4);
        // Other standards are notes for now.
        assert_eq!(parse("APS 112 Attachment B"), [Part::Note("APS 112 Attachment B".into())]);
    }

    #[test]
    fn comments_and_notes() {
        let p = parse("ARS p.61: min(facility, 20% of total outflows); APS 210 Att A para 61");
        assert!(matches!(&p[0], Part::Refs { comment: Some(c), .. } if c == "min(facility, 20% of total outflows)"));
        let p = parse("APS 210 Att A Table 4 (collateral covers short positions)");
        assert!(matches!(&p[0], Part::Refs { comment: Some(c), .. } if c == "collateral covers short positions"));
        let p = parse("ARS p.41 formula");
        assert!(matches!(&p[0], Part::Refs { comment: Some(c), .. } if c == "formula"));
        assert_eq!(parse("sum of items 1.1, 1.2"), [Part::Note("sum of items 1.1, 1.2".into())]);
        assert_eq!(parse("memo / of which"), [Part::Note("memo / of which".into())]);
        let p = parse("APS 210 Att A Table 3; SME < AUD 2m");
        assert!(matches!(&p[1], Part::Note(n) if n == "SME < AUD 2m"));
    }

    #[test]
    fn html_ids() {
        assert_eq!(refs("Att A §46(b)")[0].html_id().unwrap(), "Attachment-A-Item-46-b");
        assert_eq!(refs("APG 210 §109")[0].html_id().unwrap(), "Item-109");
    }
}
