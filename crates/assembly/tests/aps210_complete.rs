//! End-to-end integration tests covering all 9 APS 210 statutory & behavioural rules:
//! 1. Securitisation scope divergence (APS 210 §5 / APS 120)
//! 2. HQLA 30-day downgrade grace period (APS 210 Att A §25)
//! 3. HQLA price-risk hedge close-out adjustment (ARS 210.0 p. 39)
//! 4. NSFR posted variation margin no-double-count (APS 210 Att C fn 12)
//! 5. NSFR CLF/ALA 10% RSF quota spillover (APS 210 Att C §32)
//! 6. Interdependent assets & liabilities (APS 210 Att C §37)
//! 7. 12-month bitemporal lookback for trade finance & guarantees
//! 8. Quarterly daily-compliance stats (min/max/mean/breaches, APS 210 §12)
//! 9. Versioned behavioural models (operational floor & mortgage prepayment, Att A §32)

use assembly::aps210_rules::*;
use assembly::behavioural::*;
use assembly::{Bank, lcr_return, lcr_return_as_known, maturity};
use chrono::{Days, NaiveDate};
use obligations::lookback::{HistoricalOutflowEvent, LookbackKind, lookback_12m};
use prudence_core::Q;
use prudence_core::num::int;
use prudence_core::uncertain::{Mode, Uncertain};
use secured::hqla_rules::{DowngradeEvent, HedgeAdjustment};
use secured::{AssetKind, ClfCategory, Holding, Issuer};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn m(millions: i64) -> Q {
    int(millions * 1_000_000)
}

fn corp_bond(id: &str, mv_m: i64, grade: i64) -> Holding {
    Holding {
        id: id.into(),
        kind: AssetKind::DebtSecurity,
        issuer: Issuer::NonFinancialCorporateIssuer,
        market_value: m(mv_m),
        risk_weight: 50,
        rating_grade: grade,
        domestic_currency: true,
        index_constituent: false,
        own_or_associated: false,
        encumbered: false,
        drawable_in_stress: Uncertain::Known(true),
        clf_category: ClfCategory::NotClfEligible,
        value_after_rba_margin: int(0),
        maturity: None,
        operational: Uncertain::Known(true),
    }
}

#[test]
fn rule_1_securitisation_spv_in_lcr_and_maturity_but_excluded_from_nsfr_and_mlh() {
    let rd = d(2026, 9, 30);
    let mut b = Bank::default();
    b.loans.push(loans::Loan {
        id: "spv-loan".into(),
        borrower: loans::Borrower::RetailCustomer,
        affiliation: loans::Affiliation::External,
        product: loans::Product::ResidentialMortgage,
        contract: loans::Contract::Pam { maturity: rd + Days::new(15) },
        balance: m(50),
        rate: Q::new(5.into(), 100.into()),
        next_payment: rd + Days::new(15),
        cycle_months: 1,
        performing: Uncertain::Known(true),
        lvr_at_most_80: Uncertain::Known(true),
        risk_weight: 35,
        encumbered_until: None,
    });
    b.holdings.push(Holding {
        id: "spv-cash".into(),
        kind: AssetKind::NotesAndCoin,
        issuer: Issuer::AustralianGovernment,
        market_value: m(10),
        risk_weight: 0,
        rating_grade: 1,
        domestic_currency: true,
        index_constituent: false,
        own_or_associated: false,
        encumbered: false,
        drawable_in_stress: Uncertain::Known(true),
        clf_category: ClfCategory::NotClfEligible,
        value_after_rba_margin: int(0),
        maturity: None,
        operational: Uncertain::Known(true),
    });
    b.capital_relief_spv.insert("spv-loan".into());
    b.capital_relief_spv.insert("spv-cash".into());

    let ret = lcr_return(&b, rd, Mode::Conservative).unwrap();
    // Included in LCR (20.1 retail loan inflow and 1.1 cash)
    assert!(ret.inputs.get("20.1").is_some_and(|v| *v > Q::default()));
    assert_eq!(ret.inputs.get("1.1"), Some(&int(10)));
    // Excluded from NSFR
    assert!(ret.nsfr_inputs.is_empty(), "SPV loan must be excluded from NSFR: {:?}", ret.nsfr_inputs);
    // Excluded from MLH
    let mlh_rows = maturity::mlh(&b, rd, Mode::Conservative).unwrap();
    assert!(mlh_rows.iter().all(|c| c.source != "spv-cash"), "SPV holding must be excluded from MLH");
}

