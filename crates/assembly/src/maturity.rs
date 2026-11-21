//! The returns that follow from the same records without forecasts:
//! ARF 210.3.1 / 210.3.2 (contractual maturity mismatch, closing balances by
//! maturity bucket), the actual-balance column of ARF 210.4 (by the ARS
//! mapping to 210.3.x), and the ARF 210.5 figures that equal the same-date
//! LCR returns. Each cell comes with per-source contribution evidence.
//!
//! Only what the modules model is reported: capital, FX transaction legs,
//! securitisation and covered-bond funding, ARF 210.4 forecasts and the
//! five-day movements in ARF 210.5 are not: those cells stay zero.

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
pub fn build(bank: &Bank, ret: &Return, rd: NaiveDate, mode: Mode) -> Result<Vec<Built>, String> {
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
    // ARF 210.5: figures "as reported in ARF 210.1A" (and 210.1B for the AUD
    // LCR) at the reporting date.
    let lcr = eval(arf210::lcr_all(), &ret.inputs)?;
    let aud = eval(arf210::lcr_aud(), &checker::derive_aud_inputs(&crate::checker_parts(ret), &BTreeMap::new()))?;
    let mut daily = Vec::new();
    for (src, ev, from, to) in [
        ("ARF 210.1A", &lcr, "25:value", "1.1:balance"),
        ("ARF 210.1A", &lcr, "26:value", "1.4:balance"),
        ("ARF 210.1A", &lcr, "30:value", "5.2:day1"),
        ("ARF 210.1B", &aud, "30:value", "5.1:day1"),
    ] {
        if let Some(q) = ev.get(from).num()
            && q != &Q::default()
        {
            daily.push(checker::FormContribution { source: src.into(), cell: to.into(), amount: q.clone() });
        }
    }
    let daily = Acc(daily);
    Ok(vec![
        Built { code: "ARF 210.3.1", inputs: a.inputs(), contributions: a.0 },
        Built { code: "ARF 210.3.2", inputs: l.inputs(), contributions: l.0 },
        Built { code: "ARF 210.4", inputs: plan.inputs(), contributions: plan.0 },
        Built { code: "ARF 210.5", inputs: daily.inputs(), contributions: daily.0 },
    ])
}
