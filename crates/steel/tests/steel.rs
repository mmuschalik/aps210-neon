//! The steel thread: generated data, end to end, plus a test per capability
//! the full build depends on.

use std::collections::BTreeSet;

use assembly::steering;
use chrono::Days;
use prudence_core::Q;
use prudence_core::uncertain::Mode;
use steel::spikes::*;
use steel::*;

const SIZE: usize = 120;

fn cfg(seed: u64) -> Config {
    Config::new(seed, SIZE)
}

const FORMS: [&str; 8] = ["ARF 210.1A", "ARF 210.1B", "ARF 210.2", "ARF 210.3.1", "ARF 210.3.2", "ARF 210.4", "ARF 210.5", "ARF 210.6"];

fn forms_in(p: &checker::Package) -> BTreeSet<String> {
    let mut f: BTreeSet<String> = p.optional_inputs.keys().cloned().collect();
    f.insert("ARF 210.1A".into());
    f.insert("ARF 210.6".into());
    f
}

// ---------------------------------------------------------------- end to end

#[test]
fn every_return_from_generated_data_passes_the_strict_gate() {
    for seed in 1..=3 {
        let t = run(&cfg(seed), "lcr").unwrap();
        assert!(t.report.passed(), "seed {seed}: {:?}", t.report.failures());
        assert_eq!(t.bundle_id().map(str::len), Some(64));
        assert_eq!(t.report.bundle.as_ref().unwrap()["assurance"], "complete_evidence_consistency");
        let forms = forms_in(&t.package);
        for f in FORMS.iter().filter(|f| **f != "ARF 210.2") {
            assert!(forms.contains(*f), "seed {seed}: {f} missing");
        }
        // Every implemented cross-form rule of the LCR regime ran on engine output.
        let cov = t.report.bundle.as_ref().unwrap()["coverage"]["cross_form"].as_array().unwrap().clone();
        for c in &cov {
            let want = if c["rule"] == "daily_vs_mlh" { "skipped" } else { "evaluated" };
            assert_eq!(c["status"], want, "seed {seed}: {c}");
        }
        let r = t.ratios.as_ref().unwrap();
        assert!(r.lcr.is_some() && r.nsfr.is_some());
    }
}

#[test]
fn an_mlh_adi_from_generated_data_passes_the_strict_gate() {
    let t = run(&cfg(4), "mlh").unwrap();
    assert!(t.report.passed(), "{:?}", t.report.failures());
    assert!(forms_in(&t.package).contains("ARF 210.2"));
    let cov = t.report.bundle.as_ref().unwrap()["coverage"]["cross_form"].as_array().unwrap().clone();
    assert!(cov.iter().any(|c| c["rule"] == "daily_vs_mlh" && c["status"] == "evaluated"));
}

/// The generator is only useful if it reaches the hard paths of the rules.
#[test]
fn generated_data_reaches_the_hard_paths() {
    let mut c = cfg(1);
    c.unknown_pct = 30;
    let b = generate(&c);
    let ret = assembly::lcr_return(&b, c.reporting_date, Mode::Conservative).unwrap();
    // Unknowns resolved conservatively and logged. Only unknowns a rule
    // actually consults are logged (an unknown that cannot change the result
    // is not an assumption).
    let modules: BTreeSet<&str> = ret.assumptions.iter().map(|a| a.0).collect();
    assert!(modules.contains("retail") && modules.contains("wholesale") && modules.len() >= 4, "{modules:?}");
    // FCS allocations solved as LPs, with certificates.
    assert!(ret.certificates.len() > 5);
    // SME groups shared by several customers (the AUD 2m test is non-local).
    let mut per_group = std::collections::BTreeMap::<&String, usize>::new();
    for cu in &b.retail.customers {
        if let Some(g) = cu.sme_group.known() {
            *per_group.entry(g).or_default() += 1;
        }
    }
    assert!(per_group.values().any(|n| *n > 1));
    // Operational wholesale deposits with a measured operational balance.
    assert!(b.wholesale_deposits.iter().any(|d| matches!(d.operational_balance.known(), Some(q) if *q > Q::default())));
    assert!(ret.nsfr_parts.iter().any(|p| p.cell.starts_with("8.")), "operational deposits reach ARF 210.6 item 8");
    // Bitemporal facts: notices, early withdrawals, late-recorded collateral flows.
    assert!(b.retail.ledger.as_of(chrono::NaiveDate::MAX, chrono::NaiveDate::MAX).count() > 1);
    assert!(b.derivatives.collateral_flows.iter().any(|f| f.recorded > f.valid));
    // Foreign currency, so the currency split is non-trivial.
    assert!(ret.parts.iter().any(|p| p.currency != "AUD"));
    // Every LCR section is populated: stock, outflows, inflows.
    for item in ["1.", "9.", "14.", "20."] {
        assert!(ret.parts.iter().any(|p| p.item.starts_with(item)), "no contributions to {item}");
    }
}

