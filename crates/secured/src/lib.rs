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
    /// RBA repo-eligible, not HQLA (CLF collateral).
    ClfEligible,
    NonHqla,
}

impl HqlaClass {
    pub const ALL: [HqlaClass; 6] =
        [Self::Hqla1, Self::Hqla2a, Self::RmbsHqla2b, Self::NonRmbsHqla2b, Self::ClfEligible, Self::NonHqla];
    fn ctor(self) -> &'static str {
        match self {
            Self::Hqla1 => "HQLA1",
            Self::Hqla2a => "HQLA2A",
            Self::RmbsHqla2b => "RmbsHqla2B",
            Self::NonRmbsHqla2b => "NonRmbsHqla2B",
            Self::ClfEligible => "ClfEligible",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetKind {
    NotesAndCoin,
    RbaBalance,
    ForeignCentralBankBalance,
    DebtSecurity,
    CoveredBond,
    Rmbs,
    Equity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Issuer {
    AustralianGovernment,
    StateCentralBorrowingAuthority,
    ForeignSovereign,
    GuaranteedByAustralianGovernment,
    GuaranteedByForeignSovereign,
    CentralBank,
    PublicSectorEntity,
    Supranational,
    NonFinancialCorporateIssuer,
    FinancialInstitutionIssuer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClfCategory {
    SupranationalOrForeignGovernment,
    Guaranteed,
    AdiIssued,
    AssetBacked,
    OtherPrivate,
    SelfSecuritised,
    NotClfEligible,
}

/// A holding in the stock of liquid assets.
#[derive(Clone, Debug)]
pub struct Holding {
    pub id: String,
    pub kind: AssetKind,
    pub issuer: Issuer,
    pub market_value: Q,
    pub risk_weight: i64,
    pub rating_grade: i64,
    pub domestic_currency: bool,
    pub index_constituent: bool,
    pub own_or_associated: bool,
    pub encumbered: bool,
    pub operational: Uncertain<bool>,
    /// Central bank balances: can be drawn down in a stress.
    pub drawable_in_stress: Uncertain<bool>,
    pub clf_category: ClfCategory,
    pub value_after_rba_margin: Q,
    pub maturity: Option<NaiveDate>,
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

fn uncertain(u: &Uncertain<bool>, subject: &str, attribute: &str) -> Val {
    match u {
        Uncertain::Known(b) => Val::Bool(*b),
        Uncertain::Unknown(reason) => {
            Val::Unknown { reason: reason.clone(), subject: subject.into(), attribute: attribute.into() }
        }
    }
}

pub fn load(holdings: &[Holding], repos: &[Repo]) -> World {
    let mut w = World::default();
    for h in holdings {
        let ctor = |n: String| Val::Ctor(n, vec![]);
        w.insert(
            "Holding",
            Record {
                id: h.id.clone(),
                fields: vec![
                    ("kind".into(), ctor(format!("{:?}", h.kind))),
                    ("issuer".into(), ctor(format!("{:?}", h.issuer))),
                    ("market_value".into(), Val::Num(h.market_value.clone())),
                    ("risk_weight".into(), Val::num(h.risk_weight)),
                    ("rating_grade".into(), Val::num(h.rating_grade)),
                    ("domestic_currency".into(), Val::Bool(h.domestic_currency)),
                    ("index_constituent".into(), Val::Bool(h.index_constituent)),
                    ("own_or_associated".into(), Val::Bool(h.own_or_associated)),
                    ("encumbered".into(), Val::Bool(h.encumbered)),
                    ("operational".into(), uncertain(&h.operational, &h.id, "operational")),
                    ("drawable_in_stress".into(), uncertain(&h.drawable_in_stress, &h.id, "drawable_in_stress")),
                    ("clf_category".into(), ctor(format!("{:?}", h.clf_category))),
                    ("value_after_rba_margin".into(), Val::Num(h.value_after_rba_margin.clone())),
                    ("maturity".into(), h.maturity.map(date).unwrap_or(Val::None)),
                ],
            },
        );
    }
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
                    ("operational".into(), uncertain(&r.operational, &r.id, "operational")),
                    ("rehypothecated".into(), Val::Bool(r.rehypothecated)),
                    ("contract".into(), Val::Ctor("REP".into(), vec![date(r.maturity)])),
                ],
            },
        );
    }
    w
}

/// ARF 210.1A Section A items a holding can report into: the stock
/// (1.1-3.3), the CLF (6.1.1-6.1.6, 6.4), and maturing securities (21.3).
pub fn stock_items() -> Vec<&'static str> {
    input_items(|id| {
        (["1.", "2.", "3."].iter().any(|p| id.starts_with(p))
            && !["1.5", "2.5", "3.4", "3.5"].iter().any(|p| id.starts_with(p)))
            || id.starts_with("6.1.")
            || id == "6.4"
            || id == "21.3"
    })
}

