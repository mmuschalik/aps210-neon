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

/// The NSFR numerator and denominator of one ARF 210.6 (AUD millions).
#[derive(Clone, Debug, PartialEq)]
pub struct Nsfr {
    /// Total available stable funding (item 19).
    pub asf: Q,
    /// Total required stable funding (items 47 + 55).
    pub rsf: Q,
    /// Item 56, undefined without RSF.
    pub nsfr: Option<Q>,
}

impl Nsfr {
    /// ASF above what a 100% NSFR needs.
    pub fn surplus(&self) -> Q {
        &self.asf - &self.rsf
    }
}

pub fn nsfr_of(inputs: &BTreeMap<String, Q>) -> Result<Nsfr, String> {
    let ev = evaluate(arf210::nsfr(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero)
        .map_err(|e| format!("{e:?}"))?;
    let g = |k: &str| ev.get(k).num().cloned().unwrap_or_default();
    Ok(Nsfr {
        asf: g("19:calc"),
        rsf: g("47:calc") + g("55:calc"),
        nsfr: ev.get("56:value").num().cloned(),
    })
}

// ---------------------------------------------------------------- stress

/// APS 210 para 25 scenario classification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScenarioKind {
    Idiosyncratic,
    MarketWide,
    Combined,
    #[default]
    Custom,
}

/// A stress scenario: what happens to the ADI's data. The statutory rules
/// then apply unchanged (design 4.C.3: behavioural assumptions stay outside
/// the statutory modules).
#[derive(Clone, Debug, Default)]
pub struct Scenario {
    pub name: String,
    pub kind: ScenarioKind,
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
    /// Idiosyncratic 3-notch ADI downgrade: triggers derivative downgrade
    /// collateral calls into variation margin posted and realised flows.
    pub adi_downgrade: bool,
    /// Market-wide corporate rating migration: degrades non-financial corporate
    /// debt holdings by this many ECAI rating grades.
    pub corporate_rating_downgrade: i64,
    /// Fraction (0..=1) of wholesale operational deposit balances that lose
    /// operational status and revert to excess/non-operational funding.
    pub operational_erosion: Q,
    /// Fraction (0..=1) of committed undrawn facilities drawn immediately,
    /// funded out of unencumbered liquid holdings (cash -> CB -> HQLA1).
    pub facility_drawdown_shock: Q,
    /// Fraction (0..=1) of retail/SME deposit balances withdrawn immediately,
    /// funded out of unencumbered liquid holdings.
    pub retail_runoff_shock: Q,
    /// Market-wide FX swap / cross-currency funding market closure: removes
    /// receivable FX principal derivative inflows.
    pub fx_swap_market_closed: bool,
}

/// Full LCR + NSFR outcome of one stress scenario.
#[derive(Clone, Debug, PartialEq)]
pub struct StressResult {
    pub name: String,
    pub kind: ScenarioKind,
    pub horizon_days: u64,
    pub lcr: Lcr,
    pub nsfr: Nsfr,
}

/// Survival-horizon scan (APS 210 para 26): days until LCR surplus turns negative.
#[derive(Clone, Debug, PartialEq)]
pub struct SurvivalHorizon {
    pub scenario: String,
    pub max_horizon_days: u64,
    /// First day in `0..=max_horizon_days` where `lcr.surplus() < 0`, if any.
    pub breach_day: Option<u64>,
    pub min_surplus: Q,
    pub min_lcr: Option<Q>,
}

