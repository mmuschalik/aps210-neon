//! 12-month bitemporal historical lookback for trade finance and guarantees
//! (ARF 210.1A items 16.2–16.3; ARF 210.6 items 52–53).

use std::collections::BTreeMap;

use chrono::{Datelike, Months, NaiveDate};
use prudence_core::Q;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookbackKind {
    TradeFinance,
    Guarantee,
}

/// Realised historical outflow/inflow event for trade finance or guarantees.
/// Positive `amount` = net cash outflow; negative = net cash inflow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoricalOutflowEvent {
    pub kind: LookbackKind,
    pub amount: Q,
    pub valid: NaiveDate,
    pub recorded: NaiveDate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lookback12m {
    /// Total net outflows across the trailing 12-month window.
    pub total_12m: Q,
    /// Largest single calendar-month net outflow (floored at 0).
    pub max_monthly_net_outflow: Q,
    /// Exact rational monthly average (`total_12m / 12`, floored at 0).
    pub avg_monthly_net_outflow: Q,
    /// Per-(year, month) net outflows in the window.
    pub by_month: BTreeMap<(i32, u32), Q>,
}

/// Compute the trailing 12-month monthly net-outflow lookback `(rd - 12m, rd]`,
/// respecting bitemporal `recorded <= known_at`.
pub fn lookback_12m(
    events: &[HistoricalOutflowEvent],
    kind: LookbackKind,
    reporting_date: NaiveDate,
    known_at: NaiveDate,
) -> Lookback12m {
    let window_start = reporting_date.checked_sub_months(Months::new(12)).unwrap_or(reporting_date);

    let mut by_month: BTreeMap<(i32, u32), Q> = BTreeMap::new();
    let mut total = Q::default();
    for e in events {
        if e.kind == kind && e.valid > window_start && e.valid <= reporting_date && e.recorded <= known_at {
            *by_month.entry((e.valid.year(), e.valid.month())).or_default() += e.amount.clone();
            total += e.amount.clone();
        }
    }
    let zero = Q::default();
    let max_monthly = by_month.values().cloned().fold(zero.clone(), |acc, v| acc.max(v));
    let avg_monthly = (&total / Q::from_integer(12.into())).max(zero);
    Lookback12m {
        total_12m: total,
        max_monthly_net_outflow: max_monthly,
        avg_monthly_net_outflow: avg_monthly,
        by_month,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prudence_core::num::int;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn trailing_12m_lookback_respects_window_and_bitemporal_known_at() {
        let rd = d(2026, 9, 30);
        let events = vec![
            // Outside 12m window (2025-09-30 is excluded: `(rd - 12m, rd]`)
            HistoricalOutflowEvent { kind: LookbackKind::TradeFinance, amount: int(999), valid: d(2025, 9, 30), recorded: d(2025, 9, 30) },
            // Inside window: Jan 2026 (+60 and -10 = +50)
            HistoricalOutflowEvent { kind: LookbackKind::TradeFinance, amount: int(60), valid: d(2026, 1, 10), recorded: d(2026, 1, 10) },
            HistoricalOutflowEvent { kind: LookbackKind::TradeFinance, amount: int(-10), valid: d(2026, 1, 20), recorded: d(2026, 1, 20) },
            // Inside window: Mar 2026 (+70), recorded late (Oct 2)
            HistoricalOutflowEvent { kind: LookbackKind::TradeFinance, amount: int(70), valid: d(2026, 3, 15), recorded: d(2026, 10, 2) },
            // Different kind (Guarantee)
            HistoricalOutflowEvent { kind: LookbackKind::Guarantee, amount: int(500), valid: d(2026, 5, 1), recorded: d(2026, 5, 1) },
        ];
        // As known at reporting_date (Sept 30): late March event is invisible -> total 50, max 50
        let r1 = lookback_12m(&events, LookbackKind::TradeFinance, rd, rd);
        assert_eq!(r1.total_12m, int(50));
        assert_eq!(r1.max_monthly_net_outflow, int(50));

        // As known on Oct 5: late March event appears -> total 120, max 70, avg 10
        let r2 = lookback_12m(&events, LookbackKind::TradeFinance, rd, d(2026, 10, 5));
        assert_eq!(r2.total_12m, int(120));
        assert_eq!(r2.max_monthly_net_outflow, int(70));
        assert_eq!(r2.avg_monthly_net_outflow, int(10));
    }
}
