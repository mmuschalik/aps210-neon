//! Type checker for Prudence modules.
//!
//! Enforces the design's static guarantees (design.txt §4.A.1-3):
//! - an `Uncertain<T>` value cannot be used as a `T` until it is resolved with
//!   `conservative(x, worst: .., best: ..)`;
//! - `Money<Currency, Basis>` arithmetic and comparisons need matching tags;
//!   money times a rate stays money;
//! - values of different enum types are never compared or mixed;
//! - record and event fields must exist;
//! - `if` branches, `match` arms and function bodies agree with declared types;
//! - `match` patterns only use constructors of the scrutinee's type.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::ast::*;
use super::lexer::Pos;

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Bool,
    Int,
    Num,
    Rate,
    /// Money<currency, basis>; basis `None` for literals such as `0.AUD`.
    Money(String, Option<String>),
    Date,
    Days,
    Str,
    /// Opaque named scalar types (GroupId, ProductClass).
    Opaque(String),
    /// A value of an enum type alias.
    Enum(String),
    /// A bare constructor that belongs to several enums (resolved by context).
    Ctors(BTreeSet<String>),
    Data(String),
    Event(String),
    /// Any event (from `events(..)`): fields resolve across event types.
    AnyEvent,
    /// The `contract` of a data type with an ACTUS base.
    Contract(String),
    List(Box<Ty>),
    Optional(Box<Ty>),
    Uncertain(Box<Ty>),
    Tuple(Vec<Ty>),
    Struct(Vec<(String, Ty)>),
    /// A keyed view (group) or allocation: key type -> value type.
    Map(Box<Ty>, Box<Ty>),
    Table(String),
    Mode,
    /// `none`
    Nothing,
    Unknown,
}

impl Ty {
    fn name(&self) -> String {
        match self {
            Ty::Money(c, b) => format!("Money<{c}, {}>", b.as_deref().unwrap_or("_")),
            Ty::Enum(e) | Ty::Data(e) | Ty::Event(e) | Ty::Opaque(e) => e.clone(),
            Ty::Ctors(s) => s.iter().cloned().collect::<Vec<_>>().join("|"),
            Ty::List(t) => format!("List<{}>", t.name()),
            Ty::Optional(t) => format!("Option<{}>", t.name()),
            Ty::Uncertain(t) => format!("Uncertain<{}>", t.name()),
            other => format!("{other:?}"),
        }
    }
    fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int | Ty::Num | Ty::Rate)
    }
}

/// Inferred types of the module's views (for code generation).
pub fn view_types(m: &Module) -> HashMap<String, Ty> {
    let mut c = Checker::new(m);
    c.run(m);
    c.view_types
}

pub fn check(m: &Module) -> Result<(), Vec<String>> {
    let mut c = Checker::new(m);
    c.run(m);
    if c.errors.is_empty() { Ok(()) } else { Err(c.errors) }
}

struct Checker<'m> {
    m: &'m Module,
    enums: HashMap<String, BTreeSet<String>>,
    ctor_of: HashMap<String, BTreeSet<String>>,
    data: HashMap<String, Vec<(String, Ty)>>,
    variants: HashMap<String, Vec<(String, Ty)>>, // ctor -> fields (contract variants)
    contract_of: HashMap<String, String>,         // variant ctor -> data type
    events: BTreeMap<String, Vec<(String, Ty)>>,
    globals: HashMap<String, Ty>,
    fns: HashMap<String, (Vec<(String, Ty)>, Ty)>,
    scopes: Vec<Vec<(String, Ty)>>,
    errors: Vec<String>,
    view_types: HashMap<String, Ty>,
}

fn first_pos(e: &Expr) -> Option<Pos> {
    match e {
        Expr::Num(_, p) | Expr::Percent(_, p) | Expr::Str(_, p) | Expr::Ident(_, p) | Expr::Cell(_, _, p) => Some(*p),
        Expr::Unary(_, x) | Expr::Member(x, _) | Expr::Is(x, _) | Expr::Annotated(x, _) => first_pos(x),
        Expr::Binary(_, a, _) | Expr::Index(a, _) => first_pos(a),
        Expr::Call(f, _) => first_pos(f),
        Expr::List(xs) | Expr::Tuple(xs) => xs.first().and_then(first_pos),
        Expr::If { cond, .. } => first_pos(cond),
        Expr::IfLet { value, .. } | Expr::Let { value, .. } => first_pos(value),
        Expr::Match { scrutinee, .. } => first_pos(scrutinee),
        Expr::Comprehension { source, .. } | Expr::Group { source, .. } => first_pos(source),
        Expr::Optimize { objective, .. } => first_pos(objective),
        Expr::Forall { body, .. } => first_pos(body),
    }
}

impl<'m> Checker<'m> {
    fn new(m: &'m Module) -> Self {
        Checker {
            m,
            enums: HashMap::new(),
            ctor_of: HashMap::new(),
            data: HashMap::new(),
            variants: HashMap::new(),
            contract_of: HashMap::new(),
            events: BTreeMap::new(),
            globals: HashMap::new(),
            fns: HashMap::new(),
            scopes: Vec::new(),
            errors: Vec::new(),
            view_types: HashMap::new(),
        }
    }

