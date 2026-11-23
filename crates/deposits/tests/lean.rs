//! The Lean model generated from the spec, and the proofs about it.
//!
//! 1. lean/Prudence/RetailSme.lean is exactly what the generator produces
//!    from lean/Prudence/Specs/Modules/RetailSme.lean (it is never edited by hand).
//! 2. The Lake project builds: the generated model and lean/Prudence/Proofs.lean
//!    (both deposit theorems), with no `sorry` and only standard axioms.
//! 3. On generated books with one deposit per holder, every deposit's measures
//!    from the generated Lean model equal the Prudence interpreter's.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

use chrono::{Datelike, Days, NaiveDate};
use deposits::testgen::Gen;
use deposits::*;
use num_traits::ToPrimitive;
use prudence_core::date::{add_months, days_from_civil};
use prudence_core::ir::{Cell, Expr};
use prudence_core::uncertain::{Mode, Uncertain};
use prudence_core::{Q, Ref};

fn lean_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean"))
}

fn lake(args: &[&str]) -> (bool, String) {
    let out = Command::new("lake").args(args).current_dir(lean_dir()).output().expect("`lake` (elan) must be installed");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success() && !text.contains("error:"), text)
}

/// Run a Lean file that imports the built project.
fn lean_file(name: &str, src: &str) -> (bool, String) {
    let (ok, out) = lake(&["build"]);
    assert!(ok, "lake build failed:\n{out}");
    let dir = std::env::temp_dir().join(format!("prudence-lean-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.lean"));
    std::fs::write(&path, src).unwrap();
    lake(&["env", "lean", path.to_str().unwrap()])
}

#[test]
fn proofs_about_the_generated_model_check() {
    let proofs = std::fs::read_to_string(lean_dir().join("Prudence/Proofs.lean")).unwrap();
    assert!(!proofs.contains("sorry") && !proofs.contains("admit") && !proofs.contains("native_decide"));
    let (ok, out) = lean_file(
        "Axioms",
        "import Prudence\n#print axioms RetailAndSmeDeposits.unknowns_are_prudent\n\
         #print axioms RetailAndSmeDeposits.sme_cliff_never_increases_stable_funding\n",
    );
    assert!(ok, "{out}");
    // Lean wraps long lines; compare with whitespace collapsed.
    let flat = out.split_whitespace().collect::<Vec<_>>().join(" ");
    for thm in ["unknowns_are_prudent", "sme_cliff_never_increases_stable_funding"] {
        let want = format!("'RetailAndSmeDeposits.{thm}' depends on axioms: [propext, Classical.choice, Quot.sound]");
        assert!(flat.contains(&want), "{thm}: {out}");
    }
}

// ---------------------------------------------------------------------- differential

fn day(d: NaiveDate) -> i64 {
    days_from_civil(i64::from(d.year()), d.month(), d.day())
}

fn ub(u: &Uncertain<bool>) -> String {
    match u {
        Uncertain::Known(b) => format!("(.known {b})"),
        Uncertain::Unknown(_) => ".unknown".into(),
    }
}

fn nsfr_factor(item: &str, bucket: &str) -> Q {
    let col = format!("f{}", &bucket[1..]);
    match arf210::nsfr().cell(&Ref::new(item, &col)) {
        Some(Cell::Derived(Expr::Const(q))) => q.clone(),
        other => panic!("no factor for {item}:{col}: {other:?}"),
    }
}

