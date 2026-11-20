//! The 30-day unwind (design.txt §4.A.4): proven in Lean, checked at run time.

use std::path::PathBuf;
use std::process::Command;

use chrono::{Days, NaiveDate};
use num_traits::FromPrimitive;
use prudence_core::uncertain::{Mode, Uncertain};
use prudence_core::{Missing, Q, evaluate};
use secured::*;

fn rd() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
}

fn portfolio(seed: u64, n: usize) -> Vec<Repo> {
    let mut s = seed;
    let mut next = |k: u64| {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 33) % k
    };
    (0..n)
        .map(|i| Repo {
            id: format!("r{i}"),
            leg: if next(2) == 0 { Leg::Funding } else { Leg::Lending },
            cash: Q::from_u64(next(1_000_000)).unwrap(),
            collateral_class: HqlaClass::ALL[next(5) as usize],
            collateral_value: Q::from_u64(next(1_200_000)).unwrap(),
            counterparty: [Counterparty::OffshoreCentralBank, Counterparty::DomesticSovereignOrPse, Counterparty::Other]
                [next(3) as usize],
            operational: [Uncertain::Known(true), Uncertain::Known(false), Uncertain::unknown("custody feed missing")]
                [next(3) as usize]
                .clone(),
            rehypothecated: next(4) == 0,
            maturity: rd() + Days::new(next(60)),
        })
        .collect()
}

/// At run time, on every portfolio: the form's adjusted-HQLA cells, computed
/// from the reported items by the ARF 210.1A spec, equal the interpreter's
/// mechanical unwind (the checker side of the Lean theorem).
#[test]
fn form_adjustments_equal_the_mechanical_unwind() {
    let million = Q::from_integer(1_000_000.into());
    for seed in 0..60 {
        for mode in [Mode::Conservative, Mode::Optimistic] {
            let repos = portfolio(seed, 1 + (seed as usize % 9));
            let run = run(&repos, rd(), mode).unwrap();
            let inputs = arf_inputs(&run);
            let ev = evaluate(arf210::lcr_all(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
            for (item, class) in [("1.5.1", HqlaClass::Hqla1), ("2.5.1", HqlaClass::Hqla2a), ("3.4.1", HqlaClass::RmbsHqla2b), ("3.5.1", HqlaClass::NonRmbsHqla2b)] {
                assert_eq!(ev.q(item), &(&run.unwind[&class] / &million), "seed {seed} {mode:?} item {item}");
            }
            assert!(run.unwind[&HqlaClass::NonHqla] == Q::default());
        }
    }
}

#[test]
fn a_maturing_repo_reports_and_unwinds() {
    // Borrowed 50m cash against HQLA2A worth 55m, maturing in 10 days.
    let r = Repo {
        id: "r".into(),
        leg: Leg::Funding,
        cash: Q::from_u64(50_000_000).unwrap(),
        collateral_class: HqlaClass::Hqla2a,
        collateral_value: Q::from_u64(55_000_000).unwrap(),
        counterparty: Counterparty::Other,
        operational: Uncertain::Known(true),
        rehypothecated: false,
        maturity: rd() + Days::new(10),
    };
    let run = run(std::slice::from_ref(&r), rd(), Mode::Conservative).unwrap();
    let m = |x: u64| Q::from_u64(x * 1_000_000).unwrap();
    assert_eq!(run.items["11.2"], m(50));
    assert_eq!(run.items["11.2.1"], m(50));
    assert_eq!(run.items["11.2.2"], m(55));
    assert_eq!(run.unwind[&HqlaClass::Hqla1], -m(50));
    assert_eq!(run.unwind[&HqlaClass::Hqla2a], m(55));
    // Unknown operational status: conservatively the collateral does not qualify.
    let unknown = Repo { operational: Uncertain::unknown("no custody link"), ..r.clone() };
    let run = secured::run(&[unknown], rd(), Mode::Conservative).unwrap();
    assert_eq!(run.items["11.2.1"], Q::default());
    assert_eq!(run.unwind[&HqlaClass::Hqla1], Q::default());
    assert!(run.assumptions.contains_key(&("r".to_string(), "operational".to_string())));
    // Maturing after 30 days: not reported, not unwound.
    let later = Repo { maturity: rd() + Days::new(31), ..r };
    let run = secured::run(&[later], rd(), Mode::Conservative).unwrap();
    assert!(run.items.values().all(|v| *v == Q::default()));
}

#[test]
fn module_proof_obligations_hold() {
    for o in prudence_core::syntax::prove::prove_module(module()).unwrap() {
        assert!(o.proven, "{} NOT proven:\n{}", o.obligation, o.counterexample);
    }
}

fn lean_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean"))
}

#[test]
fn generated_lean_is_up_to_date() {
    let have = std::fs::read_to_string(lean_dir().join("Prudence/SecuredFunding.lean")).unwrap();
    assert!(have == lean_source().unwrap(), "stale: run `cargo run -p secured --bin secured-lean`");
}

#[test]
fn unwind_theorem_checks_in_lean() {
    let proofs = std::fs::read_to_string(lean_dir().join("Prudence/UnwindProofs.lean")).unwrap();
    assert!(!proofs.contains("sorry") && !proofs.contains("native_decide"));
    let out = Command::new("lake").arg("build").current_dir(lean_dir()).output().expect("lake");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    let dir = std::env::temp_dir().join(format!("prudence-unwind-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("Ax.lean");
    std::fs::write(&f, "import Prudence\n#print axioms SecuredFunding.unwind_portfolio\n").unwrap();
    let out = Command::new("lake").args(["env", "lean", f.to_str().unwrap()]).current_dir(lean_dir()).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(text.contains("depends on axioms: [propext") && !text.contains("sorryAx"), "{text}");
}
