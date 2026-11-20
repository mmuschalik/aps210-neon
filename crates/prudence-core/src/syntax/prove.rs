//! Proof obligations discharged by an SMT solver (Z3), design.txt §4.B.
//!
//! - Form `theorem`s: the theorem's cells are expanded to their exact
//!   definitions down to the reported inputs, which are left free; Z3 proves
//!   the theorem for *all* inputs, or returns a counterexample.
//! - Module `partition`s: the case conditions are exhaustive and pairwise
//!   disjoint, for all values of the enum-valued terms they test.
//! - Module `match`es: the arms (with guards) are exhaustive.
//!
//! The solver is not part of the trusted base for evaluation, but a proof here
//! is only as good as Z3 and this translation; counterexamples are concrete.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::process::{Command, Stdio};

use num_traits::{One, Signed};

use super::ast::{BinOp, DeclKind, Expr as SExpr, Module, Pat, Type, UnOp};
use crate::ir::{BinOp as IBin, Cell, Cmp, CmpOp, Expr, Form, Prop, Ref};
use crate::num::Q;

#[derive(Debug, Clone)]
pub struct Outcome {
    pub obligation: String,
    pub proven: bool,
    /// Solver model when not proven (a concrete counterexample).
    pub counterexample: String,
}

/// Run Z3 on an SMT-LIB script whose last check is the negated goal.
pub fn z3(script: &str) -> Result<(bool, String), String> {
    let mut child = Command::new("z3")
        .args(["-in", "-smt2", "-T:30"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run z3 (is it installed?): {e}"))?;
    child.stdin.take().unwrap().write_all(script.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let first = text.lines().next().unwrap_or("").trim().to_string();
    match first.as_str() {
        "unsat" => Ok((true, String::new())),
        "sat" => Ok((false, text.lines().skip(1).collect::<Vec<_>>().join("\n"))),
        other => Err(format!("z3 returned {other:?}: {text}")),
    }
}

// ---------------------------------------------------------------------- forms

/// Real literal (SMT-LIB: `1` is an Int, `1.0` a Real).
fn q(x: &Q) -> String {
    let n = x.numer().abs().to_string();
    let body = if x.denom().is_one() { format!("{n}.0") } else { format!("(/ {n}.0 {}.0)", x.denom()) };
    if x.is_negative() { format!("(- {body})") } else { body }
}

fn sym(r: &Ref) -> String {
    format!("|{}:{}|", r.item, r.col)
}

fn expr(e: &Expr) -> String {
    match e {
        Expr::Ref(r) => sym(r),
        Expr::Const(c) => q(c),
        Expr::Neg(x) => format!("(- {})", expr(x)),
        Expr::Bin(op, a, b) => {
            let o = match op {
                IBin::Add => "+",
                IBin::Sub => "-",
                IBin::Mul => "*",
                IBin::Div => "/",
            };
            format!("({o} {} {})", expr(a), expr(b))
        }
        Expr::Max(xs) | Expr::Min(xs) => {
            let ge = if matches!(e, Expr::Max(_)) { ">=" } else { "<=" };
            let mut it = xs.iter().map(expr);
            let first = it.next().unwrap_or_else(|| "0".into());
            it.fold(first, |acc, x| format!("(ite ({ge} {acc} {x}) {acc} {x})"))
        }
        Expr::If(c, t, o) => format!("(ite {} {} {})", cmp(c), expr(t), expr(o)),
    }
}

fn cmp(c: &Cmp) -> String {
    let o = match c.op {
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Eq => "=",
    };
    format!("({o} {} {})", expr(&c.a), expr(&c.b))
}

fn prop(p: &Prop) -> String {
    match p {
        Prop::Cmp(c) => cmp(c),
        Prop::Not(x) => format!("(not {})", prop(x)),
        Prop::And(a, b) => format!("(and {} {})", prop(a), prop(b)),
        Prop::Or(a, b) => format!("(or {} {})", prop(a), prop(b)),
        Prop::Implies(a, b) => format!("(=> {} {})", prop(a), prop(b)),
    }
}

/// The SMT-LIB script for one form theorem: inputs free, derived cells
/// defined exactly, goal negated.
pub fn theorem_script(form: &Form, goal: &Prop) -> String {
    let mut refs = Vec::new();
    goal.refs(&mut refs);
    let (mut decls, mut defs, mut seen) = (Vec::new(), Vec::new(), BTreeSet::new());
    fn visit(form: &Form, r: &Ref, seen: &mut BTreeSet<Ref>, decls: &mut Vec<String>, defs: &mut Vec<String>) {
        if !seen.insert(r.clone()) {
            return;
        }
        match form.cell(r) {
            Some(Cell::Input { .. }) | None => decls.push(format!("(declare-const {} Real)", sym(r))),
            Some(Cell::Derived(e)) => {
                let mut deps = Vec::new();
                e.refs(&mut deps);
                for d in &deps {
                    visit(form, d, seen, decls, defs);
                }
                defs.push(format!("(define-fun {} () Real {})", sym(r), expr(e)));
            }
        }
    }
    for r in &refs {
        visit(form, r, &mut seen, &mut decls, &mut defs);
    }
    format!("{}\n{}\n(assert (not {}))\n(check-sat)\n(get-model)\n", decls.join("\n"), defs.join("\n"), prop(goal))
}

/// Prove every `theorem` of a form.
pub fn prove_form(form: &Form) -> Result<Vec<Outcome>, String> {
    form.theorems
        .iter()
        .map(|t| {
            let (proven, model) = z3(&theorem_script(form, &t.prop))?;
            Ok(Outcome { obligation: format!("{} theorem {}", form.code, t.id), proven, counterexample: model })
        })
        .collect()
}

// ---------------------------------------------------------------------- modules

/// Finite-domain encoding of module conditions over enum-valued terms.
struct Enc {
    enums: BTreeMap<String, Vec<String>>,
    ctor_types: HashMap<String, BTreeSet<String>>,
    fn_ret: HashMap<String, String>,
    atoms: BTreeMap<String, Option<String>>, // canonical term -> enum (None = Bool)
}

fn canon(e: &SExpr) -> String {
    match e {
        SExpr::Ident(n, _) => n.clone(),
        SExpr::Member(x, f) => format!("{}.{f}", canon(x)),
        SExpr::Call(f, args) => {
            let a: Vec<String> = args.iter().map(|a| canon(&a.value)).collect();
            format!("{}({})", canon(f), a.join(","))
        }
        SExpr::Annotated(x, _) => canon(x),
        SExpr::Tuple(xs) => format!("({})", xs.iter().map(canon).collect::<Vec<_>>().join(", ")),
        other => format!("{other:?}"),
    }
}

impl Enc {
    fn new(m: &Module) -> Self {
        let mut enums = BTreeMap::new();
        let mut ctor_types: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut fn_ret = HashMap::new();
        for d in &m.decls {
            match &d.kind {
                DeclKind::TypeAlias(Type::Union(alts)) => {
                    let cs: Vec<String> = alts
                        .iter()
                        .filter_map(|a| if let Type::Named { name, .. } = a { Some(name.clone()) } else { None })
                        .collect();
                    for c in &cs {
                        ctor_types.entry(c.clone()).or_default().insert(d.name.clone());
                    }
                    enums.insert(d.name.clone(), cs);
                }
                DeclKind::Data { base: Some(Type::Named { name, generics, .. }), .. } if name == "Contract" => {
                    let ty = format!("Contract{}", d.name);
                    let mut cs = Vec::new();
                    for g in generics {
                        let alts = match g {
                            Type::Union(xs) => xs.iter().collect(),
                            other => vec![other],
                        };
                        for a in alts {
                            if let Type::Named { name, .. } = a {
                                cs.push(name.clone());
                                ctor_types.entry(name.clone()).or_default().insert(ty.clone());
                            }
                        }
                    }
                    enums.insert(ty, cs);
                }
                DeclKind::Fn { ret: Type::Named { name, .. }, .. } => {
                    fn_ret.insert(d.name.clone(), name.clone());
                }
                _ => {}
            }
        }
        Enc { enums, ctor_types, fn_ret, atoms: BTreeMap::new() }
    }

    fn is_ctor(&self, e: &SExpr) -> Option<&BTreeSet<String>> {
        match e {
            SExpr::Ident(n, _) => self.ctor_types.get(n),
            SExpr::Annotated(x, _) => self.is_ctor(x),
            _ => None,
        }
    }

    /// The enum type of a term compared against constructor `c`.
    fn atom(&mut self, term: &SExpr, ctor: &str) -> Option<String> {
        let key = canon(term);
        let declared = match term {
            SExpr::Call(f, _) => match &**f {
                SExpr::Ident(n, _) => self.fn_ret.get(n).filter(|t| self.enums.contains_key(*t)).cloned(),
                _ => None,
            },
            _ => None,
        };
        let cands = self.ctor_types.get(ctor)?.clone();
        let ty = match declared {
            Some(t) if cands.contains(&t) => t,
            Some(_) => return None,
            None if cands.len() == 1 => cands.into_iter().next().unwrap(),
            None => match self.atoms.get(&key) {
                Some(Some(t)) if cands.contains(t) => t.clone(),
                _ => cands.into_iter().next().unwrap(),
            },
        };
        self.atoms.insert(key.clone(), Some(ty.clone()));
        Some(ty)
    }

    fn var(&self, key: &str) -> String {
        format!("|{key}|")
    }

    fn eq(&mut self, term: &SExpr, ctor: &str) -> String {
        match self.atom(term, ctor) {
            Some(ty) => format!("(= {} {ty}_{ctor})", self.var(&canon(term))),
            None => self.opaque(term),
        }
    }

    fn opaque(&mut self, e: &SExpr) -> String {
        let key = canon(e);
        self.atoms.entry(key.clone()).or_insert(None);
        self.var(&key)
    }

    /// Boolean condition; anything not understood becomes a free Bool, which
    /// can only make a proof harder, never unsound.
    fn cond(&mut self, e: &SExpr) -> String {
        match e {
            SExpr::Annotated(x, _) => self.cond(x),
            SExpr::Binary(BinOp::And, a, b) => format!("(and {} {})", self.cond(a), self.cond(b)),
            SExpr::Binary(BinOp::Or, a, b) => format!("(or {} {})", self.cond(a), self.cond(b)),
            SExpr::Binary(BinOp::Implies, a, b) => format!("(=> {} {})", self.cond(a), self.cond(b)),
            SExpr::Unary(UnOp::Not, a) => format!("(not {})", self.cond(a)),
            SExpr::Ident(b, _) if b == "true" || b == "false" => b.clone(),
            SExpr::Binary(op @ (BinOp::Eq | BinOp::Ne), a, b) => {
                let (t, c) = match (self.is_ctor(a), self.is_ctor(b)) {
                    (None, Some(_)) => (a, b),
                    (Some(_), None) => (b, a),
                    _ => return self.opaque(e),
                };
                let SExpr::Ident(c, _) = strip(c) else { return self.opaque(e) };
                let eq = self.eq(t, &c.clone());
                if *op == BinOp::Eq { eq } else { format!("(not {eq})") }
            }
            SExpr::Binary(BinOp::In, t, list) => match strip(list) {
                SExpr::List(xs) if xs.iter().all(|x| self.is_ctor(x).is_some()) => {
                    let parts: Vec<String> = xs
                        .iter()
                        .map(|x| match strip(x) {
                            SExpr::Ident(c, _) => self.eq(t, &c.clone()),
                            _ => unreachable!(),
                        })
                        .collect();
                    format!("(or false {})", parts.join(" "))
                }
                _ => self.opaque(e),
            },
            other => self.opaque(other),
        }
    }

    fn pat(&mut self, p: &Pat, scrutinee: &SExpr) -> String {
        match (p, strip(scrutinee)) {
            (Pat::Wild | Pat::Bind(_), _) => "true".into(),
            (Pat::Tuple(ps), SExpr::Tuple(xs)) if ps.len() == xs.len() => {
                let parts: Vec<String> = ps.iter().zip(xs).map(|(p, x)| self.pat(p, x)).collect();
                format!("(and true {})", parts.join(" "))
            }
            (Pat::Ctor(c, _), _) if self.ctor_types.contains_key(c) => self.eq(scrutinee, &c.clone()),
            _ => self.opaque(&SExpr::Str(format!("pattern {p:?} on {}", canon(scrutinee)), lexpos())),
        }
    }

    fn preamble(&self) -> String {
        let mut s = String::new();
        let used: BTreeSet<&String> = self.atoms.values().flatten().collect();
        for (t, cs) in &self.enums {
            if used.contains(t) {
                let ctors: Vec<String> = cs.iter().map(|c| format!("({t}_{c})")).collect();
                s.push_str(&format!("(declare-datatypes (({t} 0)) (({})))\n", ctors.join(" ")));
            }
        }
        for (k, ty) in &self.atoms {
            s.push_str(&format!("(declare-const {} {})\n", self.var(k), ty.as_deref().unwrap_or("Bool")));
        }
        s
    }
}

fn strip(e: &SExpr) -> &SExpr {
    match e {
        SExpr::Annotated(x, _) => strip(x),
        other => other,
    }
}

fn lexpos() -> super::lexer::Pos {
    super::lexer::Pos { line: 0, col: 0 }
}

fn collect_matches<'e>(e: &'e SExpr, out: &mut Vec<&'e SExpr>) {
    use SExpr::*;
    match e {
        Match { scrutinee, arms } => {
            out.push(e);
            collect_matches(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    collect_matches(g, out);
                }
                collect_matches(&a.body, out);
            }
        }
        Annotated(x, _) | Unary(_, x) | Member(x, _) | Is(x, _) => collect_matches(x, out),
        Binary(_, a, b) | Index(a, b) => {
            collect_matches(a, out);
            collect_matches(b, out);
        }
        Call(f, args) => {
            collect_matches(f, out);
            args.iter().for_each(|a| collect_matches(&a.value, out));
        }
        List(xs) | Tuple(xs) => xs.iter().for_each(|x| collect_matches(x, out)),
        If { cond, then, other } => [cond, then, other].iter().for_each(|x| collect_matches(x, out)),
        IfLet { value, then, other, .. } => [value, then, other].iter().for_each(|x| collect_matches(x, out)),
        Let { value, body, .. } => {
            collect_matches(value, out);
            collect_matches(body, out);
        }
        Comprehension { source, filter, body, .. } => {
            collect_matches(source, out);
            filter.iter().chain(body.iter()).for_each(|x| collect_matches(x, out));
        }
        Group { source, key, filter, fields } => {
            collect_matches(source, out);
            collect_matches(key, out);
            filter.iter().for_each(|x| collect_matches(x, out));
            fields.iter().for_each(|(_, x)| collect_matches(x, out));
        }
        Optimize { filter, objective, constraints, .. } => {
            filter.iter().for_each(|x| collect_matches(x, out));
            collect_matches(objective, out);
            constraints.iter().for_each(|x| collect_matches(x, out));
        }
        Forall { body, .. } => collect_matches(body, out),
        Num(..) | Percent(..) | Str(..) | Ident(..) | Cell(..) => {}
    }
}

fn discharge(name: String, enc: &Enc, goal_negated: &str) -> Result<Outcome, String> {
    let script = format!("{}(assert {goal_negated})\n(check-sat)\n(get-model)\n", enc.preamble());
    let (proven, model) = z3(&script)?;
    Ok(Outcome { obligation: name, proven, counterexample: model })
}

/// Partition exhaustiveness/disjointness and match exhaustiveness for a module.
pub fn prove_module(m: &Module) -> Result<Vec<Outcome>, String> {
    let mut out = Vec::new();
    for d in &m.decls {
        // Partitions: exactly one case for every combination of the tested terms.
        if let DeclKind::Partition { cases, .. } = &d.kind {
            let mut enc = Enc::new(m);
            let conds: Vec<String> = cases.iter().map(|c| enc.cond(&c.cond)).collect();
            out.push(discharge(
                format!("{}: cases are exhaustive", d.name),
                &enc,
                &format!("(not (or false {}))", conds.join(" ")),
            )?);
            for i in 0..conds.len() {
                for j in i + 1..conds.len() {
                    out.push(discharge(
                        format!("{}: cases {} and {} are disjoint", d.name, i + 1, j + 1),
                        &enc,
                        &format!("(and {} {})", conds[i], conds[j]),
                    )?);
                }
            }
        }
        // Matches: some arm (pattern and guard) applies to every value.
        let mut bodies: Vec<&SExpr> = Vec::new();
        match &d.kind {
            DeclKind::Fn { body: Some(b), .. } | DeclKind::View(b) => bodies.push(b),
            DeclKind::Partition { cases, .. } => {
                for c in cases {
                    bodies.extend(c.lets.iter().map(|(_, e)| e));
                    bodies.extend(c.outputs.iter().map(|(_, e)| e));
                }
            }
            _ => {}
        }
        let mut matches = Vec::new();
        for b in bodies {
            collect_matches(b, &mut matches);
        }
        for (k, mexpr) in matches.into_iter().enumerate() {
            let SExpr::Match { scrutinee, arms } = mexpr else { continue };
            let mut enc = Enc::new(m);
            let arms: Vec<String> = arms
                .iter()
                .map(|a| {
                    let p = enc.pat(&a.pat, scrutinee);
                    match &a.guard {
                        Some(g) => format!("(and {p} {})", enc.cond(g)),
                        None => p,
                    }
                })
                .collect();
            out.push(discharge(
                format!("{}: match #{} on `{}` is exhaustive", d.name, k + 1, canon(scrutinee)),
                &enc,
                &format!("(not (or false {}))", arms.join(" ")),
            )?);
        }
    }
    Ok(out)
}
