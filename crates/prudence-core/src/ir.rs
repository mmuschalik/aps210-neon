//! Prudence core IR for regulatory forms.
//!
//! A form is a set of cells addressed by (item, column). Each cell is an input
//! or a derived expression. The expression language is deliberately tiny and
//! total: references, + - * /, max, min, if. No loops, no recursion beyond the
//! (statically acyclic) cell graph, so every evaluation terminates.

use std::collections::HashMap;
use std::fmt;

use crate::num::{Q, display};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ref {
    pub item: String,
    pub col: String,
}

impl Ref {
    pub fn new(item: impl Into<String>, col: impl Into<String>) -> Self {
        Ref { item: item.into(), col: col.into() }
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}:{}]", self.item, self.col)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Ref(Ref),
    Const(Q),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Max(Vec<Expr>),
    Min(Vec<Expr>),
    If(Box<Cmp>, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cmp {
    pub op: CmpOp,
    pub a: Expr,
    pub b: Expr,
}

impl Expr {
    pub fn refs(&self, out: &mut Vec<Ref>) {
        match self {
            Expr::Ref(r) => out.push(r.clone()),
            Expr::Const(_) => {}
            Expr::Neg(e) => e.refs(out),
            Expr::Bin(_, a, b) => {
                a.refs(out);
                b.refs(out);
            }
            Expr::Max(xs) | Expr::Min(xs) => xs.iter().for_each(|x| x.refs(out)),
            Expr::If(c, t, e) => {
                c.refs(out);
                t.refs(out);
                e.refs(out);
            }
        }
    }
}

impl Cmp {
    pub fn refs(&self, out: &mut Vec<Ref>) {
        self.a.refs(out);
        self.b.refs(out);
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn prec(e: &Expr) -> u8 {
            match e {
                Expr::Bin(BinOp::Add | BinOp::Sub, ..) => 1,
                Expr::Bin(BinOp::Mul | BinOp::Div, ..) => 2,
                _ => 3,
            }
        }
        match self {
            Expr::Ref(r) => write!(f, "{r}"),
            Expr::Const(q) => write!(f, "{}", display(q)),
            Expr::Neg(e) => {
                if prec(e) < 3 { write!(f, "-({e})") } else { write!(f, "-{e}") }
            }
            Expr::Bin(op, a, b) => {
                let p = prec(self);
                let sym = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                };
                if prec(a) < p { write!(f, "({a})")? } else { write!(f, "{a}")? }
                write!(f, " {sym} ")?;
                if prec(b) <= p && prec(b) < 3 { write!(f, "({b})") } else { write!(f, "{b}") }
            }
            Expr::Max(xs) | Expr::Min(xs) => {
                let name = if matches!(self, Expr::Max(_)) { "max" } else { "min" };
                write!(f, "{name}(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, ")")
            }
            Expr::If(c, t, e) => write!(f, "if({c}, {t}, {e})"),
        }
    }
}

impl fmt::Display for Cmp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sym = match self.op {
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Eq => "==",
        };
        write!(f, "{} {sym} {}", self.a, self.b)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// Reported by the ADI. `default` is used only when the caller opts in.
    Input { default: Option<Q> },
    Derived(Expr),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub label: String,
    pub cite: Option<String>,
    pub header: bool,
    /// Ordered: the first cell is the item's primary cell.
    pub cells: Vec<(String, Cell)>,
}

impl Item {
    pub fn cell(&self, col: &str) -> Option<&Cell> {
        self.cells.iter().find(|(c, _)| c == col).map(|(_, cell)| cell)
    }
}

/// A proposition over cells: comparisons combined with and/or/not/implies.
/// Used by form `theorem`s, which are proven for all inputs, not checked.
#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    Cmp(Cmp),
    And(Box<Prop>, Box<Prop>),
    Or(Box<Prop>, Box<Prop>),
    Not(Box<Prop>),
    Implies(Box<Prop>, Box<Prop>),
}

impl Prop {
    pub fn refs(&self, out: &mut Vec<Ref>) {
        match self {
            Prop::Cmp(c) => c.refs(out),
            Prop::Not(p) => p.refs(out),
            Prop::And(a, b) | Prop::Or(a, b) | Prop::Implies(a, b) => {
                a.refs(out);
                b.refs(out);
            }
        }
    }
}

