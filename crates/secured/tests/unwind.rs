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
            collateral_class: HqlaClass::ALL[next(6) as usize],
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
            let run = run(&[], &repos, rd(), mode, seed % 2 == 0).unwrap();
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
    let run = run(&[], std::slice::from_ref(&r), rd(), Mode::Conservative, false).unwrap();
    let m = |x: u64| Q::from_u64(x * 1_000_000).unwrap();
    assert_eq!(run.items["11.2"], m(50));
    assert_eq!(run.items["11.2.1"], m(50));
    assert_eq!(run.items["11.2.2"], m(55));
    assert_eq!(run.unwind[&HqlaClass::Hqla1], -m(50));
    assert_eq!(run.unwind[&HqlaClass::Hqla2a], m(55));
    // Unknown operational status: conservatively the collateral does not qualify.
    let unknown = Repo { operational: Uncertain::unknown("no custody link"), ..r.clone() };
    let run = secured::run(&[], &[unknown], rd(), Mode::Conservative, false).unwrap();
    assert_eq!(run.items["11.2.1"], Q::default());
    assert_eq!(run.unwind[&HqlaClass::Hqla1], Q::default());
    assert!(run.assumptions.contains_key(&("r".to_string(), "operational".to_string())));
    // Maturing after 30 days: not reported, not unwound.
    let later = Repo { maturity: rd() + Days::new(31), ..r };
    let run = secured::run(&[], &[later], rd(), Mode::Conservative, false).unwrap();
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

fn holding(id: &str, kind: AssetKind, issuer: Issuer, rw: i64, grade: i64, value_m: u64) -> Holding {
    Holding {
        id: id.into(),
        kind,
        issuer,
        market_value: Q::from_u64(value_m * 1_000_000).unwrap(),
        risk_weight: rw,
        rating_grade: grade,
        domestic_currency: true,
        index_constituent: true,
        own_or_associated: false,
        encumbered: false,
        operational: Uncertain::Known(true),
        drawable_in_stress: Uncertain::Known(true),
        clf_category: ClfCategory::NotClfEligible,
        value_after_rba_margin: Q::from_u64(value_m * 1_000_000).unwrap(),
        maturity: None,
    }
}

