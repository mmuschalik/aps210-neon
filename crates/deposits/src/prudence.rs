//! Run the Prudence Lean 4 spec (`lean/Prudence/Specs/Modules/RetailSme.lean`)
//! over a `Book`: load records and events into a Prudence `World`, interpret.

use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::{Module, Sense};
use prudence_core::syntax::load_module_json;
use prudence_core::uncertain::Uncertain;

use crate::classify::{FcsAllocation, Params};
use crate::model::{Book, Contract, CustomerKind, EarlyWithdrawal, EventKind};

pub const SOURCE: &str = include_str!("../../../specs/ir/modules/retail_sme.json");
pub const FORMS: [&str; 4] = ["Arf210_1A", "Arf210_1B", "Arf210_2", "Arf210_6"];

/// The loaded and validated module (from `lean/Prudence/Specs/Modules/RetailSme.lean`).
pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| load_module_json(SOURCE, &FORMS).unwrap_or_else(|e| panic!("retail_sme.json:\n{}", e.join("\n"))))
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
    pub fcs_rates: std::collections::BTreeMap<String, (Q, Q)>,
    pub slices: Vec<SpecSlice>,
    /// (subject, attribute) -> (value assumed, reason)
    pub assumptions: std::collections::BTreeMap<(String, String), (String, String)>,
    /// FCS cover allocations with their LP certificates.
    pub allocations: Vec<prudence_core::interp::Allocation>,
}

/// Interpret the module for `book` under `params`.
/// For large books (> 256 customers), partitions customers into independent
/// chunks (keeping every `sme_group` intact) and evaluates in parallel across
/// CPU cores, eliminating O(N²) SME-group scans while producing identical output.
pub fn run(book: &Book, params: &Params) -> Result<SpecRun, String> {
    if book.customers.len() <= 256 {
        return run_single(book, params);
    }
    // Group customers by sme_group (known group id) so connected SMEs stay together.
    let mut groups: std::collections::BTreeMap<String, Vec<usize>> = std::collections::BTreeMap::new();
    let mut singles: Vec<usize> = Vec::new();
    for (i, c) in book.customers.iter().enumerate() {
        if let Uncertain::Known(g) = &c.sme_group {
            groups.entry(g.clone()).or_default().push(i);
        } else {
            singles.push(i);
        }
    }
    // Pack groups + singles into chunks of ~128 customers.
    const CHUNK_TARGET: usize = 128;
    let mut chunks: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    for (_, idxs) in groups {
        if !cur.is_empty() && cur.len() + idxs.len() > CHUNK_TARGET {
            chunks.push(std::mem::take(&mut cur));
        }
        cur.extend(idxs);
    }
    for idx in singles {
        if cur.len() >= CHUNK_TARGET {
            chunks.push(std::mem::take(&mut cur));
        }
        cur.push(idx);
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }

    // Index accounts by customer_id once (O(N)).
    let mut accts_by_cust: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
    for (i, a) in book.accounts.iter().enumerate() {
        accts_by_cust.entry(a.customer_id.as_str()).or_default().push(i);
    }
    // Separate global EarlyWithdrawal events from account-scoped NoticeGiven events.
    let mut global_events = Vec::new();
    let mut notice_by_acct: std::collections::HashMap<&str, Vec<&crate::model::Event>> = std::collections::HashMap::new();
    for e in book.ledger.as_of(NaiveDate::MAX, NaiveDate::MAX) {
        match &e.kind {
            EventKind::EarlyWithdrawal { .. } => global_events.push(e),
            EventKind::NoticeGiven { account_id, .. } => {
                notice_by_acct.entry(account_id.as_str()).or_default().push(e);
            }
        }
    }

    // Build shard Books and run across available CPU cores.
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(chunks.len());
    let chunk_slice = &chunks;
    let accts_ref = &accts_by_cust;
    let global_ref = &global_events;
    let notice_ref = &notice_by_acct;
    let results: Result<Vec<SpecRun>, String> = std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(threads);
        let per_thread = chunk_slice.len().div_ceil(threads);
        for band in chunk_slice.chunks(per_thread) {
            handles.push(s.spawn(move || -> Result<Vec<SpecRun>, String> {
                let mut local = Vec::with_capacity(band.len());
                for c_idxs in band {
                    let mut sub = Book::default();
                    for &ci in c_idxs {
                        let c = &book.customers[ci];
                        sub.customers.push(c.clone());
                        if let Some(ais) = accts_ref.get(c.id.as_str()) {
                            for &ai in ais {
                                let a = &book.accounts[ai];
                                if let Some(nes) = notice_ref.get(a.id.as_str()) {
                                    for ne in nes {
                                        sub.ledger.record(ne.kind.clone(), ne.valid, ne.recorded);
                                    }
                                }
                                sub.accounts.push(a.clone());
                            }
                        }
                    }
                    for ge in global_ref {
                        sub.ledger.record(ge.kind.clone(), ge.valid, ge.recorded);
                    }
                    local.push(run_single(&sub, params)?);
                }
                Ok(local)
            }));
        }
        let mut all = Vec::with_capacity(chunk_slice.len());
        for h in handles {
            all.extend(h.join().map_err(|_| "worker thread panicked".to_string())??);
        }
        Ok(all)
    });

    let mut fcs_rates = std::collections::BTreeMap::new();
    let mut slices = Vec::with_capacity(book.accounts.len());
    let mut assumptions = std::collections::BTreeMap::new();
    let mut allocations = Vec::new();
    for r in results? {
        fcs_rates.extend(r.fcs_rates);
        slices.extend(r.slices);
        assumptions.extend(r.assumptions);
        allocations.extend(r.allocations);
    }
    slices.sort();
    Ok(SpecRun { fcs_rates, slices, assumptions, allocations })
}

fn run_single(book: &Book, params: &Params) -> Result<SpecRun, String> {
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
    let it = Interp::new(module(), &world, opts)?;
    let out = it.run()?;
    let mut fcs_rates = std::collections::BTreeMap::new();
    for (i, account) in book.accounts.iter().enumerate() {
        let rate = |covered| match it.call_fn("lcr_rate", vec![Val::Rec("Deposit".into(), i), Val::Bool(covered)])? {
            Val::Num(q) => Ok(q), _ => Err("non-numeric FCS rate".to_string()),
        };
        fcs_rates.insert(account.id.clone(), (rate(true)?, rate(false)?));
    }
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
    Ok(SpecRun { fcs_rates, slices, assumptions: out.assumptions, allocations: out.allocations })
}
