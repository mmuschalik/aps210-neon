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
        Part { record: "bond".into(), item: "1.3.1".into(), currency: "AUD".into(), amount: q(100_000_000), stream: Stream::Both },
        Part { record: "dep".into(), item: "10.1".into(), currency: "AUD".into(), amount: q(40_000_000), stream: Stream::Both },
        Part { record: "loan".into(), item: "20.3".into(), currency: "USD".into(), amount: q(10_000_000), stream: Stream::Both },
    ];
    let lp = Lp { sense: Sense::Max, objective: vec![q(1), q(1)], rows: vec![(vec![q(1), q(1)], Rel::Le, q(250))] };
    let solution = solve(&lp).unwrap();
    Package {
        reporting_date: "2026-09-30".into(),
        gl: [("1200".to_string(), q(100_000_000)), ("1400".into(), q(60_000_000)), ("2100".into(), q(40_000_000))].into(),
        records: vec![
            Record { id: "bond".into(), gl_account: "1200".into(), balance: q(100_000_000), split: false },
            Record { id: "loan".into(), gl_account: "1400".into(), balance: q(60_000_000), split: false },
            Record { id: "dep".into(), gl_account: "2100".into(), balance: q(40_000_000), split: true },
        ],
        slices: vec![("dep".into(), q(40_000_000))],
        lcr_inputs: derive_lcr_inputs(&parts, &BTreeMap::new()).unwrap(),
        parts,
        certificates: vec![Certificate { name: "toy".into(), lp, solution }],
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
