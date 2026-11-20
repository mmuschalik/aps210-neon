//! Hand-worked cases, one rule each. Reporting date 30 Sep 2026.

use chrono::{Days, NaiveDate};
use deposits::*;
use prudence_core::money::AudCarrying;
use prudence_core::num::{int, parse_decimal};
use prudence_core::uncertain::{Mode, Uncertain};
use prudence_core::{Missing, Q, evaluate};

fn rd() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 30).unwrap()
}
fn days(n: u64) -> NaiveDate {
    rd() + Days::new(n)
}
fn aud(n: i64) -> AudCarrying {
    AudCarrying::new(int(n))
}
fn d(s: &str) -> Q {
    parse_decimal(s).unwrap()
}

fn person(id: &str, relationship: Option<bool>) -> Customer {
    let mut c = Customer::new(id, CustomerKind::NaturalPerson);
    if let Some(r) = relationship {
        c.established_relationship = r.into();
    }
    c
}

/// An account with every attribute known: FCS-eligible, and the given flags.
fn acct(id: &str, cust: &str, contract: Contract, bal: i64, transactional: bool, online: bool) -> DepositAccount {
    let mut a = DepositAccount::new(id, cust, contract, aud(bal));
    a.fcs_eligible = true.into();
    a.transactional = transactional.into();
    a.online = online.into();
    a.rate_driven = false.into();
    a
}

fn book(customers: Vec<Customer>, accounts: Vec<DepositAccount>) -> Book {
    Book { customers, accounts, ledger: Ledger::default() }
}

fn run(b: &Book) -> Classification {
    classify(b, &Params::new(rd())).unwrap()
}

/// (item, amount) pairs for an account, sorted.
fn items(c: &Classification, account: &str) -> Vec<(Option<&'static str>, AudCarrying)> {
    let mut v: Vec<_> = c.slices_for(account).map(|s| (s.lcr_item, s.amount.clone())).collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

// ------------------------------------------------------------------ retail basics

#[test]
fn insured_transactional_deposit_is_stable() {
    let c = run(&book(vec![person("p", Some(true))], vec![acct("a", "p", Contract::Ump, 100_000, true, false)]));
    let s: Vec<_> = c.slices_for("a").collect();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].lcr_item, Some("8.1"));
    assert_eq!(s[0].lcr_rate, d("0.05"));
    assert_eq!((s[0].nsfr_item, s[0].nsfr_bucket), ("3", "b0"));
    assert!(c.assumptions.is_empty());
}

#[test]
fn deposit_above_fcs_limit_is_split() {
    // 400k online account, relationship known: 250k insured (stable, since the
    // depositor has a relationship), 150k uninsured scoring 2 + 2 = 4 -> 25%.
    let c = run(&book(vec![person("p", Some(true))], vec![acct("a", "p", Contract::Ump, 400_000, false, true)]));
    assert_eq!(items(&c, "a"), [(Some("8.1"), aud(250_000)), (Some("8.5"), aud(150_000))]);
}

#[test]
fn smsf_is_never_stable() {
    let mut c = Customer::new("s", CustomerKind::Smsf);
    c.established_relationship = true.into();
    let r = run(&book(vec![c], vec![acct("a", "s", Contract::Ump, 100_000, true, false)]));
    let s = r.slices_for("a").next().unwrap();
    assert_eq!((s.lcr_item, s.nsfr_item), (Some("8.3"), "5")); // insured, less stable, 10%
}

#[test]
fn unqualified_pie_is_a_financial_institution() {
    let pie = Customer::new("t", CustomerKind::PersonalInvestmentEntity); // qualification unknown
    let r = run(&book(vec![pie], vec![acct("a", "t", Contract::Ump, 100_000, true, false)]));
    let s = r.slices_for("a").next().unwrap();
    assert_eq!((s.lcr_item, s.lcr_rate.clone(), s.nsfr_item), (Some("9.5.5"), int(1), "12"));
    assert_eq!(r.assumptions[0].attribute, "pie_qualifies");
}

// ------------------------------------------------------------------ missing data

#[test]
fn unknown_relationship_is_conservative_and_priced() {
    // Online, not transactional, relationship unknown.
    let b = book(vec![person("p", None)], vec![acct("a", "p", Contract::Ump, 400_000, false, true)]);
    let cons = run(&b);
    // Conservative: no relationship -> insured scores 2+1 = 3 -> 25%; uninsured 25%.
    assert_eq!(cons.lcr_outflow(), aud(100_000));
    assert_eq!(cons.assumptions[0].attribute, "established_relationship");
    // Optimistic: relationship -> 250k stable at 5% + 150k at 25% = 50k.
    let opt = classify(&b, &Params { mode: Mode::Optimistic, ..Params::new(rd()) }).unwrap();
    assert_eq!(opt.lcr_outflow(), aud(50_000));
    assert_eq!(data_quality_cost(&b, &Params::new(rd())).unwrap(), aud(50_000));
}

