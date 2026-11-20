//! Derivatives and collateral: netting, options, the 24-month lookback
//! (bitemporal), collateral items, NSFR replacement cost.

use chrono::{Days, Months, NaiveDate};
use derivatives::*;
use prudence_core::uncertain::{Mode, Uncertain};
use prudence_core::{Missing, Q, evaluate};

fn rd() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
}
fn m(x: i64) -> Q {
    Q::from_integer((x * 1_000_000).into())
}
fn set(id: &str, mna: Option<bool>) -> NettingSet {
    NettingSet { id: id.into(), counterparty: id.into(), netting_agreement: mna, ..NettingSet::default() }
}
fn flow(s: &str, days: u64, amount_m: i64) -> Flow {
    Flow {
        set: s.into(),
        date: rd() + Days::new(days),
        amount: m(amount_m),
        currency: "AUD".into(),
        fx_principal: false,
        fx_trading: false,
        option: false,
        in_the_money: Uncertain::Known(true),
    }
}
fn go(b: &Book, mode: Mode) -> Run {
    run(b, rd(), rd(), mode).unwrap()
}

#[test]
fn netting_only_under_an_mna_or_same_day_fx() {
    let mut fx_in = flow("b", 3, 500);
    fx_in.fx_principal = true;
    let mut fx_out = flow("b", 3, -480);
    fx_out.fx_principal = true;
    let book = Book {
        sets: vec![set("a", Some(true)), set("b", Some(false))],
        flows: vec![
            flow("a", 5, 100),
            flow("a", 10, -150),
            flow("a", 40, -20), // beyond 30 days
            flow("b", 5, 100),
            flow("b", 10, -150),
            fx_in,
            fx_out,
        ],
        ..Book::default()
    };
    let r = go(&book, Mode::Conservative);
    // a nets to -50; b is gross (+100, -150) plus FX netted to +20.
    assert_eq!(r.lcr["13.1"], m(50 + 150));
    assert_eq!(r.lcr["21.1"], m(100 + 20));
}

#[test]
fn unknowns_resolve_conservatively() {
    let mut pay = flow("a", 5, -30);
    pay.option = true;
    pay.in_the_money = Uncertain::unknown("no valuation");
    let mut rec = flow("a", 6, 40);
    rec.option = true;
    rec.in_the_money = Uncertain::unknown("no valuation");
    let book = Book { sets: vec![set("a", None)], flows: vec![pay, rec, flow("a", 7, 25), flow("a", 8, -25)], ..Book::default() };
    // Conservative: the payable option is exercised, the receivable not; the
    // unrecorded MNA means gross: out 30 + 25, in 25.
    let r = go(&book, Mode::Conservative);
    assert_eq!((r.lcr["13.1"].clone(), r.lcr["21.1"].clone()), (m(55), m(25)));
    assert!(r.assumptions.contains_key(&("a".to_string(), "netting_agreement".to_string())));
    // Optimistic: netted, receivable option exercised: +40 + 25 - 25 = +40.
    let r = go(&book, Mode::Optimistic);
    assert_eq!((r.lcr["13.1"].clone(), r.lcr["21.1"].clone()), (m(0), m(40)));
}

fn cf(s: &str, days_before: u64, amount_m: i64) -> CollateralFlow {
    let d = rd() - Days::new(days_before);
    CollateralFlow { set: s.into(), amount: m(amount_m), currency: "AUD".into(), valid: d, recorded: d }
}

#[test]
fn lookback_is_the_largest_30_day_net_flow() {
    let book = Book {
        sets: vec![set("a", Some(true)), set("b", Some(false))],
        collateral_flows: vec![
            // a, netted: -40, +10, -30 within 26 days: |-60|.
            cf("a", 400, -40),
            cf("a", 390, 10),
            cf("a", 375, -30),
            // b, gross: +20 and -20 within 30 days count 40, not 0.
            cf("b", 200, 20),
            cf("b", 195, -20),
            // 25 months ago: outside the lookback.
            cf("a", 760, -500),
        ],
        ..Book::default()
    };
    let r = go(&book, Mode::Conservative);
    assert_eq!(r.lcr["13.6"], m(60));
    // Bitemporal: a large flow booked after the reporting date's knowledge
    // cut-off is not in the return as first lodged, and is in a restatement.
    let mut late = book.clone();
    let mut c = cf("b", 100, -90);
    c.recorded = rd() + Days::new(14);
    late.collateral_flows.push(c);
    assert_eq!(run(&late, rd(), rd(), Mode::Conservative).unwrap().lcr["13.6"], m(60));
    assert_eq!(run(&late, rd(), rd() + Days::new(20), Mode::Conservative).unwrap().lcr["13.6"], m(90));
}

