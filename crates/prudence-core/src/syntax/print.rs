//! Canonical printer for form specs (`prudence fmt`): core IR -> Prudence source.

use crate::ir::{BinOp, Cell, Cmp, CmpOp, Expr, Form, Prop};
use crate::num::display;

/// "ARF 210.1A" -> "Arf210_1A"
pub fn form_ident(code: &str) -> String {
    let rest = code.trim_start_matches("ARF").trim();
    format!("Arf{}", rest.replace('.', "_"))
}

fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn print_form(f: &Form, header: &[&str]) -> String {
    let mut s = String::new();
    for h in header {
        s.push_str(&format!("// {h}\n"));
    }
    s.push_str(&format!(
        "\n@reporting(\"ARS 210.0\", version: \"2023-03-31\")\nform {}({}, {}) {{\n  columns {}\n  input {}\n",
        form_ident(&f.code),
        q(&f.code),
        q(&f.title),
        f.columns.join(", "),
        f.default_input_col
    ));
    for it in &f.items {
        s.push('\n');
        let cite = it.cite.as_ref().map(|c| format!(" @cite({})", q(c))).unwrap_or_default();
        if it.cells.is_empty() {
            s.push_str(&format!("  item {} {} {{}}{cite}\n", q(&it.id), q(&it.label)));
            continue;
        }
        s.push_str(&format!("  item {} {} {{\n", q(&it.id), q(&it.label)));
        let w = it.cells.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
        for (col, cell) in &it.cells {
            let body = match cell {
                Cell::Input { default: None } => "input".to_string(),
                Cell::Input { default: Some(d) } => format!("input(default: {})", display(d)),
                Cell::Derived(e) => expr(e, 0),
            };
            s.push_str(&format!("    {col:<w$} = {body}\n"));
        }
        s.push_str(&format!("  }}{cite}\n"));
    }
    if !f.checks.is_empty() {
        s.push('\n');
    }
    for c in &f.checks {
        let cite = c.cite.as_ref().map(|x| format!(" @cite({})", q(x))).unwrap_or_default();
        let warn = if c.warn { "warn " } else { "" };
        s.push_str(&format!("  check {warn}{} {}: {}{cite}\n", q(&c.id), q(&c.description), cmp(&c.cond)));
    }
    if !f.theorems.is_empty() {
        s.push('\n');
    }
    for t in &f.theorems {
        let cite = t.cite.as_ref().map(|x| format!(" @cite({})", q(x))).unwrap_or_default();
        s.push_str(&format!("  theorem {} {}:\n    {}{cite}\n", q(&t.id), q(&t.description), prop(&t.prop, 0)));
    }
    s.push_str("}\n");
    s
}

/// `ctx`: 0 top (implies), 1 or, 2 and, 3 atom.
fn prop(p: &Prop, ctx: u8) -> String {
    let wrap = |s: String, lvl: u8| if lvl < ctx { format!("({s})") } else { s };
    match p {
        Prop::Cmp(c) => cmp(c),
        Prop::Not(x) => format!("!({})", prop(x, 0)),
        Prop::And(a, b) => wrap(format!("{} && {}", prop(a, 2), prop(b, 3)), 2),
        Prop::Or(a, b) => wrap(format!("{} || {}", prop(a, 1), prop(b, 2)), 1),
        Prop::Implies(a, b) => wrap(format!("{}\n    ==> {}", prop(a, 1), prop(b, 0)), 0),
    }
}

fn cmp(c: &Cmp) -> String {
    let op = match c.op {
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Eq => "==",
    };
    format!("{} {op} {}", expr(&c.a, 1), expr(&c.b, 1))
}

/// `ctx`: 0 top level, 1 additive operand, 2 multiplicative operand, 3 unary operand.
fn expr(e: &Expr, ctx: u8) -> String {
    let wrap = |s: String, p: u8| if p < ctx { format!("({s})") } else { s };
    match e {
        Expr::Ref(r) => r.to_string(),
        Expr::Const(q) => display(q),
        Expr::Neg(x) => wrap(format!("-{}", expr(x, 3)), 3),
        Expr::Bin(op, a, b) => {
            let (sym, p) = match op {
                BinOp::Add => ("+", 1),
                BinOp::Sub => ("-", 1),
                BinOp::Mul => ("*", 2),
                BinOp::Div => ("/", 2),
            };
            // Left-associative: the right operand needs parens at equal precedence.
            wrap(format!("{} {sym} {}", expr(a, p), expr(b, p + 1)), p)
        }
        Expr::Max(xs) | Expr::Min(xs) => {
            let name = if matches!(e, Expr::Max(_)) { "max" } else { "min" };
            format!("{name}({})", xs.iter().map(|x| expr(x, 0)).collect::<Vec<_>>().join(", "))
        }
        Expr::If(c, t, o) => wrap(format!("if {} then {} else {}", cmp(c), expr(t, 0), expr(o, 0)), 0),
    }
}
