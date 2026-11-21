//! Every `@cite` in the `.prd` specs, with where it sits.

use prudence_core::cite::{self, Part};
use prudence_core::syntax::ast::{self, Annotation, DeclKind, Expr, TopLevel};
use prudence_core::syntax::parser;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Site {
    /// Spec file, relative to the repository root.
    pub file: String,
    pub line: usize,
    /// What carries the cite: `form item`, `check`, `theorem`, `rule`, `param`, `field`, ...
    pub kind: &'static str,
    /// Where, for a reader: `ARF 210.1A item 1.5 Adjusted amount of HQLA1 stock`, `RetailAndSmeOutflows · fn class`.
    pub context: String,
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
            } else if p.extension().is_some_and(|e| e == "prd") {
                out.push(p);
            }
        }
    }
    let mut out = vec![];
    walk(&root.join("specs"), &mut out);
    out
}

pub fn collect(root: &Path) -> (Vec<Site>, Vec<ParseFailure>, FormItems) {
    let (mut sites, mut failures, mut forms) = (vec![], vec![], vec![]);
    for path in spec_files(root) {
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(ParseFailure { file: rel, error: e.to_string() });
                continue;
            }
        };
        match parser::parse_file(&src) {
            Ok(file) => {
                let lines: Vec<&str> = src.lines().collect();
                let mut c = Collector { file: &rel, lines: &lines, out: &mut sites };
                for item in &file.items {
                    match item {
                        TopLevel::Form(f) => {
                            c.form(f);
                            forms.push((f.code.trim_start_matches("ARF ").to_string(), f.items.iter().map(|i| i.id.clone()).collect()));
                        }
                        TopLevel::Module(m) => c.module(m),
                    }
                }
            }
            Err(error) => failures.push(ParseFailure { file: rel, error }),
        }
    }
    (sites, failures, forms)
}

struct Collector<'a> {
    file: &'a str,
    lines: &'a [&'a str],
    out: &'a mut Vec<Site>,
}

impl Collector<'_> {
    fn push(&mut self, anns: &[Annotation], kind: &'static str, context: &str) {
        for a in anns.iter().filter(|a| a.name == "cite") {
            let Some(Expr::Str(text, _)) = a.arg(None) else { continue };
            let line = a.pos.line;
            let source = self.source(line);
            self.out.push(Site {
                file: self.file.to_string(),
                line,
                kind,
                context: context.to_string(),
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
        self.push(&f.anns, "form", code);
        for it in &f.items {
            self.push(&it.anns, "form item", &format!("{code} item {} {}", it.id, it.label));
        }
        for c in &f.checks {
            self.push(&c.anns, "check", &format!("{code} check {}: {}", c.id, c.description));
            self.expr(&c.cond, "check", &format!("{code} check {}", c.id));
        }
        for t in &f.theorems {
            self.push(&t.anns, "theorem", &format!("{code} theorem {}: {}", t.id, t.description));
            self.expr(&t.cond, "theorem", &format!("{code} theorem {}", t.id));
        }
    }

    fn module(&mut self, m: &ast::Module) {
        self.push(&m.anns, "module", &m.name);
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
            let ctx = format!("{} · {word} {}", m.name, d.name);
            self.push(&d.anns, kind, &ctx);
            match &d.kind {
                DeclKind::Param { default, .. } => {
                    if let Some(e) = default {
                        self.expr(e, kind, &ctx);
                    }
                }
                DeclKind::Data { fields, .. } | DeclKind::Event(fields) => {
                    for f in fields {
                        self.push(&f.anns, "field", &format!("{} · {word} {}.{}", m.name, d.name, f.name));
                    }
                }
                DeclKind::Fn { params, body, .. } => {
                    for f in params {
                        self.push(&f.anns, "field", &format!("{ctx}({})", f.name));
                    }
                    if let Some(b) = body {
                        self.expr(b, kind, &ctx);
                    }
                }
                DeclKind::View(e) | DeclKind::Invariant(e) | DeclKind::Theorem(e) => self.expr(e, kind, &ctx),
                DeclKind::Alloc { body, .. } => self.expr(body, kind, &ctx),
                DeclKind::Partition { split, split_by, more_splits, cases, .. } => {
                    self.expr(split, kind, &ctx);
                    self.expr(split_by, kind, &ctx);
                    for (_, e) in more_splits {
                        self.expr(e, kind, &ctx);
                    }
                    for c in cases {
                        self.expr(&c.cond, kind, &ctx);
                        for (_, e) in &c.lets {
                            self.expr(e, kind, &ctx);
                        }
                        for (_, e) in &c.outputs {
                            self.expr(e, kind, &ctx);
                        }
                    }
                }
                DeclKind::TypeAlias(_) => {}
            }
        }
    }

    fn args(&mut self, args: &[ast::Arg], kind: &'static str, ctx: &str) {
        for a in args {
            self.expr(&a.value, kind, ctx);
        }
    }

    fn expr(&mut self, e: &Expr, kind: &'static str, ctx: &str) {
        match e {
            Expr::Num(..) | Expr::Percent(..) | Expr::Str(..) | Expr::Ident(..) | Expr::Cell(..) => {}
            Expr::List(xs) | Expr::Tuple(xs) => xs.iter().for_each(|x| self.expr(x, kind, ctx)),
            Expr::Unary(_, a) | Expr::Member(a, _) | Expr::Is(a, _) => self.expr(a, kind, ctx),
            Expr::Binary(_, a, b) | Expr::Index(a, b) => {
                self.expr(a, kind, ctx);
                self.expr(b, kind, ctx);
            }
            Expr::Call(f, args) => {
                self.expr(f, kind, ctx);
                self.args(args, kind, ctx);
            }
            Expr::If { cond, then, other } => {
                self.expr(cond, kind, ctx);
                self.expr(then, kind, ctx);
                self.expr(other, kind, ctx);
            }
            Expr::IfLet { value, then, other, .. } => {
                self.expr(value, kind, ctx);
                self.expr(then, kind, ctx);
                self.expr(other, kind, ctx);
            }
            Expr::Match { scrutinee, arms } => {
                self.expr(scrutinee, kind, ctx);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.expr(g, kind, ctx);
                    }
                    self.expr(&a.body, kind, ctx);
                }
            }
            Expr::Let { value, body, .. } => {
                self.expr(value, kind, ctx);
                self.expr(body, kind, ctx);
            }
            Expr::Comprehension { source, filter, body, .. } => {
                self.expr(source, kind, ctx);
                if let Some(f) = filter {
                    self.expr(f, kind, ctx);
                }
                if let Some(b) = body {
                    self.expr(b, kind, ctx);
                }
            }
            Expr::Group { source, key, filter, fields } => {
                self.expr(source, kind, ctx);
                self.expr(key, kind, ctx);
                if let Some(f) = filter {
                    self.expr(f, kind, ctx);
                }
                for (_, e) in fields {
                    self.expr(e, kind, ctx);
                }
            }
            Expr::Optimize { filter, objective, constraints, .. } => {
                if let Some(f) = filter {
                    self.expr(f, kind, ctx);
                }
                self.expr(objective, kind, ctx);
                constraints.iter().for_each(|c| self.expr(c, kind, ctx));
            }
            Expr::Forall { domain, body, .. } => {
                if let ast::Domain::In(s) = domain {
                    self.expr(s, kind, ctx);
                }
                self.expr(body, kind, ctx);
            }
            Expr::Annotated(inner, anns) => {
                self.push(anns, kind, ctx);
                self.expr(inner, kind, ctx);
            }
        }
    }
}
