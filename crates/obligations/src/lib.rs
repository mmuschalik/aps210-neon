//! Off-balance sheet outflows (ARF 210.1A items 14, 15, 16). The rules are the
//! Prudence Lean 4 spec `lean/Prudence/Specs/Modules/Commitments.lean`; this crate holds the data
//! model, the loader, and `run()`, which fills the form's input items.

pub mod lookback;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::load_module_json;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/ir/modules/commitments.json");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| load_module_json(SOURCE, &[]).unwrap_or_else(|e| panic!("commitments.json:\n{}", e.join("\n"))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counterparty {
    RetailCustomer,
    Sme,
    NonFinancialCorporate,
    SovereignCentralBankPseMdb,
    RegulatedBank,
    OtherFinancialInstitution,
    OtherLegalEntity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacilityKind {
    CreditFacility,
    LiquidityFacility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObligationKind {
    TradeFinance,
    Guarantee,
    ShortTermDebtBuyback,
    LongTermDebtBuyback,
    AffiliatedDealerBuyback,
    StructuredProductsAndFunds,
    OtherNonContractual,
    OtherContractual,
}

#[derive(Clone, Debug)]
pub struct Facility {
    pub id: String,
    pub counterparty: Counterparty,
    pub kind: FacilityKind,
    pub committed: bool,
    pub undrawn: Q,
    pub drawable_within_30_days: Uncertain<bool>,
    pub mortgage_redraw: bool,
    pub funds_vehicle: bool,
    /// Facility size, drawn plus undrawn (ARF 210.5 item 3 percentages).
    pub limit: Option<Q>,
}

#[derive(Clone, Debug)]
pub struct LendingCommitment {
    pub id: String,
    pub counterparty: Counterparty,
    pub amount: Q,
}

#[derive(Clone, Debug)]
pub struct ContingentObligation {
    pub id: String,
    pub kind: ObligationKind,
    pub amount: Q,
}

#[derive(Clone, Debug, Default)]
pub struct Book {
    pub facilities: Vec<Facility>,
    pub commitments: Vec<LendingCommitment>,
    pub obligations: Vec<ContingentObligation>,
}

fn ctor(s: String) -> Val {
    Val::Ctor(s, vec![])
}

pub fn load(b: &Book) -> World {
    let mut w = World::default();
    for f in &b.facilities {
        let drawable = match &f.drawable_within_30_days {
            Uncertain::Known(x) => Val::Bool(*x),
            Uncertain::Unknown(r) => {
                Val::Unknown { reason: r.clone(), subject: f.id.clone(), attribute: "drawable_within_30_days".into() }
            }
        };
        w.insert(
            "Facility",
            Record {
                id: f.id.clone(),
                fields: vec![
                    ("counterparty".into(), ctor(format!("{:?}", f.counterparty))),
                    ("kind".into(), ctor(format!("{:?}", f.kind))),
                    ("committed".into(), Val::Bool(f.committed)),
                    ("undrawn".into(), Val::Num(f.undrawn.clone())),
                    ("drawable_within_30_days".into(), drawable),
                    ("mortgage_redraw".into(), Val::Bool(f.mortgage_redraw)),
                    ("funds_vehicle".into(), Val::Bool(f.funds_vehicle)),
                ],
            },
        );
    }
    for c in &b.commitments {
        w.insert(
            "LendingCommitment",
            Record {
                id: c.id.clone(),
                fields: vec![
                    ("counterparty".into(), ctor(format!("{:?}", c.counterparty))),
                    ("amount".into(), Val::Num(c.amount.clone())),
                ],
            },
        );
    }
    for o in &b.obligations {
        w.insert(
            "ContingentObligation",
            Record {
                id: o.id.clone(),
                fields: vec![("kind".into(), ctor(format!("{:?}", o.kind))), ("amount".into(), Val::Num(o.amount.clone()))],
            },
        );
    }
    w
}

/// ARF 210.1A input items in sections 14-16.
pub fn items() -> Vec<&'static str> {
    arf210::lcr_all()
        .items
        .iter()
        .filter(|it| ["14.", "15.", "16."].iter().any(|p| it.id.starts_with(p)))
        .filter(|it| matches!(it.cell("amount"), Some(prudence_core::Cell::Input { .. })))
        .map(|it| it.id.as_str())
        .collect()
}

