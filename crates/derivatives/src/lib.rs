//! Derivatives and collateral: specs/derivatives/derivatives.prd.
//! ARF 210.1A items 13.1-13.7 and 21.1; ARF 210.6 items 13.1.x, 40.1.x, 42.1.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{Datelike, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::parse_module;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/derivatives/derivatives.prd");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| parse_module(SOURCE, &[]).unwrap_or_else(|e| panic!("derivatives.prd:\n{}", e.join("\n"))))
}

#[derive(Clone, Debug, Default)]
pub struct NettingSet {
    pub id: String,
    pub counterparty: String,
    pub netting_agreement: Option<bool>,
    pub collateral_received: Q,
    pub required_from_counterparty: Q,
    pub received_segregated: bool,
    pub hqla_substitutable: Q,
    pub due_not_called: Q,
    pub downgrade_3_notches: Q,
    pub posted_non_hqla1: Q,
    pub received_reusable: Q,
    pub variation_margin_posted: Q,
    pub cash_vm_received_eligible: Q,
    pub initial_margin_posted: Q,
}

#[derive(Clone, Debug)]
pub struct Trade {
    pub set: String,
    pub mtm: Q,
}

#[derive(Clone, Debug)]
pub struct Flow {
    pub set: String,
    pub date: NaiveDate,
    /// Positive: receivable.
    pub amount: Q,
    pub fx_principal: bool,
    pub option: bool,
    pub in_the_money: Uncertain<bool>,
}

/// A realised collateral flow (positive: received), bitemporal.
#[derive(Clone, Debug)]
pub struct CollateralFlow {
    pub set: String,
    pub amount: Q,
    pub valid: NaiveDate,
    pub recorded: NaiveDate,
}

#[derive(Clone, Debug, Default)]
pub struct Book {
    pub sets: Vec<NettingSet>,
    pub trades: Vec<Trade>,
    pub flows: Vec<Flow>,
    pub collateral_flows: Vec<CollateralFlow>,
}

pub fn date(d: NaiveDate) -> Val {
    Val::Date(days_from_civil(i64::from(d.year()), d.month(), d.day()))
}

pub fn load(b: &Book) -> Result<World, String> {
    let mut w = World::default();
    let mut refs = BTreeMap::new();
    for s in &b.sets {
        let n = |q: &Q| Val::Num(q.clone());
        let mna = match s.netting_agreement {
            Some(x) => Val::Bool(x),
            None => Val::Unknown {
                reason: "netting opinion not recorded".into(),
                subject: s.id.clone(),
                attribute: "netting_agreement".into(),
            },
        };
        let r = w.insert(
            "NettingSet",
            Record {
                id: s.id.clone(),
                fields: vec![
                    ("counterparty".into(), Val::Str(s.counterparty.clone())),
                    ("netting_agreement".into(), mna),
                    ("trades".into(), Val::List(vec![])),
                    ("flows".into(), Val::List(vec![])),
                    ("collateral_received".into(), n(&s.collateral_received)),
                    ("required_from_counterparty".into(), n(&s.required_from_counterparty)),
                    ("received_segregated".into(), Val::Bool(s.received_segregated)),
                    ("hqla_substitutable".into(), n(&s.hqla_substitutable)),
                    ("due_not_called".into(), n(&s.due_not_called)),
                    ("downgrade_3_notches".into(), n(&s.downgrade_3_notches)),
                    ("posted_non_hqla1".into(), n(&s.posted_non_hqla1)),
                    ("received_reusable".into(), n(&s.received_reusable)),
                    ("variation_margin_posted".into(), n(&s.variation_margin_posted)),
                    ("cash_vm_received_eligible".into(), n(&s.cash_vm_received_eligible)),
                    ("initial_margin_posted".into(), n(&s.initial_margin_posted)),
                ],
            },
        );
        refs.insert(s.id.clone(), r);
    }
    let set = |id: &str| refs.get(id).cloned().ok_or(format!("unknown netting set {id}"));
    let mut trades: BTreeMap<String, Vec<Val>> = BTreeMap::new();
    for (k, t) in b.trades.iter().enumerate() {
        let r = w.insert(
            "Trade",
            Record { id: format!("trade{k}"), fields: vec![("set".into(), set(&t.set)?), ("mtm".into(), Val::Num(t.mtm.clone()))] },
        );
        trades.entry(t.set.clone()).or_default().push(r);
    }
    let mut flows: BTreeMap<String, Vec<Val>> = BTreeMap::new();
    for (k, f) in b.flows.iter().enumerate() {
        let id = format!("flow{k}");
        let itm = match &f.in_the_money {
            Uncertain::Known(x) => Val::Bool(*x),
            Uncertain::Unknown(r) => Val::Unknown { reason: r.clone(), subject: id.clone(), attribute: "in_the_money".into() },
        };
        let r = w.insert(
            "DerivativeFlow",
            Record {
                id,
                fields: vec![
                    ("set".into(), set(&f.set)?),
                    ("date".into(), date(f.date)),
                    ("amount".into(), Val::Num(f.amount.clone())),
                    ("fx_principal".into(), Val::Bool(f.fx_principal)),
                    ("option".into(), Val::Bool(f.option)),
                    ("in_the_money".into(), itm),
                ],
            },
        );
        flows.entry(f.set.clone()).or_default().push(r);
    }
    for (id, r) in &refs {
        let Val::Rec(t, i) = r else { unreachable!() };
        w.set_field(t, *i, "trades", Val::List(trades.remove(id).unwrap_or_default()));
        w.set_field(t, *i, "flows", Val::List(flows.remove(id).unwrap_or_default()));
    }
    for (k, c) in b.collateral_flows.iter().enumerate() {
        w.insert(
            "CollateralFlow",
            Record {
                id: format!("collateral{k}"),
                fields: vec![
                    ("set".into(), set(&c.set)?),
                    ("amount".into(), Val::Num(c.amount.clone())),
                    ("valid".into(), date(c.valid)),
                    ("recorded".into(), date(c.recorded)),
                ],
            },
        );
    }
    Ok(w)
}