impl fmt::Display for Prop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Prop::Cmp(c) => write!(f, "{c}"),
            Prop::Not(p) => write!(f, "!({p})"),
            Prop::And(a, b) => write!(f, "({a} && {b})"),
            Prop::Or(a, b) => write!(f, "({a} || {b})"),
            Prop::Implies(a, b) => write!(f, "({a} ==> {b})"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theorem {
    pub id: String,
    pub description: String,
    pub cite: Option<String>,
    pub prop: Prop,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub id: String,
    pub description: String,
    pub cite: Option<String>,
    pub cond: Cmp,
}

#[derive(Clone, Debug)]
pub struct Form {
    pub code: String,
    pub title: String,
    pub columns: Vec<String>,
    pub default_input_col: String,
    pub items: Vec<Item>,
    pub checks: Vec<Check>,
    /// Proof obligations over the form's formulas (see `syntax::prove`).
    pub theorems: Vec<Theorem>,
    index: HashMap<String, usize>,
}

impl Form {
    /// Build and statically validate a form: unique items, known columns, no
    /// dangling references, no cycles.
    pub fn new(
        code: String,
        title: String,
        columns: Vec<String>,
        default_input_col: String,
        items: Vec<Item>,
        checks: Vec<Check>,
    ) -> Result<Form, String> {
        let mut index = HashMap::new();
        for (i, it) in items.iter().enumerate() {
            if index.insert(it.id.clone(), i).is_some() {
                return Err(format!("{code}: duplicate item {}", it.id));
            }
            for (col, _) in &it.cells {
                if !columns.contains(col) {
                    return Err(format!("{code}: item {} uses unknown column {col:?}", it.id));
                }
            }
        }
        if !columns.contains(&default_input_col) {
            return Err(format!("{code}: default input column {default_input_col:?} not declared"));
        }
        let form = Form { code, title, columns, default_input_col, items, checks, theorems: Vec::new(), index };
        form.check_references()?;
        form.check_acyclic()?;
        Ok(form)
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.index.get(id).map(|&i| &self.items[i])
    }

    pub fn cell(&self, r: &Ref) -> Option<&Cell> {
        self.item(&r.item)?.cell(&r.col)
    }

    pub fn all_cells(&self) -> impl Iterator<Item = (Ref, &Cell)> {
        self.items
            .iter()
            .flat_map(|it| it.cells.iter().map(move |(c, cell)| (Ref::new(&it.id, c), cell)))
    }

    pub fn input_refs(&self) -> Vec<Ref> {
        self.all_cells().filter(|(_, c)| matches!(c, Cell::Input { .. })).map(|(r, _)| r).collect()
    }

    /// Resolve "1.1", "16.4:weight" to a cell reference. Without a column: the
    /// form's default input column if the item has it, else the item's only
    /// input cell, else its first (primary) cell.
    pub fn resolve(&self, key: &str) -> Result<Ref, String> {
        if let Some((item, col)) = key.split_once(':') {
            let r = Ref::new(item, col);
            return match self.cell(&r) {
                Some(_) => Ok(r),
                None => Err(format!("{}: no cell {r}", self.code)),
            };
        }
        let it = self.item(key).ok_or_else(|| format!("{}: no item {key}", self.code))?;
        if it.cell(&self.default_input_col).is_some() {
            return Ok(Ref::new(key, &self.default_input_col));
        }
        let inputs: Vec<_> = it.cells.iter().filter(|(_, c)| matches!(c, Cell::Input { .. })).collect();
        if inputs.len() == 1 {
            return Ok(Ref::new(key, &inputs[0].0));
        }
        it.cells
            .first()
            .map(|(c, _)| Ref::new(key, c))
            .ok_or_else(|| format!("{}: item {key} is a header with no cells", self.code))
    }

    fn check_references(&self) -> Result<(), String> {
        let mut refs = Vec::new();
        for (at, cell) in self.all_cells() {
            if let Cell::Derived(e) = cell {
                refs.clear();
                e.refs(&mut refs);
                for r in &refs {
                    if self.cell(r).is_none() {
                        return Err(format!("{}: {at} refers to missing cell {r}", self.code));
                    }
                }
            }
        }
        for chk in &self.checks {
            refs.clear();
            chk.cond.refs(&mut refs);
            for r in &refs {
                if self.cell(r).is_none() {
                    return Err(format!("{}: check {} refers to missing cell {r}", self.code, chk.id));
                }
            }
        }
        Ok(())
    }

    /// Attach theorems, checking that every cell they mention exists.
    pub fn with_theorems(mut self, theorems: Vec<Theorem>) -> Result<Form, String> {
        let mut refs = Vec::new();
        for t in &theorems {
            refs.clear();
            t.prop.refs(&mut refs);
            if let Some(r) = refs.iter().find(|r| self.cell(r).is_none()) {
                return Err(format!("{}: theorem {} refers to missing cell {r}", self.code, t.id));
            }
        }
        self.theorems = theorems;
        Ok(self)
    }

    fn check_acyclic(&self) -> Result<(), String> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Visiting,
            Done,
        }
        let mut marks: HashMap<Ref, Mark> = HashMap::new();
        fn visit(form: &Form, r: &Ref, marks: &mut HashMap<Ref, Mark>) -> Result<(), String> {
            match marks.get(r) {
                Some(Mark::Done) => return Ok(()),
                Some(Mark::Visiting) => return Err(format!("{}: cycle through {r}", form.code)),
                None => {}
            }
            marks.insert(r.clone(), Mark::Visiting);
            if let Some(Cell::Derived(e)) = form.cell(r) {
                let mut refs = Vec::new();
                e.refs(&mut refs);
                for d in &refs {
                    visit(form, d, marks)?;
                }
            }
            marks.insert(r.clone(), Mark::Done);
            Ok(())
        }
        for (r, _) in self.all_cells() {
            visit(self, &r, &mut marks)?;
        }
        Ok(())
    }

    /// Human-readable spec, for review against the source document.
    pub fn describe(&self) -> String {
        let mut s = format!("{} - {}\n", self.code, self.title);
        for it in &self.items {
            s.push_str(&format!("  {:<10} {}{}\n", it.id, it.label, if it.header { "  (header)" } else { "" }));
            for (col, cell) in &it.cells {
                let body = match cell {
                    Cell::Input { default: None } => "INPUT".to_string(),
                    Cell::Input { default: Some(d) } => format!("INPUT (default {})", display(d)),
                    Cell::Derived(e) => e.to_string(),
                };
                s.push_str(&format!("      {col:<9} = {body}\n"));
            }
        }
        for c in &self.checks {
            s.push_str(&format!("  CHECK {}: {}  -- {}\n", c.id, c.cond, c.description));
        }
        s
    }
}