#[test]
fn same_seed_same_bundle_and_the_package_survives_json() {
    let a = run(&cfg(7), "lcr").unwrap();
    let b = run(&cfg(7), "lcr").unwrap();
    assert_eq!(a.bundle_id(), b.bundle_id());
    let c = run(&cfg(8), "lcr").unwrap();
    assert_ne!(a.bundle_id(), c.bundle_id());
    // What the standalone `prudence-check` reads: JSON with exact rationals.
    let text = serde_json::to_string(&checker::json::to_json(&a.package)).unwrap();
    let back = checker::json::from_json(&serde_json::from_str(&text).unwrap()).unwrap();
    let again = checker::check_complete(&back);
    assert!(again.passed(), "{:?}", again.failures());
    assert_eq!(again.bundle.unwrap()["bundle_id"].as_str(), a.bundle_id());
}

// ---------------------------------------------------------------- the gate under attack

#[test]
fn the_gate_rejects_every_injected_fault() {
    let t = run(&cfg(2), "lcr").unwrap();
    assert!(t.report.passed(), "{:?}", t.report.failures());
    for f in FAULTS {
        let mut p = t.package.clone();
        (f.apply)(&mut p);
        let rep = checker::check_complete(&p);
        assert!(rep.bundle.is_none(), "not caught: {}", f.name);
        let fired: Vec<&str> = rep.checks.iter().filter(|(_, e)| !e.is_empty()).map(|(n, _)| *n).collect();
        assert!(fired.contains(&f.expect), "{}: expected `{}` to fire, got {fired:?}", f.name, f.expect);
    }
}

/// Finding: the assumption log is hashed into the bundle but not
/// re-derived. Dropping an entry changes the bundle id, so a replay against
/// the data snapshot would expose it, but the gate alone does not.
#[test]
fn a_dropped_assumption_changes_the_bundle_but_is_not_rejected() {
    let t = run(&cfg(2), "lcr").unwrap();
    let mut p = t.package.clone();
    p.assumptions.pop();
    let rep = checker::check_complete(&p);
    assert!(rep.passed());
    assert_ne!(rep.bundle.unwrap()["bundle_id"].as_str(), t.bundle_id());
}

// ---------------------------------------------------------------- explain

#[test]
fn day_on_day_movement_is_attributed_with_zero_residual() {
    let c = cfg(3);
    let (d0, before) = synth::snapshot(&c, 1);
    let after = generate(&c);
    let (start, steps, end) = explain(&before, d0, &after, c.reporting_date).unwrap();
    let total: Q = steps.iter().map(|s| s.surplus_change.clone()).sum();
    assert_eq!(start.surplus() + total, end.surplus(), "exact: no residual");
    assert!(steps.iter().filter(|s| s.surplus_change != Q::default()).count() >= 3, "several drivers moved");
    // And by record: every input's movement is the sum of its records' movements.
    let r0 = assembly::lcr_return(&before, d0, Mode::Conservative).unwrap();
    let r1 = assembly::lcr_return(&after, c.reporting_date, Mode::Conservative).unwrap();
    let by = record_attribution(&r0, &r1).unwrap();
    assert!(by.values().map(|m| m.len()).sum::<usize>() > 50);
}

// ---------------------------------------------------------------- incremental

