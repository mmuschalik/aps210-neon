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
