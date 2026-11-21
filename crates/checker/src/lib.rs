//! The trusted verification gate (design: "Tiny Verified Checker").
//!
//! It takes the engine's output as plain data - the general ledger, the
//! balance-sheet records, how each record was split, every per-record
//! contribution to a form item, the form inputs, the LP certificates, the
//! assumptions and the spec sources - and re-derives what it can without the
//! interpreter, the LP solver or the module crates:
//!
//! 1. GL tie-out: every record maps to a ledger account and every account's
//!    balance is exactly the sum of its records (nothing dropped or added).
//! 2. Conservation: every split record is split into exactly its balance.
//! 3. No double counting: a record in the stock of liquid assets (items 1-3,
//!    6.1) never also produces an inflow (items 19-21; APS 210 Att A para 31),
//!    and contributions only go to input cells (never to totals or memo rows).
//! 4. The form inputs are exactly the sum of the contributions, and the
//!    currency split exactly the form evaluated over each currency.
//! 5. LP certificates: primal and dual feasible with no duality gap.
//! 6. Cap identities and checks: every form theorem holds on the concrete
//!    return and no (non-warning) form check fails.
//! 7. Cross-form: every available pair of ARF 210 returns.
//!
//! Only if all pass does it emit the audit bundle: a manifest of SHA-256
//! hashes (data snapshot, spec sources, certificates, assumptions, results)
//! whose own hash is the bundle id.

pub mod json;
pub mod sha256;

use std::collections::{BTreeMap, BTreeSet};

use prudence_core::lp::{Lp, Solution, verify};
use prudence_core::{Evaluation, Missing, Q, evaluate};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct Record {
    pub id: String,
    pub gl_account: String,
    /// Carrying amount, AUD.
    pub balance: Q,
    /// Must be split into slices that add up to the balance.
    pub split: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Both,
    Total,
    Split,
}

#[derive(Clone, Debug)]
pub struct Part {
    pub record: String,
    pub item: String,
    pub currency: String,
    /// AUD.
    pub amount: Q,
    pub stream: Stream,
}

#[derive(Clone, Debug)]
pub struct Certificate {
    pub name: String,
    pub subjects: Vec<String>,
    pub fcs: Option<FcsTerms>,
    pub lp: Lp,
    pub solution: Solution,
}

/// Resolved source attributes for the two supported FCS allocation models.
#[derive(Clone, Debug)]
pub struct FcsTerms {
    pub wholesale: bool,
    pub eligible: Vec<bool>,
    pub operational: Vec<Q>,
    pub covered_rates: Vec<Q>,
    pub uncovered_rates: Vec<Q>,
}

/// An allocation-dependent partition output, in AUD before reporting conversion.
#[derive(Clone, Debug)]
pub struct AllocationSlice {
    pub source: String,
    pub amount: Q,
    pub covered: bool,
    pub lcr_item: Option<String>,
    pub nsfr_cell: String,
}

/// All returns in a package must share this reporting context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportingContext {
    pub entity: String,
    pub reporting_date: String,
    pub consolidation: String,
    pub regime: String,
}

/// One source's contribution to a non-LCR form input, in reporting units.
#[derive(Clone, Debug)]
pub struct FormContribution {
    pub source: String,
    pub cell: String,
    pub amount: Q,
}

/// What an overlay is (design 4.A.6: no Excel overlays).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    /// An APRA discretion in writing (e.g. the net cash outflow overlay,
    /// ARF 210.1A item 29.1; ARF 210.6 items 18/46/54).
    ApraDiscretion,
    /// A temporary correction to a figure, pending a fix at the source.
    DataPatch,
}

/// A signed adjustment to one lodged cell, in force for a fixed period.
#[derive(Clone, Debug)]
pub struct Overlay {
    pub id: String,
    pub kind: OverlayKind,
    pub form: String,
    pub cell: String,
    /// Added to the value the contributions give (reporting units).
    pub amount: Q,
    /// The APRA instrument (required for a discretion).
    pub apra_instrument: Option<String>,
    pub reason: String,
    pub approver: String,
    /// In force from `effective` to `expiry` inclusive (YYYY-MM-DD).
    pub effective: String,
    pub expiry: String,
    /// HMAC-SHA256 of `overlay_message` under the approver's key.
    pub signature: String,
}

/// The signed content of an overlay: every field but the signature.
pub fn overlay_message(o: &Overlay) -> String {
    format!(
        "{}\n{:?}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        o.id,
        o.kind,
        o.form,
        o.cell,
        o.amount,
        o.apra_instrument.clone().unwrap_or_default(),
        o.reason,
        o.approver,
        o.effective,
        o.expiry
    )
}

/// Sign an overlay with the approver's key.
pub fn sign(o: &Overlay, key: &[u8]) -> String {
    sha256::hmac_hex(key, overlay_message(o).as_bytes())
}