/// Standard APS 210 paras 24-29 scenario suite: Idiosyncratic (30d),
/// Market-Wide (30d), Combined Acute (30d), and Combined Protracted (90d).
pub fn aps210_standard_suite() -> Vec<Scenario> {
    let pct = |n: i64| Q::new(n.into(), 100.into());
    vec![
        Scenario {
            name: "Idiosyncratic (30d acute)".into(),
            kind: ScenarioKind::Idiosyncratic,
            call_notice_deposits: true,
            adi_downgrade: true,
            operational_erosion: pct(50),
            retail_runoff_shock: pct(10),
            roll_forward_days: 30,
            ..Default::default()
        },
        Scenario {
            name: "Market-wide (30d acute)".into(),
            kind: ScenarioKind::MarketWide,
            price_shock: [pct(5), pct(15), pct(25)],
            freeze_wholesale_rollover: true,
            corporate_rating_downgrade: 1,
            facility_drawdown_shock: pct(20),
            fx_swap_market_closed: true,
            roll_forward_days: 30,
            ..Default::default()
        },
        Scenario {
            name: "Combined (30d acute)".into(),
            kind: ScenarioKind::Combined,
            price_shock: [pct(5), pct(15), pct(25)],
            call_notice_deposits: true,
            freeze_wholesale_rollover: true,
            adi_downgrade: true,
            corporate_rating_downgrade: 1,
            operational_erosion: pct(50),
            facility_drawdown_shock: pct(25),
            retail_runoff_shock: pct(10),
            fx_swap_market_closed: true,
            roll_forward_days: 30,
        },
        Scenario {
            name: "Combined (90d protracted)".into(),
            kind: ScenarioKind::Combined,
            price_shock: [pct(10), pct(20), pct(30)],
            call_notice_deposits: true,
            freeze_wholesale_rollover: true,
            adi_downgrade: true,
            corporate_rating_downgrade: 2,
            operational_erosion: pct(75),
            facility_drawdown_shock: pct(35),
            retail_runoff_shock: pct(15),
            fx_swap_market_closed: true,
            roll_forward_days: 90,
        },
    ]
}

/// Drain `cash_needed` AUD from unencumbered LCR-eligible liquid holdings in
/// strict liquidity order: NotesAndCoin -> drawable central bank balances ->
/// HQLA1 securities -> HQLA2A -> HQLA2B. Non-HQLA holdings are NOT liquidated.
/// Returns any unfunded cash shortfall (`> 0` means the bank is illiquid).
fn drain_liquid_holdings(holdings: &mut [secured::Holding], base_parts: &[crate::Part], mut cash_needed: Q) -> Q {
    let zero = Q::default();
    if cash_needed <= zero {
        return zero;
    }
    let priority = |h: &secured::Holding| -> Option<u8> {
        if h.encumbered || h.market_value <= zero {
            return None;
        }
        match h.kind {
            secured::AssetKind::NotesAndCoin => Some(0),
            secured::AssetKind::RbaBalance | secured::AssetKind::ForeignCentralBankBalance => {
                if h.drawable_in_stress == prudence_core::uncertain::Uncertain::Known(true) {
                    Some(1)
                } else {
                    None
                }
            }
            _ => {
                let item = base_parts
                    .iter()
                    .find(|p| p.record == h.id && p.module == "secured")
                    .map(|p| p.item.as_str());
                match item.and_then(|s| s.split('.').next()) {
                    Some("1") => Some(2),
                    Some("2") => Some(3),
                    Some("3") => Some(4),
                    _ => None, // Non-HQLA is not liquidated at par in acute stress
                }
            }
        }
    };
    let mut order: Vec<(u8, usize)> = holdings
        .iter()
        .enumerate()
        .filter_map(|(i, h)| priority(h).map(|p| (p, i)))
        .collect();
    order.sort_by_key(|&(p, i)| (p, i));
    for (_, i) in order {
        if cash_needed <= zero {
            break;
        }
        let h = &mut holdings[i];
        let take = h.market_value.clone().min(cash_needed.clone());
        let ratio = (&h.market_value - &take) / &h.market_value;
        h.market_value -= &take;
        h.value_after_rba_margin = &h.value_after_rba_margin * &ratio;
        cash_needed -= take;
    }
    cash_needed
}

fn borrower_from_obligation(c: obligations::Counterparty) -> loans::Borrower {
    match c {
        obligations::Counterparty::RetailCustomer => loans::Borrower::RetailCustomer,
        obligations::Counterparty::Sme => loans::Borrower::Sme,
        obligations::Counterparty::NonFinancialCorporate => loans::Borrower::NonFinancialCorporate,
        obligations::Counterparty::RegulatedBank | obligations::Counterparty::OtherFinancialInstitution => {
            loans::Borrower::FinancialInstitution
        }
        obligations::Counterparty::SovereignCentralBankPseMdb | obligations::Counterparty::OtherLegalEntity => {
            loans::Borrower::OtherEntity
        }
    }
}

