//! Granular derivatives & collateral logic (APS 210 Att A §§48-65; Att C §36):
//! - Clearing venue (`BilateralOtc`, `CcpCleared`, `ExchangeTradedFuture`) &
//!   CCP default-fund contributions (`ARF 210.6` item `42.3`) + exempt customer
//!   initial margin (`42.2`).
//! - Tiered CSA collateral substitution haircut differentials (`ARF 210.1A` item `13.5`).
//! - Lot-level posted non-HQLA1 collateral valuation base (`ARF 210.1A` item `13.7`).
//! - Schedule-based 30-day cashflow generation for IRS and FX forward contracts.

use chrono::{Days, NaiveDate};
use prudence_core::Q;
use prudence_core::uncertain::Uncertain;

use crate::Flow;

/// Clearing / execution venue of a derivative netting set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClearingKind {
    #[default]
    BilateralOtc,
    CcpCleared,
    ExchangeTradedFuture,
}

/// HQLA liquidity tier for CSA collateral substitution and lot-level valuation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HqlaTier {
    Hqla1,
    Hqla2a,
    Hqla2bRmbs,
    Hqla2bNonRmbs,
    NonHqla,
}

impl HqlaTier {
    /// Standard APS 210 LCR haircut (exact rational).
    pub fn haircut(self) -> Q {
        let pct: i64 = match self {
            HqlaTier::Hqla1 => 0,
            HqlaTier::Hqla2a => 15,
            HqlaTier::Hqla2bRmbs => 25,
            HqlaTier::Hqla2bNonRmbs => 50,
            HqlaTier::NonHqla => 100,
        };
        Q::new(pct.into(), 100.into())
    }
}

/// One CSA collateral-substitution right (APS 210 Att A §58 / ARF 210.1A item 13.5):
/// counterparty posted `collateral_held` (currently counted in HQLA) and has
/// the unilateral right to substitute it with `substitutable_with`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubstitutionRight {
    pub collateral_held: HqlaTier,
    pub substitutable_with: HqlaTier,
    pub amount: Q,
}

/// Exact rational collateral-substitution outflow (`13.5`):
/// `0` if segregated; otherwise `sum(max(0, haircut(sub) - haircut(held)) * amount)`.
pub fn substitution_outflow(rights: &[SubstitutionRight], segregated: bool) -> Q {
    if segregated {
        return Q::default();
    }
    let zero = Q::default();
    rights
        .iter()
        .map(|r| {
            let diff = (&r.substitutable_with.haircut() - &r.collateral_held.haircut()).max(zero.clone());
            &r.amount * &diff
        })
        .sum()
}

/// Lot-level collateral posted or received reusable for `ARF 210.1A` item `13.7`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostedCollateralLot {
    pub holding_id: String,
    pub hqla_tier: HqlaTier,
    pub market_value: Q,
    /// `false` = posted by ADI; `true` = received by ADI and reusable to offset posted non-HQLA1.
    pub reusable_received: bool,
}

/// Lot-level valuation-change base for `13.7`:
/// `max(0, sum(posted non-HQLA1) - sum(reusable received non-HQLA1))`.
pub fn posted_non_hqla1_valuation_base(lots: &[PostedCollateralLot]) -> Q {
    let mut posted = Q::default();
    let mut received = Q::default();
    for lot in lots {
        if lot.hqla_tier == HqlaTier::Hqla1 {
            continue;
        }
        if lot.reusable_received {
            received += lot.market_value.clone();
        } else {
            posted += lot.market_value.clone();
        }
    }
    (&posted - &received).max(Q::default())
}

/// Interest-rate swap contract terms for generating 30-day cashflows.
#[derive(Clone, Debug)]
pub struct IrsContract {
    pub set: String,
    pub currency: String,
    pub notional: Q,
    /// Annualised fixed rate (exact rational, e.g. 4/100).
    pub fixed_rate: Q,
    /// Annualised current floating rate (exact rational).
    pub floating_rate: Q,
    /// True if ADI receives fixed and pays floating.
    pub receive_fixed: bool,
    /// Coupon payment date.
    pub next_payment_date: NaiveDate,
    /// Accrual fraction of the year (e.g. 1/4 for quarterly, 1/2 for semi-annual).
    pub year_fraction: Q,
}

/// Generate the 30-day cashflow (if `next_payment_date` falls in `(rd, rd + 30d]`).
pub fn irs_30d_flows(c: &IrsContract, reporting_date: NaiveDate) -> Vec<Flow> {
    if c.next_payment_date <= reporting_date || c.next_payment_date > reporting_date + Days::new(30) {
        return vec![];
    }
    let spread = if c.receive_fixed {
        &c.fixed_rate - &c.floating_rate
    } else {
        &c.floating_rate - &c.fixed_rate
    };
    let amount = &c.notional * &spread * &c.year_fraction;
    vec![Flow {
        set: c.set.clone(),
        date: c.next_payment_date,
        amount,
        currency: c.currency.clone(),
        fx_principal: false,
        fx_trading: false,
        option: false,
        in_the_money: Uncertain::Known(true),
    }]
}

