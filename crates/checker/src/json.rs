//! The package as JSON, so the checker can run as a separate program on what
//! the engine wrote out. Exact rationals are strings ("3/4", "-12").

use std::collections::BTreeMap;
use std::str::FromStr;

use prudence_core::Q;
use prudence_core::lp::{Lp, Rel, Sense, Solution};
use serde_json::{Value, json};

use crate::{Certificate, Package, Part, Record, Stream};

fn q(x: &Q) -> Value {
    Value::String(x.to_string())
}

fn qs(xs: &[Q]) -> Value {
    Value::Array(xs.iter().map(q).collect())
}

fn qmap(m: &BTreeMap<String, Q>) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.clone(), q(v))).collect())
}

pub fn to_json(p: &Package) -> Value {
    json!({
        "reporting_date": p.reporting_date,
        "gl": qmap(&p.gl),
        "records": p.records.iter().map(|r| json!({
            "id": r.id, "gl_account": r.gl_account, "balance": q(&r.balance), "split": r.split
        })).collect::<Vec<_>>(),
        "slices": p.slices.iter().map(|(r, a)| json!([r, q(a)])).collect::<Vec<_>>(),
        "parts": p.parts.iter().map(|x| json!({
            "record": x.record, "item": x.item, "currency": x.currency, "amount": q(&x.amount),
            "stream": format!("{:?}", x.stream)
        })).collect::<Vec<_>>(),
        "rates": qmap(&p.rates),
        "lcr_inputs": qmap(&p.lcr_inputs),
        "nsfr_inputs": qmap(&p.nsfr_inputs),
        "certificates": p.certificates.iter().map(|c| json!({
            "name": c.name,
            "lp": {
                "sense": if c.lp.sense == Sense::Max { "max" } else { "min" },
                "objective": qs(&c.lp.objective),
                "rows": c.lp.rows.iter().map(|(a, rel, b)| json!({
                    "coef": qs(a),
                    "rel": match rel { Rel::Le => "<=", Rel::Ge => ">=", Rel::Eq => "=" },
                    "rhs": q(b),
                })).collect::<Vec<_>>(),
            },
            "solution": { "x": qs(&c.solution.x), "value": q(&c.solution.value), "duals": qs(&c.solution.duals) },
        })).collect::<Vec<_>>(),
        "assumptions": p.assumptions.iter().map(|(a, b, c, d)| json!([a, b, c, d])).collect::<Vec<_>>(),
        "specs": p.specs.iter().map(|(path, src)| json!({ "path": path, "source": src })).collect::<Vec<_>>(),
    })
}

type R<T> = Result<T, String>;

fn get<'a>(v: &'a Value, k: &str) -> R<&'a Value> {
    v.get(k).ok_or_else(|| format!("missing `{k}`"))
}
fn s(v: &Value) -> R<String> {
    v.as_str().map(str::to_string).ok_or_else(|| format!("expected a string, got {v}"))
}
fn rq(v: &Value) -> R<Q> {
    let t = v.as_str().ok_or_else(|| format!("expected a rational string, got {v}"))?;
    Q::from_str(t).map_err(|e| format!("bad rational `{t}`: {e}"))
}
fn arr(v: &Value) -> R<&Vec<Value>> {
    v.as_array().ok_or_else(|| format!("expected an array, got {v}"))
}
fn rqs(v: &Value) -> R<Vec<Q>> {
    arr(v)?.iter().map(rq).collect()
}
fn rmap(v: &Value) -> R<BTreeMap<String, Q>> {
    v.as_object().ok_or("expected an object")?.iter().map(|(k, x)| Ok((k.clone(), rq(x)?))).collect()
}

pub fn from_json(v: &Value) -> R<Package> {
    Ok(Package {
        reporting_date: s(get(v, "reporting_date")?)?,
        gl: rmap(get(v, "gl")?)?,
        records: arr(get(v, "records")?)?
            .iter()
            .map(|r| {
                Ok(Record {
                    id: s(get(r, "id")?)?,
                    gl_account: s(get(r, "gl_account")?)?,
                    balance: rq(get(r, "balance")?)?,
                    split: get(r, "split")?.as_bool().ok_or("split: expected a bool")?,
                })
            })
            .collect::<R<_>>()?,
        slices: arr(get(v, "slices")?)?
            .iter()
            .map(|x| match arr(x)?.as_slice() {
                [r, a] => Ok((s(r)?, rq(a)?)),
                _ => Err("slice: expected [record, amount]".to_string()),
            })
            .collect::<R<_>>()?,
        parts: arr(get(v, "parts")?)?
            .iter()
            .map(|x| {
                Ok(Part {
                    record: s(get(x, "record")?)?,
                    item: s(get(x, "item")?)?,
                    currency: s(get(x, "currency")?)?,
                    amount: rq(get(x, "amount")?)?,
                    stream: match s(get(x, "stream")?)?.as_str() {
                        "Both" => Stream::Both,
                        "Total" => Stream::Total,
                        "Split" => Stream::Split,
                        o => return Err(format!("unknown stream {o}")),
                    },
                })
            })
            .collect::<R<_>>()?,
        rates: rmap(get(v, "rates")?)?,
        lcr_inputs: rmap(get(v, "lcr_inputs")?)?,
        nsfr_inputs: rmap(get(v, "nsfr_inputs")?)?,
        certificates: arr(get(v, "certificates")?)?
            .iter()
            .map(|c| {
                let lp = get(c, "lp")?;
                let sol = get(c, "solution")?;
                Ok(Certificate {
                    name: s(get(c, "name")?)?,
                    lp: Lp {
                        sense: if s(get(lp, "sense")?)? == "max" { Sense::Max } else { Sense::Min },
                        objective: rqs(get(lp, "objective")?)?,
                        rows: arr(get(lp, "rows")?)?
                            .iter()
                            .map(|r| {
                                let rel = match s(get(r, "rel")?)?.as_str() {
                                    "<=" => Rel::Le,
                                    ">=" => Rel::Ge,
                                    "=" => Rel::Eq,
                                    o => return Err(format!("unknown relation {o}")),
                                };
                                Ok((rqs(get(r, "coef")?)?, rel, rq(get(r, "rhs")?)?))
                            })
                            .collect::<R<_>>()?,
                    },
                    solution: Solution {
                        x: rqs(get(sol, "x")?)?,
                        value: rq(get(sol, "value")?)?,
                        duals: rqs(get(sol, "duals")?)?,
                    },
                })
            })
            .collect::<R<_>>()?,
        assumptions: arr(get(v, "assumptions")?)?
            .iter()
            .map(|x| match arr(x)?.as_slice() {
                [a, b, c, d] => Ok((s(a)?, s(b)?, s(c)?, s(d)?)),
                _ => Err("assumption: expected 4 strings".to_string()),
            })
            .collect::<R<_>>()?,
        specs: arr(get(v, "specs")?)?.iter().map(|x| Ok((s(get(x, "path")?)?, s(get(x, "source")?)?))).collect::<R<_>>()?,
    })
}
