//! Level 2 consolidation (APS 210 §§26-29), cross-border trapped-surplus
//! waterfall (HQLA1 -> HQLA2A -> HQLA2B -> ALA), host-supervisor
//! `JurisdictionPolicy`, and ARF 210.1A item 35 significant-currency analysis.

use std::collections::BTreeMap;

use prudence_core::Q;

use crate::{Part, Stream};

/// Host-supervisor liquidity policy for federated Level 2 subsidiaries (APS 210 §26).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jurisdiction {
    AuApra,
    NzRbnz,
    UkPra,
    UsFed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JurisdictionPolicy {
    pub jurisdiction: Jurisdiction,
    /// Host retail stable run-off rate (exact rational, e.g. 5/100).
    pub retail_stable_rate: Q,
    /// Host retail less-stable run-off rate.
    pub retail_less_stable_rate: Q,
    /// Whether host allows Alternative Liquid Assets (e.g. RBNZ / CLF).
    pub allows_ala: bool,
    /// Minimum local LCR floor before surplus is eligible for transfer (default 100%).
    pub local_lcr_floor: Q,
}

impl JurisdictionPolicy {
    pub fn for_jurisdiction(j: Jurisdiction) -> Self {
        let pct = |n: i64| Q::new(n.into(), 100.into());
        match j {
            Jurisdiction::AuApra => Self {
                jurisdiction: j,
                retail_stable_rate: pct(5),
                retail_less_stable_rate: pct(10),
                allows_ala: true,
                local_lcr_floor: pct(100),
            },
            Jurisdiction::NzRbnz => Self {
                jurisdiction: j,
                retail_stable_rate: pct(5),
                retail_less_stable_rate: pct(10),
                allows_ala: true,
                local_lcr_floor: pct(100),
            },
            Jurisdiction::UkPra => Self {
                jurisdiction: j,
                retail_stable_rate: pct(5),
                retail_less_stable_rate: pct(10),
                allows_ala: false,
                local_lcr_floor: pct(100),
            },
            Jurisdiction::UsFed => Self {
                jurisdiction: j,
                retail_stable_rate: pct(3),
                retail_less_stable_rate: pct(10),
                allows_ala: false,
                local_lcr_floor: pct(100),
            },
        }
    }
}

/// One subsidiary's standalone HQLA stock and net cash outflows (in AUD).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubsidiaryPosition {
    pub entity: String,
    pub jurisdiction: Jurisdiction,
    pub hqla1: Q,
    pub hqla2a: Q,
    pub hqla2b: Q,
    pub ala: Q,
    pub nco: Q,
    /// Maximum surplus legally transferable to the Level 2 parent (APS 210 §29).
    /// `0` means no surplus above the local floor may be transferred.
    pub transferable_surplus_cap: Q,
}

/// Result of applying the APS 210 §29 trapped-surplus waterfall to one subsidiary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrappedExclusion {
    pub entity: String,
    pub total_surplus: Q,
    pub trapped_total: Q,
    pub excluded_hqla1: Q,
    pub excluded_hqla2a: Q,
    pub excluded_hqla2b: Q,
    pub excluded_ala: Q,
    pub consolidated_hqla1: Q,
    pub consolidated_hqla2a: Q,
    pub consolidated_hqla2b: Q,
    pub consolidated_ala: Q,
}

/// APS 210 §29 cross-border trapped-surplus exclusion in strict
/// **HQLA1 → HQLA2A → HQLA2B → ALA** waterfall order.
pub fn exclude_trapped_surplus(sub: &SubsidiaryPosition, policy: &JurisdictionPolicy) -> TrappedExclusion {
    let zero = Q::default();
    let total_hqla = &sub.hqla1 + &sub.hqla2a + &sub.hqla2b + &sub.ala;
    let required_local = (&sub.nco * &policy.local_lcr_floor).max(zero.clone());
    let total_surplus = (&total_hqla - &required_local).max(zero.clone());
    let cap = sub.transferable_surplus_cap.clone().max(zero.clone());
    let trapped_total = (&total_surplus - &cap).max(zero.clone());

    let mut rem = trapped_total.clone();
    let take = |avail: &Q, rem: &mut Q| -> Q {
        let z = Q::default();
        let ex = avail.min(rem).max(&z).clone();
        *rem -= &ex;
        ex
    };
    let excluded_hqla1 = take(&sub.hqla1, &mut rem);
    let excluded_hqla2a = take(&sub.hqla2a, &mut rem);
    let excluded_hqla2b = take(&sub.hqla2b, &mut rem);
    let excluded_ala = take(&sub.ala, &mut rem);

    TrappedExclusion {
        entity: sub.entity.clone(),
        total_surplus,
        trapped_total,
        consolidated_hqla1: &sub.hqla1 - &excluded_hqla1,
        consolidated_hqla2a: &sub.hqla2a - &excluded_hqla2a,
        consolidated_hqla2b: &sub.hqla2b - &excluded_hqla2b,
        consolidated_ala: &sub.ala - &excluded_ala,
        excluded_hqla1,
        excluded_hqla2a,
        excluded_hqla2b,
        excluded_ala,
    }
}

/// Level 2 intragroup elimination: removes internal Level 2 group records
/// (e.g. intragroup loans/funding whose record IDs appear in `intragroup_ids`).
pub fn eliminate_intragroup(parts: &[Part], intragroup_ids: &std::collections::BTreeSet<String>) -> Vec<Part> {
    parts.iter().filter(|p| !intragroup_ids.contains(&p.record)).cloned().collect()
}