pub const LCR_ITEMS: [&str; 8] = ["13.1", "13.2", "13.3", "13.4", "13.5", "13.6", "13.7", "21.1"];
pub const NSFR_ITEMS: [&str; 5] = ["13.1.1", "13.1.2", "40.1.1", "40.1.2", "42.1"];

pub struct Run {
    /// ARF 210.1A amounts (AUD).
    pub lcr: BTreeMap<String, Q>,
    /// ARF 210.6 amounts (AUD).
    pub nsfr: BTreeMap<String, Q>,
    pub assumptions: BTreeMap<(String, String), (String, String)>,
}

fn num(v: Val) -> Result<Q, String> {
    match v {
        Val::Num(q) => Ok(q),
        other => Err(format!("expected a number, got {other:?}")),
    }
}

pub fn run(book: &Book, reporting_date: NaiveDate, known_at: NaiveDate, mode: Mode) -> Result<Run, String> {
    let world = load(book)?;
    let params = vec![("reporting_date".into(), date(reporting_date)), ("known_at".into(), date(known_at))];
    let it = Interp::new(module(), &world, Options { mode, sense: None, params })?;
    it.check_invariants()?;
    let mut lcr: BTreeMap<String, Q> = LCR_ITEMS.iter().map(|i| (i.to_string(), Q::default())).collect();
    let mut nsfr: BTreeMap<String, Q> = NSFR_ITEMS.iter().map(|i| (i.to_string(), Q::default())).collect();
    for k in 0..book.sets.len() {
        let s = Val::Rec("NettingSet".into(), k);
        for (item, total) in lcr.iter_mut() {
            *total += num(it.call_fn("cash_flows", vec![s.clone(), Val::Str(item.clone())])?)?;
            *total += num(it.call_fn("collateral", vec![s.clone(), Val::Str(item.clone())])?)?;
        }
        for (item, total) in nsfr.iter_mut() {
            *total += num(it.call_fn("nsfr", vec![s.clone(), Val::Str(item.clone())])?)?;
        }
    }
    lcr.insert("13.6".into(), num(it.call_fn("largest_collateral_flow", vec![])?)?);
    Ok(Run { lcr, nsfr, assumptions: it.assumptions() })
}

/// In AUD millions (the forms' unit).
pub fn arf_inputs(m: &BTreeMap<String, Q>) -> BTreeMap<String, Q> {
    let million = Q::from_integer(1_000_000.into());
    m.iter().map(|(k, v)| (k.clone(), v / &million)).collect()
}
