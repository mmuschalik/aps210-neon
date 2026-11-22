//! The returns beyond ARF 210.1A/1B/6. From the same records:
//! ARF 210.3.1 / 210.3.2 (contractual maturity mismatch, closing balances by
//! maturity bucket), the actual-balance column of ARF 210.4 (by the ARS
//! mapping to 210.3.x), and the ARF 210.5 figures that equal the same-date
//! LCR returns; ARF 210.2 for MLH ADIs. With `Extras`: the ARF 210.5 history
//! from earlier daily snapshots and the ARF 210.4 forecasts from the funding
//! plan. Each cell comes with per-source contribution evidence.
//!
//! ARF 210.5 item 3 percentages are computed where facility limits are
//! recorded (`Facility::limit`).

use std::collections::BTreeMap;

use chrono::{Days, Months, NaiveDate};
use prudence_core::uncertain::Mode;
use prudence_core::{Missing, Q, evaluate};

use crate::{Bank, Return, memo_row};

fn million() -> Q {
    Q::from_integer(1_000_000.into())
}

/// ARF 210.3.x maturity bucket ("Columns 2 to 14", ARS p.86) of a date; no
/// contractual maturity -> column 14.
pub fn bucket(rd: NaiveDate, d: Option<NaiveDate>) -> &'static str {
    let Some(d) = d else { return "y10plus" };
    let days = (d - rd).num_days();
    let m = |n: u32| rd + Months::new(n);
    if days <= 1 {
        "at_call"
    } else if days <= 7 {
        "d7"
    } else if days <= 15 {
        "d15"
    } else if days <= 30 {
        "d30"
    } else if d < m(2) {
        "m2"
    } else if d < m(3) {
        "m3"
    } else if d < m(6) {
        "m6"
    } else if d < m(12) {
        "m12"
    } else if d < m(24) {
        "y2"
    } else if d < m(36) {
        "y3"
    } else if d < m(60) {
        "y5"
    } else if d < m(120) {
        "y10"
    } else {
        "y10plus"
    }
}

/// Bucket boundaries (exclusive ends) for a repayment profile, in column order.
fn ends(rd: NaiveDate) -> Vec<(&'static str, NaiveDate)> {
    let m = |n: u32| rd + Months::new(n);
    vec![
        ("at_call", rd + Days::new(2)),
        ("d7", rd + Days::new(8)),
        ("d15", rd + Days::new(16)),
        ("d30", rd + Days::new(31)),
        ("m2", m(2)),
        ("m3", m(3)),
        ("m6", m(6)),
        ("m12", m(12)),
        ("y2", m(24)),
        ("y3", m(36)),
        ("y5", m(60)),
        ("y10", m(120)),
    ]
}

/// One return with its evidence: (form code, inputs, contributions).
pub struct Built {
    pub code: &'static str,
    pub inputs: BTreeMap<String, Q>,
    pub contributions: Vec<checker::FormContribution>,
}

#[derive(Default)]
struct Acc(Vec<checker::FormContribution>);

impl Acc {
    fn add(&mut self, source: &str, item: &str, col: &str, aud: &Q) {
        if aud != &Q::default() {
            self.0.push(checker::FormContribution { source: source.into(), cell: format!("{item}:{col}"), amount: aud / million() });
        }
    }
    fn inputs(&self) -> BTreeMap<String, Q> {
        let mut m: BTreeMap<String, Q> = BTreeMap::new();
        for c in &self.0 {
            *m.entry(c.cell.clone()).or_default() += c.amount.clone();
        }
        m
    }
}

/// The currency rows (9.1-9.7 of 210.3.1, 8.1-8.7 of 210.3.2); "all other
/// currencies" is the derived residual.
fn ccy_row(bank: &Bank, id: &str, total: &str) -> Option<String> {
    let r = memo_row(bank.currency.get(id).map(String::as_str).unwrap_or("AUD"));
    (r < 8).then(|| format!("{total}.{r}"))
}

