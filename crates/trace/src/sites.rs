//! Every `@cite` in the Lean-exported Prudence IR specs (`specs/ir/**/*.json`).

use prudence_core::cite::{self, Part};
use prudence_core::syntax::ast::{self, Annotation, DeclKind, Expr, TopLevel};
use prudence_core::syntax::load_file_json;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Site {
    /// Spec file, relative to the repository root.
    pub file: String,
    pub line: usize,
    /// What carries the cite: `form item`, `check`, `theorem`, `rule`, `param`, `field`, ...
    pub kind: &'static str,
    /// Short name: `ARF 210.1A item 1.5`, `fn class → Sme`, `param sme_threshold`, `Customer.sme_group`.
    pub title: String,
    /// In words, where the spec has them: the item label, the check or theorem description.
    pub detail: String,
    /// The module or form it belongs to.
    pub owner: String,
    /// The cite string as written.
    pub text: String,
    pub parts: Vec<Part>,
    /// The spec source line the cite is on, trimmed.
    pub source: String,
}

/// A spec that failed to parse; its cites are missing from the report.
#[derive(Clone, Debug)]
pub struct ParseFailure {
    pub file: String,
    pub error: String,
}

/// A `theorem` in a spec, cited or not.
#[derive(Clone, Debug)]
pub struct Theorem {
    pub name: String,
    /// The form theorem's description; empty for module theorems.
    pub statement: String,
    /// The `//` comment directly above it, if any.
    pub note: String,
    pub owner: String,
    pub file: String,
    pub line: usize,
    /// How it is proven (see README, "How it was verified").
    pub prover: &'static str,
}

/// Everything read from the specs.
pub struct Collected {
    pub sites: Vec<Site>,
    pub failures: Vec<ParseFailure>,
    pub forms: FormItems,
    pub theorems: Vec<Theorem>,
}

/// Form item ids per form code (`210.1A` -> `["1", "1.1", ...]`), for checking `ARS item` cites.
pub type FormItems = Vec<(String, Vec<String>)>;

pub fn spec_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "json") {
                out.push(p);
            }
        }
    }
    let mut out = vec![];
    walk(&root.join("specs/ir"), &mut out);
    out
}

pub fn collect(root: &Path) -> Collected {
    let (mut sites, mut failures, mut forms, mut theorems) = (vec![], vec![], vec![], vec![]);
    for path in spec_files(root) {
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(ParseFailure { file: rel, error: e.to_string() });
                continue;
            }
        };
        match load_file_json(&src) {
            Ok(file) => {
                let lines: Vec<&str> = src.lines().collect();
                let mut c = Collector { file: &rel, lines: &lines, out: &mut sites, owner: String::new() };
                for item in &file.items {
                    match item {
                        TopLevel::Form(f) => {
                            c.form(f);
                            for t in &f.theorems {
                                // Form theorems carry no position; find the declaration.
                                let decl = format!("theorem \"{}\"", t.id);
                                let line = lines.iter().position(|l| l.trim_start().starts_with(&decl)).map_or(0, |i| i + 1);
                                theorems.push(Theorem {
                                    name: t.id.clone(),
                                    statement: t.description.clone(),
                                    note: comment_above(&lines, line),
                                    owner: format!("{} {}", f.code, f.title),
                                    file: rel.clone(),
                                    line,
                                    prover: "Z3",
                                });
                            }
                            forms.push((f.code.trim_start_matches("ARF ").to_string(), f.items.iter().map(|i| i.id.clone()).collect()));
                        }
                        TopLevel::Module(m) => {
                            c.module(m);
                            for d in m.decls.iter().filter(|d| matches!(d.kind, DeclKind::Theorem(_))) {
                                theorems.push(Theorem {
                                    name: d.name.clone(),
                                    statement: String::new(),
                                    note: comment_above(&lines, d.pos.line),
                                    owner: m.name.clone(),
                                    file: rel.clone(),
                                    line: d.pos.line,
                                    // Module theorems are proven in Lean, about the model generated from the spec.
                                    prover: "Lean",
                                });
                            }
                        }
                    }
                }
            }
            Err(error) => failures.push(ParseFailure { file: rel, error }),
        }
    }
    Collected { sites, failures, forms, theorems }
}