#[derive(Clone, Debug, Default)]
pub struct Package {
    pub reporting_date: String,
    pub contexts: BTreeMap<String, ReportingContext>,
    /// Ledger account -> balance (AUD).
    pub gl: BTreeMap<String, Q>,
    pub records: Vec<Record>,
    /// (record, amount) per slice of a split record.
    pub slices: Vec<(String, Q)>,
    pub parts: Vec<Part>,
    /// Explicit non-ledger sources (e.g. commitments or netting sets).
    /// These are declarations, not independently verified source records.
    pub external_sources: BTreeMap<String, String>,
    /// Rate cells applying to every currency (e.g. "16.4:weight").
    pub rates: BTreeMap<String, Q>,
    /// ARF 210.1A and ARF 210.6 inputs as lodged (AUD millions).
    pub lcr_inputs: BTreeMap<String, Q>,
    pub nsfr_inputs: BTreeMap<String, Q>,
    /// Optional returns, keyed by canonical form code (e.g. "ARF 210.1B").
    /// Presence (including an empty vector) asserts complete contribution evidence.
    pub form_contributions: BTreeMap<String, Vec<FormContribution>>,
    pub optional_inputs: BTreeMap<String, BTreeMap<String, Q>>,
    pub certificates: Vec<Certificate>,
    /// Signed overlays on lodged cells.
    pub overlays: Vec<Overlay>,
    pub allocation_slices: Vec<AllocationSlice>,
    /// (subject, attribute, assumed, reason)
    pub assumptions: Vec<(String, String, String, String)>,
    /// (path, source) of every spec the run used.
    pub specs: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct Report {
    /// (check, failures) - empty failures means it passed.
    pub checks: Vec<(&'static str, Vec<String>)>,
    pub warnings: Vec<String>,
    /// Present only if every check passed.
    pub bundle: Option<Value>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|(_, f)| f.is_empty())
    }
    pub fn failures(&self) -> Vec<String> {
        self.checks.iter().flat_map(|(c, f)| f.iter().map(move |x| format!("{c}: {x}"))).collect()
    }
}

fn million() -> Q {
    Q::from_integer(1_000_000.into())
}

fn top(item: &str) -> u32 {
    item.split('.').next().and_then(|t| t.parse().ok()).unwrap_or(0)
}

fn memo_row(ccy: &str) -> usize {
    match ccy {
        "AUD" => 1,
        "NZD" => 2,
        "USD" => 3,
        "GBP" => 4,
        "EUR" => 5,
        "JPY" => 6,
        "RMB" | "CNY" => 7,
        _ => 8,
    }
}

fn eval<'f>(form: &'f prudence_core::Form, inputs: &BTreeMap<String, Q>) -> Result<Evaluation<'f>, String> {
    evaluate(form, inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).map_err(|e| format!("{e:?}"))
}

fn gl_tie_out(p: &Package) -> Vec<String> {
    let mut out = Vec::new();
    let mut by_account: BTreeMap<&str, Q> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for r in &p.records {
        if !seen.insert(&r.id) {
            out.push(format!("record {} appears twice", r.id));
        }
        if !p.gl.contains_key(&r.gl_account) {
            out.push(format!("record {} maps to unknown account {}", r.id, r.gl_account));
        }
        *by_account.entry(&r.gl_account).or_default() += r.balance.clone();
    }
    for (acct, bal) in &p.gl {
        let got = by_account.get(acct.as_str()).cloned().unwrap_or_default();
        if &got != bal {
            out.push(format!("account {acct}: ledger {bal}, records {got}"));
        }
    }
    out
}

fn conservation(p: &Package) -> Vec<String> {
    let mut sums: BTreeMap<&str, Q> = BTreeMap::new();
    for (r, q) in &p.slices {
        *sums.entry(r).or_default() += q.clone();
    }
    let mut out = Vec::new();
    let ids: BTreeSet<&str> = p.records.iter().map(|r| r.id.as_str()).collect();
    for r in p.records.iter().filter(|r| r.split) {
        match sums.get(r.id.as_str()) {
            None => out.push(format!("{} was not split", r.id)),
            Some(s) if s != &r.balance => out.push(format!("{}: slices {s}, balance {}", r.id, r.balance)),
            _ => {}
        }
    }
    for (r, q) in &p.slices {
        if q < &Q::default() {
            out.push(format!("{r}: negative slice {q}"));
        }
        if !ids.contains(r.as_str()) {
            out.push(format!("slice of unknown record {r}"));
        }
    }
    out
}

fn no_double_counting(p: &Package) -> Vec<String> {
    let mut out = Vec::new();
    let form = arf210::lcr_all();
    let stock: BTreeSet<&str> = p
        .parts
        .iter()
        .filter(|x| (1..=3).contains(&top(&x.item)) || x.item.starts_with("6.1"))
        .map(|x| x.record.as_str())
        .collect();
    for x in &p.parts {
        if (19..=21).contains(&top(&x.item)) && stock.contains(x.record.as_str()) {
            out.push(format!("{} is in the stock of liquid assets and also an inflow ({})", x.record, x.item));
        }
        let input = form
            .items
            .iter()
            .find(|it| it.id == x.item)
            .is_some_and(|it| matches!(it.cell("amount"), Some(prudence_core::Cell::Input { .. })));
        if !input {
            out.push(format!("{} contributes to {}, which is not an input cell", x.record, x.item));
        }
    }
    out
}