fn assets(bank: &Bank, ret: &Return, rd: NaiveDate, mode: Mode) -> Result<Acc, String> {
    let mut a = Acc::default();
    let put = |a: &mut Acc, id: &str, item: &str, col: &str, q: &Q| {
        a.add(id, item, col, q);
        if let Some(row) = ccy_row(bank, id, "9") {
            a.add(id, &row, col, q);
        }
    };
    // Holdings: class from the row the holding reports in ARF 210.1A.
    for h in &bank.holdings {
        use secured::{AssetKind, Issuer};
        let row = ret.parts.iter().find(|p| p.record == h.id && p.module == "secured").map(|p| p.item.as_str()).unwrap_or("");
        let col = bucket(rd, h.maturity);
        let item = match h.kind {
            AssetKind::NotesAndCoin => "1.1",
            AssetKind::RbaBalance => "1.2",
            AssetKind::ForeignCentralBankBalance => "1.3",
            AssetKind::Equity if h.encumbered => "4",
            AssetKind::Equity => "3",
            _ if h.encumbered => "4",
            _ => {
                let short = h.maturity.is_some_and(|m| m < rd + Months::new(12));
                let class = row.split('.').next().unwrap_or("");
                match (h.domestic_currency, class) {
                    (true, "1") => match h.issuer {
                        Issuer::AustralianGovernment => "2.1.1",
                        Issuer::StateCentralBorrowingAuthority => "2.1.2",
                        _ => "2.1.3",
                    },
                    (true, "2") => "2.1.4",
                    (true, "3") => "2.1.5",
                    (true, "6") => "2.1.6",
                    (true, _) => if short { "2.1.7" } else { "2.1.8" },
                    (false, "1") => "2.2.1",
                    (false, "2") => "2.2.2",
                    (false, "3") => "2.2.3",
                    (false, "6") => "2.2.4",
                    (false, _) => if short { "2.2.5" } else { "2.2.6" },
                }
            }
        };
        let col = if matches!(item, "1.1" | "1.2" | "1.3") { "at_call" } else { col };
        put(&mut a, &h.id, item, col, &h.market_value);
    }
    // Reverse repos: receivables in the maturity bucket of the secured loan.
    for r in bank.repos.iter().filter(|r| r.leg == secured::Leg::Lending) {
        put(&mut a, &r.id, "5", bucket(rd, Some(r.maturity)), &r.cash);
    }
    // Loans: scheduled repayments by bucket (the spec's own schedule); open
    // maturity and non-performing loans in column 14 (ARS p.86).
    let e = ends(rd);
    let profile = loans::repayment_profile(&bank.loans, rd, mode, &e.iter().map(|(_, d)| *d).collect::<Vec<_>>())?;
    for (l, prof) in bank.loans.iter().zip(&profile) {
        use loans::{Affiliation, Borrower};
        let item = match (l.borrower, l.affiliation) {
            (Borrower::RetailCustomer, _) => "6.1.1",
            (Borrower::Sme, _) => "6.1.2",
            (Borrower::NonFinancialCorporate, _) => "6.1.3",
            (_, Affiliation::IntraGroupBank) => "6.1.4",
            _ => "6.1.5",
        };
        let open = matches!(l.contract, loans::Contract::Ump { .. })
            || !matches!(l.performing, prudence_core::uncertain::Uncertain::Known(true));
        let mut cols: Vec<(&str, Q)> = Vec::new();
        if open {
            cols.push(("y10plus", l.balance.clone()));
        } else {
            let mut before = Q::default();
            for ((col, _), cum) in e.iter().zip(prof) {
                cols.push((col, cum - &before));
                before = cum.clone();
            }
            cols.push(("y10plus", &l.balance - &before));
        }
        let encumbered = l.encumbered_until.is_some_and(|d| d > rd);
        for (col, q) in &cols {
            put(&mut a, &l.id, item, col, q);
            if encumbered {
                a.add(&l.id, "6.1.6", col, q);
            }
            match l.affiliation {
                Affiliation::IntraGroupBank => a.add(&l.id, "10.1", col, q),
                Affiliation::IntraGroupFinancial | Affiliation::IntraGroupOther => a.add(&l.id, "10.2", col, q),
                Affiliation::External => {}
            }
            if matches!(l.borrower, Borrower::NonFinancialCorporate | Borrower::Sme) {
                a.add(&l.id, "10.3", col, q);
                if l.borrower == Borrower::Sme {
                    a.add(&l.id, "10.3.1", col, q);
                }
            }
        }
    }
    // Deposits placed with other ADIs (on call: at call).
    for x in &bank.funding.placements {
        let item = if x.adi == funding::AdiType::ForeignAdi { "6.2.4" } else { "6.1.4" };
        let col = x.maturity.map(|d| bucket(rd, Some(d))).unwrap_or("at_call");
        put(&mut a, &x.id, item, col, &x.amount);
    }
    // FX transaction inflow legs by currency, gross (ARS p.90: item 8).
    for f in bank.derivatives.flows.iter().filter(|f| f.fx_principal && f.amount > Q::default()) {
        let col = bucket(rd, Some(f.date));
        a.add(&f.set, "8", col, &f.amount);
        let row = memo_row(&f.currency);
        if row < 8 {
            a.add(&f.set, &format!("8.{row}"), col, &f.amount);
        }
    }
    // Other receivables.
    for x in &bank.other_inflows.receivables {
        put(&mut a, &x.id, "7", bucket(rd, Some(x.due)), &x.amount);
    }
    Ok(a)
}

