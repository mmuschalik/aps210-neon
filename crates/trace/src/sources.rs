//! The text of APS 210 and APG 210, indexed by paragraph, from the APRA HTML
//! saved in the repository (`aps-210.html`, `apg-210.html`).
//!
//! The pages mark each numbered paragraph as
//! `<div class="list-item-N" data-item-number="46." id="Attachment-A-Item-46">text</div>`
//! and tables as `<h4>Table 3 – Cash outflow categories</h4>`; a scan in
//! document order is enough, no HTML parser needed.

use prudence_core::cite::{Doc, Loc, Ref};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Entry {
    pub r: Ref,
    /// As printed: `46.`, `(b)`, `Table 3`.
    pub label: String,
    pub text: String,
    /// The `<h4>` heading it sits under (`Retail deposit outflows`), if any.
    pub heading: String,
    /// Element id in the APRA page (for a link to apra.gov.au).
    pub html_id: String,
}

#[derive(Clone, Debug)]
pub struct Section {
    /// `Attachment A - Liquidity coverage ratio`, `Chapter 2 - Liquidity coverage ratio`.
    pub part: String,
    /// The `<h3>` under it, if any.
    pub title: String,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug)]
pub struct Source {
    pub doc: Doc,
    pub url: String,
    pub sections: Vec<Section>,
}

impl Source {
    pub fn load(doc: Doc, path: &Path) -> Result<Source, String> {
        let html = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let url = between(&html, "<link rel=\"canonical\" href=\"", "\"").unwrap_or_default().to_string();
        // The body proper: skip site navigation before the first paragraph's fragment.
        let mut sections: Vec<Section> = vec![];
        let (mut part, mut title, mut heading) = (String::new(), String::new(), String::new());
        let mut att: Option<char> = None;
        let mut i = 0;
        while let Some(off) = html[i..].find('<') {
            let at = i + off;
            let rest = &html[at..];
            i = at + 1;
            let tag = rest.get(1..3).unwrap_or("");
            if tag == "h2" || tag == "h3" || tag == "h4" {
                let open_tag = &rest[..rest.find('>').unwrap_or(0)];
                let Some(text) = element_text(rest) else { continue };
                if text.is_empty() || open_tag.contains("visually-hidden") {
                    continue;
                }
                match tag {
                    "h2" => {
                        // Tables and paragraphs below belong to this attachment (None: main body or a chapter).
                        att = between(open_tag, "id=\"Attachment-", "\"").and_then(|a| a.chars().next());
                        part = text;
                        title.clear();
                        heading.clear();
                    }
                    "h3" => {
                        title = text;
                        heading.clear();
                    }
                    _ => {
                        if let Some(r) = text.strip_prefix("Table ") {
                            let num: String = r.chars().take_while(char::is_ascii_digit).collect();
                            if let Ok(num) = num.parse() {
                                let frag = last_between(&html[..at], "<div class=\"fragment\" id=\"", "\"").unwrap_or_default();
                                let caption = r[r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len())..]
                                    .trim_start_matches([' ', '–', '-'])
                                    .to_string();
                                push(&mut sections, &part, &title, Entry {
                                    r: Ref { doc, loc: Loc::Table { att, num, footnote: None } },
                                    label: format!("Table {num}"),
                                    text: caption,
                                    heading: heading.clone(),
                                    html_id: frag.to_string(),
                                });
                            }
                        } else {
                            heading = text;
                        }
                    }
                }
                continue;
            }
            if !rest.starts_with("<div class=\"list-item-") {
                continue;
            }
            let (Some(label), Some(id)) = (between(rest, "data-item-number=\"", "\""), between(rest, " id=\"", "\"")) else { continue };
            let Some(r) = parse_id(doc, id) else { continue };
            let Some(text) = element_text(rest) else { continue };
            push(&mut sections, &part, &title, Entry { r, label: label.to_string(), text, heading: heading.clone(), html_id: id.to_string() });
        }
        Ok(Source { doc, url, sections })
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.sections.iter().flat_map(|s| s.entries.iter())
    }
}

fn push(sections: &mut Vec<Section>, part: &str, title: &str, e: Entry) {
    match sections.last_mut() {
        Some(s) if s.part == part && s.title == title => s.entries.push(e),
        _ => sections.push(Section { part: part.to_string(), title: title.to_string(), entries: vec![e] }),
    }
}

/// `Attachment-A-Item-46-b` / `Item-109` -> the reference.
fn parse_id(doc: Doc, id: &str) -> Option<Ref> {
    let (att, rest) = match id.strip_prefix("Attachment-") {
        Some(r) => {
            let a = r.chars().next()?;
            (Some(a), r.get(1..)?.strip_prefix("-Item-")?)
        }
        None => (None, id.strip_prefix("Item-")?),
    };
    let mut it = rest.splitn(2, '-');
    let num = it.next()?.parse().ok()?;
    let sub = it.next().map(str::to_string);
    // Only one level of sub-paragraph is cited; deeper levels (`46-b-i`) fold into their parent's text.
    if sub.as_deref().is_some_and(|s| s.contains('-')) {
        return None;
    }
    Some(Ref { doc, loc: Loc::Para { att, num, sub } })
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let a = s.find(start)? + start.len();
    let b = s[a..].find(end)? + a;
    Some(&s[a..b])
}

fn last_between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let a = s.rfind(start)? + start.len();
    let b = s[a..].find(end)? + a;
    Some(&s[a..b])
}

/// Text of the element starting at `s` (up to its closing tag, nested
/// inline tags stripped, entities decoded, whitespace collapsed).
fn element_text(s: &str) -> Option<String> {
    let name: String = s[1..].chars().take_while(char::is_ascii_alphanumeric).collect();
    let open_end = s.find('>')? + 1;
    let close = format!("</{name}>");
    // Paragraph divs hold only inline markup; headings hold none.
    let end = s[open_end..].find(&close)? + open_end;
    let inner = &s[open_end..end];
    let mut out = String::new();
    let mut in_tag = false;
    for c in inner.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = decode(&out);
    Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest.find(';').filter(|&j| j < 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert_eq!(parse_id(Doc::Aps210, "Attachment-A-Item-46-b").unwrap().to_string(), "APS 210 Att A para 46(b)");
        assert_eq!(parse_id(Doc::Apg210, "Item-109").unwrap().to_string(), "APG 210 para 109");
        assert!(parse_id(Doc::Aps210, "Attachment-A-Item-46-b-i").is_none());
    }

    #[test]
    fn text() {
        assert_eq!(element_text("<div a=\"1\">x <a href=\"#\">para&nbsp;37</a>  &amp; y</div>rest").unwrap(), "x para 37 & y");
    }
}