#[test]
fn rules_2_and_3_hqla_downgrade_grace_and_hedge_closeout_flow_into_lcr() {
    let dg = d(2026, 9, 1);
    let mut b = Bank::default();
    b.holdings.push(corp_bond("corp1", 100, 4)); // downgraded to ineligible grade 4
    b.downgrades.push(DowngradeEvent {
        holding_id: "corp1".into(),
        prior_rating_grade: 1, // HQLA2A item 2.3
        new_rating_grade: 4,
        valid: dg,
        recorded: dg,
    });
    b.hqla_hedges.push(HedgeAdjustment {
        holding_id: "corp1".into(),
        closeout_adjustment: m(-12), // $12M hedge close-out cost
    });

    // Day 29 (2026-09-30): still in 30-day grace -> item 2.3 = 100 - 12 = 88 ($M)
    let in_grace = lcr_return_as_known(&b, dg + Days::new(29), dg + Days::new(29), Mode::Conservative).unwrap();
    assert_eq!(in_grace.inputs.get("2.3"), Some(&int(88)));

    // Day 31 (2026-10-02): grace expired -> drops out of item 2.3
    let expired = lcr_return_as_known(&b, dg + Days::new(31), dg + Days::new(31), Mode::Conservative).unwrap();
    assert_eq!(expired.inputs.get("2.3"), None);
}

#[test]
fn rules_4_5_6_nsfr_posted_vm_clf_spillover_and_interdependent_pairs() {
    let rd = d(2026, 9, 30);
    let mut b = Bank::default();
    // Derivative netting set with $15M variation margin posted -> 13.1.2 = 15
    b.derivatives.sets.push(derivatives::NettingSet {
        id: "ns1".into(),
        counterparty: "cp1".into(),
        netting_agreement: Some(true),
        variation_margin_posted: m(15),
        ..Default::default()
    });
    // Loan giving $50M in NSFR 35.1:b2
    b.loans.push(loans::Loan {
        id: "loan1".into(),
        borrower: loans::Borrower::RetailCustomer,
        affiliation: loans::Affiliation::External,
        product: loans::Product::ResidentialMortgage,
        contract: loans::Contract::Pam { maturity: rd + Days::new(500) },
        balance: m(50),
        rate: Q::new(5.into(), 100.into()),
        next_payment: rd + Days::new(500),
        cycle_months: 12,
        performing: Uncertain::Known(true),
        lvr_at_most_80: Uncertain::Known(true),
        risk_weight: 35,
        encumbered_until: None,
    });
    // Rule 4: $10M of posted VM removed from 35.1:b2 (<= $15M in 13.1.2)
    b.posted_vm_assets.push(PostedVariationMargin {
        netting_set_id: "ns1".into(),
        asset_record_id: "loan1".into(),
        rsf_cell: "35.1:b2".into(),
        amount: m(10),
    });
    // Rule 5: CLF cap is 0 (no CLF in LCR), so $20M 85%-fallback spills to 37.1:b2
    b.clf_nsfr_assets.push(ClfAsset {
        id: "clf1".into(),
        carrying_value: m(20),
        fallback: ClfSpilloverBucket::Rsf85,
    });
    // Rule 6: $30M interdependent pair -> item 16:b2 (0% ASF) and item 45:b2 (0% RSF)
    b.interdependent_pairs.push(InterdependentPair {
        id: "ip1".into(),
        asset_record_id: "ip-a".into(),
        liability_record_id: "ip-l".into(),
        amount: m(30),
        asset_maturity: d(2028, 9, 30),
        liability_maturity: d(2028, 9, 30),
        pass_through_cashflows: true,
        apra_approved: true,
    });

    let ret = lcr_return(&b, rd, Mode::Conservative).unwrap();
    assert_eq!(ret.nsfr_inputs.get("35.1:b2"), Some(&int(40))); // 50 - 10 posted VM
    assert_eq!(ret.nsfr_inputs.get("37.1:b2"), Some(&int(20))); // CLF excess spillover
    assert_eq!(ret.nsfr_inputs.get("17:b2"), Some(&int(30)));   // 0% ASF interdependent
    assert_eq!(ret.nsfr_inputs.get("44:b2"), Some(&int(30)));   // 0% RSF interdependent
    // Verify ARF 210.6 evaluates cleanly with strict input cell validation and 0% calc on 17 & 44
    let nsfr_ev = prudence_core::evaluate(
        arf210::nsfr(),
        ret.nsfr_inputs.iter().map(|(k, v)| (k.as_str(), v.clone())),
        prudence_core::Missing::Zero,
    )
    .expect("all NSFR inputs must be valid input cells on ARF 210.6");
    assert_eq!(nsfr_ev.q("17:calc"), &int(0));
    assert_eq!(nsfr_ev.q("44:calc"), &int(0));
}

