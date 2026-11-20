//! Interpreter for Prudence modules (design.txt §4.A).
//!
//! Executes `view`, `fn`, `alloc ... = optimize` and `partition` declarations
//! over a `World` of records and bitemporal events, with exact rational
//! arithmetic. Runtime guarantees, checked on every run:
//! - `conservative(x, ..)` is the only way to read an unknown value; every
//!   resolution is logged as an assumption;
//! - an `optimize` solution is verified against its constraints (certificate);
//! - exactly one `partition` case matches each portion (exhaustive, disjoint);
//! - `invariant`s hold on the result.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use num_traits::{Signed, Zero};

use crate::date::add_months;
use crate::lp::{Lp, Rel, Sense as LpSense};
use crate::num::{Q, display, parse_decimal};
use crate::syntax::ast::*;
use crate::uncertain::Mode;

// ---------------------------------------------------------------------- values

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Num(Q),
    Bool(bool),
    Str(String),
    /// Day number (days since 1970-01-01).
    Date(i64),
    /// `n.days`
    Days(i64),
    None,
    /// Enum constructor or contract variant: `Demand`, `PAM(maturity, ew)`.
    Ctor(String, Vec<Val>),
    /// A record in the world: (type, index).
    Rec(String, usize),
    Tuple(Vec<Val>),
    List(Vec<Val>),
    /// A view row or slice: named fields.
    Struct(Vec<(String, Val)>),
    /// Keyed map (group views, allocations). Missing keys take `default`.
    Map(BTreeMap<String, Val>, Option<Box<Val>>),
    /// An unknown attribute; `subject.attribute` identifies it in the log.
    Unknown { reason: String, subject: String, attribute: String },
    /// A data type used as a value (`group Deposit by ...`).
    Table(String),
}

impl Val {
    pub fn num(n: i64) -> Val {
        Val::Num(Q::from_integer(n.into()))
    }
}

#[derive(Clone, Debug)]
pub struct Record {
    pub id: String,
    pub fields: Vec<(String, Val)>,
}

impl Record {
    pub fn get(&self, f: &str) -> Option<&Val> {
        self.fields.iter().find(|(n, _)| n == f).map(|(_, v)| v)
    }
}

/// Data and events for one run. Events are records of event types with two
/// extra fields, `valid` and `recorded` (dates).
#[derive(Clone, Debug, Default)]
pub struct World {
    pub tables: BTreeMap<String, Vec<Record>>,
}

