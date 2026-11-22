//! Macquarie Bank Limited (MBL) shaped synthetic ADI preset:
//! - BFS retail/SME deposits & residential mortgages
//! - Treasury HQLA1/2A/2B buffer & balanced repo book (calibrated for ~160-190% LCR, ~112-120% NSFR)
//! - CGM multi-currency derivatives & commodities (Bilateral OTC, CCP-cleared, Exchange-traded energy futures,
//!   CCP default fund contributions 42.3, customer IM 42.2, tiered CSA substitution, physical commodities 38.1)
//! - Level 2 offshore subsidiaries (UK PRA, US Fed, NZ RBNZ) for trapped-surplus & significant-currency reporting.

use std::collections::BTreeMap;

use assembly::Bank;
use assembly::level2::{Jurisdiction, SubsidiaryPosition};
use chrono::Days;
use derivatives::collateral::{ClearingKind, HqlaTier, SubstitutionRight};
use prudence_core::Q;
use prudence_core::num::int;
use prudence_core::uncertain::Uncertain;

use crate::synth::{Config, Rng};

fn m(millions: i64) -> Q {
    int(millions * 1_000_000)
}

fn to_i64(q: &Q) -> i64 {
    num_traits::ToPrimitive::to_i64(q).unwrap_or(0)
}