/// Apply `s` to `bank`, returning `(shocked_bank, stressed_date, unfunded_cash_shortfall_aud)`.
pub fn apply_with_shortfall(bank: &Bank, rd: NaiveDate, s: &Scenario) -> Result<(Bank, NaiveDate, Q), String> {
    let mut b = bank.clone();
    let zero = Q::default();
    let one = Q::from_integer(1.into());
    let need_base = s.price_shock.iter().any(|q| *q > zero)
        || s.adi_downgrade
        || s.facility_drawdown_shock > zero
        || s.retail_runoff_shock > zero;
    let base_parts = if need_base {
        lcr_return(bank, rd, Mode::Conservative)?.parts
    } else {
        Vec::new()
    };
    if s.price_shock.iter().any(|q| *q > zero) {
        for h in &mut b.holdings {
            // Do not price-shock domestic cash or central bank reserves (items 1.1 / 1.2).
            if matches!(
                h.kind,
                secured::AssetKind::NotesAndCoin
                    | secured::AssetKind::RbaBalance
                    | secured::AssetKind::ForeignCentralBankBalance
            ) {
                continue;
            }
            let class = base_parts
                .iter()
                .find(|p| p.record == h.id && p.module == "secured")
                .map(|p| p.item.split('.').next().unwrap_or("").to_string());
            let k = match class.as_deref() {
                Some("1") => 0,
                Some("2") => 1,
                Some("3") => 2,
                _ => continue,
            };
            let shock = s.price_shock[k].clone().max(zero.clone()).min(one.clone());
            let keep = &one - &shock;
            h.market_value = &h.market_value * &keep;
            h.value_after_rba_margin = &h.value_after_rba_margin * &keep;
        }
    }
    if s.corporate_rating_downgrade > 0 {
        for h in &mut b.holdings {
            if h.issuer == secured::Issuer::NonFinancialCorporateIssuer {
                h.rating_grade += s.corporate_rating_downgrade;
            }
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
    if s.operational_erosion > zero {
        let keep = (&one - &s.operational_erosion).max(zero.clone());
        for d in &mut b.wholesale_deposits {
            if let prudence_core::uncertain::Uncertain::Known(op) = &mut d.operational_balance {
                *op = &*op * &keep;
            }
        }
    }
    let mut cash_drain = Q::default();
    if s.adi_downgrade {
        for set in &mut b.derivatives.sets {
            if set.downgrade_3_notches > zero {
                // Triggered once: drain cash, post as initial margin (85% RSF, Att C para 36(a)),
                // and zero downgrade_3_notches so LCR item 13.2 does not double-count the call.
                let call = std::mem::take(&mut set.downgrade_3_notches);
                set.initial_margin_posted += &call;
                cash_drain += call;
            }
        }
    }
    if s.facility_drawdown_shock > zero {
        let frac = s.facility_drawdown_shock.clone().min(one.clone());
        let mut drawn_loans = Vec::new();
        for f in &mut b.commitments.facilities {
            if f.committed && f.undrawn > zero {
                let drawn = &f.undrawn * &frac;
                f.undrawn -= &drawn;
                cash_drain += &drawn;
                // Balance-sheet conservation: drawn facility becomes an on-balance-sheet
                // term loan (maturing >= 1y -> 85% RSF in NSFR, 0 inflow in 30d LCR).
                drawn_loans.push(loans::Loan {
                    id: format!("{}-drawn", f.id),
                    borrower: borrower_from_obligation(f.counterparty),
                    affiliation: loans::Affiliation::External,
                    product: loans::Product::OtherLoan,
                    contract: loans::Contract::Pam { maturity: rd + Days::new(400) },
                    balance: drawn,
                    rate: Q::new(6.into(), 100.into()),
                    next_payment: rd + Days::new(400),
                    cycle_months: 12,
                    performing: prudence_core::uncertain::Uncertain::Known(true),
                    lvr_at_most_80: prudence_core::uncertain::Uncertain::Known(false),
                    risk_weight: 100,
                    encumbered_until: None,
                });
            }
        }
        b.loans.extend(drawn_loans);
    }
    if s.retail_runoff_shock > zero {
        let frac = s.retail_runoff_shock.clone().min(one.clone());
        for a in &mut b.retail.accounts {
            let bal = a.balance.amount().clone();
            if bal > zero {
                let withdrawn = &bal * &frac;
                a.balance = prudence_core::money::Money::new(&bal - &withdrawn);
                cash_drain += withdrawn;
            }
        }
    }
    let shortfall = if cash_drain > zero {
        drain_liquid_holdings(&mut b.holdings, &base_parts, cash_drain)
    } else {
        zero.clone()
    };
    if s.fx_swap_market_closed {
        for f in &mut b.derivatives.flows {
            if f.fx_principal && f.amount > zero {
                f.amount = Q::default();
            }
        }
    }
    Ok((b, rd + Days::new(s.roll_forward_days), shortfall))
}

pub fn apply(bank: &Bank, rd: NaiveDate, s: &Scenario) -> Result<(Bank, NaiveDate), String> {
    let (b, d, _) = apply_with_shortfall(bank, rd, s)?;
    Ok((b, d))
}

/// The LCR under each scenario, with the base first.
pub fn stress(bank: &Bank, rd: NaiveDate, scenarios: &[Scenario]) -> Result<Vec<(String, Lcr)>, String> {
    let mut out = vec![("base".to_string(), lcr_of(&lcr_return(bank, rd, Mode::Conservative)?.inputs)?)];
    for s in scenarios {
        let (b, d, shortfall) = apply_with_shortfall(bank, rd, s)?;
        let mut lcr = lcr_of(&lcr_return(&b, d, Mode::Conservative)?.inputs)?;
        if shortfall > Q::default() {
            lcr.liquid_assets -= &shortfall / Q::from_integer(1_000_000.into());
        }
        out.push((s.name.clone(), lcr));
    }
    Ok(out)
}

/// Full LCR + NSFR stress results for Base plus each scenario (APS 210 paras 24-29).
pub fn stress_full(bank: &Bank, rd: NaiveDate, scenarios: &[Scenario]) -> Result<Vec<StressResult>, String> {
    let base_ret = lcr_return(bank, rd, Mode::Conservative)?;
    let mut out = vec![StressResult {
        name: "base".into(),
        kind: ScenarioKind::Custom,
        horizon_days: 0,
        lcr: lcr_of(&base_ret.inputs)?,
        nsfr: nsfr_of(&base_ret.nsfr_inputs)?,
    }];
    let million = Q::from_integer(1_000_000.into());
    for s in scenarios {
        let (b, d, shortfall) = apply_with_shortfall(bank, rd, s)?;
        let ret = lcr_return(&b, d, Mode::Conservative)?;
        let mut lcr = lcr_of(&ret.inputs)?;
        if shortfall > Q::default() {
            lcr.liquid_assets -= &shortfall / &million;
        }
        out.push(StressResult {
            name: s.name.clone(),
            kind: s.kind,
            horizon_days: s.roll_forward_days,
            lcr,
            nsfr: nsfr_of(&ret.nsfr_inputs)?,
        });
    }
    Ok(out)
}

/// Compute the liquidity survival horizon under `scenario` by rolling forward
/// from day `0` to `max_days` in steps of `step_days` (minimum step 1).
pub fn survival_horizon(
    bank: &Bank,
    rd: NaiveDate,
    scenario: &Scenario,
    max_days: u64,
    step_days: u64,
) -> Result<SurvivalHorizon, String> {
    let step = step_days.max(1);
    let mut base_s = scenario.clone();
    base_s.roll_forward_days = 0;
    let (shocked_bank, _, shortfall) = apply_with_shortfall(bank, rd, &base_s)?;
    let shortfall_m = &shortfall / Q::from_integer(1_000_000.into());
    let zero = Q::default();
    let mut breach_day = None;
    let mut min_surplus: Option<Q> = None;
    let mut min_lcr: Option<Q> = None;

    let mut days: Vec<u64> = (0..=max_days).step_by(step as usize).collect();
    if days.last().copied() != Some(max_days) {
        days.push(max_days);
    }
    for day in days {
        let ret = lcr_return(&shocked_bank, rd + Days::new(day), Mode::Conservative)?;
        let mut lcr = lcr_of(&ret.inputs)?;
        if shortfall_m > zero {
            lcr.liquid_assets -= &shortfall_m;
        }
        let surplus = lcr.surplus();
        if breach_day.is_none() && surplus < zero {
            breach_day = Some(day);
        }
        if min_surplus.as_ref().is_none_or(|m| surplus < *m) {
            min_surplus = Some(surplus);
        }
        if let Some(r) = &lcr.lcr
            && min_lcr.as_ref().is_none_or(|m| r < m)
        {
            min_lcr = Some(r.clone());
        }
    }
    Ok(SurvivalHorizon {
        scenario: scenario.name.clone(),
        max_horizon_days: max_days,
        breach_day,
        min_surplus: min_surplus.unwrap_or_default(),
        min_lcr,
    })
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