/// The ARF 210.1A inputs implied by the contributions alone.
pub fn derive_lcr_inputs(parts: &[Part], rates: &BTreeMap<String, Q>) -> Result<BTreeMap<String, Q>, String> {
    let mut inputs = rates.clone();
    let mut by_ccy: BTreeMap<usize, BTreeMap<String, Q>> = BTreeMap::new();
    for x in parts {
        let q = &x.amount / million();
        if x.stream != Stream::Split {
            *inputs.entry(x.item.clone()).or_default() += q.clone();
        }
        if x.stream != Stream::Total {
            *by_ccy.entry(memo_row(&x.currency)).or_insert_with(|| rates.clone()).entry(x.item.clone()).or_default() += q;
        }
    }
    for row in 1..=8 {
        let v = match by_ccy.get(&row) {
            Some(m) => {
                let e = eval(arf210::lcr_all(), m)?;
                [e.q("18").clone(), e.q("18:weighted").clone(), e.q("23").clone(), e.q("23:weighted").clone()]
            }
            None => Default::default(),
        };
        let [a, b, c, d] = v;
        inputs.insert(format!("18.{row}"), a);
        inputs.insert(format!("18.{row}:weighted"), b);
        inputs.insert(format!("23.{row}"), c);
        inputs.insert(format!("23.{row}:weighted"), d);
    }
    Ok(inputs)
}

/// AUD-only inputs use the currency split stream, including FX contributions.
pub fn derive_aud_inputs(parts: &[Part], rates: &BTreeMap<String, Q>) -> BTreeMap<String, Q> {
    let mut inputs = rates.clone();
    for x in parts.iter().filter(|x| x.currency == "AUD" && x.stream != Stream::Total) {
        *inputs.entry(x.item.clone()).or_default() += &x.amount / million();
    }
    inputs
}

fn inputs_match(p: &Package) -> Vec<String> {
    let want = match derive_lcr_inputs(&p.parts, &p.rates) {
        Ok(w) => w,
        Err(e) => return vec![e],
    };
    let overlay = |form: &str, mut m: BTreeMap<String, Q>| {
        for o in p.overlays.iter().filter(|o| o.form == form) {
            *m.entry(o.cell.clone()).or_default() += o.amount.clone();
        }
        m
    };
    let mut out = compare_inputs(&overlay("ARF 210.1A", want), &p.lcr_inputs);
    if let Some(aud) = p.optional_inputs.get("ARF 210.1B") {
        out.extend(
            compare_inputs(&overlay("ARF 210.1B", derive_aud_inputs(&p.parts, &p.rates)), aud)
                .into_iter()
                .map(|e| format!("ARF 210.1B {e}")),
        );
    }
    for (code, parts) in &p.form_contributions {
        let lodged = if code == "ARF 210.6" { Some(&p.nsfr_inputs) } else { p.optional_inputs.get(code) };
        if matches!(code.as_str(), "ARF 210.1A" | "ARF 210.1B") {
            out.push(format!("{code}: use LCR parts, not form_contributions"));
            continue;
        }
        let Some(lodged) = lodged else {
            out.push(format!("{code}: contribution evidence has no return"));
            continue;
        };
        let mut want = BTreeMap::new();
        for part in parts {
            *want.entry(part.cell.clone()).or_default() += part.amount.clone();
        }
        if let Some(form) = arf210::form(code)
            && let Err(e) = eval(form, &want)
        {
            out.push(format!("{code}: invalid contribution cell: {e}"));
        }
        out.extend(compare_inputs(&overlay(code, want), lodged).into_iter().map(|e| format!("{code} {e}")));
    }
    out
}

/// Every overlay: signed by its approver, in force at the reporting date,
/// a discretion citing its APRA instrument, on an input cell of its form.
fn overlay_checks(p: &Package, keys: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut out = Vec::new();
    let day = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    let rd = day(&p.reporting_date);
    let mut seen = BTreeSet::new();
    for o in &p.overlays {
        if !seen.insert(&o.id) {
            out.push(format!("{}: duplicate overlay id", o.id));
        }
        match keys.get(&o.approver) {
            None => out.push(format!("{}: no key for approver {}", o.id, o.approver)),
            Some(k) if sign(o, k) != o.signature => out.push(format!("{}: signature does not verify", o.id)),
            _ => {}
        }
        if o.kind == OverlayKind::ApraDiscretion && o.apra_instrument.as_deref().is_none_or(|s| s.trim().is_empty()) {
            out.push(format!("{}: an APRA discretion must cite its instrument", o.id));
        }
        match (day(&o.effective), day(&o.expiry), rd) {
            (Some(e), Some(x), Some(r)) if e <= r && r <= x => {}
            (Some(_), Some(x), Some(r)) if r > x => out.push(format!("{}: expired on {x}", o.id)),
            (Some(e), Some(_), Some(r)) if r < e => out.push(format!("{}: not in force until {e}", o.id)),
            _ => out.push(format!("{}: invalid dates", o.id)),
        }
        let input = arf210::form(&o.form).is_some_and(|f| {
            f.resolve(&o.cell).is_ok_and(|r| matches!(f.cell(&r), Some(prudence_core::Cell::Input { .. })))
        });
        if !input {
            out.push(format!("{}: {} {} is not an input cell", o.id, o.form, o.cell));
        }
    }
    out
}