/// Generate a Macquarie-shaped ADI (`Bank`) plus its Level 2 offshore subsidiaries.
pub fn generate_macquarie(cfg: &Config) -> (Bank, Vec<SubsidiaryPosition>) {
    let mut rng = Rng::new(cfg.seed ^ 0x4D41_4351_5541_5249); // "MACQUARI"
    let rd = cfg.reporting_date;

    // 1. Start from BFS retail/SME core + loans from synth::generate.
    let mut b = crate::synth::generate(cfg);

    let retail_aud: i64 = b.retail.accounts.iter().map(|a| to_i64(a.balance.amount())).sum::<i64>().max(1_000_000_000);
    let retail_m = (retail_aud / 1_000_000).max(500);

    // 2. Recalibrate wholesale deposits & tag multi-currency funding (AUD/USD/EUR/GBP/JPY/NZD)
    //    so significant currencies (>=5% of liabilities, item 35) reflect a global CGM+BFS bank.
    let ccy_weights: [(u64, &str); 6] = [
        (46, "AUD"),
        (26, "USD"),
        (11, "EUR"),
        (9, "GBP"),
        (4, "JPY"),
        (4, "NZD"),
    ];
    for d in &mut b.wholesale_deposits {
        let c = rng.weighted(&ccy_weights);
        if c != "AUD" {
            b.currency.insert(d.id.clone(), c.to_string());
        }
    }

    // 3. Recalibrate commitments & lending obligations linearly to BFS/corporate scale
    //    (prevents superlinear item-15 blowup at large customer counts).
    b.commitments.commitments.clear();
    let n_comm = (cfg.customers / 200).clamp(10, 400);
    let comm_each = (retail_m / 20 / n_comm as i64).max(1);
    for i in 0..n_comm {
        let cp = match i % 4 {
            0 => obligations::Counterparty::NonFinancialCorporate,
            1 => obligations::Counterparty::RetailCustomer,
            2 => obligations::Counterparty::Sme,
            _ => obligations::Counterparty::OtherFinancialInstitution,
        };
        b.commitments.commitments.push(obligations::LendingCommitment {
            id: format!("mbl-comm-{i}"),
            counterparty: cp,
            amount: m(comm_each),
        });
    }

    // 4. Recalibrate Treasury HQLA1/2A/2B + Physical Commodities (CGM) + balanced Repo book.
    b.holdings.clear();
    b.repos.clear();
    let hqla1_total_m = retail_m * 62 / 100; // Strong MBL HQLA1 liquidity buffer
    let hqla2a_m = retail_m * 8 / 100;
    let hqla2b_m = retail_m * 4 / 100;
    let phys_comm_m = retail_m * 3 / 100; // Physical energy/metals commodities -> NSFR 38.1 (85% RSF)

    let holding_specs = [
        ("mbl-cash", secured::AssetKind::NotesAndCoin, secured::Issuer::AustralianGovernment, hqla1_total_m * 2 / 100, 0, 1, None, false),
        ("mbl-rba-esa", secured::AssetKind::RbaBalance, secured::Issuer::CentralBank, hqla1_total_m * 35 / 100, 0, 1, None, false),
        ("mbl-fed-cb", secured::AssetKind::ForeignCentralBankBalance, secured::Issuer::CentralBank, hqla1_total_m * 13 / 100, 0, 1, None, false),
        ("mbl-acgb-3y", secured::AssetKind::DebtSecurity, secured::Issuer::AustralianGovernment, hqla1_total_m * 28 / 100, 0, 1, Some(rd + Days::new(1100)), false),
        ("mbl-semi-4y", secured::AssetKind::DebtSecurity, secured::Issuer::StateCentralBorrowingAuthority, hqla1_total_m * 15 / 100, 0, 1, Some(rd + Days::new(1460)), false),
        ("mbl-ust-2y", secured::AssetKind::DebtSecurity, secured::Issuer::ForeignSovereign, hqla1_total_m * 7 / 100, 0, 1, Some(rd + Days::new(730)), false),
        ("mbl-cbond-2a", secured::AssetKind::CoveredBond, secured::Issuer::FinancialInstitutionIssuer, hqla2a_m, 20, 1, Some(rd + Days::new(900)), false),
        ("mbl-rmbs-2b", secured::AssetKind::Rmbs, secured::Issuer::FinancialInstitutionIssuer, hqla2b_m, 20, 1, Some(rd + Days::new(1200)), false),
        ("mbl-cgm-commodities", secured::AssetKind::Equity, secured::Issuer::NonFinancialCorporateIssuer, phys_comm_m, 100, 3, None, false),
    ];
    for (id, kind, issuer, amt_m, rw, grade, mat, enc) in holding_specs {
        b.holdings.push(secured::Holding {
            id: id.into(),
            kind,
            issuer,
            market_value: m(amt_m.max(1)),
            risk_weight: rw,
            rating_grade: grade,
            domestic_currency: !id.contains("ust") && !id.contains("fed"),
            index_constituent: false,
            own_or_associated: false,
            encumbered: enc,
            drawable_in_stress: Uncertain::Known(true),
            clf_category: secured::ClfCategory::NotClfEligible,
            value_after_rba_margin: int(0),
            maturity: mat,
            operational: Uncertain::Known(true),
        });
        if id.contains("ust") || id.contains("fed") {
            b.currency.insert(id.into(), "USD".into());
        }
    }

    // Balanced matched-book Treasury + CGM repos (funding & reverse repos <= 12% of retail_m)
    let n_repos = (cfg.customers / 100).clamp(20, 1500);
    let repo_each = (retail_m * 10 / 100 / n_repos as i64).max(1);
    for i in 0..n_repos {
        let leg = if i % 2 == 0 { secured::Leg::Lending } else { secured::Leg::Funding };
        let coll = match i % 5 {
            0 | 1 | 2 => secured::HqlaClass::Hqla1,
            3 => secured::HqlaClass::Hqla2a,
            _ => secured::HqlaClass::RmbsHqla2b,
        };
        let mat = rd + Days::new(rng.range(7, 240));
        let id = format!("mbl-repo-{i}");
        let c = rng.weighted(&ccy_weights);
        if c != "AUD" {
            b.currency.insert(id.clone(), c.into());
        }
        b.repos.push(secured::Repo {
            id,
            leg,
            cash: m(repo_each),
            collateral_class: coll,
            collateral_value: m(repo_each) * Q::new(102.into(), 100.into()),
            counterparty: if i % 10 == 0 {
                secured::Counterparty::DomesticSovereignOrPse
            } else {
                secured::Counterparty::Other
            },
            operational: Uncertain::Known(true),
            rehypothecated: leg == secured::Leg::Lending && i % 6 == 0,
            maturity: mat,
        });
    }

    // 5. CGM (Commodities & Global Markets) heavy multi-currency derivatives & energy futures book.
    b.derivatives = derivatives::Book::default();
    let n_sets = (cfg.customers / 50).clamp(40, 2000);
    let trades_per_set = 10usize;
    let cgm_ccys = ["AUD", "USD", "EUR", "GBP", "JPY", "NZD"];
    let scale_d = (retail_m / 2000).max(1);

    for s_idx in 0..n_sets {
        let sid = format!("cgm-set-{s_idx}");
        let clearing = match s_idx % 3 {
            0 => ClearingKind::BilateralOtc,
            1 => ClearingKind::CcpCleared,
            _ => ClearingKind::ExchangeTradedFuture,
        };
        let is_ccp = clearing != ClearingKind::BilateralOtc;
        let vm_post = m((rng.range(1, 4) as i64) * scale_d / (n_sets as i64 / 40).max(1));
        let vm_recv = m((rng.range(1, 4) as i64) * scale_d / (n_sets as i64 / 40).max(1));
        let im_own = m((rng.range(1, 3) as i64) * scale_d / (n_sets as i64 / 40).max(1));
        let im_cust = if is_ccp { im_own.clone() } else { int(0) };
        let ccp_df = if clearing == ClearingKind::CcpCleared {
            m(scale_d.max(1)) / Q::from_integer((n_sets as i64 / 20).max(1).into())
        } else {
            int(0)
        };
        let subst = if clearing == ClearingKind::BilateralOtc && s_idx % 4 == 0 {
            vec![SubstitutionRight {
                collateral_held: HqlaTier::Hqla1,
                substitutable_with: HqlaTier::Hqla2bNonRmbs,
                amount: m(scale_d) / Q::from_integer((n_sets as i64 / 10).max(1).into()),
            }]
        } else {
            Vec::new()
        };
        b.derivatives.sets.push(derivatives::NettingSet {
            id: sid.clone(),
            counterparty: format!("cgm-cp-{s_idx}"),
            netting_agreement: Some(true),
            collateral_received: vm_recv.clone(),
            required_from_counterparty: vm_recv.clone(),
            received_segregated: is_ccp,
            hqla_substitutable: int(0),
            due_not_called: int(0),
            downgrade_3_notches: if clearing == ClearingKind::BilateralOtc {
                m(scale_d) / Q::from_integer((n_sets as i64 / 10).max(1).into())
            } else {
                int(0)
            },
            posted_non_hqla1: int(0),
            received_reusable: int(0),
            variation_margin_posted: vm_post,
            cash_vm_received_eligible: vm_recv,
            initial_margin_posted: im_own,
            clearing,
            customer_initial_margin_posted: im_cust,
            ccp_default_fund_contribution: ccp_df,
            substitution_rights: subst,
            posted_lots: Vec::new(),
        });

        for t_idx in 0..trades_per_set {
            let sign = if (s_idx + t_idx) % 2 == 0 { 1i64 } else { -1i64 };
            let mag = (rng.range(1, 5) as i64) * scale_d;
            let div = (n_sets as i64 / 25).max(1);
            b.derivatives.trades.push(derivatives::Trade {
                set: sid.clone(),
                mtm: m(sign * mag) / Q::from_integer(div.into()),
            });
            let ccy = cgm_ccys[(s_idx + t_idx) % cgm_ccys.len()];
            b.derivatives.flows.push(derivatives::Flow {
                set: sid.clone(),
                date: rd + Days::new(rng.range(1, 29)),
                amount: m(sign * mag / 5) / Q::from_integer(div.into()),
                currency: ccy.into(),
                fx_principal: t_idx % 3 == 0,
                fx_trading: false,
                option: false,
                in_the_money: Uncertain::Known(true),
            });
        }
        // Historical energy/commodity collateral flows across 24 months (HLBA item 13.6)
        for cf_idx in 0..4 {
            let ago = rng.range(10, 700);
            let ccy = cgm_ccys[(s_idx + cf_idx) % cgm_ccys.len()];
            let sign = if cf_idx % 2 == 0 { -1i64 } else { 1i64 };
            let mag = (rng.range(1, 3) as i64) * scale_d;
            let div = (n_sets as i64 / 15).max(1);
            b.derivatives.collateral_flows.push(derivatives::CollateralFlow {
                set: sid.clone(),
                amount: m(sign * mag) / Q::from_integer(div.into()),
                currency: ccy.into(),
                valid: rd - Days::new(ago),
                recorded: rd - Days::new(ago),
            });
        }
    }

    // 6. Strong CET1/AT1/T2 capital + long-term senior/covered funding for ~115% NSFR.
    b.funding.capital.push(funding::CapitalInstrument {
        id: "mbl-cet1-core".into(),
        tier: funding::Tier::CET1,
        amount: m(retail_m * 18 / 100),
        maturity: None,
    });
    b.funding.securities.push(funding::IssuedSecurity {
        id: "mbl-lt-senior-usd".into(),
        kind: funding::SecurityKind::LongTerm,
        amount: m(retail_m * 28 / 100),
        maturity: rd + Days::new(1095),
        offshore: true,
    });
    b.currency.insert("mbl-lt-senior-usd".into(), "USD".into());
    b.funding.securities.push(funding::IssuedSecurity {
        id: "mbl-lt-covered-eur".into(),
        kind: funding::SecurityKind::CoveredBond,
        amount: m(retail_m * 12 / 100),
        maturity: rd + Days::new(1460),
        offshore: true,
    });
    b.currency.insert("mbl-lt-covered-eur".into(), "EUR".into());

    // 7. Rebook GL from the recalibrated books so GL ties out cleanly.
    b.gl = BTreeMap::new();
    let mut post = |kind: &str, amt: &Q| {
        *b.gl.entry(assembly::default_account(kind).into()).or_default() += amt.clone();
    };
    for a in &b.retail.accounts { post("retail", a.balance.amount()); }
    for d in &b.wholesale_deposits { post("wholesale", &d.balance); }
    for h in &b.holdings { post("holding", &h.market_value); }
    for r in &b.repos {
        post(if r.leg == secured::Leg::Funding { "repo-funding" } else { "repo-lending" }, &r.cash);
    }
    for l in &b.loans { post("loan", &l.balance); }
    for x in &b.other_inflows.receivables { post("receivable", &x.amount); }
    for t in &b.derivatives.trades { post("derivative", &t.mtm); }
    for x in &b.funding.securities { post("issued", &x.amount); }
    for x in &b.funding.capital { post("capital", &x.amount); }
    for x in &b.funding.placements { post("placement", &x.amount); }

    // 8. Level 2 offshore subsidiaries (UK PRA, US Fed, NZ RBNZ).
    let subs = vec![
        SubsidiaryPosition {
            entity: "MBL-UK (PRA)".into(),
            jurisdiction: Jurisdiction::UkPra,
            hqla1: m(retail_m * 9 / 100),
            hqla2a: m(retail_m * 2 / 100),
            hqla2b: int(0),
            ala: int(0),
            nco: m(retail_m * 7 / 100),
            transferable_surplus_cap: m(retail_m / 100), // ring-fenced UK surplus
        },
        SubsidiaryPosition {
            entity: "MBL-US (Fed)".into(),
            jurisdiction: Jurisdiction::UsFed,
            hqla1: m(retail_m * 12 / 100),
            hqla2a: m(retail_m * 3 / 100),
            hqla2b: int(0),
            ala: int(0),
            nco: m(retail_m * 10 / 100),
            transferable_surplus_cap: m(retail_m * 2 / 100),
        },
        SubsidiaryPosition {
            entity: "MBL-NZ (RBNZ)".into(),
            jurisdiction: Jurisdiction::NzRbnz,
            hqla1: m(retail_m * 3 / 100),
            hqla2a: m(retail_m / 100),
            hqla2b: int(0),
            ala: int(0),
            nco: m(retail_m * 3 / 100),
            transferable_surplus_cap: int(0),
        },
    ];

    (b, subs)
}
