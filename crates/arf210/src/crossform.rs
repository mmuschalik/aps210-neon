//! Consistency rules spanning more than one ARF 210 form, for the same
//! reporting date and consolidation level.

use num_traits::Zero;
use prudence_core::num::display;
use prudence_core::{Evaluation, Finding, Q, Ref, Value};

const BUCKETS: [&str; 3] = ["b0", "b1", "b2"];

/// Sum of an NSFR row's maturity-bucket amounts, or of all its leaf sub-rows
/// (so a header plus its children is never double counted).
pub fn bucket_total(ev: &Evaluation, item: &str) -> Q {
    let form = ev.form;
    let rows: Vec<&str> = form
        .items
        .iter()
        .filter(|it| it.id == item || it.id.starts_with(&format!("{item}.")))
        .filter(|it| BUCKETS.iter().any(|b| it.cell(b).is_some()))
        .map(|it| it.id.as_str())
        .collect();
    let leaves = rows.iter().filter(|i| !rows.iter().any(|j| j != *i && j.starts_with(&format!("{i}."))));
    let mut total = Q::zero();
    for id in leaves {
        for b in BUCKETS {
            if let Some(Value::Num(q)) = ev.values.get(&Ref::new(*id, b)) {
                total += q;
            }
        }
    }
    total
}

/// ARF 210.6 items 26-28 are capped by what ARF 210.1A admits to the LCR
/// numerator (ARS 210.0 pp. 152-153; APS 210 Att C para 32).
pub fn nsfr_vs_lcr(nsfr: &Evaluation, lcr_all: &Evaluation) -> Vec<Finding> {
    assert_eq!(nsfr.form.code, "ARF 210.6");
    assert_eq!(lcr_all.form.code, "ARF 210.1A");
    let mut out = Vec::new();
    let mut rule = |id: &str, desc: &str, lhs: Q, rhs: &Q| {
        if &lhs > rhs {
            out.push(Finding {
                warn: false,
                check_id: id.into(),
                description: desc.into(),
                detail: format!("{} > {}", display(&lhs), display(rhs)),
            });
        }
    };
    rule(
        "210.6:26.1+27.1<=210.1A:6.3",
        "Unencumbered CLF assets at 10% RSF exceed the CLF amount included in the LCR",
        bucket_total(nsfr, "26.1") + bucket_total(nsfr, "27.1"),
        lcr_all.q("6.3"),
    );
    rule(
        "210.6:26+27<=210.1A:6.2.1",
        "CLF assets (incl. encumbered) at 10% RSF exceed the approved CLF size",
        bucket_total(nsfr, "26") + bucket_total(nsfr, "27"),
        lcr_all.q("6.2.1"),
    );
    // Necessary condition only: item 6.5 may include more than third-party debt.
    rule(
        "210.6:28.1<=210.1A:6.5",
        "Other jurisdictions' ALA at 10% RSF exceed ALA reported in the LCR",
        bucket_total(nsfr, "28.1"),
        lcr_all.q("6.5"),
    );
    out
}

// ---------------------------------------------------------------------------
// Rules between returns for the same reporting date and consolidation level.
// Each states an ARS 210.0 cross-reference; amounts are compared exactly, and
// ARF 210.5 figures (reported in millions or percent to one decimal place,
// ARS p.127) at that precision.

fn get(ev: &Evaluation, key: &str) -> Option<Q> {
    ev.get(key).num().cloned()
}

fn finding(id: &str, desc: &str, detail: String) -> Finding {
    Finding { warn: false, check_id: id.into(), description: desc.into(), detail }
}

/// `a == b` exactly; skipped when either side is undefined.
fn tie(out: &mut Vec<Finding>, id: &str, desc: &str, a: Option<Q>, b: Option<Q>) {
    if let (Some(a), Some(b)) = (a, b)
        && a != b
    {
        out.push(finding(id, desc, format!("{} vs {}", display(&a), display(&b))));
    }
}