fn compare_inputs(want: &BTreeMap<String, Q>, lodged: &BTreeMap<String, Q>) -> Vec<String> {
    let zero = Q::default();
    let keys: BTreeSet<&String> = want.keys().chain(lodged.keys()).collect();
    keys.into_iter()
        .filter_map(|k| {
            let (a, b) = (want.get(k).unwrap_or(&zero), lodged.get(k).unwrap_or(&zero));
            (a != b).then(|| format!("{k}: lodged {b}, contributions give {a}"))
        })
        .collect()
}

fn context_checks(p: &Package) -> Vec<String> {
    let mut errors = Vec::new();
    if !p.contexts.is_empty() && chrono::NaiveDate::parse_from_str(&p.reporting_date, "%Y-%m-%d").is_err() {
        errors.push("invalid reporting date".into());
    }
    for (code, c) in &p.contexts {
        if c.entity.trim().is_empty()
            || c.reporting_date != p.reporting_date
            || !matches!(c.consolidation.as_str(), "level1" | "level2")
            || !matches!(c.regime.as_str(), "lcr" | "mlh")
        {
            errors.push(format!("{code}: invalid reporting context"));
        }
        if code != "ARF 210.1A" && code != "ARF 210.6" && !p.optional_inputs.contains_key(code) {
            errors.push(format!("{code}: context without return"));
        }
    }
    if let Some(first) = p.contexts.values().next() {
        for (code, c) in &p.contexts {
            if c != first {
                errors.push(format!("{code}: reporting contexts differ within package"));
            }
        }
        for code in ["ARF 210.1A", "ARF 210.6"].into_iter().chain(p.optional_inputs.keys().map(String::as_str)) {
            if !p.contexts.contains_key(code) {
                errors.push(format!("{code}: missing reporting context"));
            }
        }
    }
    errors
}

fn evidence_checks(p: &Package) -> Vec<String> {
    let mut out = Vec::new();
    let ids: BTreeSet<_> = p.records.iter().map(|r| r.id.as_str()).collect();
    for (id, reason) in &p.external_sources {
        if reason.trim().is_empty() || ids.contains(id.as_str()) {
            out.push(format!("invalid or ambiguous external source {id}"));
        }
    }
    for part in &p.parts {
        if !ids.contains(part.record.as_str()) && !p.external_sources.contains_key(&part.record) {
            out.push(format!("contribution from unknown source {}", part.record));
        }
    }
    for parts in p.form_contributions.values() {
        for part in parts {
            if !ids.contains(part.source.as_str()) && !p.external_sources.contains_key(&part.source) {
                out.push(format!("form contribution from unknown source {}", part.source));
            }
        }
    }
    for key in p.rates.keys() {
        if !key.ends_with(":weight") {
            out.push(format!("non-rate input in rates: {key}"));
        }
    }
    let mut seen = BTreeSet::new();
    for (path, source) in &p.specs {
        if !seen.insert(path) {
            out.push(format!("duplicate spec {path}"));
        }
        if let Some(code) = path.strip_prefix("forms/") {
            match arf210::SPECS.iter().find(|(c, _)| *c == code) {
                Some((_, actual)) if *actual == source => {}
                _ => out.push(format!("form source does not match executed spec: {path}")),
            }
        }
    }
    out
}

