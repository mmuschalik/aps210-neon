//! Cross-form rules between ARF 210 returns of the same date and level.

use std::collections::BTreeMap;

use arf210::crossform::*;
use arf210::*;
use prudence_core::{Evaluation, Form, Missing, Q, evaluate};

fn q(n: i64) -> Q {
    Q::from_integer(n.into())
}

fn eval(f: &'static Form, inputs: &BTreeMap<String, Q>) -> Evaluation<'static> {
    evaluate(f, inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap()
}

fn ids(fs: &[prudence_core::Finding]) -> Vec<String> {
    fs.iter().map(|f| f.check_id.clone()).collect()
}

fn norm(t: &str) -> String {
    t.to_lowercase().replace(['–', '-'], " ").split_whitespace().map(|w| w.trim_end_matches('s')).collect::<Vec<_>>().join(" ")
}

/// The ARS item mapping joins items that mean the same thing: every mapped
/// pair has the same title on both forms (apart from 210.4's "Total assets"
/// / "Total liabilities and capital" rows, whose 210.3.x counterparts are the
/// same totals).
#[test]
fn the_funding_plan_mapping_joins_like_items() {
    let map = funding_plan_mapping();
    assert!(map.len() > 80, "{}", map.len());
    for (a, code, b) in &map {
        let ta = &funding_plan().items.iter().find(|i| &i.id == a).unwrap().label;
        let other = form(code).unwrap();
        let tb = &other.items.iter().find(|i| &i.id == b).unwrap().label;
        let (na, nb) = (norm(ta), norm(tb));
        // 210.4 memo rows add a qualifier ("Residential mortgages" vs "... - forecast").
        assert!(na == nb || nb.starts_with(&na) || na.starts_with(&nb), "210.4 {a} {ta:?} vs {code} {b} {tb:?}");
    }
}

#[test]
fn funding_plan_ties_to_the_maturity_mismatch_returns() {
    // 210.3.1: notes and coin 5 at call, AUD total assets via the buckets.
    let assets: BTreeMap<String, Q> =
        [("1.1:at_call", 5), ("2.1.1:y2", 40), ("9.1:at_call", 45)].iter().map(|(k, v)| (k.to_string(), q(*v))).collect();
    let liabilities: BTreeMap<String, Q> =
        [("1.1.1:at_call", 30), ("6.1:y10plus", 15), ("8.1:at_call", 45), ("9.1:m3", 2)].iter().map(|(k, v)| (k.to_string(), q(*v))).collect();
    let (a, l) = (eval(maturity_assets(), &assets), eval(maturity_liabilities(), &liabilities));
    let mut plan: BTreeMap<String, Q> = BTreeMap::new();
    for (k, v) in [("1.1", 5), ("2.1.1", 40), ("8.1", 45), ("9.1.1", 30), ("14.1", 15), ("19.1", 2)] {
        plan.insert(format!("{k}:actual"), q(v));
    }
    // Totals in 210.4 are reported (inputs), so they must be reported consistently.
    for (k, v) in [("1", 5), ("2", 40), ("2.1", 40), ("8", 45), ("9", 30), ("9.1", 30), ("14", 15), ("15", 45), ("15.1", 45)] {
        plan.insert(format!("{k}:actual"), q(v));
    }
    let e = eval(funding_plan(), &plan);
    let got = ids(&funding_plan_vs_maturity(&e, &a, &l));
    assert!(got.is_empty(), "{got:?}");
    // A forecast plan whose actual balance disagrees with the maturity return.
    plan.insert("2.1.1:actual".into(), q(41));
    let e = eval(funding_plan(), &plan);
    assert_eq!(ids(&funding_plan_vs_maturity(&e, &a, &l)), ["210.4:2.1.1=210.3.1:2.1.1"]);
}

#[test]
fn aud_return_ties_to_the_currency_split() {
    let mut all: BTreeMap<String, Q> = BTreeMap::new();
    all.insert("10.1".into(), q(100)); // all-currency outflows
    all.insert("18.1".into(), q(60));
    all.insert("18.1:weighted".into(), q(60));
    let mut aud: BTreeMap<String, Q> = BTreeMap::new();
    aud.insert("10.1".into(), q(60));
    let (ea, eb) = (eval(lcr_all(), &all), eval(lcr_aud(), &aud));
    assert!(aud_vs_all(&eb, &ea).is_empty(), "{:?}", aud_vs_all(&eb, &ea));
    aud.insert("10.1".into(), q(70));
    let eb = eval(lcr_aud(), &aud);
    assert_eq!(ids(&aud_vs_all(&eb, &ea)), ["210.1A:18.1:amount=210.1B:18:amount", "210.1A:18.1:weighted=210.1B:18:weighted"]);
}

#[test]
fn daily_report_ties_to_the_lcr_and_mlh_returns() {
    let mut lcr: BTreeMap<String, Q> = BTreeMap::new();
    lcr.insert("1.1".into(), q(120)); // notes and coin (HQLA1)
    lcr.insert("10.1".into(), q(100)); // outflows at 100%
    let e = eval(lcr_all(), &lcr);
    let hqla = e.q("25:value").clone();
    let ratio = e.q("30:value").clone();
    let mut daily: BTreeMap<String, Q> = BTreeMap::new();
    daily.insert("1.1:balance".into(), hqla.clone());
    daily.insert("5.2:day1".into(), ratio.clone());
    let d = eval(arf210::daily(), &daily);
    assert!(daily_vs_lcr(&d, &e, None).is_empty(), "{:?}", daily_vs_lcr(&d, &e, None));
    // Reported to one decimal place: 120.04 is the same LCR, 120.2 is not.
    daily.insert("5.2:day1".into(), Q::new(12004.into(), 100.into()));
    assert!(daily_vs_lcr(&eval(arf210::daily(), &daily), &e, None).is_empty());
    daily.insert("5.2:day1".into(), Q::new(1202.into(), 10.into()));
    assert_eq!(ids(&daily_vs_lcr(&eval(arf210::daily(), &daily), &e, None)), ["210.5:5.2=210.1A:30"]);

    // MLH ADI: HQLA are the listed ARF 210.2 items.
    let mut mlh_in: BTreeMap<String, Q> = BTreeMap::new();
    for (k, v) in [("1.1", 3), ("1.3.1", 7), ("2.1", 50), ("1.2", 1), ("7", 9)] {
        mlh_in.insert(k.into(), q(v));
    }
    let m = eval(mlh(), &mlh_in);
    let mut daily: BTreeMap<String, Q> = BTreeMap::new();
    daily.insert("1.1:balance".into(), q(60));
    daily.insert("1.5:balance".into(), q(10));
    assert!(daily_vs_mlh(&eval(arf210::daily(), &daily), &m).is_empty());
    daily.insert("1.5:balance".into(), q(11));
    assert_eq!(ids(&daily_vs_mlh(&eval(arf210::daily(), &daily), &m)), ["210.5:1.5=210.2:1.2+1.3.2-1.3.5+6+7"]);
}
