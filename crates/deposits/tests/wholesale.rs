//! Wholesale deposits: operational / excess split, insurance, counterparties.

use chrono::NaiveDate;
use deposits::wholesale::*;
use prudence_core::uncertain::{Mode, Uncertain};
use prudence_core::{Missing, Q, evaluate};

fn rd() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
}
fn k(x: u64) -> Q {
    Q::from_integer((x * 1000).into())
}
fn dep(id: &str, cust: &str, bal_k: u64) -> Deposit {
    Deposit {
        id: id.into(),
        customer_id: cust.into(),
        balance: k(bal_k),
        within_30_days: true,
        called: false,
        fcs_eligible: Uncertain::Known(true),
        operational_service: Uncertain::Known(false),
        operational_balance: Uncertain::unknown("not assessed"),
        maturity: None,
    }
}
fn items(s: &[WSlice], acct: &str) -> Vec<(Option<String>, Q)> {
    s.iter().filter(|x| x.account == acct).map(|x| (x.lcr_item.clone(), x.amount.clone())).collect()
}
fn some(i: &str) -> Option<String> {
    Some(i.to_string())
}

#[test]
fn operational_deposit_splits_three_ways() {
    // A corporate's cash-management account: 1,000k, of which 600k operational.
    // FCS covers 250k of the operational part (5%), the rest of it runs off at
    // 25%, and the 400k excess is non-operational (not entirely insured: 40%).
    let mut d = dep("op", "corp", 1000);
    d.operational_service = Uncertain::Known(true);
    d.operational_balance = Uncertain::Known(k(600));
    let cust = [Customer { id: "corp".into(), kind: Kind::NonFinancialCorporate }];
    let s = run(&cust, &[d.clone()], rd(), Mode::Conservative).unwrap();
    let mut got = items(&s, "op");
    got.sort();
    assert_eq!(got, vec![(some("9.2.2"), k(250)), (some("9.2.4"), k(350)), (some("9.2.6"), k(400))]);
    // NSFR: the operational 600k at 50% (item 8), the excess as corporate funding (item 9).
    let ops: Q = s.iter().filter(|x| x.nsfr == ("8".to_string(), "b0".to_string())).map(|x| x.amount.clone()).sum();
    assert_eq!(ops, k(600));

    // Operational amount unknown: the whole deposit is excess (APG 210 para 135).
    d.operational_balance = Uncertain::unknown("no analysis");
    let s = run(&cust, &[d.clone()], rd(), Mode::Conservative).unwrap();
    assert_eq!(items(&s, "op"), vec![(some("9.2.6"), k(1000))]);
    // Optimistically the whole balance would be operational.
    let s = run(&cust, &[d], rd(), Mode::Optimistic).unwrap();
    let mut got = items(&s, "op");
    got.sort();
    assert_eq!(got, vec![(some("9.2.2"), k(250)), (some("9.2.4"), k(750))]);
}

#[test]
fn counterparties_and_horizons() {
    let cust = [
        Customer { id: "gov".into(), kind: Kind::SovereignCentralBankPseMdb },
        Customer { id: "bank".into(), kind: Kind::Bank },
        Customer { id: "fund".into(), kind: Kind::OtherFinancialInstitution },
    ];
    let mut called = dep("called", "gov", 50);
    called.called = true;
    let mut term = dep("term", "fund", 70);
    term.within_30_days = false;
    let s = run(&cust, &[dep("small", "gov", 100), dep("interbank", "bank", 300), called, term], rd(), Mode::Conservative).unwrap();
    // The sovereign's deposits total 150k <= 250k and are all eligible: entirely insured, 20%.
    assert_eq!(items(&s, "small"), vec![(some("9.3.4"), k(100))]);
    assert_eq!(items(&s, "called"), vec![(some("9.3.6"), k(50))]);
    assert_eq!(items(&s, "interbank"), vec![(some("9.4.5"), k(300))]);
    assert_eq!(items(&s, "term"), vec![(None, k(70))]);
    // Into the form: 100 x 20% + 50 x 100% + 300 x 100% = 370k.
    let mut inputs: std::collections::BTreeMap<String, Q> = std::collections::BTreeMap::new();
    let million = Q::from_integer(1_000_000.into());
    for x in &s {
        if let Some(i) = &x.lcr_item {
            *inputs.entry(i.clone()).or_default() += &x.amount / &million;
        }
    }
    let ev = evaluate(arf210::lcr_all(), inputs.iter().map(|(a, b)| (a.as_str(), b.clone())), Missing::Zero).unwrap();
    assert_eq!(ev.q("9:weighted") * &million, k(370));
}

#[test]
fn module_proof_obligations_hold() {
    for o in prudence_core::syntax::prove::prove_module(module()).unwrap() {
        assert!(o.proven, "{} NOT proven:\n{}", o.obligation, o.counterexample);
    }
}