/// The complete LCR numerator from raw holdings and transactions: Section A
/// stock, the unwind of a maturing repo, and the 15% / 40% caps.
#[test]
fn lcr_numerator_from_raw_holdings_and_repos() {
    let holdings = vec![
        holding("cash", AssetKind::NotesAndCoin, Issuer::CentralBank, 0, 1, 10),
        holding("acgb", AssetKind::DebtSecurity, Issuer::AustralianGovernment, 0, 1, 100),
        holding("semi-pledged", AssetKind::DebtSecurity, Issuer::StateCentralBorrowingAuthority, 0, 1, 40),
        holding("corp-aa", AssetKind::DebtSecurity, Issuer::NonFinancialCorporateIssuer, 50, 1, 50),
        holding("other-bank-cb", AssetKind::CoveredBond, Issuer::FinancialInstitutionIssuer, 20, 1, 30),
        holding("own-cb", AssetKind::CoveredBond, Issuer::FinancialInstitutionIssuer, 20, 1, 25),
        holding("rmbs", AssetKind::Rmbs, Issuer::FinancialInstitutionIssuer, 20, 1, 80),
        holding("corp-bbb", AssetKind::DebtSecurity, Issuer::NonFinancialCorporateIssuer, 100, 3, 40),
    ];
    let mut holdings = holdings;
    holdings[2].encumbered = true; // pledged in the repo below
    holdings[5].own_or_associated = true; // own covered bond: never HQLA
    // Repo: borrowed 35m cash against the pledged semi (MV 40m), maturing in 5 days.
    let repo = Repo {
        id: "repo".into(),
        leg: Leg::Funding,
        cash: Q::from_u64(35_000_000).unwrap(),
        collateral_class: HqlaClass::Hqla1,
        collateral_value: Q::from_u64(40_000_000).unwrap(),
        counterparty: Counterparty::Other,
        operational: Uncertain::Known(true),
        rehypothecated: false,
        maturity: rd() + Days::new(5),
    };
    let r = run(&holdings, &[repo], rd(), Mode::Conservative, false).unwrap();
    let m = |x: u64| Q::from_u64(x).unwrap();
    let ev = evaluate(arf210::lcr_all(), arf_inputs(&r).iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    assert_eq!(ev.q("1"), &m(110)); // notes 10 + ACGB 100 (pledged semi excluded)
    assert_eq!(ev.q("2"), &m(80)); // corp AA 50 + other bank's covered bond 30 (own covered bond excluded)
    assert_eq!(ev.q("3"), &m(120)); // RMBS 80 + BBB corporate 40
    // Unwinding the repo: return 35m cash, get the 40m semi back.
    assert_eq!(ev.q("1.5"), &m(115));
    assert_eq!(ev.q("1.5:weighted"), &m(115));
    // HQLA2A 80 x 0.85 = 68; HQLA2B 80 x 0.75 + 40 x 0.5 = 80.
    assert_eq!(ev.q("2.5:weighted"), &m(68));
    assert_eq!(ev.q("3.6:weighted"), &m(80));
    // 15% cap: max(80 - 15/85 x (115 + 68), 80 - 15/60 x 115, 0) = 80 - 28.75 = 51.25
    assert_eq!(ev.q("4.1:weighted"), &(m(80) - Q::new(2875.into(), 100.into())));
    // 40% cap: max(68 + 80 - 51.25 - 2/3 x 115, 0) = 96.75 - 76.666...
    let cap40 = m(68) + m(80) - Q::new(5125.into(), 100.into()) - Q::new(230.into(), 3.into());
    assert_eq!(ev.q("4.2:weighted"), &cap40);
    let total = m(110) + m(68) + m(80) - Q::new(5125.into(), 100.into()) - cap40;
    assert_eq!(ev.q("4:weighted"), &total);
    // The only finding: this slice does not yet split outflows by currency (18.x).
    let ids: Vec<&str> = ev.findings.iter().map(|f| f.check_id.as_str()).collect();
    assert_eq!(ids, ["18 ccy amount"]); // weighted balances: HQLA1 repos run off at 0%
}

/// Central bank balances count only if drawable in stress; unknown is
/// conservatively not drawable.
#[test]
fn central_bank_balances_need_to_be_drawable() {
    let mut fcb = holding("rbnz", AssetKind::ForeignCentralBankBalance, Issuer::CentralBank, 0, 1, 20);
    let r = run(std::slice::from_ref(&fcb), &[], rd(), Mode::Conservative, false).unwrap();
    assert_eq!(r.items["1.2.2"], Q::from_u64(20_000_000).unwrap());
    fcb.drawable_in_stress = Uncertain::unknown("terms not captured");
    let r = run(std::slice::from_ref(&fcb), &[], rd(), Mode::Conservative, false).unwrap();
    assert_eq!(r.items["1.2.2"], Q::default());
    let r = run(&[fcb], &[], rd(), Mode::Optimistic, false).unwrap();
    assert_eq!(r.items["1.2.2"], Q::from_u64(20_000_000).unwrap());
}

/// The CLF: eligible non-HQLA securities after RBA margins (6.1.x), the
/// approved size adjusted for secured funding with CLF collateral (6.2), the
/// amount in the numerator (6.3), and maturing CLF securities (6.4, 21.2).
#[test]
fn clf_from_raw_holdings_and_repos() {
    let m = |x: u64| Q::from_u64(x).unwrap();
    let mut adi = holding("adi-bond", AssetKind::DebtSecurity, Issuer::FinancialInstitutionIssuer, 20, 1, 200);
    adi.clf_category = ClfCategory::AdiIssued;
    adi.value_after_rba_margin = Q::from_u64(190_000_000).unwrap();
    let mut abs = holding("self-sec", AssetKind::DebtSecurity, Issuer::FinancialInstitutionIssuer, 20, 1, 120);
    abs.clf_category = ClfCategory::SelfSecuritised;
    abs.value_after_rba_margin = Q::from_u64(110_000_000).unwrap();
    abs.maturity = Some(rd() + Days::new(20)); // matures within 30 days
    // A repo raising 50m against CLF securities worth 60m after margin.
    let repo = Repo {
        id: "clf-repo".into(),
        leg: Leg::Funding,
        cash: Q::from_u64(50_000_000).unwrap(),
        collateral_class: HqlaClass::ClfEligible,
        collateral_value: Q::from_u64(60_000_000).unwrap(),
        counterparty: Counterparty::Other,
        operational: Uncertain::Known(true),
        rehypothecated: false,
        maturity: rd() + Days::new(7),
    };
    let r = run(&[adi.clone(), abs.clone()], std::slice::from_ref(&repo), rd(), Mode::Conservative, true).unwrap();
    assert_eq!(r.unwind[&HqlaClass::Hqla1], Q::default(), "CLF repos are not part of the HQLA unwind");
    let mut inputs = arf_inputs(&r);
    inputs.insert("6.2.1".into(), m(200)); // approved CLF size
    let ev = evaluate(arf210::lcr_all(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    assert_eq!(ev.q("6.1"), &m(300)); // 190 + 110 after margins
    assert_eq!(ev.q("11.5"), &m(50));
    assert_eq!(ev.q("6.2.2"), &m(60)); // CLF securities pledged under maturing secured funding
    assert_eq!(ev.q("6.2"), &m(140)); // 200 - 60
    assert_eq!(ev.q("6.3"), &m(140)); // min(300, 140)
    assert_eq!(ev.q("6.4"), &m(110));
    assert_eq!(ev.q("21.2"), &m(110)); // min(6.4, 6.1 - 6.3 = 160)
    // Without a CLF the same securities are not liquid assets at all.
    let r = run(&[adi, abs], &[repo], rd(), Mode::Conservative, false).unwrap();
    assert!(["6.1.3", "6.1.6", "6.4", "11.5"].iter().all(|i| r.items[*i] == Q::default()));
    assert_eq!(r.items["11.6.2"], Q::from_u64(50_000_000).unwrap());
}