fn fcs_problem(c: &Certificate, p: &Package) -> Result<(), String> {
    use prudence_core::lp::{Rel, Sense};
    let Some(t) = &c.fcs else {
        return Ok(());
    };
    let n = c.subjects.len();
    if n == 0
        || [t.eligible.len(), t.operational.len(), t.covered_rates.len(), t.uncovered_rates.len()]
            .iter()
            .any(|x| *x != n)
    {
        return Err("FCS evidence has wrong shape".into());
    }
    let zero = Q::default();
    let one = Q::from_integer(1.into());
    let mut caps = Vec::new();
    let mut objective = Vec::new();
    for (i, id) in c.subjects.iter().enumerate() {
        let balance = &p.records.iter().find(|r| &r.id == id).ok_or("FCS subject missing from ledger")?.balance;
        if balance < &zero || t.operational[i] < zero || &t.operational[i] > balance {
            return Err(format!("{id}: invalid FCS balance or operational amount"));
        }
        caps.push(if !t.eligible[i] {
            zero.clone()
        } else if t.wholesale {
            t.operational[i].clone()
        } else {
            balance.clone()
        });
        if t.covered_rates[i] < zero || t.covered_rates[i] > one || t.uncovered_rates[i] < zero || t.uncovered_rates[i] > one {
            return Err(format!("{id}: invalid counterfactual run-off rate"));
        }
        objective.push(if t.wholesale { one.clone() } else { &t.covered_rates[i] - &t.uncovered_rates[i] });
    }
    let limit = Q::from_integer(250_000.into());
    let budget = if t.wholesale { limit } else { limit.min(caps.iter().cloned().sum()) };
    let mut rows = vec![(vec![one.clone(); n], if t.wholesale { Rel::Le } else { Rel::Eq }, budget)];
    for (i, cap) in caps.into_iter().enumerate() {
        let mut lower = vec![zero.clone(); n];
        lower[i] = -one.clone();
        let mut upper = vec![zero.clone(); n];
        upper[i] = one.clone();
        rows.push((lower, Rel::Le, zero.clone()));
        rows.push((upper, Rel::Le, cap));
    }
    // Compare constraints as a multiset, accepting either orientation of >=.
    let normalize = |rows: &[(Vec<Q>, Rel, Q)]| {
        let mut out: Vec<_> = rows
            .iter()
            .map(|(a, rel, b)| {
                if *rel == Rel::Ge {
                    format!("{:?}|Le|{}", a.iter().map(|q| -q).collect::<Vec<_>>(), -b)
                } else {
                    format!("{a:?}|{rel:?}|{b}")
                }
            })
            .collect();
        out.sort();
        out
    };
    if c.lp.sense != Sense::Max || c.lp.objective != objective || normalize(&c.lp.rows) != normalize(&rows) {
        return Err("LP differs from reconstructed FCS budget, bounds or objective".into());
    }
    Ok(())
}

fn allocation_bindings(p: &Package) -> Vec<String> {
    let mut errors = Vec::new();
    let mut owners = BTreeSet::new();
    let mut names = BTreeSet::new();
    for c in &p.certificates {
        if !names.insert(&c.name) {
            errors.push(format!("duplicate certificate {}", c.name));
        }
        if let Err(e) = fcs_problem(c, p) {
            errors.push(format!("{}: {e}", c.name));
        }
        if c.subjects.is_empty() {
            continue;
        }
        if c.subjects.len() != c.solution.x.len() {
            errors.push(format!("{}: subject vector has wrong shape", c.name));
            continue;
        }
        for (source, allocated) in c.subjects.iter().zip(&c.solution.x) {
            if !owners.insert(source) {
                errors.push(format!("{source}: allocated by multiple variables"));
            }
            let slices: Vec<_> = p.allocation_slices.iter().filter(|s| &s.source == source).collect();
            let total: Q = slices.iter().map(|s| s.amount.clone()).sum();
            let covered: Q = slices.iter().filter(|s| s.covered).map(|s| s.amount.clone()).sum();
            if &covered != allocated {
                errors.push(format!("{source}: covered slices {covered} != allocation {allocated}"));
            }
            if p.records.iter().find(|r| &r.id == source).is_none_or(|r| r.balance != total) {
                errors.push(format!("{source}: allocation slices do not conserve the ledger record"));
            }
            let mut lcr = BTreeMap::new();
            let mut nsfr = BTreeMap::new();
            for s in slices {
                if s.amount < Q::default() {
                    errors.push(format!("{source}: negative allocation slice"));
                }
                if let Some(item) = &s.lcr_item {
                    *lcr.entry(item.clone()).or_default() += s.amount.clone();
                }
                *nsfr.entry(s.nsfr_cell.clone()).or_default() += &s.amount / million();
            }
            let mut actual_lcr = BTreeMap::new();
            for part in p.parts.iter().filter(|part| &part.record == source && part.stream != Stream::Split) {
                *actual_lcr.entry(part.item.clone()).or_default() += part.amount.clone();
            }
            let mut actual_nsfr = BTreeMap::new();
            for part in p.form_contributions.get("ARF 210.6").into_iter().flatten().filter(|part| &part.source == source) {
                *actual_nsfr.entry(part.cell.clone()).or_default() += part.amount.clone();
            }
            errors.extend(compare_inputs(&lcr, &actual_lcr).into_iter().map(|e| format!("{source} LCR allocation output: {e}")));
            errors.extend(compare_inputs(&nsfr, &actual_nsfr).into_iter().map(|e| format!("{source} NSFR allocation output: {e}")));
        }
    }
    for s in &p.allocation_slices {
        if !owners.contains(&s.source) {
            errors.push(format!("{}: allocation slice has no certificate variable", s.source));
        }
    }
    errors
}