// ------------------------------------------------------------------ FCS allocation

#[test]
fn fcs_cover_allocation_across_accounts() {
    // One depositor, two 200k accounts, 250k of cover between them.
    //   transactional: insured 5%, uninsured 10%  (cover saves 5 points)
    //   online:        insured 5%, uninsured 25%  (cover saves 20 points)
    let b = book(
        vec![person("p", Some(true))],
        vec![acct("t", "p", Contract::Ump, 200_000, true, false), acct("o", "p", Contract::Ump, 200_000, false, true)],
    );
    // Conservative: cover goes where it helps least first.
    let c = run(&b);
    assert_eq!(items(&c, "t"), [(Some("8.1"), aud(200_000))]);
    assert_eq!(items(&c, "o"), [(Some("8.1"), aud(50_000)), (Some("8.5"), aud(150_000))]);
    assert_eq!(c.lcr_outflow(), aud(50_000)); // 10k + 2.5k + 37.5k
    // Optimised (needs sign-off): cover goes where it helps most.
    let o = classify(&b, &Params { fcs: FcsAllocation::Optimised, ..Params::new(rd()) }).unwrap();
    assert_eq!(o.lcr_outflow(), aud(27_500)); // 10k + 2.5k + 15k
}

// ------------------------------------------------------------------ SME

fn sme(id: &str, group: Option<&str>) -> Customer {
    let mut c = Customer::new(id, CustomerKind::Sme);
    c.managed_as_retail = true.into();
    c.established_relationship = true.into();
    if let Some(g) = group {
        c.sme_group = g.to_string().into();
    }
    c
}

#[test]
fn sme_group_below_2m_is_retail() {
    let c = run(&book(
        vec![sme("s1", Some("G")), sme("s2", Some("G"))],
        vec![acct("a1", "s1", Contract::Ump, 1_000_000, true, false), acct("a2", "s2", Contract::Ump, 900_000, true, false)],
    ));
    assert_eq!(items(&c, "a1"), [(Some("9.1.1"), aud(250_000)), (Some("9.1.4"), aud(750_000))]);
}

#[test]
fn sme_group_at_or_above_2m_flips_entirely_to_wholesale() {
    // Group H: 1.95m + 0.2m = 2.15m >= 2m. Each entity's whole balance is
    // wholesale. h2 alone is within the FCS limit -> "fully covered" 20%;
    // h1 is not -> 40%. The cliff: a $1 move changes the run-off on the lot.
    let c = run(&book(
        vec![sme("h1", Some("H")), sme("h2", Some("H"))],
        vec![acct("a1", "h1", Contract::Ump, 1_950_000, true, false), acct("a2", "h2", Contract::Ump, 200_000, true, false)],
    ));
    assert_eq!(items(&c, "a1"), [(Some("9.2.6"), aud(1_950_000))]);
    assert_eq!(items(&c, "a2"), [(Some("9.2.5"), aud(200_000))]);
    assert_eq!(c.slices_for("a1").next().unwrap().nsfr_item, "9");
}

#[test]
fn sme_threshold_is_strict() {
    let at = run(&book(vec![sme("s", Some("G"))], vec![acct("a", "s", Contract::Ump, 2_000_000, true, false)]));
    assert_eq!(items(&at, "a"), [(Some("9.2.6"), aud(2_000_000))]);
}

#[test]
fn unknown_sme_group_is_wholesale_unless_optimistic() {
    let b = book(vec![sme("s", None)], vec![acct("a", "s", Contract::Ump, 100_000, true, false)]);
    assert_eq!(items(&run(&b), "a"), [(Some("9.2.5"), aud(100_000))]);
    let o = classify(&b, &Params { mode: Mode::Optimistic, ..Params::new(rd()) }).unwrap();
    assert_eq!(items(&o, "a"), [(Some("9.1.1"), aud(100_000))]);
}

// ------------------------------------------------------------------ term deposits

fn td(id: &str, maturity_days: u64, ew: Option<EarlyWithdrawal>) -> DepositAccount {
    let early_withdrawal = match ew {
        Some(e) => Uncertain::Known(e),
        None => Uncertain::unknown("terms not captured"),
    };
    let mut a = acct(id, "p", Contract::Pam { maturity: days(maturity_days), early_withdrawal }, 100_000, false, false);
    a.product_class = "TD-NLR".into();
    a
}