    fn err(&mut self, at: Option<Pos>, msg: String) {
        let at = at.map(|p| format!("{p}: ")).unwrap_or_default();
        self.errors.push(format!("{at}{msg}"));
    }

    // ------------------------------------------------------------------ declared types
    fn resolve_ty(&self, t: &Type) -> Ty {
        match t {
            Type::Union(_) => Ty::Unknown,
            Type::Named { name, generics, .. } => {
                let g = |i: usize| generics.get(i).map(|t| self.resolve_ty(t)).unwrap_or(Ty::Unknown);
                match name.as_str() {
                    "Bool" => Ty::Bool,
                    "String" => Ty::Str,
                    "Int" => Ty::Int,
                    "Rate" => Ty::Rate,
                    "Date" => Ty::Date,
                    "Mode" => Ty::Mode,
                    "GroupId" | "ProductClass" => Ty::Opaque(name.clone()),
                    "Money" => {
                        let tag = |i: usize| match generics.get(i) {
                            Some(Type::Named { name, .. }) => Some(name.clone()),
                            _ => None,
                        };
                        Ty::Money(tag(0).unwrap_or_default(), tag(1))
                    }
                    "List" => Ty::List(Box::new(g(0))),
                    "Option" => Ty::Optional(Box::new(g(0))),
                    "Uncertain" => Ty::Uncertain(Box::new(g(0))),
                    "Book" => Ty::Opaque("Book".into()),
                    n if self.enums.contains_key(n) => Ty::Enum(n.into()),
                    n if self.data.contains_key(n) => Ty::Data(n.into()),
                    n if self.events.contains_key(n) => Ty::Event(n.into()),
                    _ => Ty::Unknown,
                }
            }
        }
    }

