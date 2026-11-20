//! Hand-worked scenarios: each one documents a rule in APS 210 / ARS 210.0.

use std::path::Path;

use arf210::crossform::nsfr_vs_lcr;
use arf210::{lcr_all, lcr_aud, mlh, nsfr};
use prudence_core::num::{int, parse_decimal};
use prudence_core::{Evaluation, Form, InputError, Missing, Q, Value, evaluate};

fn d(s: &str) -> Q {
    parse_decimal(s).unwrap()
}

fn run<'f>(form: &'f Form, inputs: &[(&str, &str)]) -> Evaluation<'f> {
    evaluate(form, inputs.iter().map(|(k, v)| (*k, d(v))), Missing::Zero).unwrap()
}

fn failed(ev: &Evaluation) -> Vec<String> {
    ev.findings.iter().map(|f| f.check_id.clone()).collect()
}

// ------------------------------------------------------------ spec integrity

fn key(i: &str) -> Vec<u32> {
    i.split('.').map(|p| p.parse().unwrap()).collect()
}

#[test]
fn item_ids_match_the_pdf() {
    // Fixtures were extracted mechanically from ars-210.0.pdf.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for (form, file) in [
        (lcr_all(), "arf_210_1a_items.txt"),
        (lcr_aud(), "arf_210_1b_items.txt"),
        (mlh(), "arf_210_2_items.txt"),
        (nsfr(), "arf_210_6_items.txt"),
    ] {
        let mut want: Vec<String> =
            std::fs::read_to_string(dir.join(file)).unwrap().split_whitespace().map(String::from).collect();
        let mut got: Vec<String> = form.items.iter().map(|i| i.id.clone()).collect();
        want.sort_by_key(|i| key(i));
        got.sort_by_key(|i| key(i));
        assert_eq!(got, want, "{}", form.code);
    }
}

#[test]
fn all_zero_returns_are_clean() {
    for f in arf210::forms() {
        let ev = evaluate(f, std::iter::empty::<(&str, Q)>(), Missing::Zero).unwrap();
        assert!(ev.ok(), "{}: {:?}", f.code, ev.findings);
    }
}

#[test]
fn cap_fractions_are_exact() {
    let text = lcr_all().describe();
    assert!(text.contains("15 / 85"), "15/85 must stay an exact fraction");
}

// ------------------------------------------------------------ LCR

#[test]
fn caps_and_inflow_cap_end_to_end() {
    let ev = run(lcr_all(), &[
        ("1.1", "60"), ("2.1", "50"), ("3.1", "40"),  // HQLA1 60, HQLA2A 42.5 wtd, RMBS 30 wtd
        ("9.2.6", "250"),                             // non-op corporate, uninsured: 40% -> 100
        ("20.3", "200"),                              // corporate inflows: 50% -> 100
        ("18.1", "250"), ("18.1:weighted", "100"),
        ("23.1", "200"), ("23.1:weighted", "100"),
        ("7.1:weighted", "100"),
    ]);
    // 15% cap: max(30 - 15/85*(60+42.5), 30 - 15/60*60, 0) = 15
    assert_eq!(ev.q("4.1:weighted"), &int(15));
    // 40% cap: max((42.5 + 30 - 15) - 2/3*60, 0) = 17.5 -- exactly, no rounding
    assert_eq!(ev.q("4.2:weighted"), &d("17.5"));
    assert_eq!(ev.q("4:weighted"), &int(100));
    assert_eq!(ev.q("28:value"), &int(75)); // 75% inflow cap binds
    assert_eq!(ev.q("29:value"), &int(25));
    assert_eq!(ev.q("30:value"), &int(400));
    assert!(ev.ok(), "{:?}", ev.findings);
}

#[test]
fn unwind_of_maturing_repo_moves_hqla_between_levels() {
    // Borrowed 50 cash (HQLA1) against HQLA2A collateral with MV 55, maturing
    // within 30 days. Unwinding: adjusted HQLA1 -50, adjusted HQLA2A +55.
    let ev = run(lcr_all(), &[
        ("1.1", "100"), ("11.2", "50"), ("11.2.1", "50"), ("11.2.2", "55"),
        ("18.1", "50"), ("18.1:weighted", "7.5"),
    ]);
    assert_eq!(ev.q("1.5.1"), &int(-50));
    assert_eq!(ev.q("1.5"), &int(50));
    assert_eq!(ev.q("2.5:weighted"), &d("46.75"));
    // 40% cap on the unwound stock: 46.75 - 2/3*50, exactly
    let cap = d("46.75") - Q::new(100.into(), 3.into());
    assert_eq!(ev.q("4.2:weighted"), &cap);
    assert_eq!(ev.q("4:weighted"), &(int(100) - cap));
}

#[test]
fn adjusted_hqla1_is_floored_at_zero() {
    let ev = run(lcr_all(), &[("1.1", "10"), ("11.1", "50"), ("11.1.1", "50")]);
    assert_eq!(ev.q("1.5"), &int(0));
}

#[test]
fn contractual_lending_net_of_implicit_rollover() {
    let ev = run(lcr_all(), &[("15.2", "30"), ("20.1", "40")]);
    assert_eq!(ev.q("15.7"), &int(20));
    assert_eq!(ev.q("15.8"), &int(10));
    assert_eq!(ev.q("15:weighted"), &int(10));
}

