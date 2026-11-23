//! Derivatives and collateral: `lean/Prudence/Specs/Modules/Derivatives.lean`.
//! ARF 210.1A items 13.1-13.7 and 21.1; ARF 210.6 items 13.1.x, 40.1.x, 42.1.

pub mod collateral;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::{Datelike, Months, NaiveDate};
use prudence_core::Q;
use prudence_core::date::days_from_civil;
use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::syntax::ast::Module;
use prudence_core::syntax::load_module_json;
use prudence_core::uncertain::{Mode, Uncertain};

pub const SOURCE: &str = include_str!("../../../specs/ir/modules/derivatives.json");

pub fn module() -> &'static Module {
    static M: OnceLock<Module> = OnceLock::new();
    M.get_or_init(|| load_module_json(SOURCE, &[]).unwrap_or_else(|e| panic!("derivatives.json:\n{}", e.join("\n"))))
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
    /// Clearing venue (bilateral OTC, CCP-cleared, or exchange-traded future).
    pub clearing: collateral::ClearingKind,
    /// Initial margin posted on behalf of a customer (ARF 210.6 item 42.2, exempt memo).
    pub customer_initial_margin_posted: Q,
    /// Cash/assets contributed to a CCP default fund (ARF 210.6 item 42.3, 85% RSF).
    pub ccp_default_fund_contribution: Q,
    /// Optional granular CSA substitution rights (overrides scalar `hqla_substitutable` when non-empty).
    pub substitution_rights: Vec<collateral::SubstitutionRight>,
    /// Optional lot-level posted/received collateral (overrides `posted_non_hqla1`/`received_reusable` when non-empty).
    pub posted_lots: Vec<collateral::PostedCollateralLot>,
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
    pub currency: String,
    pub fx_principal: bool,
    pub fx_trading: bool,
    pub option: bool,
    pub in_the_money: Uncertain<bool>,
}

/// A realised collateral flow (positive: received), bitemporal.
#[derive(Clone, Debug)]
pub struct CollateralFlow {
    pub set: String,
    pub amount: Q,
    pub currency: String,
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
                    (
                        "hqla_substitutable".into(),
                        n(&if s.substitution_rights.is_empty() {
                            s.hqla_substitutable.clone()
                        } else {
                            collateral::substitution_outflow(&s.substitution_rights, false)
                        }),
                    ),
                    ("due_not_called".into(), n(&s.due_not_called)),
                    ("downgrade_3_notches".into(), n(&s.downgrade_3_notches)),
                    (
                        "posted_non_hqla1".into(),
                        n(&if s.posted_lots.is_empty() {
                            s.posted_non_hqla1.clone()
                        } else {
                            collateral::posted_non_hqla1_valuation_base(&s.posted_lots)
                        }),
                    ),
                    (
                        "received_reusable".into(),
                        n(&if s.posted_lots.is_empty() {
                            s.received_reusable.clone()
                        } else {
                            Q::default()
                        }),
                    ),
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
                    ("currency".into(), Val::Str(f.currency.clone())),
                    ("fx_principal".into(), Val::Bool(f.fx_principal)),
                    ("fx_trading".into(), Val::Bool(f.fx_trading)),
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
                    ("currency".into(), Val::Str(c.currency.clone())),
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
    pub nsfr_parts: Vec<(String, String, Q)>,
    /// Non-zero (netting set, item, AUD) contributions behind `lcr`; the
    /// lookback (13.6) is portfolio-wide, under the id "lookback".
    pub parts: Vec<(String, String, Q)>,
    /// Items 13.1 / 21.1 by currency for the split of items 18 and 23:
    /// (netting set, item, currency, AUD).
    pub by_currency: Vec<(String, String, String, Q)>,
    pub assumptions: BTreeMap<(String, String), (String, String)>,
}

fn num(v: Val) -> Result<Q, String> {
    match v {
        Val::Num(q) => Ok(q),
        other => Err(format!("expected a number, got {other:?}")),
    }
}