/// Edit one record; only its group's outputs may change, and splicing a
/// recompute of just that group into the old return equals a full recompute.
#[test]
fn outputs_are_local_and_a_delta_update_equals_the_full_recompute() {
    let c = cfg(5);
    let rd = c.reporting_date;
    let base_bank = generate(&c);
    let base = assembly::lcr_return(&base_bank, rd, Mode::Conservative).unwrap();
    let sme = base_bank
        .retail
        .accounts
        .iter()
        .find(|a| base_bank.retail.customers.iter().any(|cu| cu.id == a.customer_id && cu.sme_group.known().is_some()))
        .unwrap()
        .id
        .clone();
    type Edit = Box<dyn Fn(&mut assembly::Bank) -> String>;
    let edits: Vec<(&str, Edit)> = vec![
        ("loan balance", Box::new(|b| { b.loans[3].balance += Q::from_integer(250_000.into()); b.loans[3].id.clone() })),
        ("retail balance", Box::new(|b| {
            let a = &mut b.retail.accounts[0];
            a.balance = prudence_core::money::AudCarrying::new(a.balance.amount() + Q::from_integer(300_000.into()));
            a.id.clone()
        })),
        ("SME balance (group-wide 2m test)", Box::new(move |b| {
            let a = b.retail.accounts.iter_mut().find(|a| a.id == sme).unwrap();
            a.balance = prudence_core::money::AudCarrying::new(a.balance.amount() + Q::from_integer(1_900_000.into()));
            a.id.clone()
        })),
        ("holding price", Box::new(|b| { b.holdings[2].market_value *= Q::new(9.into(), 10.into()); b.holdings[2].id.clone() })),
        ("wholesale balance", Box::new(|b| { b.wholesale_deposits[0].balance *= Q::from_integer(2.into()); b.wholesale_deposits[0].id.clone() })),
        ("facility undrawn", Box::new(|b| { b.commitments.facilities[0].undrawn += Q::from_integer(5_000_000.into()); b.commitments.facilities[0].id.clone() })),
    ];
    for (name, edit) in edits {
        let mut after = base_bank.clone();
        let id = edit(&mut after);
        let full = assembly::lcr_return(&after, rd, Mode::Conservative).unwrap();
        let scope: BTreeSet<String> = [id].into();
        let group = sub_bank_records(&sub_bank(&after, &scope));
        let changed = changed_records(&base, &full);
        assert!(!changed.is_empty(), "{name}: the edit changed nothing");
        assert!(changed.is_subset(&group), "{name}: {:?} changed outside the group {group:?}", changed.difference(&group).collect::<Vec<_>>());
        let (inc, n) = incremental(&base, &after, &scope, rd).unwrap();
        let want = assembly::assemble(&full.parts, &Default::default()).unwrap();
        assert_eq!(inc, want, "{name}: delta update differs from the full recompute");
        assert!(n < 30, "{name}: recomputed {n} records");
    }
}

/// Tainting is keyed by product class, not by account: one non-hardship
/// break moves every term deposit in that class, and nothing else.
#[test]
fn an_event_affects_exactly_its_product_class() {
    let c = cfg(6);
    let rd = c.reporting_date;
    let b = generate(&c);
    let base = assembly::lcr_return(&b, rd, Mode::Conservative).unwrap();
    let mut after = b.clone();
    let on = rd - Days::new(3);
    after.retail.ledger.record(
        deposits::EventKind::EarlyWithdrawal { account_id: "x".into(), product_class: "TD-6M".into(), hardship: false },
        on,
        on,
    );
    let full = assembly::lcr_return(&after, rd, Mode::Conservative).unwrap();
    // The class's term deposits change rate; a holder's other accounts can
    // change too, because the customer's one AUD 250k of FCS cover is
    // re-optimised across all their accounts. So the key is "customers
    // holding the class", not the class.
    let holders: BTreeSet<&str> = b.retail.accounts.iter().filter(|a| a.product_class == "TD-6M").map(|a| a.customer_id.as_str()).collect();
    let scope: BTreeSet<String> = b.retail.accounts.iter().filter(|a| holders.contains(a.customer_id.as_str())).map(|a| a.id.clone()).collect();
    let changed = changed_records(&base, &full);
    assert!(!changed.is_empty() && changed.is_subset(&scope), "{changed:?}");
    assert!(changed.iter().any(|id| b.retail.accounts.iter().any(|a| &a.id == id && a.product_class == "TD-6M")));
}