fn certificates(p: &Package) -> Vec<String> {
    p.certificates
        .iter()
        .filter_map(|c| {
            if c.lp.rows.iter().any(|(row, _, _)| row.len() != c.lp.objective.len()) {
                Some(format!("{}: LP row has the wrong shape", c.name))
            } else {
                verify(&c.lp, &c.solution).err().map(|e| format!("{}: {e}", c.name))
            }
        })
        .collect()
}

fn form_checks(ev: &mut Evaluation, warnings: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for f in &ev.findings {
        if f.warn {
            warnings.push(format!("{} {}: {}", ev.form.code, f.check_id, f.detail));
        } else {
            out.push(format!("{} {}: {}", ev.form.code, f.check_id, f.detail));
        }
    }
    for t in &ev.form.theorems.clone() {
        if prudence_core::eval::holds(&t.prop, ev) != Some(true) {
            out.push(format!("{} theorem {} fails on this return", ev.form.code, t.id));
        }
    }
    out
}

fn canonical(p: &Package) -> String {
    let mut s = format!("reporting_date\t{}\n", p.reporting_date);
    for (a, b) in &p.gl {
        s += &format!("gl\t{a}\t{b}\n");
    }
    let mut rs: Vec<String> = p
        .records
        .iter()
        .map(|r| format!("record\t{}\t{}\t{}\t{}\n", r.id, r.gl_account, r.balance, r.split))
        .collect();
    let mut sl: Vec<String> = p.slices.iter().map(|(r, q)| format!("slice\t{r}\t{q}\n")).collect();
    let mut ps: Vec<String> = p
        .parts
        .iter()
        .map(|x| format!("part\t{}\t{}\t{}\t{}\t{:?}\n", x.record, x.item, x.currency, x.amount, x.stream))
        .collect();
    rs.sort();
    sl.sort();
    ps.sort();
    s + &rs.concat() + &sl.concat() + &ps.concat()
}

fn hash_map(m: &BTreeMap<String, Q>) -> String {
    sha256::hex(m.iter().map(|(k, v)| format!("{k}\t{v}\n")).collect::<String>().as_bytes())
}

/// Check that evidence covers every supplied return and every allocation.
fn coverage_requirements(p: &Package) -> Vec<String> {
    let mut errors = Vec::new();
    if p.contexts.is_empty() {
        errors.push("reporting context is required".into());
    }
    for code in std::iter::once("ARF 210.6").chain(p.optional_inputs.keys().map(String::as_str).filter(|c| *c != "ARF 210.1B")) {
        if !p.form_contributions.contains_key(code) {
            errors.push(format!("{code}: contribution evidence is required"));
        }
    }
    for c in &p.certificates {
        if c.subjects.is_empty() || c.fcs.is_none() {
            errors.push(format!("{}: supported problem evidence and output binding are required", c.name));
        }
    }
    errors
}

/// Full-evidence gate: omission of coverage is an error, never a silent downgrade.
pub fn check_complete(p: &Package) -> Report {
    check_with_coverage(p, true, &BTreeMap::new())
}

/// Compatibility diagnostics for packages with partial evidence.
pub fn check(p: &Package) -> Report {
    check_with_coverage(p, false, &BTreeMap::new())
}

fn cross_coverage(p: &Package) -> Vec<Value> {
    let present = |c: &str| matches!(c, "ARF 210.1A" | "ARF 210.6") || p.optional_inputs.contains_key(c);
    let context = p.contexts.values().next();
    let mut out = Vec::new();
    for (rule, forms, regime, level) in [
        ("nsfr_vs_lcr", vec!["ARF 210.6", "ARF 210.1A"], None, None),
        ("aud_vs_all", vec!["ARF 210.1B", "ARF 210.1A"], None, None),
        ("daily_lcr_stock", vec!["ARF 210.5", "ARF 210.1A"], Some("lcr"), None),
        ("daily_all_lcr_ratio", vec!["ARF 210.5", "ARF 210.1A"], Some("lcr"), Some("level1")),
        ("daily_aud_lcr_ratio", vec!["ARF 210.5", "ARF 210.1A", "ARF 210.1B"], Some("lcr"), Some("level1")),
        ("daily_vs_mlh", vec!["ARF 210.5", "ARF 210.2"], Some("mlh"), None),
        ("funding_plan_vs_assets", vec!["ARF 210.4", "ARF 210.3.1"], None, None),
        ("funding_plan_vs_liabilities", vec!["ARF 210.4", "ARF 210.3.2"], None, None),
    ] {
        let reason = if forms.iter().any(|c| !present(c)) {
            Some("required form absent")
        } else if regime.is_some() && context.is_none() {
            Some("reporting context absent")
        } else if regime.is_some_and(|r| context.is_some_and(|c| c.regime != r)) {
            Some("different reporting regime")
        } else if level.is_some_and(|l| context.is_some_and(|c| c.consolidation != l)) {
            Some("different consolidation level")
        } else {
            None
        };
        out.push(json!({"rule": rule, "forms": forms, "status": if reason.is_some() { "skipped" } else { "evaluated" }, "reason": reason}));
    }
    out
}

