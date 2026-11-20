//! Run the Prudence source of the deposit rules (specs/deposits/retail_sme.prd)
//! over a `Book`: load records and events into a Prudence `World`, interpret.

use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::{Module, Sense};
use prudence_core::syntax::parse_module;
use prudence_core::uncertain::Uncertain;

use crate::classify::{FcsAllocation, Params};
use crate::model::{Book, Contract, CustomerKind, EarlyWithdrawal, EventKind};

pub const SOURCE: &str = include_str!("../../../specs/deposits/retail_sme.prd");
pub const FORMS: [&str; 4] = ["Arf210_1A", "Arf210_1B", "Arf210_2", "Arf210_6"];

/// The parsed and name-resolved module.
pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| parse_module(SOURCE, &FORMS).unwrap_or_else(|e| panic!("retail_sme.prd:\n{}", e.join("\n"))))
}

fn date(d: NaiveDate) -> Val {
    Val::Date(days_from_civil(i64::from(d.year()), d.month(), d.day()))
}

fn uncertain<T>(u: &Uncertain<T>, subject: &str, attribute: &str, f: impl Fn(&T) -> Val) -> Val {
    match u {
        Uncertain::Known(v) => f(v),
        Uncertain::Unknown(reason) => {
            Val::Unknown { reason: reason.clone(), subject: subject.into(), attribute: attribute.into() }
        }
    }
}

/// Load a book into Prudence records. Customers and deposits refer to each
/// other (holder / deposits), so customers are inserted first and patched.
pub fn load(book: &Book) -> World {
    let mut w = World::default();
    let mut cust_ref = std::collections::HashMap::new();
    for c in &book.customers {
        let kind = match c.kind {
            CustomerKind::NaturalPerson => "NaturalPerson",
            CustomerKind::Smsf => "Smsf",
            CustomerKind::PersonalInvestmentEntity => "PersonalInvestmentEntity",
            CustomerKind::Sme => "Sme",
            CustomerKind::NonFinancialCorporate => "NonFinancialCorporate",
            CustomerKind::FinancialInstitution => "FinancialInstitution",
        };
        let b = |v: &bool| Val::Bool(*v);
        let r = w.insert(
            "Customer",
            Record {
                id: c.id.clone(),
                fields: vec![
                    ("kind".into(), Val::Ctor(kind.into(), vec![])),
                    ("established_relationship".into(), uncertain(&c.established_relationship, &c.id, "established_relationship", b)),
                    ("sme_group".into(), uncertain(&c.sme_group, &c.id, "sme_group", |g| Val::Str(g.clone()))),
                    ("managed_as_retail".into(), uncertain(&c.managed_as_retail, &c.id, "managed_as_retail", b)),
                    ("pie_qualifies".into(), uncertain(&c.pie_qualifies, &c.id, "pie_qualifies", b)),
                    ("deposits".into(), Val::List(vec![])),
                ],
            },
        );
        cust_ref.insert(c.id.clone(), r);
    }
    let mut dep_ref = std::collections::HashMap::new();
    let mut by_holder: std::collections::HashMap<String, Vec<Val>> = std::collections::HashMap::new();
    for a in &book.accounts {
        let b = |v: &bool| Val::Bool(*v);
        let contract = match &a.contract {
            Contract::Ump => Val::Ctor("UMP".into(), vec![]),
            Contract::Clm { notice_days, grace_period_end } => Val::Ctor(
                "CLM".into(),
                vec![Val::num(i64::from(*notice_days)), grace_period_end.map(date).unwrap_or(Val::None)],
            ),
            Contract::Pam { maturity, early_withdrawal } => Val::Ctor(
                "PAM".into(),
                vec![
                    date(*maturity),
                    uncertain(early_withdrawal, &a.id, "early_withdrawal", |e| {
                        Val::Ctor(
                            match e {
                                EarlyWithdrawal::NoLegalRight => "NoLegalRight",
                                EarlyWithdrawal::SignificantPenalty => "SignificantPenalty",
                                EarlyWithdrawal::Permitted => "Permitted",
                            }
                            .into(),
                            vec![],
                        )
                    }),
                ],
            ),
        };
        let r = w.insert(
            "Deposit",
            Record {
                id: a.id.clone(),
                fields: vec![
                    ("holder".into(), cust_ref[&a.customer_id].clone()),
                    ("product_class".into(), Val::Str(a.product_class.clone())),
                    ("balance".into(), Val::Num(a.balance.amount().clone())),
                    ("fcs_eligible".into(), uncertain(&a.fcs_eligible, &a.id, "fcs_eligible", b)),
                    ("transactional".into(), uncertain(&a.transactional, &a.id, "transactional", b)),
                    ("online".into(), uncertain(&a.online, &a.id, "online", b)),
                    ("rate_driven".into(), uncertain(&a.rate_driven, &a.id, "rate_driven", b)),
                    ("contract".into(), contract),
                ],
            },
        );
        by_holder.entry(a.customer_id.clone()).or_default().push(r.clone());
        dep_ref.insert(a.id.clone(), r);
    }
    for (cid, deps) in by_holder {
        let Val::Rec(t, i) = &cust_ref[&cid] else { unreachable!() };
        w.set_field(t, *i, "deposits", Val::List(deps));
    }
    for (n, e) in book.ledger.as_of(NaiveDate::MAX, NaiveDate::MAX).enumerate() {
        let (ty, mut fields) = match &e.kind {
            EventKind::NoticeGiven { account_id, repayment_date } => {
                let Some(d) = dep_ref.get(account_id) else { continue };
                ("NoticeGiven", vec![("deposit".to_string(), d.clone()), ("repayment".to_string(), date(*repayment_date))])
            }
            EventKind::EarlyWithdrawal { product_class, hardship, .. } => (
                "EarlyWithdrawal",
                vec![("product_class".to_string(), Val::Str(product_class.clone())), ("hardship".to_string(), Val::Bool(*hardship))],
            ),
        };
        fields.push(("valid".into(), date(e.valid)));
        fields.push(("recorded".into(), date(e.recorded)));
        w.insert(ty, Record { id: format!("event{n}"), fields });
    }
    w
}