struct Collector<'a> {
    file: &'a str,
    lines: &'a [&'a str],
    out: &'a mut Vec<Site>,
    owner: String,
}

impl Collector<'_> {
    fn push(&mut self, anns: &[Annotation], kind: &'static str, title: &str, detail: &str) {
        for a in anns.iter().filter(|a| a.name == "cite") {
            let Some(Expr::Str(text, _)) = a.arg(None) else { continue };
            let line = a.pos.line;
            let source = self.source(line);
            self.out.push(Site {
                file: self.file.to_string(),
                line,
                kind,
                title: title.to_string(),
                detail: detail.to_string(),
                owner: self.owner.clone(),
                text: text.clone(),
                parts: cite::parse(text),
                source,
            });
        }
    }

    /// The cite's line; when that line only closes a block (`} @cite(..)`) or
    /// holds nothing but the cite, also the lines above it, up to 3.
    fn source(&self, line: usize) -> String {
        let at = |n: usize| n.checked_sub(1).and_then(|i| self.lines.get(i)).copied().unwrap_or("");
        let mut first = line;
        while first > 1 && line - first < 3 {
            let l = at(first).trim_start();
            if !(l.starts_with('}') || l.starts_with('@')) {
                break;
            }
            first -= 1;
        }
        let lines: Vec<&str> = (first..=line).map(at).collect();
        let indent = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
        lines.iter().map(|l| l.get(indent..).unwrap_or(l.trim()).trim_end()).collect::<Vec<_>>().join("\n")
    }

    fn form(&mut self, f: &ast::FormDecl) {
        let code = &f.code;
        self.owner = format!("{code} {}", f.title);
        self.push(&f.anns, "form", code, &f.title);
        for it in &f.items {
            self.push(&it.anns, "form item", &format!("{code} item {}", it.id), &it.label);
        }
        for c in &f.checks {
            let t = format!("check {}", c.id);
            self.push(&c.anns, "check", &t, &c.description);
            self.expr(&c.cond, "check", &t, &c.description);
        }
        for t in &f.theorems {
            let n = format!("theorem {}", t.id);
            self.push(&t.anns, "theorem", &n, &t.description);
            self.expr(&t.cond, "theorem", &n, &t.description);
        }
    }

    fn module(&mut self, m: &ast::Module) {
        self.owner = m.name.clone();
        self.push(&m.anns, "module", &m.name, "");
        for d in &m.decls {
            let (kind, word) = match &d.kind {
                DeclKind::Param { .. } => ("param", "param"),
                DeclKind::TypeAlias(_) => ("type", "type"),
                DeclKind::Data { .. } => ("type", "data"),
                DeclKind::Event(_) => ("type", "event"),
                DeclKind::View(_) => ("rule", "view"),
                DeclKind::Fn { .. } => ("rule", "fn"),
                DeclKind::Alloc { .. } => ("rule", "alloc"),
                DeclKind::Partition { .. } => ("rule", "partition"),
                DeclKind::Invariant(_) => ("invariant", "invariant"),
                DeclKind::Theorem(_) => ("theorem", "theorem"),
            };
            // A partition's name is its loop variable (`d`); name it by what it splits.
            let name = match &d.kind {
                DeclKind::Partition { source: ast::Type::Named { name, .. }, .. } => name.clone(),
                _ => d.name.clone(),
            };
            let t = format!("{word} {name}");
            self.push(&d.anns, kind, &t, "");
            match &d.kind {
                DeclKind::Param { default, .. } => {
                    if let Some(e) = default {
                        self.expr(e, kind, &t, "");
                    }
                }
                DeclKind::Data { fields, .. } | DeclKind::Event(fields) => {
                    for f in fields {
                        self.push(&f.anns, "field", &format!("{}.{}", d.name, f.name), "");
                    }
                }
                DeclKind::Fn { params, body, .. } => {
                    for f in params {
                        self.push(&f.anns, "field", &format!("{t}({})", f.name), "");
                    }
                    if let Some(b) = body {
                        self.expr(b, kind, &t, "");
                    }
                }
                DeclKind::View(e) | DeclKind::Invariant(e) | DeclKind::Theorem(e) => self.expr(e, kind, &t, ""),
                DeclKind::Alloc { body, .. } => self.expr(body, kind, &t, ""),
                DeclKind::Partition { split, split_by, more_splits, cases, .. } => {
                    self.expr(split, kind, &t, "");
                    self.expr(split_by, kind, &t, "");
                    for (_, e) in more_splits {
                        self.expr(e, kind, &t, "");
                    }
                    for c in cases {
                        self.expr(&c.cond, kind, &t, "");
                        for (_, e) in &c.lets {
                            self.expr(e, kind, &t, "");
                        }
                        for (name, e) in &c.outputs {
                            self.expr(e, kind, &format!("{t} · {name}"), "");
                        }
                    }
                }
                DeclKind::TypeAlias(_) => {}
            }
        }
    }

    fn expr(&mut self, e: &Expr, kind: &'static str, t: &str, d: &str) {
        let go = |s: &mut Self, x: &Expr| s.expr(x, kind, t, d);
        match e {
            Expr::Num(..) | Expr::Percent(..) | Expr::Str(..) | Expr::Ident(..) | Expr::Cell(..) => {}
            Expr::List(xs) | Expr::Tuple(xs) => xs.iter().for_each(|x| go(self, x)),
            Expr::Unary(_, a) | Expr::Member(a, _) | Expr::Is(a, _) => go(self, a),
            Expr::Binary(_, a, b) | Expr::Index(a, b) => {
                go(self, a);
                go(self, b);
            }
            Expr::Call(f, args) => {
                go(self, f);
                args.iter().for_each(|a| go(self, &a.value));
            }
            Expr::If { cond, then, other } => {
                go(self, cond);
                go(self, then);
                go(self, other);
            }
            Expr::IfLet { value, then, other, .. } => {
                go(self, value);
                go(self, then);
                go(self, other);
            }
            Expr::Match { scrutinee, arms } => {
                go(self, scrutinee);
                for a in arms {
                    // Name the branch: `fn class → Sme`.
                    let arm = format!("{t} → {}", pat(&a.pat));
                    if let Some(g) = &a.guard {
                        self.expr(g, kind, &arm, d);
                    }
                    self.expr(&a.body, kind, &arm, d);
                }
            }
            Expr::Let { value, body, .. } => {
                go(self, value);
                go(self, body);
            }
            Expr::Comprehension { source, filter, body, .. } => {
                go(self, source);
                filter.iter().for_each(|f| go(self, f));
                body.iter().for_each(|b| go(self, b));
            }
            Expr::Group { source, key, filter, fields } => {
                go(self, source);
                go(self, key);
                filter.iter().for_each(|f| go(self, f));
                fields.iter().for_each(|(_, e)| go(self, e));
            }
            Expr::Optimize { filter, objective, constraints, .. } => {
                filter.iter().for_each(|f| go(self, f));
                go(self, objective);
                constraints.iter().for_each(|c| go(self, c));
            }
            Expr::Forall { domain, body, .. } => {
                if let ast::Domain::In(s) = domain {
                    go(self, s);
                }
                go(self, body);
            }
            Expr::Annotated(inner, anns) => {
                self.push(anns, kind, t, d);
                go(self, inner);
            }
        }
    }
}

/// The `//` comment lines directly above `line` (1-based), joined.
fn comment_above(lines: &[&str], line: usize) -> String {
    let mut out = vec![];
    let mut i = line.saturating_sub(1);
    while i > 0 {
        let Some(c) = lines.get(i - 1).and_then(|l| l.trim().strip_prefix("//")) else { break };
        out.push(c.trim());
        i -= 1;
    }
    out.reverse();
    out.join(" ")
}

fn pat(p: &ast::Pat) -> String {
    match p {
        ast::Pat::Wild => "otherwise".into(),
        ast::Pat::Bind(n) => n.clone(),
        ast::Pat::Ctor(n, _) => n.clone(),
        ast::Pat::Tuple(ps) => format!("({})", ps.iter().map(pat).collect::<Vec<_>>().join(", ")),
    }
}
