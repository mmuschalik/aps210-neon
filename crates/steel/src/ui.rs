//! Self-contained interactive HTML workbench for ARF 210.1A (LCR) & ARF 210.6 (NSFR)
//! with formula inspection, per-item contract drilldown, cross-form contract trace,
//! Level 2 trapped-surplus waterfall, significant currencies, and APS 210 stress suite.

use std::collections::{BTreeMap, BTreeSet};

use assembly::level2::{JurisdictionPolicy, SubsidiaryPosition, exclude_trapped_surplus, significant_currencies};
use assembly::steering::{aps210_standard_suite, stress_full};
use assembly::{Bank, Return, Stream};
use chrono::NaiveDate;
use num_traits::ToPrimitive;
use prudence_core::ir::{Cell, Expr};
use prudence_core::num::int;
use prudence_core::{Evaluation, Missing, Q, Ref, evaluate};
use serde_json::{Value, json};

const TEMPLATE: &str = include_str!("ui.html");
const MAX_CONTRIBS_PER_ITEM: usize = 60;
const MAX_INDEXED_CONTRACTS: usize = 1500;

fn f64_opt(q: Option<&Q>) -> Value {
    match q.and_then(|v| v.to_f64()) {
        Some(x) => json!((x * 100.0).round() / 100.0),
        None => Value::Null,
    }
}

fn f64_val(q: &Q) -> f64 {
    q.to_f64().unwrap_or(0.0)
}

fn collect_refs(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::Ref(r) => {
            out.insert(r.item.clone());
        }
        Expr::Neg(a) => collect_refs(a, out),
        Expr::Bin(_, a, b) => {
            collect_refs(a, out);
            collect_refs(b, out);
        }
        Expr::Max(args) | Expr::Min(args) => {
            for a in args {
                collect_refs(a, out);
            }
        }
        Expr::If(c, t, e) => {
            collect_refs(&c.a, out);
            collect_refs(&c.b, out);
            collect_refs(t, out);
            collect_refs(e, out);
        }
        Expr::Const(_) => {}
    }
}

fn fmt_expr(expr: &Expr) -> String {
    match expr {
        Expr::Const(q) => format!("{q}"),
        Expr::Ref(r) => format!("[{}:{}]", r.item, r.col),
        Expr::Neg(a) => format!("-({})", fmt_expr(a)),
        Expr::Bin(op, a, b) => {
            let sym = match op {
                prudence_core::ir::BinOp::Add => "+",
                prudence_core::ir::BinOp::Sub => "-",
                prudence_core::ir::BinOp::Mul => "*",
                prudence_core::ir::BinOp::Div => "/",
            };
            format!("({} {sym} {})", fmt_expr(a), fmt_expr(b))
        }
        Expr::Max(args) => format!("max({})", args.iter().map(fmt_expr).collect::<Vec<_>>().join(", ")),
        Expr::Min(args) => format!("min({})", args.iter().map(fmt_expr).collect::<Vec<_>>().join(", ")),
        Expr::If(c, t, e) => format!("if({} {:?} {}, {}, {})", fmt_expr(&c.a), c.op, fmt_expr(&c.b), fmt_expr(t), fmt_expr(e)),
    }
}

fn cell_num<'a>(ev: &'a Evaluation<'a>, id: &str, col: &str) -> Option<&'a Q> {
    ev.values.get(&Ref::new(id, col)).and_then(|v| v.num())
}

