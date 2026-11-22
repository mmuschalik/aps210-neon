//! Capability spikes on generated data: what the full build needs that the
//! foundations must already carry. Each is small and exact (rationals end to
//! end), and each is exercised by `tests/steel.rs`.
//!
//! - `explain`: move attribution between two reporting dates, by driver, with
//!   zero residual (design 4.C.2).
//! - `record_attribution`: the movement in every ARF 210.1A input, by record.
//! - `changed_records` / `incremental`: locality of every module's outputs and
//!   a delta update that equals the full recompute - the property an
//!   incremental engine (DBSP, design 4.B) relies on.
//! - `replay_deposits` / `replay_derivatives`: bitemporal replay as known at
//!   an earlier date.
//! - `faults`: corruptions of a real package, each of which the gate must
//!   reject.

use std::collections::{BTreeMap, BTreeSet};

use assembly::steering::{Lcr, lcr_of};
use assembly::{Bank, Part, lcr_return};
use chrono::NaiveDate;
use prudence_core::Q;
use prudence_core::uncertain::Mode;

// ---------------------------------------------------------------- move attribution

/// One driver of the movement, applied on top of all earlier ones.
pub struct Step {
    pub driver: &'static str,
    pub lcr: Lcr,
    /// Change in the surplus over a 100% LCR (AUD millions) from this driver.
    pub surplus_change: Q,
}

type Swap = fn(&mut Bank, &Bank);

/// The drivers, in waterfall order. Each moves one book from `after` into
/// the working bank. Together they move every field of `Bank`.
pub const DRIVERS: [(&str, Swap); 9] = [
    ("securities held (prices, trades)", |w, a| w.holdings = a.holdings.clone()),
    ("repos and reverse repos", |w, a| w.repos = a.repos.clone()),
    ("retail and SME deposits", |w, a| w.retail = a.retail.clone()),
    ("wholesale deposits", |w, a| {
        w.wholesale_customers = a.wholesale_customers.clone();
        w.wholesale_deposits = a.wholesale_deposits.clone();
    }),
    ("loans and receivables", |w, a| {
        w.loans = a.loans.clone();
        w.other_inflows = a.other_inflows.clone();
    }),
    ("off-balance sheet", |w, a| w.commitments = a.commitments.clone()),
    ("derivatives and collateral", |w, a| w.derivatives = a.derivatives.clone()),
    ("issued debt, capital, placements", |w, a| w.funding = a.funding.clone()),
    ("static data (currency, CLF, ledger)", |w, a| {
        w.currency = a.currency.clone();
        w.has_clf = a.has_clf;
        w.gl = a.gl.clone();
        w.gl_account = a.gl_account.clone();
    }),
];

/// Why the LCR moved from (`before`, `d0`) to (`after`, `d1`): first the
/// passage of time (the old books re-reported at `d1`: maturities roll into
/// the 30-day window), then each book in turn. Returns the start, the steps,
/// and the independently computed end. By construction the waterfall ends at
/// the working bank; `explain` checks that bank *is* `after` (same LCR), so
/// start + sum(steps) = end exactly: no residual.
pub fn explain(before: &Bank, d0: NaiveDate, after: &Bank, d1: NaiveDate) -> Result<(Lcr, Vec<Step>, Lcr), String> {
    let lcr = |b: &Bank, d| lcr_return(b, d, Mode::Conservative).and_then(|r| lcr_of(&r.inputs));
    let start = lcr(before, d0)?;
    let mut steps = Vec::new();
    let mut prev = start.clone();
    let mut work = before.clone();
    let rolled = lcr(&work, d1)?;
    steps.push(Step { driver: "time (maturities roll down)", surplus_change: rolled.surplus() - prev.surplus(), lcr: rolled.clone() });
    prev = rolled;
    for (driver, swap) in DRIVERS {
        swap(&mut work, after);
        let x = lcr(&work, d1)?;
        steps.push(Step { driver, surplus_change: x.surplus() - prev.surplus(), lcr: x.clone() });
        prev = x;
    }
    let end = lcr(after, d1)?;
    if prev != end {
        return Err("the drivers do not cover every field of Bank: the waterfall did not reach the target".into());
    }
    Ok((start, steps, end))
}

/// ARF 210.1A input cells that are sums of contributions (not the per-currency
/// rows 18.x / 23.x, which the assembler derives by evaluating the form).
fn summed_cell(k: &str) -> bool {
    !(k.starts_with("18.") || k.starts_with("23.")) || k.split('.').count() > 2
}

