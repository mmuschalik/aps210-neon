//! Lean 4 backend: compile a Prudence module to Lean definitions
//! (design.txt §4.A: "core IR ... embedded directly in Lean 4").
//!
//! What compiles to Lean code: enum types, data records, ACTUS contract
//! variants, events, and every `fn` whose body is pure computation; the
//! `partition`'s outputs; and, given the target forms' tables, the measure of
//! each target (e.g. LCR outflow and ASF per subject).
//!
//! What stays abstract, as fields of a `World` structure: parameters; views;
//! `fn`s over events (`events(..)`, `latest(..)`); `alloc ... = optimize`
//! solutions; list-valued record fields that would make records cyclic; and the
//! calendar built-ins `bucket` and `solo`. A field whose value depends on the
//! reporting mode takes the `Mode`. Proofs about the generated code state what
//! they need of these fields as hypotheses.
//!
//! Generated code is total: comparisons with an absent optional value are
//! false (as in the interpreter), a guarded `match` falls through to the
//! remaining arms, and the last `partition` case is the `else` branch (its
//! condition is implied by the Z3 exhaustiveness obligation).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use super::ast::*;
use super::typecheck::{Ty, view_types};

/// How a partition output is measured on its target form.
pub enum Measure {
    /// Item id (or `none`) -> integer percentage weight.
    Weight(BTreeMap<String, i64>),
    /// (item id, maturity bucket) -> integer percentage factor.
    Factor(BTreeMap<(String, u32), i64>),
}

type R<T> = Result<T, String>;

/// Renders an aggregate from (variable, source, filter, body).
type CompFn<'a> = dyn Fn(&str, &str, Option<String>, String) -> String + 'a;

struct Gen<'m> {
    m: &'m Module,
    enums: BTreeMap<String, Vec<String>>,
    ctor_types: HashMap<String, BTreeSet<String>>,
    contract_of: HashMap<String, String>, // contract ctor -> Lean inductive
    contract_fields: HashMap<String, Vec<String>>,
    events: BTreeMap<String, Vec<(String, Type)>>,
    data: BTreeMap<String, Vec<(String, Type)>>,
    cyclic: BTreeMap<(String, String), String>, // (data, field) -> world fn name
    params: BTreeSet<String>,
    fns: BTreeSet<String>,       // compiled fns
    world_fns: BTreeSet<String>, // abstract fns/views/allocs (world fields)
    moded: BTreeSet<String>,     // world fields taking Mode
}

fn kw(n: &str) -> String {
    const RESERVED: [&str; 12] = ["class", "match", "fun", "def", "let", "if", "then", "else", "do", "end", "where", "structure"];
    if RESERVED.contains(&n) { format!("«{n}»") } else { n.to_string() }
}

fn strip(e: &Expr) -> &Expr {
    match e {
        Expr::Annotated(x, _) => strip(x),
        other => other,
    }
}

impl<'m> Gen<'m> {
    fn new(m: &'m Module) -> Self {
        let mut g = Gen {
            m,
            enums: BTreeMap::new(),
            ctor_types: HashMap::new(),
            contract_of: HashMap::new(),
            contract_fields: HashMap::new(),
            events: BTreeMap::new(),
            data: BTreeMap::new(),
            cyclic: BTreeMap::new(),
            params: BTreeSet::new(),
            fns: BTreeSet::new(),
            world_fns: BTreeSet::new(),
            moded: BTreeSet::new(),
        };
        for d in &m.decls {
            match &d.kind {
                DeclKind::TypeAlias(Type::Union(alts)) => {
                    let cs: Vec<String> =
                        alts.iter().filter_map(|a| if let Type::Named { name, .. } = a { Some(name.clone()) } else { None }).collect();
                    for c in &cs {
                        g.ctor_types.entry(c.clone()).or_default().insert(d.name.clone());
                    }
                    g.enums.insert(d.name.clone(), cs);
                }
                DeclKind::Data { base, fields } => {
                    let mut fs: Vec<(String, Type)> = fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect();
                    if let Some(Type::Named { name, generics, .. }) = base
                        && name == "Contract"
                    {
                        let ind = format!("Contract_{}", d.name);
                        for gt in generics {
                            let alts = match gt {
                                Type::Union(xs) => xs.iter().collect(),
                                other => vec![other],
                            };
                            for a in alts {
                                if let Type::Named { name: v, params, .. } = a {
                                    g.contract_of.insert(v.clone(), ind.clone());
                                    g.contract_fields.insert(
                                        v.clone(),
                                        params.iter().filter_map(|p| if let TypeParam::Field(n, _) = p { Some(n.clone()) } else { None }).collect(),
                                    );
                                }
                            }
                        }
                        let pos = super::lexer::Pos { line: 0, col: 0 };
                        fs.push(("contract".into(), Type::Named { name: ind, generics: vec![], params: vec![], pos }));
                    }
                    g.data.insert(d.name.clone(), fs);
                }
                DeclKind::Event(fields) => {
                    g.events.insert(d.name.clone(), fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect());
                }
                DeclKind::Param { .. } => {
                    g.params.insert(d.name.clone());
                }
                _ => {}
            }
        }
        // List-valued fields referring to data types would make records cyclic:
        // they become world functions.
        for (dn, fs) in &g.data {
            for (f, t) in fs {
                if let Type::Named { name, generics, .. } = t
                    && name == "List"
                    && matches!(generics.first(), Some(Type::Named { name: inner, .. }) if g.data_names().contains(inner))
                {
                    g.cyclic.insert((dn.clone(), f.clone()), format!("{dn}_{f}"));
                }
            }
        }
        g
    }