#[test]
fn term_deposits_beyond_30_days_are_excluded_from_lcr_but_not_nsfr() {
    let c = run(&book(
        vec![person("p", Some(true))],
        vec![td("t90", 90, Some(EarlyWithdrawal::NoLegalRight)), td("t400", 400, Some(EarlyWithdrawal::SignificantPenalty)),
             td("t20", 20, Some(EarlyWithdrawal::NoLegalRight))],
    ));
    let s = |a| c.slices_for(a).next().unwrap().clone();
    assert_eq!((s("t90").lcr_item, s("t90").nsfr_bucket), (None, "b0"));
    assert_eq!((s("t400").lcr_item, s("t400").nsfr_bucket), (None, "b2")); // 100% ASF
    assert_eq!(s("t20").lcr_item, Some("8.1")); // matures inside the horizon
}

#[test]
fn unknown_early_withdrawal_terms_are_treated_as_withdrawable() {
    let c = run(&book(vec![person("p", Some(true))], vec![td("t", 90, None)]));
    let s = c.slices_for("t").next().unwrap();
    assert_eq!((s.lcr_item, s.nsfr_bucket), (Some("8.1"), "b0"));
}

#[test]
fn non_hardship_early_withdrawal_taints_the_whole_product_class() {
    let accounts = vec![td("t90", 90, Some(EarlyWithdrawal::NoLegalRight)), td("t400", 400, Some(EarlyWithdrawal::NoLegalRight))];
    let mut b = book(vec![person("p", Some(true))], accounts);
    let broken = |hardship| EventKind::EarlyWithdrawal { account_id: "other".into(), product_class: "TD-NLR".into(), hardship };

    // A hardship withdrawal does not taint (Att A para 43).
    b.ledger.record(broken(true), days(0) - Days::new(10), days(0) - Days::new(9));
    assert_eq!(run(&b).slices_for("t400").next().unwrap().lcr_item, None);

    // A non-hardship one does: both deposits become demand deposits.
    b.ledger.record(broken(false), days(0) - Days::new(10), days(0) - Days::new(9));
    let c = run(&b);
    for a in ["t90", "t400"] {
        let s = c.slices_for(a).next().unwrap();
        assert_eq!((s.lcr_item, s.nsfr_bucket), (Some("8.1"), "b0"), "{a}");
    }
}

#[test]
fn tainting_respects_both_time_axes() {
    let mut b = book(vec![person("p", Some(true))], vec![td("t", 90, Some(EarlyWithdrawal::NoLegalRight))]);
    // Happened before the reporting date, but only recorded 5 days after it.
    b.ledger.record(
        EventKind::EarlyWithdrawal { account_id: "x".into(), product_class: "TD-NLR".into(), hardship: false },
        days(0) - Days::new(2),
        days(5),
    );
    // As known on the reporting date: not tainted.
    assert_eq!(run(&b).slices_for("t").next().unwrap().lcr_item, None);
    // Replayed with later knowledge (a restatement): tainted.
    let later = Params { known_at: days(10), ..Params::new(rd()) };
    assert_eq!(classify(&b, &later).unwrap().slices_for("t").next().unwrap().lcr_item, Some("8.1"));
}

// ------------------------------------------------------------------ notice deposits

fn notice(id: &str, grace_end: Option<NaiveDate>) -> DepositAccount {
    // 50k each, so all four fit within the depositor's 250k of FCS cover.
    acct(id, "p", Contract::Clm { notice_days: 31, grace_period_end: grace_end }, 50_000, false, false)
}

#[test]
fn notice_deposits() {
    let mut b = book(
        vec![person("p", Some(true))],
        vec![notice("quiet", None), notice("called20", None), notice("called40", None), notice("grace", Some(days(3)))],
    );
    let give = |acc: &str, repay| EventKind::NoticeGiven { account_id: acc.into(), repayment_date: repay };
    b.ledger.record(give("called20", days(20)), days(0) - Days::new(11), days(0) - Days::new(11));
    b.ledger.record(give("called40", days(40)), days(0), days(0));
    let c = run(&b);
    let s = |a| c.slices_for(a).next().unwrap().clone();
    assert_eq!((s("quiet").lcr_item, s("quiet").nsfr_bucket), (None, "b0")); // 31-day notice not given
    assert_eq!((s("called20").lcr_item, s("called20").lcr_rate.clone()), (Some("8.6"), int(1)));
    assert_eq!(s("called40").lcr_item, None);
    assert_eq!(s("grace").lcr_item, Some("8.1")); // in grace period: demand
}

// ------------------------------------------------------------------ integrity

