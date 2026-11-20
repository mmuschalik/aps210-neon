//! Loans: ACTUS schedules -> 30-day inflows and NSFR maturity buckets.

use chrono::{Days, Months, NaiveDate};
use loans::*;
use prudence_core::uncertain::{Mode, Uncertain};
use prudence_core::{Missing, Q, evaluate};

fn rd() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
}
fn q(n: i64, d: i64) -> Q {
    Q::new(n.into(), d.into())
}
fn aud(x: i64) -> Q {
    Q::from_integer(x.into())
}

fn loan(id: &str, contract: Contract, balance: i64) -> Loan {
    Loan {
        id: id.into(),
        borrower: Borrower::RetailCustomer,
        affiliation: Affiliation::External,
        product: Product::ResidentialMortgage,
        contract,
        balance: aud(balance),
        rate: q(6, 100),
        next_payment: rd() + Days::new(10),
        cycle_months: 1,
        performing: Uncertain::Known(true),
        lvr_at_most_80: Uncertain::Known(true),
        risk_weight: 35,
        encumbered_until: None,
    }
}

fn slices(r: &Run, id: &str) -> Vec<(String, String, Q)> {
    r.slices.iter().filter(|s| s.0 == id).map(|s| (s.1.clone(), s.2.clone(), s.3.clone())).collect()
}

/// Linear amortizer, 1,200,000 at 6%, 100,000 principal a month from 10 Oct:
/// only 10 Oct is within 30 days (10 Nov is 41 days out): 100,000 + 6,000
/// interest. NSFR: payments on
/// 10 Oct .. 10 Mar (6) are < 6 months, 10 Apr .. 10 Sep (6) are 6-12 months.
#[test]
fn linear_amortizer() {
    let l = loan("lam", Contract::Lam { principal_per_period: aud(100_000) }, 1_200_000);
    let r = run(&[l], rd(), Mode::Conservative).unwrap();
    assert_eq!(r.inflows["20.1"], aud(106_000));
    assert_eq!(
        slices(&r, "lam"),
        vec![("35.1".into(), "b0".into(), aud(600_000)), ("35.1".into(), "b1".into(), aud(600_000))]
    );
}

/// An annuity's principal portions follow from the level payment exactly
/// (rationals, no rounding): balance after n payments
/// B g^n - A (g^n - 1) / i, with g = 1 + i.
#[test]
fn annuity_schedule_is_exact() {
    let mut l = loan("ann", Contract::Ann { payment: aud(10_000) }, 500_000);
    l.next_payment = rd() + Days::new(1); // 1 Oct; 1 Nov is 32 days out
    let i = q(6, 100) / aud(12);
    let r = run(std::slice::from_ref(&l), rd(), Mode::Conservative).unwrap();
    assert_eq!(r.inflows["20.1"], aud(10_000));
    // Principal before 31 Mar 2027: payments 1 Oct .. 1 Mar (6).
    let mut b = aud(500_000);
    for _ in 0..6 {
        b = &b + &b * &i - aud(10_000);
    }
    let s = slices(&r, "ann");
    assert_eq!(s[0], ("35.1".into(), "b0".into(), aud(500_000) - &b));
    let total: Q = s.iter().map(|x| x.2.clone()).sum();
    assert_eq!(total, aud(500_000), "the split conserves the balance");
    // At a month end a 30-day window can catch two monthly payments:
    // from 31 Jan, 1 Feb and 1 Mar (29 days) are both due.
    let jan31 = NaiveDate::from_ymd_opt(2027, 1, 31).unwrap();
    l.next_payment = jan31 + Days::new(1);
    let r = run(std::slice::from_ref(&l), jan31, Mode::Conservative).unwrap();
    assert_eq!(r.inflows["20.1"], aud(20_000));
}

