//! Loans (ACTUS ANN / LAM / PAM / UMP): LCR inflows (ARF 210.1A item 20) and
//! NSFR required stable funding (ARF 210.6 items 24, 35-37, 39). The rules are
//! the Prudence module specs/loans/loans.prd.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::parse_module;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/loans/loans.prd");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| {
        parse_module(SOURCE, &["Arf210_1A", "Arf210_6"]).unwrap_or_else(|e| panic!("loans.prd:\n{}", e.join("\n")))
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
    let world = load(loans);
    let it = Interp::new(module(), &world, Options { mode, sense: None, params: vec![("reporting_date".into(), date(reporting_date))] })?;
    let mut inflows: BTreeMap<String, Q> =
        ["20.1", "20.2", "20.3", "20.4", "20.5", "20.6"].iter().map(|i| (i.to_string(), Q::default())).collect();
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