/// The movement in each summed ARF 210.1A input, by record (AUD millions),
/// and a check that the records' movements add up to each input's movement.
pub fn record_attribution(
    r0: &assembly::Return,
    r1: &assembly::Return,
) -> Result<BTreeMap<String, BTreeMap<String, Q>>, String> {
    let million = Q::from_integer(1_000_000.into());
    let mut by: BTreeMap<String, BTreeMap<String, Q>> = BTreeMap::new();
    for (sign, ret) in [(-1, r0), (1, r1)] {
        for p in ret.parts.iter().filter(|p| p.stream != assembly::Stream::Split) {
            *by.entry(p.item.clone()).or_default().entry(p.record.clone()).or_default() += &p.amount / &million * Q::from_integer(sign.into());
        }
    }
    let keys: BTreeSet<&String> = r0.inputs.keys().chain(r1.inputs.keys()).filter(|k| summed_cell(k) && !k.contains(':')).collect();
    for k in keys {
        let d = r1.inputs.get(k).cloned().unwrap_or_default() - r0.inputs.get(k).cloned().unwrap_or_default();
        let s: Q = by.get(k.as_str()).map(|m| m.values().sum()).unwrap_or_default();
        if d != s {
            return Err(format!("item {k}: input moved {d} but its records moved {s}"));
        }
    }
    by.retain(|_, m| {
        m.retain(|_, q| *q != Q::default());
        !m.is_empty()
    });
    Ok(by)
}

// ---------------------------------------------------------------- locality and incremental update

/// The contributions of one record, as a sorted multiset.
fn by_record(parts: &[Part]) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in parts {
        m.entry(p.record.clone()).or_default().push(format!("{}|{}|{:?}|{}", p.item, p.currency, p.stream, p.amount));
    }
    for v in m.values_mut() {
        v.sort();
    }
    m
}

/// The records whose contributions differ between two returns.
pub fn changed_records(r0: &assembly::Return, r1: &assembly::Return) -> BTreeSet<String> {
    let (a, b) = (by_record(&r0.parts), by_record(&r1.parts));
    a.keys().chain(b.keys()).filter(|k| a.get(*k) != b.get(*k)).cloned().collect()
}

/// The part of `bank` an edit to `scope` can affect: the records whose
/// outputs must be recomputed (the group key an incremental engine would
/// partition by). Retail: the customer's accounts and every account of a
/// customer sharing its SME group (the AUD 2m test is per group) plus the
/// whole event log (tainting is per product class; notices per account).
pub fn sub_bank(bank: &Bank, scope: &BTreeSet<String>) -> Bank {
    let mut s = Bank { has_clf: bank.has_clf, currency: bank.currency.clone(), ..Bank::default() };
    let customers: BTreeSet<&str> =
        bank.retail.accounts.iter().filter(|a| scope.contains(&a.id)).map(|a| a.customer_id.as_str()).collect();
    let groups: BTreeSet<String> = bank
        .retail
        .customers
        .iter()
        .filter(|c| customers.contains(c.id.as_str()))
        .filter_map(|c| c.sme_group.known().cloned())
        .collect();
    let in_scope = |c: &deposits::Customer| customers.contains(c.id.as_str()) || c.sme_group.known().is_some_and(|g| groups.contains(g));
    s.retail.customers = bank.retail.customers.iter().filter(|c| in_scope(c)).cloned().collect();
    let keep: BTreeSet<&str> = s.retail.customers.iter().map(|c| c.id.as_str()).collect();
    s.retail.accounts = bank.retail.accounts.iter().filter(|a| keep.contains(a.customer_id.as_str())).cloned().collect();
    s.retail.ledger = bank.retail.ledger.clone();
    s.loans = bank.loans.iter().filter(|l| scope.contains(&l.id)).cloned().collect();
    s.holdings = bank.holdings.iter().filter(|h| scope.contains(&h.id)).cloned().collect();
    let wc: BTreeSet<&str> = bank.wholesale_deposits.iter().filter(|d| scope.contains(&d.id)).map(|d| d.customer_id.as_str()).collect();
    s.wholesale_customers = bank.wholesale_customers.iter().filter(|c| wc.contains(c.id.as_str())).cloned().collect();
    s.wholesale_deposits = bank.wholesale_deposits.iter().filter(|d| wc.contains(d.customer_id.as_str())).cloned().collect();
    s.commitments.facilities = bank.commitments.facilities.iter().filter(|f| scope.contains(&f.id)).cloned().collect();
    s
}

/// The records `sub_bank(bank, scope)` reports on.
pub fn sub_bank_records(sub: &Bank) -> BTreeSet<String> {
    sub.retail
        .accounts
        .iter()
        .map(|a| a.id.clone())
        .chain(sub.loans.iter().map(|l| l.id.clone()))
        .chain(sub.holdings.iter().map(|h| h.id.clone()))
        .chain(sub.wholesale_deposits.iter().map(|d| d.id.clone()))
        .chain(sub.commitments.facilities.iter().map(|f| f.id.clone()))
        .collect()
}

