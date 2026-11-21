//! Balance-sheet steering on top of the statutory rules (design 4.A.3-5,
//! 4.C.3): stress scenarios as transformations of the ADI's data (the
//! statutory rules run unchanged on the result), the cost of missing data,
//! and marginal liquidity costs for funds transfer pricing.

use std::collections::BTreeMap;

use chrono::{Days, NaiveDate};
use prudence_core::uncertain::Mode;
use prudence_core::{Missing, Q, evaluate};

use crate::{Bank, lcr_return, lcr_return_modes};

/// The LCR numerator and denominator of one ARF 210.1A (AUD millions).
#[derive(Clone, Debug, PartialEq)]
pub struct Lcr {
    /// HQLA plus RBNZ securities / ALA (items 25 + 26).
    pub liquid_assets: Q,
    /// Total weighted cash outflows (item 18) and inflows (item 23).
    pub outflows: Q,
    pub inflows: Q,
    /// Net cash outflows (item 29): after the 75% inflow cap and overlays.
    pub net_outflows: Q,
    /// Item 30, undefined without net outflows.
    pub lcr: Option<Q>,
}

impl Lcr {
    /// Liquid assets above what a 100% LCR needs.
    pub fn surplus(&self) -> Q {
        &self.liquid_assets - &self.net_outflows
    }
}

pub fn lcr_of(inputs: &BTreeMap<String, Q>) -> Result<Lcr, String> {
    let ev = evaluate(arf210::lcr_all(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).map_err(|e| format!("{e:?}"))?;
    let g = |k: &str| ev.get(k).num().cloned().unwrap_or_default();
    Ok(Lcr {
        liquid_assets: g("25:value") + g("26:value"),
        outflows: g("18:weighted"),
        inflows: g("23:weighted"),
        net_outflows: g("29:value"),
        lcr: ev.get("30:value").num().cloned(),
    })
}

// ---------------------------------------------------------------- stress

/// A stress scenario: what happens to the ADI's data. The statutory rules
/// then apply unchanged (design 4.C.3: behavioural assumptions stay outside
/// the statutory modules).
#[derive(Clone, Debug, Default)]
pub struct Scenario {
    pub name: String,
    /// Market value falls on HQLA1 / HQLA2A / HQLA2B holdings (fractions,
    /// e.g. 1/10 for 10%), by the class each holding reports in.
    pub price_shock: [Q; 3],
    /// Every notice deposit becomes callable today (e.g. 31-day notice
    /// savers called in full).
    pub call_notice_deposits: bool,
    /// Wholesale funding is not rolled over: every wholesale deposit is called.
    pub freeze_wholesale_rollover: bool,
    /// Report as at this many days later (maturities roll into the window).
    pub roll_forward_days: u64,
}

pub fn apply(bank: &Bank, rd: NaiveDate, s: &Scenario) -> Result<(Bank, NaiveDate), String> {
    let mut b = bank.clone();
    if s.price_shock.iter().any(|q| *q != Q::default()) {
        // The class each holding reports in, from the base run.
        let base = lcr_return(bank, rd, Mode::Conservative)?;
        for h in &mut b.holdings {
            let class = base.parts.iter().find(|p| p.record == h.id && p.module == "secured").map(|p| p.item.split('.').next().unwrap_or("").to_string());
            let k = match class.as_deref() {
                Some("1") => 0,
                Some("2") => 1,
                Some("3") => 2,
                _ => continue,
            };
            let keep = Q::from_integer(1.into()) - &s.price_shock[k];
            h.market_value = &h.market_value * &keep;
            h.value_after_rba_margin = &h.value_after_rba_margin * &keep;
        }
    }
    if s.call_notice_deposits {
        for a in &mut b.retail.accounts {
            if matches!(a.contract, deposits::Contract::Clm { .. }) {
                a.contract = deposits::Contract::Ump;
            }
        }
    }
    if s.freeze_wholesale_rollover {
        for d in &mut b.wholesale_deposits {
            d.called = true;
        }
    }
    Ok((b, rd + Days::new(s.roll_forward_days)))
}

/// The LCR under each scenario, with the base first.
pub fn stress(bank: &Bank, rd: NaiveDate, scenarios: &[Scenario]) -> Result<Vec<(String, Lcr)>, String> {
    let mut out = vec![("base".to_string(), lcr_of(&lcr_return(bank, rd, Mode::Conservative)?.inputs)?)];
    for s in scenarios {
        let (b, d) = apply(bank, rd, s)?;
        out.push((s.name.clone(), lcr_of(&lcr_return(&b, d, Mode::Conservative)?.inputs)?));
    }
    Ok(out)
}

// ---------------------------------------------------------------- data quality

pub const MODULES: [&str; 7] = ["retail", "wholesale", "secured", "obligations", "loans", "funding", "derivatives"];

/// What resolving one module's unknowns would be worth (design 4.A.3:
/// "data-quality capital drag").
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    pub module: &'static str,
    /// Unknown attributes resolved conservatively in the reported return.
    pub unknowns: usize,
    /// Liquid assets freed (AUD millions): the change in surplus over a 100%
    /// LCR if this module's unknowns all turned out favourably.
    pub liquid_assets_freed: Q,
    /// At the given carry cost of holding liquid assets (per year).
    pub annual_cost: Q,
}