    fn collect(&mut self) {
        // Enums first (types of fields may refer to them).
        for d in &self.m.decls {
            if let DeclKind::TypeAlias(Type::Union(alts)) = &d.kind {
                let ctors: BTreeSet<String> = alts
                    .iter()
                    .filter_map(|a| match a {
                        Type::Named { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
                for c in &ctors {
                    self.ctor_of.entry(c.clone()).or_default().insert(d.name.clone());
                }
                self.enums.insert(d.name.clone(), ctors);
            }
        }
        // Linear pools (design: Pool<T>): every record of a `@linear` data
        // type is placed into each target form by exactly one partition, whose
        // cases Z3 proves exhaustive and disjoint - so a record takes exactly
        // one role per form (e.g. HQLA stock or a maturing inflow, never both).
        for d in &self.m.decls {
            if !matches!(d.kind, DeclKind::Data { .. }) || !d.anns.iter().any(|a| a.name == "linear") {
                continue;
            }
            let mut by_form: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for p in &self.m.decls {
                if let DeclKind::Partition { source: Type::Named { name, .. }, targets, .. } = &p.kind
                    && name == &d.name
                {
                    for t in targets {
                        *by_form.entry(t.as_str()).or_default() += 1;
                    }
                }
            }
            if by_form.is_empty() {
                self.err(Some(d.pos), format!("linear data `{}` is never partitioned", d.name));
            }
            for (f, n) in by_form.into_iter().filter(|(_, n)| *n > 1) {
                self.err(Some(d.pos), format!("linear data `{}` is partitioned into {f} {n} times", d.name));
            }
        }
        for d in &self.m.decls {
            match &d.kind {
                DeclKind::Data { .. } => {
                    self.data.insert(d.name.clone(), vec![]);
                }
                DeclKind::Event(_) => {
                    self.events.insert(d.name.clone(), vec![]);
                }
                _ => {}
            }
        }
        for d in &self.m.decls {
            match &d.kind {
                DeclKind::Data { base, fields } => {
                    let mut fs: Vec<(String, Ty)> = fields.iter().map(|f| (f.name.clone(), self.resolve_ty(&f.ty))).collect();
                    if let Some(Type::Named { name, generics, .. }) = base
                        && name == "Contract" {
                            fs.push(("contract".into(), Ty::Contract(d.name.clone())));
                            for g in generics {
                                let alts = match g {
                                    Type::Union(xs) => xs.iter().collect(),
                                    other => vec![other],
                                };
                                for a in alts {
                                    if let Type::Named { name: v, params, .. } = a {
                                        let vf = params
                                            .iter()
                                            .filter_map(|p| match p {
                                                TypeParam::Field(n, t) => Some((n.clone(), self.resolve_ty(t))),
                                                _ => None,
                                            })
                                            .collect();
                                        self.variants.insert(v.clone(), vf);
                                        self.contract_of.insert(v.clone(), d.name.clone());
                                    }
                                }
                            }
                        }
                    self.data.insert(d.name.clone(), fs);
                }
                DeclKind::Event(fields) => {
                    let fs = fields.iter().map(|f| (f.name.clone(), self.resolve_ty(&f.ty))).collect();
                    self.events.insert(d.name.clone(), fs);
                }
                DeclKind::Param { ty, .. } => {
                    let t = self.resolve_ty(ty);
                    self.globals.insert(d.name.clone(), t);
                }
                DeclKind::Fn { params, ret, .. } => {
                    let ps = params.iter().map(|p| (p.name.clone(), self.resolve_ty(&p.ty))).collect();
                    let r = self.resolve_ty(ret);
                    self.fns.insert(d.name.clone(), (ps, r));
                }
                _ => {}
            }
        }
    }

    fn run(&mut self, m: &'m Module) {
        self.collect();
        // Views and allocs: infer types in order (allocs may use views).
        for d in &m.decls {
            match &d.kind {
                DeclKind::View(e) => {
                    let t = self.view_ty(e);
                    self.view_types.insert(d.name.clone(), t.clone());
                    self.globals.insert(d.name.clone(), t);
                }
                DeclKind::Alloc { ty, .. } => {
                    // ConservationSplit(T.field, per: k): a map from T to the field's type.
                    let t = match ty {
                        Type::Named { params, .. } => match params.first() {
                            Some(TypeParam::Expr(Expr::Member(src, f))) => match &**src {
                                Expr::Ident(t, _) => {
                                    let ft = self.field_ty(&Ty::Data(t.clone()), f).unwrap_or(Ty::Unknown);
                                    Ty::Map(Box::new(Ty::Data(t.clone())), Box::new(ft))
                                }
                                _ => Ty::Unknown,
                            },
                            _ => Ty::Unknown,
                        },
                        _ => Ty::Unknown,
                    };
                    self.globals.insert(d.name.clone(), t);
                }
                _ => {}
            }
        }
        for d in &m.decls {
            match &d.kind {
                DeclKind::Param { ty, default: Some(e) } => {
                    let want = self.resolve_ty(ty);
                    let got = self.expr(e);
                    self.expect(&want, &got, first_pos(e), &format!("default of `{}`", d.name));
                }
                DeclKind::Fn { params, body: Some(b), .. } => {
                    let (ps, ret) = self.fns[&d.name].clone();
                    let _ = params;
                    self.scopes.push(ps);
                    let got = self.expr(b);
                    self.scopes.pop();
                    self.expect(&ret, &got, first_pos(b), &format!("body of `{}`", d.name));
                }
                DeclKind::Alloc { ty, body } => self.alloc(ty, body),
                DeclKind::Partition { var, source, split, split_var, split_by, more_splits, cases, .. } => {
                    let st = self.resolve_ty(source);
                    self.scopes.push(vec![(var.clone(), st)]);
                    let whole = self.expr(split);
                    let part = self.expr(split_by);
                    self.expect(&whole, &part, first_pos(split_by), "partition split");
                    self.scopes.push(vec![(split_var.clone(), Ty::Bool)]);
                    for (n, e) in more_splits {
                        let t = self.expr(e);
                        self.expect(&whole, &t, first_pos(e), "partition split");
                        self.scopes.push(vec![(n.clone(), Ty::Bool)]);
                    }
                    for c in cases {
                        let ct = self.expr(&c.cond);
                        self.want_bool(&ct, first_pos(&c.cond), "partition case");
                        let mut n = 0;
                        for (p, e) in &c.lets {
                            let t = self.expr(e);
                            let binds = self.pat_binds(p, &t);
                            self.scopes.push(binds);
                            n += 1;
                        }
                        for (_, e) in &c.outputs {
                            self.expr(e);
                        }
                        for _ in 0..n {
                            self.scopes.pop();
                        }
                    }
                    for _ in more_splits {
                        self.scopes.pop();
                    }
                    self.scopes.pop();
                    self.scopes.pop();
                }
                DeclKind::Invariant(e) | DeclKind::Theorem(e) => {
                    let t = self.expr(e);
                    self.want_bool(&t, first_pos(e), &d.name);
                }
                _ => {}
            }
        }
    }

    fn view_ty(&mut self, e: &Expr) -> Ty {
        match e {
            Expr::Annotated(x, _) => self.view_ty(x),
            Expr::Group { source, key, filter, fields } => {
                let Expr::Ident(t, _) = &**source else { return Ty::Unknown };
                let row = self.data.get(t).cloned().unwrap_or_default();
                self.scopes.push(row);
                let kt = self.expr(key);
                if let Some(f) = filter {
                    let ft = self.expr(f);
                    self.want_bool(&ft, first_pos(f), "group filter");
                }
                let mut fs = Vec::new();
                for (n, fe) in fields {
                    // `sum(balance)`: the aggregate of a row expression.
                    let t = match fe {
                        Expr::Call(_, args) if args.len() == 1 => self.expr(&args[0].value),
                        other => self.expr(other),
                    };
                    fs.push((n.clone(), t));
                }
                self.scopes.pop();
                let kt = match kt {
                    Ty::Optional(t) => *t,
                    t => t,
                };
                Ty::Map(Box::new(kt), Box::new(Ty::Struct(fs)))
            }
            other => self.expr(other),
        }
    }

    fn alloc(&mut self, ty: &Type, body: &Expr) {
        let mut body = body;
        while let Expr::Annotated(x, _) = body {
            body = x;
        }
        let Expr::Optimize { per, filter, objective, constraints, .. } = body else {
            self.err(first_pos(body), "alloc body must be `optimize`".into());
            return;
        };
        // `per` ranges over the values of the named field of the source type.
        let per_ty = match ty {
            Type::Named { params, .. } => match params.as_slice() {
                [TypeParam::Expr(Expr::Member(src, _)), TypeParam::Field(_, Type::Named { name: f, .. })] => match &**src {
                    Expr::Ident(t, _) => self.field_ty(&Ty::Data(t.clone()), f).unwrap_or(Ty::Unknown),
                    _ => Ty::Unknown,
                },
                _ => Ty::Unknown,
            },
            _ => Ty::Unknown,
        };
        self.scopes.push(vec![(per.clone(), per_ty)]);
        if let Some(f) = filter {
            let t = self.expr(f);
            self.want_bool(&t, first_pos(f), "optimize filter");
        }
        let ot = self.expr(objective);
        if !matches!(ot, Ty::Money(..) | Ty::Num | Ty::Int | Ty::Rate | Ty::Unknown) {
            self.err(first_pos(objective), format!("objective must be numeric, found {}", ot.name()));
        }
        for c in constraints {
            let t = self.expr(c);
            self.want_bool(&t, first_pos(c), "constraint");
        }
        self.scopes.pop();
    }

    // ------------------------------------------------------------------ helpers
    fn lookup(&self, n: &str) -> Option<Ty> {
        for s in self.scopes.iter().rev() {
            if let Some((_, t)) = s.iter().rev().find(|(k, _)| k == n) {
                return Some(t.clone());
            }
        }
        self.globals.get(n).cloned()
    }

    fn field_ty(&self, t: &Ty, f: &str) -> Option<Ty> {
        let find = |fs: &Vec<(String, Ty)>| fs.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone());
        // Every event is bitemporal: when it took effect, and when it was recorded.
        if matches!(t, Ty::Event(_) | Ty::AnyEvent) && (f == "valid" || f == "recorded") {
            return Some(Ty::Date);
        }
        match t {
            Ty::Data(d) => self.data.get(d).and_then(find),
            Ty::Event(e) => self.events.get(e).and_then(find),
            Ty::AnyEvent => {
                let hits: Vec<Ty> = self.events.values().filter_map(find).collect();
                hits.first().cloned()
            }
            Ty::Struct(fs) => find(fs),
            _ => None,
        }
    }

    fn want_bool(&mut self, t: &Ty, at: Option<Pos>, what: &str) {
        match t {
            Ty::Bool | Ty::Unknown => {}
            Ty::Uncertain(_) => self.err(at, format!("{what}: unknown value used as Bool; resolve it with conservative(..)")),
            other => self.err(at, format!("{what}: expected Bool, found {}", other.name())),
        }
    }

    /// Unify two types; returns the joined type or None if incompatible.
    fn join(&self, a: &Ty, b: &Ty) -> Option<Ty> {
        Some(match (a, b) {
            (Ty::Unknown, t) | (t, Ty::Unknown) => t.clone(),
            (x, y) if x == y => x.clone(),
            (x, y) if x.is_numeric() && y.is_numeric() => {
                if matches!(x, Ty::Rate) || matches!(y, Ty::Rate) { Ty::Rate } else { Ty::Num }
            }
            (Ty::Money(c1, b1), Ty::Money(c2, b2)) if c1 == c2 => match (b1, b2) {
                (Some(x), Some(y)) if x != y => return None,
                (Some(x), _) | (_, Some(x)) => Ty::Money(c1.clone(), Some(x.clone())),
                _ => Ty::Money(c1.clone(), None),
            },
            (Ty::Enum(e), Ty::Ctors(s)) | (Ty::Ctors(s), Ty::Enum(e)) if s.contains(e) => Ty::Enum(e.clone()),
            (Ty::Ctors(s1), Ty::Ctors(s2)) => {
                let i: BTreeSet<String> = s1.intersection(s2).cloned().collect();
                match i.len() {
                    0 => return None,
                    1 => Ty::Enum(i.into_iter().next().unwrap()),
                    _ => Ty::Ctors(i),
                }
            }
            (Ty::Nothing, Ty::Optional(t)) | (Ty::Optional(t), Ty::Nothing) => Ty::Optional(t.clone()),
            (Ty::Nothing, t) | (t, Ty::Nothing) => Ty::Optional(Box::new(t.clone())),
            (Ty::Optional(t), u) | (u, Ty::Optional(t)) => Ty::Optional(Box::new(self.join(t, u)?)),
            (Ty::Event(_), Ty::AnyEvent) | (Ty::AnyEvent, Ty::Event(_)) => Ty::AnyEvent,
            (Ty::Tuple(xs), Ty::Tuple(ys)) if xs.len() == ys.len() => {
                Ty::Tuple(xs.iter().zip(ys).map(|(x, y)| self.join(x, y)).collect::<Option<Vec<_>>>()?)
            }
            (Ty::List(x), Ty::List(y)) => Ty::List(Box::new(self.join(x, y)?)),
            _ => return None,
        })
    }

    fn expect(&mut self, want: &Ty, got: &Ty, at: Option<Pos>, what: &str) {
        if let Ty::Uncertain(_) = got
            && !matches!(want, Ty::Uncertain(_)) {
                self.err(at, format!("{what}: unknown value used where {} is required; resolve it with conservative(..)", want.name()));
                return;
            }
        if self.join(want, got).is_none() {
            self.err(at, format!("{what}: expected {}, found {}", want.name(), got.name()));
        }
    }

    fn pat_binds(&mut self, p: &Pat, t: &Ty) -> Vec<(String, Ty)> {
        match (p, t) {
            (Pat::Wild, _) => vec![],
            (Pat::Bind(n), _) => vec![(n.clone(), t.clone())],
            (Pat::Tuple(ps), Ty::Tuple(ts)) if ps.len() == ts.len() => {
                ps.iter().zip(ts).flat_map(|(p, t)| self.pat_binds(p, t)).collect()
            }
            (Pat::Tuple(ps), Ty::Unknown) => ps.iter().flat_map(|p| self.pat_binds(p, &Ty::Unknown)).collect(),
            (Pat::Ctor(c, ps), _) if c == "Some" => match (ps.as_slice(), t) {
                ([p], Ty::Optional(x)) => self.pat_binds(p, x),
                // Optional record fields are typed as their value type
                // (absent = `none`); `Some(x)` binds the value.
                ([p], other) => self.pat_binds(p, other),
                _ => {
                    self.err(None, "`Some(..)` takes one pattern".into());
                    vec![]
                }
            },
            (Pat::Ctor(c, ps), _) => {
                let fields: Vec<Ty> = if let Some(vf) = self.variants.get(c).cloned() {
                    if let Ty::Contract(d) = t
                        && self.contract_of.get(c) != Some(d) {
                            self.err(None, format!("pattern `{c}` is not a contract of `{d}`"));
                        }
                    vf.iter().map(|(_, t)| t.clone()).collect()
                } else if let Some(ef) = self.events.get(c) {
                    ef.iter().map(|(_, t)| t.clone()).collect()
                } else {
                    let ok = match t {
                        Ty::Enum(e) => self.enums.get(e).is_some_and(|cs| cs.contains(c)),
                        Ty::Unknown | Ty::Ctors(_) => true,
                        _ => false,
                    };
                    if !ok {
                        self.err(None, format!("pattern `{c}` does not match a value of type {}", t.name()));
                    }
                    vec![]
                };
                if !ps.is_empty() && ps.len() != fields.len() {
                    self.err(None, format!("pattern `{c}` has {} fields, expected {}", ps.len(), fields.len()));
                }
                ps.iter().zip(fields.iter().chain(std::iter::repeat(&Ty::Unknown))).flat_map(|(p, t)| self.pat_binds(p, t)).collect()
            }
            (Pat::Tuple(_), other) => {
                self.err(None, format!("tuple pattern against {}", other.name()));
                vec![]
            }
        }
    }

    fn comprehension(&mut self, a: &Expr) -> Ty {
        match a {
            Expr::Comprehension { var, source, filter, body } => {
                let st = self.expr(source);
                let elem = match st {
                    Ty::List(t) => *t,
                    Ty::Table(d) => Ty::Data(d),
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.err(first_pos(source), format!("comprehension over non-list {}", other.name()));
                        Ty::Unknown
                    }
                };
                self.scopes.push(vec![(var.clone(), elem.clone())]);
                if let Some(f) = filter {
                    let ft = self.expr(f);
                    self.want_bool(&ft, first_pos(f), "filter");
                }
                let bt = match body {
                    Some(b) => self.expr(b),
                    None => elem,
                };
                self.scopes.pop();
                bt
            }
            other => match self.expr(other) {
                Ty::List(t) => *t,
                t => t,
            },
        }
    }

    // ------------------------------------------------------------------ expressions
    fn expr(&mut self, e: &Expr) -> Ty {
        let at = first_pos(e);
        match e {
            Expr::Num(n, _) => if n.contains('.') { Ty::Num } else { Ty::Int },
            Expr::Percent(..) => Ty::Rate,
            Expr::Str(..) => Ty::Str,
            Expr::Cell(..) => Ty::Num,
            Expr::Annotated(x, _) => self.expr(x),
            Expr::Ident(n, _) => match n.as_str() {
                "true" | "false" => Ty::Bool,
                "none" => Ty::Nothing,
                "Conservative" | "Optimistic" => Ty::Mode,
                _ => {
                    if let Some(t) = self.lookup(n) {
                        return t;
                    }
                    if let Some(es) = self.ctor_of.get(n) {
                        return if es.len() == 1 { Ty::Enum(es.iter().next().unwrap().clone()) } else { Ty::Ctors(es.clone()) };
                    }
                    if self.data.contains_key(n) {
                        return Ty::Table(n.clone());
                    }
                    if let Some(d) = self.contract_of.get(n) {
                        return Ty::Contract(d.clone());
                    }
                    Ty::Unknown
                }
            },
            Expr::List(xs) => {
                let mut t = Ty::Unknown;
                for x in xs {
                    let xt = self.expr(x);
                    t = self.join(&t, &xt).unwrap_or({
                        Ty::Unknown
                    });
                }
                Ty::List(Box::new(t))
            }
            Expr::Tuple(xs) => Ty::Tuple(xs.iter().map(|x| self.expr(x)).collect()),
            Expr::Unary(UnOp::Not, x) => {
                let t = self.expr(x);
                self.want_bool(&t, at, "`!`");
                Ty::Bool
            }
            Expr::Unary(UnOp::Neg, x) => self.expr(x),
            Expr::Member(x, f) => {
                let t = self.expr(x);
                match &t {
                    Ty::Unknown => Ty::Unknown,
                    Ty::Int if f == "days" => Ty::Days,
                    Ty::Int | Ty::Num if f.chars().all(|c| c.is_ascii_uppercase()) => Ty::Money(f.clone(), None),
                    Ty::Table(d) => match self.field_ty(&Ty::Data(d.clone()), f) {
                        Some(ft) => Ty::List(Box::new(ft)),
                        None => {
                            self.err(at, format!("`{d}` has no field `{f}`"));
                            Ty::Unknown
                        }
                    },
                    Ty::Uncertain(_) => {
                        self.err(at, format!("reading `.{f}` of an unknown value; resolve it with conservative(..)"));
                        Ty::Unknown
                    }
                    other => match self.field_ty(other, f) {
                        Some(ft) => ft,
                        None => {
                            self.err(at, format!("{} has no field `{f}`", other.name()));
                            Ty::Unknown
                        }
                    },
                }
            }
            Expr::Index(x, k) => {
                let t = self.expr(x);
                let kt = self.expr(k);
                match t {
                    Ty::Map(key, val) => {
                        let kt = match kt {
                            Ty::Optional(t) => *t,
                            t => t,
                        };
                        self.expect(&key, &kt, at, "map key");
                        *val
                    }
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.err(at, format!("cannot index {}", other.name()));
                        Ty::Unknown
                    }
                }
            }
            Expr::Is(x, ty) => {
                self.expr(x);
                if !self.events.contains_key(ty) && !self.data.contains_key(ty) {
                    self.err(at, format!("`is {ty}`: not an event or data type"));
                }
                Ty::Bool
            }
            Expr::Binary(op, a, b) => self.binary(*op, a, b, at),
            Expr::If { cond, then, other } => {
                let ct = self.expr(cond);
                self.want_bool(&ct, first_pos(cond), "`if` condition");
                let t1 = self.expr(then);
                let t2 = self.expr(other);
                self.join(&t1, &t2).unwrap_or_else(|| {
                    self.err(at, format!("`if` branches differ: {} vs {}", t1.name(), t2.name()));
                    Ty::Unknown
                })
            }
            Expr::IfLet { pat, value, then, other } => {
                let vt = self.expr(value);
                let inner = match vt {
                    Ty::Optional(t) => *t,
                    t => t,
                };
                let binds = self.pat_binds(pat, &inner);
                self.scopes.push(binds);
                let t1 = self.expr(then);
                self.scopes.pop();
                let t2 = self.expr(other);
                self.join(&t1, &t2).unwrap_or_else(|| {
                    self.err(at, format!("`if let` branches differ: {} vs {}", t1.name(), t2.name()));
                    Ty::Unknown
                })
            }
            Expr::Let { pat, value, body } => {
                let vt = self.expr(value);
                let binds = self.pat_binds(pat, &vt);
                self.scopes.push(binds);
                let t = self.expr(body);
                self.scopes.pop();
                t
            }
            Expr::Match { scrutinee, arms } => {
                let st = self.expr(scrutinee);
                if let Ty::Uncertain(_) = st {
                    self.err(at, "matching on an unknown value; resolve it with conservative(..)".into());
                }
                let mut out = Ty::Unknown;
                for arm in arms {
                    let binds = self.pat_binds(&arm.pat, &st);
                    self.scopes.push(binds);
                    if let Some(g) = &arm.guard {
                        let gt = self.expr(g);
                        self.want_bool(&gt, first_pos(g), "guard");
                    }
                    let bt = self.expr(&arm.body);
                    self.scopes.pop();
                    out = match self.join(&out, &bt) {
                        Some(t) => t,
                        None => {
                            self.err(first_pos(&arm.body), format!("match arms differ: {} vs {}", out.name(), bt.name()));
                            Ty::Unknown
                        }
                    };
                }
                out
            }
            Expr::Call(f, args) => self.call(f, args, at),
            Expr::Forall { var, domain, body } => {
                let vt = match domain {
                    Domain::Type(t) => self.resolve_ty(t),
                    Domain::In(src) => match self.expr(src) {
                        Ty::List(t) => *t,
                        _ => Ty::Unknown,
                    },
                };
                self.scopes.push(vec![(var.clone(), vt)]);
                let bt = self.expr(body);
                self.scopes.pop();
                self.want_bool(&bt, first_pos(body), "forall body");
                Ty::Bool
            }
            Expr::Comprehension { .. } => {
                self.err(at, "comprehension outside an aggregate".into());
                Ty::Unknown
            }
            Expr::Group { .. } | Expr::Optimize { .. } => Ty::Unknown,
        }
    }