    fn data_names(&self) -> BTreeSet<String> {
        self.m.decls.iter().filter(|d| matches!(d.kind, DeclKind::Data { .. })).map(|d| d.name.clone()).collect()
    }

    fn decl(&self, n: &str) -> Option<&Decl> {
        self.m.decls.iter().find(|d| d.name == n)
    }

    // ------------------------------------------------------------------ types
    fn ty(&self, t: &Type) -> R<String> {
        match t {
            Type::Union(_) => Err("union types only in type declarations".into()),
            Type::Named { name, generics, .. } => {
                let g = |i: usize| generics.get(i).map(|t| self.ty(t)).unwrap_or(Ok("Unit".into()));
                Ok(match name.as_str() {
                    "Bool" => "Bool".into(),
                    "String" => "String".into(),
                    "Int" | "Date" | "Rate" | "Money" => "Int".into(),
                    "GroupId" | "ProductClass" | "Mode" => name.clone(),
                    "Uncertain" => format!("U ({})", g(0)?),
                    "Option" => format!("Option ({})", g(0)?),
                    "List" => format!("List ({})", g(0)?),
                    other => other.to_string(),
                })
            }
        }
    }

    fn ty_of(&self, t: &Ty) -> String {
        match t {
            Ty::Bool => "Bool".into(),
            Ty::Int | Ty::Num | Ty::Rate | Ty::Money(..) | Ty::Date => "Int".into(),
            Ty::Str => "String".into(),
            Ty::Opaque(n) | Ty::Enum(n) | Ty::Data(n) | Ty::Event(n) => n.clone(),
            Ty::List(x) => format!("List ({})", self.ty_of(x)),
            Ty::Optional(x) => format!("Option ({})", self.ty_of(x)),
            Ty::Uncertain(x) => format!("U ({})", self.ty_of(x)),
            other => format!("Unit /- {other:?} -/"),
        }
    }

    // ------------------------------------------------------------------ mode dependence
    fn uses_mode(&self, e: &Expr, seen: &mut BTreeSet<String>) -> bool {
        let mut found = false;
        walk(e, &mut |x| match x {
            Expr::Call(f, _) => {
                if let Expr::Ident(n, _) = &**f {
                    if n == "conservative" {
                        found = true;
                    } else if seen.insert(n.clone())
                        && let Some(d) = self.decl(n)
                        && let DeclKind::Fn { body: Some(b), .. } = &d.kind
                    {
                        found |= self.uses_mode(b, seen);
                    }
                }
            }
            Expr::Ident(n, _) => {
                if seen.insert(n.clone())
                    && let Some(d) = self.decl(n)
                {
                    match &d.kind {
                        DeclKind::View(b) => found |= self.uses_mode(b, seen),
                        DeclKind::Alloc { .. } => found = true,
                        _ => {}
                    }
                }
            }
            _ => {}
        });
        found
    }

    // ------------------------------------------------------------------ expressions
    fn ctor(&self, c: &str, qualified: bool) -> String {
        if let Some(ind) = self.contract_of.get(c) {
            return if qualified { format!("{ind}.{c}") } else { format!(".{c}") };
        }
        match self.ctor_types.get(c) {
            Some(ts) if ts.len() == 1 && qualified => format!("{}.{c}", ts.iter().next().unwrap()),
            _ => format!(".{c}"),
        }
    }

    fn is_ctor(&self, n: &str) -> bool {
        self.ctor_types.contains_key(n) || self.contract_of.contains_key(n)
    }

    fn pat(&self, p: &Pat) -> R<String> {
        Ok(match p {
            Pat::Wild => "_".into(),
            Pat::Bind(n) => kw(n),
            Pat::Tuple(ps) => format!("({})", ps.iter().map(|p| self.pat(p)).collect::<R<Vec<_>>>()?.join(", ")),
            Pat::Ctor(c, ps) if self.events.contains_key(c) => {
                format!("⟨{}⟩", ps.iter().map(|p| self.pat(p)).collect::<R<Vec<_>>>()?.join(", "))
            }
            Pat::Ctor(c, ps) => {
                let mut s = self.ctor(c, false);
                for p in ps {
                    s.push(' ');
                    s.push_str(&self.pat(p)?);
                }
                s
            }
        })
    }

    fn world_ref(&self, n: &str) -> String {
        if self.moded.contains(n) { format!("(W.{n} m)") } else { format!("W.{n}") }
    }