fn liabilities(bank: &Bank, rd: NaiveDate) -> Acc {
    let mut a = Acc::default();
    let put = |a: &mut Acc, id: &str, item: &str, col: &str, q: &Q| {
        a.add(id, item, col, q);
        if let Some(row) = ccy_row(bank, id, "8") {
            a.add(id, &row, col, q);
        }
    };
    // Retail and SME deposits: by contractual maturity, notice deposits by
    // their notice period (ARS p.106).
    for d in &bank.retail.accounts {
        use deposits::{Contract, CustomerKind};
        let Some(c) = bank.retail.customers.iter().find(|c| c.id == d.customer_id) else { continue };
        let item = match c.kind {
            CustomerKind::Sme => "1.1.2",
            CustomerKind::NonFinancialCorporate => "1.1.3",
            CustomerKind::FinancialInstitution => "1.1.5",
            _ => "1.1.1",
        };
        let col = match &d.contract {
            Contract::Ump => "at_call",
            Contract::Clm { notice_days, .. } => bucket(rd, Some(rd + Days::new(u64::from(*notice_days)))),
            Contract::Pam { maturity, .. } => bucket(rd, Some(*maturity)),
        };
        let q = d.balance.amount();
        put(&mut a, &d.id, item, col, q);
        if matches!(c.kind, CustomerKind::Smsf | CustomerKind::PersonalInvestmentEntity) {
            a.add(&d.id, "9.5", col, q);
            if c.kind == CustomerKind::Smsf {
                a.add(&d.id, "9.5.1", col, q);
            }
        }
    }
    for d in &bank.wholesale_deposits {
        use deposits::wholesale::Kind;
        let Some(c) = bank.wholesale_customers.iter().find(|c| c.id == d.customer_id) else { continue };
        let item = match c.kind {
            Kind::NonFinancialCorporate | Kind::SovereignCentralBankPseMdb => "1.1.3",
            Kind::Bank => "1.1.4",
            Kind::OtherFinancialInstitution | Kind::OtherLegalEntity => "1.1.5",
        };
        let col = d.maturity.map(|m| bucket(rd, Some(m))).unwrap_or("at_call");
        put(&mut a, &d.id, item, col, &d.balance);
    }
    // Debt securities issued; covered bonds and securitisation as secured funding.
    for x in &bank.funding.securities {
        use funding::SecurityKind::*;
        let item = match (x.kind, x.offshore) {
            (ShortTerm, false) => "2.1.1",
            (LongTerm, false) => "2.1.2",
            (EuroCommercialPaper, _) => "2.2.1",
            (UsCommercialPaper, _) => "2.2.2",
            (ShortTerm, true) => "2.2.3",
            (LongTerm, true) => "2.2.4",
            (CoveredBond, false) => "3.1.2",
            (ShortTermSecuritisation, false) => "3.1.3",
            (LongTermSecuritisation, false) => "3.1.4",
            (CoveredBond, true) => "3.2.2",
            (ShortTermSecuritisation, true) => "3.2.3",
            (LongTermSecuritisation, true) => "3.2.4",
        };
        let col = bucket(rd, Some(x.maturity));
        put(&mut a, &x.id, item, col, &x.amount);
        if x.kind == LongTermSecuritisation {
            a.add(&x.id, "9.7", col, &x.amount);
        }
    }
    // Capital: no contractual maturity -> column 14 (ARS p.107).
    for x in &bank.funding.capital {
        let item = match x.tier {
            funding::Tier::CET1 => "6.1",
            funding::Tier::AT1 => "6.2",
            funding::Tier::T2 => "6.3",
        };
        put(&mut a, &x.id, item, bucket(rd, x.maturity), &x.amount);
    }
    // FX transaction outflow legs by currency, gross (item 7).
    for f in bank.derivatives.flows.iter().filter(|f| f.fx_principal && f.amount < Q::default()) {
        let col = bucket(rd, Some(f.date));
        let q = -f.amount.clone();
        a.add(&f.set, "7", col, &q);
        let row = memo_row(&f.currency);
        if row < 8 {
            a.add(&f.set, &format!("7.{row}"), col, &q);
        }
    }
    // Repurchase agreements.
    for r in bank.repos.iter().filter(|r| r.leg == secured::Leg::Funding) {
        let col = bucket(rd, Some(r.maturity));
        if r.counterparty == secured::Counterparty::OffshoreCentralBank {
            put(&mut a, &r.id, "3.2.1", col, &r.cash);
            a.add(&r.id, "3.2.1.1", col, &r.cash);
        } else {
            put(&mut a, &r.id, "3.1.1", col, &r.cash);
        }
    }
    a
}

