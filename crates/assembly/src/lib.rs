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

pub mod maturity;

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
            (out, out_w, inn, inn_w) = (ev.q("18").clone(), ev.q("18:weighted").clone(), ev.q("23").clone(), ev.q("23:weighted").clone());
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
    /// Other contractual inflows (21.4) and head office support (21.5).
    pub other_inflows: loans::Other,
    pub derivatives: derivatives::Book,
    /// Record id -> currency (default AUD).
    pub currency: BTreeMap<String, String>,
    /// The general ledger: account -> balance (AUD).
    pub gl: BTreeMap<String, Q>,
    /// Record id -> ledger account, overriding `default_account`.
    pub gl_account: BTreeMap<String, String>,
}

/// The ledger account a balance-sheet record posts to by default.
pub fn default_account(kind: &str) -> &'static str {
    match kind {
        "retail" => "2100 Retail and SME deposits",
        "wholesale" => "2200 Wholesale deposits",
        "holding" => "1200 Securities",
        "repo-funding" => "2300 Repurchase agreements",
        "repo-lending" => "1300 Reverse repurchase agreements",
        "loan" => "1400 Loans and advances",
        "receivable" => "1600 Other receivables",
        _ => "1500 Derivatives (net mark-to-market)",
    }
}

pub struct Return {
    pub parts: Vec<Part>,
    /// ARF 210.1A inputs, AUD millions.
    pub inputs: BTreeMap<String, Q>,
    /// ARF 210.6 inputs from the modules that model it, AUD millions.
    pub nsfr_inputs: BTreeMap<String, Q>,
    pub nsfr_parts: Vec<checker::FormContribution>,
    /// Balance-sheet records: (id, kind, carrying amount, split?).
    pub records: Vec<(String, &'static str, Q, bool)>,
    /// (record, amount) per slice of split records.
    pub slices: Vec<(String, Q)>,
    pub certificates: Vec<checker::Certificate>,
    pub allocation_slices: Vec<checker::AllocationSlice>,
    /// (module, subject, attribute) -> (assumed value, reason).
    pub assumptions: Vec<(&'static str, String, String, String, String)>,
}

/// Run every module and assemble ARF 210.1A.
pub fn lcr_return(bank: &Bank, reporting_date: NaiveDate, mode: Mode) -> Result<Return, String> {
    let ccy = |id: &str| bank.currency.get(id).cloned().unwrap_or_else(|| "AUD".into());
    let mut parts = Vec::new();
    let mut assumptions = Vec::new();
    let mut push = |module: &'static str, record: &str, item: &str, amount: Q, currency: String, stream| {
        parts.push(Part {
            module,
            record: record.into(),
            item: item.into(),
            currency,
            amount,
            stream,
        });
    };

    let mut nsfr: BTreeMap<String, Q> = BTreeMap::new();
    let mut nsfr_parts = Vec::new();
    let mut nsfr_add = |source: &str, item: &str, col: &str, q: &Q| {
        let k = if col == "amount" { item.to_string() } else { format!("{item}:{col}") };
        nsfr_parts.push(checker::FormContribution {
            source: source.into(),
            cell: k.clone(),
            amount: q / million(),
        });
        *nsfr.entry(k).or_default() += q / million();
    };
    let mut records = Vec::new();
    let mut slices = Vec::new();
    let mut certificates = Vec::new();
    let mut allocation_slices = Vec::new();
    let mut certify = |module: &str, allocs: Vec<prudence_core::interp::Allocation>| {
        for a in allocs {
            certificates.push(checker::Certificate {
                subjects: a.subjects,
                fcs: None,
                name: format!("{module}/{}/{}", a.alloc, a.group),
                lp: a.lp,
                solution: a.solution,
            });
        }
    };

    let c = deposits::classify(
        &bank.retail,
        &deposits::Params {
            mode,
            ..deposits::Params::new(reporting_date)
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    for a in &bank.retail.accounts {
        records.push((a.id.clone(), "retail", a.balance.amount().clone(), true));
    }
    let allocated: std::collections::BTreeSet<_> = c.allocations.iter().flat_map(|a| a.subjects.iter()).collect();
    for s in &c.slices {
        if allocated.contains(&s.account_id) {
            allocation_slices.push(checker::AllocationSlice {
                source: s.account_id.clone(),
                amount: s.amount.amount().clone(),
                covered: s.covered,
                lcr_item: s.lcr_item.map(str::to_string),
                nsfr_cell: format!("{}:{}", s.nsfr_item, s.nsfr_bucket),
            });
        }
        slices.push((s.account_id.clone(), s.amount.amount().clone()));
        nsfr_add(&s.account_id, s.nsfr_item, s.nsfr_bucket, s.amount.amount());
        if let Some(i) = s.lcr_item {
            push("retail", &s.account_id, i, s.amount.amount().clone(), ccy(&s.account_id), Stream::Both);
        }
    }
    certify("retail", c.allocations);
    for a in &c.assumptions {
        assumptions.push(("retail", a.subject.clone(), a.attribute.to_string(), a.assumed.clone(), a.reason.clone()));
    }
    let (ws, allocs) = deposits::wholesale::run_with_certificates(&bank.wholesale_customers, &bank.wholesale_deposits, reporting_date, mode)?;
    certify("wholesale", allocs);
    for d in &bank.wholesale_deposits {
        records.push((d.id.clone(), "wholesale", d.balance.clone(), true));
    }
    for s in ws {
        allocation_slices.push(checker::AllocationSlice {
            source: s.account.clone(),
            amount: s.amount.clone(),
            covered: s.covered,
            lcr_item: s.lcr_item.clone(),
            nsfr_cell: format!("{}:{}", s.nsfr.0, s.nsfr.1),
        });
        slices.push((s.account.clone(), s.amount.clone()));
        nsfr_add(&s.account, &s.nsfr.0, &s.nsfr.1, &s.amount);
        if let Some(i) = &s.lcr_item {
            push("wholesale", &s.account, i, s.amount.clone(), ccy(&s.account), Stream::Both);
        }
    }
    for h in &bank.holdings {
        records.push((h.id.clone(), "holding", h.market_value.clone(), false));
    }
    for r in &bank.repos {
        let kind = if r.leg == secured::Leg::Funding { "repo-funding" } else { "repo-lending" };
        records.push((r.id.clone(), kind, r.cash.clone(), false));
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
    let r = loans::run_all(&bank.loans, &bank.other_inflows, reporting_date, mode)?;
    for l in &bank.loans {
        records.push((l.id.clone(), "loan", l.balance.clone(), true));
    }
    for x in &bank.other_inflows.receivables {
        records.push((x.id.clone(), "receivable", x.amount.clone(), false));
    }
    for (id, item, bucket, q) in &r.slices {
        slices.push((id.clone(), q.clone()));
        nsfr_add(id, item, bucket, q);
    }
    for (id, i, q) in r.parts {
        push("loans", &id, &i, q, ccy(&id), Stream::Both);
    }
    keep("loans", r.assumptions);
    let r = derivatives::run(&bank.derivatives, reporting_date, reporting_date, mode)?;
    for (k, t) in bank.derivatives.trades.iter().enumerate() {
        records.push((format!("trade{k}"), "derivative", t.mtm.clone(), false));
    }
    for (id, item, q) in &r.nsfr_parts {
        nsfr_add(id, item, "amount", q);
    }
    for (id, i, q) in r.parts {
        let stream = if ["13.1", "21.1", "13.6"].contains(&i.as_str()) { Stream::Total } else { Stream::Both };
        push("derivatives", &id, &i, q, ccy(&id), stream);
    }
    for (id, i, c, q) in r.by_currency {
        push("derivatives", &id, &i, q, c, Stream::Split);
    }
    keep("derivatives", r.assumptions);
    let resolved_bool = |v: &prudence_core::uncertain::Uncertain<bool>| v.known().copied().unwrap_or(mode == Mode::Optimistic);
    for cert in &mut certificates {
        let wholesale = cert.name.starts_with("wholesale/");
        let mut t = checker::FcsTerms {
            wholesale,
            eligible: vec![],
            operational: vec![],
            covered_rates: vec![],
            uncovered_rates: vec![],
        };
        for id in &cert.subjects {
            if wholesale {
                let d = bank.wholesale_deposits.iter().find(|d| &d.id == id).ok_or("missing wholesale FCS subject")?;
                t.eligible.push(resolved_bool(&d.fcs_eligible));
                let op = if resolved_bool(&d.operational_service) {
                    d.operational_balance
                        .known()
                        .cloned()
                        .unwrap_or_else(|| if mode == Mode::Optimistic { d.balance.clone() } else { Q::default() })
                        .min(d.balance.clone())
                } else {
                    Q::default()
                };
                t.operational.push(op);
                t.covered_rates.push(Q::default());
                t.uncovered_rates.push(Q::default());
            } else {
                let d = bank.retail.accounts.iter().find(|d| &d.id == id).ok_or("missing retail FCS subject")?;
                t.eligible.push(resolved_bool(&d.fcs_eligible));
                t.operational.push(Q::default());
                let (covered, uncovered) = &c.fcs_rates[id];
                t.covered_rates.push(covered.clone());
                t.uncovered_rates.push(uncovered.clone());
            }
        }
        cert.fcs = Some(t);
    }
    let inputs = assemble(&parts, &BTreeMap::new())?;
    Ok(Return {
        parts,
        inputs,
        nsfr_inputs: nsfr,
        nsfr_parts,
        records,
        slices,
        certificates,
        allocation_slices,
        assumptions,
    })
}

/// The checker's view of a return: plain data, nothing it has to trust.
pub fn package(bank: &Bank, ret: &Return, reporting_date: NaiveDate) -> checker::Package {
    let stream = |s: Stream| match s {
        Stream::Both => checker::Stream::Both,
        Stream::Total => checker::Stream::Total,
        Stream::Split => checker::Stream::Split,
    };
    let mut specs: Vec<(String, String)> = arf210::SPECS.iter().map(|(code, src)| (format!("forms/{code}"), src.to_string())).collect();
    for (path, src) in [
        ("specs/deposits/retail_sme.prd", deposits::prudence::SOURCE),
        ("specs/deposits/wholesale.prd", deposits::wholesale::SOURCE),
        ("specs/secured/secured_funding.prd", secured::SOURCE),
        ("specs/obligations/commitments.prd", obligations::SOURCE),
        ("specs/loans/loans.prd", loans::SOURCE),
        ("specs/derivatives/derivatives.prd", derivatives::SOURCE),
    ] {
        specs.push((path.into(), src.into()));
    }
    let mut package = checker::Package {
        reporting_date: reporting_date.to_string(),
        contexts: BTreeMap::new(),
        gl: bank.gl.clone(),
        records: ret
            .records
            .iter()
            .map(|(id, kind, balance, split)| checker::Record {
                id: id.clone(),
                gl_account: bank.gl_account.get(id).cloned().unwrap_or_else(|| default_account(kind).into()),
                balance: balance.clone(),
                split: *split,
            })
            .collect(),
        slices: ret.slices.clone(),
        parts: ret
            .parts
            .iter()
            .map(|p| checker::Part {
                record: p.record.clone(),
                item: p.item.clone(),
                currency: p.currency.clone(),
                amount: p.amount.clone(),
                stream: stream(p.stream),
            })
            .collect(),
        rates: BTreeMap::new(),
        lcr_inputs: ret.inputs.clone(),
        nsfr_inputs: ret.nsfr_inputs.clone(),
        certificates: ret.certificates.clone(),
        allocation_slices: ret.allocation_slices.clone(),
        assumptions: ret
            .assumptions
            .iter()
            .map(|(m, s, a, v, w)| (format!("{m}/{s}"), a.clone(), v.clone(), w.clone()))
            .collect(),
        specs,
        optional_inputs: BTreeMap::new(),
        form_contributions: [("ARF 210.6".into(), ret.nsfr_parts.clone())].into(),
        external_sources: ret
            .parts
            .iter()
            .filter(|p| !ret.records.iter().any(|(id, _, _, _)| id == &p.record))
            .map(|p| (p.record.clone(), format!("{} aggregate or off-balance-sheet source; not GL reconciled", p.module)))
            .collect(),
    };
    for part in &ret.nsfr_parts {
        if !package.records.iter().any(|r| r.id == part.source) {
            package
                .external_sources
                .entry(part.source.clone())
                .or_insert_with(|| "NSFR netting-set source; not GL reconciled".into());
        }
    }
    package
        .optional_inputs
        .insert("ARF 210.1B".into(), checker::derive_aud_inputs(&package.parts, &package.rates));
    package
}

/// Attach the caller's entity, consolidation level and regime to every return.
/// The checker validates these declarations and rejects mixed contexts.
pub fn package_with_context(bank: &Bank, ret: &Return, context: checker::ReportingContext, date: NaiveDate) -> checker::Package {
    let mut p = package(bank, ret, date);
    for code in ["ARF 210.1A", "ARF 210.6"].into_iter().chain(p.optional_inputs.keys().map(String::as_str)) {
        p.contexts.insert(code.into(), context.clone());
    }
    p
}

/// The engine's contributions in the checker's form.
pub fn checker_parts(ret: &Return) -> Vec<checker::Part> {
    ret.parts
        .iter()
        .map(|p| checker::Part {
            record: p.record.clone(),
            item: p.item.clone(),
            currency: p.currency.clone(),
            amount: p.amount.clone(),
            stream: match p.stream {
                Stream::Both => checker::Stream::Both,
                Stream::Total => checker::Stream::Total,
                Stream::Split => checker::Stream::Split,
            },
        })
        .collect()
}

/// Every return the engine can produce for one reporting context: ARF 210.1A,
/// 210.1B, 210.6, and (from the same records) 210.3.1, 210.3.2, the actual
/// column of 210.4 and the same-date figures of 210.5. ARF 210.2 is for MLH
/// ADIs, which do not report the LCR.
pub fn package_all(
    bank: &Bank,
    ret: &Return,
    context: checker::ReportingContext,
    date: NaiveDate,
    mode: Mode,
) -> Result<checker::Package, String> {
    let mut p = package_with_context(bank, ret, context.clone(), date);
    for b in maturity::build(bank, ret, date, mode)? {
        for c in &b.contributions {
            if !p.records.iter().any(|r| r.id == c.source) {
                p.external_sources.entry(c.source.clone()).or_insert_with(|| "the same-date LCR return".into());
            }
        }
        p.optional_inputs.insert(b.code.into(), b.inputs);
        p.form_contributions.insert(b.code.into(), b.contributions);
        p.contexts.insert(b.code.into(), context.clone());
    }
    Ok(p)
}