/// Run every check; emit the bundle only if all pass.
/// The full gate with the approvers' overlay signing keys.
pub fn check_signed(p: &Package, keys: &BTreeMap<String, Vec<u8>>) -> Report {
    check_with_coverage(p, true, keys)
}

fn check_with_coverage(p: &Package, complete: bool, keys: &BTreeMap<String, Vec<u8>>) -> Report {
    let mut warnings = vec!["Verification is scoped: source classifications and returns without contribution evidence are not verified".into()];
    let mut checks: Vec<(&'static str, Vec<String>)> = vec![
        ("gl tie-out", gl_tie_out(p)),
        ("conservation", conservation(p)),
        ("no double counting", no_double_counting(p)),
        ("inputs = contributions", inputs_match(p)),
        ("lp certificates", certificates(p)),
        ("allocation bindings", allocation_bindings(p)),
        ("evidence integrity", evidence_checks(p)),
        ("reporting context", context_checks(p)),
        ("overlays", overlay_checks(p, keys)),
    ];
    if complete {
        checks.push(("required coverage", coverage_requirements(p)));
    }
    let mut evaluations = BTreeMap::new();
    let mut errors = Vec::new();
    let mut failures = Vec::new();
    for (code, inputs) in [("ARF 210.1A", &p.lcr_inputs), ("ARF 210.6", &p.nsfr_inputs)]
        .into_iter()
        .chain(p.optional_inputs.iter().map(|(k, v)| (k.as_str(), v)))
    {
        if p.optional_inputs.contains_key(code) && matches!(code, "ARF 210.1A" | "ARF 210.6") {
            errors.push(format!("duplicate required form {code}"));
            continue;
        }
        let Some(form) = arf210::forms().iter().find(|f| f.code == code) else {
            errors.push(format!("unknown form {code}"));
            continue;
        };
        match eval(form, inputs) {
            Ok(mut ev) => {
                failures.extend(form_checks(&mut ev, &mut warnings));
                evaluations.insert(code, ev);
            }
            Err(e) => errors.push(format!("{code}: {e}")),
        }
    }
    checks.push(("form evaluation", errors));
    checks.push(("form checks and theorems", failures));
    use arf210::crossform::*;
    let mut findings = Vec::new();
    let get = |code| evaluations.get(code);
    if let (Some(n), Some(l)) = (get("ARF 210.6"), get("ARF 210.1A")) {
        findings.extend(nsfr_vs_lcr(n, l));
    }
    if let (Some(a), Some(l)) = (get("ARF 210.1B"), get("ARF 210.1A")) {
        findings.extend(aud_vs_all(a, l));
    }
    let regime = p.contexts.values().next().map(|c| c.regime.as_str());
    if let Some(d) = get("ARF 210.5") {
        if regime == Some("lcr")
            && let Some(l) = get("ARF 210.1A")
        {
            // Daily day-1 ratios are Level 1; stock ties apply at either level.
            let mut daily = daily_vs_lcr(d, l, get("ARF 210.1B"));
            if p.contexts.values().next().is_some_and(|c| c.consolidation != "level1") {
                daily.retain(|f| !f.check_id.starts_with("210.5:5."));
            }
            findings.extend(daily);
        }
        if regime == Some("mlh")
            && let Some(m) = get("ARF 210.2")
        {
            findings.extend(daily_vs_mlh(d, m));
        }
    }
    if let Some(plan) = get("ARF 210.4") {
        for code in ["ARF 210.3.1", "ARF 210.3.2"] {
            if let Some(other) = get(code) {
                findings.extend(funding_plan_vs_one_maturity(plan, other));
            }
        }
    }
    let mut cross = Vec::new();
    for f in findings {
        let detail = format!("{}: {}", f.check_id, f.detail);
        if f.warn {
            warnings.push(detail);
        } else {
            cross.push(detail);
        }
    }
    if get("ARF 210.5").is_some() && regime.is_none() {
        warnings.push("Daily cross-form checks skipped: reporting regime not supplied".into());
    }
    checks.push(("cross-form", cross));
    let mut report = Report { checks, warnings, bundle: None };
    if report.passed() {
        report.bundle = Some(bundle(p, &evaluations["ARF 210.1A"], &report, complete));
    }
    report
}