/// ARF 210.3.1, 210.3.2, 210.4 (actual balances) and 210.5 (the same-date
/// LCR figures), each with its contribution evidence.
pub fn build(bank: &Bank, ret: &Return, rd: NaiveDate, mode: Mode, regime: &str, extras: &Extras) -> Result<Vec<Built>, String> {
    let a = assets(bank, ret, rd, mode)?;
    let l = liabilities(bank, rd);
    let eval = |form: &'static prudence_core::Form, m: &BTreeMap<String, Q>| {
        evaluate(form, m.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).map_err(|e| format!("{e:?}"))
    };
    // ARF 210.4 actual balances: each source's 210.3.x balances, item by item
    // through the ARS mapping.
    let mapping = arf210::crossform::funding_plan_mapping();
    let mut plan = Acc::default();
    for (acc, code) in [(&a, "ARF 210.3.1"), (&l, "ARF 210.3.2")] {
        let form = arf210::form(code).unwrap();
        let mut by_source: BTreeMap<&str, BTreeMap<String, Q>> = BTreeMap::new();
        for c in &acc.0 {
            *by_source.entry(c.source.as_str()).or_default().entry(c.cell.clone()).or_default() += c.amount.clone();
        }
        for (src, cells) in by_source {
            let ev = eval(form, &cells)?;
            for (pi, _, ti) in mapping.iter().filter(|(_, c, _)| *c == code) {
                if let Some(q) = ev.get(&format!("{ti}:balance")).num()
                    && q != &Q::default()
                {
                    plan.0.push(checker::FormContribution { source: src.into(), cell: format!("{pi}:actual"), amount: q.clone() });
                }
            }
        }
    }
    plan.0.extend(plan_forecasts(extras));
    let daily = Acc(if regime == "mlh" { daily_mlh(bank, rd, mode)? } else { daily_lcr(bank, ret, rd, mode, extras)? });
    let mut out = vec![
        Built { code: "ARF 210.3.1", inputs: a.inputs(), contributions: a.0 },
        Built { code: "ARF 210.3.2", inputs: l.inputs(), contributions: l.0 },
        Built { code: "ARF 210.4", inputs: plan.inputs(), contributions: plan.0 },
        Built { code: "ARF 210.5", inputs: daily.inputs(), contributions: daily.0 },
    ];
    if regime == "mlh" {
        let m = Acc(mlh(bank, rd, mode)?);
        out.push(Built { code: "ARF 210.2", inputs: m.inputs(), contributions: m.0 });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Data beyond one reporting date: earlier daily snapshots (ARF 210.5) and the
// funding plan (ARF 210.4 forecasts).

/// What the reporting-date records cannot give.
#[derive(Default)]
pub struct Extras {
    /// The bank as at earlier business days, most recent first: days 2 to 5
    /// of ARF 210.5 item 5 (the reporting date is day 1).
    pub recent: Vec<(NaiveDate, Bank)>,
    /// The bank five business days before the reporting date (ARF 210.5
    /// column 2, and the five-day movements).
    pub prior: Option<(NaiveDate, Bank)>,
    /// ARF 210.4 forecast balances (AUD) by cell, e.g. "9.1.1:m12", from the
    /// ADI's funding plan.
    pub plan: BTreeMap<String, Q>,
}

/// ARF 210.5 funding balances (item 2) of one bank: (source, row, AUD).
fn funding_balances(bank: &Bank) -> Vec<(String, &'static str, Q)> {
    use deposits::CustomerKind;
    use deposits::wholesale::Kind;
    let mut out = Vec::new();
    for d in &bank.retail.accounts {
        let Some(c) = bank.retail.customers.iter().find(|c| c.id == d.customer_id) else { continue };
        let row = match c.kind {
            CustomerKind::NonFinancialCorporate => "2.2",
            CustomerKind::FinancialInstitution => "2.3",
            _ => "2.1",
        };
        out.push((d.id.clone(), row, d.balance.amount().clone()));
    }
    for d in &bank.wholesale_deposits {
        let Some(c) = bank.wholesale_customers.iter().find(|c| c.id == d.customer_id) else { continue };
        let row = match c.kind {
            Kind::Bank | Kind::OtherFinancialInstitution => "2.3",
            _ => "2.2",
        };
        out.push((d.id.clone(), row, d.balance.clone()));
    }
    // Wholesale funding not included above: short-term (2.4) and other (2.5).
    for x in &bank.funding.securities {
        use funding::SecurityKind::*;
        let row = if matches!(x.kind, ShortTerm | EuroCommercialPaper | UsCommercialPaper) { "2.4" } else { "2.5" };
        out.push((x.id.clone(), row, x.amount.clone()));
        if x.offshore {
            out.push((x.id.clone(), if row == "2.4" { "2.4.1" } else { "2.5.1" }, x.amount.clone()));
        }
    }
    out
}

/// ARF 210.5 funding maturity bucket (columns 2-10): within six months only.
fn daily_bucket(rd: NaiveDate, d: Option<NaiveDate>) -> Option<&'static str> {
    let Some(d) = d else { return Some("at_call") };
    let days = (d - rd).num_days();
    let m = |n: u32| rd + Months::new(n);
    Some(if days <= 1 {
        "at_call"
    } else if days <= 7 {
        "d7"
    } else if days <= 15 {
        "d15"
    } else if days <= 30 {
        "d30"
    } else if d < m(2) {
        "m2"
    } else if d < m(3) {
        "m3"
    } else if d < m(4) {
        "m4"
    } else if d < m(5) {
        "m5"
    } else if d < m(6) {
        "m6"
    } else {
        return None;
    })
}

