//! Static name resolution for Prudence modules: every identifier must refer to
//! a declaration, a local binding, or a built-in; every type name must exist;
//! `partition ... into` targets must be known forms. Member names are not
//! checked yet (that needs the type checker).

use std::collections::{HashMap, HashSet};

use super::ast::*;

const BUILTIN_VALUES: [&str; 22] = [
    "conservative", "known", "latest", "days", "bucket", "cycle", "add_months", "pow", "item", "min", "max", "sum", "all", "distinct", "events",
    "slices", "solo", "true", "false", "none", "Conservative", "Optimistic",
];
const BUILTIN_TYPES: [&str; 18] = [
    "Date", "Int", "Bool", "String", "Rate", "Money", "AUD", "Carrying", "MarketValue", "Uncertain", "Option", "List",
    "GroupId", "ProductClass", "Contract", "ConservationSplit", "Book", "Mode",
];

pub fn resolve(m: &Module, known_forms: &[&str]) -> Result<(), Vec<String>> {
    let mut r = Resolver { values: HashSet::new(), types: HashSet::new(), fields: HashMap::new(), scopes: vec![], errors: vec![] };
    r.values.extend(BUILTIN_VALUES.iter().map(|s| s.to_string()));
    r.types.extend(BUILTIN_TYPES.iter().map(|s| s.to_string()));

    // Pass 1: collect declarations.
    let mut seen = HashSet::new();
    for d in &m.decls {
        if !matches!(d.kind, DeclKind::Partition { .. }) && !seen.insert(d.name.clone()) {
            r.errors.push(format!("{}: `{}` is declared twice", d.pos, d.name));
        }
        match &d.kind {
            DeclKind::Param { .. } | DeclKind::View(_) | DeclKind::Fn { .. } | DeclKind::Alloc { .. } => {
                r.values.insert(d.name.clone());
            }
            DeclKind::TypeAlias(t) => {
                r.types.insert(d.name.clone());
                for alt in alternatives(t) {
                    if let Type::Named { name, .. } = alt {
                        r.values.insert(name.clone());
                    }
                }
            }
            DeclKind::Data { base, fields } => {
                r.types.insert(d.name.clone());
                r.values.insert(d.name.clone());
                r.fields.insert(d.name.clone(), fields.iter().map(|f| f.name.clone()).collect());
                if let Some(Type::Named { generics, .. }) = base {
                    for g in generics {
                        for alt in alternatives(g) {
                            if let Type::Named { name, .. } = alt {
                                r.values.insert(name.clone());
                            }
                        }
                    }
                }
            }
            DeclKind::Event(_) => {
                r.types.insert(d.name.clone());
                r.values.insert(d.name.clone());
            }
            _ => {}
        }
    }

    // Pass 2: check bodies.
    for d in &m.decls {
        match &d.kind {
            DeclKind::Param { ty, default } => {
                r.ty(ty);
                if let Some(e) = default {
                    r.expr(e);
                }
            }
            DeclKind::TypeAlias(_) => {} // alternatives are constructor definitions
            DeclKind::Data { base, fields } => {
                if let Some(Type::Named { name, generics, .. }) = base {
                    r.type_name(name, d.pos);
                    for g in generics {
                        for alt in alternatives(g) {
                            if let Type::Named { params, .. } = alt {
                                for p in params {
                                    if let TypeParam::Field(_, t) = p {
                                        r.ty(t);
                                    }
                                }
                            }
                        }
                    }
                }
                fields.iter().for_each(|f| r.ty(&f.ty));
            }
            DeclKind::Event(fields) => fields.iter().for_each(|f| r.ty(&f.ty)),
            DeclKind::View(e) | DeclKind::Invariant(e) | DeclKind::Theorem(e) => r.expr(e),
            DeclKind::Fn { params, ret, body } => {
                params.iter().for_each(|p| r.ty(&p.ty));
                r.ty(ret);
                if let Some(b) = body {
                    r.push(params.iter().map(|p| p.name.clone()));
                    r.expr(b);
                    r.pop();
                }
            }
            DeclKind::Alloc { ty, body } => {
                r.ty(ty);
                r.expr(body);
            }
            DeclKind::Partition { var, source, targets, split, split_var, split_by, more_splits, cases } => {
                r.ty(source);
                for t in targets {
                    if !known_forms.contains(&t.as_str()) {
                        r.errors.push(format!("{}: unknown form `{t}` in partition", d.pos));
                    }
                }
                r.push([var.clone()]);
                r.expr(split);
                r.expr(split_by);
                r.push([split_var.clone()]);
                for (n, e) in more_splits {
                    r.expr(e);
                    r.push([n.clone()]);
                }
                for c in cases {
                    r.expr(&c.cond);
                    let mut n = 0;
                    for (pat, e) in &c.lets {
                        r.expr(e);
                        r.push(binds(pat));
                        n += 1;
                    }
                    for (_, e) in &c.outputs {
                        r.expr(e);
                    }
                    for _ in 0..n {
                        r.pop();
                    }
                }
                for _ in more_splits {
                    r.pop();
                }
                r.pop();
                r.pop();
            }
        }
    }
    if r.errors.is_empty() { Ok(()) } else { Err(r.errors) }
}