/// Exact O(N log N) sliding-window evaluation of `largest_collateral_flow_in`
/// (ARF 210.1A item 13.6, APS 210 Att A para 58), matching `Derivatives.lean`
/// bit-for-bit.
fn fast_largest_collateral_flow(
    book: &Book,
    reporting_date: NaiveDate,
    known_at: NaiveDate,
    mode: Mode,
    ccy: Option<&str>,
) -> Q {
    let rd = days_from_civil(i64::from(reporting_date.year()), reporting_date.month(), reporting_date.day());
    let ka = days_from_civil(i64::from(known_at.year()), known_at.month(), known_at.day());
    let lb_date = reporting_date.checked_sub_months(Months::new(24)).unwrap();
    let start_plus_30 = days_from_civil(i64::from(lb_date.year()), lb_date.month(), lb_date.day()) + 30;
    let zero = Q::default();
    let abs_q = |q: &Q| if *q < zero { -q } else { q.clone() };

    let set_nets: std::collections::HashMap<&str, bool> = book
        .sets
        .iter()
        .map(|s| (s.id.as_str(), s.netting_agreement.unwrap_or(mode == Mode::Optimistic)))
        .collect();

    // Filter realised collateral flows: valid <= reporting_date && recorded <= known_at.
    let mut events: Vec<(i64, &str, &Q)> = Vec::new();
    let mut ends = std::collections::BTreeSet::new();
    ends.insert(start_plus_30);
    ends.insert(rd);
    for e in &book.collateral_flows {
        if let Some(want) = ccy
            && e.currency != want
        {
            continue;
        }
        let v = days_from_civil(i64::from(e.valid.year()), e.valid.month(), e.valid.day());
        let r = days_from_civil(i64::from(e.recorded.year()), e.recorded.month(), e.recorded.day());
        if v <= rd && r <= ka {
            events.push((v, e.set.as_str(), &e.amount));
            if v >= start_plus_30 && v <= rd {
                ends.insert(v);
            }
            if v + 30 >= start_plus_30 && v + 30 <= rd {
                ends.insert(v + 30);
            }
        }
    }
    if events.is_empty() {
        return Q::default();
    }
    events.sort_by_key(|&(v, _, _)| v);

    // Two-pointer sweep across sorted window_ends for (end - 30, end].
    let mut per_set: std::collections::HashMap<&str, Q> = std::collections::HashMap::new();
    let mut left = 0usize;
    let mut right = 0usize;
    let mut best = Q::default();
    for end in ends {
        let lo = end - 30;
        while right < events.len() && events[right].0 <= end {
            let (_, sid, amt) = events[right];
            let nets = set_nets.get(sid).copied().unwrap_or(false);
            let delta = if nets { amt.clone() } else { abs_q(amt) };
            *per_set.entry(sid).or_default() += delta;
            right += 1;
        }
        while left < right && events[left].0 <= lo {
            let (_, sid, amt) = events[left];
            let nets = set_nets.get(sid).copied().unwrap_or(false);
            let delta = if nets { amt.clone() } else { abs_q(amt) };
            *per_set.entry(sid).or_default() -= delta;
            left += 1;
        }
        let mut total = Q::default();
        for (sid, sum) in &per_set {
            let nets = set_nets.get(sid).copied().unwrap_or(false);
            total += if nets { abs_q(sum) } else { sum.clone() };
        }
        if total > best {
            best = total;
        }
    }
    best
}

pub fn run(book: &Book, reporting_date: NaiveDate, known_at: NaiveDate, mode: Mode) -> Result<Run, String> {
    let world = load(book)?;
    let params = vec![("reporting_date".into(), date(reporting_date)), ("known_at".into(), date(known_at))];
    let it = Interp::new(module(), &world, Options { mode, sense: None, params })?;
    it.check_invariants()?;
    let mut lcr: BTreeMap<String, Q> = LCR_ITEMS.iter().map(|i| (i.to_string(), Q::default())).collect();
    let mut nsfr: BTreeMap<String, Q> = NSFR_ITEMS.iter().map(|i| (i.to_string(), Q::default())).collect();
    let mut parts = Vec::new();
    let mut nsfr_parts = Vec::new();
    let mut by_currency = Vec::new();
    for (k, set) in book.sets.iter().enumerate() {
        let s = Val::Rec("NettingSet".into(), k);
        for (item, total) in lcr.iter_mut() {
            let q = num(it.call_fn("cash_flows", vec![s.clone(), Val::Str(item.clone())])?)?
                + num(it.call_fn("collateral", vec![s.clone(), Val::Str(item.clone())])?)?;
            if q != Q::default() {
                parts.push((set.id.clone(), item.clone(), q.clone()));
            }
            *total += q;
        }
        let ccys: std::collections::BTreeSet<&String> =
            book.flows.iter().filter(|f| f.set == set.id).map(|f| &f.currency).collect();
        for c in ccys {
            for item in ["13.1", "21.1"] {
                let q = num(it.call_fn(
                    "cash_flows_in_currency",
                    vec![s.clone(), Val::Str(item.into()), Val::Str(c.clone())],
                )?)?;
                if q != Q::default() {
                    by_currency.push((set.id.clone(), item.to_string(), c.clone(), q));
                }
            }
        }
        for (item, total) in nsfr.iter_mut() {
            let q = num(it.call_fn("nsfr", vec![s.clone(), Val::Str(item.clone())])?)?;
            if q != Q::default() { nsfr_parts.push((set.id.clone(), item.clone(), q.clone())); }
            *total += q;
        }
        // ARF 210.6 item 42.2 (exempt customer IM memo) & 42.3 (CCP default fund, 85% RSF).
        if set.customer_initial_margin_posted != Q::default() {
            nsfr_parts.push((set.id.clone(), "42.2".into(), set.customer_initial_margin_posted.clone()));
            *nsfr.entry("42.2".into()).or_default() += set.customer_initial_margin_posted.clone();
        }
        if set.ccp_default_fund_contribution != Q::default() {
            nsfr_parts.push((set.id.clone(), "42.3".into(), set.ccp_default_fund_contribution.clone()));
            *nsfr.entry("42.3".into()).or_default() += set.ccp_default_fund_contribution.clone();
        }
    }
    let lookback = fast_largest_collateral_flow(book, reporting_date, known_at, mode, None);
    if lookback != Q::default() {
        parts.push(("lookback".into(), "13.6".into(), lookback.clone()));
    }
    lcr.insert("13.6".into(), lookback);
    let ccys: std::collections::BTreeSet<&String> = book.collateral_flows.iter().map(|c| &c.currency).collect();
    for c in ccys {
        let q = fast_largest_collateral_flow(book, reporting_date, known_at, mode, Some(c.as_str()));
        if q != Q::default() {
            by_currency.push(("lookback".into(), "13.6".into(), c.clone(), q));
        }
    }
    Ok(Run { lcr, nsfr, nsfr_parts, parts, by_currency, assumptions: it.assumptions() })
}

/// In AUD millions (the forms' unit).
pub fn arf_inputs(m: &BTreeMap<String, Q>) -> BTreeMap<String, Q> {
    let million = Q::from_integer(1_000_000.into());
    m.iter().map(|(k, v)| (k.clone(), v / &million)).collect()
}
