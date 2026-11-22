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
    // NSFR: the operational 600k in item 8.2 (item 8 = 8.1 + 8.2, at 50%), the excess as corporate funding (item 9).
    let ops: Q = s.iter().filter(|x| x.nsfr == ("8.2".to_string(), "amount".to_string())).map(|x| x.amount.clone()).sum();
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

#[test]
fn nsfr_gt_30d_maturity_and_counterparty_routing() {
    let cust = [
        Customer { id: "corp".into(), kind: Kind::NonFinancialCorporate },
        Customer { id: "sov".into(), kind: Kind::SovereignCentralBankPseMdb },
        Customer { id: "fi".into(), kind: Kind::OtherFinancialInstitution },
    ];
    let mut corp_9m = dep("corp_9m", "corp", 100);
    corp_9m.within_30_days = false;
    corp_9m.maturity = Some(NaiveDate::from_ymd_opt(2027, 6, 30).unwrap()); // 9m -> b1 (50% ASF)

    let mut sov_18m = dep("sov_18m", "sov", 200);
    sov_18m.within_30_days = false;
    sov_18m.maturity = Some(NaiveDate::from_ymd_opt(2028, 3, 31).unwrap()); // 18m -> b2 (100% ASF)

    let mut fi_9m = dep("fi_9m", "fi", 300);
    fi_9m.within_30_days = false;
    fi_9m.maturity = Some(NaiveDate::from_ymd_opt(2027, 6, 30).unwrap()); // 9m -> b1 (50% ASF)

    let mut fi_2m = dep("fi_2m", "fi", 400);
    fi_2m.within_30_days = false;
    fi_2m.maturity = Some(NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()); // 2m -> b0 (0% ASF)

    let mut corp_called = dep("corp_called", "corp", 50);
    corp_called.called = true;
    corp_called.maturity = Some(NaiveDate::from_ymd_opt(2028, 3, 31).unwrap()); // called -> b0 (50% ASF)

    let s = run(&cust, &[corp_9m, sov_18m, fi_9m, fi_2m, corp_called], rd(), Mode::Conservative).unwrap();
    let nsfr_of = |acct: &str| -> Vec<(String, String)> {
        s.iter().filter(|x| x.account == acct).map(|x| x.nsfr.clone()).collect()
    };
    assert_eq!(nsfr_of("corp_9m"), vec![("9".into(), "b1".into())]);
    assert_eq!(nsfr_of("sov_18m"), vec![("10".into(), "b2".into())]);
    assert_eq!(nsfr_of("fi_9m"), vec![("12".into(), "b1".into())]);
    assert_eq!(nsfr_of("fi_2m"), vec![("12".into(), "b0".into())]);
    assert_eq!(nsfr_of("corp_called"), vec![("9".into(), "b0".into())]);

    // Evaluate on ARF 210.6:
    // corp_9m: 100k * 50% = 50k
    // sov_18m: 200k * 100% = 200k
    // fi_9m:   300k * 50% = 150k
    // fi_2m:   400k * 0% = 0
    // corp_called: 50k * 50% = 25k
    // Total ASF = 425k
    let mut inputs: std::collections::BTreeMap<String, Q> = std::collections::BTreeMap::new();
    let million = Q::from_integer(1_000_000.into());
    for x in &s {
        let key = format!("{}:{}", x.nsfr.0, x.nsfr.1);
        *inputs.entry(key).or_default() += &x.amount / &million;
    }
    let ev = evaluate(arf210::nsfr(), inputs.iter().map(|(a, b)| (a.as_str(), b.clone())), Missing::Zero).unwrap();
    assert_eq!(ev.q("19:calc") * &million, k(425));
}
