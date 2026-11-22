//! Statutory HQLA rules (APS 210 Att A §25 & ARS 210.0 p. 39):
//! - **30-day downgrade grace period**: a downgraded security retains its
//!   pre-downgrade rating grade for 30 calendar days from the downgrade date
//!   (respecting bitemporal `recorded <= known_at`).
//! - **Hedge close-out adjustment**: LCR HQLA market value is adjusted for the
//!   early close-out cost/gain of associated price-risk hedges (floored at 0).

use std::collections::BTreeMap;

use chrono::{Days, NaiveDate};
use prudence_core::Q;

use crate::Holding;

/// Bitemporal rating downgrade event for an HQLA holding (APS 210 Att A §25).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DowngradeEvent {
    pub holding_id: String,
    pub prior_rating_grade: i64,
    pub new_rating_grade: i64,
    /// Date the downgrade took effect.
    pub valid: NaiveDate,
    /// Date the downgrade was recorded in the bank's systems.
    pub recorded: NaiveDate,
}

/// Price-risk hedge early close-out adjustment for an HQLA holding (ARS 210.0 p. 39).
/// Negative = close-out cost (reduces HQLA MV); positive = close-out gain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HedgeAdjustment {
    pub holding_id: String,
    pub closeout_adjustment: Q,
}

/// Effective rating grade of a holding under the APS 210 Att A §25 30-day
/// downgrade grace period, as known at `known_at`.
pub fn effective_rating_grade(
    holding_id: &str,
    current_grade: i64,
    downgrades: &[DowngradeEvent],
    reporting_date: NaiveDate,
    known_at: NaiveDate,
) -> i64 {
    // Find the latest downgrade known at `known_at` on or before `reporting_date`.
    let latest = downgrades
        .iter()
        .filter(|e| e.holding_id == holding_id && e.valid <= reporting_date && e.recorded <= known_at)
        .max_by_key(|e| (e.valid, e.recorded));
    match latest {
        Some(ev) => {
            let grace_end = ev.valid + Days::new(30);
            if reporting_date <= grace_end {
                ev.prior_rating_grade
            } else {
                ev.new_rating_grade
            }
        }
        None => current_grade,
    }
}

/// Apply the 30-day downgrade grace period to a slice of holdings.
pub fn apply_downgrade_grace(
    holdings: &[Holding],
    downgrades: &[DowngradeEvent],
    reporting_date: NaiveDate,
    known_at: NaiveDate,
) -> Vec<Holding> {
    if downgrades.is_empty() {
        return holdings.to_vec();
    }
    holdings
        .iter()
        .map(|h| {
            let mut h2 = h.clone();
            h2.rating_grade = effective_rating_grade(&h.id, h.rating_grade, downgrades, reporting_date, known_at);
            h2
        })
        .collect()
}

/// Apply ARS 210.0 p. 39 price-risk hedge close-out adjustments to HQLA holdings
/// (market value floored at 0).
pub fn apply_hedge_adjustments(holdings: &[Holding], adjustments: &[HedgeAdjustment]) -> Vec<Holding> {
    if adjustments.is_empty() {
        return holdings.to_vec();
    }
    let mut adj_by_id: BTreeMap<&str, Q> = BTreeMap::new();
    for a in adjustments {
        *adj_by_id.entry(a.holding_id.as_str()).or_default() += a.closeout_adjustment.clone();
    }
    let zero = Q::default();
    holdings
        .iter()
        .map(|h| {
            let mut h2 = h.clone();
            if let Some(delta) = adj_by_id.get(h.id.as_str()) {
                h2.market_value = (&h2.market_value + delta).max(zero.clone());
            }
            h2
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetKind, ClfCategory, Issuer};
    use prudence_core::num::int;
    use prudence_core::uncertain::Uncertain;

    fn corp_bond(id: &str, mv: i64, grade: i64) -> Holding {
        Holding {
            id: id.into(),
            kind: AssetKind::DebtSecurity,
            issuer: Issuer::NonFinancialCorporateIssuer,
            market_value: int(mv),
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
    fn downgrade_grace_period_lasts_exactly_30_days_and_respects_known_at() {
        let dg_date = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let ev = DowngradeEvent {
            holding_id: "b1".into(),
            prior_rating_grade: 1, // HQLA2A
            new_rating_grade: 4,   // ineligible
            valid: dg_date,
            recorded: dg_date + Days::new(2),
        };
        // Day 30 (2026-10-01): still in grace -> grade 1
        let d30 = dg_date + Days::new(30);
        assert_eq!(effective_rating_grade("b1", 4, std::slice::from_ref(&ev), d30, d30), 1);
        // Day 31 (2026-10-02): grace expired -> grade 4
        let d31 = dg_date + Days::new(31);
        assert_eq!(effective_rating_grade("b1", 4, std::slice::from_ref(&ev), d31, d31), 4);
        // Bitemporal replay before `recorded`: event not yet known -> current_grade (1)
        assert_eq!(effective_rating_grade("b1", 1, std::slice::from_ref(&ev), d31, dg_date + Days::new(1)), 1);
    }

    #[test]
    fn hedge_closeout_adjusts_market_value_and_floors_at_zero() {
        let hs = vec![corp_bond("b1", 100, 1), corp_bond("b2", 10, 1)];
        let adjs = vec![
            HedgeAdjustment { holding_id: "b1".into(), closeout_adjustment: int(-15) },
            HedgeAdjustment { holding_id: "b2".into(), closeout_adjustment: int(-25) },
        ];
        let out = apply_hedge_adjustments(&hs, &adjs);
        assert_eq!(out[0].market_value, int(85));
        assert_eq!(out[1].market_value, int(0));
    }
}
