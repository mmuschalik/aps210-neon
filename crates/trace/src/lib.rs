//! Clause traceability: every `@cite` in the specs, joined to the paragraphs
//! of APS 210 / APG 210 it refers to.
//!
//! `cargo run -p trace` writes `target/trace/index.html` (the clause browser)
//! and prints the checks: citations of paragraphs that don't exist, ARS items
//! that no form has, and notes written as `@cite`s.

pub mod html;
pub mod sites;
pub mod sources;

use prudence_core::cite::{Doc, Loc, Part, Ref};
use serde_json::{Value, json};
use sites::{FormItems, ParseFailure, Site};
use sources::Source;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub struct Report {
    pub sites: Vec<Site>,
    pub failures: Vec<ParseFailure>,
    pub sources: Vec<Source>,
    /// Canonical ref -> indices into `sites` that cite it directly.
    pub cited: BTreeMap<Ref, Vec<usize>>,
    pub findings: Vec<Finding>,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub kind: FindingKind,
    pub site: usize,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingKind {
    /// Cites a paragraph, sub-paragraph or table the document doesn't have.
    Dangling,
    /// Cites an ARS item that no form spec defines.
    UnknownItem,
    /// A part of the `@cite` that is not a reference (`sum of items ...`).
    Note,
}

impl FindingKind {
    pub fn name(self) -> &'static str {
        match self {
            FindingKind::Dangling => "dangling",
            FindingKind::UnknownItem => "unknown-item",
            FindingKind::Note => "note",
        }
    }
}

pub fn build(root: &Path) -> Result<Report, String> {
    let (sites, failures, forms) = sites::collect(root);
    let sources = vec![
        Source::load(Doc::Aps210, &root.join("aps-210.html"))?,
        Source::load(Doc::Apg210, &root.join("apg-210.html"))?,
    ];
    let known: BTreeSet<Ref> = sources.iter().flat_map(|s| s.entries().map(|e| e.r.clone())).collect();
    let mut cited: BTreeMap<Ref, Vec<usize>> = BTreeMap::new();
    let mut findings = vec![];
    for (i, s) in sites.iter().enumerate() {
        for p in &s.parts {
            let refs = match p {
                Part::Note(n) => {
                    findings.push(Finding { kind: FindingKind::Note, site: i, detail: n.clone() });
                    continue;
                }
                Part::Refs { refs, .. } => refs,
            };
            for r in refs {
                let key = match &r.loc {
                    // A table footnote is cited as part of its table.
                    Loc::Table { att, num, footnote: Some(_) } => Ref { doc: r.doc, loc: Loc::Table { att: *att, num: *num, footnote: None } },
                    _ => r.clone(),
                };
                match (&key.doc, &key.loc) {
                    (Doc::Ars2100, Loc::Item(id)) if !item_known(&forms, id) => {
                        findings.push(Finding { kind: FindingKind::UnknownItem, site: i, detail: r.to_string() })
                    }
                    (Doc::Aps210 | Doc::Apg210, _) if !known.contains(&key) => {
                        findings.push(Finding { kind: FindingKind::Dangling, site: i, detail: r.to_string() })
                    }
                    _ => {}
                }
                let v = cited.entry(key).or_default();
                if v.last() != Some(&i) {
                    v.push(i);
                }
            }
        }
    }
    findings.sort_by_key(|f| (f.kind, f.site));
    Ok(Report { sites, failures, sources, cited, findings })
}

/// `9.1` or a pattern with `x` wildcards (`11.x.1`) against every form's items.
fn item_known(forms: &FormItems, id: &str) -> bool {
    let pat: Vec<&str> = id.split('.').collect();
    forms.iter().flat_map(|(_, items)| items).any(|it| {
        let parts: Vec<&str> = it.split('.').collect();
        parts.len() == pat.len() && parts.iter().zip(&pat).all(|(a, b)| *b == "x" || a == b)
    })
}

impl Report {
    /// Sites citing this entry: the reference itself, and for a whole
    /// paragraph also its sub-paragraphs.
    pub fn sites_for(&self, r: &Ref) -> Vec<usize> {
        let mut out: Vec<usize> = self.cited.get(r).cloned().unwrap_or_default();
        if let Loc::Para { sub: None, .. } = r.loc {
            for (k, v) in self.cited.range(r.clone()..) {
                if k.para().as_ref() != Some(r) {
                    break;
                }
                out.extend(v);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// (cited, total) numbered paragraphs of a document: a paragraph counts
    /// as cited when it or one of its sub-paragraphs is.
    pub fn coverage(&self, doc: Doc) -> (usize, usize) {
        let paras: Vec<&Ref> = self
            .sources
            .iter()
            .filter(|s| s.doc == doc)
            .flat_map(|s| s.entries())
            .map(|e| &e.r)
            .filter(|r| matches!(r.loc, Loc::Para { sub: None, .. }))
            .collect();
        (paras.iter().filter(|r| !self.sites_for(r).is_empty()).count(), paras.len())
    }

    pub fn to_json(&self) -> Value {
        let sites: Vec<Value> = self
            .sites
            .iter()
            .map(|s| {
                let parts: Vec<Value> = s
                    .parts
                    .iter()
                    .map(|p| match p {
                        Part::Refs { text, refs, comment } => json!({
                            "text": text,
                            "refs": refs.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
                            "comment": comment,
                        }),
                        Part::Note(n) => json!({ "note": n }),
                    })
                    .collect();
                json!({
                    "file": s.file, "line": s.line, "kind": s.kind, "context": s.context,
                    "text": s.text, "source": s.source, "parts": parts,
                })
            })
            .collect();
        let docs: Vec<Value> = self
            .sources
            .iter()
            .map(|src| {
                let (c, t) = self.coverage(src.doc);
                let sections: Vec<Value> = src
                    .sections
                    .iter()
                    .map(|sec| {
                        let entries: Vec<Value> = sec
                            .entries
                            .iter()
                            .map(|e| {
                                let kind = match e.r.loc {
                                    Loc::Para { sub: None, .. } => "para",
                                    Loc::Para { .. } => "sub",
                                    _ => "table",
                                };
                                json!({
                                    "key": e.r.to_string(), "kind": kind, "label": e.label, "text": e.text,
                                    "html_id": e.html_id, "sites": self.sites_for(&e.r),
                                    "direct": self.cited.get(&e.r).cloned().unwrap_or_default(),
                                })
                            })
                            .collect();
                        json!({ "part": sec.part, "title": sec.title, "entries": entries })
                    })
                    .collect();
                json!({ "name": src.doc.name(), "url": src.url, "cited": c, "total": t, "sections": sections })
            })
            .collect();
        // ARS 210.0 is cited by page and item; its text isn't indexed.
        let ars: Vec<Value> = self
            .cited
            .iter()
            .filter(|(r, _)| r.doc == Doc::Ars2100)
            .map(|(r, v)| json!({ "key": r.to_string(), "sites": v }))
            .collect();
        let findings: Vec<Value> = self
            .findings
            .iter()
            .map(|f| json!({ "kind": f.kind.name(), "site": f.site, "detail": f.detail }))
            .collect();
        let failures: Vec<Value> = self.failures.iter().map(|f| json!({ "file": f.file, "error": f.error })).collect();
        json!({ "docs": docs, "ars": ars, "sites": sites, "findings": findings, "failures": failures })
    }
}