#[test]
fn rules_7_8_9_lookback_quarterly_stats_and_behavioural_models() {
    let rd = d(2026, 9, 30);
    // Rule 7: 12-month guarantee lookback
    let lb = lookback_12m(
        &[HistoricalOutflowEvent { kind: LookbackKind::Guarantee, amount: m(24), valid: d(2026, 4, 15), recorded: d(2026, 4, 16) }],
        LookbackKind::Guarantee,
        rd,
        rd,
    );
    assert_eq!(lb.max_monthly_net_outflow, m(24));
    assert_eq!(lb.avg_monthly_net_outflow, m(2));

    // Rule 8: Quarterly compliance stats
    let stats = quarterly_compliance_stats(
        &[
            DailyObservation { date: d(2026, 7, 1), ratio_pct: int(112) },
            DailyObservation { date: d(2026, 8, 1), ratio_pct: int(99) },
            DailyObservation { date: d(2026, 9, 30), ratio_pct: int(119) },
        ],
        &int(100),
    )
    .unwrap();
    assert_eq!(stats.mean_ratio_pct, int(110));
    assert_eq!(stats.breaches.len(), 1);

    // Rule 9: Versioned behavioural models (operational floor + prepayment)
    let meta = ModelMetadata {
        model_id: "aps210-beh".into(),
        version: "1.0".into(),
        approved_by: "ALCO".into(),
        effective_from: d(2026, 1, 1),
        review_due: d(2026, 12, 31),
    };
    let op_model = RollingFloorOperationalModel { meta: meta.clone(), percentile: 5 };
    let (op, ex) = op_model.estimate(&m(100), &[m(70), m(80), m(90)], rd).unwrap();
    assert_eq!((op, ex), (m(70), m(30)));

    let prepay = ConstantPrepaymentModel { meta, smm_per_bucket: Q::new(1.into(), 5.into()) };
    let out = prepay.apply_prepayment(&[m(10), m(40), m(50)], rd).unwrap();
    let total: Q = out.iter().cloned().sum();
    assert_eq!(total, m(100));
}