// ---------------------------------------------------------------- bitemporal replay

#[test]
fn a_return_replays_exactly_as_known_at_an_earlier_date() {
    let c = cfg(9);
    let rd = c.reporting_date;
    let mut b = generate(&c);
    // A notice given two days ago but only keyed in today.
    let acct = b.retail.accounts.iter().find(|a| matches!(a.contract, deposits::Contract::Clm { .. })).unwrap().id.clone();
    b.retail.ledger.record(
        deposits::EventKind::NoticeGiven { account_id: acct.clone(), repayment_date: rd + Days::new(20) },
        rd - Days::new(2),
        rd,
    );
    let now = replay_deposits(&b, rd, rd).unwrap();
    let then = replay_deposits(&b, rd, rd - Days::new(1)).unwrap();
    let diff: BTreeSet<&String> = now.iter().zip(&then).filter(|(x, y)| x != y).map(|(x, _)| &x.0).collect();
    assert_eq!(diff, [&acct].into(), "only the late notice's account differs");
    assert_eq!(replay_deposits(&b, rd, rd - Days::new(1)).unwrap(), then, "replay is deterministic");

    // A large collateral outflow 40 days ago, recorded only yesterday: the
    // 24-month lookback (item 13.6) as known two days ago does not see it.
    let set = b.derivatives.sets.iter().find(|s| s.netting_agreement == Some(true)).unwrap().id.clone();
    b.derivatives.collateral_flows.push(derivatives::CollateralFlow {
        set,
        amount: Q::from_integer((-900_000_000i64).into()),
        currency: "AUD".into(),
        valid: rd - Days::new(40),
        recorded: rd - Days::new(1),
    });
    let now = replay_derivatives(&b, rd, rd).unwrap();
    let then = replay_derivatives(&b, rd, rd - Days::new(2)).unwrap();
    assert!(now.get("13.6") > then.get("13.6"), "{:?} vs {:?}", now.get("13.6"), then.get("13.6"));
}

// ---------------------------------------------------------------- steering on generated data

#[test]
fn stress_and_data_quality_run_on_generated_data() {
    let c = cfg(10);
    let rd = c.reporting_date;
    let b = generate(&c);
    let tenth = Q::new(1.into(), 10.into());
    let scenarios = [
        steering::Scenario { name: "HQLA prices -10%".into(), price_shock: [tenth.clone(), tenth.clone(), tenth], ..Default::default() },
        steering::Scenario { name: "notice deposits called".into(), call_notice_deposits: true, ..Default::default() },
        steering::Scenario { name: "wholesale freeze".into(), freeze_wholesale_rollover: true, ..Default::default() },
    ];
    let out = steering::stress(&b, rd, &scenarios).unwrap();
    let base = out[0].1.surplus();
    for (name, l) in &out[1..] {
        assert!(l.surplus() < base, "{name} should hurt");
    }
    let (reported, _best, drags) = steering::data_quality(&b, rd, &Q::new(1.into(), 100.into())).unwrap();
    assert!(reported.lcr.is_some());
    let unknowns: usize = drags.iter().map(|d| d.unknowns).sum();
    assert!(unknowns > 0 && drags.iter().any(|d| d.liquid_assets_freed > Q::default()));
}

/// Wholesale unknowns (operational service, operational balance, FCS
/// eligibility) are logged like every other module's. The steel thread found
/// them missing from the assumption log; this guards the fix.
#[test]
fn wholesale_unknowns_are_logged() {
    let mut c = cfg(1);
    c.unknown_pct = 50;
    let b = generate(&c);
    let ret = assembly::lcr_return(&b, c.reporting_date, Mode::Conservative).unwrap();
    assert!(ret.assumptions.iter().any(|a| a.0 == "wholesale"));
}

// ---------------------------------------------------------------- signed overlays

