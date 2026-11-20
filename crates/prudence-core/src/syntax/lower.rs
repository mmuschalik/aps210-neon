//! Lower a `form` declaration to the core IR evaluated by `eval`.

use super::ast::{self, BinOp, CellDef, Expr as SExpr, FormDecl, UnOp};
use crate::ir::{self, Cell, Check, Cmp, CmpOp, Expr, Form, Item, Prop, Ref, Theorem};
use crate::num::parse_decimal;

fn cite(anns: &[ast::Annotation]) -> Option<String> {
    anns.iter().find(|a| a.name == "cite").and_then(|a| match a.arg(None) {
        Some(SExpr::Str(s, _)) => Some(s.clone()),
        _ => None,
    })
}

pub fn lower_form(f: &FormDecl) -> Result<Form, String> {
    let mut items = Vec::new();
    for it in &f.items {
        let mut cells = Vec::new();
        for (col, def) in &it.cells {
            let cell = match def {
                CellDef::Input(None) => Cell::Input { default: None },
                CellDef::Input(Some(e)) => match expr(e)? {
                    Expr::Const(q) => Cell::Input { default: Some(q) },
                    Expr::Neg(x) => match *x {
                        Expr::Const(q) => Cell::Input { default: Some(-q) },
                        _ => return Err(format!("{}: item {} default must be a number", it.pos, it.id)),
                    },
                    _ => return Err(format!("{}: item {} default must be a number", it.pos, it.id)),
                },
                CellDef::Expr(e) => Cell::Derived(expr(e).map_err(|m| format!("{}: item {}: {m}", it.pos, it.id))?),
            };
            cells.push((col.clone(), cell));
        }
        items.push(Item {
            id: it.id.clone(),
            label: it.label.clone(),
            cite: cite(&it.anns),
            header: cells.is_empty(),
            cells,
        });
    }
    let mut checks = Vec::new();
    for c in &f.checks {
        checks.push(Check {
            warn: c.warn,
            id: c.id.clone(),
            description: c.description.clone(),
            cite: cite(&c.anns),
            cond: cmp(&c.cond).map_err(|m| format!("check {}: {m}", c.id))?,
        });
    }
    let mut theorems = Vec::new();
    for t in &f.theorems {
        theorems.push(Theorem {
            id: t.id.clone(),
            description: t.description.clone(),
            cite: cite(&t.anns),
            prop: prop(&t.cond).map_err(|m| format!("theorem {}: {m}", t.id))?,
        });
    }
    Form::new(f.code.clone(), f.title.clone(), f.columns.clone(), f.input.clone(), items, checks)?.with_theorems(theorems)
}

fn prop(e: &SExpr) -> Result<Prop, String> {
    Ok(match e {
        SExpr::Annotated(x, _) => prop(x)?,
        SExpr::Binary(BinOp::And, a, b) => Prop::And(Box::new(prop(a)?), Box::new(prop(b)?)),
        SExpr::Binary(BinOp::Or, a, b) => Prop::Or(Box::new(prop(a)?), Box::new(prop(b)?)),
        SExpr::Binary(BinOp::Implies, a, b) => Prop::Implies(Box::new(prop(a)?), Box::new(prop(b)?)),
        SExpr::Unary(UnOp::Not, a) => Prop::Not(Box::new(prop(a)?)),
        other => Prop::Cmp(cmp(other)?),
    })
}

fn cmp(e: &SExpr) -> Result<Cmp, String> {
    match e {
        SExpr::Annotated(x, _) => cmp(x),
        SExpr::Binary(op, a, b) => {
            let op = match op {
                BinOp::Eq => CmpOp::Eq,
                BinOp::Lt => CmpOp::Lt,
                BinOp::Le => CmpOp::Le,
                BinOp::Gt => CmpOp::Gt,
                BinOp::Ge => CmpOp::Ge,
                other => return Err(format!("expected a comparison, found {other:?}")),
            };
            Ok(Cmp { op, a: expr(a)?, b: expr(b)? })
        }
        _ => Err("expected a single comparison".into()),
    }
}

fn expr(e: &SExpr) -> Result<Expr, String> {
    Ok(match e {
        SExpr::Annotated(x, _) => expr(x)?,
        SExpr::Num(n, pos) => Expr::Const(parse_decimal(n).ok_or_else(|| format!("{pos}: bad number {n}"))?),
        SExpr::Cell(item, col, _) => Expr::Ref(Ref::new(item, col)),
        SExpr::Unary(UnOp::Neg, x) => Expr::Neg(Box::new(expr(x)?)),
        SExpr::Binary(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div), a, b) => {
            let op = match op {
                BinOp::Add => ir::BinOp::Add,
                BinOp::Sub => ir::BinOp::Sub,
                BinOp::Mul => ir::BinOp::Mul,
                _ => ir::BinOp::Div,
            };
            Expr::Bin(op, Box::new(expr(a)?), Box::new(expr(b)?))
        }
        SExpr::Call(f, args) => {
            let SExpr::Ident(name, _) = &**f else { return Err("only max(..) and min(..) calls in forms".into()) };
            let xs = args.iter().map(|a| expr(&a.value)).collect::<Result<Vec<_>, _>>()?;
            match name.as_str() {
                "max" => Expr::Max(xs),
                "min" => Expr::Min(xs),
                other => return Err(format!("unknown function `{other}` in a form")),
            }
        }
        SExpr::If { cond, then, other } => Expr::If(Box::new(cmp(cond)?), Box::new(expr(then)?), Box::new(expr(other)?)),
        other => return Err(format!("not allowed in a form cell: {other:?}")),
    })
}