    fn binary(&mut self, op: BinOp, a: &Expr, b: &Expr, at: Option<Pos>) -> Ty {
        use BinOp::*;
        let ta = self.expr(a);
        let tb = self.expr(b);
        for t in [&ta, &tb] {
            if let Ty::Uncertain(_) = t {
                self.err(at, format!("unknown value in `{op:?}`; resolve it with conservative(..)"));
                return Ty::Unknown;
            }
        }
        match op {
            And | Or | Implies => {
                self.want_bool(&ta, first_pos(a), &format!("{op:?}"));
                self.want_bool(&tb, first_pos(b), &format!("{op:?}"));
                Ty::Bool
            }
            In => {
                match &tb {
                    Ty::List(elem) => {
                        if self.join(&ta, elem).is_none() {
                            self.err(at, format!("`in`: {} is never an element of {}", ta.name(), tb.name()));
                        }
                    }
                    Ty::Unknown => {}
                    other => self.err(at, format!("`in` needs a list, found {}", other.name())),
                }
                Ty::Bool
            }
            Eq | Ne | Lt | Le | Gt | Ge => {
                let ordered = !matches!(op, Eq | Ne);
                match self.join(&ta, &tb) {
                    None => self.err(at, format!("cannot compare {} with {}", ta.name(), tb.name())),
                    Some(t) if ordered => {
                        let base = match &t {
                            Ty::Optional(x) => &**x,
                            x => x,
                        };
                        if !(base.is_numeric() || matches!(base, Ty::Money(..) | Ty::Date | Ty::Unknown)) {
                            self.err(at, format!("{} values are not ordered", t.name()));
                        }
                    }
                    _ => {}
                }
                Ty::Bool
            }
            Add | Sub | Mul | Div => match (op, &ta, &tb) {
                (_, Ty::Unknown, _) | (_, _, Ty::Unknown) => Ty::Unknown,
                (Add | Sub, Ty::Money(..), Ty::Money(..)) => self.join(&ta, &tb).unwrap_or_else(|| {
                    self.err(at, format!("money tags differ: {} {op:?} {}", ta.name(), tb.name()));
                    Ty::Unknown
                }),
                (Mul, Ty::Money(..), t) | (Mul, t, Ty::Money(..)) if t.is_numeric() => {
                    if let Ty::Money(..) = ta { ta.clone() } else { tb.clone() }
                }
                (Div, Ty::Money(..), t) if t.is_numeric() => ta.clone(),
                (Div, Ty::Money(..), Ty::Money(..)) => Ty::Rate,
                (Add | Sub, Ty::Date, Ty::Days) => Ty::Date,
                (Add, Ty::List(_), Ty::List(_)) => self.join(&ta, &tb).unwrap_or_else(|| {
                    self.err(at, format!("cannot concatenate {} and {}", ta.name(), tb.name()));
                    Ty::Unknown
                }),
                (_, x, y) if x.is_numeric() && y.is_numeric() => self.join(x, y).unwrap_or(Ty::Num),
                _ => {
                    self.err(at, format!("cannot apply {op:?} to {} and {}", ta.name(), tb.name()));
                    Ty::Unknown
                }
            },
        }
    }