/// FX forward / swap leg contract terms for generating gross 30-day FX principal flows.
#[derive(Clone, Debug)]
pub struct FxForwardContract {
    pub set: String,
    pub settlement_date: NaiveDate,
    pub receive_currency: String,
    /// AUD equivalent of the received principal leg (positive).
    pub receive_aud_amount: Q,
    pub pay_currency: String,
    /// AUD equivalent of the paid principal leg (positive).
    pub pay_aud_amount: Q,
    pub fx_trading: bool,
}

/// Generate both principal legs (positive receive, negative pay) if settling within 30 days.
pub fn fx_forward_30d_flows(c: &FxForwardContract, reporting_date: NaiveDate) -> Vec<Flow> {
    if c.settlement_date <= reporting_date || c.settlement_date > reporting_date + Days::new(30) {
        return vec![];
    }
    vec![
        Flow {
            set: c.set.clone(),
            date: c.settlement_date,
            amount: c.receive_aud_amount.clone(),
            currency: c.receive_currency.clone(),
            fx_principal: true,
            fx_trading: c.fx_trading,
            option: false,
            in_the_money: Uncertain::Known(true),
        },
        Flow {
            set: c.set.clone(),
            date: c.settlement_date,
            amount: -c.pay_aud_amount.clone(),
            currency: c.pay_currency.clone(),
            fx_principal: true,
            fx_trading: c.fx_trading,
            option: false,
            in_the_money: Uncertain::Known(true),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use prudence_core::num::int;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn tiered_substitution_uses_exact_haircut_differential() {
        let rights = vec![
            // HQLA1 (0%) -> HQLA2A (15%) on $100 -> $15 outflow
            SubstitutionRight { collateral_held: HqlaTier::Hqla1, substitutable_with: HqlaTier::Hqla2a, amount: int(100) },
            // HQLA2A (15%) -> NonHQLA (100%) on $100 -> $85 outflow
            SubstitutionRight { collateral_held: HqlaTier::Hqla2a, substitutable_with: HqlaTier::NonHqla, amount: int(100) },
            // Upgrade (HQLA2A -> HQLA1) -> $0 outflow
            SubstitutionRight { collateral_held: HqlaTier::Hqla2a, substitutable_with: HqlaTier::Hqla1, amount: int(100) },
        ];
        assert_eq!(substitution_outflow(&rights, false), int(100));
        assert_eq!(substitution_outflow(&rights, true), int(0));
    }

    #[test]
    fn lot_level_posted_collateral_excludes_hqla1_and_nets_reusable() {
        let lots = vec![
            PostedCollateralLot { holding_id: "h1".into(), hqla_tier: HqlaTier::Hqla1, market_value: int(500), reusable_received: false },
            PostedCollateralLot { holding_id: "h2".into(), hqla_tier: HqlaTier::Hqla2a, market_value: int(120), reusable_received: false },
            PostedCollateralLot { holding_id: "h3".into(), hqla_tier: HqlaTier::NonHqla, market_value: int(40), reusable_received: true },
        ];
        assert_eq!(posted_non_hqla1_valuation_base(&lots), int(80));
    }

    #[test]
    fn irs_and_fx_forward_generate_exact_30d_flows() {
        let rd = d(2026, 9, 30);
        let irs = IrsContract {
            set: "s1".into(),
            currency: "AUD".into(),
            notional: int(1_000_000),
            fixed_rate: Q::new(5.into(), 100.into()),
            floating_rate: Q::new(3.into(), 100.into()),
            receive_fixed: true,
            next_payment_date: d(2026, 10, 15),
            year_fraction: Q::new(1.into(), 4.into()),
        };
        let fs = irs_30d_flows(&irs, rd);
        assert_eq!(fs.len(), 1);
        assert_eq!(fs[0].amount, int(5_000)); // 1M * 2% * 0.25

        let fx = FxForwardContract {
            set: "s1".into(),
            settlement_date: d(2026, 10, 20),
            receive_currency: "AUD".into(),
            receive_aud_amount: int(100_000),
            pay_currency: "USD".into(),
            pay_aud_amount: int(98_000),
            fx_trading: false,
        };
        let fx_fs = fx_forward_30d_flows(&fx, rd);
        assert_eq!(fx_fs.len(), 2);
        assert_eq!(fx_fs[0].amount, int(100_000));
        assert_eq!(fx_fs[1].amount, int(-98_000));
    }
}