/// LCR figures of one bank as at one date: (HQLA, RBNZ+ALA, LCR %, AUD LCR %).
fn lcr_figures(bank: &Bank, date: NaiveDate, mode: Mode) -> Result<[Option<Q>; 4], String> {
    figures_of(&crate::lcr_return(bank, date, mode)?)
}

/// The same figures from a return as lodged (with any overlays applied).
fn figures_of(ret: &Return) -> Result<[Option<Q>; 4], String> {
    let eval = |form: &'static prudence_core::Form, m: &BTreeMap<String, Q>| {
        evaluate(form, m.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).map_err(|e| format!("{e:?}"))
    };
    let all = eval(arf210::lcr_all(), &ret.inputs)?;
    let aud = eval(arf210::lcr_aud(), &checker::derive_aud_inputs(&crate::checker_parts(ret), &BTreeMap::new()))?;
    let g = |e: &prudence_core::Evaluation, k: &str| e.get(k).num().cloned();
    Ok([g(&all, "25:value"), g(&all, "26:value"), g(&all, "30:value"), g(&aud, "30:value")])
}

/// ARF 210.5 for an LCR ADI: liquid assets (as in ARF 210.1A) now and five
/// business days before, funding balances, net loan principal outflow and
/// collateral flows over the five days, the LCRs over five business days,
/// and funding maturities over six months.
pub fn daily_lcr(bank: &Bank, ret: &Return, rd: NaiveDate, mode: Mode, extras: &Extras) -> Result<Vec<checker::FormContribution>, String> {
    let mut a = Acc::default();
    let lit = |a: &mut Acc, source: &str, cell: &str, q: Option<Q>| {
        if let Some(q) = q
            && q != Q::default()
        {
            a.0.push(checker::FormContribution { source: source.into(), cell: cell.into(), amount: q });
        }
    };
    let [hqla, ala, lcr, lcr_aud] = figures_of(ret)?;
    lit(&mut a, "ARF 210.1A", "1.1:balance", hqla);
    lit(&mut a, "ARF 210.1A", "1.4:balance", ala);
    lit(&mut a, "ARF 210.1A", "5.2:day1", lcr);
    lit(&mut a, "ARF 210.1B", "5.1:day1", lcr_aud);
    for (n, (date, snap)) in extras.recent.iter().take(4).enumerate() {
        let [_, _, l, la] = lcr_figures(snap, *date, mode)?;
        let src = format!("snapshot {date}");
        lit(&mut a, &src, &format!("5.2:day{}", n + 2), l);
        lit(&mut a, &src, &format!("5.1:day{}", n + 2), la);
    }
    for (src, row, q) in funding_balances(bank) {
        a.add(&src, row, "balance", &q);
    }
    if let Some((pd, prior)) = &extras.prior {
        let src = format!("snapshot {pd}");
        let [h, al, _, _] = lcr_figures(prior, *pd, mode)?;
        lit(&mut a, &src, "1.1:prior", h);
        lit(&mut a, &src, "1.4:prior", al);
        for (_, row, q) in funding_balances(prior) {
            a.add(&src, row, "prior", &q);
        }
        // Item 3.1: net principal outflow = growth of the loan book
        // (disbursements positive, repayments negative; ARS p.130).
        let now: Q = bank.loans.iter().map(|l| l.balance.clone()).sum();
        let then: Q = prior.loans.iter().map(|l| l.balance.clone()).sum();
        a.add(&src, "3.1", "outflow", &(&now - &then));
        // Percentages (ARS p.130): 3.1 of the principal outstanding five days
        // before; 3.2-3.5 of the facility sizes five days before; 3.6 their
        // size-weighted average.
        let pct = |out: &Q, base: &Q| (base != &Q::default()).then(|| Q::from_integer(100.into()) * out / base);
        if let Some(p) = pct(&(&now - &then), &then) {
            a.0.push(checker::FormContribution { source: src.clone(), cell: "3.1:outflow_pct".into(), amount: p });
        }
        // Item 8.1: notices given on minimum notice deposits over the five
        // days (number and value); the percentage of the average balance of
        // notice deposits over the five days, when all five days are known.
        let notice_balance = |b: &Bank| -> Q {
            b.retail
                .accounts
                .iter()
                .filter(|d| matches!(d.contract, deposits::Contract::Clm { .. }))
                .map(|d| d.balance.amount().clone())
                .sum()
        };
        let (mut n, mut value) = (0i64, Q::default());
        for e in bank.retail.ledger.as_of(rd, rd) {
            if let deposits::EventKind::NoticeGiven { account_id, .. } = &e.kind
                && *pd < e.valid
                && let Some(d) = bank.retail.accounts.iter().find(|d| &d.id == account_id)
            {
                n += 1;
                value += d.balance.amount().clone();
            }
        }
        if n > 0 {
            let src = format!("notice events {pd}..{rd}");
            a.0.push(checker::FormContribution { source: src.clone(), cell: "8.1:number".into(), amount: Q::from_integer(n.into()) });
            a.add(&src, "8.1", "amount", &value);
            if extras.recent.len() >= 4 {
                let days: Vec<Q> = std::iter::once(notice_balance(bank)).chain(extras.recent.iter().take(4).map(|(_, b)| notice_balance(b))).collect();
                let avg = days.iter().cloned().sum::<Q>() / Q::from_integer(5.into());
                if avg != Q::default() {
                    a.0.push(checker::FormContribution {
                        source: src,
                        cell: "8.1:amount_pct".into(),
                        amount: Q::from_integer(100.into()) * &value / avg,
                    });
                }
            }
        }
        // Items 3.2-3.5: net drawings under facilities over the five days -
        // the fall in each facility's undrawn amount (column 1); the undrawn
        // amount today (column 3).
        let mut sizes: BTreeMap<&str, (Q, Q, bool)> = BTreeMap::new();
        for f in &bank.commitments.facilities {
            use obligations::FacilityKind;
            let row = if !(f.committed || f.mortgage_redraw) {
                "3.2"
            } else if f.mortgage_redraw {
                "3.5"
            } else if f.kind == FacilityKind::LiquidityFacility {
                "3.3"
            } else {
                "3.4"
            };
            a.add(&f.id, row, "undrawn", &f.undrawn);
            if let Some(before) = prior.commitments.facilities.iter().find(|g| g.id == f.id) {
                let drawn = &before.undrawn - &f.undrawn;
                a.add(&src, row, "outflow", &drawn);
                let e = sizes.entry(row).or_insert((Q::default(), Q::default(), true));
                e.0 += drawn.clone();
                match &before.limit {
                    Some(l) => e.1 += l.clone(),
                    None => e.2 = false,
                }
            }
        }
        let (mut tot_out, mut tot_size, mut all_known) = (Q::default(), Q::default(), true);
        for (row, (out, size, known)) in &sizes {
            tot_out += out.clone();
            tot_size += size.clone();
            all_known &= *known;
            if *known && let Some(p) = pct(out, size) {
                a.0.push(checker::FormContribution { source: src.clone(), cell: format!("{row}:outflow_pct"), amount: p });
            }
        }
        if all_known && let Some(p) = pct(&tot_out, &tot_size) {
            a.0.push(checker::FormContribution { source: src.clone(), cell: "3.6:outflow_pct".into(), amount: p });
        }
        // Items 8.2-8.3: early repayment requests over the five days, number,
        // value, approved, and value as a percentage of the five-day average
        // balance of that funding (when all five days are known).
        let short = |k: funding::SecurityKind| {
            use funding::SecurityKind::*;
            matches!(k, ShortTerm | EuroCommercialPaper | UsCommercialPaper)
        };
        let balance = |b: &Bank, st: bool| -> Q {
            b.funding.securities.iter().filter(|x| short(x.kind) == st).map(|x| x.amount.clone()).sum()
        };
        for (row, st) in [("8.2", true), ("8.3", false)] {
            let reqs: Vec<&funding::EarlyRepaymentRequest> = bank
                .funding
                .early_repayment_requests
                .iter()
                .filter(|r| *pd < r.requested && r.requested <= rd)
                .filter(|r| bank.funding.securities.iter().any(|x| x.id == r.security && short(x.kind) == st))
                .collect();
            if reqs.is_empty() {
                continue;
            }
            let rsrc = format!("early repayment requests {pd}..{rd}");
            let value: Q = reqs.iter().map(|r| r.amount.clone()).sum();
            let approved: Q = reqs.iter().filter(|r| r.approved).map(|r| r.amount.clone()).sum();
            a.0.push(checker::FormContribution { source: rsrc.clone(), cell: format!("{row}:number"), amount: Q::from_integer((reqs.len() as i64).into()) });
            a.0.push(checker::FormContribution {
                source: rsrc.clone(),
                cell: format!("{row}.1:number"),
                amount: Q::from_integer((reqs.iter().filter(|r| r.approved).count() as i64).into()),
            });
            a.add(&rsrc, row, "amount", &value);
            a.add(&rsrc, &format!("{row}.1"), "amount", &approved);
            if extras.recent.len() >= 4 {
                let avg = (std::iter::once(balance(bank, st)).chain(extras.recent.iter().take(4).map(|(_, b)| balance(b, st))).sum::<Q>())
                    / Q::from_integer(5.into());
                if avg != Q::default() {
                    a.0.push(checker::FormContribution {
                        source: rsrc,
                        cell: format!("{row}:amount_pct"),
                        amount: Q::from_integer(100.into()) * &value / avg,
                    });
                }
            }
        }
        // Item 4: collateral realised over the five days, as variation margin
        // when initial and variation cannot be told apart (ARS p.130);
        // outflows positive, inflows negative.
        for c in &bank.derivatives.collateral_flows {
            if *pd < c.valid && c.valid <= rd && c.recorded <= rd {
                let row = if c.amount < Q::default() { "4.3" } else { "4.4" };
                a.add(&format!("collateral events {pd}..{rd}"), row, "outflow", &-c.amount.clone());
            }
        }
    }
    // Item 6: funding maturing within six months.
    for d in &bank.retail.accounts {
        use deposits::{Contract, CustomerKind};
        let Some(c) = bank.retail.customers.iter().find(|c| c.id == d.customer_id) else { continue };
        let row = match c.kind {
            CustomerKind::NonFinancialCorporate => "6.2",
            CustomerKind::FinancialInstitution => "6.3",
            _ => "6.1",
        };
        let due = match &d.contract {
            Contract::Ump => None,
            Contract::Clm { notice_days, .. } => Some(rd + Days::new(u64::from(*notice_days))),
            Contract::Pam { maturity, .. } => Some(*maturity),
        };
        if let Some(col) = daily_bucket(rd, due) {
            a.add(&d.id, row, col, d.balance.amount());
        }
    }
    for d in &bank.wholesale_deposits {
        use deposits::wholesale::Kind;
        let Some(c) = bank.wholesale_customers.iter().find(|c| c.id == d.customer_id) else { continue };
        let row = if matches!(c.kind, Kind::Bank | Kind::OtherFinancialInstitution) { "6.3" } else { "6.2" };
        if let Some(col) = daily_bucket(rd, d.maturity) {
            a.add(&d.id, row, col, &d.balance);
        }
    }
    for r in bank.repos.iter().filter(|r| r.leg == secured::Leg::Funding) {
        if let Some(col) = daily_bucket(rd, Some(r.maturity)) {
            a.add(&r.id, "6.5", col, &r.cash);
        }
    }
    for x in &bank.funding.securities {
        use funding::SecurityKind::*;
        let row = match x.kind {
            ShortTerm | EuroCommercialPaper | UsCommercialPaper => "6.4",
            LongTerm => "6.6",
            _ => "6.5",
        };
        if let Some(col) = daily_bucket(rd, Some(x.maturity)) {
            a.add(&x.id, row, col, &x.amount);
            if x.offshore {
                a.add(&x.id, &format!("{row}.1"), col, &x.amount);
            }
        }
    }
    Ok(a.0)
}