/// Rounded to one decimal place (half away from zero), as ARF 210.5 reports.
fn one_dp(q: &Q) -> Q {
    let ten = Q::from_integer(10.into());
    let x = q * &ten;
    let r = if x >= Q::zero() { (x + Q::new(1.into(), 2.into())).floor() } else { (x - Q::new(1.into(), 2.into())).ceil() };
    r / ten
}

fn tie_1dp(out: &mut Vec<Finding>, id: &str, desc: &str, daily: Option<Q>, other: Option<Q>) {
    if let (Some(a), Some(b)) = (daily, other)
        && one_dp(&a) != one_dp(&b)
    {
        out.push(finding(id, desc, format!("{} vs {} (to one decimal place)", display(&a), display(&b))));
    }
}

/// ARF 210.1B (AUD only) against ARF 210.1A (all currencies). The AUD rows of
/// the currency split of total outflows and inflows (210.1A items 18.1, 23.1)
/// and 210.1B's totals (items 18, 23) report the same AUD cash flows, with the
/// same treatment of FX derivatives (ARS pp. 51, 55-56, 61-62).
pub fn aud_vs_all(lcr_aud: &Evaluation, lcr_all: &Evaluation) -> Vec<Finding> {
    assert_eq!(lcr_aud.form.code, "ARF 210.1B");
    assert_eq!(lcr_all.form.code, "ARF 210.1A");
    let mut out = Vec::new();
    for (memo, total, what) in [("18.1", "18", "outflows"), ("23.1", "23", "inflows")] {
        for col in ["amount", "weighted"] {
            tie(
                &mut out,
                &format!("210.1A:{memo}:{col}=210.1B:{total}:{col}"),
                &format!("AUD cash {what} differ between ARF 210.1A (currency split) and ARF 210.1B"),
                get(lcr_all, &format!("{memo}:{col}")),
                get(lcr_aud, &format!("{total}:{col}")),
            );
        }
    }
    out
}

/// ARF 210.5 (daily) against the LCR returns of the same date (LCR ADIs):
/// HQLA and RBNZ securities / ALA "as reported in ARF 210.1A" (ARS p.128-129),
/// and the day-1 Level 1 LCRs (item 5, column 1).
pub fn daily_vs_lcr(daily: &Evaluation, lcr_all: &Evaluation, lcr_aud: Option<&Evaluation>) -> Vec<Finding> {
    assert_eq!(daily.form.code, "ARF 210.5");
    assert_eq!(lcr_all.form.code, "ARF 210.1A");
    let mut out = Vec::new();
    tie_1dp(&mut out, "210.5:1.1=210.1A:25", "HQLA in ARF 210.5 differ from ARF 210.1A", get(daily, "1.1:balance"), get(lcr_all, "25:value"));
    tie_1dp(
        &mut out,
        "210.5:1.4=210.1A:26",
        "RBNZ eligible securities and ALA in ARF 210.5 differ from ARF 210.1A",
        get(daily, "1.4:balance"),
        get(lcr_all, "26:value"),
    );
    tie_1dp(&mut out, "210.5:5.2=210.1A:30", "All-currency LCR in ARF 210.5 differs from ARF 210.1A", get(daily, "5.2:day1"), get(lcr_all, "30:value"));
    if let Some(aud) = lcr_aud {
        tie_1dp(&mut out, "210.5:5.1=210.1B:30", "AUD LCR in ARF 210.5 differs from ARF 210.1B", get(daily, "5.1:day1"), get(aud, "30:value"));
    }
    out
}

/// ARF 210.5 against ARF 210.2 (MLH ADIs): items 1.1 and 1.5 are the listed
/// ARF 210.2 items (ARS p.129).
pub fn daily_vs_mlh(daily: &Evaluation, mlh: &Evaluation) -> Vec<Finding> {
    assert_eq!(daily.form.code, "ARF 210.5");
    assert_eq!(mlh.form.code, "ARF 210.2");
    let sum = |items: &[&str]| -> Option<Q> { items.iter().map(|i| get(mlh, &format!("{i}:amount"))).sum() };
    let mut out = Vec::new();
    tie_1dp(
        &mut out,
        "210.5:1.1=210.2:1.1+1.3.1+2.1+2.2+3",
        "HQLA in ARF 210.5 differ from the MLH assets that qualify as HQLA in ARF 210.2",
        get(daily, "1.1:balance"),
        sum(&["1.1", "1.3.1", "2.1", "2.2", "3"]),
    );
    tie_1dp(
        &mut out,
        "210.5:1.5=210.2:1.2+1.3.2-1.3.5+6+7",
        "Other liquids in ARF 210.5 differ from the other MLH assets in ARF 210.2",
        get(daily, "1.5:balance"),
        sum(&["1.2", "1.3.2", "1.3.3", "1.3.4", "1.3.5", "6", "7"]),
    );
    out
}

