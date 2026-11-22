//! Static dependencies of a module, for staging (design 4.A: one return can
//! feed another, e.g. the NSFR's 10% RSF quota is capped by the CLF amount in
//! the LCR numerator).
//!
//! `cell("ARF 210.1A", "6.3")` reads a cell of another return. The runtime
//! binds it once that return has been evaluated, so a partition can only run
//! after every form it reads. A partition that reads a cell of the form it
//! feeds would be a cycle, and is rejected here, at compile time.

use std::collections::{BTreeMap, BTreeSet};

use super::ast::*;

/// The form code of a partition target: `Arf210_1A` -> `ARF 210.1A`,
/// `Arf210_3_1` -> `ARF 210.3.1`.
pub fn form_code(target: &str) -> String {
    match target.strip_prefix("Arf") {
        Some(rest) => format!("ARF {}", rest.replace('_', ".")),
        None => target.to_string(),
    }
}

/// Every sub-expression of `e`, including `e`.
pub fn walk<'e>(e: &'e Expr, f: &mut dyn FnMut(&'e Expr)) {
    f(e);
    let mut go = |x: &'e Expr| walk(x, f);
    match e {
        Expr::Num(..) | Expr::Percent(..) | Expr::Str(..) | Expr::Ident(..) | Expr::Cell(..) => {}
        Expr::List(xs) | Expr::Tuple(xs) => xs.iter().for_each(go),
        Expr::Unary(_, x) | Expr::Member(x, _) | Expr::Is(x, _) | Expr::Annotated(x, _) => go(x),
        Expr::Binary(_, a, b) | Expr::Index(a, b) => {
            go(a);
            go(b);
        }
        Expr::Call(g, args) => {
            go(g);
            args.iter().for_each(|a| go(&a.value));
        }
        Expr::If { cond, then, other } => {
            go(cond);
            go(then);
            go(other);
        }
        Expr::IfLet { value, then, other, .. } => {
            go(value);
            go(then);
            go(other);
        }
        Expr::Match { scrutinee, arms } => {
            go(scrutinee);
            for a in arms {
                if let Some(g) = &a.guard {
                    go(g);
                }
                go(&a.body);
            }
        }
        Expr::Let { value, body, .. } => {
            go(value);
            go(body);
        }
        Expr::Comprehension { source, filter, body, .. } => {
            go(source);
            filter.iter().for_each(|x| go(x));
            body.iter().for_each(|x| go(x));
        }
        Expr::Group { source, key, filter, fields } => {
            go(source);
            go(key);
            filter.iter().for_each(|x| go(x));
            fields.iter().for_each(|(_, x)| go(x));
        }
        Expr::Optimize { filter, objective, constraints, .. } => {
            filter.iter().for_each(|x| go(x));
            go(objective);
            constraints.iter().for_each(go);
        }
        Expr::Forall { domain, body, .. } => {
            if let Domain::In(s) = domain {
                go(s);
            }
            go(body);
        }
    }
}

/// The expressions a declaration evaluates.
fn decl_exprs(d: &Decl) -> Vec<&Expr> {
    match &d.kind {
        DeclKind::Param { default, .. } => default.iter().collect(),
        DeclKind::View(e) | DeclKind::Invariant(e) | DeclKind::Theorem(e) => vec![e],
        DeclKind::Fn { body, .. } => body.iter().collect(),
        DeclKind::Alloc { body, .. } => vec![body],
        DeclKind::Partition { split, split_by, more_splits, cases, .. } => {
            let mut v = vec![split, split_by];
            v.extend(more_splits.iter().map(|(_, e)| e));
            for c in cases {
                v.push(&c.cond);
                v.extend(c.lets.iter().map(|(_, e)| e));
                v.extend(c.outputs.iter().map(|(_, e)| e));
            }
            v
        }
        _ => vec![],
    }
}

/// The declarations `d` reaches (itself included), through the names its
/// expressions use.
pub fn reachable<'m>(m: &'m Module, d: &'m Decl) -> BTreeSet<&'m str> {
    let by_name: BTreeMap<&str, &Decl> = m.decls.iter().filter(|d| !matches!(d.kind, DeclKind::Partition { .. })).map(|d| (d.name.as_str(), d)).collect();
    let mut seen: BTreeSet<&str> = BTreeSet::from([d.name.as_str()]);
    let mut todo: Vec<&Decl> = vec![d];
    while let Some(d) = todo.pop() {
        for e in decl_exprs(d) {
            walk(e, &mut |x| {
                if let Expr::Ident(n, _) = x
                    && let Some(&t) = by_name.get(n.as_str())
                    && seen.insert(t.name.as_str())
                {
                    todo.push(t);
                }
            });
        }
    }
    seen
}

/// `cell(form, key)` calls in `d`'s own expressions, as (form, key).
fn cells_in(d: &Decl) -> Result<BTreeSet<(String, String)>, String> {
    let mut out = BTreeSet::new();
    let mut err = None;
    for e in decl_exprs(d) {
        walk(e, &mut |x| {
            if let Expr::Call(g, args) = x
                && matches!(&**g, Expr::Ident(n, _) if n == "cell")
            {
                match args.as_slice() {
                    [Arg { value: Expr::Str(f, _), .. }, Arg { value: Expr::Str(k, _), .. }] => {
                        out.insert((f.clone(), k.clone()));
                    }
                    _ => err = Some(format!("`{}`: cell(..) takes two string literals (form, key)", d.name)),
                }
            }
        });
    }
    err.map_or(Ok(out), Err)
}

/// For each partition target, the cells of other returns it reads. Errors on
/// a cycle (a partition reading the form it feeds) or a malformed `cell`.
pub fn stages(m: &Module) -> Result<BTreeMap<String, BTreeSet<(String, String)>>, String> {
    let mut out = BTreeMap::new();
    for p in m.decls.iter().filter(|d| matches!(d.kind, DeclKind::Partition { .. })) {
        let DeclKind::Partition { targets, .. } = &p.kind else { unreachable!() };
        let mut cells = BTreeSet::new();
        for name in reachable(m, p) {
            let d = if name == p.name { p } else { m.decls.iter().find(|d| d.name == name).unwrap() };
            cells.extend(cells_in(d)?);
        }
        for t in targets {
            let code = form_code(t);
            if let Some((f, k)) = cells.iter().find(|(f, _)| *f == code) {
                return Err(format!("{}: partition into {t} reads cell({f:?}, {k:?}) of the same return: a cycle", p.pos));
            }
            out.insert(t.clone(), cells.clone());
        }
    }
    Ok(out)
}