    fn expr(&self, e: &Expr, locals: &BTreeSet<String>) -> R<String> {
        let ex = |e: &Expr| self.expr(e, locals);
        Ok(match e {
            Expr::Annotated(x, _) => ex(x)?,
            Expr::Num(n, _) if !n.contains('.') => format!("({n} : Int)"),
            Expr::Percent(n, _) if !n.contains('.') => format!("({n} : Int)"),
            Expr::Num(n, _) | Expr::Percent(n, _) => return Err(format!("non-integer literal {n}")),
            Expr::Str(s, _) => format!("{s:?}"),
            Expr::Ident(n, _) => match n.as_str() {
                _ if locals.contains(n) => kw(n),
                "true" | "false" => n.clone(),
                "none" => "none".into(),
                "Conservative" | "Optimistic" => format!("Mode.{n}"),
                _ if self.params.contains(n) => format!("W.{n}"),
                _ if self.world_fns.contains(n) => self.world_ref(n),
                _ if self.is_ctor(n) => self.ctor(n, true),
                _ => return Err(format!("cannot compile name `{n}`")),
            },
            Expr::List(xs) => format!("[{}]", xs.iter().map(ex).collect::<R<Vec<_>>>()?.join(", ")),
            Expr::Tuple(xs) => format!("({})", xs.iter().map(ex).collect::<R<Vec<_>>>()?.join(", ")),
            Expr::Unary(UnOp::Not, x) => format!("(!{})", ex(x)?),
            Expr::Unary(UnOp::Neg, x) => format!("(-{})", ex(x)?),
            Expr::Member(x, f) => match (strip(x), f.as_str()) {
                (Expr::Num(n, _), u) if u.chars().all(|c| c.is_ascii_uppercase()) => format!("({n} : Int)"),
                (Expr::Unary(UnOp::Neg, inner), u) if u.chars().all(|c| c.is_ascii_uppercase()) => format!("(-{})", ex(inner)?),
                (_, "days") => ex(x)?,
                _ => {
                    let obj = ex(x)?;
                    match self.cyclic.iter().find(|((_, field), _)| field == f) {
                        Some((_, wf)) => format!("(W.{wf} {obj})"),
                        None => format!("{obj}.{f}"),
                    }
                }
            },
            Expr::Index(x, k) => match strip(x) {
                Expr::Ident(n, _) if self.world_fns.contains(n) && !locals.contains(n) => format!("({} {})", self.world_ref(n), ex(k)?),
                _ => return Err("indexing is only supported on views and allocations".into()),
            },
            Expr::Binary(op, a, b) => {
                let (l, r) = (ex(a)?, ex(b)?);
                match op {
                    BinOp::And => format!("({l} && {r})"),
                    BinOp::Or => format!("({l} || {r})"),
                    BinOp::Implies => format!("(!{l} || {r})"),
                    BinOp::Eq => format!("({l} == {r})"),
                    BinOp::Ne => format!("({l} != {r})"),
                    BinOp::Lt => format!("(pcmp (· < ·) {l} {r})"),
                    BinOp::Le => format!("(pcmp (· ≤ ·) {l} {r})"),
                    BinOp::Gt => format!("(pcmp (· > ·) {l} {r})"),
                    BinOp::Ge => format!("(pcmp (· ≥ ·) {l} {r})"),
                    BinOp::In => format!("(List.contains {r} {l})"),
                    BinOp::Add => format!("({l} + {r})"),
                    BinOp::Sub => format!("({l} - {r})"),
                    BinOp::Mul => format!("({l} * {r})"),
                    BinOp::Div => return Err("division is not compiled to Lean".into()),
                }
            }
            Expr::If { cond, then, other } => format!("(if {} then {} else {})", ex(cond)?, ex(then)?, ex(other)?),
            Expr::IfLet { pat, value, then, other } => {
                let mut inner = locals.clone();
                bind_names(pat, &mut inner);
                format!(
                    "(match {} with\n    | some {} => {}\n    | _ => {})",
                    ex(value)?,
                    self.pat(pat)?,
                    self.expr(then, &inner)?,
                    ex(other)?
                )
            }
            Expr::Let { pat: Pat::Bind(n), value, body } => {
                let mut inner = locals.clone();
                inner.insert(n.clone());
                format!("(let {} := {}\n    {})", kw(n), ex(value)?, self.expr(body, &inner)?)
            }
            Expr::Let { .. } => return Err("`let` with a pattern is not compiled".into()),
            Expr::Match { scrutinee, arms } => self.matchx(&ex(scrutinee)?, arms, locals)?,
            Expr::Call(f, args) => self.call(f, args, locals)?,
            other => return Err(format!("not compiled to Lean: {other:?}")),
        })
    }

