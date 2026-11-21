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
    let mut b = Bank {
        retail: deposits::testgen::Gen::new(7).book(rd()),
        ..Bank::default()
    };
    // Some retail accounts are in NZD.
    for a in b.retail.accounts.iter().step_by(3) {
        b.currency.insert(a.id.clone(), "NZD".into());
    }
    b.wholesale_customers.push(deposits::wholesale::Customer {
        id: "corp".into(),
        kind: deposits::wholesale::Kind::NonFinancialCorporate,
    });
    b.wholesale_deposits.push(deposits::wholesale::Deposit {
        id: "corp-usd".into(),
        customer_id: "corp".into(),
        balance: m(40),
        within_30_days: true,
        called: false,
        fcs_eligible: Uncertain::Known(false),
        operational_service: Uncertain::Known(false),
        operational_balance: Uncertain::unknown("n/a"),
        maturity: Some(rd() + Days::new(20)),
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
        affiliation: loans::Affiliation::IntraGroupOther,
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
    b.other_inflows.receivables.push(loans::Receivable {
        id: "settlement".into(),
        amount: m(5),
        due: rd() + Days::new(2),
        non_financial_revenue: false,
        contingent: false,
        performing: Uncertain::Known(true),
    });
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
    assert_eq!(r.inputs["21.4"], Q::from_integer(5.into()));
    assert_eq!(r.inputs["20.9"], r.inputs["20.3"], "the USD loan is intra-group");
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

// ---------------------------------------------------------------- the trusted gate

/// The ledger as the bank's finance system would supply it.
fn with_ledger(mut b: Bank) -> Bank {
    let r = lcr_return(&b, rd(), Mode::Conservative).unwrap();
    for (id, kind, bal, _) in &r.records {
        let acct = b.gl_account.get(id).cloned().unwrap_or_else(|| default_account(kind).into());
        *b.gl.entry(acct).or_default() += bal.clone();
    }
    b
}

fn gate(b: &Bank) -> (checker::Package, checker::Report) {
    let r = lcr_return(b, rd(), Mode::Conservative).unwrap();
    let p = package(b, &r, rd());
    let rep = checker::check(&p);
    (p, rep)
}

#[test]
fn an_honest_return_passes_and_gets_a_content_addressed_bundle() {
    let b = with_ledger(bank(true));
    let (p, rep) = gate(&b);
    assert!(rep.passed(), "{:?}", rep.failures());
    assert!(!p.certificates.is_empty(), "the FCS allocations carry LP certificates");
    let bundle = rep.bundle.unwrap();
    // FX funding swaps make the currency split differ from the totals: a warning.
    assert!(bundle["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("ccy")));
    // Same data and specs: same bundle id.
    let again = checker::check(&p).bundle.unwrap();
    assert_eq!(bundle["bundle_id"], again["bundle_id"]);
    assert_eq!(bundle["bundle_id"].as_str().unwrap().len(), 64);
    // A declared form spec must match the rules actually executed.
    let mut q = p.clone();
    q.specs[0].1.push(' ');
    fails(&q, "evidence integrity");
}

fn fails(p: &checker::Package, check: &str) {
    let rep = checker::check(p);
    assert!(rep.bundle.is_none(), "a failing return must not get a bundle");
    let failed: Vec<_> = rep.checks.iter().filter(|(_, f)| !f.is_empty()).map(|(c, _)| *c).collect();
    assert_eq!(failed, [check], "{:?}", rep.failures());
}

#[test]
fn the_gate_catches_each_kind_of_error() {
    let b = with_ledger(bank(false));
    let (p, rep) = gate(&b);
    assert!(rep.passed(), "{:?}", rep.failures());

    // A record missing from the run: the ledger does not tie out.
    let mut q = p.clone();
    q.records.retain(|r| r.id != "acgb");
    let failures = checker::check(&q).failures();
    assert!(failures.iter().any(|f| f.starts_with("gl tie-out:")));
    assert!(failures.iter().any(|f| f.starts_with("evidence integrity:")));

    // A slice lost from a split deposit.
    let mut q = p.clone();
    let i = q.slices.iter().position(|(r, _)| r == "corp-usd").unwrap();
    q.slices.remove(i);
    fails(&q, "conservation");

    // A lodged figure that the contributions do not support.
    let mut q = p.clone();
    *q.lcr_inputs.get_mut("20.3").unwrap() += Q::from_integer(1.into());
    fails(&q, "inputs = contributions");

    // An HQLA security also counted as a maturing-securities inflow.
    let mut q = p.clone();
    let part = checker::Part {
        record: "acgb".into(),
        item: "21.3".into(),
        currency: "AUD".into(),
        amount: m(1),
        stream: checker::Stream::Both,
    };
    q.parts.push(part);
    q.lcr_inputs = checker::derive_lcr_inputs(&q.parts, &q.rates).unwrap();
    q.optional_inputs.insert("ARF 210.1B".into(), checker::derive_aud_inputs(&q.parts, &q.rates));
    fails(&q, "no double counting");

    // A tampered LP certificate.
    let mut q = p.clone();
    q.certificates[0].solution.value += Q::from_integer(1.into());
    fails(&q, "lp certificates");
}

/// What the engine writes out is exactly what the standalone checker reads:
/// a whole-bank package (with FCS certificates) survives JSON unchanged.
#[test]
fn the_package_round_trips_through_json() {
    let b = with_ledger(bank(true));
    let (p, rep) = gate(&b);
    let text = checker::json::to_json(&p).to_string();
    let back = checker::json::from_json(&serde_json::from_str(&text).unwrap()).unwrap();
    let again = checker::check(&back);
    assert_eq!(rep.bundle.unwrap()["bundle_id"], again.bundle.unwrap()["bundle_id"]);
}

#[test]
fn nsfr_is_reconciled_to_source_contributions() {
    let (mut p, report) = gate(&with_ledger(bank(true)));
    assert!(report.passed(), "{:?}", report.failures());
    assert!(!p.form_contributions["ARF 210.6"].is_empty());
    let key = p.nsfr_inputs.keys().next().unwrap().clone();
    *p.nsfr_inputs.get_mut(&key).unwrap() += Q::from_integer(1.into());
    assert!(checker::check(&p).failures().iter().any(|f| f.starts_with("inputs = contributions: ARF 210.6")));
}

#[test]
fn dropping_nsfr_evidence_cannot_preserve_reconciliation() {
    let (mut p, _) = gate(&with_ledger(bank(true)));
    p.form_contributions.get_mut("ARF 210.6").unwrap().clear();
    assert!(checker::check(&p).failures().iter().any(|f| f.starts_with("inputs = contributions: ARF 210.6")));
}

#[test]
fn valid_lp_cannot_hide_changed_allocation_outputs() {
    let (p, report) = gate(&with_ledger(bank(true)));
    assert!(report.passed(), "{:?}", report.failures());
    assert!(p.certificates.iter().all(|c| !c.subjects.is_empty()));
    let mut changed = p.clone();
    let slice = changed.allocation_slices.iter_mut().find(|s| s.amount > Q::default()).unwrap();
    slice.covered = !slice.covered;
    assert!(checker::check(&changed).failures().iter().any(|f| f.starts_with("allocation bindings:")));
    let mut changed = p.clone();
    let source = changed.certificates[0].subjects[0].clone();
    let part = changed.parts.iter_mut().find(|part| part.record == source).unwrap();
    part.amount += Q::from_integer(1.into());
    changed.lcr_inputs = checker::derive_lcr_inputs(&changed.parts, &changed.rates).unwrap();
    changed
        .optional_inputs
        .insert("ARF 210.1B".into(), checker::derive_aud_inputs(&changed.parts, &changed.rates));
    assert!(checker::check(&changed).failures().iter().any(|f| f.contains("LCR allocation output")));
    let mut changed = p.clone();
    changed.certificates[0].subjects[0] = "invented".into();
    assert!(checker::check(&changed).failures().iter().any(|f| f.starts_with("allocation bindings:")));
}

#[test]
fn allocation_evidence_roundtrips_and_is_content_bound() {
    let (p, _) = gate(&with_ledger(bank(true)));
    let b = checker::check(&p).bundle.unwrap();
    let back = checker::json::from_json(&checker::json::to_json(&p)).unwrap();
    assert_eq!(checker::check(&back).bundle.unwrap(), b);
    assert_eq!(b["coverage"]["allocation_bound"].as_array().unwrap().len(), p.certificates.len());
    assert!(b["coverage"]["allocation_binding_missing"].as_array().unwrap().is_empty());
}

#[test]
fn valid_certificate_for_substituted_problem_is_rejected() {
    let (mut p, _) = gate(&with_ledger(bank(true)));
    let c = &mut p.certificates[0];
    c.lp.objective.fill(Q::default());
    c.solution = prudence_core::lp::solve(&c.lp).unwrap();
    prudence_core::lp::verify(&c.lp, &c.solution).unwrap();
    assert!(checker::check(&p).failures().iter().any(|f| f.contains("reconstructed FCS")));
}

#[test]
fn every_engine_allocation_has_a_reconstructed_problem() {
    let (p, report) = gate(&with_ledger(bank(true)));
    assert!(report.passed(), "{:?}", report.failures());
    let b = report.bundle.unwrap();
    assert_eq!(b["coverage"]["fcs_problem_reconstructed"].as_array().unwrap().len(), p.certificates.len());
    let mut p = p;
    p.certificates[0].fcs.as_mut().unwrap().eligible.clear();
    assert!(checker::check(&p).failures().iter().any(|f| f.contains("FCS evidence has wrong shape")));
}

#[test]
fn complete_gate_requires_all_evidence_and_accepts_assembled_package() {
    let (mut p, _) = gate(&with_ledger(bank(true)));
    let c = checker::ReportingContext {
        entity: "test-bank".into(),
        reporting_date: p.reporting_date.clone(),
        consolidation: "level1".into(),
        regime: "lcr".into(),
    };
    for code in ["ARF 210.1A", "ARF 210.1B", "ARF 210.6"] {
        p.contexts.insert(code.into(), c.clone());
    }
    let report = checker::check_complete(&p);
    assert!(report.passed(), "{:?}", report.failures());
    assert_eq!(report.bundle.unwrap()["assurance"], "complete_evidence_consistency");
    let mut missing = p.clone();
    missing.form_contributions.remove("ARF 210.6");
    assert!(checker::check_complete(&missing).failures().iter().any(|f| f.contains("contribution evidence is required")));
    let mut missing = p.clone();
    missing.certificates[0].fcs = None;
    assert!(checker::check_complete(&missing).failures().iter().any(|f| f.contains("supported problem evidence")));
    let mut missing = p;
    missing.contexts.clear();
    assert!(checker::check_complete(&missing).failures().iter().any(|f| f.contains("reporting context is required")));
}

/// Every return the engine can produce, from one bank, through the strict
/// gate: each cross-form rule of the design's verification gate runs on
/// engine output (210.2 is for MLH ADIs, so its rule is skipped by regime).
#[test]
fn all_returns_from_one_bank_pass_every_cross_form_rule() {
    let b = with_ledger(bank(true));
    let r = lcr_return(&b, rd(), Mode::Conservative).unwrap();
    let context = checker::ReportingContext {
        entity: "test-bank".into(),
        reporting_date: rd().to_string(),
        consolidation: "level1".into(),
        regime: "lcr".into(),
    };
    let p = package_all(&b, &r, context, rd(), Mode::Conservative, &assembly::maturity::Extras::default()).unwrap();
    // The maturity ladders hold the modelled balance sheet.
    let assets = &p.optional_inputs["ARF 210.3.1"];
    assert!(assets.contains_key("2.1.1:y10plus"), "{assets:?}"); // the bond, no maturity
    assert!(assets.keys().any(|k| k.starts_with("6.1.3:")), "the corporate loan");
    let rep = checker::check_complete(&p);
    assert!(rep.passed(), "{:?}", rep.failures());
    let bundle = rep.bundle.unwrap();
    let cov = bundle["coverage"]["cross_form"].as_array().unwrap();
    let status = |rule: &str| cov.iter().find(|c| c["rule"] == rule).unwrap()["status"].as_str().unwrap().to_string();
    for rule in [
        "nsfr_vs_lcr",
        "aud_vs_all",
        "daily_lcr_stock",
        "daily_all_lcr_ratio",
        "daily_aud_lcr_ratio",
        "funding_plan_vs_assets",
        "funding_plan_vs_liabilities",
    ] {
        assert_eq!(status(rule), "evaluated", "{rule}");
    }
    assert_eq!(status("daily_vs_mlh"), "skipped");

    // A lodged 210.4 that disagrees with the maturity ladder is caught.
    let mut q = p.clone();
    let k = q.optional_inputs["ARF 210.4"].keys().next().unwrap().clone();
    *q.optional_inputs.get_mut("ARF 210.4").unwrap().get_mut(&k).unwrap() += Q::from_integer(1.into());
    let rep = checker::check_complete(&q);
    assert!(!rep.passed() && rep.bundle.is_none());
    // A daily report whose HQLA differ from the LCR return is caught.
    let mut q = p.clone();
    q.optional_inputs.get_mut("ARF 210.5").unwrap().insert("1.1:balance".into(), Q::from_integer(1.into()));
    assert!(checker::check_complete(&q).failures().iter().any(|f| f.contains("210.5:1.1=210.1A:25")));
}

fn ctx(regime: &str) -> checker::ReportingContext {
    checker::ReportingContext {
        entity: "test-bank".into(),
        reporting_date: rd().to_string(),
        consolidation: "level1".into(),
        regime: regime.into(),
    }
}

/// ARF 210.5 history from earlier daily snapshots and ARF 210.4 forecasts
/// from the funding plan, through the strict gate.
#[test]
fn daily_history_and_funding_plan() {
    let mut b = with_ledger(bank(true));
    b.derivatives.sets.push(derivatives::NettingSet { id: "csa".into(), netting_agreement: Some(true), ..Default::default() });
    let paid = rd() - Days::new(2);
    b.derivatives.collateral_flows.push(derivatives::CollateralFlow {
        set: "csa".into(),
        amount: m(-7),
        currency: "AUD".into(),
        valid: paid,
        recorded: paid,
    });
    let mut prior = bank(true);
    prior.loans[0].balance = m(50); // the loan book grew by 10m over the week
    let extras = assembly::maturity::Extras {
        recent: (1..=4).map(|k| (rd() - Days::new(k), bank(true))).collect(),
        prior: Some((rd() - Days::new(7), prior)),
        plan: [("9.1.1:m12".to_string(), m(55))].into(),
    };
    let r = lcr_return(&b, rd(), Mode::Conservative).unwrap();
    let p = package_all(&b, &r, ctx("lcr"), rd(), Mode::Conservative, &extras).unwrap();
    let daily = &p.optional_inputs["ARF 210.5"];
    for k in ["5.2:day2", "5.2:day5", "1.1:prior", "2.2:prior", "2.2:balance", "3.1:outflow", "4.3:outflow"] {
        assert!(daily.contains_key(k), "{k}: {daily:?}");
    }
    assert_eq!(daily["3.1:outflow"], Q::from_integer(10.into()));
    assert_eq!(daily["4.3:outflow"], Q::from_integer(7.into()));
    assert_eq!(p.optional_inputs["ARF 210.4"]["9.1.1:m12"], Q::from_integer(55.into()));
    let rep = checker::check_complete(&p);
    assert!(rep.passed(), "{:?}", rep.failures());
}

/// An MLH ADI: ARF 210.2 from the holdings, liabilities and commitments, and
/// ARF 210.5 tied to it.
#[test]
fn mlh_adi_returns() {
    let b = with_ledger(bank(false));
    let r = lcr_return(&b, rd(), Mode::Conservative).unwrap();
    let p = package_all(&b, &r, ctx("mlh"), rd(), Mode::Conservative, &assembly::maturity::Extras::default()).unwrap();
    let mlh = &p.optional_inputs["ARF 210.2"];
    assert_eq!(mlh["2.1:amount"], Q::from_integer(500.into())); // the Commonwealth bond
    assert!(mlh["12.1:amount"] > Q::default() && mlh["13.4:amount"] == Q::from_integer(80.into()));
    let rep = checker::check_complete(&p);
    assert!(rep.passed(), "{:?}", rep.failures());
    let bundle = rep.bundle.unwrap();
    let cov = bundle["coverage"]["cross_form"].as_array().unwrap();
    let st = |rule: &str| cov.iter().find(|c| c["rule"] == rule).unwrap()["status"].as_str().unwrap().to_string();
    assert_eq!(st("daily_vs_mlh"), "evaluated");
    assert_eq!(st("daily_lcr_stock"), "skipped");
    // A daily report out of line with ARF 210.2 is caught.
    let mut q = p.clone();
    q.optional_inputs.get_mut("ARF 210.5").unwrap().insert("1.1:balance".into(), Q::from_integer(1.into()));
    assert!(checker::check_complete(&q).failures().iter().any(|f| f.contains("210.5:1.1=210.2")));
}