/// The window ends the spec examines (flow dates and flow dates + 30 days)
/// give the same maximum as trying every day in the 24 months.
#[test]
fn window_ends_cover_every_window() {
    let mut s: u64 = 5;
    let mut next = |k: u64| {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 33) % k
    };
    let start = rd() - Months::new(24);
    for _ in 0..25 {
        let sets = vec![set("a", Some(true)), set("b", Some(false))];
        let flows: Vec<CollateralFlow> = (0..next(12))
            .map(|_| cf(["a", "b"][next(2) as usize], next(800), next(200) as i64 - 100))
            .collect();
        let book = Book { sets, collateral_flows: flows.clone(), ..Book::default() };
        let got = go(&book, Mode::Conservative).lcr["13.6"].clone();
        let mut best = Q::default();
        let mut end = start + Days::new(30);
        while end <= rd() {
            let inw = |c: &&CollateralFlow| end - Days::new(30) < c.valid && c.valid <= end;
            let a: Q = flows.iter().filter(|c| c.set == "a").filter(inw).map(|c| c.amount.clone()).sum();
            let b: Q = flows.iter().filter(|c| c.set == "b").filter(inw).map(|c| if c.amount < Q::default() { -c.amount.clone() } else { c.amount.clone() }).sum();
            let w = (if a < Q::default() { -a } else { a }) + b;
            if w > best {
                best = w;
            }
            end = end + Days::new(1);
        }
        assert_eq!(got, best);
    }
}

#[test]
fn collateral_items_and_the_form() {
    let mut a = set("a", Some(true));
    a.collateral_received = m(100);
    a.required_from_counterparty = m(70);
    a.hqla_substitutable = m(40);
    a.due_not_called = m(5);
    a.downgrade_3_notches = m(12);
    a.posted_non_hqla1 = m(50);
    a.received_reusable = m(30);
    let mut b = a.clone();
    b.id = "b".into();
    b.received_segregated = true; // segregated: not callable, not substitutable
    let book = Book { sets: vec![a, b], flows: vec![flow("a", 5, -8)], ..Book::default() };
    let r = go(&book, Mode::Conservative);
    assert_eq!(r.lcr["13.3"], m(30));
    assert_eq!(r.lcr["13.5"], m(40));
    assert_eq!(r.lcr["13.4"], m(10));
    assert_eq!(r.lcr["13.2"], m(24));
    assert_eq!(r.lcr["13.7"], m(40)); // (50 - 30) per set
    let inputs = arf_inputs(&r.lcr);
    let ev = evaluate(arf210::lcr_all(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    // 8 + 24 + 30 + 10 + 40 + 0 + 40 x 20%
    assert_eq!(ev.q("13:weighted"), &Q::from_integer(120.into()));
}

#[test]
fn nsfr_replacement_cost() {
    let mut a = set("a", Some(true));
    a.variation_margin_posted = m(10);
    a.initial_margin_posted = m(7);
    let book = Book {
        sets: vec![a, set("b", Some(false))],
        trades: vec![
            Trade { set: "a".into(), mtm: m(50) },
            Trade { set: "a".into(), mtm: m(-80) },
            Trade { set: "b".into(), mtm: m(50) },
            Trade { set: "b".into(), mtm: m(-80) },
        ],
        ..Book::default()
    };
    let r = go(&book, Mode::Conservative);
    assert_eq!(r.nsfr["13.1.1"], m(30 + 80));
    assert_eq!(r.nsfr["40.1.1"], m(50));
    let inputs = arf_inputs(&r.nsfr);
    let ev = evaluate(arf210::nsfr(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    // Liabilities 110 - VM 10 = 100 vs assets 50: a net liability of 50, no RSF for assets.
    assert_eq!(ev.q("13:amount"), &Q::from_integer(50.into()));
    assert_eq!(ev.q("40:amount"), &Q::default());
    assert_eq!(ev.q("41:amount"), &Q::from_integer(22.into())); // 20% of 110
    assert_eq!(ev.q("42:calc"), &(Q::from_integer(7.into()) * Q::new(85.into(), 100.into())));
}

#[test]
fn module_proof_obligations_hold() {
    for o in prudence_core::syntax::prove::prove_module(module()).unwrap() {
        assert!(o.proven, "{} NOT proven", o.obligation);
    }
}

/// The `netting_never_adds` invariant is evaluated on every run: a mutated
/// (false) version of it is reported as violated.
#[test]
fn the_netting_invariant_is_checked() {
    use prudence_core::interp::{Interp, Options};
    let src = SOURCE.replace("net_out(s) <= sum(", "net_out(s) > sum(");
    let m = prudence_core::syntax::parse_module(&src, &[]).unwrap();
    let book = Book { sets: vec![set("a", Some(true))], ..Book::default() };
    let w = load(&book).unwrap();
    let params = vec![("reporting_date".into(), date(rd())), ("known_at".into(), date(rd()))];
    let it = Interp::new(&m, &w, Options { mode: Mode::Conservative, sense: None, params }).unwrap();
    assert_eq!(it.check_invariants(), Err("invariant `netting_never_adds` violated".to_string()));
}

/// For the currency split each currency reports its own largest window; the
/// total is the portfolio's largest window, not their sum.
#[test]
fn lookback_by_currency() {
    let mut usd = cf("a", 100, -50);
    usd.currency = "USD".into();
    let book = Book {
        sets: vec![set("a", Some(true))],
        collateral_flows: vec![cf("a", 400, -30), usd],
        ..Book::default()
    };
    let r = go(&book, Mode::Conservative);
    assert_eq!(r.lcr["13.6"], m(50));
    let mut by: Vec<(String, Q)> =
        r.by_currency.iter().filter(|x| x.1 == "13.6").map(|x| (x.2.clone(), x.3.clone())).collect();
    by.sort();
    assert_eq!(by, vec![("AUD".into(), m(30)), ("USD".into(), m(50))]);
}
