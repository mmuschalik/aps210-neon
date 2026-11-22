//! Steel thread: one synthetic ADI, generated from a seed, run end to end
//! through every Prudence module, all eight ARF 210 forms and the trusted
//! gate, to a content-addressed APS 310 bundle; plus the capability spikes
//! (`spikes`) showing the foundations carry what design.txt still asks for.

pub mod macquarie;
pub mod synth;
pub mod spikes;
pub mod sql_export;
pub mod ui;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use assembly::{Bank, Return};
use chrono::NaiveDate;
use prudence_core::uncertain::Mode;
use prudence_core::{Missing, Q, evaluate};

pub use synth::{Config, generate};

/// The headline numbers of one return.
#[derive(Clone, Debug, PartialEq)]
pub struct Ratios {
    pub hqla: Q,
    pub outflows: Q,
    pub inflows_capped: Q,
    pub nco: Q,
    /// Percent; `None` if NCO is zero.
    pub lcr: Option<Q>,
    pub asf: Q,
    pub rsf: Q,
    /// A ratio (not percent); `None` if RSF is zero.
    pub nsfr: Option<Q>,
}

pub fn lcr_eval(inputs: &BTreeMap<String, Q>) -> prudence_core::Evaluation<'static> {
    evaluate(arf210::lcr_all(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).expect("ARF 210.1A evaluates")
}

pub fn ratios(ret: &Return) -> Result<Ratios, String> {
    let l = evaluate(arf210::lcr_all(), ret.inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero)
        .map_err(|e| format!("ARF 210.1A: {e:?}"))?;
    let n = evaluate(arf210::nsfr(), ret.nsfr_inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero)
        .map_err(|e| format!("ARF 210.6: {e:?}"))?;
    Ok(Ratios {
        hqla: l.q("25").clone(),
        outflows: l.q("27").clone(),
        inflows_capped: l.q("28").clone(),
        nco: l.q("29").clone(),
        lcr: l.get("30").num().cloned(),
        asf: n.q("19").clone(),
        rsf: n.q("47") + n.q("55"),
        nsfr: n.get("56").num().cloned(),
    })
}

pub fn context(cfg: &Config, regime: &str) -> checker::ReportingContext {
    checker::ReportingContext {
        entity: format!("synthetic-adi-{}", cfg.seed),
        reporting_date: cfg.reporting_date.to_string(),
        consolidation: "level1".into(),
        regime: regime.into(),
    }
}

/// Everything one run produced, with the time each stage took.
pub struct Thread {
    pub cfg: Config,
    pub bank: Bank,
    pub ret: Return,
    pub package: checker::Package,
    pub report: checker::Report,
    /// `Err` if a return cannot be evaluated (the gate then fails too).
    pub ratios: Result<Ratios, String>,
    pub timings: Vec<(&'static str, Duration)>,
}

impl Thread {
    pub fn bundle_id(&self) -> Option<&str> {
        self.report.bundle.as_ref().and_then(|b| b["bundle_id"].as_str())
    }
}

/// Generate, run every module, assemble every return, and gate it.
pub fn run(cfg: &Config, regime: &str) -> Result<Thread, String> {
    let mut timings = Vec::new();
    let mut t = Instant::now();
    let mut lap = |name, timings: &mut Vec<_>| {
        timings.push((name, t.elapsed()));
        t = Instant::now();
    };
    let bank = generate(cfg);
    let extras = synth::extras(cfg);
    lap("generate", &mut timings);
    let ret = assembly::lcr_return(&bank, cfg.reporting_date, Mode::Conservative)?;
    lap("modules + ARF 210.1A/210.6", &mut timings);
    let package = assembly::package_all(&bank, &ret, context(cfg, regime), cfg.reporting_date, Mode::Conservative, &extras)?;
    lap("other returns (1B, 2, 3.x, 4, 5)", &mut timings);
    let report = checker::check_complete(&package);
    lap("trusted gate + bundle", &mut timings);
    let ratios = ratios(&ret);
    Ok(Thread { cfg: cfg.clone(), bank, ret, package, report, ratios, timings })
}

/// Record counts by book.
pub fn census(b: &Bank) -> Vec<(&'static str, usize)> {
    vec![
        ("retail/SME customers", b.retail.customers.len()),
        ("retail/SME accounts", b.retail.accounts.len()),
        ("deposit events", b.retail.ledger.as_of(NaiveDate::MAX, NaiveDate::MAX).count()),
        ("wholesale deposits", b.wholesale_deposits.len()),
        ("securities held", b.holdings.len()),
        ("repos / reverse repos", b.repos.len()),
        ("loans", b.loans.len()),
        ("receivables", b.other_inflows.receivables.len()),
        ("facilities", b.commitments.facilities.len()),
        ("commitments + contingents", b.commitments.commitments.len() + b.commitments.obligations.len()),
        ("netting sets", b.derivatives.sets.len()),
        ("derivative trades + flows", b.derivatives.trades.len() + b.derivatives.flows.len()),
        ("collateral flow events", b.derivatives.collateral_flows.len()),
        ("issued + capital + placements", b.funding.securities.len() + b.funding.capital.len() + b.funding.placements.len()),
    ]
}

pub fn pct(q: &Option<Q>) -> String {
    q.as_ref().map(|q| prudence_core::num::to_decimal_string(q, 1) + "%").unwrap_or_else(|| "undefined".into())
}

pub fn m(q: &Q) -> String {
    prudence_core::num::to_decimal_string(q, 2) + "m"
}

/// Each module run on its own, timed: where the engine spends its time.
pub fn profile(b: &Bank, rd: NaiveDate) -> Result<Vec<(&'static str, usize, Duration)>, String> {
    let mode = Mode::Conservative;
    let mut out = Vec::new();
    let t = Instant::now();
    deposits::classify(&b.retail, &deposits::Params { mode, ..deposits::Params::new(rd) }).map_err(|e| format!("{e:?}"))?;
    out.push(("retail/SME deposits", b.retail.accounts.len(), t.elapsed()));
    let t = Instant::now();
    deposits::wholesale::run(&b.wholesale_customers, &b.wholesale_deposits, rd, mode)?;
    out.push(("wholesale deposits", b.wholesale_deposits.len(), t.elapsed()));
    let t = Instant::now();
    secured::run(&b.holdings, &b.repos, rd, mode, b.has_clf)?;
    out.push(("secured (holdings, repos, unwind)", b.holdings.len() + b.repos.len(), t.elapsed()));
    let t = Instant::now();
    loans::run_all(&b.loans, &b.other_inflows, rd, mode)?;
    out.push(("loans", b.loans.len(), t.elapsed()));
    let t = Instant::now();
    obligations::run(&b.commitments, rd, mode)?;
    out.push(("off-balance sheet", b.commitments.facilities.len(), t.elapsed()));
    let t = Instant::now();
    derivatives::run(&b.derivatives, rd, rd, mode)?;
    out.push(("derivatives", b.derivatives.flows.len(), t.elapsed()));
    let t = Instant::now();
    funding::run(&b.funding, rd, mode)?;
    out.push(("funding", b.funding.securities.len(), t.elapsed()));
    Ok(out)
}