impl World {
    pub fn insert(&mut self, ty: &str, rec: Record) -> Val {
        let t = self.tables.entry(ty.to_string()).or_default();
        t.push(rec);
        Val::Rec(ty.to_string(), t.len() - 1)
    }
    pub fn rec(&self, ty: &str, i: usize) -> &Record {
        &self.tables[ty][i]
    }
    pub fn set_field(&mut self, ty: &str, i: usize, f: &str, v: Val) {
        let r = &mut self.tables.get_mut(ty).unwrap()[i];
        match r.fields.iter_mut().find(|(n, _)| n == f) {
            Some(slot) => slot.1 = v,
            None => r.fields.push((f.to_string(), v)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub mode: Mode,
    /// Override the optimisation sense of every `alloc` (e.g. the signed-off
    /// `minimize` policy).
    pub sense: Option<Sense>,
    pub params: Vec<(String, Val)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Slice {
    pub subject: String,
    pub amount: Q,
    /// The split variable's value for this portion (e.g. `covered`).
    pub split: bool,
    pub outputs: Vec<(String, Val)>,
    /// `@cite`s of every rule evaluated for this portion (clause-level provenance).
    pub cites: Vec<String>,
}

impl Slice {
    pub fn output(&self, name: &str) -> Option<&Val> {
        self.outputs.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

/// Result of one allocation problem (one `per` group).
#[derive(Clone, Debug)]
pub struct Allocation {
    pub alloc: String,
    pub group: String,
    /// Marginal objective value per unit of the budget (shadow price).
    pub shadow_price: Option<Q>,
}

#[derive(Debug)]
pub struct Output {
    pub slices: Vec<Slice>,
    /// (subject, attribute) -> (value assumed, reason it was unknown)
    pub assumptions: BTreeMap<(String, String), (String, String)>,
    pub allocations: Vec<Allocation>,
}

type R<T> = Result<T, String>;

// ---------------------------------------------------------------------- interpreter

pub struct Interp<'a> {
    module: &'a Module,
    world: &'a World,
    opts: Options,
    decls: HashMap<&'a str, &'a Decl>,
    params: HashMap<String, Val>,
    ctor_names: BTreeSet<String>,
    event_types: BTreeSet<String>,
    data_types: BTreeSet<String>,
    view_cache: RefCell<HashMap<String, Val>>,
    alloc_cache: RefCell<HashMap<String, Val>>,
    assumptions: RefCell<BTreeMap<(String, String), (String, String)>>,
    trace: RefCell<Option<BTreeSet<String>>>,
    allocations: RefCell<Vec<Allocation>>,
    slices: RefCell<Option<Vec<Slice>>>,
}

type Scope = Vec<(String, Val)>;


fn lookup<'s>(scope: &'s Scope, n: &str) -> Option<&'s Val> {
    scope.iter().rev().find(|(k, _)| k == n).map(|(_, v)| v)
}

impl<'a> Interp<'a> {
    pub fn new(module: &'a Module, world: &'a World, opts: Options) -> R<Self> {
        let mut decls = HashMap::new();
        let mut ctor_names = BTreeSet::new();
        let mut event_types = BTreeSet::new();
        let mut data_types = BTreeSet::new();
        for d in &module.decls {
            decls.insert(d.name.as_str(), d);
            match &d.kind {
                DeclKind::TypeAlias(Type::Union(alts)) => {
                    for a in alts {
                        if let Type::Named { name, .. } = a {
                            ctor_names.insert(name.clone());
                        }
                    }
                }
                DeclKind::Data { base, .. } => {
                    data_types.insert(d.name.clone());
                    if let Some(Type::Named { generics, .. }) = base {
                        for g in generics {
                            let alts = match g {
                                Type::Union(xs) => xs.iter().collect(),
                                other => vec![other],
                            };
                            for a in alts {
                                if let Type::Named { name, .. } = a {
                                    ctor_names.insert(name.clone());
                                }
                            }
                        }
                    }
                }
                DeclKind::Event(_) => {
                    event_types.insert(d.name.clone());
                }
                _ => {}
            }
        }
        let mut it = Interp {
            module,
            world,
            opts,
            decls,
            params: HashMap::new(),
            ctor_names,
            event_types,
            data_types,
            view_cache: RefCell::default(),
            alloc_cache: RefCell::default(),
            assumptions: RefCell::default(),
            trace: RefCell::default(),
            allocations: RefCell::default(),
            slices: RefCell::default(),
        };
        // Parameters, in declaration order (defaults may refer to earlier ones).
        let supplied: HashMap<String, Val> = it.opts.params.iter().cloned().collect();
        for d in &module.decls {
            if let DeclKind::Param { default, .. } = &d.kind {
                let v = match (supplied.get(&d.name), default) {
                    (Some(v), _) => v.clone(),
                    (None, Some(e)) => it.eval(e, &mut Vec::new())?,
                    (None, None) => return Err(format!("parameter `{}` not supplied", d.name)),
                };
                it.params.insert(d.name.clone(), v);
            }
        }
        Ok(it)
    }

    /// Run the module's partition and check its invariants.
    pub fn run(&self) -> R<Output> {
        let part = self
            .module
            .decls
            .iter()
            .find(|d| matches!(d.kind, DeclKind::Partition { .. }))
            .ok_or("module has no partition")?;
        // Evaluate views and allocations up front: per-slice provenance then
        // only reflects the rules applied to that slice.
        for d in &self.module.decls {
            match &d.kind {
                DeclKind::View(b) => {
                    self.view(&d.name, b)?;
                }
                DeclKind::Alloc { ty, body } => {
                    self.alloc(&d.name, ty, body)?;
                }
                _ => {}
            }
        }
        let slices = self.partition(part)?;
        *self.slices.borrow_mut() = Some(slices.clone());
        for d in &self.module.decls {
            if let DeclKind::Invariant(e) = &d.kind
                && self.eval(e, &mut Vec::new())? != Val::Bool(true) {
                    return Err(format!("invariant `{}` violated", d.name));
                }
        }
        Ok(Output {
            slices,
            assumptions: self.assumptions.borrow().clone(),
            allocations: self.allocations.borrow().clone(),
        })
    }

    fn param(&self, n: &str) -> R<&Val> {
        self.params.get(n).ok_or_else(|| format!("no parameter `{n}`"))
    }

    fn rows(&self, ty: &str) -> Vec<Val> {
        let n = self.world.tables.get(ty).map_or(0, |t| t.len());
        (0..n).map(|i| Val::Rec(ty.to_string(), i)).collect()
    }

    /// Human-readable value for logs.
    pub fn render(&self, v: &Val) -> String {
        match v {
            Val::Rec(..) => self.id_of(v),
            Val::Ctor(n, args) if args.is_empty() => n.clone(),
            Val::Bool(b) => b.to_string(),
            Val::None => "none".into(),
            other => show(other),
        }
    }

    fn id_of(&self, v: &Val) -> String {
        match v {
            Val::Rec(t, i) => self.world.rec(t, *i).id.clone(),
            other => key(other),
        }
    }

    // ------------------------------------------------------------------ expressions
    fn eval(&self, e: &Expr, scope: &mut Scope) -> R<Val> {
        match e {
            Expr::Num(n, _) => Ok(Val::Num(parse_decimal(n).ok_or("bad number")?)),
            Expr::Percent(n, _) => Ok(Val::Num(parse_decimal(n).ok_or("bad number")? / Q::from_integer(100.into()))),
            Expr::Str(s, _) => Ok(Val::Str(s.clone())),
            Expr::Cell(..) => Err("cell references are only valid in forms".into()),
            Expr::Annotated(x, anns) => {
                if let Some(t) = self.trace.borrow_mut().as_mut() {
                    for a in anns.iter().filter(|a| a.name == "cite") {
                        if let Some(Expr::Str(c, _)) = a.arg(None) {
                            t.insert(c.clone());
                        }
                    }
                }
                self.eval(x, scope)
            }
            Expr::Ident(n, pos) => self.ident(n, scope).map_err(|m| format!("{pos}: {m}")),
            Expr::List(xs) => Ok(Val::List(xs.iter().map(|x| self.eval(x, scope)).collect::<R<_>>()?)),
            Expr::Tuple(xs) => Ok(Val::Tuple(xs.iter().map(|x| self.eval(x, scope)).collect::<R<_>>()?)),
            Expr::Unary(op, x) => match (op, self.eval(x, scope)?) {
                (UnOp::Not, Val::Bool(b)) => Ok(Val::Bool(!b)),
                (UnOp::Neg, Val::Num(q)) => Ok(Val::Num(-q)),
                (_, v) => Err(format!("bad operand for {op:?}: {}", show(&v))),
            },
            Expr::Binary(op, a, b) => self.binary(*op, a, b, scope),
            Expr::Member(x, f) => {
                let v = self.eval(x, scope)?;
                self.member(&v, f)
            }
            Expr::Index(x, k) => {
                let m = self.eval(x, scope)?;
                let k = self.eval(k, scope)?;
                match m {
                    Val::Map(entries, default) => entries
                        .get(&key(&k))
                        .cloned()
                        .or(default.map(|d| *d))
                        .ok_or_else(|| format!("no entry for {}", show(&k))),
                    other => Err(format!("cannot index {}", show(&other))),
                }
            }
            Expr::Is(x, ty) => Ok(Val::Bool(matches!(self.eval(x, scope)?, Val::Rec(t, _) if &t == ty))),
            Expr::If { cond, then, other } => match self.eval(cond, scope)? {
                Val::Bool(true) => self.eval(then, scope),
                Val::Bool(false) => self.eval(other, scope),
                v => Err(format!("`if` needs a Bool, got {}", show(&v))),
            },
            Expr::IfLet { pat, value, then, other } => {
                let v = self.eval(value, scope)?;
                let mark = scope.len();
                let hit = self.bind(pat, &v, scope);
                let out = if hit { self.eval(then, scope) } else { Ok(Val::None) };
                scope.truncate(mark);
                if hit { out } else { self.eval(other, scope) }
            }
            Expr::Let { pat, value, body } => {
                let v = self.eval(value, scope)?;
                let mark = scope.len();
                if !self.bind(pat, &v, scope) {
                    return Err("`let` pattern did not match".into());
                }
                let out = self.eval(body, scope);
                scope.truncate(mark);
                out
            }
            Expr::Match { scrutinee, arms } => {
                let v = self.eval(scrutinee, scope)?;
                for arm in arms {
                    let mark = scope.len();
                    if self.bind(&arm.pat, &v, scope) {
                        let guard_ok = match &arm.guard {
                            None => true,
                            Some(g) => self.eval(g, scope)? == Val::Bool(true),
                        };
                        if guard_ok {
                            let out = self.eval(&arm.body, scope);
                            scope.truncate(mark);
                            return out;
                        }
                    }
                    scope.truncate(mark);
                }
                Err(format!("no match arm for {}", show(&v)))
            }
            Expr::Call(f, args) => self.call(f, args, scope),
            Expr::Forall { var, domain, body } => {
                let items = match domain {
                    Domain::Type(Type::Named { name, .. }) => self.rows(name),
                    Domain::Type(_) => return Err("forall over a union type".into()),
                    Domain::In(src) => list(self.eval(src, scope)?)?,
                };
                for x in items {
                    scope.push((var.clone(), x));
                    let r = self.eval(body, scope);
                    scope.pop();
                    if r? != Val::Bool(true) {
                        return Ok(Val::Bool(false));
                    }
                }
                Ok(Val::Bool(true))
            }
            Expr::Comprehension { .. } => Err("a comprehension must be the argument of an aggregate".into()),
            Expr::Group { .. } | Expr::Optimize { .. } => {
                Err("`group` and `optimize` are only valid as a view or alloc body".into())
            }
        }
    }

    fn ident(&self, n: &str, scope: &Scope) -> R<Val> {
        if let Some(v) = lookup(scope, n) {
            return Ok(v.clone());
        }
        if let Some(v) = self.params.get(n) {
            return Ok(v.clone());
        }
        match n {
            "true" => return Ok(Val::Bool(true)),
            "false" => return Ok(Val::Bool(false)),
            "none" => return Ok(Val::None),
            "Conservative" | "Optimistic" => return Ok(Val::Ctor(n.into(), vec![])),
            _ => {}
        }
        if let Some(d) = self.decls.get(n) {
            match &d.kind {
                DeclKind::View(body) => return self.view(n, body),
                DeclKind::Alloc { ty, body } => return self.alloc(n, ty, body),
                DeclKind::Data { .. } => return Ok(Val::Table(n.into())),
                _ => {}
            }
        }
        if self.ctor_names.contains(n) {
            return Ok(Val::Ctor(n.into(), vec![]));
        }
        Err(format!("unbound name `{n}`"))
    }

    fn member(&self, v: &Val, f: &str) -> R<Val> {
        match v {
            Val::Rec(t, i) => self
                .world
                .rec(t, *i)
                .get(f)
                .cloned()
                .ok_or_else(|| format!("{t} has no field `{f}`")),
            Val::Struct(fields) => {
                fields.iter().find(|(n, _)| n == f).map(|(_, v)| v.clone()).ok_or(format!("no field `{f}`"))
            }
            Val::Num(q) if f == "days" && q.is_integer() => Ok(Val::Days(i64::try_from(q.to_integer()).map_err(|e| e.to_string())?)),
            // Currency/basis tags on literals (`250_000.AUD`) are erased at run time;
            // the type checker enforces them.
            Val::Num(q) if f.chars().all(|c| c.is_ascii_uppercase()) => Ok(Val::Num(q.clone())),
            Val::Table(t) => Ok(Val::List(
                self.rows(t).into_iter().map(|r| self.member(&r, f)).collect::<R<Vec<_>>>()?,
            )),
            other => Err(format!("cannot read `.{f}` of {}", show(other))),
        }
    }

    fn binary(&self, op: BinOp, a: &Expr, b: &Expr, scope: &mut Scope) -> R<Val> {
        use BinOp::*;
        match op {
            And | Or | Implies => {
                let l = truth(self.eval(a, scope)?)?;
                let short = match op {
                    And => !l,
                    Or => l,
                    _ => !l,
                };
                if short {
                    return Ok(Val::Bool(op == Or || op == Implies));
                }
                Ok(Val::Bool(truth(self.eval(b, scope)?)?))
            }
            _ => {
                let l = self.eval(a, scope)?;
                let r = self.eval(b, scope)?;
                match (op, &l, &r) {
                    (Add, Val::Num(x), Val::Num(y)) => Ok(Val::Num(x + y)),
                    (Sub, Val::Num(x), Val::Num(y)) => Ok(Val::Num(x - y)),
                    (Mul, Val::Num(x), Val::Num(y)) => Ok(Val::Num(x * y)),
                    (Div, Val::Num(_), Val::Num(y)) if y.is_zero() => Err("division by zero".into()),
                    (Div, Val::Num(x), Val::Num(y)) => Ok(Val::Num(x / y)),
                    (Add, Val::Date(d), Val::Days(n)) => Ok(Val::Date(d + n)),
                    (Eq, ..) => Ok(Val::Bool(self.same(&l, &r))),
                    (Ne, ..) => Ok(Val::Bool(!self.same(&l, &r))),
                    (In, _, Val::List(xs)) => Ok(Val::Bool(xs.iter().any(|x| self.same(&l, x)))),
                    (Lt | Le | Gt | Ge, ..) => {
                        let ord = match (&l, &r) {
                            (Val::Num(x), Val::Num(y)) => x.cmp(y),
                            (Val::Date(x), Val::Date(y)) => x.cmp(y),
                            // An absent optional date compares false (e.g. no grace period).
                            (Val::None, _) | (_, Val::None) => return Ok(Val::Bool(false)),
                            _ => return Err(format!("cannot compare {} and {}", show(&l), show(&r))),
                        };
                        Ok(Val::Bool(match op {
                            Lt => ord.is_lt(),
                            Le => ord.is_le(),
                            Gt => ord.is_gt(),
                            _ => ord.is_ge(),
                        }))
                    }
                    _ => Err(format!("bad operands for {op:?}: {} and {}", show(&l), show(&r))),
                }
            }
        }
    }

    fn same(&self, a: &Val, b: &Val) -> bool {
        match (a, b) {
            (Val::Unknown { .. }, _) | (_, Val::Unknown { .. }) => false,
            _ => a == b,
        }
    }

    fn bind(&self, p: &Pat, v: &Val, scope: &mut Scope) -> bool {
        match (p, v) {
            (Pat::Wild, _) => true,
            (Pat::Bind(n), _) => {
                scope.push((n.clone(), v.clone()));
                true
            }
            (Pat::Tuple(ps), Val::Tuple(vs)) if ps.len() == vs.len() => {
                ps.iter().zip(vs).all(|(p, v)| self.bind(p, v, scope))
            }
            (Pat::Ctor(n, ps), Val::Ctor(m, vs)) => {
                n == m && ps.len() == vs.len() && ps.iter().zip(vs).all(|(p, v)| self.bind(p, v, scope))
            }
            // Event records match positionally on their declared fields.
            (Pat::Ctor(n, ps), Val::Rec(t, i)) if n == t => {
                let rec = self.world.rec(t, *i);
                ps.len() <= rec.fields.len()
                    && ps.iter().zip(&rec.fields).all(|(p, (_, v))| self.bind(p, v, scope))
            }
            _ => false,
        }
    }

    fn args_named(&self, args: &[Arg], scope: &mut Scope) -> R<(Vec<Val>, HashMap<String, Val>)> {
        let (mut pos, mut named) = (Vec::new(), HashMap::new());
        for a in args {
            if matches!(a.value, Expr::Comprehension { .. }) {
                continue;
            }
            let v = self.eval(&a.value, scope)?;
            match &a.name {
                Some(n) => {
                    named.insert(n.clone(), v);
                }
                None => pos.push(v),
            }
        }
        Ok((pos, named))
    }

    /// Elements produced by a comprehension argument, or by a plain list argument.
    fn elements(&self, args: &[Arg], scope: &mut Scope) -> R<Vec<Val>> {
        match args {
            [Arg { value: Expr::Comprehension { var, source, filter, body }, .. }] => {
                let items = list(self.eval(source, scope)?)?;
                let mut out = Vec::new();
                for x in items {
                    scope.push((var.clone(), x.clone()));
                    let keep = match filter {
                        None => true,
                        Some(f) => truth(self.eval(f, scope)?)?,
                    };
                    let r = if keep {
                        Some(match body {
                            Some(b) => self.eval(b, scope),
                            None => Ok(x),
                        })
                    } else {
                        None
                    };
                    scope.pop();
                    if let Some(r) = r {
                        out.push(r?);
                    }
                }
                Ok(out)
            }
            [Arg { value, .. }] => list(self.eval(value, scope)?),
            _ => Err("aggregate takes one argument".into()),
        }
    }

    fn call(&self, f: &Expr, args: &[Arg], scope: &mut Scope) -> R<Val> {
        let Expr::Ident(name, pos) = f else { return Err("only named functions can be called".into()) };
        let at = |m: String| format!("{pos}: {m}");
        if let Some(d) = self.decls.get(name.as_str())
            && let DeclKind::Fn { params, body, .. } = &d.kind {
                let body = body.as_ref().ok_or_else(|| at(format!("`{name}` is extern: no executable body")))?;
                let (pos_args, named) = self.args_named(args, scope)?;
                let mut fs: Scope = Vec::new();
                let mut it = pos_args.into_iter();
                for p in params {
                    let v = named.get(&p.name).cloned().or_else(|| it.next());
                    fs.push((p.name.clone(), v.ok_or_else(|| at(format!("missing argument `{}`", p.name)))?));
                }
                return self.eval(body, &mut fs);
            }
        if self.event_types.contains(name) || self.data_types.contains(name) {
            return Err(at(format!("cannot construct `{name}` in a rule")));
        }
        if self.ctor_names.contains(name) {
            let (vals, _) = self.args_named(args, scope)?;
            return Ok(Val::Ctor(name.clone(), vals));
        }
        match name.as_str() {
            "conservative" => {
                let x = self.eval(&args.first().ok_or("conservative(x, worst: ..)")?.value, scope)?;
                let (_, named) = self.args_named(&args[1..], scope)?;
                let Val::Unknown { subject, attribute, reason } = x else { return Ok(x) };
                let worst = named.get("worst").cloned().ok_or_else(|| at("conservative(..) needs `worst:`".into()))?;
                let best = match (named.get("best"), &worst) {
                    (Some(b), _) => b.clone(),
                    (None, Val::Bool(w)) => Val::Bool(!w),
                    (None, _) => return Err(at("conservative(..) needs `best:` for non-Bool values".into())),
                };
                let v = match self.opts.mode {
                    Mode::Conservative => worst,
                    Mode::Optimistic => best,
                };
                self.assumptions.borrow_mut().insert((subject, attribute), (self.render(&v), reason));
                Ok(v)
            }
            // The group consisting of just `x` (a fresh GroupId).
            "solo" => {
                let (v, _) = self.args_named(args, scope)?;
                Ok(Val::Str(format!("solo:{}", self.id_of(v.first().ok_or("solo(x)")?))))
            }
            "known" => {
                let (v, _) = self.args_named(args, scope)?;
                Ok(Val::Bool(!matches!(v.first(), Some(Val::Unknown { .. }))))
            }
            "days" => match self.args_named(args, scope)?.0.as_slice() {
                [Val::Date(a), Val::Date(b)] => Ok(Val::num(b - a)),
                other => Err(at(format!("days(Date, Date), got {other:?}"))),
            },
            "item" => match self.args_named(args, scope)?.0.as_slice() {
                [Val::Str(p), Val::Num(n)] => Ok(Val::Str(format!("{p}.{n}"))),
                other => Err(at(format!("item(String, Int), got {other:?}"))),
            },
            "bucket" => match self.args_named(args, scope)?.0.as_slice() {
                [Val::Date(d)] => {
                    let Val::Date(rd) = self.param("reporting_date")?.clone() else { return Err("reporting_date".into()) };
                    let b = if *d < add_months(rd, 6) {
                        "b0"
                    } else if *d < add_months(rd, 12) {
                        "b1"
                    } else {
                        "b2"
                    };
                    Ok(Val::Str(b.into()))
                }
                other => Err(at(format!("bucket(Date), got {other:?}"))),
            },
            "min" | "max" => {
                let vals = if args.len() == 1 { self.elements(args, scope)? } else { self.args_named(args, scope)?.0 };
                let nums = vals.iter().map(num).collect::<R<Vec<_>>>()?;
                let pick = if name == "min" { nums.into_iter().min() } else { nums.into_iter().max() };
                pick.map(Val::Num).ok_or_else(|| at(format!("{name} of nothing")))
            }
            "sum" => Ok(Val::Num(self.elements(args, scope)?.iter().map(num).sum::<R<Q>>()?)),
            "all" => Ok(Val::Bool(self.elements(args, scope)?.into_iter().map(truth).collect::<R<Vec<_>>>()?.into_iter().all(|b| b))),
            "distinct" => {
                let mut out: Vec<Val> = Vec::new();
                for v in self.elements(args, scope)? {
                    if !out.contains(&v) {
                        out.push(v);
                    }
                }
                Ok(Val::List(out))
            }
            "latest" => {
                // Latest by `valid` date; ties go to the later-recorded element.
                let mut best: Option<(i64, Val)> = None;
                for v in self.elements(args, scope)? {
                    let at = match self.member(&v, "valid") {
                        Ok(Val::Date(d)) => d,
                        _ => i64::MIN,
                    };
                    if best.as_ref().is_none_or(|(b, _)| at >= *b) {
                        best = Some((at, v));
                    }
                }
                Ok(best.map(|(_, v)| v).unwrap_or(Val::None))
            }
            "events" => {
                let (_, named) = self.args_named(args, scope)?;
                let (Some(Val::Date(as_of)), Some(Val::Date(known))) = (named.get("as_of"), named.get("known_at")) else {
                    return Err(at("events(as_of: Date, known_at: Date)".into()));
                };
                let mut out = Vec::new();
                for ty in &self.event_types {
                    for r in self.rows(ty) {
                        let (Ok(Val::Date(v)), Ok(Val::Date(k))) = (self.member(&r, "valid"), self.member(&r, "recorded")) else {
                            return Err(format!("event {ty} lacks valid/recorded dates"));
                        };
                        if v <= *as_of && k <= *known {
                            out.push(r);
                        }
                    }
                }
                Ok(Val::List(out))
            }
            "slices" => {
                let (v, _) = self.args_named(args, scope)?;
                let id = self.id_of(v.first().ok_or("slices(x)")?);
                let all = self.slices.borrow();
                let all = all.as_ref().ok_or("slices(..) is only available after the partition has run")?;
                Ok(Val::List(
                    all.iter()
                        .filter(|s| s.subject == id)
                        .map(|s| Val::Struct(vec![("amount".into(), Val::Num(s.amount.clone()))]))
                        .collect(),
                ))
            }
            other => Err(at(format!("unknown function `{other}`"))),
        }
    }

    // ------------------------------------------------------------------ views
    fn view(&self, name: &str, body: &Expr) -> R<Val> {
        if let Some(v) = self.view_cache.borrow().get(name) {
            return Ok(v.clone());
        }
        let v = match body {
            Expr::Annotated(x, _) => return self.view(name, x),
            Expr::Group { source, key: kexpr, filter, fields } => {
                let Val::Table(ty) = self.eval(source, &mut Vec::new())? else {
                    return Err("group source must be a data type".into());
                };
                let mut groups: BTreeMap<String, Vec<Val>> = BTreeMap::new();
                for row in self.rows(&ty) {
                    let mut scope = self.row_scope(&row);
                    if let Some(f) = filter
                        && !truth(self.eval(f, &mut scope)?)? {
                            continue;
                        }
                    let k = self.eval(kexpr, &mut scope)?;
                    if k == Val::None {
                        continue; // no key: not in any group
                    }
                    groups.entry(key(&k)).or_default().push(row);
                }
                let mut out = BTreeMap::new();
                for (k, rows) in groups {
                    let mut fs = Vec::new();
                    for (fname, fexpr) in fields {
                        fs.push((fname.clone(), self.aggregate_rows(fexpr, &rows)?));
                    }
                    out.insert(k, Val::Struct(fs));
                }
                Val::Map(out, None)
            }
            other => self.eval(other, &mut Vec::new())?,
        };
        self.view_cache.borrow_mut().insert(name.into(), v.clone());
        Ok(v)
    }

    fn row_scope(&self, row: &Val) -> Scope {
        let Val::Rec(t, i) = row else { return vec![] };
        self.world.rec(t, *i).fields.clone()
    }

    /// `sum(balance)` inside a group: evaluate the argument per row, then aggregate.
    fn aggregate_rows(&self, e: &Expr, rows: &[Val]) -> R<Val> {
        let Expr::Call(f, args) = e else { return Err("group fields must be aggregates".into()) };
        let Expr::Ident(agg, _) = &**f else { return Err("group fields must be aggregates".into()) };
        let [Arg { value, .. }] = args.as_slice() else { return Err("aggregate takes one argument".into()) };
        let vals = rows.iter().map(|r| self.eval(value, &mut self.row_scope(r))).collect::<R<Vec<_>>>()?;
        match agg.as_str() {
            "sum" => Ok(Val::Num(vals.iter().map(num).sum::<R<Q>>()?)),
            "count" => Ok(Val::num(vals.len() as i64)),
            other => Err(format!("unsupported group aggregate `{other}`")),
        }
    }

    // ------------------------------------------------------------------ allocation (optimize)
    fn alloc(&self, name: &str, ty: &Type, body: &Expr) -> R<Val> {
        if let Some(v) = self.alloc_cache.borrow().get(name) {
            return Ok(v.clone());
        }
        let Type::Named { params, .. } = ty else { return Err("alloc type must be ConservationSplit(..)".into()) };
        // ConservationSplit(Source.field, per: key)
        let (src_ty, per_field) = match params.as_slice() {
            [TypeParam::Expr(Expr::Member(src, _)), TypeParam::Field(p, Type::Named { name: key, .. })] if p == "per" => {
                let Expr::Ident(t, _) = &**src else { return Err("ConservationSplit(Type.field, per: key)".into()) };
                (t.clone(), key.clone())
            }
            _ => return Err("expected ConservationSplit(Type.field, per: key)".into()),
        };
        let mut body = body;
        while let Expr::Annotated(x, _) = body {
            body = x;
        }
        let Expr::Optimize { per, filter, sense, objective, constraints } = body else {
            return Err("alloc body must be `optimize`".into());
        };
        let sense = self.opts.sense.unwrap_or(*sense);

        // Groups: distinct values of `per_field` over the source rows.
        let mut groups: Vec<Val> = Vec::new();
        for r in self.rows(&src_ty) {
            let g = self.member(&r, &per_field)?;
            if !groups.contains(&g) {
                groups.push(g);
            }
        }
        let mut result: BTreeMap<String, Val> = BTreeMap::new();
        for g in groups {
            let mut scope: Scope = vec![(per.clone(), g.clone())];
            if let Some(f) = filter
                && !truth(self.eval(f, &mut scope)?)?
            {
                continue;
            }
            // Decision variables: this group's rows, in subject-id order (the
            // order also breaks ties between optimal solutions).
            let mut rows: Vec<Val> = Vec::new();
            for r in self.rows(&src_ty) {
                if self.member(&r, &per_field)? == g {
                    rows.push(r);
                }
            }
            rows.sort_by_key(|r| self.id_of(r));
            let vars: Vec<String> = rows.iter().map(key).collect();
            let at = |m: String| format!("{name} for {}: {m}", self.id_of(&g));

            let (objective_coef, _) = self.linear(name, &vars, objective, &mut scope).map_err(at)?;
            let mut atoms = Vec::new();
            for c in constraints {
                self.flatten(c, &mut scope, &mut atoms).map_err(at)?;
            }
            let mut lp_rows = Vec::new();
            for (op, l, r, mut sc) in atoms {
                let (lc, l0) = self.linear(name, &vars, l, &mut sc).map_err(at)?;
                let (rc, r0) = self.linear(name, &vars, r, &mut sc).map_err(at)?;
                let coef: Vec<Q> = lc.iter().zip(&rc).map(|(a, b)| a - b).collect();
                let rhs = r0 - l0;
                let rel = match op {
                    BinOp::Le => Rel::Le,
                    BinOp::Ge => Rel::Ge,
                    BinOp::Eq => Rel::Eq,
                    other => return Err(at(format!("{other:?}: only <=, >= and == are linear constraints"))),
                };
                if coef.iter().all(|c| c.is_zero()) {
                    let holds = match rel {
                        Rel::Le => Q::zero() <= rhs,
                        Rel::Ge => Q::zero() >= rhs,
                        Rel::Eq => rhs.is_zero(),
                    };
                    if !holds {
                        return Err(at("infeasible (a constant constraint is false)".into()));
                    }
                    continue;
                }
                lp_rows.push((coef, rel, rhs));
            }
            let lp = Lp {
                sense: if sense == Sense::Maximize { LpSense::Max } else { LpSense::Min },
                objective: objective_coef,
                rows: lp_rows,
            };
            // Exact simplex; the primal-dual certificate is verified inside.
            let sol = crate::lp::solve_lexicographic(&lp).map_err(at)?;
            let shadow_price =
                lp.rows.iter().zip(&sol.duals).find(|((_, r, _), _)| *r == Rel::Eq).map(|(_, y)| y.clone());
            for (v, q) in vars.iter().zip(sol.x) {
                result.insert(v.clone(), Val::Num(q));
            }
            self.allocations.borrow_mut().push(Allocation { alloc: name.into(), group: self.id_of(&g), shadow_price });
        }
        let v = Val::Map(result, Some(Box::new(Val::num(0))));
        self.alloc_cache.borrow_mut().insert(name.into(), v.clone());
        Ok(v)
    }

    /// Value of `e` with the allocation `a` set to `assign` (missing = 0).
    fn eval_with(&self, a: &str, vars: &[String], assign: &[Q], e: &Expr, scope: &mut Scope) -> R<Q> {
        let map: BTreeMap<String, Val> = vars.iter().cloned().zip(assign.iter().cloned().map(Val::Num)).collect();
        scope.push((a.to_string(), Val::Map(map, Some(Box::new(Val::num(0))))));
        let v = self.eval(e, scope);
        scope.pop();
        num(&v?)
    }

    /// `e` as a linear form `coef · x + constant` in the allocation variables,
    /// found by evaluation and checked at an extra point.
    fn linear(&self, a: &str, vars: &[String], e: &Expr, scope: &mut Scope) -> R<(Vec<Q>, Q)> {
        let n = vars.len();
        let zero = vec![Q::zero(); n];
        let c0 = self.eval_with(a, vars, &zero, e, scope)?;
        let mut coef = Vec::with_capacity(n);
        for k in 0..n {
            let mut unit = zero.clone();
            unit[k] = Q::from_integer(1.into());
            coef.push(self.eval_with(a, vars, &unit, e, scope)? - &c0);
        }
        let probe: Vec<Q> = (0..n).map(|k| Q::from_integer((k as i64 + 2).into())).collect();
        let want: Q = &c0 + coef.iter().zip(&probe).map(|(c, p)| c * p).sum::<Q>();
        if self.eval_with(a, vars, &probe, e, scope)? != want {
            return Err(format!("not linear in `{a}`"));
        }
        Ok((coef, c0))
    }

    /// Flatten `&&`, chained comparisons and `forall x in S, ..` into atomic
    /// comparisons, each with the scope it is evaluated in.
    fn flatten<'e>(&self, e: &'e Expr, scope: &mut Scope, out: &mut Vec<(BinOp, &'e Expr, &'e Expr, Scope)>) -> R<()> {
        match e {
            Expr::Annotated(x, _) => self.flatten(x, scope, out),
            Expr::Binary(BinOp::And, a, b) => {
                self.flatten(a, scope, out)?;
                self.flatten(b, scope, out)
            }
            Expr::Binary(op @ (BinOp::Le | BinOp::Ge | BinOp::Eq | BinOp::Lt | BinOp::Gt), a, b) => {
                out.push((*op, a, b, scope.clone()));
                Ok(())
            }
            Expr::Forall { var, domain: Domain::In(src), body } => {
                let mut items = list(self.eval(src, scope)?)?;
                items.sort_by_key(|x| self.id_of(x));
                for x in items {
                    scope.push((var.clone(), x));
                    let r = self.flatten(body, scope, out);
                    scope.pop();
                    r?;
                }
                Ok(())
            }
            _ => Err("constraints must be comparisons, `&&` or `forall x in S, ..`".into()),
        }
    }

    // ------------------------------------------------------------------ partition
    fn partition(&self, d: &Decl) -> R<Vec<Slice>> {
        let DeclKind::Partition { var, source, split, split_var, split_by, cases, .. } = &d.kind else { unreachable!() };
        let Type::Named { name: src, .. } = source else { return Err("partition source must be a data type".into()) };
        let mut out = Vec::new();
        let mut rows = self.rows(src);
        rows.sort_by_key(|r| self.id_of(r));
        for row in rows {
            let subject = self.id_of(&row);
            let mut scope: Scope = vec![(var.clone(), row.clone())];
            let whole = num(&self.eval(split, &mut scope)?)?;
            let part = num(&self.eval(split_by, &mut scope)?)?;
            let rest = &whole - &part;
            if part.is_negative() || rest.is_negative() {
                return Err(format!("{subject}: split {} of {} is out of range", display(&part), display(&whole)));
            }
            let mut portions = Vec::new();
            if !part.is_zero() {
                portions.push((true, part));
            }
            if !rest.is_zero() || portions.is_empty() {
                portions.push((false, rest));
            }
            for (flag, amount) in portions {
                scope.push((split_var.clone(), Val::Bool(flag)));
                *self.trace.borrow_mut() = Some(BTreeSet::new());
                let mut hit = None;
                for (ci, c) in cases.iter().enumerate() {
                    if truth(self.eval(&c.cond, &mut scope)?)? {
                        if let Some(prev) = hit {
                            return Err(format!("{subject}: partition cases {prev} and {ci} overlap"));
                        }
                        hit = Some(ci);
                    }
                }
                let ci = hit.ok_or_else(|| format!("{subject}: no partition case matches"))?;
                let c = &cases[ci];
                let mark = scope.len();
                for (p, e) in &c.lets {
                    let v = self.eval(e, &mut scope)?;
                    if !self.bind(p, &v, &mut scope) {
                        return Err("case `let` pattern did not match".into());
                    }
                }
                let outputs = c
                    .outputs
                    .iter()
                    .map(|(n, e)| Ok((n.clone(), self.eval(e, &mut scope)?)))
                    .collect::<R<Vec<_>>>()?;
                scope.truncate(mark);
                scope.pop();
                let cites = self.trace.borrow_mut().take().unwrap_or_default().into_iter().collect();
                out.push(Slice { subject: subject.clone(), amount, split: flag, outputs, cites });
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------- helpers

fn truth(v: Val) -> R<bool> {
    match v {
        Val::Bool(b) => Ok(b),
        Val::Unknown { subject, attribute, .. } => {
            Err(format!("{subject}.{attribute} is unknown: resolve it with conservative(..) before use"))
        }
        other => Err(format!("expected a Bool, got {}", show(&other))),
    }
}

fn num(v: &Val) -> R<Q> {
    match v {
        Val::Num(q) => Ok(q.clone()),
        Val::Unknown { subject, attribute, .. } => {
            Err(format!("{subject}.{attribute} is unknown: resolve it with conservative(..) before use"))
        }
        other => Err(format!("expected a number, got {}", show(other))),
    }
}

fn list(v: Val) -> R<Vec<Val>> {
    match v {
        Val::List(xs) => Ok(xs),
        other => Err(format!("expected a list, got {}", show(&other))),
    }
}

/// Canonical key for maps and allocations.
pub fn key(v: &Val) -> String {
    match v {
        Val::Rec(t, i) => format!("{t}#{i}"),
        Val::Str(s) => format!("s:{s}"),
        Val::Num(q) => format!("n:{q}"),
        other => format!("{other:?}"),
    }
}

fn show(v: &Val) -> String {
    match v {
        Val::Num(q) => display(q),
        Val::Unknown { subject, attribute, .. } => format!("unknown {subject}.{attribute}"),
        other => format!("{other:?}"),
    }
}
