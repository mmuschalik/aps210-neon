//! The wholesale Lean model: generated from the spec, its proofs, and agreement
//! with the interpreter.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

use chrono::{Datelike, Days, NaiveDate};
use deposits::wholesale::*;
use num_traits::ToPrimitive;
use prudence_core::Q;
use prudence_core::date::{add_months, days_from_civil};
use prudence_core::ir::{Cell, Expr};
use prudence_core::uncertain::{Mode, Uncertain};

fn lean_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean"))
}

fn lake(args: &[&str]) -> (bool, String) {
    let out = Command::new("lake").args(args).current_dir(lean_dir()).output().expect("`lake` (elan) must be installed");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success() && !text.contains("error:"), text)
}

fn lean_file(name: &str, src: &str) -> (bool, String) {
    let (ok, out) = lake(&["build"]);
    assert!(ok, "lake build failed:\n{out}");
    let dir = std::env::temp_dir().join(format!("prudence-wholesale-lean-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.lean"));
    std::fs::write(&path, src).unwrap();
    lake(&["env", "lean", path.to_str().unwrap()])
}

#[test]
fn proofs_check_with_standard_axioms_only() {
    let proofs = std::fs::read_to_string(lean_dir().join("Prudence/WholesaleProofs.lean")).unwrap();
    assert!(!proofs.contains("sorry") && !proofs.contains("admit") && !proofs.contains("native_decide"));
    let thms = ["portions_conserve", "unknown_service_is_excess", "unknown_balance_is_excess"];
    let src: String =
        "import Prudence\n".to_string() + &thms.iter().map(|t| format!("#print axioms WholesaleDeposits.{t}\n")).collect::<String>();
    let (ok, out) = lean_file("Axioms", &src);
    assert!(ok, "{out}");
    let flat = out.split_whitespace().collect::<Vec<_>>().join(" ");
    for t in thms {
        let std = ["[propext]", "[propext, Quot.sound]", "[propext, Classical.choice, Quot.sound]"];
        assert!(
            std.iter().any(|a| flat.contains(&format!("'WholesaleDeposits.{t}' depends on axioms: {a}")))
                || flat.contains(&format!("'WholesaleDeposits.{t}' does not depend on any axioms")),
            "{t}: {out}"
        );
    }
}

fn pct(form: &prudence_core::Form, item: &str, col: &str) -> i64 {
    match form.items.iter().find(|i| i.id == item).and_then(|i| i.cell(col)) {
        Some(Cell::Derived(Expr::Const(q))) => (q * Q::from_integer(100.into())).to_integer().to_i64().unwrap(),
        _ => 0,
    }
}

fn u<T>(x: &Uncertain<T>, f: impl Fn(&T) -> String) -> String {
    match x {
        Uncertain::Known(v) => format!("(.known {})", f(v)),
        Uncertain::Unknown(_) => ".unknown".into(),
    }
}

/// One deposit per customer: the Lean measures equal the interpreter's
/// slices weighted by the forms, in both modes.
#[test]
fn generated_model_agrees_with_the_interpreter() {
    let rd = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
    let kinds = [
        Kind::NonFinancialCorporate,
        Kind::SovereignCentralBankPseMdb,
        Kind::Bank,
        Kind::OtherFinancialInstitution,
        Kind::OtherLegalEntity,
    ];
    let mut s: u64 = 99;
    let mut next = |k: u64| {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 33) % k
    };
    let factors = arf210::nsfr_factors().unwrap();
    let mut guards = String::new();
    let mut n = 0;
    let rd_days = days_from_civil(i64::from(rd.year()), rd.month(), rd.day());
    let m6 = add_months(rd_days, 6);
    let m12 = add_months(rd_days, 12);
    for i in 0..150 {
        let kind = kinds[next(5) as usize];
        let bal = 1000 * (1 + next(900) as i64);
        let ub = |k: u64| if k == 0 { Uncertain::unknown("x") } else { Uncertain::Known(k == 1) };
        let maturity = match next(4) {
            0 => None,
            1 => Some(rd + Days::new(90)),
            2 => Some(rd + Days::new(250)),
            _ => Some(rd + Days::new(500)),
        };
        let mat_lean = match maturity {
            None => "none".to_string(),
            Some(dt) => format!("(some {})", days_from_civil(i64::from(dt.year()), dt.month(), dt.day())),
        };
        let d = Deposit {
            id: format!("d{i}"),
            customer_id: format!("c{i}"),
            balance: Q::from_integer(bal.into()),
            within_30_days: next(4) > 0,
            called: next(5) == 0,
            fcs_eligible: ub(next(3)),
            operational_service: ub(next(3)),
            operational_balance: if next(3) == 0 {
                Uncertain::unknown("x")
            } else {
                Uncertain::Known(Q::from_integer((1000 * next(1000) as i64).into()))
            },
            maturity,
        };
        let c = Customer { id: d.customer_id.clone(), kind };
        for mode in [Mode::Conservative, Mode::Optimistic] {
            let slices = run(std::slice::from_ref(&c), std::slice::from_ref(&d), rd, mode).unwrap();
            let (mut lcr, mut asf, mut cover) = (0i64, 0i64, 0i64);
            for x in &slices {
                let a = x.amount.to_integer().to_i64().unwrap();
                if let Some(item) = &x.lcr_item {
                    lcr += a * pct(arf210::lcr_all(), item, "weight");
                }
                let idx: u32 = if x.nsfr.1 == "amount" { 9 } else { x.nsfr.1[1..].parse().unwrap() };
                asf += a * factors.get(&(x.nsfr.0.clone(), idx)).copied().unwrap_or(0);
            }
            // The insured portion, as allocated by the interpreter's LP.
            let (_, allocs) = run_with_certificates(std::slice::from_ref(&c), std::slice::from_ref(&d), rd, mode).unwrap();
            for a in &allocs {
                cover += a.solution.x.iter().map(|q| q.to_integer().to_i64().unwrap()).sum::<i64>();
            }
            let m = if mode == Mode::Conservative { ".Conservative" } else { ".Optimistic" };
            let term = format!(
                "(let c : Customer := {{ kind := .{kind:?} }}\n  \
                 let d : Deposit := {{ holder := c, balance := {bal}, within_30_days := {}, called := {}, \
                 fcs_eligible := {}, operational_service := {}, operational_balance := {}, maturity := {mat_lean} }}\n  \
                 let W : World := {{ reporting_date := {rd_days}, fcs_limit := 250000, insured := fun _ _ => {cover}, \
                 Customer_deposits := fun _ => [d], m6 := {m6}, m12 := {m12} }}\n  \
                 (measure_Arf210_1A W {m} d, measure_Arf210_6 W {m} d))",
                d.within_30_days,
                d.called,
                u(&d.fcs_eligible, |b| b.to_string()),
                u(&d.operational_service, |b| b.to_string()),
                u(&d.operational_balance, |q| q.to_string()),
            );
            writeln!(guards, "#guard {term} == ({lcr}, {asf})\n").unwrap();
            n += 1;
        }
    }
    let src = format!("import Prudence.Wholesale\nopen WholesaleDeposits\n\n{guards}");
    let (ok, out) = lean_file("Differential", &src);
    let failures: Vec<&str> = out.lines().filter(|l| l.contains("error")).take(5).collect();
    assert!(ok, "{n} deposits; first failures:\n{}", failures.join("\n"));
}
