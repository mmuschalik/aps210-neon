//! Loans (ACTUS ANN / LAM / PAM / UMP): LCR inflows (ARF 210.1A item 20) and
//! NSFR required stable funding (ARF 210.6 items 24, 35-37, 39). The rules are
//! the Prudence Lean 4 spec `lean/Prudence/Specs/Modules/Loans.lean`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::load_module_json;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/ir/modules/loans.json");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| {
        load_module_json(SOURCE, &["Arf210_1A", "Arf210_6"]).unwrap_or_else(|e| panic!("loans.json:\n{}", e.join("\n")))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Borrower {
    RetailCustomer,
    Sme,
    NonFinancialCorporate,
    CentralBank,
    FinancialInstitution,
    OtherEntity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Affiliation {
    #[default]
    External,
    IntraGroupBank,
    IntraGroupFinancial,
    IntraGroupOther,
}

/// A contractual receivable that is not a loan.
#[derive(Clone, Debug)]
pub struct Receivable {
    pub id: String,
    pub amount: Q,
    pub due: NaiveDate,
    pub non_financial_revenue: bool,
    pub contingent: bool,
    pub performing: Uncertain<bool>,
}

/// Committed head office funding facility (ARF 210.1A item 21.5).
#[derive(Clone, Debug, Default)]
pub struct HeadOffice {
    pub foreign_adi: bool,
    pub local_banking_subsidiary: bool,
    pub approved: bool,
    pub facility: Q,
}

/// Inflows other than loans.
#[derive(Clone, Debug, Default)]
pub struct Other {
    pub receivables: Vec<Receivable>,
    pub head_office: HeadOffice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Product {
    ResidentialMortgage,
    OtherLoan,
}

/// ACTUS contract types.
#[derive(Clone, Debug)]
pub enum Contract {
    /// Annuity: level payment covering interest and principal.
    Ann { payment: Q },
    /// Linear amortizer: fixed principal per period, plus interest.
    Lam { principal_per_period: Q },
    /// Principal at maturity, periodic interest.
    Pam { maturity: NaiveDate },
    /// Open maturity (revolving): only the minimum payment is contractual.
    Ump { minimum_payment: Q },
}

#[derive(Clone, Debug)]
pub struct Loan {
    pub id: String,
    pub borrower: Borrower,
    pub affiliation: Affiliation,
    pub product: Product,
    pub contract: Contract,
    pub balance: Q,
    /// Nominal annual rate.
    pub rate: Q,
    pub next_payment: NaiveDate,
    pub cycle_months: u32,
    pub performing: Uncertain<bool>,
    pub lvr_at_most_80: Uncertain<bool>,
    pub risk_weight: u32,
    pub encumbered_until: Option<NaiveDate>,
}

fn date(d: NaiveDate) -> Val {
    Val::Date(days_from_civil(i64::from(d.year()), d.month(), d.day()))
}

fn uncertain(u: &Uncertain<bool>, subject: &str, attribute: &str) -> Val {
    match u {
        Uncertain::Known(b) => Val::Bool(*b),
        Uncertain::Unknown(r) => Val::Unknown { reason: r.clone(), subject: subject.into(), attribute: attribute.into() },
    }
}

pub fn load(loans: &[Loan]) -> World {
    load_all(loans, &Other::default())
}

pub fn load_all(loans: &[Loan], other: &Other) -> World {
    let mut w = World::default();
    for l in loans {
        let contract = match &l.contract {
            Contract::Ann { payment } => Val::Ctor("ANN".into(), vec![Val::Num(payment.clone())]),
            Contract::Lam { principal_per_period } => Val::Ctor("LAM".into(), vec![Val::Num(principal_per_period.clone())]),
            Contract::Pam { maturity } => Val::Ctor("PAM".into(), vec![date(*maturity)]),
            Contract::Ump { minimum_payment } => Val::Ctor("UMP".into(), vec![Val::Num(minimum_payment.clone())]),
        };
        w.insert(
            "Loan",
            Record {
                id: l.id.clone(),
                fields: vec![
                    ("contract".into(), contract),
                    ("borrower".into(), Val::Ctor(format!("{:?}", l.borrower), vec![])),
                    ("affiliation".into(), Val::Ctor(format!("{:?}", l.affiliation), vec![])),
                    ("product".into(), Val::Ctor(format!("{:?}", l.product), vec![])),
                    ("balance".into(), Val::Num(l.balance.clone())),
                    ("rate".into(), Val::Num(l.rate.clone())),
                    ("next_payment".into(), date(l.next_payment)),
                    ("cycle_months".into(), Val::num(i64::from(l.cycle_months))),
                    ("performing".into(), uncertain(&l.performing, &l.id, "performing")),
                    ("lvr_at_most_80".into(), uncertain(&l.lvr_at_most_80, &l.id, "lvr_at_most_80")),
                    ("risk_weight".into(), Val::num(i64::from(l.risk_weight))),
                    ("encumbered_until".into(), l.encumbered_until.map(date).unwrap_or(Val::None)),
                ],
            },
        );
    }
    for r in &other.receivables {
        w.insert(
            "Receivable",
            Record {
                id: r.id.clone(),
                fields: vec![
                    ("amount".into(), Val::Num(r.amount.clone())),
                    ("due".into(), date(r.due)),
                    ("non_financial_revenue".into(), Val::Bool(r.non_financial_revenue)),
                    ("contingent".into(), Val::Bool(r.contingent)),
                    ("performing".into(), uncertain(&r.performing, &r.id, "performing")),
                ],
            },
        );
    }
    w
}

pub struct Run {
    /// ARF 210.1A item 20.x amounts (AUD): contractual inflows within 30 days.
    pub inflows: BTreeMap<String, Q>,
    /// Non-zero (loan id, item, AUD) contributions behind `inflows`.
    pub parts: Vec<(String, String, Q)>,
    /// ARF 210.6 (item, bucket) -> principal (AUD).
    pub rsf: BTreeMap<(String, String), Q>,
    /// (loan, item, bucket, amount) slices.
    pub slices: Vec<(String, String, String, Q)>,
    pub assumptions: BTreeMap<(String, String), (String, String)>,
}

pub fn run(loans: &[Loan], reporting_date: NaiveDate, mode: Mode) -> Result<Run, String> {
    run_all(loans, &Other::default(), reporting_date, mode)
}

/// Loans plus other contractual inflows (21.4) and head office support (21.5).
pub fn run_all(loans: &[Loan], other: &Other, reporting_date: NaiveDate, mode: Mode) -> Result<Run, String> {
    let world = load_all(loans, other);
    let ho = &other.head_office;
    let params = vec![
        ("reporting_date".into(), date(reporting_date)),
        ("foreign_adi".into(), Val::Bool(ho.foreign_adi)),
        ("local_banking_subsidiary".into(), Val::Bool(ho.local_banking_subsidiary)),
        ("head_office_approved".into(), Val::Bool(ho.approved)),
        ("head_office_facility".into(), Val::Num(ho.facility.clone())),
    ];
    let it = Interp::new(module(), &world, Options { mode, sense: None, params })?;
    let mut inflows: BTreeMap<String, Q> = ["20.1", "20.2", "20.3", "20.4", "20.5", "20.6", "20.7", "20.8", "20.9", "21.4", "21.5"]
        .iter()
        .map(|i| (i.to_string(), Q::default()))
        .collect();
    let mut parts = Vec::new();
    for (k, l) in loans.iter().enumerate() {
        for (item, total) in inflows.iter_mut() {
            match it.call_fn("inflow", vec![Val::Rec("Loan".into(), k), Val::Str(item.clone())])? {
                Val::Num(q) => {
                    if q != Q::default() {
                        parts.push((l.id.clone(), item.clone(), q.clone()));
                    }
                    *total += q
                }
                other => return Err(format!("inflow: {other:?}")),
            }
        }
    }
    for (k, r) in other.receivables.iter().enumerate() {
        match it.call_fn("other_inflow", vec![Val::Rec("Receivable".into(), k), Val::Str("21.4".into())])? {
            Val::Num(q) if q != Q::default() => {
                parts.push((r.id.clone(), "21.4".into(), q.clone()));
                *inflows.get_mut("21.4").unwrap() += q;
            }
            Val::Num(_) => {}
            other => return Err(format!("other_inflow: {other:?}")),
        }
    }
    match it.call_fn("head_office_support", vec![])? {
        Val::Num(q) if q != Q::default() => {
            parts.push(("head-office".into(), "21.5".into(), q.clone()));
            inflows.insert("21.5".into(), q);
        }
        Val::Num(_) => {}
        other => return Err(format!("head_office_support: {other:?}")),
    }
    let out = it.run()?;
    let mut rsf: BTreeMap<(String, String), Q> = BTreeMap::new();
    let mut slices = Vec::new();
    for s in out.slices {
        let (item, bucket) = match s.output("nsfr") {
            Some(Val::Tuple(t)) => match t.as_slice() {
                [Val::Str(a), Val::Str(b)] => (a.clone(), b.clone()),
                other => return Err(format!("nsfr {other:?}")),
            },
            other => return Err(format!("{}: nsfr {other:?}", s.subject)),
        };
        *rsf.entry((item.clone(), bucket.clone())).or_default() += s.amount.clone();
        slices.push((s.subject, item, bucket, s.amount));
    }
    Ok(Run { inflows, parts, rsf, slices, assumptions: it.assumptions() })
}

/// Form inputs in AUD millions: ARF 210.1A `item` and ARF 210.6 `item:bucket`.
pub fn arf_inputs(run: &Run) -> (BTreeMap<String, Q>, BTreeMap<String, Q>) {
    let million = Q::from_integer(1_000_000.into());
    let lcr = run.inflows.iter().map(|(k, v)| (k.clone(), v / &million)).collect();
    let nsfr = run.rsf.iter().map(|((i, b), v)| (format!("{i}:{b}"), v / &million)).collect();
    (lcr, nsfr)
}

/// Principal falling due before each date in `ends` (exclusive), per loan,
/// from the spec's own schedule (`principal_before`): the cumulative
/// repayment profile behind the ARF 210.3.1 maturity ladder.
pub fn repayment_profile(
    loans: &[Loan],
    reporting_date: NaiveDate,
    mode: Mode,
    ends: &[NaiveDate],
) -> Result<Vec<Vec<Q>>, String> {
    let world = load(loans);
    let it = Interp::new(module(), &world, Options { mode, sense: None, params: vec![("reporting_date".into(), date(reporting_date))] })?;
    let mut out = Vec::new();
    for k in 0..loans.len() {
        let mut row = Vec::new();
        for e in ends {
            match it.call_fn("principal_before", vec![Val::Rec("Loan".into(), k), date(*e)])? {
                Val::Num(q) => row.push(q),
                other => return Err(format!("principal_before: {other:?}")),
            }
        }
        out.push(row);
    }
    Ok(out)
}
