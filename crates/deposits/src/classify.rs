//! Classify retail and SME deposits into ARF 210.1A/1B outflow items and ARF
//! 210.6 ASF items.
//!
//! The rules live in `specs/deposits/retail_sme.prd` (Prudence) and are run by
//! the interpreter; this module only validates inputs, runs the spec, and maps
//! its outputs onto the form specs (`specs/arf210/*.prd`), which supply the
//! LCR run-off rates. Every slice carries the `@cite`s of the rules applied.

use std::collections::{BTreeMap, HashSet};

use chrono::NaiveDate;
use num_traits::Zero;
use prudence_core::ir::{Cell, Expr};
use prudence_core::money::{AudCarrying, conserve};
use prudence_core::num::int;
use prudence_core::uncertain::{Assumption, Mode};
use prudence_core::{Q, Ref};

use crate::model::Book;

/// How the per-depositor FCS cover is spread across that depositor's accounts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FcsAllocation {
    /// The spec's `maximize` (cover goes where it lowers the run-off least).
    /// The default: APG 210 doesn't say the ADI may choose the split.
    Conservative,
    /// The spec's `@policy(minimize: requires_signoff)`: cover goes where it
    /// lowers the run-off most. Requires sign-off before use in a return.
    Optimised,
}

#[derive(Clone, Debug)]
pub struct Params {
    pub reporting_date: NaiveDate,
    /// What the bank knew as at this date (bitemporal replay).
    pub known_at: NaiveDate,
    pub mode: Mode,
    pub fcs: FcsAllocation,
    pub fcs_limit: AudCarrying,
    pub sme_threshold: AudCarrying,
}