    /// `match`, with guards compiled as fall-through to the remaining arms.
    /// Arms after a guarded arm with the same pattern live only in its
    /// fall-through; the fall-through ends in `default` (never reached when
    /// the arms are exhaustive, which Z3 checks).
    fn matchx(&self, scrut: &str, arms: &[Arm], locals: &BTreeSet<String>) -> R<String> {
        let mut s = format!("(match {scrut} with");
        let mut skip: BTreeSet<String> = BTreeSet::new();
        for (i, arm) in arms.iter().enumerate() {
            let pat = self.pat(&arm.pat)?;
            if skip.contains(&pat) {
                continue;
            }
            let mut inner = locals.clone();
            bind_names(&arm.pat, &mut inner);
            let body = self.expr(&arm.body, &inner)?;
            match &arm.guard {
                None => write!(s, "\n    | {pat} => {body}").unwrap(),
                Some(g) => {
                    let mut rest = self.matchx(scrut, &arms[i + 1..], locals)?;
                    rest.truncate(rest.len() - 1);
                    rest.push_str("\n    | _ => default)");
                    write!(s, "\n    | {pat} => if {} then {body} else {rest}", self.expr(g, &inner)?).unwrap();
                    skip.insert(pat);
                }
            }
        }
        s.push(')');
        Ok(s)
    }