fn bundle(p: &Package, lcr: &Evaluation, r: &Report, complete: bool) -> Value {
    let specs: BTreeMap<&str, String> = arf210::SPECS.iter().map(|(code, src)| (*code, sha256::hex(src.as_bytes()))).collect();
    let spec_version = sha256::hex(specs.iter().map(|(k, v)| format!("{k}\t{v}\n")).collect::<String>().as_bytes());
    let certs: Vec<Value> = p
        .certificates
        .iter()
        .map(|c| {
            let text = format!("{:?}\n{:?}", c.lp, c.solution);
            json!({ "name": c.name, "sha256": sha256::hex(text.as_bytes()), "value": c.solution.value.to_string() })
        })
        .collect();
    let mut assumptions = p.assumptions.clone();
    assumptions.sort();
    let assumptions_text: String = assumptions.iter().map(|(a, b, c, d)| format!("{a}\t{b}\t{c}\t{d}\n")).collect();
    let mut manifest = json!({
        "standard": "APS 210 (2025-07-01); ARS 210.0 (2023-03-31)",
        "reporting_date": p.reporting_date,
        "schema_version": 3,
        "checker_sources": {
            "checker": sha256::hex(include_str!("lib.rs").as_bytes()),
            "json": sha256::hex(include_str!("json.rs").as_bytes()),
            "crossform": sha256::hex(include_str!("../../arf210/src/crossform.rs").as_bytes()),
            "form_evaluator": sha256::hex(include_str!("../../prudence-core/src/eval.rs").as_bytes()),
            "lp_verifier": sha256::hex(include_str!("../../prudence-core/src/lp.rs").as_bytes())
        },
        "reporting_context": json::to_json(p)["contexts"],
        "assurance": if complete { "complete_evidence_consistency" } else { "scoped_consistency" },
        "data_snapshot": sha256::hex(canonical(p).as_bytes()),
        "package_sha256": sha256::hex(json::to_json(p).to_string().as_bytes()),
        "declared_sources": p.specs.iter().map(|(path, src)| json!({"path": path, "sha256": sha256::hex(src.as_bytes())})).collect::<Vec<_>>(),
        "coverage": {
            "cross_form": cross_coverage(p),
            "contribution_reconciled": if p.optional_inputs.contains_key("ARF 210.1B") { vec!["ARF 210.1A", "ARF 210.1B"] } else { vec!["ARF 210.1A"] },
            "additional_contribution_reconciled": p.form_contributions.keys().collect::<Vec<_>>(),
            "fcs_problem_evidence_missing": p.certificates.iter().filter(|c| c.fcs.is_none()).map(|c| &c.name).collect::<Vec<_>>(),
            "fcs_problem_reconstructed": p.certificates.iter().filter(|c| c.fcs.is_some()).map(|c| &c.name).collect::<Vec<_>>(),
            "allocation_bound": p.certificates.iter().filter(|c| !c.subjects.is_empty()).map(|c| &c.name).collect::<Vec<_>>(),
            "allocation_binding_missing": p.certificates.iter().filter(|c| c.subjects.is_empty()).map(|c| &c.name).collect::<Vec<_>>(),
            "contribution_evidence_missing": std::iter::once("ARF 210.6")
                .chain(p.optional_inputs.keys().map(String::as_str).filter(|c| *c != "ARF 210.1B"))
                .filter(|c| !p.form_contributions.contains_key(*c)).collect::<Vec<_>>(),
            "form_checks": std::iter::once("ARF 210.1A").chain(std::iter::once("ARF 210.6")).chain(p.optional_inputs.keys().map(String::as_str)).collect::<Vec<_>>(),
            "undefined_cross_form_operands": "equality rules skip undefined operands; see lcr_status for the primary ratio",
            "unverified": ["source completeness and classification", "resolved FCS source attributes and counterfactual classifications", "signed overlays"],
            "reporting_context_verified": !p.contexts.is_empty(),
            "daily_cross_form": match p.contexts.values().next() {
                None => "skipped: missing context",
                Some(c) if c.regime == "mlh" => "MLH comparisons when forms present",
                Some(c) if c.consolidation == "level2" => "LCR stock comparisons only; day-1 Level 1 ratios skipped",
                Some(_) => "LCR comparisons when forms present",
            },
            "external_sources": p.external_sources,
            "missing_inputs": "zero with spec defaults"
        },
        "spec_version": spec_version,
        "specs": specs,
        "certificates": certs,
        "assumptions": { "count": assumptions.len(), "sha256": sha256::hex(assumptions_text.as_bytes()) },
        "overlays": p.overlays.iter().map(|o| json!({
            "id": o.id, "kind": format!("{:?}", o.kind), "form": o.form, "cell": o.cell, "amount": o.amount.to_string(),
            "apra_instrument": o.apra_instrument, "approver": o.approver, "effective": o.effective, "expiry": o.expiry,
            "signature": o.signature,
        })).collect::<Vec<_>>(),
        "returns": {
            "ARF 210.1A": hash_map(&p.lcr_inputs),
            "ARF 210.6": hash_map(&p.nsfr_inputs),
        },
        "lcr_percent": lcr.get("30").num().map(ToString::to_string),
        "lcr_status": if lcr.get("30").num().is_some() { "defined" } else { "undefined" },
        "checks": r.checks.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        "warnings": r.warnings,
    });
    for (code, inputs) in &p.optional_inputs {
        manifest["returns"][code] = json!(hash_map(inputs));
    }
    let id = sha256::hex(manifest.to_string().as_bytes());
    manifest["bundle_id"] = json!(id);
    manifest
}
