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
//! 7. Cross-form: ARF 210.6 against ARF 210.1A.
//!
//! Only if all pass does it emit the audit bundle: a manifest of SHA-256
//! hashes (data snapshot, spec sources, certificates, assumptions, results)
//! whose own hash is the bundle id.

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
    pub lp: Lp,
    pub solution: Solution,
}

#[derive(Clone, Debug, Default)]
pub struct Package {
    pub reporting_date: String,
    /// Ledger account -> balance (AUD).
    pub gl: BTreeMap<String, Q>,
    pub records: Vec<Record>,
    /// (record, amount) per slice of a split record.
    pub slices: Vec<(String, Q)>,
    pub parts: Vec<Part>,
    /// Rate cells applying to every currency (e.g. "16.4:weight").
    pub rates: BTreeMap<String, Q>,
    /// ARF 210.1A and ARF 210.6 inputs as lodged (AUD millions).
    pub lcr_inputs: BTreeMap<String, Q>,
    pub nsfr_inputs: BTreeMap<String, Q>,
    pub certificates: Vec<Certificate>,
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

fn inputs_match(p: &Package) -> Vec<String> {
    let want = match derive_lcr_inputs(&p.parts, &p.rates) {
        Ok(w) => w,
        Err(e) => return vec![e],
    };
    let zero = Q::default();
    let keys: BTreeSet<&String> = want.keys().chain(p.lcr_inputs.keys()).collect();
    keys.into_iter()
        .filter_map(|k| {
            let (a, b) = (want.get(k).unwrap_or(&zero), p.lcr_inputs.get(k).unwrap_or(&zero));
            (a != b).then(|| format!("{k}: lodged {b}, contributions give {a}"))
        })
        .collect()
}

fn certificates(p: &Package) -> Vec<String> {
    p.certificates.iter().filter_map(|c| verify(&c.lp, &c.solution).err().map(|e| format!("{}: {e}", c.name))).collect()
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
    let mut rs: Vec<String> = p.records.iter().map(|r| format!("record\t{}\t{}\t{}\t{}\n", r.id, r.gl_account, r.balance, r.split)).collect();
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

/// Run every check; emit the bundle only if all pass.
pub fn check(p: &Package) -> Report {
    let mut warnings = Vec::new();
    let mut checks: Vec<(&'static str, Vec<String>)> = vec![
        ("gl tie-out", gl_tie_out(p)),
        ("conservation", conservation(p)),
        ("no double counting", no_double_counting(p)),
        ("inputs = contributions", inputs_match(p)),
        ("lp certificates", certificates(p)),
    ];
    let lcr = eval(arf210::lcr_all(), &p.lcr_inputs);
    let nsfr = eval(arf210::nsfr(), &p.nsfr_inputs);
    match (lcr, nsfr) {
        (Ok(mut l), Ok(mut n)) => {
            let mut f = form_checks(&mut l, &mut warnings);
            f.extend(form_checks(&mut n, &mut warnings));
            checks.push(("form checks and theorems", f));
            let x = arf210::crossform::nsfr_vs_lcr(&n, &l)
                .into_iter()
                .filter(|f| !f.warn)
                .map(|f| format!("{}: {}", f.check_id, f.detail))
                .collect();
            checks.push(("cross-form", x));
            let mut report = Report { checks, warnings, bundle: None };
            if report.passed() {
                report.bundle = Some(bundle(p, &l, &report));
            }
            report
        }
        (l, n) => {
            let errs = [l.err(), n.err()].into_iter().flatten().collect();
            checks.push(("form evaluation", errs));
            Report { checks, warnings, bundle: None }
        }
    }
}

fn bundle(p: &Package, lcr: &Evaluation, r: &Report) -> Value {
    let specs: BTreeMap<&str, String> = p.specs.iter().map(|(path, src)| (path.as_str(), sha256::hex(src.as_bytes()))).collect();
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
        "data_snapshot": sha256::hex(canonical(p).as_bytes()),
        "spec_version": spec_version,
        "specs": specs,
        "certificates": certs,
        "assumptions": { "count": assumptions.len(), "sha256": sha256::hex(assumptions_text.as_bytes()) },
        "returns": {
            "ARF 210.1A": hash_map(&p.lcr_inputs),
            "ARF 210.6": hash_map(&p.nsfr_inputs),
        },
        "lcr_percent": lcr.q("30").to_string(),
        "checks": r.checks.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        "warnings": r.warnings,
    });
    let id = sha256::hex(manifest.to_string().as_bytes());
    manifest["bundle_id"] = json!(id);
    manifest
}