/// The ARF 210.1A items a secured transaction can report into (items 11 and 19).
pub fn secured_items() -> Vec<&'static str> {
    input_items(|id| id.starts_with("11.") || id.starts_with("19."))
}

fn input_items(keep: impl Fn(&str) -> bool) -> Vec<&'static str> {
    arf210::lcr_all()
        .items
        .iter()
        .filter(|it| keep(&it.id) && !it.header)
        .filter(|it| matches!(it.cell("amount"), Some(prudence_core::Cell::Input { .. })))
        .map(|it| it.id.as_str())
        .collect()
}

pub struct Run {
    /// ARF 210.1A column-1 amounts (AUD): Section A from `stock`, items 11
    /// and 19 from `contribution`.
    pub items: BTreeMap<String, Q>,
    /// Mechanical unwind per HQLA class (AUD), from `unwind`.
    pub unwind: BTreeMap<HqlaClass, Q>,
    /// Non-zero (record id, item, AUD) contributions behind `items`.
    pub parts: Vec<(String, String, Q)>,
    pub assumptions: BTreeMap<(String, String), (String, String)>,
}

/// Run the module over holdings and secured transactions as at `reporting_date`.
pub fn run(holdings: &[Holding], repos: &[Repo], reporting_date: NaiveDate, mode: Mode, has_clf: bool) -> Result<Run, String> {
    let world = load(holdings, repos);
    let opts = Options {
        mode,
        sense: None,
        params: vec![("reporting_date".into(), date(reporting_date)), ("has_clf".into(), Val::Bool(has_clf))],
    };
    let it = Interp::new(module(), &world, opts)?;
    let num = |v: Val| match v {
        Val::Num(q) => Ok(q),
        other => Err(format!("expected a number, got {other:?}")),
    };
    let mut items: BTreeMap<String, Q> = secured_items().into_iter().map(|i| (i.to_string(), Q::default())).collect();
    let mut stock: BTreeMap<String, Q> = stock_items().into_iter().map(|i| (i.to_string(), Q::default())).collect();
    let mut unwind: BTreeMap<HqlaClass, Q> = HqlaClass::ALL.iter().map(|k| (*k, Q::default())).collect();
    let mut parts = Vec::new();
    for (n, h) in holdings.iter().enumerate() {
        for (item, total) in stock.iter_mut() {
            let q = num(it.call_fn("holding_items", vec![Val::Rec("Holding".into(), n), Val::Str(item.clone())])?)?;
            if q != Q::default() {
                parts.push((h.id.clone(), item.clone(), q.clone()));
            }
            *total += q;
        }
    }
    for (n, repo) in repos.iter().enumerate() {
        let r = Val::Rec("Repo".into(), n);
        for (item, total) in items.iter_mut() {
            let q = num(it.call_fn("contribution", vec![r.clone(), Val::Str(item.clone())])?)?;
            if q != Q::default() {
                parts.push((repo.id.clone(), item.clone(), q.clone()));
            }
            *total += q;
        }
        for (k, total) in unwind.iter_mut() {
            *total += num(it.call_fn("unwind", vec![r.clone(), Val::Ctor(k.ctor().into(), vec![])])?)?;
        }
    }
    items.extend(stock);
    Ok(Run { items, unwind, parts, assumptions: it.assumptions() })
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
