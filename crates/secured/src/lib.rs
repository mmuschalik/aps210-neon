//! Secured funding and lending (APS 210 Att A para 8; ARF 210.1A items 11, 19
//! and the adjusted HQLA items 1.5.1, 2.5.1, 3.4.1, 3.5.1).
//!
//! The rules are the Prudence module specs/secured/secured_funding.prd. This
//! crate holds the data model, the loader into Prudence records, and the two
//! views the module defines: what each transaction reports in ARF 210.1A, and
//! the mechanical 30-day unwind. lean/Prudence/UnwindProofs.lean proves the
//! two agree for every portfolio.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::parse_module;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/secured/secured_funding.prd");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| parse_module(SOURCE, &[]).unwrap_or_else(|e| panic!("secured_funding.prd:\n{}", e.join("\n"))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HqlaClass {
    Hqla1,
    Hqla2a,
    RmbsHqla2b,
    NonRmbsHqla2b,
    NonHqla,
}

impl HqlaClass {
    pub const ALL: [HqlaClass; 5] = [Self::Hqla1, Self::Hqla2a, Self::RmbsHqla2b, Self::NonRmbsHqla2b, Self::NonHqla];
    fn ctor(self) -> &'static str {
        match self {
            Self::Hqla1 => "HQLA1",
            Self::Hqla2a => "HQLA2A",
            Self::RmbsHqla2b => "RmbsHqla2B",
            Self::NonRmbsHqla2b => "NonRmbsHqla2B",
            Self::NonHqla => "NonHqla",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leg {
    /// Repo: we received cash against collateral.
    Funding,
    /// Reverse repo: we extended cash against collateral.
    Lending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counterparty {
    OffshoreCentralBank,
    DomesticSovereignOrPse,
    Other,
}

/// ACTUS REP contract.
#[derive(Clone, Debug)]
pub struct Repo {
    pub id: String,
    pub leg: Leg,
    pub cash: Q,
    pub collateral_class: HqlaClass,
    pub collateral_value: Q,
    pub counterparty: Counterparty,
    pub operational: Uncertain<bool>,
    pub rehypothecated: bool,
    pub maturity: NaiveDate,
}

fn date(d: NaiveDate) -> Val {
    Val::Date(days_from_civil(i64::from(d.year()), d.month(), d.day()))
}

pub fn load(repos: &[Repo]) -> World {
    let mut w = World::default();
    for r in repos {
        let ctor = |n: &str| Val::Ctor(n.into(), vec![]);
        w.insert(
            "Repo",
            Record {
                id: r.id.clone(),
                fields: vec![
                    ("leg".into(), ctor(if r.leg == Leg::Funding { "Funding" } else { "Lending" })),
                    ("cash".into(), Val::Num(r.cash.clone())),
                    ("collateral_class".into(), ctor(r.collateral_class.ctor())),
                    ("collateral_value".into(), Val::Num(r.collateral_value.clone())),
                    (
                        "counterparty".into(),
                        ctor(match r.counterparty {
                            Counterparty::OffshoreCentralBank => "OffshoreCentralBank",
                            Counterparty::DomesticSovereignOrPse => "DomesticSovereignOrPse",
                            Counterparty::Other => "OtherCounterparty",
                        }),
                    ),
                    (
                        "operational".into(),
                        match &r.operational {
                            Uncertain::Known(b) => Val::Bool(*b),
                            Uncertain::Unknown(reason) => Val::Unknown {
                                reason: reason.clone(),
                                subject: r.id.clone(),
                                attribute: "operational".into(),
                            },
                        },
                    ),
                    ("rehypothecated".into(), Val::Bool(r.rehypothecated)),
                    ("contract".into(), Val::Ctor("REP".into(), vec![date(r.maturity)])),
                ],
            },
        );
    }
    w
}

/// The ARF 210.1A items a secured transaction can report into (items 11 and 19).
pub fn secured_items() -> Vec<&'static str> {
    arf210::lcr_all()
        .items
        .iter()
        .filter(|it| (it.id.starts_with("11.") || it.id.starts_with("19.")) && !it.header)
        .filter(|it| matches!(it.cell("amount"), Some(prudence_core::Cell::Input { .. })))
        .map(|it| it.id.as_str())
        .collect()
}

pub struct Run {
    /// ARF 210.1A column-1 amounts (AUD), from `contribution`.
    pub items: BTreeMap<String, Q>,
    /// Mechanical unwind per HQLA class (AUD), from `unwind`.
    pub unwind: BTreeMap<HqlaClass, Q>,
    pub assumptions: BTreeMap<(String, String), (String, String)>,
}

/// Run the module over a portfolio as at `reporting_date`.
pub fn run(repos: &[Repo], reporting_date: NaiveDate, mode: Mode) -> Result<Run, String> {
    let world = load(repos);
    let opts = Options { mode, sense: None, params: vec![("reporting_date".into(), date(reporting_date))] };
    let it = Interp::new(module(), &world, opts)?;
    let num = |v: Val| match v {
        Val::Num(q) => Ok(q),
        other => Err(format!("expected a number, got {other:?}")),
    };
    let mut items: BTreeMap<String, Q> = secured_items().into_iter().map(|i| (i.to_string(), Q::default())).collect();
    let mut unwind: BTreeMap<HqlaClass, Q> = HqlaClass::ALL.iter().map(|k| (*k, Q::default())).collect();
    for (n, _) in repos.iter().enumerate() {
        let r = Val::Rec("Repo".into(), n);
        for (item, total) in items.iter_mut() {
            *total += num(it.call_fn("contribution", vec![r.clone(), Val::Str(item.clone())])?)?;
        }
        for (k, total) in unwind.iter_mut() {
            *total += num(it.call_fn("unwind", vec![r.clone(), Val::Ctor(k.ctor().into(), vec![])])?)?;
        }
    }
    Ok(Run { items, unwind, assumptions: it.assumptions() })
}

/// ARF 210.1A inputs in AUD millions (the form's unit).
pub fn arf_inputs(run: &Run) -> BTreeMap<String, Q> {
    let million = Q::from_integer(1_000_000.into());
    run.items.iter().map(|(k, v)| (k.clone(), v / &million)).collect()
}

/// Generated Lean: the module plus the ARF 210.1A adjustment formulas, as
/// written to lean/Prudence/SecuredFunding.lean.
pub const ADJUSTMENTS: [(&str, &str); 4] =
    [("1.5.1", "arf_1_5_1"), ("2.5.1", "arf_2_5_1"), ("3.4.1", "arf_3_4_1"), ("3.5.1", "arf_3_5_1")];

pub fn lean_source() -> Result<String, String> {
    use prudence_core::syntax::lean::{form_cells, generate};
    let model = generate(module(), &[])?;
    let cells = form_cells(arf210::lcr_all(), &ADJUSTMENTS)?;
    let end = "end SecuredFunding\n";
    let body = model.strip_suffix(end).ok_or("unexpected generated tail")?;
    Ok(format!("{body}-- ARF 210.1A adjustment formulas\n\n{cells}{end}"))
}