fn overlay(rd: &str, expiry: &str, key: &[u8]) -> checker::Overlay {
    let mut o = checker::Overlay {
        id: "apra-ncO-overlay".into(),
        kind: checker::OverlayKind::ApraDiscretion,
        form: "ARF 210.1A".into(),
        cell: "29.1".into(),
        amount: Q::from_integer(3.into()),
        apra_instrument: Some("APRA letter 2026/123".into()),
        reason: "supervisory add-on to net cash outflows".into(),
        approver: "cro".into(),
        effective: rd.into(),
        expiry: expiry.into(),
        signature: String::new(),
    };
    o.signature = checker::sign(&o, key);
    o
}

/// An APRA overlay on generated data: in force and signed, it lowers the
/// LCR and every return still ties out; expired or unsigned, no bundle.
#[test]
fn a_signed_apra_overlay_flows_through_every_return() {
    let c = cfg(11);
    let rd = c.reporting_date;
    let key = b"cro-key".to_vec();
    let keys: std::collections::BTreeMap<String, Vec<u8>> = [("cro".to_string(), key.clone())].into();
    let b = generate(&c);
    let build = |o: &checker::Overlay| {
        let mut ret = assembly::lcr_return(&b, rd, Mode::Conservative).unwrap();
        let before = steering::lcr_of(&ret.inputs).unwrap();
        assembly::apply_overlays(&mut ret, std::slice::from_ref(o));
        let mut p = assembly::package_all(&b, &ret, context(&c, "lcr"), rd, Mode::Conservative, &synth::extras(&c)).unwrap();
        p.overlays = vec![o.clone()];
        (before, steering::lcr_of(&ret.inputs).unwrap(), p)
    };
    let (before, after, p) = build(&overlay("2026-07-01", "2027-06-30", &key));
    assert!(after.lcr < before.lcr, "the overlay adds to net cash outflows");
    let rep = checker::check_signed(&p, &keys);
    assert!(rep.passed(), "{:?}", rep.failures());
    let (_, _, p) = build(&overlay("2025-07-01", "2026-06-30", &key));
    assert!(checker::check_signed(&p, &keys).failures().iter().any(|f| f.contains("expired")));
    let (_, _, p) = build(&overlay("2026-07-01", "2027-06-30", b"someone-else"));
    assert!(checker::check_signed(&p, &keys).failures().iter().any(|f| f.contains("signature")));
}

#[test]
fn duckdb_export_and_canonical_counterparty() {
    let t = run(&cfg(1), "lcr").unwrap();
    assert!(!t.bank.counterparties.is_empty());
    assert_eq!(
        t.bank.counterparties.len(),
        t.bank.retail.customers.len() + t.bank.wholesale_customers.len()
    );
    let dir = std::env::temp_dir().join(format!("steel-duckdb-{}", std::process::id()));
    steel::sql_export::export_duckdb(&t, &dir).unwrap();
    for rel in [
        "run.sql",
        "data/counterparty.csv",
        "data/retail_sme/customer.csv",
        "data/retail_sme/deposit.csv",
        "data/retail_sme/alloc_covered.csv",
        "data/wholesale/alloc_insured.csv",
        "data/forms/arf_210_1a_inputs.csv",
        "data/forms/arf_210_1a_expected.csv",
        "data/forms/arf_210_6_inputs.csv",
        "data/forms/arf_210_6_expected.csv",
    ] {
        let content = std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert!(content.lines().count() >= 2, "{rel} should have header + data");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn whole_return_bitemporal_replay_via_known_at() {
    let c = cfg(2);
    let rd = c.reporting_date;
    let mut b = generate(&c);
    let set = b.derivatives.sets[0].id.clone();
    b.derivatives.collateral_flows.push(derivatives::CollateralFlow {
        set,
        amount: Q::from_integer((-900_000_000i64).into()),
        currency: "AUD".into(),
        valid: rd - Days::new(40),
        recorded: rd - Days::new(1),
    });
    let as_of_rd = assembly::lcr_return_as_known(&b, rd, rd, Mode::Conservative).unwrap();
    let earlier = assembly::lcr_return_as_known(&b, rd, rd - Days::new(2), Mode::Conservative).unwrap();
    assert!(
        as_of_rd.inputs.get("13.6") > earlier.inputs.get("13.6"),
        "late-recorded collateral flow should only appear once known_at >= recorded"
    );
}
