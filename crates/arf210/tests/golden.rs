//! Parity with the original (verified) Python implementation.
//!
//! `tests/golden/*.json` hold inputs and every resulting cell, produced by the
//! Python evaluator (50-digit Decimal) before it was retired. Rust evaluates
//! exactly over rationals, so values must agree to within Python's precision;
//! cells Python reported as non-finite (x / 0) must be Undefined here.

use std::collections::BTreeSet;

use num_traits::Signed;
use prudence_core::num::parse_decimal;
use prudence_core::{Missing, Q, Ref, Value, evaluate};
use serde_json::Value as Json;

/// Parse Python's `str(Decimal)`, which may use exponent notation (`1E+2`).
fn py_decimal(s: &str) -> Q {
    let (mant, exp) = match s.split_once(['E', 'e']) {
        Some((m, e)) => (m, e.parse::<i32>().unwrap()),
        None => (s, 0),
    };
    let m = parse_decimal(mant).unwrap_or_else(|| panic!("bad decimal {s:?}"));
    let ten = Q::from_integer(10.into());
    if exp >= 0 { m * num_traits::pow(ten, exp as usize) } else { m / num_traits::pow(ten, (-exp) as usize) }
}

fn check(code: &str, file: &str) {
    let form = arf210::form(code).unwrap();
    let path = format!("{}/tests/golden/{file}.json", env!("CARGO_MANIFEST_DIR"));
    let vectors: Json = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let tol = parse_decimal("0.000000000000000000000000000001").unwrap(); // 1e-30
    let mut compared = 0usize;
    for (n, v) in vectors.as_array().unwrap().iter().enumerate() {
        let inputs: Vec<(String, Q)> = v["inputs"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, val)| (k.clone(), py_decimal(val.as_str().unwrap())))
            .collect();
        let ev = evaluate(form, inputs, Missing::Zero).unwrap();
        for (key, want) in v["cells"].as_object().unwrap() {
            let (item, col) = key.split_once(':').unwrap();
            let got = &ev.values[&Ref::new(item, col)];
            match (want.as_str(), got) {
                (None, Value::Undefined) => {}
                (Some(w), Value::Num(g)) => {
                    let w = py_decimal(w);
                    let scale = if w.abs() > Q::from_integer(1.into()) { w.abs() } else { Q::from_integer(1.into()) };
                    assert!(
                        (g - &w).abs() <= &tol * scale,
                        "{code} vector {n} cell {key}: rust {g} vs python {w}"
                    );
                }
                (w, g) => panic!("{code} vector {n} cell {key}: python {w:?} vs rust {g:?}"),
            }
            compared += 1;
        }
        let want: BTreeSet<&str> =
            v["failed_checks"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        let got: BTreeSet<&str> = ev.findings.iter().map(|f| f.check_id.as_str()).collect();
        assert_eq!(want, got, "{code} vector {n}: failed checks differ");
    }
    assert!(compared >= 12 * form.items.len(), "{code}: only {compared} cells compared");
}

#[test]
fn lcr_all_currencies_matches_python() {
    check("210.1A", "arf_210_1a");
}

#[test]
fn lcr_aud_matches_python() {
    check("210.1B", "arf_210_1b");
}

#[test]
fn mlh_matches_python() {
    check("210.2", "arf_210_2");
}

#[test]
fn nsfr_matches_python() {
    check("210.6", "arf_210_6");
}