/// Delta update: recompute only the edited records' group, splice its
/// contributions into the previous return, and re-sum. Returns the new
/// ARF 210.1A inputs and the number of records recomputed.
pub fn incremental(
    old: &assembly::Return,
    after: &Bank,
    scope: &BTreeSet<String>,
    rd: NaiveDate,
) -> Result<(BTreeMap<String, Q>, usize), String> {
    let sub = sub_bank(after, scope);
    let recomputed = sub_bank_records(&sub);
    let fresh = lcr_return(&sub, rd, Mode::Conservative)?;
    let mut parts: Vec<Part> = old.parts.iter().filter(|p| !recomputed.contains(&p.record)).cloned().collect();
    parts.extend(fresh.parts.into_iter().filter(|p| recomputed.contains(&p.record)));
    Ok((assembly::assemble(&parts, &BTreeMap::new())?, recomputed.len()))
}

// ---------------------------------------------------------------- bitemporal replay

/// Retail deposit slices (account, LCR item, amount) as known at `known_at`.
pub fn replay_deposits(bank: &Bank, rd: NaiveDate, known_at: NaiveDate) -> Result<Vec<(String, Option<String>, Q)>, String> {
    let c = deposits::classify(&bank.retail, &deposits::Params { known_at, ..deposits::Params::new(rd) }).map_err(|e| format!("{e:?}"))?;
    let mut v: Vec<_> = c.slices.iter().map(|s| (s.account_id.clone(), s.lcr_item.map(String::from), s.amount.amount().clone())).collect();
    v.sort();
    Ok(v)
}

/// Derivative ARF 210.1A amounts as known at `known_at`.
pub fn replay_derivatives(bank: &Bank, rd: NaiveDate, known_at: NaiveDate) -> Result<BTreeMap<String, Q>, String> {
    Ok(derivatives::run(&bank.derivatives, rd, known_at, Mode::Conservative)?.lcr)
}

// ---------------------------------------------------------------- fault injection

/// A corruption of an honest package, and the gate check (by name) that
/// must catch it.
pub struct Fault {
    pub name: &'static str,
    pub expect: &'static str,
    pub apply: fn(&mut checker::Package),
}

fn bump(q: &mut Q) {
    *q += Q::from_integer(1.into());
}

pub const FAULTS: &[Fault] = &[
    Fault {
        name: "a loan is booked in the GL but missing from the engine's records",
        expect: "gl tie-out",
        apply: |p| {
            let i = p.records.iter().position(|r| r.id.starts_with("loan")).unwrap();
            p.records.remove(i);
        },
    },
    Fault {
        name: "the GL is a dollar out",
        expect: "gl tie-out",
        apply: |p| bump(p.gl.values_mut().next().unwrap()),
    },
    Fault {
        name: "a split deposit's slices no longer add up to its balance",
        expect: "conservation",
        apply: |p| bump(&mut p.slices[0].1),
    },
    Fault {
        name: "an HQLA holding also reported as a 30-day inflow",
        expect: "no double counting",
        apply: |p| {
            let mut x = p.parts.iter().find(|x| x.item.starts_with("1.") && !x.item.starts_with("1.5")).unwrap().clone();
            x.item = "21.3".into();
            p.parts.push(x);
            p.lcr_inputs = checker::derive_lcr_inputs(&p.parts, &p.rates).unwrap();
            p.optional_inputs.insert("ARF 210.1B".into(), checker::derive_aud_inputs(&p.parts, &p.rates));
        },
    },
    Fault {
        name: "a lodged ARF 210.1A input differs from its contributions",
        expect: "inputs = contributions",
        apply: |p| bump(p.lcr_inputs.values_mut().find(|q| **q != Q::default()).unwrap()),
    },
    Fault {
        name: "an FCS allocation is not optimal (certificate tampered)",
        expect: "lp certificates",
        apply: |p| {
            let c = p.certificates.iter_mut().find(|c| !c.solution.x.is_empty()).unwrap();
            bump(&mut c.solution.value);
        },
    },
    Fault {
        name: "a form spec differs from the one the engine ran",
        expect: "evidence integrity",
        apply: |p| p.specs[0].1.push(' '),
    },
    Fault {
        name: "ARF 210.4 disagrees with the maturity ladder",
        expect: "cross-form",
        apply: |p| bump(p.optional_inputs.get_mut("ARF 210.4").unwrap().values_mut().next().unwrap()),
    },
    Fault {
        name: "a return's reporting context names another entity",
        expect: "reporting context",
        apply: |p| p.contexts.values_mut().next().unwrap().entity = "someone-else".into(),
    },
];
