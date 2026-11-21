//! Debt securities issued, regulatory capital and deposits placed with other
//! ADIs: specs/funding/funding.prd.

use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::parse_module;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/funding/funding.prd");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| parse_module(SOURCE, &[]).unwrap_or_else(|e| panic!("funding.prd:\n{}", e.join("\n"))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityKind {
    ShortTerm,
    EuroCommercialPaper,
    UsCommercialPaper,
    LongTerm,
    CoveredBond,
    ShortTermSecuritisation,
    LongTermSecuritisation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    CET1,
    AT1,
    T2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdiType {
    MajorBank,
    LcrAdi,
    OtherAdi,
    ForeignAdi,
}

#[derive(Clone, Debug)]
pub struct IssuedSecurity {
    pub id: String,
    pub kind: SecurityKind,
    pub amount: Q,
    pub maturity: NaiveDate,
    pub offshore: bool,
}

#[derive(Clone, Debug)]
pub struct CapitalInstrument {
    pub id: String,
    pub tier: Tier,
    pub amount: Q,
    pub maturity: Option<NaiveDate>,
}

#[derive(Clone, Debug)]
pub struct Placement {
    pub id: String,
    pub adi: AdiType,
    pub amount: Q,
    pub maturity: Option<NaiveDate>,
    pub operational: Uncertain<bool>,
}

/// A holder's request to repay a security early (ARF 210.5 item 8).
#[derive(Clone, Debug)]
pub struct EarlyRepaymentRequest {
    pub security: String,
    pub requested: NaiveDate,
    pub amount: Q,
    pub approved: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Book {
    pub securities: Vec<IssuedSecurity>,
    pub capital: Vec<CapitalInstrument>,
    pub placements: Vec<Placement>,
    pub early_repayment_requests: Vec<EarlyRepaymentRequest>,
}

fn date(d: NaiveDate) -> Val {
    Val::Date(days_from_civil(i64::from(d.year()), d.month(), d.day()))
}
fn ctor<T: std::fmt::Debug>(x: T) -> Val {
    Val::Ctor(format!("{x:?}"), vec![])
}

fn load(b: &Book) -> World {
    let mut w = World::default();
    for s in &b.securities {
        w.insert(
            "IssuedSecurity",
            Record {
                id: s.id.clone(),
                fields: vec![
                    ("kind".into(), ctor(s.kind)),
                    ("amount".into(), Val::Num(s.amount.clone())),
                    ("maturity".into(), date(s.maturity)),
                    ("offshore".into(), Val::Bool(s.offshore)),
                ],
            },
        );
    }
    for c in &b.capital {
        w.insert(
            "CapitalInstrument",
            Record {
                id: c.id.clone(),
                fields: vec![
                    ("tier".into(), ctor(c.tier)),
                    ("amount".into(), Val::Num(c.amount.clone())),
                    ("maturity".into(), c.maturity.map(date).unwrap_or(Val::None)),
                ],
            },
        );
    }
    for p in &b.placements {
        let op = match &p.operational {
            Uncertain::Known(x) => Val::Bool(*x),
            Uncertain::Unknown(r) => Val::Unknown { reason: r.clone(), subject: p.id.clone(), attribute: "operational".into() },
        };
        w.insert(
            "Placement",
            Record {
                id: p.id.clone(),
                fields: vec![
                    ("adi".into(), ctor(p.adi)),
                    ("amount".into(), Val::Num(p.amount.clone())),
                    ("maturity".into(), p.maturity.map(date).unwrap_or(Val::None)),
                    ("operational".into(), op),
                ],
            },
        );
    }
    w
}

/// What each record reports: (record, form, item, column, AUD). Form is
/// "ARF 210.1A", "ARF 210.6" or "ARF 210.2"; column "amount" for single-column
/// cells.
pub struct Run {
    pub cells: Vec<(String, &'static str, String, String, Q)>,
    pub assumptions: std::collections::BTreeMap<(String, String), (String, String)>,
}

pub fn run(book: &Book, reporting_date: NaiveDate, mode: Mode) -> Result<Run, String> {
    let w = load(book);
    let it = Interp::new(module(), &w, Options { mode, sense: None, params: vec![("reporting_date".into(), date(reporting_date))] })?;
    let s = |v: Val| match v {
        Val::Str(x) => Ok(x),
        other => Err(format!("expected a string, got {other:?}")),
    };
    let mut cells = Vec::new();
    let mut push = |id: &str, form: &'static str, item: String, col: String, q: &Q| {
        if item != "none" && *q != Q::default() {
            cells.push((id.to_string(), form, item, col, q.clone()));
        }
    };
    for (k, x) in book.securities.iter().enumerate() {
        let r = Val::Rec("IssuedSecurity".into(), k);
        push(&x.id, "ARF 210.1A", s(it.call_fn("security_lcr_row", vec![r.clone()])?)?, "amount".into(), &x.amount);
        let item = s(it.call_fn("security_nsfr_item", vec![r.clone()])?)?;
        push(&x.id, "ARF 210.6", item, s(it.call_fn("security_bucket", vec![r])?)?, &x.amount);
    }
    for (k, x) in book.capital.iter().enumerate() {
        let r = Val::Rec("CapitalInstrument".into(), k);
        push(&x.id, "ARF 210.1A", s(it.call_fn("capital_lcr_row", vec![r.clone()])?)?, "amount".into(), &x.amount);
        let item = s(it.call_fn("capital_nsfr_item", vec![r.clone()])?)?;
        push(&x.id, "ARF 210.6", item, s(it.call_fn("capital_bucket", vec![r])?)?, &x.amount);
    }
    for (k, x) in book.placements.iter().enumerate() {
        let r = Val::Rec("Placement".into(), k);
        push(&x.id, "ARF 210.1A", s(it.call_fn("placement_lcr_row", vec![r.clone()])?)?, "amount".into(), &x.amount);
        let item = s(it.call_fn("placement_nsfr_item", vec![r.clone()])?)?;
        push(&x.id, "ARF 210.6", item, s(it.call_fn("placement_bucket", vec![r.clone()])?)?, &x.amount);
        push(&x.id, "ARF 210.2", s(it.call_fn("placement_mlh_row", vec![r])?)?, "amount".into(), &x.amount);
    }
    Ok(Run { cells, assumptions: it.assumptions() })
}