impl Params {
    pub fn new(reporting_date: NaiveDate) -> Self {
        Params {
            reporting_date,
            known_at: reporting_date,
            mode: Mode::Conservative,
            fcs: FcsAllocation::Conservative,
            fcs_limit: AudCarrying::new(int(250_000)),
            sme_threshold: AudCarrying::new(int(2_000_000)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Slice {
    pub account_id: String,
    pub amount: AudCarrying,
    /// ARF 210.1A/1B item, or None when outside the 30-day horizon.
    pub lcr_item: Option<&'static str>,
    /// Run-off rate: the item's column-2 weight in the ARF 210.1A spec.
    pub lcr_rate: Q,
    /// ARF 210.6 item and maturity bucket (b0 < 6m, b1 6-12m, b2 >= 1y).
    pub nsfr_item: &'static str,
    pub nsfr_bucket: &'static str,
    /// The FCS-covered portion of the deposit (the partition's split).
    pub covered: bool,
    /// Citations of every rule applied to this slice.
    pub reasons: Vec<String>,
}

impl Slice {
    pub fn lcr_outflow(&self) -> AudCarrying {
        self.amount.times(&self.lcr_rate)
    }
}

#[derive(Debug)]
pub struct Classification {
    pub fcs_rates: BTreeMap<String, (Q, Q)>,
    pub params: Params,
    pub slices: Vec<Slice>,
    pub assumptions: Vec<Assumption>,
    /// FCS cover allocations with their LP certificates.
    pub allocations: Vec<prudence_core::interp::Allocation>,
}

#[derive(Debug, PartialEq)]
pub enum ClassifyError {
    UnknownCustomer { account: String, customer: String },
    DuplicateAccount(String),
    NegativeBalance(String),
    Conservation { account: String, detail: String },
    /// The spec failed at run time (no case matched, unresolved unknown, ...).
    Spec(String),
}

impl std::fmt::Display for ClassifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ClassifyError {}

impl Classification {
    /// Total weighted LCR outflow (AUD).
    pub fn lcr_outflow(&self) -> AudCarrying {
        self.slices.iter().map(|s| s.lcr_outflow()).sum()
    }

    /// ARF 210.1A/1B column-1 inputs, in AUD millions (the forms' unit).
    pub fn lcr_inputs(&self) -> BTreeMap<String, Q> {
        let million = int(1_000_000);
        let mut out: BTreeMap<String, Q> = BTreeMap::new();
        for s in &self.slices {
            if let Some(item) = s.lcr_item {
                *out.entry(item.to_string()).or_insert_with(Q::zero) += s.amount.amount() / &million;
            }
        }
        out
    }

    /// ARF 210.6 inputs ("3:b0" etc.), in AUD millions.
    pub fn nsfr_inputs(&self) -> BTreeMap<String, Q> {
        let million = int(1_000_000);
        let mut out: BTreeMap<String, Q> = BTreeMap::new();
        for s in &self.slices {
            *out.entry(format!("{}:{}", s.nsfr_item, s.nsfr_bucket)).or_insert_with(Q::zero) +=
                s.amount.amount() / &million;
        }
        out
    }

    pub fn slices_for<'a>(&'a self, account_id: &'a str) -> impl Iterator<Item = &'a Slice> {
        self.slices.iter().filter(move |s| s.account_id == account_id)
    }
}

/// The form item id as a `'static` str, checking the spec's output names a real item.
fn form_item(form: &'static prudence_core::Form, id: &str) -> Result<&'static str, ClassifyError> {
    form.item(id)
        .map(|it| it.id.as_str())
        .ok_or_else(|| ClassifyError::Spec(format!("rules produced {id}, which is not an item of {}", form.code)))
}

fn lcr_weight(item: &'static str) -> Result<Q, ClassifyError> {
    match arf210::lcr_all().cell(&Ref::new(item, "weight")) {
        Some(Cell::Derived(Expr::Const(q))) => Ok(q.clone()),
        _ => Err(ClassifyError::Spec(format!("ARF 210.1A item {item} has no fixed weight"))),
    }
}

fn attribute(a: &str) -> &'static str {
    const KNOWN: [&str; 9] = [
        "established_relationship", "sme_group", "managed_as_retail", "pie_qualifies", "early_withdrawal",
        "fcs_eligible", "transactional", "online", "rate_driven",
    ];
    KNOWN.into_iter().find(|k| *k == a).unwrap_or("other")
}

/// Classify every deposit in `book` by running the Prudence rules.
pub fn classify(book: &Book, params: &Params) -> Result<Classification, ClassifyError> {
    let customers: HashSet<&str> = book.customers.iter().map(|c| c.id.as_str()).collect();
    let mut seen = HashSet::new();
    for a in &book.accounts {
        if !seen.insert(a.id.as_str()) {
            return Err(ClassifyError::DuplicateAccount(a.id.clone()));
        }
        if !customers.contains(a.customer_id.as_str()) {
            return Err(ClassifyError::UnknownCustomer { account: a.id.clone(), customer: a.customer_id.clone() });
        }
        if a.balance.is_negative() {
            return Err(ClassifyError::NegativeBalance(a.id.clone()));
        }
    }

    let run = crate::prudence::run(book, params).map_err(ClassifyError::Spec)?;
    let mut slices = Vec::new();
    for s in run.slices {
        let lcr_item = s.lcr_item.as_deref().map(|i| form_item(arf210::lcr_all(), i)).transpose()?;
        let lcr_rate = match lcr_item {
            Some(i) => lcr_weight(i)?,
            None => Q::zero(),
        };
        let bucket = match s.nsfr_bucket.as_str() {
            "b0" => "b0",
            "b1" => "b1",
            "b2" => "b2",
            other => return Err(ClassifyError::Spec(format!("bad NSFR bucket {other}"))),
        };
        slices.push(Slice {
            account_id: s.account,
            amount: AudCarrying::new(s.amount),
            lcr_item,
            lcr_rate,
            nsfr_item: form_item(arf210::nsfr(), &s.nsfr_item)?,
            nsfr_bucket: bucket,
            covered: s.covered,
            reasons: s.cites,
        });
    }

    // Exhaustive and disjoint: every account's slices add back to its balance (O(N)).
    let mut by_acct: std::collections::HashMap<&str, Vec<AudCarrying>> =
        std::collections::HashMap::with_capacity(book.accounts.len());
    for s in &slices {
        by_acct.entry(s.account_id.as_str()).or_default().push(s.amount.clone());
    }
    for a in &book.accounts {
        let Some(parts) = by_acct.get(a.id.as_str()) else {
            return Err(ClassifyError::Conservation { account: a.id.clone(), detail: "not classified".into() });
        };
        conserve(&a.balance, parts).map_err(|detail| ClassifyError::Conservation { account: a.id.clone(), detail })?;
    }

    let allocations = run.allocations;
    let assumptions = run
        .assumptions
        .into_iter()
        .map(|((subject, attr), (assumed, reason))| Assumption { subject, attribute: attribute(&attr), assumed, reason })
        .collect();
    Ok(Classification { fcs_rates: run.fcs_rates, params: params.clone(), slices, assumptions, allocations })
}