pub struct Run {
    /// ARF 210.1A column-1 amounts (AUD).
    pub items: BTreeMap<String, Q>,
    /// Non-zero (record id, item, AUD) contributions behind `items`.
    pub parts: Vec<(String, String, Q)>,
    /// Non-zero (record id, ARF 210.6 item 48-53, AUD) contributions for NSFR.
    pub nsfr_parts: Vec<(String, String, Q)>,
    pub assumptions: BTreeMap<(String, String), (String, String)>,
}

pub fn run(book: &Book, reporting_date: NaiveDate, mode: Mode) -> Result<Run, String> {
    let world = load(book);
    let day = Val::Date(days_from_civil(i64::from(reporting_date.year()), reporting_date.month(), reporting_date.day()));
    let it = Interp::new(module(), &world, Options { mode, sense: None, params: vec![("reporting_date".into(), day)] })?;
    let mut items: BTreeMap<String, Q> = items().into_iter().map(|i| (i.to_string(), Q::default())).collect();
    let mut parts = Vec::new();
    let mut nsfr_parts = Vec::new();
    let zero = Q::default();
    let ids = |t: &str| -> Vec<String> {
        match t {
            "Facility" => book.facilities.iter().map(|x| x.id.clone()).collect(),
            "LendingCommitment" => book.commitments.iter().map(|x| x.id.clone()).collect(),
            _ => book.obligations.iter().map(|x| x.id.clone()).collect(),
        }
    };
    for (table, f) in [("Facility", "facility"), ("LendingCommitment", "commitment"), ("ContingentObligation", "obligation")] {
        for (k, id) in ids(table).into_iter().enumerate() {
            for (item, total) in items.iter_mut() {
                match it.call_fn(f, vec![Val::Rec(table.into(), k), Val::Str(item.clone())])? {
                    Val::Num(q) => {
                        if q != zero {
                            parts.push((id.clone(), item.clone(), q.clone()));
                        }
                        *total += q
                    }
                    other => return Err(format!("{f}: expected a number, got {other:?}")),
                }
            }
        }
    }
    // ARF 210.6 off-balance sheet items 48-53 (APS 210 Att C Table 1 & para 42).
    for f in &book.facilities {
        if f.undrawn > zero {
            let item = if f.mortgage_redraw {
                "49"
            } else {
                match (f.committed, f.kind) {
                    (true, FacilityKind::LiquidityFacility) => "48",
                    (true, FacilityKind::CreditFacility) => "49",
                    (false, FacilityKind::LiquidityFacility) => "50",
                    (false, FacilityKind::CreditFacility) => "51",
                }
            };
            nsfr_parts.push((f.id.clone(), item.to_string(), f.undrawn.clone()));
        }
    }
    for c in &book.commitments {
        if c.amount > zero {
            nsfr_parts.push((c.id.clone(), "49".to_string(), c.amount.clone()));
        }
    }
    // Items 52 & 53 (Att C para 42): actual 12-month total net outflow, floored at 0.
    // ContingentObligation.amount stores the 12-month monthly average used in LCR 16.2/16.3,
    // so multiply by 12 and net per category before flooring at 0 (ARS p.157 checks 52>=0, 53>=0).
    let twelve = Q::from_integer(12.into());
    let mut tf_net = Q::default();
    let mut guar_net = Q::default();
    for o in &book.obligations {
        match o.kind {
            ObligationKind::TradeFinance => tf_net += &o.amount * &twelve,
            ObligationKind::Guarantee => guar_net += &o.amount * &twelve,
            _ => {}
        }
    }
    if tf_net > zero {
        nsfr_parts.push(("trade-finance-12m".into(), "52".to_string(), tf_net));
    }
    if guar_net > zero {
        nsfr_parts.push(("guarantees-12m".into(), "53".to_string(), guar_net));
    }
    Ok(Run { items, parts, nsfr_parts, assumptions: it.assumptions() })
}

/// In AUD millions (the form's unit).
pub fn arf_inputs(run: &Run) -> BTreeMap<String, Q> {
    let million = Q::from_integer(1_000_000.into());
    run.items.iter().map(|(k, v)| (k.clone(), v / &million)).collect()
}