/// One output slice of the interpreted rules.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SpecSlice {
    pub account: String,
    pub amount: Q,
    pub lcr_item: Option<String>,
    pub nsfr_item: String,
    pub nsfr_bucket: String,
    /// Which portion of the partition split this is (FCS-covered or not).
    pub covered: bool,
    pub cites: Vec<String>,
}

pub struct SpecRun {
    pub slices: Vec<SpecSlice>,
    /// (subject, attribute) -> (value assumed, reason)
    pub assumptions: std::collections::BTreeMap<(String, String), (String, String)>,
    /// FCS cover allocations with their LP certificates.
    pub allocations: Vec<prudence_core::interp::Allocation>,
}

/// Interpret the module for `book` under `params`.
pub fn run(book: &Book, params: &Params) -> Result<SpecRun, String> {
    let world = load(book);
    let opts = Options {
        mode: params.mode,
        sense: match params.fcs {
            FcsAllocation::Conservative => None,
            FcsAllocation::Optimised => Some(Sense::Minimize),
        },
        params: vec![
            ("reporting_date".into(), date(params.reporting_date)),
            ("known_at".into(), date(params.known_at)),
            ("fcs_limit".into(), Val::Num(params.fcs_limit.amount().clone())),
            ("sme_threshold".into(), Val::Num(params.sme_threshold.amount().clone())),
        ],
    };
    let out = Interp::new(module(), &world, opts)?.run()?;
    let mut slices = Vec::new();
    for s in out.slices {
        let lcr_item = match s.output("lcr") {
            Some(Val::Str(i)) => Some(i.clone()),
            Some(Val::None) => None,
            other => return Err(format!("{}: bad lcr output {other:?}", s.subject)),
        };
        let (nsfr_item, nsfr_bucket) = match s.output("nsfr") {
            Some(Val::Tuple(t)) => match t.as_slice() {
                [Val::Str(i), Val::Str(b)] => (i.clone(), b.clone()),
                other => return Err(format!("{}: bad nsfr output {other:?}", s.subject)),
            },
            other => return Err(format!("{}: bad nsfr output {other:?}", s.subject)),
        };
        slices.push(SpecSlice { account: s.subject, amount: s.amount, lcr_item, nsfr_item, nsfr_bucket, covered: s.split, cites: s.cites });
    }
    slices.sort();
    Ok(SpecRun { slices, assumptions: out.assumptions, allocations: out.allocations })
}

/// The Lean model generated from the spec, measured on ARF 210.1A (weights)
/// and ARF 210.6 (factors), as written to lean/Prudence/RetailSme.lean.
pub fn lean_source() -> Result<String, String> {
    use prudence_core::ir::{Cell, Expr};
    use prudence_core::syntax::lean::{Measure, generate};
    use std::collections::BTreeMap;
    let pct = |q: &Q| -> Result<i64, String> {
        let p = q * Q::from_integer(100.into());
        if !p.is_integer() {
            return Err(format!("{q} is not a whole percentage"));
        }
        num_traits::ToPrimitive::to_i64(&p.to_integer()).ok_or("overflow".into())
    };
    let constant = |c: Option<&Cell>| match c {
        Some(Cell::Derived(Expr::Const(q))) => Some(q.clone()),
        _ => None,
    };
    let mut weights = BTreeMap::new();
    for it in &arf210::lcr_all().items {
        if let Some(q) = constant(it.cell("weight")) {
            weights.insert(it.id.clone(), pct(&q)?);
        }
    }
    let mut factors = BTreeMap::new();
    for it in &arf210::nsfr().items {
        for (b, col) in ["f0", "f1", "f2"].iter().enumerate() {
            if let Some(q) = constant(it.cell(col)) {
                factors.insert((it.id.clone(), b as u32), pct(&q)?);
            }
        }
    }
    generate(module(), &[("Arf210_1A".into(), Measure::Weight(weights)), ("Arf210_6".into(), Measure::Factor(factors))])
}