/// Negative amortization: the payment is below the interest, so the
/// balance grows. All principal is beyond a year; the payment still flows in.
#[test]
fn negative_amortization() {
    let l = loan("neg", Contract::Ann { payment: aud(1_000) }, 1_000_000); // interest 5,000 a month
    let r = run(&[l], rd(), Mode::Conservative).unwrap();
    assert_eq!(r.inflows["20.1"], aud(1_000));
    assert_eq!(slices(&r, "neg"), vec![("35.1".into(), "b2".into(), aud(1_000_000))]);
}

/// Bullet loan to a corporate maturing in 20 days: principal and interest
/// both flow in; to an FI the inflow is 100% weighted and RSF is item 24.
#[test]
fn bullet_loans_and_counterparties() {
    let mut corp = loan("corp", Contract::Pam { maturity: rd() + Days::new(20) }, 1_000_000);
    corp.borrower = Borrower::NonFinancialCorporate;
    corp.product = Product::OtherLoan;
    corp.risk_weight = 100;
    corp.cycle_months = 3;
    corp.next_payment = rd() + Days::new(20);
    let mut fi = loan("fi", Contract::Pam { maturity: rd() + Months::new(9) }, 2_000_000);
    fi.borrower = Borrower::FinancialInstitution;
    fi.product = Product::OtherLoan;
    fi.next_payment = rd() + Days::new(5);
    fi.cycle_months = 3;
    let r = run(&[corp, fi], rd(), Mode::Conservative).unwrap();
    // corp: 1,000,000 + 1,000,000 x 6% x 3/12 = 1,015,000
    assert_eq!(r.inflows["20.3"], aud(1_015_000));
    // fi: one quarterly coupon of 30,000 on 5 Oct
    assert_eq!(r.inflows["20.5"], aud(30_000));
    assert_eq!(slices(&r, "corp"), vec![("37.1".into(), "b0".into(), aud(1_000_000))]);
    assert_eq!(slices(&r, "fi"), vec![("24.1".into(), "b1".into(), aud(2_000_000))]);
    let (lcr, nsfr) = arf_inputs(&r);
    let ev = evaluate(arf210::lcr_all(), lcr.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    // 1.015 x 50% + 0.03 x 100%
    assert_eq!(ev.q("20:weighted"), &q(5375, 10_000));
    let ev = evaluate(arf210::nsfr(), nsfr.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    // corp 1.0 x 50% + fi 2.0 x 50%
    assert_eq!(ev.q("37.1:calc") + ev.q("24.1:calc"), q(15, 10));
}

/// Revolving credit: only the minimum payment flows in; the balance is
/// open maturity (>= 1 year).
#[test]
fn open_maturity_loans() {
    let mut card = loan("card", Contract::Ump { minimum_payment: aud(300) }, 9_000);
    card.product = Product::OtherLoan;
    card.risk_weight = 75;
    let r = run(&[card], rd(), Mode::Conservative).unwrap();
    assert_eq!(r.inflows["20.1"], aud(300));
    assert_eq!(slices(&r, "card"), vec![("37.1".into(), "b2".into(), aud(9_000))]);
}

/// Unknown performance status: no inflow and 100% RSF (item 39), logged.
/// Unknown LVR: not a standard mortgage (item 37, 85% beyond a year).
/// Encumbered for 2 years: item 35.2.2 (100%); for 8 months: 35.2.1; for
/// 3 months: as if unencumbered (Att C para 23).
#[test]
fn unknowns_and_encumbrance() {
    let mut a = loan("a", Contract::Lam { principal_per_period: aud(100_000) }, 1_200_000);
    a.performing = Uncertain::unknown("arrears feed missing");
    let mut b = loan("b", Contract::Lam { principal_per_period: aud(100_000) }, 1_200_000);
    b.lvr_at_most_80 = Uncertain::unknown("no valuation");
    b.risk_weight = 50;
    let mut c = loan("c", Contract::Lam { principal_per_period: aud(100_000) }, 1_200_000);
    c.encumbered_until = Some(rd() + Months::new(24));
    let mut d = c.clone();
    d.id = "d".into();
    d.encumbered_until = Some(rd() + Months::new(8));
    let mut e = c.clone();
    e.id = "e".into();
    e.encumbered_until = Some(rd() + Months::new(3));
    let r = run(&[a.clone(), b, c, d, e], rd(), Mode::Conservative).unwrap();
    assert_eq!(r.inflows["20.1"], aud(4 * 106_000)); // all but a
    assert!(slices(&r, "d").iter().all(|s| s.0 == "35.2.1"));
    assert!(slices(&r, "e").iter().all(|s| s.0 == "35.1"));
    assert!(slices(&r, "a").iter().all(|s| s.0 == "39"));
    assert!(slices(&r, "b").iter().all(|s| s.0 == "37.1"));
    assert!(slices(&r, "c").iter().all(|s| s.0 == "35.2.2"));
    assert!(r.assumptions.contains_key(&("a".to_string(), "performing".to_string())));
    let r = run(&[a], rd(), Mode::Optimistic).unwrap();
    assert_eq!(r.inflows["20.1"], aud(106_000));
}

/// Over random books: the NSFR split conserves each balance; missing data
/// never raises inflows nor lowers required stable funding.
#[test]
fn conservation_and_prudence() {
    let mut s: u64 = 11;
    let mut next = |k: u64| {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 33) % k
    };
    let unk = |b: bool, n: u64| if n == 0 { Uncertain::unknown("x") } else { Uncertain::Known(b) };
    for _ in 0..30 {
        let loans: Vec<Loan> = (0..1 + next(6))
            .map(|i| {
                let balance = aud(1000 * (1 + next(2000) as i64));
                let contract = match next(4) {
                    0 => Contract::Ann { payment: aud(1000 * (1 + next(100) as i64)) },
                    1 => Contract::Lam { principal_per_period: aud(1000 * (1 + next(200) as i64)) },
                    2 => Contract::Pam { maturity: rd() + Days::new(1 + next(900)) },
                    _ => Contract::Ump { minimum_payment: aud(next(5000) as i64) },
                };
                Loan {
                    id: format!("l{i}"),
                    borrower: [Borrower::RetailCustomer, Borrower::Sme, Borrower::FinancialInstitution][next(3) as usize],
                    affiliation: Affiliation::External,
                    product: if next(2) == 0 { Product::ResidentialMortgage } else { Product::OtherLoan },
                    contract,
                    balance,
                    rate: q(next(12) as i64, 100),
                    next_payment: rd() + Days::new(1 + next(60)),
                    cycle_months: [1, 3, 6, 12][next(4) as usize],
                    performing: unk(next(5) > 0, next(4)),
                    lvr_at_most_80: unk(next(2) == 0, next(4)),
                    risk_weight: [20, 35, 50, 100][next(4) as usize] as u32,
                    encumbered_until: if next(3) == 0 { Some(rd() + Days::new(next(800))) } else { None },
                }
            })
            .collect();
        let cons = run(&loans, rd(), Mode::Conservative).unwrap();
        let opt = run(&loans, rd(), Mode::Optimistic).unwrap();
        for l in &loans {
            let total: Q = slices(&cons, &l.id).iter().map(|x| x.2.clone()).sum();
            assert_eq!(total, l.balance, "{}", l.id);
        }
        let inflow = |r: &Run| -> Q { r.inflows.values().cloned().sum() };
        assert!(inflow(&cons) <= inflow(&opt));
        let rsf = |r: &Run| {
            let (_, nsfr) = arf_inputs(r);
            let ev = evaluate(arf210::nsfr(), nsfr.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
            ["24.1", "24.2.1", "24.2.2", "24.2.3", "35.1", "35.2.1", "35.2.2", "36.1", "36.2.1", "36.2.2", "37.1", "37.2.1", "37.2.2", "39"]
                .iter()
                .map(|i| ev.q(&format!("{i}:calc")).clone())
                .sum::<Q>()
        };
        assert!(rsf(&cons) >= rsf(&opt));
    }
}

#[test]
fn module_proof_obligations_hold() {
    for o in prudence_core::syntax::prove::prove_module(module()).unwrap() {
        assert!(o.proven, "{} NOT proven", o.obligation);
    }
}

/// Intra-group inflows are "of which" rows: reported in their counterparty
/// row and again in 20.7-20.9, but counted once in the totals.
#[test]
fn intra_group_of_which_rows() {
    let mut bank = loan("sub-bank", Contract::Pam { maturity: rd() + Days::new(10) }, 1_000_000);
    bank.borrower = Borrower::FinancialInstitution;
    bank.affiliation = Affiliation::IntraGroupBank;
    bank.next_payment = rd() + Days::new(10);
    bank.cycle_months = 12;
    let mut other = bank.clone();
    other.id = "sub-corp".into();
    other.borrower = Borrower::NonFinancialCorporate;
    other.affiliation = Affiliation::IntraGroupOther;
    let r = run(&[bank, other], rd(), Mode::Conservative).unwrap();
    assert_eq!(r.inflows["20.7"], r.inflows["20.5"]);
    assert_eq!(r.inflows["20.9"], r.inflows["20.3"]);
    assert_eq!(r.inflows["20.8"], Q::default());
    let (lcr, _) = arf_inputs(&r);
    let ev = evaluate(arf210::lcr_all(), lcr.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    assert_eq!(ev.q("20"), &(ev.q("20.5") + ev.q("20.3")), "of which rows are not added again");
    assert!(ev.findings.iter().all(|f| !f.check_id.starts_with("20.7")), "{:?}", ev.findings);
}

/// Item 21.4: only contractual, non-contingent financial receivables due
/// within 30 days. Item 21.5: the head office facility, only for an approved
/// foreign ADI without a local banking subsidiary, capped by the form at 20%
/// of total outflows.
#[test]
fn other_contractual_inflows_and_head_office_support() {
    let rec = |id: &str, days: u64| Receivable {
        id: id.into(),
        amount: aud(1_000_000),
        due: rd() + Days::new(days),
        non_financial_revenue: false,
        contingent: false,
        performing: Uncertain::Known(true),
    };
    let mut rent = rec("rent", 10);
    rent.non_financial_revenue = true;
    let mut guarantee = rec("called-guarantee", 10);
    guarantee.contingent = true;
    let mut unknown = rec("doubtful", 10);
    unknown.performing = Uncertain::unknown("no arrears data");
    let other = Other {
        receivables: vec![rec("settlement", 2), rec("later", 45), rent, guarantee, unknown],
        head_office: HeadOffice { foreign_adi: true, local_banking_subsidiary: false, approved: true, facility: aud(500_000_000) },
    };
    let r = run_all(&[], &other, rd(), Mode::Conservative).unwrap();
    assert_eq!(r.inflows["21.4"], aud(1_000_000));
    assert_eq!(r.inflows["21.5"], aud(500_000_000));
    let (mut lcr, _) = arf_inputs(&r);
    lcr.insert("10.1".into(), Q::from_integer(1000.into())); // 1,000m outflows at 100%
    let ev = evaluate(arf210::lcr_all(), lcr.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    assert_eq!(ev.q("21.5:weighted"), &Q::from_integer(200.into())); // 20% of 1,000m
    // Not approved, or part of a group with a local banking subsidiary: nothing.
    for ho in [
        HeadOffice { approved: false, ..other.head_office.clone() },
        HeadOffice { local_banking_subsidiary: true, ..other.head_office.clone() },
    ] {
        let r = run_all(&[], &Other { head_office: ho, ..other.clone() }, rd(), Mode::Conservative).unwrap();
        assert_eq!(r.inflows["21.5"], Q::default());
    }
}
