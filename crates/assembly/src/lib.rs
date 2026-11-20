//! Return assembly: every module reports per-record contributions
//! (record, item, AUD amount); this crate tags them with the record's
//! currency, sums them into ARF 210.1A inputs, and fills the currency split
//! of total outflows and inflows (items 18.1-18.8, 23.1-23.8).
//!
//! The split is computed by evaluating the form over each currency's
//! contributions alone (ARS Instructions pp. 55-56, 61-62: "by underlying
//! currency exposure"), so non-linear items such as 15.8 are worked out within
//! the currency. Derivative cash flows use the ARS currency rule (FX funding
//! swaps gross, FX trading excluded) rather than the netted totals; the form
//! therefore states the tie-out to items 18 and 23 as a warning, not an error.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use prudence_core::uncertain::Mode;
use prudence_core::{Evaluation, Missing, Q, evaluate};

/// Which totals a contribution feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    /// The item totals and the currency split.
    Both,
    /// Only the item totals (netted derivative flows).
    Total,
    /// Only the currency split (derivative flows under the ARS currency rule).
    Split,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub module: &'static str,
    pub record: String,
    pub item: String,
    pub currency: String,
    /// AUD (or AUD equivalent).
    pub amount: Q,
    pub stream: Stream,
}

/// The memo row (1-8) for a currency.
pub fn memo_row(ccy: &str) -> usize {
    match ccy {
        "AUD" => 1,
        "NZD" => 2,
        "USD" => 3,
        "GBP" => 4,
        "EUR" => 5,
        "JPY" => 6,
        "RMB" | "CNY" => 7,
        _ => 8,
    }
}

fn million() -> Q {
    Q::from_integer(1_000_000.into())
}

fn eval(inputs: &BTreeMap<String, Q>) -> Result<Evaluation<'static>, String> {
    evaluate(arf210::lcr_all(), inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).map_err(|e| format!("{e:?}"))
}

/// ARF 210.1A inputs (AUD millions) from contributions, plus `rates`: cells
/// such as "16.4:weight" that apply to every currency.
pub fn assemble(parts: &[Part], rates: &BTreeMap<String, Q>) -> Result<BTreeMap<String, Q>, String> {
    if let Some(k) = rates.keys().find(|k| !k.contains(':')) {
        return Err(format!("`{k}` is an amount: report it as a Part so it has a currency"));
    }
    let mut inputs = rates.clone();
    let mut by_ccy: BTreeMap<usize, BTreeMap<String, Q>> = BTreeMap::new();
    for p in parts {
        let q = &p.amount / million();
        if p.stream != Stream::Split {
            *inputs.entry(p.item.clone()).or_default() += q.clone();
        }
        if p.stream != Stream::Total {
            let m = by_ccy.entry(memo_row(&p.currency)).or_insert_with(|| rates.clone());
            *m.entry(p.item.clone()).or_default() += q;
        }
    }
    for row in 1..=8 {
        let (mut out, mut out_w, mut inn, mut inn_w) = Default::default();
        if let Some(m) = by_ccy.get(&row) {
            let ev = eval(m)?;
            (out, out_w, inn, inn_w) =
                (ev.q("18").clone(), ev.q("18:weighted").clone(), ev.q("23").clone(), ev.q("23:weighted").clone());
        }
        inputs.insert(format!("18.{row}"), out);
        inputs.insert(format!("18.{row}:weighted"), out_w);
        inputs.insert(format!("23.{row}"), inn);
        inputs.insert(format!("23.{row}:weighted"), inn_w);
    }
    Ok(inputs)
}

/// Everything one ADI reports, by module.
#[derive(Default)]
pub struct Bank {
    pub retail: deposits::Book,
    pub wholesale_customers: Vec<deposits::wholesale::Customer>,
    pub wholesale_deposits: Vec<deposits::wholesale::Deposit>,
    pub holdings: Vec<secured::Holding>,
    pub repos: Vec<secured::Repo>,
    pub has_clf: bool,
    pub commitments: obligations::Book,
    pub loans: Vec<loans::Loan>,
    pub derivatives: derivatives::Book,
    /// Record id -> currency (default AUD).
    pub currency: BTreeMap<String, String>,
}

pub struct Return {
    pub parts: Vec<Part>,
    /// ARF 210.1A inputs, AUD millions.
    pub inputs: BTreeMap<String, Q>,
    /// (module, subject, attribute) -> (assumed value, reason).
    pub assumptions: Vec<(&'static str, String, String, String, String)>,
}

/// Run every module and assemble ARF 210.1A.
pub fn lcr_return(bank: &Bank, reporting_date: NaiveDate, mode: Mode) -> Result<Return, String> {
    let ccy = |id: &str| bank.currency.get(id).cloned().unwrap_or_else(|| "AUD".into());
    let mut parts = Vec::new();
    let mut assumptions = Vec::new();
    let mut push = |module: &'static str, record: &str, item: &str, amount: Q, currency: String, stream| {
        parts.push(Part { module, record: record.into(), item: item.into(), currency, amount, stream });
    };

    let c = deposits::classify(&bank.retail, &deposits::Params { mode, ..deposits::Params::new(reporting_date) })
        .map_err(|e| format!("{e:?}"))?;
    for s in &c.slices {
        if let Some(i) = s.lcr_item {
            push("retail", &s.account_id, i, s.amount.amount().clone(), ccy(&s.account_id), Stream::Both);
        }
    }
    for a in &c.assumptions {
        assumptions.push(("retail", a.subject.clone(), a.attribute.to_string(), a.assumed.clone(), a.reason.clone()));
    }
    for s in deposits::wholesale::run(&bank.wholesale_customers, &bank.wholesale_deposits, reporting_date, mode)? {
        if let Some(i) = &s.lcr_item {
            push("wholesale", &s.account, i, s.amount.clone(), ccy(&s.account), Stream::Both);
        }
    }
    let r = secured::run(&bank.holdings, &bank.repos, reporting_date, mode, bank.has_clf)?;
    for (id, i, q) in r.parts {
        push("secured", &id, &i, q, ccy(&id), Stream::Both);
    }
    let mut keep = |module: &'static str, a: BTreeMap<(String, String), (String, String)>| {
        for ((s, at), (v, why)) in a {
            assumptions.push((module, s, at, v, why));
        }
    };
    keep("secured", r.assumptions);
    let r = obligations::run(&bank.commitments, reporting_date, mode)?;
    for (id, i, q) in r.parts {
        push("obligations", &id, &i, q, ccy(&id), Stream::Both);
    }
    keep("obligations", r.assumptions);
    let r = loans::run(&bank.loans, reporting_date, mode)?;
    for (id, i, q) in r.parts {
        push("loans", &id, &i, q, ccy(&id), Stream::Both);
    }
    keep("loans", r.assumptions);
    let r = derivatives::run(&bank.derivatives, reporting_date, reporting_date, mode)?;
    for (id, i, q) in r.parts {
        let stream = if i == "13.1" || i == "21.1" { Stream::Total } else { Stream::Both };
        push("derivatives", &id, &i, q, ccy(&id), stream);
    }
    for (id, i, c, q) in r.by_currency {
        push("derivatives", &id, &i, q, c, Stream::Split);
    }
    keep("derivatives", r.assumptions);
    let inputs = assemble(&parts, &BTreeMap::new())?;
    Ok(Return { parts, inputs, assumptions })
}