/// Render the complete interactive Form & Drilldown HTML page.
pub fn render(
    adi_name: &str,
    bank: &Bank,
    ret: &Return,
    subs: &[SubsidiaryPosition],
    rd: NaiveDate,
    total_records: usize,
) -> Result<String, String> {
    let million = int(1_000_000);
    let zero = Q::default();

    let lcr_form = arf210::lcr_all();
    let nsfr_form = arf210::nsfr();
    let lcr_ev = evaluate(lcr_form, ret.inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero)
        .map_err(|e| format!("LCR eval: {e:?}"))?;
    let nsfr_ev = evaluate(nsfr_form, ret.nsfr_inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero)
        .map_err(|e| format!("NSFR eval: {e:?}"))?;

    // Index LCR parts by item (excluding Split-only rows so sums match the form)
    let mut lcr_by_item: BTreeMap<&str, Vec<(&str, &str, &str, Q)>> = BTreeMap::new();
    for p in &ret.parts {
        if p.stream == Stream::Split || p.amount == zero {
            continue;
        }
        lcr_by_item
            .entry(p.item.as_str())
            .or_default()
            .push((p.record.as_str(), p.module, p.currency.as_str(), &p.amount / &million));
    }

    // Index NSFR parts by bare item ID (cell is "item:b0" or "item")
    let mut nsfr_by_item: BTreeMap<String, Vec<(&str, &str, Q)>> = BTreeMap::new();
    for p in &ret.nsfr_parts {
        if p.amount == zero {
            continue;
        }
        let item_id = p.cell.split(':').next().unwrap_or(p.cell.as_str()).to_string();
        nsfr_by_item
            .entry(item_id)
            .or_default()
            .push((p.source.as_str(), p.cell.as_str(), p.amount.clone()));
    }

    // Collect top contributing contracts for cross-form impact tab
    let mut contract_lcr: BTreeMap<String, (&str, Vec<Value>)> = BTreeMap::new();
    let mut contract_nsfr: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut featured_ids: BTreeSet<String> = BTreeSet::new();

    // Build ARF 210.1A JSON rows
    let mut lcr_items = Vec::new();
    for item in &lcr_form.items {
        let amt = cell_num(&lcr_ev, &item.id, "amount");
        let wtd = cell_num(&lcr_ev, &item.id, "weighted").or_else(|| cell_num(&lcr_ev, &item.id, "value"));
        let wt = match item.cell("weight") {
            Some(Cell::Derived(Expr::Const(q))) => Some(q),
            _ => None,
        };
        let is_input = matches!(item.cell("amount"), Some(Cell::Input { .. }));
        let is_zero = amt.is_none_or(|v| *v == zero) && wtd.is_none_or(|v| *v == zero);

        let mut deps = BTreeSet::new();
        let mut formula = None;
        for col in ["weighted", "value", "amount"] {
            if let Some(Cell::Derived(e)) = item.cell(col) {
                collect_refs(e, &mut deps);
                if formula.is_none() {
                    formula = Some(format!("{col} = {}", fmt_expr(e)));
                }
            }
        }
        deps.remove(&item.id);

        let mut list = lcr_by_item.remove(item.id.as_str()).unwrap_or_default();
        let contrib_total = list.len();
        list.sort_by(|a, b| f64_val(&b.3).abs().total_cmp(&f64_val(&a.3).abs()));
        let top: Vec<Value> = list
            .into_iter()
            .take(MAX_CONTRIBS_PER_ITEM)
            .map(|(id, module, ccy, amt_m)| {
                if featured_ids.len() < MAX_INDEXED_CONTRACTS {
                    featured_ids.insert(id.to_string());
                }
                json!({ "id": id, "module": module, "ccy": ccy, "amount": f64_val(&amt_m) })
            })
            .collect();

        lcr_items.push(json!({
            "id": item.id,
            "label": item.label,
            "is_input": is_input,
            "is_zero": is_zero,
            "amount": f64_opt(amt),
            "weight": wt.and_then(|q| q.to_f64()),
            "weighted": f64_opt(wtd),
            "formula": formula,
            "deps": deps.into_iter().collect::<Vec<_>>(),
            "cite": item.cite,
            "contrib_total": contrib_total,
            "contribs": top,
        }));
    }

    // Build ARF 210.6 JSON rows
    let mut nsfr_items = Vec::new();
    for item in &nsfr_form.items {
        let b0 = cell_num(&nsfr_ev, &item.id, "b0").or_else(|| cell_num(&nsfr_ev, &item.id, "amount"));
        let b1 = cell_num(&nsfr_ev, &item.id, "b1");
        let b2 = cell_num(&nsfr_ev, &item.id, "b2");
        let calc = cell_num(&nsfr_ev, &item.id, "calc");
        let val = cell_num(&nsfr_ev, &item.id, "value").map(|v| v * int(100));
        let eff_calc = val.as_ref().or(calc);
        let is_input = ["b0", "b1", "b2", "amount"]
            .iter()
            .any(|c| matches!(item.cell(c), Some(Cell::Input { .. })));
        let is_zero = [b0, b1, b2, eff_calc].iter().all(|o| o.is_none_or(|v| *v == zero));

        let mut deps = BTreeSet::new();
        let mut formula = None;
        for col in ["value", "calc", "amount"] {
            if let Some(Cell::Derived(e)) = item.cell(col) {
                collect_refs(e, &mut deps);
                if formula.is_none() {
                    formula = Some(format!("{col} = {}", fmt_expr(e)));
                }
            }
        }
        deps.remove(&item.id);

        let mut list = nsfr_by_item.remove(&item.id).unwrap_or_default();
        let contrib_total = list.len();
        list.sort_by(|a, b| f64_val(&b.2).abs().total_cmp(&f64_val(&a.2).abs()));
        let top: Vec<Value> = list
            .into_iter()
            .take(MAX_CONTRIBS_PER_ITEM)
            .map(|(id, cell, amt_m)| {
                if featured_ids.len() < MAX_INDEXED_CONTRACTS {
                    featured_ids.insert(id.to_string());
                }
                json!({ "id": id, "cell": cell, "amount": f64_val(&amt_m) })
            })
            .collect();

        nsfr_items.push(json!({
            "id": item.id,
            "label": item.label,
            "is_input": is_input,
            "is_zero": is_zero,
            "b0": f64_opt(b0),
            "b1": f64_opt(b1),
            "b2": f64_opt(b2),
            "calc": f64_opt(eff_calc),
            "formula": formula,
            "deps": deps.into_iter().collect::<Vec<_>>(),
            "cite": item.cite,
            "contrib_total": contrib_total,
            "contribs": top,
        }));
    }

    // Build cross-form index for all featured contracts
    for p in &ret.parts {
        if p.stream != Stream::Split && p.amount != zero && featured_ids.contains(&p.record) {
            let entry = contract_lcr.entry(p.record.clone()).or_insert((p.module, Vec::new()));
            entry.1.push(json!({ "item": p.item, "ccy": p.currency, "amount": f64_val(&(&p.amount / &million)) }));
        }
    }
    for p in &ret.nsfr_parts {
        if p.amount != zero && featured_ids.contains(&p.source) {
            contract_nsfr
                .entry(p.source.clone())
                .or_default()
                .push(json!({ "cell": p.cell, "amount": f64_val(&p.amount) }));
        }
    }
    let mut contracts = serde_json::Map::new();
    for id in featured_ids {
        let (module, lcr_list) = contract_lcr.remove(&id).unwrap_or(("nsfr", Vec::new()));
        let nsfr_list = contract_nsfr.remove(&id).unwrap_or_default();
        contracts.insert(id, json!({ "module": module, "lcr": lcr_list, "nsfr": nsfr_list }));
    }

    // Level 2 trapped surplus
    let mut level2_rows = Vec::new();
    let mut trapped_total = Q::default();
    for sub in subs {
        let pol = JurisdictionPolicy::for_jurisdiction(sub.jurisdiction);
        let ex = exclude_trapped_surplus(sub, &pol);
        trapped_total += &ex.trapped_total;
        level2_rows.push(json!({
            "entity": ex.entity,
            "jurisdiction": format!("{:?}", sub.jurisdiction),
            "total_surplus": f64_val(&(&ex.total_surplus / &million)),
            "trapped_total": f64_val(&(&ex.trapped_total / &million)),
            "excluded_hqla1": f64_val(&(&ex.excluded_hqla1 / &million)),
            "excluded_hqla2a": f64_val(&(&ex.excluded_hqla2a / &million)),
            "consolidated_hqla1": f64_val(&(&ex.consolidated_hqla1 / &million)),
        }));
    }

    // Significant currencies (item 35)
    let mut liab_by_ccy: BTreeMap<String, Q> = BTreeMap::new();
    for (id, kind, bal, _) in &ret.records {
        if matches!(*kind, "retail" | "wholesale" | "repo-funding" | "issued" | "capital") {
            let c = bank.currency.get(id).cloned().unwrap_or_else(|| "AUD".into());
            *liab_by_ccy.entry(c).or_default() += bal.clone();
        }
    }
    let sig_ccy: Vec<Value> = significant_currencies(&ret.parts, &liab_by_ccy)
        .into_iter()
        .map(|s| {
            json!({
                "ccy": s.currency,
                "share_pct": f64_val(&s.share_of_liabilities) * 100.0,
                "significant": s.significant,
                "outflows": f64_val(&(&s.total_outflows / &million)),
                "inflows": f64_val(&(&s.total_inflows / &million)),
                "nco": f64_val(&(&s.net_cash_outflows / &million)),
            })
        })
        .collect();

    // APS 210 Stress Suite
    let suite = aps210_standard_suite();
    let stress_rows: Vec<Value> = stress_full(bank, rd, &suite)?
        .into_iter()
        .map(|sr| {
            json!({
                "name": sr.name,
                "kind": format!("{:?}", sr.kind),
                "horizon": sr.horizon_days,
                "lcr": sr.lcr.lcr.as_ref().map(f64_val),
                "lcr_surplus": f64_val(&sr.lcr.surplus()),
                "nsfr": sr.nsfr.nsfr.as_ref().map(|q| f64_val(q) * 100.0),
                "nsfr_surplus": f64_val(&sr.nsfr.surplus()),
            })
        })
        .collect();

    let r = crate::ratios(ret)?;
    let data = json!({
        "meta": {
            "adi": adi_name,
            "reporting_date": rd.to_string(),
            "total_records": total_records,
            "lcr": r.lcr.as_ref().map(f64_val),
            "hqla": f64_val(&r.hqla),
            "nco": f64_val(&r.nco),
            "nsfr": r.nsfr.as_ref().map(|q| f64_val(q) * 100.0),
            "asf": f64_val(&r.asf),
            "rsf": f64_val(&r.rsf),
            "trapped_hqla": f64_val(&(&trapped_total / &million)),
        },
        "lcr_items": lcr_items,
        "nsfr_items": nsfr_items,
        "contracts": Value::Object(contracts),
        "level2": level2_rows,
        "sig_ccy": sig_ccy,
        "stress": stress_rows,
    });

    Ok(TEMPLATE.replace("__DATA__", &data.to_string()))
}