/// ARF 210.1A item 35: significant-currency summary (APS 210 §45).
/// A currency is significant when its aggregate liabilities are >= 5% of total liabilities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrencySummary {
    pub currency: String,
    pub share_of_liabilities: Q,
    pub significant: bool,
    pub total_outflows: Q,
    pub total_inflows: Q,
    pub net_cash_outflows: Q,
}

/// Compute per-currency liquidity summary and flag currencies >= 5% of total liabilities.
pub fn significant_currencies(
    parts: &[Part],
    liabilities_by_ccy: &BTreeMap<String, Q>,
) -> Vec<CurrencySummary> {
    let total_liab: Q = liabilities_by_ccy.values().cloned().sum();
    let five_pct = Q::new(5.into(), 100.into());
    let seventy_five_pct = Q::new(3.into(), 4.into());
    let zero = Q::default();

    let mut out_by_ccy: BTreeMap<String, Q> = BTreeMap::new();
    let mut in_by_ccy: BTreeMap<String, Q> = BTreeMap::new();
    for p in parts {
        if p.stream == Stream::Total {
            continue;
        }
        // ARF 210.1A items 8..=17 are outflows; 19..=22 are inflows.
        let top: u32 = p.item.split('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
        if (8..=17).contains(&top) {
            *out_by_ccy.entry(p.currency.clone()).or_default() += p.amount.clone();
        } else if (19..=22).contains(&top) {
            *in_by_ccy.entry(p.currency.clone()).or_default() += p.amount.clone();
        }
    }

    let mut ccys: std::collections::BTreeSet<String> = liabilities_by_ccy.keys().cloned().collect();
    ccys.extend(out_by_ccy.keys().cloned());
    ccys.extend(in_by_ccy.keys().cloned());

    ccys.into_iter()
        .map(|ccy| {
            let liab = liabilities_by_ccy.get(&ccy).cloned().unwrap_or_default();
            let share = if total_liab > zero { &liab / &total_liab } else { zero.clone() };
            let significant = share >= five_pct;
            let outflows = out_by_ccy.get(&ccy).cloned().unwrap_or_default();
            let inflows = in_by_ccy.get(&ccy).cloned().unwrap_or_default();
            let capped_in = inflows.clone().min(&outflows * &seventy_five_pct);
            let nco = (&outflows - &capped_in).max(zero.clone());
            CurrencySummary {
                currency: ccy,
                share_of_liabilities: share,
                significant,
                total_outflows: outflows,
                total_inflows: inflows,
                net_cash_outflows: nco,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prudence_core::num::int;

    #[test]
    fn trapped_surplus_follows_strict_hqla1_2a_2b_ala_waterfall() {
        let sub = SubsidiaryPosition {
            entity: "nz-sub".into(),
            jurisdiction: Jurisdiction::NzRbnz,
            hqla1: int(50),
            hqla2a: int(30),
            hqla2b: int(20),
            ala: int(10),
            nco: int(40),
            transferable_surplus_cap: int(10),
        };
        // Total HQLA = 110, local floor = 40, surplus = 70, cap = 10 -> trapped = 60.
        // Waterfall excludes 50 HQLA1 + 10 HQLA2A + 0 HQLA2B + 0 ALA.
        let ex = exclude_trapped_surplus(&sub, &JurisdictionPolicy::for_jurisdiction(Jurisdiction::NzRbnz));
        assert_eq!(ex.total_surplus, int(70));
        assert_eq!(ex.trapped_total, int(60));
        assert_eq!(ex.excluded_hqla1, int(50));
        assert_eq!(ex.excluded_hqla2a, int(10));
        assert_eq!(ex.excluded_hqla2b, int(0));
        assert_eq!(ex.excluded_ala, int(0));
        // Conservation: consolidated + excluded == original
        assert_eq!(&ex.consolidated_hqla1 + &ex.consolidated_hqla2a + &ex.consolidated_hqla2b + &ex.consolidated_ala, int(50));
    }

    #[test]
    fn significant_currencies_flags_five_percent_threshold() {
        let liab: BTreeMap<String, Q> = [
            ("AUD".into(), int(800)),
            ("USD".into(), int(150)),
            ("NZD".into(), int(40)), // 4% < 5%
            ("GBP".into(), int(10)), // 1% < 5%
        ]
        .into();
        let parts = vec![
            Part { module: "retail", record: "d1".into(), item: "8.1".into(), currency: "AUD".into(), amount: int(100), stream: Stream::Both },
            Part { module: "funding", record: "s1".into(), item: "10.2".into(), currency: "USD".into(), amount: int(50), stream: Stream::Both },
            Part { module: "loans", record: "l1".into(), item: "20.3".into(), currency: "USD".into(), amount: int(40), stream: Stream::Both },
        ];
        let res = significant_currencies(&parts, &liab);
        let sig: Vec<&str> = res.iter().filter(|c| c.significant).map(|c| c.currency.as_str()).collect();
        assert_eq!(sig, vec!["AUD", "USD"]);
        let usd = res.iter().find(|c| c.currency == "USD").unwrap();
        // USD inflows (40) capped at 75% of outflows (50) = 37.5 -> NCO = 12.5
        assert_eq!(usd.net_cash_outflows, Q::new(25.into(), 2.into()));
    }
}
