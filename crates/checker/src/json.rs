//! The package as JSON, so the checker can run as a separate program on what
//! the engine wrote out. Exact rationals are strings ("3/4", "-12").

use std::collections::BTreeMap;
use std::str::FromStr;

use prudence_core::Q;
use prudence_core::lp::{Lp, Rel, Sense, Solution};
use serde_json::{Value, json};

use crate::{AllocationSlice, Certificate, FcsTerms, FormContribution, Package, Part, Record, ReportingContext, Stream};

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
    let overlays: Vec<Value> = p
        .overlays
        .iter()
        .map(|o| {
            json!({
                "id": o.id, "kind": format!("{:?}", o.kind), "form": o.form, "cell": o.cell, "amount": q(&o.amount),
                "apra_instrument": o.apra_instrument, "reason": o.reason, "approver": o.approver,
                "effective": o.effective, "expiry": o.expiry, "signature": o.signature,
            })
        })
        .collect();
    let mut v = json!({
        "reporting_date": p.reporting_date,
        "contexts": p.contexts.iter().map(|(code, c)| (code.clone(), json!({
            "entity": c.entity, "reporting_date": c.reporting_date, "consolidation": c.consolidation, "regime": c.regime
        }))).collect::<serde_json::Map<_, _>>(),
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
        "external_sources": p.external_sources,
        "form_contributions": p.form_contributions.iter().map(|(code, parts)| (code.clone(),
            Value::Array(parts.iter().map(|p| json!({"source": p.source, "cell": p.cell, "amount": q(&p.amount)})).collect())
        )).collect::<serde_json::Map<_, _>>(),
        "lcr_inputs": qmap(&p.lcr_inputs),
        "nsfr_inputs": qmap(&p.nsfr_inputs),
        "optional_inputs": p.optional_inputs.iter().map(|(k, v)| (k.clone(), qmap(v))).collect::<serde_json::Map<_, _>>(),
        "certificates": p.certificates.iter().map(|c| json!({
            "name": c.name,
            "subjects": c.subjects,
            "fcs": c.fcs.as_ref().map(|t| json!({"wholesale": t.wholesale, "eligible": t.eligible,
                "operational": qs(&t.operational), "covered_rates": qs(&t.covered_rates), "uncovered_rates": qs(&t.uncovered_rates)})),
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
        "allocation_slices": p.allocation_slices.iter().map(|s| json!({
            "source": s.source, "amount": q(&s.amount), "covered": s.covered,
            "lcr_item": s.lcr_item, "nsfr_cell": s.nsfr_cell
        })).collect::<Vec<_>>(),
        "assumptions": p.assumptions.iter().map(|(a, b, c, d)| json!([a, b, c, d])).collect::<Vec<_>>(),
        "specs": p.specs.iter().map(|(path, src)| json!({ "path": path, "source": src })).collect::<Vec<_>>(),
    });
    v["overlays"] = Value::Array(overlays);
    v
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
        contexts: match v.get("contexts") {
            None => BTreeMap::new(),
            Some(x) => x
                .as_object()
                .ok_or("contexts: expected object")?
                .iter()
                .map(|(code, c)| {
                    Ok((
                        code.clone(),
                        ReportingContext {
                            entity: s(get(c, "entity")?)?,
                            reporting_date: s(get(c, "reporting_date")?)?,
                            consolidation: s(get(c, "consolidation")?)?,
                            regime: s(get(c, "regime")?)?,
                        },
                    ))
                })
                .collect::<R<_>>()?,
        },
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
        form_contributions: match v.get("form_contributions") {
            None => BTreeMap::new(),
            Some(x) => x
                .as_object()
                .ok_or("form_contributions: expected object")?
                .iter()
                .map(|(code, parts)| {
                    Ok((
                        code.clone(),
                        arr(parts)?
                            .iter()
                            .map(|p| {
                                Ok(FormContribution {
                                    source: s(get(p, "source")?)?,
                                    cell: s(get(p, "cell")?)?,
                                    amount: rq(get(p, "amount")?)?,
                                })
                            })
                            .collect::<R<_>>()?,
                    ))
                })
                .collect::<R<_>>()?,
        },
        external_sources: match v.get("external_sources") {
            None => BTreeMap::new(),
            Some(x) => x
                .as_object()
                .ok_or("external_sources: expected object")?
                .iter()
                .map(|(k, v)| Ok((k.clone(), s(v)?)))
                .collect::<R<_>>()?,
        },
        rates: rmap(get(v, "rates")?)?,
        lcr_inputs: rmap(get(v, "lcr_inputs")?)?,
        nsfr_inputs: rmap(get(v, "nsfr_inputs")?)?,
        optional_inputs: match v.get("optional_inputs") {
            None => BTreeMap::new(),
            Some(x) => x
                .as_object()
                .ok_or("optional_inputs: expected an object")?
                .iter()
                .map(|(k, v)| Ok((k.clone(), rmap(v)?)))
                .collect::<R<_>>()?,
        },
        overlays: match v.get("overlays") {
            None => Vec::new(),
            Some(o) => arr(o)?
                .iter()
                .map(|o| {
                    Ok(crate::Overlay {
                        id: s(get(o, "id")?)?,
                        kind: match s(get(o, "kind")?)?.as_str() {
                            "ApraDiscretion" => crate::OverlayKind::ApraDiscretion,
                            "DataPatch" => crate::OverlayKind::DataPatch,
                            k => return Err(format!("unknown overlay kind {k}")),
                        },
                        form: s(get(o, "form")?)?,
                        cell: s(get(o, "cell")?)?,
                        amount: rq(get(o, "amount")?)?,
                        apra_instrument: get(o, "apra_instrument")?.as_str().map(str::to_string),
                        reason: s(get(o, "reason")?)?,
                        approver: s(get(o, "approver")?)?,
                        effective: s(get(o, "effective")?)?,
                        expiry: s(get(o, "expiry")?)?,
                        signature: s(get(o, "signature")?)?,
                    })
                })
                .collect::<R<_>>()?,
        },
        certificates: arr(get(v, "certificates")?)?
            .iter()
            .map(|c| {
                let lp = get(c, "lp")?;
                let sol = get(c, "solution")?;
                Ok(Certificate {
                    name: s(get(c, "name")?)?,
                    subjects: match c.get("subjects") {
                        None => vec![],
                        Some(v) => arr(v)?.iter().map(s).collect::<R<_>>()?,
                    },
                    fcs: match c.get("fcs") {
                        None | Some(Value::Null) => None,
                        Some(t) => Some(FcsTerms {
                            wholesale: get(t, "wholesale")?.as_bool().ok_or("wholesale: expected bool")?,
                            eligible: arr(get(t, "eligible")?)?
                                .iter()
                                .map(|v| v.as_bool().ok_or("eligible: expected bool".into()))
                                .collect::<R<_>>()?,
                            operational: rqs(get(t, "operational")?)?,
                            covered_rates: rqs(get(t, "covered_rates")?)?,
                            uncovered_rates: rqs(get(t, "uncovered_rates")?)?,
                        }),
                    },
                    lp: Lp {
                        sense: match s(get(lp, "sense")?)?.as_str() {
                            "max" => Sense::Max,
                            "min" => Sense::Min,
                            other => return Err(format!("unknown LP sense {other}")),
                        },
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
        allocation_slices: match v.get("allocation_slices") {
            None => vec![],
            Some(v) => arr(v)?
                .iter()
                .map(|v| {
                    Ok(AllocationSlice {
                        source: s(get(v, "source")?)?,
                        amount: rq(get(v, "amount")?)?,
                        covered: get(v, "covered")?.as_bool().ok_or("covered: expected bool")?,
                        lcr_item: match get(v, "lcr_item")? {
                            Value::Null => None,
                            v => Some(s(v)?),
                        },
                        nsfr_cell: s(get(v, "nsfr_cell")?)?,
                    })
                })
                .collect::<R<_>>()?,
        },
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
