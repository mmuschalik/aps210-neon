//! Items 14-16 from raw facilities, commitments and contingent obligations.

use chrono::NaiveDate;
use obligations::*;
use prudence_core::uncertain::{Mode, Uncertain};
use prudence_core::{Missing, Q, evaluate};

fn rd() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
}
fn m(x: u64) -> Q {
    Q::from_integer((x * 1_000_000).into())
}

fn facility(id: &str, cp: Counterparty, kind: FacilityKind, committed: bool, undrawn_m: u64) -> Facility {
    Facility {
        id: id.into(),
        counterparty: cp,
        kind,
        committed,
        undrawn: m(undrawn_m),
        drawable_within_30_days: Uncertain::Known(true),
        mortgage_redraw: false,
        funds_vehicle: false,
    }
}

fn weighted_outflow(book: &Book, mode: Mode, extra: &[(&str, Q)]) -> (Q, prudence_core::Evaluation<'static>) {
    let r = run(book, rd(), mode).unwrap();
    let mut inputs = arf_inputs(&r);
    for (k, v) in extra {
        inputs.insert(k.to_string(), v.clone());
    }
    let ev = evaluate(arf210::lcr_all(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    let total = ev.q("14:weighted") + ev.q("15:weighted") + ev.q("16:weighted");
    (total, ev)
}

#[test]
fn facilities_land_in_their_rows_with_their_run_offs() {
    let mut redraw = facility("redraw", Counterparty::RetailCustomer, FacilityKind::CreditFacility, false, 100);
    redraw.mortgage_redraw = true; // reported as committed even if revocable
    let mut mmf = facility("mmf", Counterparty::NonFinancialCorporate, FacilityKind::CreditFacility, true, 10);
    mmf.funds_vehicle = true; // para 52: liquidity facility to an FI in its entirety
    let book = Book {
        facilities: vec![
            redraw,
            facility("corp-credit", Counterparty::NonFinancialCorporate, FacilityKind::CreditFacility, true, 200),
            facility("corp-liq", Counterparty::NonFinancialCorporate, FacilityKind::LiquidityFacility, true, 50),
            facility("bank", Counterparty::RegulatedBank, FacilityKind::LiquidityFacility, true, 40),
            mmf,
            facility("uncommitted", Counterparty::Sme, FacilityKind::CreditFacility, false, 60),
        ],
        ..Book::default()
    };
    let (total, ev) = weighted_outflow(&book, Mode::Conservative, &[]);
    let at = |i: &str| ev.q(i).clone();
    assert_eq!(at("14.1.1"), Q::from_integer(100.into()));
    assert_eq!(at("14.3.1"), Q::from_integer(200.into()));
    assert_eq!(at("14.3.2"), Q::from_integer(50.into()));
    assert_eq!(at("14.5.1"), Q::from_integer(40.into()));
    assert_eq!(at("14.6.2"), Q::from_integer(10.into()));
    assert_eq!(at("16.1"), Q::from_integer(60.into()));
    // 100 x 5% + 200 x 10% + 50 x 30% + 40 x 40% + 10 x 100% + 60 x 5% = 69
    assert_eq!(total, Q::from_integer(69.into()));
}

#[test]
fn unknown_drawability_is_conservative() {
    let mut f = facility("f", Counterparty::OtherFinancialInstitution, FacilityKind::LiquidityFacility, true, 30);
    f.drawable_within_30_days = Uncertain::unknown("facility terms not captured");
    let book = Book { facilities: vec![f.clone()], ..Book::default() };
    let (cons, _) = weighted_outflow(&book, Mode::Conservative, &[]);
    let (opt, _) = weighted_outflow(&book, Mode::Optimistic, &[]);
    assert_eq!(cons, Q::from_integer(30.into())); // drawable: 100% to another FI
    assert_eq!(opt, Q::default());
    let r = run(&book, rd(), Mode::Conservative).unwrap();
    assert!(r.assumptions.contains_key(&("f".to_string(), "drawable_within_30_days".to_string())));
    f.drawable_within_30_days = Uncertain::Known(false);
    let (none, _) = weighted_outflow(&Book { facilities: vec![f], ..Book::default() }, Mode::Conservative, &[]);
    assert_eq!(none, Q::default());
}

#[test]
fn lending_commitments_net_of_implicit_rollover_and_contingent_obligations() {
    let book = Book {
        commitments: vec![
            LendingCommitment { id: "fi".into(), counterparty: Counterparty::RegulatedBank, amount: m(20) },
            LendingCommitment { id: "retail".into(), counterparty: Counterparty::RetailCustomer, amount: m(30) },
        ],
        obligations: vec![
            ContingentObligation { id: "tf".into(), kind: ObligationKind::TradeFinance, amount: m(8) },
            ContingentObligation { id: "g".into(), kind: ObligationKind::Guarantee, amount: m(6) },
            ContingentObligation { id: "bb".into(), kind: ObligationKind::ShortTermDebtBuyback, amount: m(50) },
        ],
        ..Book::default()
    };
    // Retail inflows of 40 at 50%: implicit roll-over 20 offsets the retail commitment of 30.
    let (total, ev) = weighted_outflow(&book, Mode::Conservative, &[("20.1", Q::from_integer(40.into()))]);
    assert_eq!(ev.q("15.1"), &Q::from_integer(20.into()));
    assert_eq!(ev.q("15.8"), &Q::from_integer(10.into()));
    // 15: 20 x 100% + 10; 16: 8 x 100% + 6 x 50% + 50 x 10% (default buyback rate) = 16
    assert_eq!(ev.q("15:weighted"), &Q::from_integer(30.into()));
    assert_eq!(ev.q("16:weighted"), &Q::from_integer(16.into()));
    assert_eq!(total, Q::from_integer(46.into()));
}

/// Missing data never lowers the outflow (every unknown here is drawability,
/// whose conservative value is "drawable").
#[test]
fn missing_data_never_lowers_outflows() {
    let mut s: u64 = 7;
    let mut next = |k: u64| {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 33) % k
    };
    let cps = [
        Counterparty::RetailCustomer,
        Counterparty::Sme,
        Counterparty::NonFinancialCorporate,
        Counterparty::SovereignCentralBankPseMdb,
        Counterparty::RegulatedBank,
        Counterparty::OtherFinancialInstitution,
        Counterparty::OtherLegalEntity,
    ];
    for _ in 0..40 {
        let facilities = (0..1 + next(8))
            .map(|i| Facility {
                id: format!("f{i}"),
                counterparty: cps[next(7) as usize],
                kind: if next(2) == 0 { FacilityKind::CreditFacility } else { FacilityKind::LiquidityFacility },
                committed: next(3) > 0,
                undrawn: m(next(500)),
                drawable_within_30_days: [Uncertain::Known(true), Uncertain::Known(false), Uncertain::unknown("x")]
                    [next(3) as usize]
                    .clone(),
                mortgage_redraw: next(5) == 0,
                funds_vehicle: next(6) == 0,
            })
            .collect();
        let book = Book { facilities, ..Book::default() };
        let (cons, _) = weighted_outflow(&book, Mode::Conservative, &[]);
        let (opt, _) = weighted_outflow(&book, Mode::Optimistic, &[]);
        assert!(cons >= opt);
    }
}

#[test]
fn module_proof_obligations_hold() {
    for o in prudence_core::syntax::prove::prove_module(module()).unwrap() {
        assert!(o.proven, "{} NOT proven", o.obligation);
    }
}
