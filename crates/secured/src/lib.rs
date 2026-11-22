//! Secured funding and lending (APS 210 Att A para 8; ARF 210.1A items 11, 19
//! and the adjusted HQLA items 1.5.1, 2.5.1, 3.4.1, 3.5.1).
//!
//! The rules are the Prudence module specs/secured/secured_funding.prd. This
//! crate holds the data model, the loader into Prudence records, and the two
//! views the module defines: what each transaction reports in ARF 210.1A, and
//! the mechanical 30-day unwind. lean/Prudence/UnwindProofs.lean proves the
//! two agree for every portfolio.

pub mod hqla_rules;
pub mod pool;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::{add_months, days_from_civil};
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::load_module_json;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/ir/modules/secured_funding.json");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| load_module_json(SOURCE, &["Arf210_1A"]).unwrap_or_else(|e| panic!("secured_funding.json:\n{}", e.join("\n"))))
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
    /// Non-zero (record id, ARF 210.6 item, column, AUD) contributions for NSFR.
    pub nsfr_parts: Vec<(String, String, String, Q)>,
    pub assumptions: BTreeMap<(String, String), (String, String)>,
}

fn nsfr_maturity_bucket(maturity: Option<NaiveDate>, default_when_none: &'static str, rd_days: i64) -> &'static str {
    let Some(m) = maturity else {
        return default_when_none;
    };
    let m_days = days_from_civil(i64::from(m.year()), m.month(), m.day());
    if m_days < add_months(rd_days, 6) {
        "b0"
    } else if m_days < add_months(rd_days, 12) {
        "b1"
    } else {
        "b2"
    }
}

/// Classify a `Holding` into its ARF 210.6 `(item, column)` per APS 210 Att C paras 29-36.
fn holding_nsfr(h: &Holding, rd_days: i64) -> (&'static str, &'static str) {
    match h.kind {
        AssetKind::NotesAndCoin => return ("20", "b0"),
        AssetKind::RbaBalance | AssetKind::ForeignCentralBankBalance => {
            return ("21", nsfr_maturity_bucket(h.maturity, "b0", rd_days));
        }
        _ => {}
    }
    let enc = h.encumbered;
    let bucket = nsfr_maturity_bucket(h.maturity, "b2", rd_days);
    // HQLA1 (Att C para 30: item 25.1 / 25.2.1)
    let is_hqla1 = h.kind == AssetKind::DebtSecurity
        && ((h.risk_weight == 0
            && matches!(
                h.issuer,
                Issuer::AustralianGovernment
                    | Issuer::StateCentralBorrowingAuthority
                    | Issuer::ForeignSovereign
                    | Issuer::GuaranteedByAustralianGovernment
                    | Issuer::GuaranteedByForeignSovereign
                    | Issuer::CentralBank
                    | Issuer::PublicSectorEntity
                    | Issuer::Supranational
            ))
            || (matches!(h.issuer, Issuer::ForeignSovereign | Issuer::CentralBank) && h.risk_weight > 20));
    if is_hqla1 {
        // Without an encumbrance end date, conservatively treat encumbered holdings as >= 1y (Att C para 23).
        return (if enc { "25.2.3" } else { "25.1" }, bucket);
    }
    // HQLA2A (Att C para 33(a): item 30.1 / 30.2.3)
    let is_hqla2a = (h.kind == AssetKind::DebtSecurity
        && ((h.risk_weight == 20
            && matches!(
                h.issuer,
                Issuer::ForeignSovereign | Issuer::CentralBank | Issuer::PublicSectorEntity | Issuer::Supranational
            ))
            || (h.issuer == Issuer::NonFinancialCorporateIssuer && h.rating_grade == 1)))
        || (h.kind == AssetKind::CoveredBond && h.rating_grade == 1 && !h.own_or_associated);
    if is_hqla2a {
        return (if enc { "30.2.3" } else { "30.1" }, bucket);
    }
    // HQLA2B debt / RMBS (Att C para 34(a): item 31.1 / 31.2.2)
    let is_hqla2b = (h.kind == AssetKind::DebtSecurity
        && h.issuer == Issuer::NonFinancialCorporateIssuer
        && (2..=3).contains(&h.rating_grade))
        || (h.kind == AssetKind::Rmbs && h.rating_grade == 1 && !h.own_or_associated);
    if is_hqla2b {
        return (if enc { "31.2.2" } else { "31.1" }, bucket);
    }
    // ASX200 non-FI equities (Att C para 34(e): item 34.1 / 34.2.2, column b2 only)
    if h.kind == AssetKind::Equity && h.issuer == Issuer::NonFinancialCorporateIssuer && h.index_constituent {
        return (if enc { "34.2.2" } else { "34.1" }, "b2");
    }
    // All other non-HQLA securities / exchange-traded equities (Att C para 36(c)-(d): item 38.1 / 38.2.2).
    // Note: CLF 10% RSF (items 26/27) is capped by LCR CLF (Att C para 32) and handled in assembly::aps210_rules.
    (if enc { "38.2.2" } else { "38.1" }, bucket)
}

/// Classify a `Repo` into its ARF 210.6 `(item, column)` per APS 210 Att C paras 14-15, 29(c), 31, 33(b), 35(b).
fn repo_nsfr(r: &Repo, rd_days: i64) -> (&'static str, &'static str) {
    let bucket = nsfr_maturity_bucket(Some(r.maturity), "b0", rd_days);
    match r.leg {
        Leg::Lending => match r.counterparty {
            // Claims on central banks < 1 year -> 32.1 (0% <6m, 50% 6m-1y); >= 1y -> 36.1:b2 (<=35% RW).
            Counterparty::OffshoreCentralBank => {
                if bucket == "b2" {
                    ("36.1", "b2")
                } else {
                    ("32.1", bucket)
                }
            }
            // Loans to domestic sovereign/PSE (non-FI, <=35% RW) -> 36.1 (50%/50%/65% RSF).
            Counterparty::DomesticSovereignOrPse => ("36.1", bucket),
            // Loans to FIs: HQLA1 freely re-hypothecable -> 23.1 (10%/50%/100%), else 24.1 (15%/50%/100%).
            Counterparty::Other => {
                if r.collateral_class == HqlaClass::Hqla1 && !r.rehypothecated {
                    ("23.1", bucket)
                } else {
                    ("24.1", bucket)
                }
            }
        },
        Leg::Funding => match r.counterparty {
            Counterparty::OffshoreCentralBank => ("11", bucket),
            Counterparty::DomesticSovereignOrPse => ("10", bucket),
            Counterparty::Other => ("12", bucket),
        },
    }
}

/// Run the module over holdings and secured transactions as at `reporting_date`.
pub fn run(holdings: &[Holding], repos: &[Repo], reporting_date: NaiveDate, mode: Mode, has_clf: bool) -> Result<Run, String> {
    let world = load(holdings, repos);
    let rd_days = days_from_civil(i64::from(reporting_date.year()), reporting_date.month(), reporting_date.day());
    let opts = Options {
        mode,
        sense: None,
        params: vec![("reporting_date".into(), Val::Date(rd_days)), ("has_clf".into(), Val::Bool(has_clf))],
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
    let mut nsfr_parts = Vec::new();
    // Holdings: one role each, from the linear partition.
    let out = it.run()?;
    for sl in &out.slices {
        let value = match sl.output("value") {
            Some(v) => num(v.clone())?,
            None => sl.amount.clone(),
        };
        for key in ["lcr", "memo"] {
            if let Some(Val::Str(item)) = sl.output(key)
                && value != Q::default()
            {
                let total = stock.get_mut(item).ok_or_else(|| format!("{}: {item} is not a Section A input", sl.subject))?;
                *total += value.clone();
                parts.push((sl.subject.clone(), item.clone(), value.clone()));
            }
        }
    }
    for h in holdings {
        if h.market_value != Q::default() {
            let (item, col) = holding_nsfr(h, rd_days);
            nsfr_parts.push((h.id.clone(), item.to_string(), col.to_string(), h.market_value.clone()));
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
        if repo.cash != Q::default() {
            let (item, col) = repo_nsfr(repo, rd_days);
            nsfr_parts.push((repo.id.clone(), item.to_string(), col.to_string(), repo.cash.clone()));
        }
    }
    items.extend(stock);
    Ok(Run { items, unwind, parts, nsfr_parts, assumptions: it.assumptions() })
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