#[test]
fn clf_numerator_and_maturing_clf_inflows() {
    for (assets, maturing, want_63, want_212) in [("80", "15", 80, 0), ("120", "15", 80, 15), ("120", "50", 80, 40)] {
        let ev = run(lcr_all(), &[
            ("6.1.1", assets), ("6.2.1", "100"), ("11.5.1", "30"), ("19.1.5.1", "10"), ("6.4", maturing),
        ]);
        assert_eq!(ev.q("6.2.2"), &int(20));
        assert_eq!(ev.q("6.2"), &int(80));
        assert_eq!(ev.q("6.3"), &int(want_63));
        assert_eq!(ev.q("21.2"), &int(want_212));
    }
}

#[test]
fn head_office_facility_capped_at_20pct_of_outflows() {
    let ev = run(lcr_all(), &[("21.5", "100"), ("10.1", "200"), ("18.1", "200"), ("18.1:weighted", "200"),
                              ("23.1", "100"), ("23.1:weighted", "40")]);
    assert_eq!(ev.q("21.5:weighted"), &int(40));
}

#[test]
fn overlay_and_undefined_ratio() {
    let ev = run(lcr_all(), &[("1.1", "150"), ("10.1", "100"), ("29.1:value", "50"),
                              ("18.1", "100"), ("18.1:weighted", "100")]);
    assert_eq!(ev.q("29:value"), &int(150));
    assert_eq!(ev.q("30:value"), &int(100));
    let ev = run(lcr_aud(), &[("1.1", "150")]);
    assert_eq!(ev.get("30:value"), &Value::Undefined); // no outflows: LCR undefined, not a crash
}

#[test]
fn validation_findings() {
    assert_eq!(failed(&run(lcr_all(), &[("6.2.3", "-5")])), ["6.2.3>=0"]);
    let ev = run(lcr_all(), &[("10.1", "200"), ("18.1", "150"), ("18.1:weighted", "150")]);
    assert!(failed(&ev).contains(&"18 ccy amount".to_string()));
}

#[test]
fn inputs_are_strict() {
    let f = lcr_all();
    assert!(matches!(evaluate(f, [("99.9", int(1))], Missing::Zero), Err(InputError::UnknownCell(_))));
    assert!(matches!(evaluate(f, [("1.5", int(1))], Missing::Zero), Err(InputError::NotAnInput(_))));
    assert!(matches!(evaluate(f, [("1.1", int(1))], Missing::Error), Err(InputError::Missing(_))));
    let ev = run(f, &[("16.4", "100")]);
    assert_eq!(ev.q("16.4:weighted"), &int(10)); // default 10% buyback run-off
}

// ------------------------------------------------------------ MLH

#[test]
fn mlh_netting_reduces_numerator_and_denominator_by_min() {
    let ev = run(mlh(), &[("2.1", "100"), ("6.1.1", "30"), ("9.1.1", "50"),
                          ("12.1", "1000"), ("12.2", "100"), ("13.1", "30")]);
    assert_eq!(ev.q("10"), &int(20));
    assert_eq!(ev.q("11"), &int(100)); // 130 - min(50, 30)
    assert_eq!(ev.q("12.3"), &int(30));
    assert_eq!(ev.q("15"), &(int(10000) / int(900)));
}

// ------------------------------------------------------------ NSFR

#[test]
fn nsfr_end_to_end() {
    let ev = run(nsfr(), &[
        ("1:b2", "100"), ("3:b0", "200"),
        ("13.1.1", "100"), ("13.1.2", "30"), ("40.1.1", "50"), ("40.1.2", "10"),
        ("35.1:b2", "200"), ("42.1", "10"), ("48", "100"),
    ]);
    assert_eq!(ev.q("13"), &int(30));
    assert_eq!(ev.q("13:calc"), &int(0));
    assert_eq!(ev.q("40"), &int(0));
    assert_eq!(ev.q("41:calc"), &int(20));
    assert_eq!(ev.q("42:calc"), &d("8.5"));
    assert_eq!(ev.q("19:calc"), &int(290));
    assert_eq!(ev.q("47:calc"), &d("158.5"));
    assert_eq!(ev.q("55:calc"), &int(5));
    assert_eq!(ev.q("56:value"), &(int(290) / d("163.5"))); // exact
}

#[test]
fn nsfr_clf_rows_capped_by_lcr() {
    let lcr = run(lcr_all(), &[("6.1.1", "80"), ("6.2.1", "100")]);
    let ids = |n: &Evaluation| -> Vec<String> { nsfr_vs_lcr(n, &lcr).into_iter().map(|f| f.check_id).collect() };
    assert_eq!(ids(&run(nsfr(), &[("26.1:b0", "50"), ("27.1:b2", "40")])), ["210.6:26.1+27.1<=210.1A:6.3"]);
    assert_eq!(ids(&run(nsfr(), &[("26.1:b0", "50"), ("27.2.3:b2", "60")])), ["210.6:26+27<=210.1A:6.2.1"]);
    assert!(ids(&run(nsfr(), &[("26.1:b0", "50"), ("27.1:b2", "30")])).is_empty());
}