#[test]
fn nsfr_secured_holdings_repos_and_obligations_evaluate_cleanly() {
    let rd = d(2026, 9, 30);
    let mut b = Bank::default();
    // Holdings: cash (20:b0), HQLA1 bond (25.1:b2), HQLA2A corp (30.1:b2), ASX200 equity (34.1:b2)
    b.holdings.push(Holding {
        id: "cash".into(),
        kind: AssetKind::NotesAndCoin,
        issuer: Issuer::AustralianGovernment,
        market_value: m(10),
        risk_weight: 0,
        rating_grade: 1,
        domestic_currency: true,
        index_constituent: false,
        own_or_associated: false,
        encumbered: false,
        drawable_in_stress: Uncertain::Known(true),
        clf_category: ClfCategory::NotClfEligible,
        value_after_rba_margin: int(0),
        maturity: None,
        operational: Uncertain::Known(true),
    });
    b.holdings.push(Holding {
        id: "acgb".into(),
        kind: AssetKind::DebtSecurity,
        issuer: Issuer::AustralianGovernment,
        market_value: m(100),
        risk_weight: 0,
        rating_grade: 1,
        domestic_currency: true,
        index_constituent: false,
        own_or_associated: false,
        encumbered: false,
        drawable_in_stress: Uncertain::Known(true),
        clf_category: ClfCategory::NotClfEligible,
        value_after_rba_margin: int(0),
        maturity: Some(d(2029, 9, 30)),
        operational: Uncertain::Known(true),
    });
    b.holdings.push(corp_bond("hqla2a", 40, 1));
    b.holdings.push(Holding {
        id: "asx200".into(),
        kind: AssetKind::Equity,
        issuer: Issuer::NonFinancialCorporateIssuer,
        market_value: m(20),
        risk_weight: 100,
        rating_grade: 2,
        domestic_currency: true,
        index_constituent: true,
        own_or_associated: false,
        encumbered: false,
        drawable_in_stress: Uncertain::Known(true),
        clf_category: ClfCategory::NotClfEligible,
        value_after_rba_margin: int(0),
        maturity: None,
        operational: Uncertain::Known(true),
    });
    // Repos: reverse repo secured by HQLA1 (<6m -> 23.1:b0, 10% RSF) + 9m funding repo (12:b1, 50% ASF)
    b.repos.push(secured::Repo {
        id: "revrepo1".into(),
        leg: secured::Leg::Lending,
        cash: m(50),
        collateral_class: secured::HqlaClass::Hqla1,
        collateral_value: m(52),
        counterparty: secured::Counterparty::Other,
        operational: Uncertain::Known(true),
        rehypothecated: false,
        maturity: d(2026, 12, 31),
    });
    b.repos.push(secured::Repo {
        id: "repo1".into(),
        leg: secured::Leg::Funding,
        cash: m(80),
        collateral_class: secured::HqlaClass::Hqla1,
        collateral_value: m(82),
        counterparty: secured::Counterparty::Other,
        operational: Uncertain::Known(true),
        rehypothecated: false,
        maturity: d(2027, 6, 30),
    });
    // Obligations: committed liquidity (48, 5%), mortgage redraw (49, 5%), uncommitted credit (51, 1%),
    // trade finance (52, 100%), guarantee (53, 100%).
    b.commitments.facilities.push(obligations::Facility {
        id: "liq1".into(),
        counterparty: obligations::Counterparty::NonFinancialCorporate,
        kind: obligations::FacilityKind::LiquidityFacility,
        committed: true,
        undrawn: m(100),
        drawable_within_30_days: Uncertain::Known(true),
        mortgage_redraw: false,
        funds_vehicle: false,
        limit: Some(m(100)),
    });
    b.commitments.facilities.push(obligations::Facility {
        id: "redraw1".into(),
        counterparty: obligations::Counterparty::RetailCustomer,
        kind: obligations::FacilityKind::CreditFacility,
        committed: false,
        undrawn: m(60),
        drawable_within_30_days: Uncertain::Known(true),
        mortgage_redraw: true,
        funds_vehicle: false,
        limit: Some(m(60)),
    });
    b.commitments.facilities.push(obligations::Facility {
        id: "uncomm1".into(),
        counterparty: obligations::Counterparty::NonFinancialCorporate,
        kind: obligations::FacilityKind::CreditFacility,
        committed: false,
        undrawn: m(200),
        drawable_within_30_days: Uncertain::Known(true),
        mortgage_redraw: false,
        funds_vehicle: false,
        limit: Some(m(200)),
    });
    b.commitments.obligations.push(obligations::ContingentObligation {
        id: "tf1".into(),
        kind: obligations::ObligationKind::TradeFinance,
        amount: m(3),
    });
    b.commitments.obligations.push(obligations::ContingentObligation {
        id: "guar1".into(),
        kind: obligations::ObligationKind::Guarantee,
        amount: m(2),
    });

    let ret = lcr_return(&b, rd, Mode::Conservative).unwrap();
    assert_eq!(ret.nsfr_inputs.get("20:b0"), Some(&int(10)));
    assert_eq!(ret.nsfr_inputs.get("25.1:b2"), Some(&int(100)));
    assert_eq!(ret.nsfr_inputs.get("30.1:b2"), Some(&int(40)));
    assert_eq!(ret.nsfr_inputs.get("34.1:b2"), Some(&int(20)));
    assert_eq!(ret.nsfr_inputs.get("23.1:b0"), Some(&int(50)));
    assert_eq!(ret.nsfr_inputs.get("12:b1"), Some(&int(80)));
    assert_eq!(ret.nsfr_inputs.get("48"), Some(&int(100)));
    assert_eq!(ret.nsfr_inputs.get("49"), Some(&int(60)));
    assert_eq!(ret.nsfr_inputs.get("51"), Some(&int(200)));
    assert_eq!(ret.nsfr_inputs.get("52"), Some(&int(36)));
    assert_eq!(ret.nsfr_inputs.get("53"), Some(&int(24)));

    let ev = prudence_core::evaluate(
        arf210::nsfr(),
        ret.nsfr_inputs.iter().map(|(k, v)| (k.as_str(), v.clone())),
        prudence_core::Missing::Zero,
    )
    .expect("all secured & commitment NSFR inputs must be valid ARF 210.6 cells");
    // ASF: repo1 80 * 50% = 40
    assert_eq!(ev.q("19:calc"), &int(40));
    // On-BS RSF: cash 0 + acgb 100*5%=5 + hqla2a 40*15%=6 + asx200 20*100%=20 + revrepo1 50*10%=5 = 36
    assert_eq!(ev.q("47:calc"), &int(36));
    // Off-BS RSF: 48(100*5%=5) + 49(60*5%=3) + 51(200*1%=2) + 52(3*12=36) + 53(2*12=24) = 70
    assert_eq!(ev.q("55:calc"), &int(70));
}

