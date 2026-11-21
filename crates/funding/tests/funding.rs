//! Issued debt, capital and placements into ARF 210.1A, 210.6 and 210.2.

use chrono::{Days, Months, NaiveDate};
use funding::*;
use prudence_core::Q;
use prudence_core::uncertain::{Mode, Uncertain};

fn rd() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
}
fn m(x: i64) -> Q {
    Q::from_integer((x * 1_000_000).into())
}
fn cells(b: &Book, mode: Mode) -> Vec<(String, &'static str, String, String, Q)> {
    let mut c = run(b, rd(), mode).unwrap().cells;
    c.sort();
    c
}
fn c(id: &str, form: &'static str, item: &str, col: &str, x: i64) -> (String, &'static str, String, String, Q) {
    (id.into(), form, item.into(), col.into(), m(x))
}

#[test]
fn issued_securities() {
    let b = Book {
        securities: vec![
            IssuedSecurity { id: "cd".into(), kind: SecurityKind::ShortTerm, amount: m(50), maturity: rd() + Days::new(20), offshore: false },
            IssuedSecurity { id: "ecp".into(), kind: SecurityKind::EuroCommercialPaper, amount: m(30), maturity: rd() + Days::new(10), offshore: true },
            IssuedSecurity { id: "cb".into(), kind: SecurityKind::CoveredBond, amount: m(100), maturity: rd() + Days::new(25), offshore: false },
            IssuedSecurity { id: "mtn".into(), kind: SecurityKind::LongTerm, amount: m(200), maturity: rd() + Months::new(30), offshore: false },
        ],
        ..Book::default()
    };
    assert_eq!(
        cells(&b, Mode::Conservative),
        vec![
            c("cb", "ARF 210.1A", "12.1", "amount", 100),
            c("cb", "ARF 210.6", "12", "b0", 100),
            c("cd", "ARF 210.1A", "10.1", "amount", 50),
            c("cd", "ARF 210.6", "12", "b0", 50),
            c("ecp", "ARF 210.1A", "10.2", "amount", 30),
            c("ecp", "ARF 210.6", "12", "b0", 30),
            c("mtn", "ARF 210.6", "12", "b2", 200),
        ]
    );
}

#[test]
fn capital() {
    let b = Book {
        capital: vec![
            CapitalInstrument { id: "cet1".into(), tier: Tier::CET1, amount: m(500), maturity: None },
            CapitalInstrument { id: "t2-long".into(), tier: Tier::T2, amount: m(100), maturity: Some(rd() + Months::new(36)) },
            CapitalInstrument { id: "t2-short".into(), tier: Tier::T2, amount: m(40), maturity: Some(rd() + Days::new(20)) },
        ],
        ..Book::default()
    };
    assert_eq!(
        cells(&b, Mode::Conservative),
        vec![
            c("cet1", "ARF 210.6", "1", "b2", 500),
            c("t2-long", "ARF 210.6", "1", "b2", 100),
            // Tier 2 maturing within a year is term funding; within 30 days an outflow.
            c("t2-short", "ARF 210.1A", "10.1", "amount", 40),
            c("t2-short", "ARF 210.6", "12", "b0", 40),
        ]
    );
}

#[test]
fn placements_and_unknown_operational_status() {
    let p = |id: &str, maturity, op| Placement { id: id.into(), adi: AdiType::MajorBank, amount: m(10), maturity, operational: op };
    let b = Book {
        placements: vec![
            p("call", None, Uncertain::Known(false)),
            p("term", Some(rd() + Months::new(9)), Uncertain::Known(false)),
            p("ops", None, Uncertain::Known(true)),
            p("unknown", None, Uncertain::unknown("no assessment")),
        ],
        ..Book::default()
    };
    let got = cells(&b, Mode::Conservative);
    assert!(got.contains(&c("call", "ARF 210.1A", "20.5", "amount", 10)));
    assert!(got.contains(&c("call", "ARF 210.2", "6.1.1", "amount", 10)));
    assert!(got.contains(&c("term", "ARF 210.6", "24.1", "b1", 10)));
    assert!(got.contains(&c("term", "ARF 210.2", "6.2.1", "amount", 10)));
    assert!(!got.iter().any(|x| x.0 == "term" && x.1 == "ARF 210.1A"), "beyond 30 days: no inflow");
    assert!(got.contains(&c("ops", "ARF 210.6", "33.1", "b0", 10)));
    // Unknown: treated as operational - no inflow, 50% RSF; optimistic: inflow.
    assert!(!got.iter().any(|x| x.0 == "unknown" && x.1 == "ARF 210.1A"));
    assert!(got.contains(&c("unknown", "ARF 210.6", "33.1", "b0", 10)));
    assert!(cells(&b, Mode::Optimistic).contains(&c("unknown", "ARF 210.1A", "20.5", "amount", 10)));
}

#[test]
fn module_proof_obligations_hold() {
    for o in prudence_core::syntax::prove::prove_module(module()).unwrap() {
        assert!(o.proven, "{} NOT proven", o.obligation);
    }
}
