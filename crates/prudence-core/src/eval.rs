//! Evaluator: exact rational arithmetic over a statically validated form.

use std::collections::HashMap;

use num_traits::Zero;

use crate::ir::{BinOp, Cell, Cmp, CmpOp, Expr, Form, Ref};
use crate::num::{Q, display};

/// A cell's value. A ratio over a zero denominator is `Undefined` rather than
/// an error, so the rest of the return still evaluates (e.g. LCR with no NCO).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(Q),
    Undefined,
}

impl Value {
    pub fn num(&self) -> Option<&Q> {
        match self {
            Value::Num(q) => Some(q),
            Value::Undefined => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missing {
    /// Every input must be supplied (ARS 210.0: an amount in every field).
    Error,
    /// Use spec defaults where they exist; otherwise error.
    Default,
    /// Use spec defaults where they exist; otherwise zero.
    Zero,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// From a `check warn`: worth a look, not an error.
    pub warn: bool,
    pub check_id: String,
    pub description: String,
    pub detail: String,
}

#[derive(Debug)]
pub struct Evaluation<'f> {
    pub form: &'f Form,
    pub values: HashMap<Ref, Value>,
    pub findings: Vec<Finding>,
}

impl Evaluation<'_> {
    /// Look up by key ("1.1", "16.4:weight"). Panics on an unknown key:
    /// callers address cells from the spec, so a miss is a programming error.
    pub fn get(&self, key: &str) -> &Value {
        let r = self.form.resolve(key).unwrap_or_else(|e| panic!("{e}"));
        &self.values[&r]
    }

    /// Numeric value; panics if the cell is undefined.
    pub fn q(&self, key: &str) -> &Q {
        self.get(key).num().unwrap_or_else(|| panic!("{key} is undefined"))
    }

    pub fn ok(&self) -> bool {
        self.findings.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputError {
    UnknownCell(String),
    NotAnInput(String),
    Missing(Vec<Ref>),
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputError::UnknownCell(m) => write!(f, "{m}"),
            InputError::NotAnInput(r) => write!(f, "{r} is derived, not an input"),
            InputError::Missing(rs) => {
                let shown: Vec<String> = rs.iter().take(20).map(|r| r.to_string()).collect();
                write!(f, "missing inputs {}", shown.join(", "))?;
                if rs.len() > 20 {
                    write!(f, " (+{} more)", rs.len() - 20)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for InputError {}

pub fn evaluate<'f, K: AsRef<str>>(
    form: &'f Form,
    inputs: impl IntoIterator<Item = (K, Q)>,
    missing: Missing,
) -> Result<Evaluation<'f>, InputError> {
    let mut values: HashMap<Ref, Value> = HashMap::new();
    for (k, v) in inputs {
        let r = form.resolve(k.as_ref()).map_err(InputError::UnknownCell)?;
        match form.cell(&r) {
            Some(Cell::Input { .. }) => {}
            _ => return Err(InputError::NotAnInput(r.to_string())),
        }
        values.insert(r, Value::Num(v));
    }
    let mut absent = Vec::new();
    for (r, cell) in form.all_cells() {
        let Cell::Input { default } = cell else { continue };
        if values.contains_key(&r) {
            continue;
        }
        match (missing, default) {
            (Missing::Default | Missing::Zero, Some(d)) => {
                values.insert(r, Value::Num(d.clone()));
            }
            (Missing::Zero, None) => {
                values.insert(r, Value::Num(Q::zero()));
            }
            _ => absent.push(r),
        }
    }
    if !absent.is_empty() {
        return Err(InputError::Missing(absent));
    }

    // Cells form a DAG (checked when the form was built), so plain memoised
    // recursion terminates.
    let refs: Vec<Ref> = form.all_cells().map(|(r, _)| r).collect();
    for r in &refs {
        value_of(form, r, &mut values);
    }

    let mut findings = Vec::new();
    for chk in &form.checks {
        let holds = cmp(&chk.cond, form, &mut values);
        if holds != Some(true) {
            let detail = match (eval(&chk.cond.a, form, &mut values), eval(&chk.cond.b, form, &mut values)) {
                (Value::Num(a), Value::Num(b)) => format!("{} vs {} ({})", display(&a), display(&b), chk.cond),
                _ => format!("cannot be evaluated: {}", chk.cond),
            };
            findings.push(Finding { warn: chk.warn, check_id: chk.id.clone(), description: chk.description.clone(), detail });
        }
    }
    Ok(Evaluation { form, values, findings })
}

fn value_of(form: &Form, r: &Ref, values: &mut HashMap<Ref, Value>) -> Value {
    if let Some(v) = values.get(r) {
        return v.clone();
    }
    let Some(Cell::Derived(e)) = form.cell(r) else {
        unreachable!("inputs are pre-populated; references are validated")
    };
    let v = eval(e, form, values);
    values.insert(r.clone(), v.clone());
    v
}

fn eval(e: &Expr, form: &Form, values: &mut HashMap<Ref, Value>) -> Value {
    use Value::*;
    match e {
        Expr::Ref(r) => value_of(form, r, values),
        Expr::Const(q) => Num(q.clone()),
        Expr::Neg(x) => match eval(x, form, values) {
            Num(q) => Num(-q),
            Undefined => Undefined,
        },
        Expr::Bin(op, a, b) => {
            let (Num(a), Num(b)) = (eval(a, form, values), eval(b, form, values)) else { return Undefined };
            match op {
                BinOp::Add => Num(a + b),
                BinOp::Sub => Num(a - b),
                BinOp::Mul => Num(a * b),
                BinOp::Div if b.is_zero() => Undefined,
                BinOp::Div => Num(a / b),
            }
        }
        Expr::Max(xs) | Expr::Min(xs) => {
            let mut best: Option<Q> = None;
            for x in xs {
                let Num(v) = eval(x, form, values) else { return Undefined };
                best = Some(match best {
                    None => v,
                    Some(b) if matches!(e, Expr::Max(_)) => b.max(v),
                    Some(b) => b.min(v),
                });
            }
            best.map(Num).unwrap_or(Undefined)
        }
        Expr::If(c, t, f) => match cmp(c, form, values) {
            Some(true) => eval(t, form, values),
            Some(false) => eval(f, form, values),
            None => Undefined,
        },
    }
}

/// A form theorem evaluated on one concrete return: the proven property,
/// checked again on the numbers about to be lodged.
pub fn holds(p: &crate::ir::Prop, ev: &mut Evaluation) -> Option<bool> {
    use crate::ir::Prop;
    Some(match p {
        Prop::Cmp(c) => cmp(c, ev.form, &mut ev.values)?,
        Prop::And(a, b) => holds(a, ev)? && holds(b, ev)?,
        Prop::Or(a, b) => holds(a, ev)? || holds(b, ev)?,
        Prop::Not(a) => !holds(a, ev)?,
        Prop::Implies(a, b) => !holds(a, ev)? || holds(b, ev)?,
    })
}

fn cmp(c: &Cmp, form: &Form, values: &mut HashMap<Ref, Value>) -> Option<bool> {
    let (Value::Num(a), Value::Num(b)) = (eval(&c.a, form, values), eval(&c.b, form, values)) else {
        return None;
    };
    Some(match c.op {
        CmpOp::Gt => a > b,
        CmpOp::Ge => a >= b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Eq => a == b,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::{int, parse_decimal};
    use crate::syntax::parse_form;

    fn form() -> Form {
        parse_form(
            r#"form T("T", "t") {
  columns amount, weighted
  input amount
  item "1" "a" {
    amount = input
    weighted = [1:amount] * 0.85
  }
  item "2" "b" {
    amount = input(default: 5)
  }
  item "3" "ratio" {
    amount = [1:weighted] / [2:amount]
  }
  check "pos" "a must be non-negative": [1:amount] >= 0
}
"#,
        )
        .unwrap()
    }

    #[test]
    fn evaluates_exactly() {
        let f = form();
        let ev = evaluate(&f, [("1", int(10)), ("2", int(3))], Missing::Error).unwrap();
        assert_eq!(ev.q("1:weighted"), &parse_decimal("8.5").unwrap());
        assert_eq!(ev.q("3"), &(parse_decimal("8.5").unwrap() / int(3)));
        assert!(ev.ok());
    }

    #[test]
    fn missing_policies() {
        let f = form();
        assert!(matches!(evaluate(&f, [("1", int(1))], Missing::Error), Err(InputError::Missing(_))));
        let ev = evaluate(&f, [("1", int(1))], Missing::Default).unwrap();
        assert_eq!(ev.q("2"), &int(5));
        let ev = evaluate(&f, std::iter::empty::<(&str, Q)>(), Missing::Zero).unwrap();
        assert_eq!(ev.q("1"), &int(0));
    }

    #[test]
    fn division_by_zero_is_undefined_not_a_crash() {
        let f = form();
        let ev = evaluate(&f, [("1", int(1)), ("2", int(0))], Missing::Error).unwrap();
        assert_eq!(ev.get("3"), &Value::Undefined);
    }

    #[test]
    fn strict_inputs_and_checks() {
        let f = form();
        assert!(matches!(evaluate(&f, [("9", int(1))], Missing::Zero), Err(InputError::UnknownCell(_))));
        assert!(matches!(evaluate(&f, [("3", int(1))], Missing::Zero), Err(InputError::NotAnInput(_))));
        let ev = evaluate(&f, [("1", int(-1))], Missing::Zero).unwrap();
        assert_eq!(ev.findings[0].check_id, "pos");
    }
}