#[test]
fn aps210_stress_suite_and_survival_horizon() {
    use assembly::steering::{ScenarioKind, aps210_standard_suite, stress_full, survival_horizon};

    let rd = d(2026, 9, 30);
    let mut b = Bank::default();
    // $80M cash + $60M HQLA1 bond + $30M HQLA2A corporate bond
    b.holdings.push(Holding {
        id: "cash".into(),
        kind: AssetKind::NotesAndCoin,
        issuer: Issuer::AustralianGovernment,
        market_value: m(80),
        risk_weight: 0,
        rating_grade: 1,
        domestic_currency: true,
        index_constituent: false,
        own_or_associated: false,
        encumbered: false,
        drawable_in_stress: Uncertain::Known(true),
        clf_category: ClfCategory::NotClfEligible,
        value_after_rba_margin: int(0),
        maturity: None,
        operational: Uncertain::Known(true),
    });
    b.holdings.push(Holding {
        id: "acgb1".into(),
        kind: AssetKind::DebtSecurity,
        issuer: Issuer::AustralianGovernment,
        market_value: m(60),
        risk_weight: 0,
        rating_grade: 1,
        domestic_currency: true,
        index_constituent: false,
        own_or_associated: false,
        encumbered: false,
        drawable_in_stress: Uncertain::Known(true),
        clf_category: ClfCategory::NotClfEligible,
        value_after_rba_margin: int(0),
        maturity: Some(d(2029, 9, 30)),
        operational: Uncertain::Known(true),
    });
    b.holdings.push(corp_bond("corp2a", 30, 1));
    // Wholesale operational deposit ($100M, of which $60M operational)
    b.wholesale_customers.push(deposits::wholesale::Customer {
        id: "c1".into(),
        kind: deposits::wholesale::Kind::NonFinancialCorporate,
    });
    b.wholesale_deposits.push(deposits::wholesale::Deposit {
        id: "wd1".into(),
        customer_id: "c1".into(),
        balance: m(100),
        within_30_days: true,
        called: false,
        fcs_eligible: Uncertain::Known(false),
        operational_service: Uncertain::Known(true),
        operational_balance: Uncertain::Known(m(60)),
        maturity: None,
    });
    // Committed corporate facility ($100M undrawn)
    b.commitments.facilities.push(obligations::Facility {
        id: "rcf1".into(),
        counterparty: obligations::Counterparty::NonFinancialCorporate,
        kind: obligations::FacilityKind::CreditFacility,
        committed: true,
        undrawn: m(100),
        drawable_within_30_days: Uncertain::Known(true),
        mortgage_redraw: false,
        funds_vehicle: false,
        limit: Some(m(100)),
    });
    // Derivative netting set with $15M 3-notch downgrade collateral call
    b.derivatives.sets.push(derivatives::NettingSet {
        id: "csa1".into(),
        counterparty: "cp1".into(),
        netting_agreement: Some(true),
        downgrade_3_notches: m(15),
        ..Default::default()
    });
    // Long-term issued debt maturing in 60 days ($70M -> rolls into 30d window at day 31!)
    b.funding.securities.push(funding::IssuedSecurity {
        id: "bond60".into(),
        kind: funding::SecurityKind::LongTerm,
        amount: m(70),
        maturity: rd + Days::new(60),
        offshore: false,
    });

    let suite = aps210_standard_suite();
    assert_eq!(suite.len(), 4);
    assert_eq!(suite[0].kind, ScenarioKind::Idiosyncratic);
    assert_eq!(suite[1].kind, ScenarioKind::MarketWide);
    assert_eq!(suite[2].kind, ScenarioKind::Combined);
    assert_eq!(suite[3].roll_forward_days, 90);

    let results = stress_full(&b, rd, &suite).unwrap();
    assert_eq!(results.len(), 5); // base + 4 scenarios
    let base_surplus = results[0].lcr.surplus();
    let base_rsf = results[0].nsfr.rsf.clone();
    for res in &results[1..] {
        assert!(res.lcr.surplus() < base_surplus, "{} should reduce LCR surplus", res.name);
        // Drawing facilities + posting downgrade IM must increase NSFR RSF (never improve NSFR)
        assert!(res.nsfr.rsf > base_rsf, "{} should increase NSFR RSF: {} vs {}", res.name, res.nsfr.rsf, base_rsf);
    }
    // Combined acute should be worse than either standalone 30d scenario
    assert!(results[3].lcr.surplus() <= results[1].lcr.surplus());
    assert!(results[3].lcr.surplus() <= results[2].lcr.surplus());

    // Survival horizon under Combined (90d): bond60 rolls into the 30-day window at day 30, causing a breach!
    let sh = survival_horizon(&b, rd, &suite[2], 60, 15).unwrap();
    assert!(sh.breach_day.is_some(), "expected breach when 60d bond rolls into 30d LCR window: {sh:?}");
}