/// The reported LCR, the LCR if every unknown turned out favourably, and the
/// drag of each module, largest first. `carry` is the yearly cost of holding
/// one dollar of liquid assets instead of lending it (e.g. 1/100 for 1%).
pub fn data_quality(bank: &Bank, rd: NaiveDate, carry: &Q) -> Result<(Lcr, Lcr, Vec<Drag>), String> {
    let base = lcr_return(bank, rd, Mode::Conservative)?;
    let reported = lcr_of(&base.inputs)?;
    let best = lcr_of(&lcr_return(bank, rd, Mode::Optimistic)?.inputs)?;
    let mut drags = Vec::new();
    for m in MODULES {
        let r = lcr_return_modes(bank, rd, &|x| if x == m { Mode::Optimistic } else { Mode::Conservative })?;
        let freed = lcr_of(&r.inputs)?.surplus() - reported.surplus();
        let unknowns = base.assumptions.iter().filter(|a| a.0 == m).count();
        drags.push(Drag { module: m, unknowns, annual_cost: &freed * carry, liquid_assets_freed: freed });
    }
    drags.sort_by(|a, b| b.liquid_assets_freed.cmp(&a.liquid_assets_freed));
    Ok((reported, best, drags))
}

// ---------------------------------------------------------------- funds transfer pricing

/// The marginal effect of one more million in an ARF 210.1A input item.
#[derive(Clone, Debug, PartialEq)]
pub struct Marginal {
    pub item: String,
    pub net_outflows: Q,
    pub liquid_assets: Q,
    /// Liquid assets needed to keep the surplus unchanged: the liquidity
    /// cost of the position, per million (negative: it frees liquidity).
    pub liquidity_cost: Q,
}

/// Exact one-sided marginals at the current return, including any cap that
/// binds (the 40% HQLA2 and 15% HQLA2B caps, the 75% inflow cap).
pub fn marginals(inputs: &BTreeMap<String, Q>, items: &[&str]) -> Result<Vec<Marginal>, String> {
    let base = lcr_of(inputs)?;
    let mut out = Vec::new();
    for item in items {
        let mut bumped = inputs.clone();
        *bumped.entry(item.to_string()).or_default() += Q::from_integer(1.into());
        let x = lcr_of(&bumped)?;
        let d_out = &x.net_outflows - &base.net_outflows;
        let d_liq = &x.liquid_assets - &base.liquid_assets;
        out.push(Marginal { item: item.to_string(), liquidity_cost: &d_out - &d_liq, net_outflows: d_out, liquid_assets: d_liq });
    }
    Ok(out)
}