/// A Lean term evaluating both measures of `a` in a world built from `book`.
fn lean_term(book: &Book, a: &DepositAccount, mode: Mode, rd: NaiveDate, known_at: NaiveDate, cover: &Q) -> String {
    let ci = book.customers.iter().position(|c| c.id == a.customer_id).unwrap();
    let c = &book.customers[ci];
    let kind = match c.kind {
        CustomerKind::NaturalPerson => "NaturalPerson",
        CustomerKind::Smsf => "Smsf",
        CustomerKind::PersonalInvestmentEntity => "PersonalInvestmentEntity",
        CustomerKind::Sme => "Sme",
        CustomerKind::NonFinancialCorporate => "NonFinancialCorporate",
        CustomerKind::FinancialInstitution => "FinancialInstitution",
    };
    // Group ids: known groups by name, `solo` groups per customer.
    let gid = |g: &str| 1 + g.trim_start_matches('g').parse::<u64>().unwrap();
    let solo = 1000 + ci as u64;
    let mut totals: BTreeMap<u64, Q> = BTreeMap::new();
    for k in book.customers.iter().filter(|k| k.kind == CustomerKind::Sme) {
        let own: Q = book.accounts.iter().filter(|x| x.customer_id == k.id).map(|x| x.balance.amount().clone()).sum();
        if let Uncertain::Known(g) = &k.sme_group {
            *totals.entry(gid(g)).or_default() += own.clone();
        }
    }
    let own: Q = book.accounts.iter().filter(|x| x.customer_id == c.id).map(|x| x.balance.amount().clone()).sum();
    totals.insert(solo, own);
    let groups: String = totals.iter().map(|(g, t)| format!("| some {g} => {t} ")).collect();
    let sme_group = match &c.sme_group {
        Uncertain::Known(g) => format!("(.known {})", gid(g)),
        Uncertain::Unknown(_) => ".unknown".into(),
    };
    let class_id = |p: &str| if p == "TD-A" { 1 } else { 2 };
    let events: Vec<_> = book.ledger.as_of(rd, known_at).collect();
    let tainted: BTreeSet<u32> = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::EarlyWithdrawal { product_class, hardship: false, .. } => Some(class_id(product_class)),
            _ => None,
        })
        .collect();
    let mut latest: Option<(NaiveDate, NaiveDate)> = None;
    for e in &events {
        if let EventKind::NoticeGiven { account_id, repayment_date } = &e.kind
            && *account_id == a.id
            && *repayment_date >= rd
            && latest.is_none_or(|(v, _)| e.valid >= v)
        {
            latest = Some((e.valid, *repayment_date));
        }
    }
    let notice = match latest {
        Some((v, r)) => format!("some ⟨d, {}, {}, {}⟩", day(r), day(v), day(v)),
        None => "none".into(),
    };
    let contract = match &a.contract {
        Contract::Ump => ".UMP".to_string(),
        Contract::Clm { notice_days, grace_period_end } => format!(
            "(.CLM {notice_days} {})",
            grace_period_end.map(|g| format!("(some {})", day(g))).unwrap_or("none".into())
        ),
        Contract::Pam { maturity, early_withdrawal } => {
            let ew = match early_withdrawal {
                Uncertain::Known(t) => format!("(.known .{t:?})"),
                Uncertain::Unknown(_) => ".unknown".into(),
            };
            format!("(.PAM {} {ew})", day(*maturity))
        }
    };
    let rdn = day(rd);
    let m = if mode == Mode::Conservative { ".Conservative" } else { ".Optimistic" };
    format!(
        "(let c : Customer := {{ kind := .{kind}, established_relationship := {}, sme_group := {sme_group}, \
         managed_as_retail := {}, pie_qualifies := {} }}\n  \
         let d : Deposit := {{ holder := c, product_class := {}, balance := {}, fcs_eligible := {}, \
         transactional := {}, online := {}, rate_driven := {}, contract := {contract} }}\n  \
         let W : World := {{ reporting_date := {rdn}, known_at := {}, fcs_limit := 250000, sme_threshold := 2000000, \
         SmeGroups := fun _ g => ⟨match g with {groups}| _ => 0⟩, Tainted := {:?}, covered := fun _ _ => {cover}, \
         Customer_deposits := fun _ => [d], bucket := fun x => if x < {} then 0 else if x < {} then 1 else 2, \
         solo := fun _ => {solo}, latest_notice := fun _ => {notice} }}\n  \
         (measure_Arf210_1A W {m} d, measure_Arf210_6 W {m} d))",
        ub(&c.established_relationship),
        ub(&c.managed_as_retail),
        ub(&c.pie_qualifies),
        class_id(&a.product_class),
        a.balance.amount(),
        ub(&a.fcs_eligible),
        ub(&a.transactional),
        ub(&a.online),
        ub(&a.rate_driven),
        day(known_at),
        tainted.into_iter().collect::<Vec<_>>(),
        add_months(rdn, 6),
        add_months(rdn, 12),
    )
}

#[test]
fn generated_model_agrees_with_the_interpreter() {
    let rd = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
    let hundred = Q::from_integer(100.into());
    let mut g = Gen::new(0x1ea4);
    let mut guards = String::new();
    let mut n = 0;
    for _ in 0..120 {
        let mut book = g.book(rd);
        let mut seen = BTreeSet::new();
        book.accounts.retain(|a| seen.insert(a.customer_id.clone()));
        let known_at = rd + Days::new(g.next(2) * 10);
        for mode in [Mode::Conservative, Mode::Optimistic] {
            let c = classify(&book, &Params { mode, known_at, ..Params::new(rd) }).unwrap();
            let mut per: BTreeMap<&str, (Q, Q, Q)> = BTreeMap::new();
            for s in &c.slices {
                let e = per.entry(s.account_id.as_str()).or_default();
                e.0 += s.amount.amount() * &s.lcr_rate * &hundred;
                e.1 += s.amount.amount() * nsfr_factor(s.nsfr_item, s.nsfr_bucket) * &hundred;
                if s.covered {
                    e.2 += s.amount.amount().clone();
                }
            }
            for a in &book.accounts {
                let (out, asf, cover) = &per[a.id.as_str()];
                let term = lean_term(&book, a, mode, rd, known_at, cover);
                let (o, s) = (out.to_integer().to_i64().unwrap(), asf.to_integer().to_i64().unwrap());
                writeln!(guards, "#guard {term} == ({o}, {s})\n").unwrap();
                n += 1;
            }
        }
    }
    assert!(n > 500, "{n}");
    let src = format!("import Prudence.RetailSme\nopen RetailAndSmeDeposits\n\n{guards}");
    let (ok, out) = lean_file("Differential", &src);
    let failures: Vec<&str> = out.lines().filter(|l| l.contains("error")).take(5).collect();
    assert!(ok, "{n} deposits; first failures:\n{}", failures.join("\n"));
}
