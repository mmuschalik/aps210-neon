//! A whole ADI: every module's contributions assembled into ARF 210.1A,
//! with the currency split of total outflows and inflows.

use std::collections::BTreeMap;

use assembly::*;
use chrono::{Days, NaiveDate};
use prudence_core::uncertain::{Mode, Uncertain};
use prudence_core::{Missing, Q, evaluate};

fn rd() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
}
fn m(x: i64) -> Q {
    Q::from_integer((x * 1_000_000).into())
}

fn bank(with_fx_swap: bool) -> Bank {
    let mut b = Bank { retail: deposits::testgen::Gen::new(7).book(rd()), ..Bank::default() };
    // Some retail accounts are in NZD.
    for a in b.retail.accounts.iter().step_by(3) {
        b.currency.insert(a.id.clone(), "NZD".into());
    }
    b.wholesale_customers.push(deposits::wholesale::Customer { id: "corp".into(), kind: deposits::wholesale::Kind::NonFinancialCorporate });
    b.wholesale_deposits.push(deposits::wholesale::Deposit {
        id: "corp-usd".into(),
        customer_id: "corp".into(),
        balance: m(40),
        within_30_days: true,
        called: false,
        fcs_eligible: Uncertain::Known(false),
        operational_service: Uncertain::Known(false),
        operational_balance: Uncertain::unknown("n/a"),
    });
    b.currency.insert("corp-usd".into(), "USD".into());
    b.holdings.push(secured::Holding {
        id: "acgb".into(),
        kind: secured::AssetKind::DebtSecurity,
        issuer: secured::Issuer::AustralianGovernment,
        market_value: m(500),
        risk_weight: 0,
        rating_grade: 1,
        domestic_currency: true,
        index_constituent: true,
        own_or_associated: false,
        encumbered: false,
        operational: Uncertain::Known(true),
        drawable_in_stress: Uncertain::Known(true),
        clf_category: secured::ClfCategory::NotClfEligible,
        value_after_rba_margin: m(500),
        maturity: None,
    });
    b.repos.push(secured::Repo {
        id: "repo".into(),
        leg: secured::Leg::Funding,
        cash: m(30),
        collateral_class: secured::HqlaClass::Hqla2a,
        collateral_value: m(35),
        counterparty: secured::Counterparty::Other,
        operational: Uncertain::Known(true),
        rehypothecated: false,
        maturity: rd() + Days::new(7),
    });
    b.commitments.facilities.push(obligations::Facility {
        id: "rcf".into(),
        counterparty: obligations::Counterparty::NonFinancialCorporate,
        kind: obligations::FacilityKind::CreditFacility,
        committed: true,
        undrawn: m(80),
        drawable_within_30_days: Uncertain::Known(true),
        mortgage_redraw: false,
        funds_vehicle: false,
    });
    b.loans.push(loans::Loan {
        id: "usd-loan".into(),
        borrower: loans::Borrower::NonFinancialCorporate,
        product: loans::Product::OtherLoan,
        contract: loans::Contract::Pam { maturity: rd() + Days::new(15) },
        balance: m(60),
        rate: Q::new(5.into(), 100.into()),
        next_payment: rd() + Days::new(15),
        cycle_months: 3,
        performing: Uncertain::Known(true),
        lvr_at_most_80: Uncertain::Known(false),
        risk_weight: 100,
        encumbered_until: None,
    });
    b.currency.insert("usd-loan".into(), "USD".into());
    if with_fx_swap {
        // Pay AUD 100m, receive USD 102m (AUD equivalent) on the same day, to
        // fund USD assets: netted in the totals, gross in the currency split.
        b.derivatives.sets.push(derivatives::NettingSet {
            id: "fx".into(),
            counterparty: "bank-x".into(),
            netting_agreement: Some(false),
            ..Default::default()
        });
        for (ccy, amount) in [("AUD", -100), ("USD", 102)] {
            b.derivatives.flows.push(derivatives::Flow {
                set: "fx".into(),
                date: rd() + Days::new(3),
                amount: m(amount),
                currency: ccy.into(),
                fx_principal: true,
                fx_trading: false,
                option: false,
                in_the_money: Uncertain::Known(true),
            });
        }
    }
    b
}

fn ev(inputs: &BTreeMap<String, Q>) -> prudence_core::Evaluation<'static> {
    evaluate(arf210::lcr_all(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap()
}

#[test]
fn every_module_feeds_the_return_and_the_split_ties_out() {
    let r = lcr_return(&bank(false), rd(), Mode::Conservative).unwrap();
    let e = ev(&r.inputs);
    // Each module is present.
    for module in ["retail", "wholesale", "secured", "obligations", "loans"] {
        assert!(r.parts.iter().any(|p| p.module == module), "{module}");
    }
    // Totals are the sum of the parts.
    let parts_total: Q = r.parts.iter().filter(|p| p.item == "20.3").map(|p| p.amount.clone()).sum();
    assert_eq!(r.inputs["20.3"].clone() * Q::from_integer(1_000_000.into()), parts_total);
    // USD: the corporate deposit (40 at 40%) out, the loan (60 + coupon) in.
    assert_eq!(r.inputs["18.3"], Q::from_integer(40.into()));
    assert_eq!(r.inputs["18.3:weighted"], Q::from_integer(16.into()));
    assert_eq!(r.inputs["23.3"], Q::new(6075.into(), 100.into()));
    assert!(r.inputs["18.2"] > Q::default(), "NZD retail outflows");
    // Without FX derivatives the split ties out exactly, in both columns.
    let ccy: Vec<_> = e.findings.iter().filter(|f| f.check_id.contains("ccy")).collect();
    assert!(ccy.is_empty(), "{ccy:?}");
    assert!(e.q("30") > &Q::default(), "LCR computed");
    let hard: Vec<_> = e.findings.iter().filter(|f| !f.warn).collect();
    assert!(hard.is_empty(), "{hard:?}");
}

#[test]
fn fx_funding_swaps_are_gross_in_the_split_and_only_warn() {
    let r = lcr_return(&bank(true), rd(), Mode::Conservative).unwrap();
    // Totals: the same-day FX exchange nets to a 2m inflow.
    assert_eq!(r.inputs.get("13.1"), None); // no net outflow
    assert_eq!(r.inputs["21.1"], Q::from_integer(2.into()));
    let base = lcr_return(&bank(false), rd(), Mode::Conservative).unwrap();
    // Split: AUD 100m out, USD 102m in, gross.
    assert_eq!(&r.inputs["18.1"] - &base.inputs["18.1"], Q::from_integer(100.into()));
    assert_eq!(&r.inputs["23.3"] - &base.inputs["23.3"], Q::from_integer(102.into()));
    let e = ev(&r.inputs);
    let ccy: Vec<_> = e.findings.iter().filter(|f| f.check_id.contains("ccy")).collect();
    assert!(!ccy.is_empty() && ccy.iter().all(|f| f.warn), "{ccy:?}");
    // Trading FX is excluded from the split altogether.
    let mut b = bank(true);
    for f in &mut b.derivatives.flows {
        f.fx_trading = true;
    }
    let r = lcr_return(&b, rd(), Mode::Conservative).unwrap();
    assert_eq!(r.inputs["18.1"], base.inputs["18.1"]);
    assert_eq!(r.inputs["23.3"], base.inputs["23.3"]);
}

#[test]
fn amounts_must_come_with_a_currency() {
    let rates: BTreeMap<String, Q> = [("12.1".to_string(), Q::default())].into();
    assert!(assemble(&[], &rates).is_err());
}
