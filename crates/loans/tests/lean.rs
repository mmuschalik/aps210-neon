//! The loans Lean model: generated from the spec, its proofs, and agreement
//! with the interpreter (exact rationals on both sides).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

use chrono::{Datelike, Days, NaiveDate};
use loans::*;
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
    let dir = std::env::temp_dir().join(format!("prudence-loans-lean-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.lean"));
    std::fs::write(&path, src).unwrap();
    lake(&["env", "lean", path.to_str().unwrap()])
}

fn day(d: NaiveDate) -> i64 {
    days_from_civil(i64::from(d.year()), d.month(), d.day())
}

/// A rational as a Lean `Rat` term.
fn rat(q: &Q) -> String {
    format!("(({} : Rat) / {})", q.numer(), q.denom())
}

#[test]
fn proofs_check_with_standard_axioms_only() {
    let proofs = std::fs::read_to_string(lean_dir().join("Prudence/LoansProofs.lean")).unwrap();
    assert!(!proofs.contains("sorry") && !proofs.contains("admit") && !proofs.contains("native_decide"));
    let thms = ["portions_conserve", "unknown_performing_no_inflow", "open_maturity_minimum_payment"];
    let src: String = "import Prudence\n".to_string() + &thms.iter().map(|t| format!("#print axioms Loans.{t}\n")).collect::<String>();
    let (ok, out) = lean_file("Axioms", &src);
    assert!(ok, "{out}");
    let flat = out.split_whitespace().collect::<Vec<_>>().join(" ");
    for t in thms {
        let line = flat.split(&format!("'Loans.{t}'")).nth(1).unwrap_or_default();
        let line = line.split(" '").next().unwrap_or_default();
        assert!(line.contains("depends on axioms") || line.contains("does not depend"), "{t}: {out}");
        for ax in ["sorryAx", "Lean.ofReduceBool", "Lean.trustCompiler"] {
            assert!(!line.contains(ax), "{t} uses {ax}: {out}");
        }
    }
}

fn factor_pct(item: &str, bucket: &str) -> Q {
    let col = format!("f{}", &bucket[1..]);
    match arf210::nsfr().items.iter().find(|i| i.id == item).and_then(|i| i.cell(&col)) {
        Some(Cell::Derived(Expr::Const(q))) => q * Q::from_integer(100.into()),
        other => panic!("{item}:{col}: {other:?}"),
    }
}

fn u(x: &Uncertain<bool>) -> String {
    match x {
        Uncertain::Known(b) => format!("(.known {b})"),
        Uncertain::Unknown(_) => ".unknown".into(),
    }
}

#[test]
fn generated_model_agrees_with_the_interpreter() {
    let rd = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
    let mut s: u64 = 2024;
    let mut next = |k: u64| {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 33) % k
    };
    let unk = |b: bool, n: u64| if n == 0 { Uncertain::unknown("x") } else { Uncertain::Known(b) };
    let mut guards = String::new();
    let mut n = 0;
    // The calendar: Lean's addMonths is the interpreter's add_months.
    for _ in 0..60 {
        let d = 19_000 + next(4000) as i64;
        let k = next(40) as u32;
        writeln!(guards, "#guard addMonths {d} {k} == {}", add_months(d, k)).unwrap();
    }
    for i in 0..120 {
        let balance = Q::from_integer((1000 * (1 + next(2000) as i64)).into());
        let contract = match next(4) {
            0 => Contract::Ann { payment: Q::from_integer((1000 * (1 + next(100) as i64)).into()) },
            1 => Contract::Lam { principal_per_period: Q::from_integer((1000 * (1 + next(200) as i64)).into()) },
            2 => Contract::Pam { maturity: rd + Days::new(1 + next(900)) },
            _ => Contract::Ump { minimum_payment: Q::from_integer((next(5000) as i64).into()) },
        };
        let l = Loan {
            id: format!("l{i}"),
            borrower: [Borrower::RetailCustomer, Borrower::Sme, Borrower::NonFinancialCorporate, Borrower::FinancialInstitution]
                [next(4) as usize],
            affiliation: Affiliation::External,
            product: if next(2) == 0 { Product::ResidentialMortgage } else { Product::OtherLoan },
            contract,
            balance,
            rate: Q::new((next(12) as i64).into(), 100.into()),
            next_payment: rd + Days::new(1 + next(60)),
            cycle_months: [1, 3, 6, 12][next(4) as usize],
            performing: unk(next(5) > 0, next(4)),
            lvr_at_most_80: unk(next(2) == 0, next(4)),
            risk_weight: [20, 35, 50, 100][next(4) as usize],
            encumbered_until: if next(3) == 0 { Some(rd + Days::new(next(800))) } else { None },
        };
        let contract = match &l.contract {
            Contract::Ann { payment } => format!("(.ANN {})", rat(payment)),
            Contract::Lam { principal_per_period } => format!("(.LAM {})", rat(principal_per_period)),
            Contract::Pam { maturity } => format!("(.PAM {})", day(*maturity)),
            Contract::Ump { minimum_payment } => format!("(.UMP {})", rat(minimum_payment)),
        };
        for mode in [Mode::Conservative, Mode::Optimistic] {
            let r = run(std::slice::from_ref(&l), rd, mode).unwrap();
            let inflow: Q = ["20.1", "20.2", "20.3", "20.4", "20.5", "20.6"].iter().map(|i| r.inflows[*i].clone()).sum();
            let rsf: Q = r.slices.iter().map(|(_, item, b, a)| a * factor_pct(item, b)).sum();
            let m = if mode == Mode::Conservative { ".Conservative" } else { ".Optimistic" };
            let term = format!(
                "(let l : Loan := {{ borrower := .{:?}, affiliation := .External, product := .{:?}, balance := {}, \
                 rate := {}, next_payment := {}, cycle_months := {}, performing := {}, lvr_at_most_80 := {}, \
                 risk_weight := {}, encumbered_until := {}, contract := {contract} }}\n  \
                 let W : World := {{ reporting_date := {}, foreign_adi := false, local_banking_subsidiary := false, \
                 head_office_approved := false, head_office_facility := 0 }}\n  \
                 (inflow_30d W {m} l, measure_Arf210_6 W {m} l))",
                l.borrower,
                l.product,
                rat(&l.balance),
                rat(&l.rate),
                day(l.next_payment),
                l.cycle_months,
                u(&l.performing),
                u(&l.lvr_at_most_80),
                l.risk_weight,
                l.encumbered_until.map(|d| format!("(some {})", day(d))).unwrap_or("none".into()),
                day(rd),
            );
            writeln!(guards, "#guard {term} == ({}, {})\n", rat(&inflow), rat(&rsf)).unwrap();
            n += 1;
        }
    }
    let src = format!("import Prudence.Generated.Loans\nopen Loans\n\n{guards}");
    let (ok, out) = lean_file("Differential", &src);
    let failures: Vec<&str> = out.lines().filter(|l| l.contains("error")).take(5).collect();
    assert!(ok, "{n} loans; first failures:\n{}", failures.join("\n"));
}
