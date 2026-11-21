//! The checker as a separate program: package JSON in, report and bundle out.

use std::collections::BTreeMap;
use std::process::Command;

use checker::*;
use prudence_core::Q;
use prudence_core::lp::{Lp, Rel, Sense, solve};

fn q(n: i64) -> Q {
    Q::from_integer(n.into())
}

/// A small consistent return: one HQLA1 bond, one maturing loan inflow,
/// one deposit run-off, a ledger, and an LP certificate.
fn package() -> Package {
    let parts = vec![
        Part {
            record: "bond".into(),
            item: "1.3.1".into(),
            currency: "AUD".into(),
            amount: q(100_000_000),
            stream: Stream::Both,
        },
        Part {
            record: "dep".into(),
            item: "10.1".into(),
            currency: "AUD".into(),
            amount: q(40_000_000),
            stream: Stream::Both,
        },
        Part {
            record: "loan".into(),
            item: "20.3".into(),
            currency: "USD".into(),
            amount: q(10_000_000),
            stream: Stream::Both,
        },
    ];
    let lp = Lp {
        sense: Sense::Max,
        objective: vec![q(1), q(1)],
        rows: vec![(vec![q(1), q(1)], Rel::Le, q(250))],
    };
    let solution = solve(&lp).unwrap();
    Package {
        reporting_date: "2026-09-30".into(),
        gl: [("1200".to_string(), q(100_000_000)), ("1400".into(), q(60_000_000)), ("2100".into(), q(40_000_000))].into(),
        records: vec![
            Record {
                id: "bond".into(),
                gl_account: "1200".into(),
                balance: q(100_000_000),
                split: false,
            },
            Record {
                id: "loan".into(),
                gl_account: "1400".into(),
                balance: q(60_000_000),
                split: false,
            },
            Record {
                id: "dep".into(),
                gl_account: "2100".into(),
                balance: q(40_000_000),
                split: true,
            },
        ],
        slices: vec![("dep".into(), q(40_000_000))],
        lcr_inputs: derive_lcr_inputs(&parts, &BTreeMap::new()).unwrap(),
        parts,
        certificates: vec![Certificate {
            name: "toy".into(),
            subjects: vec![],
            fcs: None,
            lp,
            solution,
        }],
        specs: vec![("forms/ARF 210.1A".into(), arf210::SPECS[0].1.into())],
        ..Package::default()
    }
}