fn alternatives(t: &Type) -> Vec<&Type> {
    match t {
        Type::Union(xs) => xs.iter().collect(),
        other => vec![other],
    }
}

fn binds(p: &Pat) -> Vec<String> {
    match p {
        Pat::Wild => vec![],
        Pat::Bind(n) => vec![n.clone()],
        Pat::Ctor(_, ps) | Pat::Tuple(ps) => ps.iter().flat_map(binds).collect(),
    }
}

struct Resolver {
    values: HashSet<String>,
    types: HashSet<String>,
    fields: HashMap<String, Vec<String>>,
    scopes: Vec<HashSet<String>>,
    errors: Vec<String>,
}

impl Resolver {
    fn push(&mut self, names: impl IntoIterator<Item = String>) {
        self.scopes.push(names.into_iter().collect());
    }
    fn pop(&mut self) {
        self.scopes.pop();
    }
    fn bound(&self, n: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(n)) || self.values.contains(n)
    }
    fn type_name(&mut self, n: &str, pos: super::lexer::Pos) {
        if !self.types.contains(n) {
            self.errors.push(format!("{pos}: unknown type `{n}`"));
        }
    }
    fn ty(&mut self, t: &Type) {
        match t {
            Type::Union(xs) => xs.iter().for_each(|x| self.ty(x)),
            Type::Named { name, generics, params, pos } => {
                self.type_name(name, *pos);
                generics.iter().for_each(|g| self.ty(g));
                for p in params {
                    if let TypeParam::Expr(e) = p {
                        self.expr(e);
                    }
                }
            }
        }
    }
    fn pat(&mut self, p: &Pat, pos: Option<super::lexer::Pos>) {
        match p {
            Pat::Ctor(name, ps) => {
                // `Some(x)` destructures an optional value.
                if name != "Some" && !self.values.contains(name) {
                    let at = pos.map(|p| format!("{p}: ")).unwrap_or_default();
                    self.errors.push(format!("{at}unknown constructor `{name}` in pattern"));
                }
                ps.iter().for_each(|x| self.pat(x, pos));
            }
            Pat::Tuple(ps) => ps.iter().for_each(|x| self.pat(x, pos)),
            Pat::Wild | Pat::Bind(_) => {}
        }
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Num(..) | Expr::Percent(..) | Expr::Str(..) | Expr::Cell(..) => {}
            Expr::Ident(n, pos) => {
                if !self.bound(n) {
                    self.errors.push(format!("{pos}: unknown name `{n}`"));
                }
            }
            Expr::List(xs) | Expr::Tuple(xs) => xs.iter().for_each(|x| self.expr(x)),
            Expr::Unary(_, x) | Expr::Member(x, _) | Expr::Annotated(x, _) => self.expr(x),
            Expr::Is(x, ty) => {
                self.expr(x);
                if !self.types.contains(ty) {
                    self.errors.push(format!("unknown type `{ty}` after `is`"));
                }
            }
            Expr::Binary(_, a, b) | Expr::Index(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            Expr::Call(f, args) => {
                self.expr(f);
                args.iter().for_each(|a| self.expr(&a.value));
            }
            Expr::If { cond, then, other } => {
                self.expr(cond);
                self.expr(then);
                self.expr(other);
            }
            Expr::IfLet { pat, value, then, other } => {
                self.expr(value);
                self.pat(pat, None);
                self.push(binds(pat));
                self.expr(then);
                self.pop();
                self.expr(other);
            }
            Expr::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for a in arms {
                    self.pat(&a.pat, None);
                    self.push(binds(&a.pat));
                    if let Some(g) = &a.guard {
                        self.expr(g);
                    }
                    self.expr(&a.body);
                    self.pop();
                }
            }
            Expr::Let { pat, value, body } => {
                self.expr(value);
                self.push(binds(pat));
                self.expr(body);
                self.pop();
            }
            Expr::Comprehension { var, source, filter, body } => {
                self.expr(source);
                self.push([var.clone()]);
                if let Some(f) = filter {
                    self.expr(f);
                }
                if let Some(b) = body {
                    self.expr(b);
                }
                self.pop();
            }
            Expr::Group { source, key, filter, fields } => {
                self.expr(source);
                // Inside a group, the grouped rows' fields are in scope.
                let row_fields = match &**source {
                    Expr::Ident(n, _) => self.fields.get(n).cloned().unwrap_or_default(),
                    _ => vec![],
                };
                self.push(row_fields);
                self.expr(key);
                if let Some(f) = filter {
                    self.expr(f);
                }
                fields.iter().for_each(|(_, x)| self.expr(x));
                self.pop();
            }
            Expr::Optimize { per, filter, objective, constraints, .. } => {
                self.push([per.clone()]);
                if let Some(f) = filter {
                    self.expr(f);
                }
                self.expr(objective);
                constraints.iter().for_each(|c| self.expr(c));
                self.pop();
            }
            Expr::Forall { var, domain, body } => {
                match domain {
                    Domain::Type(t) => self.ty(t),
                    Domain::In(src) => self.expr(src),
                }
                self.push([var.clone()]);
                self.expr(body);
                self.pop();
            }
        }
    }
}