#[test]
fn unknown_sme_status_can_lower_the_lcr_outflow() {
    // Evidence-based default, not "maximum outflow": a high-scoring SME deposit
    // (online, rate-driven, no relationship) would run off at 25% as retail, but
    // retail status can't be shown, so it is wholesale - fully insured: 20%.
    let mut s = Customer::new("s", CustomerKind::Sme); // managed_as_retail unknown
    s.sme_group = "G".to_string().into();
    s.established_relationship = false.into();
    let mut a = acct("a", "s", Contract::Ump, 100_000, false, true);
    a.rate_driven = true.into();
    let b = book(vec![s], vec![a]);
    let cons = run(&b);
    let opt = classify(&b, &Params { mode: Mode::Optimistic, ..Params::new(rd()) }).unwrap();
    assert_eq!(items(&cons, "a"), [(Some("9.2.5"), aud(100_000))]);
    assert_eq!(items(&opt, "a"), [(Some("9.1.5"), aud(100_000))]);
    assert!(cons.lcr_outflow() < opt.lcr_outflow());
    assert!(asf(&cons) < asf(&opt)); // but the NSFR stays conservative
}

#[test]
fn shared_fcs_cover_makes_missing_data_non_monotone() {
    // One depositor (no relationship), two term deposits outside the LCR horizon.
    // "t" is transactional, so its insured part is stable; "a" is not, and its
    // FCS eligibility is unknown. Conservative: only "t" is eligible and takes
    // the cover (250k stable). Optimistic: both are eligible, and the
    // conservative allocator gives the cover to "a", where it buys no stability.
    // So knowing MORE (a is eligible) lowers ASF. Any allocation rule over a
    // shared per-depositor limit can do this.
    let mut t = td("t", 90, Some(EarlyWithdrawal::NoLegalRight));
    t.transactional = true.into();
    let mut a = td("a", 90, Some(EarlyWithdrawal::NoLegalRight));
    a.fcs_eligible = Uncertain::unknown("product not mapped to FCS register");
    a.balance = aud(300_000);
    t.balance = aud(300_000);
    let b = book(vec![person("p", Some(false))], vec![a, t]);
    let cons = run(&b);
    let opt = classify(&b, &Params { mode: Mode::Optimistic, ..Params::new(rd()) }).unwrap();
    assert!(cons.slices_for("t").any(|s| s.nsfr_item == "3"));
    assert!(opt.slices_for("t").all(|s| s.nsfr_item == "5"));
    assert!(asf(&cons) > asf(&opt));
}

#[test]
fn bad_inputs_are_rejected() {
    let orphan = book(vec![], vec![acct("a", "nobody", Contract::Ump, 1, true, false)]);
    assert!(matches!(classify(&orphan, &Params::new(rd())), Err(ClassifyError::UnknownCustomer { .. })));
    let dup = book(
        vec![person("p", None)],
        vec![acct("a", "p", Contract::Ump, 1, true, false), acct("a", "p", Contract::Ump, 2, true, false)],
    );
    assert!(matches!(classify(&dup, &Params::new(rd())), Err(ClassifyError::DuplicateAccount(_))));
    let neg = book(vec![person("p", None)], vec![acct("a", "p", Contract::Ump, -1, true, false)]);
    assert!(matches!(classify(&neg, &Params::new(rd())), Err(ClassifyError::NegativeBalance(_))));
}

#[test]
fn every_slice_says_why() {
    let c = run(&book(vec![person("p", Some(true))], vec![acct("a", "p", Contract::Ump, 400_000, false, true)]));
    for s in &c.slices {
        assert!(s.reasons.iter().any(|r| r.contains("Att A")), "{:?}", s.reasons);
    }
}