    fn call(&mut self, f: &Expr, args: &[Arg], at: Option<Pos>) -> Ty {
        let Expr::Ident(name, _) = f else { return Ty::Unknown };
        if let Some((ps, ret)) = self.fns.get(name).cloned() {
            let mut positional = 0;
            for a in args {
                let t = self.expr(&a.value);
                let want = match &a.name {
                    Some(n) => ps.iter().find(|(p, _)| p == n).map(|(_, t)| t.clone()),
                    None => {
                        positional += 1;
                        ps.get(positional - 1).map(|(_, t)| t.clone())
                    }
                };
                match want {
                    Some(w) => self.expect(&w, &t, first_pos(&a.value), &format!("argument of `{name}`")),
                    None => self.err(at, format!("`{name}` has no parameter for this argument")),
                }
            }
            return ret;
        }
        match name.as_str() {
            "conservative" => {
                let Some(first) = args.first() else { return Ty::Unknown };
                let xt = self.expr(&first.value);
                let inner = match xt {
                    Ty::Uncertain(t) => *t,
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.err(at, format!("conservative(..) of a known {} value", other.name()));
                        other
                    }
                };
                let mut result = inner.clone();
                for a in &args[1..] {
                    let t = self.expr(&a.value);
                    match self.join(&inner, &t) {
                        Some(j) => {
                            if t == Ty::Nothing {
                                result = Ty::Optional(Box::new(result));
                            } else {
                                let _ = j;
                            }
                        }
                        None => self.err(
                            first_pos(&a.value),
                            format!("`{}:` must be a {}, found {}", a.name.as_deref().unwrap_or("?"), inner.name(), t.name()),
                        ),
                    }
                }
                if let Ty::Optional(x) = &result
                    && let Ty::Optional(_) = **x {
                        result = (**x).clone();
                    }
                result
            }
            "known" => {
                args.iter().for_each(|a| {
                    self.expr(&a.value);
                });
                Ty::Bool
            }
            "solo" => {
                args.iter().for_each(|a| {
                    self.expr(&a.value);
                });
                Ty::Opaque("GroupId".into())
            }
            "days" => {
                for a in args {
                    let t = self.expr(&a.value);
                    self.expect(&Ty::Date, &t, first_pos(&a.value), "days(..)");
                }
                Ty::Int
            }
            // ACTUS cycle: the payment numbers k = 0, 1, .. whose dates
            // add_months(start, k * months) fall on or before `upto`.
            "cycle" => {
                for (a, want) in args.iter().zip([Ty::Date, Ty::Int, Ty::Date]) {
                    let t = self.expr(&a.value);
                    self.expect(&want, &t, first_pos(&a.value), "cycle(start: Date, months: Int, upto: Date)");
                }
                Ty::List(Box::new(Ty::Int))
            }
            "add_months" => {
                for (a, want) in args.iter().zip([Ty::Date, Ty::Int]) {
                    let t = self.expr(&a.value);
                    self.expect(&want, &t, first_pos(&a.value), "add_months(Date, Int)");
                }
                Ty::Date
            }
            "pow" => {
                let t = args.first().map(|a| self.expr(&a.value)).unwrap_or(Ty::Unknown);
                if let Some(k) = args.get(1) {
                    let kt = self.expr(&k.value);
                    self.expect(&Ty::Int, &kt, first_pos(&k.value), "pow(x, k: Int)");
                }
                if !(t.is_numeric() || matches!(t, Ty::Unknown)) {
                    self.err(at, format!("pow of {}", t.name()));
                }
                t
            }
            "bucket" => {
                for a in args {
                    let t = self.expr(&a.value);
                    self.expect(&Ty::Date, &t, first_pos(&a.value), "bucket(..)");
                }
                Ty::Str
            }
            "item" => {
                args.iter().for_each(|a| {
                    self.expr(&a.value);
                });
                Ty::Str
            }
            "min" | "max" => {
                let ts: Vec<Ty> = if args.len() == 1 {
                    vec![self.comprehension(&args[0].value)]
                } else {
                    args.iter().map(|a| self.expr(&a.value)).collect()
                };
                let mut acc = ts[0].clone();
                for t in &ts[1..] {
                    match self.join(&acc, t) {
                        Some(j) => acc = j,
                        None => {
                            self.err(at, format!("{name}(..) of {} and {}", acc.name(), t.name()));
                            acc = Ty::Unknown;
                        }
                    }
                }
                acc
            }
            "sum" => {
                let t = args.first().map(|a| self.comprehension(&a.value)).unwrap_or(Ty::Unknown);
                if !(t.is_numeric() || matches!(t, Ty::Money(..) | Ty::Unknown)) {
                    self.err(at, format!("sum of {}", t.name()));
                }
                t
            }
            "all" => {
                let t = args.first().map(|a| self.comprehension(&a.value)).unwrap_or(Ty::Unknown);
                self.want_bool(&t, at, "all(..)");
                Ty::Bool
            }
            "distinct" => Ty::List(Box::new(args.first().map(|a| self.comprehension(&a.value)).unwrap_or(Ty::Unknown))),
            "latest" => Ty::Optional(Box::new(args.first().map(|a| self.comprehension(&a.value)).unwrap_or(Ty::Unknown))),
            "events" => {
                for a in args {
                    let t = self.expr(&a.value);
                    self.expect(&Ty::Date, &t, first_pos(&a.value), "events(..)");
                }
                Ty::List(Box::new(Ty::AnyEvent))
            }
            "slices" => {
                args.iter().for_each(|a| {
                    self.expr(&a.value);
                });
                let money = self.data.values().flat_map(|fs| fs.iter()).find(|(n, _)| n == "balance").map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown);
                Ty::List(Box::new(Ty::Struct(vec![("amount".into(), money)])))
            }
            _ => {
                args.iter().for_each(|a| {
                    self.expr(&a.value);
                });
                Ty::Unknown
            }
        }
    }
}
