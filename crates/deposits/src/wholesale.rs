//! Wholesale deposits: specs/deposits/wholesale.prd (operational and
//! non-operational deposits of corporates, sovereigns, banks and other FIs).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::parse_module;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/deposits/wholesale.prd");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| parse_module(SOURCE, &crate::prudence::FORMS).unwrap_or_else(|e| panic!("wholesale.prd:\n{}", e.join("\n"))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    NonFinancialCorporate,
    SovereignCentralBankPseMdb,
    Bank,
    OtherFinancialInstitution,
    OtherLegalEntity,
}

#[derive(Clone, Debug)]
pub struct Customer {
    pub id: String,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub struct Deposit {
    pub id: String,
    pub customer_id: String,
    pub balance: Q,
    pub within_30_days: bool,
    pub called: bool,
    pub fcs_eligible: Uncertain<bool>,
    pub operational_service: Uncertain<bool>,
    pub operational_balance: Uncertain<Q>,
    /// Contractual maturity (ARF 210.3.2); `None`: at call.
    pub maturity: Option<NaiveDate>,
}

/// One slice: the account, its amount, and its LCR / NSFR items.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WSlice {
    pub account: String,
    pub covered: bool,
    pub amount: Q,
    pub lcr_item: Option<String>,
    pub nsfr: (String, String),
}

fn unknown(reason: &str, subject: &str, attribute: &str) -> Val {
    Val::Unknown { reason: reason.into(), subject: subject.into(), attribute: attribute.into() }
}

pub fn run(customers: &[Customer], deposits: &[Deposit], reporting_date: NaiveDate, mode: Mode) -> Result<Vec<WSlice>, String> {
    Ok(run_with_certificates(customers, deposits, reporting_date, mode)?.0)
}

/// As `run`, with the FCS allocations and their LP certificates.
pub fn run_with_certificates(
    customers: &[Customer],
    deposits: &[Deposit],
    reporting_date: NaiveDate,
    mode: Mode,
) -> Result<(Vec<WSlice>, Vec<prudence_core::interp::Allocation>), String> {
    let mut w = World::default();
    let mut cref = BTreeMap::new();
    for c in customers {
        let r = w.insert(
            "Customer",
            Record {
                id: c.id.clone(),
                fields: vec![("kind".into(), Val::Ctor(format!("{:?}", c.kind), vec![])), ("deposits".into(), Val::List(vec![]))],
            },
        );
        cref.insert(c.id.clone(), r);
    }
    let mut by: BTreeMap<String, Vec<Val>> = BTreeMap::new();
    for d in deposits {
        let holder = cref.get(&d.customer_id).ok_or(format!("{}: unknown customer {}", d.id, d.customer_id))?.clone();
        let ub = |u: &Uncertain<bool>, a: &str| match u {
            Uncertain::Known(b) => Val::Bool(*b),
            Uncertain::Unknown(r) => unknown(r, &d.id, a),
        };
        let r = w.insert(
            "Deposit",
            Record {
                id: d.id.clone(),
                fields: vec![
                    ("holder".into(), holder),
                    ("balance".into(), Val::Num(d.balance.clone())),
                    ("within_30_days".into(), Val::Bool(d.within_30_days)),
                    ("called".into(), Val::Bool(d.called)),
                    ("fcs_eligible".into(), ub(&d.fcs_eligible, "fcs_eligible")),
                    ("operational_service".into(), ub(&d.operational_service, "operational_service")),
                    (
                        "operational_balance".into(),
                        match &d.operational_balance {
                            Uncertain::Known(q) => Val::Num(q.clone()),
                            Uncertain::Unknown(r) => unknown(r, &d.id, "operational_balance"),
                        },
                    ),
                ],
            },
        );
        by.entry(d.customer_id.clone()).or_default().push(r);
    }
    for (cid, ds) in by {
        let Val::Rec(t, i) = &cref[&cid] else { unreachable!() };
        w.set_field(t, *i, "deposits", Val::List(ds));
    }
    let day = Val::Date(days_from_civil(i64::from(reporting_date.year()), reporting_date.month(), reporting_date.day()));
    let out = Interp::new(module(), &w, Options { mode, sense: None, params: vec![("reporting_date".into(), day)] })?.run()?;
    let mut slices = Vec::new();
    for s in out.slices {
        let lcr_item = match s.output("lcr") {
            Some(Val::Str(i)) => Some(i.clone()),
            Some(Val::None) => None,
            other => return Err(format!("{}: lcr {other:?}", s.subject)),
        };
        let nsfr = match s.output("nsfr") {
            Some(Val::Tuple(t)) => match t.as_slice() {
                [Val::Str(a), Val::Str(b)] => (a.clone(), b.clone()),
                other => return Err(format!("nsfr {other:?}")),
            },
            other => return Err(format!("{}: nsfr {other:?}", s.subject)),
        };
        slices.push(WSlice { covered: s.splits.iter().any(|(name, value)| name == "covered" && *value), account: s.subject, amount: s.amount, lcr_item, nsfr });
    }
    slices.sort();
    Ok((slices, out.allocations))
}

/// The Lean model generated from the spec, measured on ARF 210.1A and
/// ARF 210.6, as written to lean/Prudence/Wholesale.lean.
pub fn lean_source() -> Result<String, String> {
    prudence_core::syntax::lean::generate(module(), &arf210::lean_targets()?)
}