/// ARF 210.2 for an MLH ADI: MLH assets from the holdings, the liability base
/// from deposits and repurchase agreements, and off-balance sheet irrevocable
/// commitments from the commitments module.
pub fn mlh(bank: &Bank, rd: NaiveDate, mode: Mode) -> Result<Vec<checker::FormContribution>, String> {
    use secured::{AssetKind, Issuer};
    let mut a = Acc::default();
    for h in bank.holdings.iter().filter(|h| !h.encumbered && !bank.capital_relief_spv.contains(&h.id)) {
        let item = match (h.kind, h.issuer) {
            (AssetKind::NotesAndCoin, _) => "1.1",
            (AssetKind::RbaBalance, _) => "1.3.1",
            (AssetKind::Equity, _) | (AssetKind::ForeignCentralBankBalance, _) => continue,
            (_, Issuer::AustralianGovernment) => "2.1",
            (_, Issuer::StateCentralBorrowingAuthority) => "2.2",
            (_, Issuer::GuaranteedByAustralianGovernment) => "3",
            (_, Issuer::Supranational) => "4.1",
            (_, Issuer::ForeignSovereign) => "4.2",
            (_, Issuer::GuaranteedByForeignSovereign) => "4.3",
            (_, Issuer::FinancialInstitutionIssuer) => "5.3",
            _ => continue,
        };
        a.add(&h.id, item, "amount", &h.market_value);
    }
    for d in bank.retail.accounts.iter().filter(|d| !bank.capital_relief_spv.contains(&d.id)) {
        a.add(&d.id, "12.1", "amount", d.balance.amount());
    }
    for d in bank.wholesale_deposits.iter().filter(|d| !bank.capital_relief_spv.contains(&d.id)) {
        a.add(&d.id, "12.1", "amount", &d.balance);
    }
    for r in bank.repos.iter().filter(|r| r.leg == secured::Leg::Funding && !bank.capital_relief_spv.contains(&r.id)) {
        a.add(&r.id, "12.1", "amount", &r.cash);
    }
    for x in bank.funding.securities.iter().filter(|x| !bank.capital_relief_spv.contains(&x.id)) {
        a.add(&x.id, "12.1", "amount", &x.amount);
    }
    // Placements with other ADIs: the spec's ARF 210.2 rows.
    for (id, form, item, _, q) in funding::run(&bank.funding, rd, mode)?.cells {
        if form == "ARF 210.2" && !bank.capital_relief_spv.contains(&id) {
            a.add(&id, &item, "amount", &q);
        }
    }
    for f in bank.commitments.facilities.iter().filter(|f| (f.committed || f.mortgage_redraw) && !bank.capital_relief_spv.contains(&f.id)) {
        a.add(&f.id, if f.mortgage_redraw { "13.6" } else { "13.4" }, "amount", &f.undrawn);
    }
    for c in bank.commitments.commitments.iter().filter(|c| !bank.capital_relief_spv.contains(&c.id)) {
        a.add(&c.id, "13.5", "amount", &c.amount);
    }
    for o in bank.commitments.obligations.iter().filter(|o| !bank.capital_relief_spv.contains(&o.id)) {
        use obligations::ObligationKind;
        let row = match o.kind {
            ObligationKind::TradeFinance => "13.1",
            ObligationKind::Guarantee => "13.2",
            _ => "13.8",
        };
        a.add(&o.id, row, "amount", &o.amount);
    }
    Ok(a.0)
}

/// ARF 210.5 for an MLH ADI: liquid assets as in ARF 210.2 (ARS p.129).
pub fn daily_mlh(bank: &Bank, rd: NaiveDate, mode: Mode) -> Result<Vec<checker::FormContribution>, String> {
    let mut out = Vec::new();
    for c in mlh(bank, rd, mode)? {
        let item = c.cell.trim_end_matches(":amount");
        let row = match item {
            "1.1" | "1.3.1" | "2.1" | "2.2" | "3" => "1.1",
            "1.2" | "1.3.2" | "1.3.3" | "1.3.4" | "1.3.5" | "7" => "1.5",
            i if i.starts_with("6.") => "1.5",
            _ => continue,
        };
        out.push(checker::FormContribution { source: c.source, cell: format!("{row}:balance"), amount: c.amount });
    }
    Ok(out)
}

/// ARF 210.4 forecast columns from the funding plan.
pub fn plan_forecasts(extras: &Extras) -> Vec<checker::FormContribution> {
    extras
        .plan
        .iter()
        .filter(|(k, q)| !k.ends_with(":actual") && **q != Q::default())
        .map(|(k, q)| checker::FormContribution { source: "funding plan".into(), cell: k.clone(), amount: q / million() })
        .collect()
}