    fn call(&self, f: &Expr, args: &[Arg], locals: &BTreeSet<String>) -> R<String> {
        let Expr::Ident(name, _) = strip(f) else { return Err("only named calls".into()) };
        let ex = |e: &Expr| self.expr(e, locals);
        let named = |n: &str| args.iter().find(|a| a.name.as_deref() == Some(n)).map(|a| &a.value);
        if self.fns.contains(name) {
            let ps = match &self.decl(name).unwrap().kind {
                DeclKind::Fn { params, .. } => params.clone(),
                _ => unreachable!(),
            };
            let mut pos = args.iter().filter(|a| a.name.is_none());
            let mut out = format!("({} W m", kw(name));
            for p in &ps {
                let v = named(&p.name).or_else(|| pos.next().map(|a| &a.value)).ok_or(format!("missing argument {}", p.name))?;
                write!(out, " {}", ex(v)?).unwrap();
            }
            out.push(')');
            return Ok(out);
        }
        if self.world_fns.contains(name) {
            let a = args.iter().map(|a| ex(&a.value)).collect::<R<Vec<_>>>()?;
            return Ok(format!("({} {})", self.world_ref(name), a.join(" ")));
        }
        let comp = |f: &CompFn<'_>| -> R<String> {
            let [Arg { value: Expr::Comprehension { var, source, filter, body }, .. }] = args else {
                return Err(format!("{name}(..) needs `x in xs | ..`"));
            };
            let mut inner = locals.clone();
            inner.insert(var.clone());
            let src = ex(source)?;
            let filt = filter.as_ref().map(|c| self.expr(c, &inner)).transpose()?;
            let body = match body {
                Some(b) => self.expr(b, &inner)?,
                None => kw(var),
            };
            Ok(f(&kw(var), &src, filt, body))
        };
        let filtered = |v: &str, src: &str, filt: Option<String>| match filt {
            Some(c) => format!("({src}.filter (fun {v} => {c}))"),
            None => src.to_string(),
        };
        match name.as_str() {
            "conservative" => {
                let x = ex(&args[0].value)?;
                let worst = named("worst").ok_or("conservative(.., worst: ..)")?;
                let best = named("best");
                let is_none = |e: &Expr| matches!(strip(e), Expr::Ident(n, _) if n == "none");
                if is_none(worst) || best.is_some_and(is_none) {
                    let lift = |e: Option<&Expr>| -> R<String> {
                        match e {
                            Some(e) if is_none(e) => Ok("none".into()),
                            Some(e) => Ok(format!("(some {})", ex(e)?)),
                            None => Err("conservative(..) with `none` needs both worst and best".into()),
                        }
                    };
                    Ok(format!("(resolve m (U.map some {x}) {} {})", lift(Some(worst))?, lift(best)?))
                } else {
                    let w = ex(worst)?;
                    let b = match best {
                        Some(b) => ex(b)?,
                        None => format!("(!{w})"),
                    };
                    Ok(format!("(resolve m {x} {w} {b})"))
                }
            }
            "days" => Ok(format!("({} - {})", ex(&args[1].value)?, ex(&args[0].value)?)),
            // `item(p, 6)` -> `p ++ ".6"`: literal appends simplify in proofs.
            "item" => match strip(&args[1].value) {
                Expr::Num(n, _) if !n.contains('.') => Ok(format!("({} ++ \".{n}\")", ex(&args[0].value)?)),
                _ => Ok(format!("(s!\"{{{}}}.{{{}}}\")", ex(&args[0].value)?, ex(&args[1].value)?)),
            },
            "bucket" => Ok(format!("(W.bucket {})", ex(&args[0].value)?)),
            "solo" => Ok(format!("(W.solo {})", ex(&args[0].value)?)),
            "min" | "max" if args.len() == 2 => Ok(format!("({name} {} {})", ex(&args[0].value)?, ex(&args[1].value)?)),
            "all" => comp(&|v, src, filt, body| format!("(List.all {} (fun {v} => {body}))", filtered(v, src, filt))),
            "sum" => comp(&|v, src, filt, body| format!("(List.foldr (· + ·) 0 (List.map (fun {v} => {body}) {}))", filtered(v, src, filt))),
            other => Err(format!("built-in `{other}` is not compiled")),
        }
    }

    // ------------------------------------------------------------------ module
    fn generate(&mut self, targets: &[(String, Measure)]) -> R<String> {
        let m = self.m;
        let mut out = String::new();
        writeln!(out, "/-\n  GENERATED by `prudence lean` from the Prudence module `{}`. Do not edit.\n  Regenerate: cargo run -p deposits --bin prudence-lean\n-/", m.name).unwrap();
        writeln!(out, "set_option autoImplicit false\nset_option linter.unusedVariables false\nset_option match.ignoreUnusedAlts true\n\nnamespace {}\n", m.name).unwrap();
        out.push_str(PRELUDE);

        // Enums.
        for (n, cs) in &self.enums {
            writeln!(out, "inductive {n} where\n{}\n  deriving DecidableEq, Repr, Inhabited\n", cs.iter().map(|c| format!("  | {c}")).collect::<Vec<_>>().join("\n")).unwrap();
        }
        // Records, in dependency order; contracts before their owner.
        let mut done: BTreeSet<String> = BTreeSet::new();
        let names: Vec<String> = self.data.keys().cloned().collect();
        let mut pending = names.clone();
        while !pending.is_empty() {
            let before = pending.len();
            pending.retain(|dn| {
                let deps: Vec<String> = self.data[dn]
                    .iter()
                    .filter(|(f, _)| !self.cyclic.contains_key(&(dn.clone(), f.clone())))
                    .filter_map(|(_, t)| match t {
                        Type::Named { name, .. } if names.contains(name) => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
                if deps.iter().any(|d| !done.contains(d)) {
                    return true;
                }
                // Contract variants first.
                if let Some((_, Type::Named { name: ind, .. })) = self.data[dn].iter().find(|(f, _)| f == "contract") {
                    let ctors: Vec<&String> = self.contract_of.iter().filter(|(_, i)| *i == ind).map(|(c, _)| c).collect();
                    let mut ctors: Vec<&String> = ctors;
                    ctors.sort();
                    writeln!(out, "inductive {ind} where").unwrap();
                    for c in ctors {
                        let fields = &self.contract_fields[c];
                        let types = self.contract_types(c);
                        let fs: Vec<String> = fields.iter().zip(types).map(|(f, t)| format!(" ({} : {t})", kw(f))).collect();
                        writeln!(out, "  | {c}{}", fs.join("")).unwrap();
                    }
                    writeln!(out, "  deriving DecidableEq, Repr, Inhabited\n").unwrap();
                }
                writeln!(out, "structure {dn} where").unwrap();
                for (f, t) in &self.data[dn] {
                    if self.cyclic.contains_key(&(dn.clone(), f.clone())) {
                        writeln!(out, "  -- {f}: world function W.{dn}_{f} (would make records cyclic)").unwrap();
                        continue;
                    }
                    writeln!(out, "  {} : {}", kw(f), self.ty(t).unwrap_or_else(|e| format!("Unit /- {e} -/"))).unwrap();
                }
                writeln!(out, "  deriving DecidableEq, Repr, Inhabited\n").unwrap();
                done.insert(dn.clone());
                false
            });
            if pending.len() == before {
                return Err(format!("cyclic records: {pending:?}"));
            }
        }
        for (n, fs) in &self.events {
            writeln!(out, "structure {n} where").unwrap();
            for (f, t) in fs {
                writeln!(out, "  {} : {}", kw(f), self.ty(t)?).unwrap();
            }
            writeln!(out, "  deriving DecidableEq, Repr, Inhabited\n").unwrap();
        }

        // Decide what is compiled and what is a world field.
        let vtypes = view_types(m);
        let mut world: Vec<String> = Vec::new();
        for d in &m.decls {
            if let DeclKind::Param { ty, .. } = &d.kind {
                world.push(format!("  {} : {}", d.name, self.ty(ty)?));
            }
        }
        for d in &m.decls {
            let moded = match &d.kind {
                DeclKind::View(b) => self.uses_mode(b, &mut BTreeSet::new()),
                DeclKind::Alloc { .. } => true,
                DeclKind::Fn { body: Some(b), .. } => self.uses_mode(b, &mut BTreeSet::from([d.name.clone()])),
                _ => false,
            };
            if moded {
                self.moded.insert(d.name.clone());
            }
        }
        // Rows of grouped views.
        let mut rows = String::new();
        for d in &m.decls {
            match &d.kind {
                DeclKind::View(_) => {
                    let mode = if self.moded.contains(&d.name) { "Mode → " } else { "" };
                    let t = match vtypes.get(&d.name) {
                        Some(Ty::Map(k, v)) => {
                            let row = format!("{}_row", d.name);
                            if let Ty::Struct(fs) = &**v {
                                writeln!(rows, "structure {row} where").unwrap();
                                for (f, ft) in fs {
                                    writeln!(rows, "  {f} : {}", self.ty_of(ft)).unwrap();
                                }
                                writeln!(rows, "  deriving Repr, Inhabited\n").unwrap();
                            }
                            format!("Option ({}) → {row}", self.ty_of(k))
                        }
                        Some(t) => self.ty_of(t),
                        None => return Err(format!("no type for view {}", d.name)),
                    };
                    world.push(format!("  {} : {mode}{t}  -- view", d.name));
                    self.world_fns.insert(d.name.clone());
                }
                DeclKind::Alloc { ty: Type::Named { params, .. }, .. } => {
                    let (src, fld) = match params.first() {
                        Some(TypeParam::Expr(Expr::Member(s, f))) => match &**s {
                            Expr::Ident(t, _) => (t.clone(), f.clone()),
                            _ => return Err("alloc source".into()),
                        },
                        _ => return Err("alloc source".into()),
                    };
                    let vt = self.data[&src].iter().find(|(f, _)| *f == fld).map(|(_, t)| self.ty(t)).transpose()?.unwrap_or("Int".into());
                    world.push(format!("  {} : Mode → {src} → {vt}  -- optimize solution", d.name));
                    self.world_fns.insert(d.name.clone());
                }
                _ => {}
            }
        }
        for ((dn, f), wf) in &self.cyclic {
            let t = self.data[dn].iter().find(|(n, _)| n == f).map(|(_, t)| self.ty(t)).transpose()?.unwrap();
            world.push(format!("  {wf} : {dn} → {t}  -- cyclic record field"));
        }
        // Built-ins, only if the module uses them.
        let mut used: BTreeSet<String> = BTreeSet::new();
        for d in &m.decls {
            let mut note = |e: &Expr| {
                walk(e, &mut |x| {
                    if let Expr::Call(f, args) = x
                        && let Expr::Ident(n, _) = &**f
                    {
                        used.insert(n.clone());
                        if n == "solo"
                            && let Some(Arg { value: Expr::Ident(v, _), .. }) = args.first()
                        {
                            used.insert(format!("solo:{v}"));
                        }
                    }
                })
            };
            match &d.kind {
                DeclKind::Fn { body: Some(b), .. } | DeclKind::View(b) | DeclKind::Invariant(b) => note(b),
                DeclKind::Partition { cases, .. } => {
                    for c in cases {
                        c.lets.iter().for_each(|(_, e)| note(e));
                        c.outputs.iter().for_each(|(_, e)| note(e));
                    }
                }
                _ => {}
            }
        }
        if used.contains("bucket") {
            world.push("  bucket : Int → Nat  -- NSFR maturity bucket of a date (built-in)".into());
        }
        if used.contains("solo") {
            // The argument's type: the data type holding the grouped field.
            let owner = self.data.iter().find(|(_, fs)| fs.iter().any(|(f, _)| f == "sme_group")).map(|(n, _)| n.clone());
            let owner = owner.ok_or("solo(..) needs a data type with a group field")?;
            world.push(format!("  solo : {owner} → GroupId  -- the group of just one {owner} (built-in)"));
        }
        out.push_str(&rows);

        // Functions: compile in order; ones that don't compile become world fields.
        let mut fns_out = String::new();
        for d in &m.decls {
            let DeclKind::Fn { params, ret, body: Some(b) } = &d.kind else { continue };
            let locals: BTreeSet<String> = params.iter().map(|p| p.name.clone()).collect();
            self.fns.insert(d.name.clone());
            match self.expr(b, &locals) {
                Ok(body) => {
                    let ps: Vec<String> = params.iter().map(|p| Ok(format!(" ({} : {})", kw(&p.name), self.ty(&p.ty)?))).collect::<R<_>>()?;
                    writeln!(fns_out, "def {} (W : World) (m : Mode){} : {} :=\n  {body}\n", kw(&d.name), ps.join(""), self.ty(ret)?).unwrap();
                }
                Err(e) => {
                    self.fns.remove(&d.name);
                    self.world_fns.insert(d.name.clone());
                    let mode = if self.moded.contains(&d.name) { "Mode → " } else { "" };
                    let ps: Vec<String> = params.iter().map(|p| self.ty(&p.ty)).collect::<R<_>>()?;
                    world.push(format!("  {} : {mode}{} → {}  -- fn over events/views: {e}", d.name, ps.join(" → "), self.ty(ret)?));
                }
            }
        }
        writeln!(out, "/-- Everything the rules read that is not computed here. -/\nstructure World where\n{}\n", world.join("\n")).unwrap();
        out.push_str(&fns_out);

        // Partition outputs and measures (if the module has a partition).
        let Some(part) = m.decls.iter().find(|d| matches!(d.kind, DeclKind::Partition { .. })) else {
            writeln!(out, "end {}", m.name).unwrap();
            return Ok(out);
        };
        let DeclKind::Partition { var, source, targets: tnames, split, split_var, split_by, cases } = &part.kind else { unreachable!() };
        let src = self.ty(source)?;
        // The split amount is evaluated before the split variable is bound.
        let outer: BTreeSet<String> = BTreeSet::from([var.clone()]);
        let amount_split = self.expr(split_by, &outer)?;
        let amount_whole = self.expr(split, &outer)?;
        let mut locals: BTreeSet<String> = BTreeSet::from([var.clone(), split_var.clone()]);
        writeln!(
            out,
            "/-- The `partition` portion of `{var}` with `{split_var}`. -/\ndef portion (W : World) (m : Mode) ({var} : {src}) ({split_var} : Bool) : Int :=\n  if {split_var} then {amount_split} else {amount_whole} - {amount_split}\n"
        )
        .unwrap();
        for (k, ((tname, measure), out_name)) in targets.iter().zip(cases[0].outputs.iter().map(|(n, _)| n)).enumerate() {
            if tnames.get(k) != Some(tname) {
                return Err(format!("partition target {k} is {:?}, expected {tname}", tnames.get(k)));
            }
            let ret = match measure {
                Measure::Weight(_) => "Option String",
                Measure::Factor(_) => "String × Nat",
            };
            // Output as an if-chain over the cases; the last case is the else.
            let mut body = String::new();
            for (i, c) in cases.iter().enumerate() {
                let mut inner = locals.clone();
                let mut lets = String::new();
                for (p, e) in &c.lets {
                    let Pat::Bind(n) = p else { return Err("case `let` must bind a name".into()) };
                    write!(lets, "let {} := {}\n    ", kw(n), self.expr(e, &inner)?).unwrap();
                    inner.insert(n.clone());
                }
                let (_, oe) = c.outputs.iter().find(|(n, _)| n == out_name).ok_or("missing output")?;
                let val = format!("{lets}{}", self.expr(oe, &inner)?);
                if i + 1 < cases.len() {
                    write!(body, "if {} then\n    {val}\n  else ", self.expr(&c.cond, &locals)?).unwrap();
                } else {
                    write!(body, "\n    -- remaining case (exhaustive by the Z3 obligation): {}\n    {val}", self.expr(&c.cond, &locals)?).unwrap();
                }
            }
            writeln!(out, "def out_{out_name} (W : World) (m : Mode) ({var} : {src}) ({split_var} : Bool) : {ret} :=\n  {body}\n").unwrap();
            match measure {
                Measure::Weight(w) => {
                    writeln!(out, "/-- Column-2 weights of {tname} (percent). -/\ndef weight_{tname} : Option String → Int").unwrap();
                    for (item, pct) in w {
                        writeln!(out, "  | some {item:?} => {pct}").unwrap();
                    }
                    writeln!(out, "  | _ => 0\n").unwrap();
                    writeln!(
                        out,
                        "/-- {tname} measure of `{var}` (percent of the amount). -/\ndef measure_{tname} (W : World) (m : Mode) ({var} : {src}) : Int :=\n  weight_{tname} (out_{out_name} W m {var} true) * portion W m {var} true\n    + weight_{tname} (out_{out_name} W m {var} false) * portion W m {var} false\n"
                    )
                    .unwrap();
                }
                Measure::Factor(f) => {
                    writeln!(out, "/-- Factors of {tname} by maturity bucket (percent). -/\ndef factor_{tname} : String × Nat → Int").unwrap();
                    for ((item, b), pct) in f {
                        writeln!(out, "  | ({item:?}, {b}) => {pct}").unwrap();
                    }
                    writeln!(out, "  | _ => 0\n").unwrap();
                    writeln!(
                        out,
                        "/-- {tname} measure of `{var}` (percent of the amount). -/\ndef measure_{tname} (W : World) (m : Mode) ({var} : {src}) : Int :=\n  factor_{tname} (out_{out_name} W m {var} true) * portion W m {var} true\n    + factor_{tname} (out_{out_name} W m {var} false) * portion W m {var} false\n"
                    )
                    .unwrap();
                }
            }
        }
        locals.clear();
        writeln!(out, "end {}", m.name).unwrap();
        Ok(out)
    }

    fn contract_types(&self, c: &str) -> Vec<String> {
        for d in &self.m.decls {
            if let DeclKind::Data { base: Some(Type::Named { generics, .. }), .. } = &d.kind {
                for gt in generics {
                    let alts = match gt {
                        Type::Union(xs) => xs.iter().collect(),
                        other => vec![other],
                    };
                    for a in alts {
                        if let Type::Named { name, params, .. } = a
                            && name == c
                        {
                            return params
                                .iter()
                                .filter_map(|p| if let TypeParam::Field(_, t) = p { self.ty(t).ok() } else { None })
                                .collect();
                        }
                    }
                }
            }
        }
        vec![]
    }
}

fn bind_names(p: &Pat, out: &mut BTreeSet<String>) {
    match p {
        Pat::Bind(n) => {
            out.insert(n.clone());
        }
        Pat::Ctor(_, ps) | Pat::Tuple(ps) => ps.iter().for_each(|p| bind_names(p, out)),
        Pat::Wild => {}
    }
}

fn walk(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    match e {
        Expr::Annotated(x, _) | Expr::Unary(_, x) | Expr::Member(x, _) | Expr::Is(x, _) => walk(x, f),
        Expr::Binary(_, a, b) | Expr::Index(a, b) => {
            walk(a, f);
            walk(b, f);
        }
        Expr::Call(g, args) => {
            walk(g, f);
            args.iter().for_each(|a| walk(&a.value, f));
        }
        Expr::List(xs) | Expr::Tuple(xs) => xs.iter().for_each(|x| walk(x, f)),
        Expr::If { cond, then, other } => [cond, then, other].iter().for_each(|x| walk(x, f)),
        Expr::IfLet { value, then, other, .. } => [value, then, other].iter().for_each(|x| walk(x, f)),
        Expr::Let { value, body, .. } => {
            walk(value, f);
            walk(body, f);
        }
        Expr::Match { scrutinee, arms } => {
            walk(scrutinee, f);
            for a in arms {
                if let Some(g) = &a.guard {
                    walk(g, f);
                }
                walk(&a.body, f);
            }
        }
        Expr::Comprehension { source, filter, body, .. } => {
            walk(source, f);
            filter.iter().chain(body.iter()).for_each(|x| walk(x, f));
        }
        Expr::Group { source, key, filter, fields } => {
            walk(source, f);
            walk(key, f);
            filter.iter().for_each(|x| walk(x, f));
            fields.iter().for_each(|(_, x)| walk(x, f));
        }
        Expr::Optimize { filter, objective, constraints, .. } => {
            filter.iter().for_each(|x| walk(x, f));
            walk(objective, f);
            constraints.iter().for_each(|x| walk(x, f));
        }
        Expr::Forall { body, .. } => walk(body, f),
        Expr::Num(..) | Expr::Percent(..) | Expr::Str(..) | Expr::Ident(..) | Expr::Cell(..) => {}
    }
}

const PRELUDE: &str = r#"inductive Mode where
  | Conservative
  | Optimistic
  deriving DecidableEq, Repr, Inhabited

/-- `Uncertain<T>` -/
inductive U (α : Type) where
  | known (a : α)
  | unknown
  deriving DecidableEq, Repr, Inhabited

def U.map {α β : Type} (f : α → β) : U α → U β
  | .known a => .known (f a)
  | .unknown => .unknown

/-- `conservative(x, worst: w, best: b)` -/
def resolve {α : Type} (m : Mode) (x : U α) (worst best : α) : α :=
  match x with
  | .known a => a
  | .unknown => match m with
    | .Conservative => worst
    | .Optimistic => best

/-- Ordered comparison where an absent optional date compares false. -/
class ToOpt (α : Type) where
  toOpt : α → Option Int
instance : ToOpt Int := ⟨some⟩
instance : ToOpt (Option Int) := ⟨id⟩
def pcmp {α β : Type} [ToOpt α] [ToOpt β] (r : Int → Int → Prop) [DecidableRel r] (a : α) (b : β) : Bool :=
  match ToOpt.toOpt a, ToOpt.toOpt b with
  | some x, some y => decide (r x y)
  | _, _ => false

abbrev GroupId := Nat
abbrev ProductClass := Nat

"#;

/// Compile `m` to Lean. `targets` gives, in partition order, each target
/// form's measure table.
pub fn generate(m: &Module, targets: &[(String, Measure)]) -> R<String> {
    Gen::new(m).generate(targets)
}

/// Compile derived cells of a form spec to Lean functions of the reported
/// column-1 items: `def <name> (cell : String → Int) : Int := ...`.
pub fn form_cells(form: &crate::ir::Form, cells: &[(&str, &str)]) -> R<String> {
    use crate::ir::{BinOp as B, Cell, Expr as E, Ref};
    fn ex(e: &E) -> R<String> {
        Ok(match e {
            E::Ref(r) => format!("(cell {:?})", r.item),
            E::Const(q) if q.is_integer() => format!("({q} : Int)"),
            E::Const(q) => return Err(format!("non-integer constant {q}")),
            E::Neg(x) => format!("(-{})", ex(x)?),
            E::Bin(op, a, b) => {
                let o = match op {
                    B::Add => "+",
                    B::Sub => "-",
                    B::Mul => "*",
                    B::Div => return Err("division".into()),
                };
                format!("({} {o} {})", ex(a)?, ex(b)?)
            }
            E::Max(xs) | E::Min(xs) => {
                let f = if matches!(e, E::Max(_)) { "max" } else { "min" };
                let mut it = xs.iter().map(ex);
                let first = it.next().ok_or("empty max/min")??;
                it.try_fold(first, |acc, x| Ok::<_, String>(format!("({f} {acc} {})", x?)))?
            }
            E::If(..) => return Err("conditional cells are not compiled".into()),
        })
    }
    let mut out = String::new();
    for (item, name) in cells {
        let r = Ref::new(*item, "amount");
        let Some(Cell::Derived(e)) = form.cell(&r) else { return Err(format!("{} has no derived cell {r}", form.code)) };
        let mut refs = Vec::new();
        e.refs(&mut refs);
        if refs.iter().any(|r| r.col != "amount") {
            return Err(format!("{r} refers to non-amount columns"));
        }
        writeln!(out, "/-- {} item {item} (generated from the form spec). -/\ndef {name} (cell : String → Int) : Int :=\n  {}\n", form.code, ex(e)?).unwrap();
    }
    Ok(out)
}