fn asf(c: &Classification) -> Q {
    let ev = evaluate(arf210::nsfr(), c.nsfr_inputs().iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    ev.q("19:calc").clone()
}

/// Deterministic pseudo-random books. Invariants:
/// - conservation: slices add back to balances, always;
/// - optimised FCS allocation never exceeds the conservative allocation;
/// - missing data never lowers LCR outflows or raises ASF - but only when no
///   depositor's FCS cover is shared across accounts and no SME is involved
///   (see the two counterexample tests above).
#[test]
fn invariants_hold_on_generated_books() {
    let mut seed: u64 = 0x5eed;
    let mut next = |n: u64| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    let mut monotone_checked = 0;
    for _ in 0..1500 {
        let mut b = Book::default();
        for ci in 0..(1 + next(6)) {
            let kind = [CustomerKind::NaturalPerson, CustomerKind::Smsf, CustomerKind::PersonalInvestmentEntity,
                        CustomerKind::Sme, CustomerKind::NonFinancialCorporate, CustomerKind::FinancialInstitution][next(6) as usize];
            let mut c = Customer::new(format!("c{ci}"), kind);
            let tri = |x: u64| match x { 0 => Uncertain::unknown("x"), 1 => Uncertain::Known(true), _ => Uncertain::Known(false) };
            c.established_relationship = tri(next(3));
            c.managed_as_retail = tri(next(3));
            c.pie_qualifies = tri(next(3));
            if next(3) > 0 {
                c.sme_group = format!("g{}", next(2)).into();
            }
            for ai in 0..(1 + next(4)) {
                let contract = match next(3) {
                    0 => Contract::Ump,
                    1 => Contract::Clm { notice_days: [7, 31, 60][next(3) as usize], grace_period_end: None },
                    _ => Contract::Pam {
                        maturity: days(next(800)),
                        early_withdrawal: [Uncertain::unknown("x"), EarlyWithdrawal::NoLegalRight.into(),
                                           EarlyWithdrawal::SignificantPenalty.into(), EarlyWithdrawal::Permitted.into()][next(4) as usize].clone(),
                    },
                };
                let mut a = DepositAccount::new(format!("c{ci}a{ai}"), format!("c{ci}"), contract, aud(next(3_000_000) as i64));
                a.fcs_eligible = tri(next(3));
                a.transactional = tri(next(3));
                a.online = tri(next(3));
                a.rate_driven = tri(next(3));
                b.accounts.push(a);
            }
            b.customers.push(c);
        }
        let p = Params::new(rd());
        let cons = classify(&b, &p).expect("conservation holds");
        let opt = classify(&b, &Params { mode: Mode::Optimistic, ..p.clone() }).unwrap();
        let optimised = classify(&b, &Params { fcs: FcsAllocation::Optimised, ..p.clone() }).unwrap();
        // Missing data is monotone only when accounts don't compete for one
        // depositor's FCS cover and no SME retail status is in play.
        let has_sme = b.customers.iter().any(|c| c.kind == CustomerKind::Sme);
        let shared_cover = b.customers.iter().any(|c| b.accounts.iter().filter(|a| a.customer_id == c.id).count() > 1);
        if !has_sme && !shared_cover {
            assert!(cons.lcr_outflow() >= opt.lcr_outflow());
            assert!(asf(&cons) <= asf(&opt));
            monotone_checked += 1;
        }
        assert!(optimised.lcr_outflow() <= cons.lcr_outflow());
        assert_eq!(cons.slices.iter().map(|s| s.amount.clone()).sum::<AudCarrying>(),
                   b.accounts.iter().map(|a| a.balance.clone()).sum::<AudCarrying>());
    }
    assert!(monotone_checked >= 50, "only {monotone_checked} books exercised the monotonicity invariant");
}

// ------------------------------------------------------------------ into the forms

#[test]
fn classification_feeds_the_forms() {
    let mut b = book(
        vec![person("p", Some(true)), sme("s", Some("G")), Customer::new("f", CustomerKind::FinancialInstitution)],
        vec![
            acct("a1", "p", Contract::Ump, 400_000, false, true),
            td("t400", 400, Some(EarlyWithdrawal::NoLegalRight)),
            acct("a2", "s", Contract::Ump, 2_500_000, true, false),
            acct("a3", "f", Contract::Ump, 1_000_000, false, false),
        ],
    );
    b.accounts[1].customer_id = "p".into();
    let c = run(&b);

    // LCR (AUD only form): items 8 + 9 equal the classifier's weighted outflow.
    let lcr = evaluate(arf210::lcr_aud(), c.lcr_inputs().iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    let outflow_m = c.lcr_outflow().amount() / int(1_000_000);
    assert_eq!(lcr.q("8:weighted") + lcr.q("9:weighted"), outflow_m);
    assert!(lcr.ok(), "{:?}", lcr.findings);

    // NSFR: total ASF from these deposits, computed by hand.
    let nsfr = evaluate(arf210::nsfr(), c.nsfr_inputs().iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).unwrap();
    // Depositor p has 500k and 250k of cover. Conservative allocation covers the
    // LCR-excluded t400 first (cover doesn't change its LCR rate), then 150k of a1.
    // t400: 100k stable, >= 1y -> 100%; a1: 150k stable 95% + 250k less stable 90%;
    // a2: wholesale SME, 50%; a3: FI < 6m, 0%.
    let want = (d("100000") + d("142500") + d("225000") + d("1250000")) / int(1_000_000);
    assert_eq!(nsfr.q("19:calc"), &want);
}
