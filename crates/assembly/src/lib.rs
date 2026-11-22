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

pub mod aps210_rules;
pub mod behavioural;
pub mod level2;
pub mod maturity;
pub mod steering;

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

/// Canonical cross-book counterparty / legal entity (APS 210 / APS 221).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counterparty {
    pub id: String,
    pub legal_name: String,
    pub sector: String,
    pub adi_tier: Option<String>,
    pub affiliation: String,
    pub jurisdiction: String,
    pub connected_group_id: Option<String>,
    pub sme_group: Option<String>,
    pub established_relationship: Option<bool>,
    pub managed_as_retail: Option<bool>,
    pub pie_qualifies: Option<bool>,
}

/// Everything one ADI reports, by module.
#[derive(Clone, Default)]
pub struct Bank {
    /// Canonical shared counterparty registry across all books.
    pub counterparties: Vec<Counterparty>,
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
    /// Debt securities issued, regulatory capital, placements with other ADIs.
    pub funding: funding::Book,
    /// Record id -> currency (default AUD).
    pub currency: BTreeMap<String, String>,
    /// The general ledger: account -> balance (AUD).
    pub gl: BTreeMap<String, Q>,
    /// Record id -> ledger account, overriding `default_account`.
    pub gl_account: BTreeMap<String, String>,
    /// Record IDs belonging to APS 120 capital-relief SPVs: excluded from MLH &
    /// NSFR, included in LCR, maturity mismatch, funding plan and daily (APS 210 §5).
    pub capital_relief_spv: std::collections::BTreeSet<String>,
    /// Bitemporal HQLA rating downgrade events (APS 210 Att A §25 30-day grace).
    pub downgrades: Vec<secured::hqla_rules::DowngradeEvent>,
    /// Price-risk hedge close-out adjustments to LCR HQLA market values (ARS p.39).
    pub hqla_hedges: Vec<secured::hqla_rules::HedgeAdjustment>,
    /// Posted variation margin deducted in NSFR 13.1.2 that must be removed from RSF (Att C fn 12).
    pub posted_vm_assets: Vec<aps210_rules::PostedVariationMargin>,
    /// CLF/ALA assets subject to the NSFR 10% RSF quota cap & 65%/85% spillover (Att C §32).
    pub clf_nsfr_assets: Vec<aps210_rules::ClfAsset>,
    /// Validated interdependent asset/liability pairs (0% ASF / 0% RSF, Att C §37).
    pub interdependent_pairs: Vec<aps210_rules::InterdependentPair>,
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
        "placement" => "1700 Deposits with other ADIs",
        "issued" => "2400 Debt securities issued",
        "capital" => "3000 Regulatory capital instruments",
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

/// Run every module and assemble ARF 210.1A (as known on `reporting_date`).
pub fn lcr_return(bank: &Bank, reporting_date: NaiveDate, mode: Mode) -> Result<Return, String> {
    lcr_return_as_known(bank, reporting_date, reporting_date, mode)
}

/// Whole-return bitemporal replay: run every module as known at `known_at`
/// (threads `known_at` into both retail/SME deposits and derivatives).
pub fn lcr_return_as_known(
    bank: &Bank,
    reporting_date: NaiveDate,
    known_at: NaiveDate,
    mode: Mode,
) -> Result<Return, String> {
    lcr_return_modes_as_known(bank, reporting_date, known_at, &|_| mode)
}

/// As `lcr_return`, with each module's unknowns resolved in its own mode
/// (module names: retail, wholesale, secured, obligations, loans, funding,
/// derivatives) - for attributing the cost of missing data.
pub fn lcr_return_modes(bank: &Bank, reporting_date: NaiveDate, modes: &dyn Fn(&str) -> Mode) -> Result<Return, String> {
    lcr_return_modes_as_known(bank, reporting_date, reporting_date, modes)
}

/// As `lcr_return_modes`, with an explicit bitemporal `known_at` cutoff.
pub fn lcr_return_modes_as_known(
    bank: &Bank,
    reporting_date: NaiveDate,
    known_at: NaiveDate,
    modes: &dyn Fn(&str) -> Mode,
) -> Result<Return, String> {
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
        // APS 210 §5: APS 120 capital-relief SPVs are excluded from NSFR.
        if bank.capital_relief_spv.contains(source) {
            return;
        }
        let k = nsfr_key(item, col);
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
            mode: modes("retail"),
            known_at,
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
                nsfr_cell: nsfr_key(s.nsfr_item, s.nsfr_bucket),
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
    let wr = deposits::wholesale::run_full(&bank.wholesale_customers, &bank.wholesale_deposits, reporting_date, modes("wholesale"))?;
    let (ws, allocs) = (wr.slices, wr.allocations);
    for ((s, at), (v, why)) in wr.assumptions {
        assumptions.push(("wholesale", s, at, v, why));
    }
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
            nsfr_cell: nsfr_key(&s.nsfr.0, &s.nsfr.1),
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
    let eff_holdings = secured::hqla_rules::apply_downgrade_grace(&bank.holdings, &bank.downgrades, reporting_date, known_at);
    let eff_holdings = secured::hqla_rules::apply_hedge_adjustments(&eff_holdings, &bank.hqla_hedges);
    let clf_ids: std::collections::BTreeSet<&str> = bank.clf_nsfr_assets.iter().map(|a| a.id.as_str()).collect();
    let r = secured::run(&eff_holdings, &bank.repos, reporting_date, modes("secured"), bank.has_clf)?;
    for (id, item, col, q) in &r.nsfr_parts {
        // Avoid double-counting holdings whose NSFR RSF is allocated via bank.clf_nsfr_assets.
        if !clf_ids.contains(id.as_str()) {
            nsfr_add(id, item, col, q);
        }
    }
    for (id, i, q) in r.parts {
        push("secured", &id, &i, q, ccy(&id), Stream::Both);
    }
    let mut keep = |module: &'static str, a: BTreeMap<(String, String), (String, String)>| {
        for ((s, at), (v, why)) in a {
            assumptions.push((module, s, at, v, why));
        }
    };
    keep("secured", r.assumptions);
    let r = obligations::run(&bank.commitments, reporting_date, modes("obligations"))?;
    for (id, item, q) in &r.nsfr_parts {
        nsfr_add(id, item, "amount", q);
    }
    for (id, i, q) in r.parts {
        push("obligations", &id, &i, q, ccy(&id), Stream::Both);
    }
    keep("obligations", r.assumptions);
    let r = loans::run_all(&bank.loans, &bank.other_inflows, reporting_date, modes("loans"))?;
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
    let r = funding::run(&bank.funding, reporting_date, modes("funding"))?;
    for x in &bank.funding.securities {
        records.push((x.id.clone(), "issued", x.amount.clone(), false));
    }
    for x in &bank.funding.capital {
        records.push((x.id.clone(), "capital", x.amount.clone(), false));
    }
    for x in &bank.funding.placements {
        records.push((x.id.clone(), "placement", x.amount.clone(), false));
    }
    for (id, form, item, col, q) in &r.cells {
        match *form {
            "ARF 210.1A" => push("funding", id, item, q.clone(), ccy(id), Stream::Both),
            "ARF 210.6" => nsfr_add(id, item, col, q),
            _ => {}
        }
    }
    keep("funding", r.assumptions);
    let r = derivatives::run(&bank.derivatives, reporting_date, known_at, modes("derivatives"))?;
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
    let wholesale_by_id: std::collections::HashMap<&str, &deposits::wholesale::Deposit> =
        bank.wholesale_deposits.iter().map(|d| (d.id.as_str(), d)).collect();
    let retail_by_id: std::collections::HashMap<&str, &deposits::DepositAccount> =
        bank.retail.accounts.iter().map(|d| (d.id.as_str(), d)).collect();
    for cert in &mut certificates {
        let wholesale = cert.name.starts_with("wholesale/");
        let mode = modes(if wholesale { "wholesale" } else { "retail" });
        let resolved_bool = |v: &prudence_core::uncertain::Uncertain<bool>| v.known().copied().unwrap_or(mode == Mode::Optimistic);
        let mut t = checker::FcsTerms {
            wholesale,
            eligible: vec![],
            operational: vec![],
            covered_rates: vec![],
            uncovered_rates: vec![],
        };
        for id in &cert.subjects {
            if wholesale {
                let d = wholesale_by_id.get(id.as_str()).ok_or("missing wholesale FCS subject")?;
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
                let d = retail_by_id.get(id.as_str()).ok_or("missing retail FCS subject")?;
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

    // Item 5 (Att C §32): NSFR CLF/ALA 10% RSF quota capped by LCR CLF, spilling excess to 65%/85%.
    if !bank.clf_nsfr_assets.is_empty() {
        let lcr_ev = eval(&inputs)?;
        let lcr_clf_cap_aud = lcr_ev.q("6.5").clone() * million();
        let sp = aps210_rules::apply_clf_rsf_quota(&bank.clf_nsfr_assets, &lcr_clf_cap_aud);
        if sp.eligible_10pct > Q::default() {
            nsfr_add("clf-quota", "27.1", "b2", &sp.eligible_10pct);
        }
        if sp.spillover_65pct > Q::default() {
            nsfr_add("clf-spillover-65", "35.1", "b2", &sp.spillover_65pct);
        }
        if sp.spillover_85pct > Q::default() {
            nsfr_add("clf-spillover-85", "37.1", "b2", &sp.spillover_85pct);
        }
    }

    // Item 6 (Att C §37): validated interdependent asset/liability pairs -> 0% ASF (item 17) & 0% RSF (item 44).
    if !bank.interdependent_pairs.is_empty() {
        aps210_rules::validate_interdependent_pairs(&bank.interdependent_pairs)?;
        for ip in &bank.interdependent_pairs {
            nsfr_add(&ip.liability_record_id, "17", "b2", &ip.amount);
            nsfr_add(&ip.asset_record_id, "44", "b2", &ip.amount);
        }
    }

    // Item 4 (Att C fn 12): remove posted variation margin deducted in 13.1.2 from RSF assets.
    if !bank.posted_vm_assets.is_empty() {
        let vm_13_1_2_aud = nsfr.get("13.1.2").cloned().unwrap_or_default() * million();
        aps210_rules::verify_posted_vm_conservation(&bank.posted_vm_assets, &vm_13_1_2_aud)?;
        let zero = Q::default();
        for pvm in &bank.posted_vm_assets {
            let amt_m = &pvm.amount / million();
            let entry = nsfr.entry(pvm.rsf_cell.clone()).or_default();
            *entry = (&*entry - &amt_m).max(zero.clone());
            nsfr_parts.push(checker::FormContribution {
                source: pvm.asset_record_id.clone(),
                cell: pvm.rsf_cell.clone(),
                amount: -amt_m,
            });
        }
    }

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

/// Apply signed overlays to the LCR and NSFR inputs before any dependent
/// return (ARF 210.5) is derived from them; the overlays go into the package
/// so the gate can verify them.
pub fn apply_overlays(ret: &mut Return, overlays: &[checker::Overlay]) {
    for o in overlays {
        let target = match o.form.as_str() {
            "ARF 210.1A" => &mut ret.inputs,
            "ARF 210.6" => &mut ret.nsfr_inputs,
            _ => continue,
        };
        *target.entry(o.cell.clone()).or_default() += o.amount.clone();
    }
}

/// The ARF 210.6 input key of an (item, column) pair: a single-column row
/// (`amount`) is keyed by the bare item, as everywhere else.
fn nsfr_key(item: &str, col: &str) -> String {
    if col == "amount" { item.to_string() } else { format!("{item}:{col}") }
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
        ("specs/ir/modules/retail_sme.json", deposits::prudence::SOURCE),
        ("specs/ir/modules/wholesale.json", deposits::wholesale::SOURCE),
        ("specs/ir/modules/secured_funding.json", secured::SOURCE),
        ("specs/ir/modules/commitments.json", obligations::SOURCE),
        ("specs/ir/modules/loans.json", loans::SOURCE),
        ("specs/ir/modules/derivatives.json", derivatives::SOURCE),
        ("specs/ir/modules/funding.json", funding::SOURCE),
    ] {
        specs.push((path.into(), src.into()));
    }
    let record_ids: std::collections::HashSet<&str> = ret.records.iter().map(|(id, _, _, _)| id.as_str()).collect();
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
        overlays: Vec::new(),
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
            .filter(|p| !record_ids.contains(p.record.as_str()))
            .map(|p| (p.record.clone(), format!("{} aggregate or off-balance-sheet source; not GL reconciled", p.module)))
            .collect(),
    };
    for part in &ret.nsfr_parts {
        if !record_ids.contains(part.source.as_str()) {
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

/// Every return for one reporting context: ARF 210.1A, 210.1B, 210.6, 210.3.1,
/// 210.3.2, 210.4 (actuals from the records, forecasts from the funding plan),
/// 210.5 (history from earlier snapshots), and 210.2 for an MLH ADI.
pub fn package_all(
    bank: &Bank,
    ret: &Return,
    context: checker::ReportingContext,
    date: NaiveDate,
    mode: Mode,
    extras: &maturity::Extras,
) -> Result<checker::Package, String> {
    let mut p = package_with_context(bank, ret, context.clone(), date);
    for b in maturity::build(bank, ret, date, mode, &context.regime, extras)? {
        for c in &b.contributions {
            if !p.records.iter().any(|r| r.id == c.source) {
                let why = if c.source.starts_with("ARF ") {
                    "the same-date return"
                } else if c.source.starts_with("snapshot ") {
                    "the ADI's records as at an earlier business day; not GL reconciled at this date"
                } else if c.source.starts_with("collateral events") {
                    "realised collateral flows (bitemporal events); not GL reconciled"
                } else if c.source.starts_with("early repayment requests") || c.source.starts_with("notice events") {
                    "requests and notices received over the five days; not GL reconciled"
                } else if c.source == "funding plan" {
                    "the ADI's funding plan (forecast); not GL reconciled"
                } else {
                    "not GL reconciled"
                };
                p.external_sources.entry(c.source.clone()).or_insert_with(|| why.into());
            }
        }
        p.optional_inputs.insert(b.code.into(), b.inputs);
        p.form_contributions.insert(b.code.into(), b.contributions);
        p.contexts.insert(b.code.into(), context.clone());
    }
    Ok(p)
}