fn run(p: &Package, name: &str) -> (bool, String, Option<serde_json::Value>) {
    let dir = std::env::temp_dir().join(format!("prudence-check-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("package.json");
    let bundle = dir.join("bundle.json");
    std::fs::write(&input, json::to_json(p).to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_prudence-check"))
        .arg(&input)
        .arg("--allow-partial")
        .arg("--bundle")
        .arg(&bundle)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let b = std::fs::read_to_string(&bundle).ok().map(|t| serde_json::from_str(&t).unwrap());
    (out.status.success(), text, b)
}

#[test]
fn json_round_trip_keeps_the_bundle_id() {
    let p = package();
    let back = json::from_json(&json::to_json(&p)).unwrap();
    let (a, b) = (check(&p), check(&back));
    assert!(a.passed(), "{:?}", a.failures());
    assert_eq!(a.bundle.unwrap()["bundle_id"], b.bundle.unwrap()["bundle_id"]);
}

#[test]
fn the_program_signs_off_a_good_return_and_refuses_a_bad_one() {
    let (ok, text, bundle) = run(&package(), "good");
    assert!(ok, "{text}");
    assert!(text.lines().filter(|l| l.starts_with("PASS")).count() >= 7, "{text}");
    let bundle = bundle.expect("bundle written");
    assert!(text.contains(bundle["bundle_id"].as_str().unwrap()));

    let mut bad = package();
    bad.gl.insert("1400".into(), q(61_000_000));
    let (ok, text, bundle) = run(&bad, "bad");
    assert!(!ok);
    assert!(text.contains("FAIL gl tie-out") && text.contains("account 1400"), "{text}");
    assert!(bundle.is_none());
}

#[test]
fn optional_returns_roundtrip_and_detect_tampering() {
    let mut p = package();
    p.optional_inputs.insert("ARF 210.1B".into(), derive_aud_inputs(&p.parts, &p.rates));
    let report = check(&p);
    assert!(report.passed(), "{:?}", report.failures());
    let original = report.bundle.unwrap();
    let back = json::from_json(&json::to_json(&p)).unwrap();
    assert_eq!(original, check(&back).bundle.unwrap());
    p.optional_inputs.get_mut("ARF 210.1B").unwrap().insert("10.1".into(), q(41));
    assert!(check(&p).failures().iter().any(|f| f.starts_with("cross-form:")));
    let mut legacy = json::to_json(&package());
    legacy.as_object_mut().unwrap().remove("optional_inputs");
    assert!(json::from_json(&legacy).unwrap().optional_inputs.is_empty());
}

#[test]
fn each_maturity_side_is_checked_independently() {
    for (code, plan_item) in [("ARF 210.3.1", "1:actual"), ("ARF 210.3.2", "9:actual")] {
        let mut p = package();
        p.optional_inputs.insert(code.into(), BTreeMap::new());
        p.optional_inputs.insert("ARF 210.4".into(), [(plan_item.into(), q(1))].into());
        assert!(check(&p).failures().iter().any(|f| f.starts_with("cross-form:")), "{code}");
    }
}

#[test]
fn invented_sources_and_aud_stock_are_rejected() {
    let mut p = package();
    p.parts[0].record = "invented".into();
    assert!(check(&p).failures().iter().any(|f| f.contains("unknown source invented")));
    let mut p = package();
    let mut aud = derive_aud_inputs(&p.parts, &p.rates);
    aud.insert("1.3.1".into(), q(999));
    p.optional_inputs.insert("ARF 210.1B".into(), aud);
    assert!(check(&p).failures().iter().any(|f| f.contains("ARF 210.1B 1.3.1")));
}

#[test]
fn undefined_lcr_and_evidence_scope_are_explicit() {
    let p = Package::default();
    let b = check(&p).bundle.expect("zero return is a scoped result");
    assert!(b["lcr_percent"].is_null());
    assert_eq!(b["lcr_status"], "undefined");
    assert_eq!(b["assurance"], "scoped_consistency");
    assert!(!b["coverage"]["unverified"].as_array().unwrap().is_empty());
}

#[test]
fn executed_specs_cannot_be_relabelled_by_the_caller() {
    let mut p = package();
    let original = check(&p).bundle.unwrap();
    p.specs[0].1.push(' ');
    assert!(check(&p).bundle.is_none());
    p.specs = vec![("arbitrary declaration".into(), "untrusted".into())];
    let b = check(&p).bundle.unwrap();
    assert_eq!(original["spec_version"], b["spec_version"]);
    assert_ne!(original["package_sha256"], b["package_sha256"]);
}

#[test]
fn rate_map_cannot_inject_amounts_and_lp_sense_is_strict() {
    let mut p = package();
    p.rates.insert("1.3.1".into(), q(999));
    assert!(check(&p).failures().iter().any(|f| f.contains("non-rate input")));
    let mut v = json::to_json(&package());
    v["certificates"][0]["lp"]["sense"] = serde_json::json!("typo");
    assert!(json::from_json(&v).is_err());
}

#[test]
fn malformed_lp_rows_fail_without_panicking() {
    let mut p = package();
    p.certificates[0].lp.rows[0].0.clear();
    assert!(check(&p).failures().iter().any(|f| f.contains("LP row has the wrong shape")));
}

#[test]
fn optional_form_contributions_require_known_sources_and_exact_totals() {
    let mut p = package();
    p.optional_inputs.insert("ARF 210.3.1".into(), [("1.1:at_call".into(), q(10))].into());
    p.form_contributions.insert(
        "ARF 210.3.1".into(),
        vec![FormContribution {
            source: "bond".into(),
            cell: "1.1:at_call".into(),
            amount: q(10),
        }],
    );
    assert!(check(&p).passed(), "{:?}", check(&p).failures());
    let back = json::from_json(&json::to_json(&p)).unwrap();
    assert_eq!(check(&p).bundle, check(&back).bundle);
    p.form_contributions.get_mut("ARF 210.3.1").unwrap()[0].source = "invented".into();
    assert!(check(&p).failures().iter().any(|f| f.contains("unknown source invented")));
    p.form_contributions.get_mut("ARF 210.3.1").unwrap()[0].source = "bond".into();
    p.form_contributions.get_mut("ARF 210.3.1").unwrap()[0].amount = q(11);
    assert!(check(&p).failures().iter().any(|f| f.starts_with("inputs = contributions: ARF 210.3.1")));
}

fn set_context(p: &mut Package, regime: &str, consolidation: &str) {
    let c = ReportingContext {
        entity: "bank".into(),
        reporting_date: p.reporting_date.clone(),
        regime: regime.into(),
        consolidation: consolidation.into(),
    };
    for code in ["ARF 210.1A", "ARF 210.6"].into_iter().chain(p.optional_inputs.keys().map(String::as_str)) {
        p.contexts.insert(code.into(), c.clone());
    }
}

#[test]
fn mixed_reporting_contexts_fail_and_roundtrip() {
    let mut p = package();
    set_context(&mut p, "lcr", "level1");
    assert!(check(&p).passed());
    assert_eq!(json::from_json(&json::to_json(&p)).unwrap().contexts, p.contexts);
    p.contexts.get_mut("ARF 210.6").unwrap().entity = "other bank".into();
    assert!(check(&p).failures().iter().any(|f| f.contains("contexts differ")));
    p.contexts.remove("ARF 210.6");
    assert!(check(&p).failures().iter().any(|f| f.contains("missing reporting context")));
}

#[test]
fn daily_rules_follow_regime_and_level() {
    let mut p = package();
    p.optional_inputs.insert("ARF 210.5".into(), BTreeMap::new());
    p.optional_inputs.insert("ARF 210.2".into(), BTreeMap::new());
    set_context(&mut p, "mlh", "level1");
    assert!(!check(&p).failures().iter().any(|f| f.contains("210.5:1.1=210.1A")));
    set_context(&mut p, "lcr", "level1");
    assert!(check(&p).failures().iter().any(|f| f.contains("210.5:1.1=210.1A")));
    p.optional_inputs.get_mut("ARF 210.5").unwrap().insert("1.1:balance".into(), q(100));
    set_context(&mut p, "lcr", "level2");
    assert!(!check(&p).failures().iter().any(|f| f.contains("210.5:5.")));
}

#[test]
fn cli_defaults_to_complete_evidence() {
    let dir = std::env::temp_dir().join(format!("checker-complete-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("package.json");
    let mut p = package();
    std::fs::write(&path, json::to_json(&p).to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_prudence-check")).arg(&path).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("FAIL required coverage"));
    p.certificates.clear();
    p.form_contributions.insert("ARF 210.6".into(), vec![]);
    set_context(&mut p, "lcr", "level1");
    std::fs::write(&path, json::to_json(&p).to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_prudence-check")).arg(&path).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn complete_gate_refuses_optional_returns_without_evidence() {
    let mut p = package();
    p.certificates.clear();
    p.form_contributions.insert("ARF 210.6".into(), vec![]);
    p.optional_inputs.insert("ARF 210.3.1".into(), BTreeMap::new());
    set_context(&mut p, "lcr", "level1");
    assert!(check_complete(&p).failures().iter().any(|f| f.contains("ARF 210.3.1: contribution evidence is required")));
    p.form_contributions.insert("ARF 210.3.1".into(), vec![]);
    let r = check_complete(&p);
    assert!(r.passed(), "{:?}", r.failures());
    let coverage = r.bundle.unwrap()["coverage"]["cross_form"].as_array().unwrap().clone();
    assert!(coverage.iter().any(|v| v["rule"] == "funding_plan_vs_assets" && v["status"] == "skipped"));
    assert!(coverage.iter().any(|v| v["rule"] == "nsfr_vs_lcr" && v["status"] == "evaluated"));
}

#[test]
fn impossible_reporting_dates_fail_context_validation() {
    let mut p = package();
    p.reporting_date = "2026-02-30".into();
    set_context(&mut p, "lcr", "level1");
    assert!(check(&p).failures().iter().any(|f| f.contains("invalid reporting date")));
}