/// ARF 210.4 (actual balances, column 1) against ARF 210.3.1 and ARF 210.3.2
/// (closing balances, column 1), by the ARS mapping (ARS p.120-121): assets
/// 1-7 -> 210.3.1 items 1-7, 8 -> 9; liabilities and capital 9-14 -> 210.3.2
/// items 1-6, 15 -> 8; memo 18.1-18.6 -> 210.3.1 items 10.1-10.6, 19.1-19.2 ->
/// 210.3.2 items 9.1-9.2. Sub-items follow their parents where both forms
/// have them.
/// The ARS mapping from ARF 210.4 items to ARF 210.3.1 / 210.3.2 items:
/// (210.4 item, target form code, target item).
pub fn funding_plan_mapping() -> Vec<(String, &'static str, String)> {
    let plan = crate::funding_plan();
    let mut out = Vec::new();
    let mut map = |from: &str, to: &str, other: &'static prudence_core::Form, subitems: bool| {
        out.push((from.to_string(), other.code.as_str(), to.to_string()));
        if subitems {
            for it in &plan.items {
                if let Some(rest) = it.id.strip_prefix(&format!("{from}.")) {
                    let target = format!("{to}.{rest}");
                    if other.items.iter().any(|o| o.id == target) {
                        out.push((it.id.clone(), other.code.as_str(), target));
                    }
                }
            }
        }
    };
    let (assets, liabilities) = (crate::maturity_assets(), crate::maturity_liabilities());
    for n in 1..=7 {
        map(&n.to_string(), &n.to_string(), assets, true);
    }
    map("8", "9", assets, true);
    for n in 9..=14 {
        map(&n.to_string(), &(n - 8).to_string(), liabilities, true);
    }
    map("15", "8", liabilities, true);
    for n in 1..=6 {
        map(&format!("18.{n}"), &format!("10.{n}"), assets, false);
    }
    for n in 1..=2 {
        map(&format!("19.{n}"), &format!("9.{n}"), liabilities, false);
    }
    out
}

/// ARF 210.4 (actual balances, column 1) against ARF 210.3.1 and ARF 210.3.2
/// (closing balances, column 1), by the ARS mapping (ARS pp. 120-121): assets
/// 1-7 -> 210.3.1 items 1-7, 8 -> 9; liabilities and capital 9-14 -> 210.3.2
/// items 1-6, 15 -> 8; memo 18.1-18.6 -> 210.3.1 items 10.1-10.6, 19.1-19.2 ->
/// 210.3.2 items 9.1-9.2. Sub-items follow their parents where both forms
/// have them.
pub fn funding_plan_vs_maturity(plan: &Evaluation, assets: &Evaluation, liabilities: &Evaluation) -> Vec<Finding> {
    assert_eq!(plan.form.code, "ARF 210.4");
    assert_eq!(assets.form.code, "ARF 210.3.1");
    assert_eq!(liabilities.form.code, "ARF 210.3.2");
    let mut out = Vec::new();
    for (a, code, b) in funding_plan_mapping() {
        let other = if code == "ARF 210.3.1" { assets } else { liabilities };
        let short = code.trim_start_matches("ARF ");
        tie(
            &mut out,
            &format!("210.4:{a}={short}:{b}"),
            &format!("ARF 210.4 actual balance of item {a} differs from {code} item {b}"),
            get(plan, &format!("{a}:actual")),
            get(other, &format!("{b}:balance")),
        );
    }
    out
}
