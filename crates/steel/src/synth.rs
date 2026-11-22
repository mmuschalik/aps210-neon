//! A synthetic ADI from a seed: every book the engine reads, at any size, with
//! a controllable share of missing regulatory attributes, an independently
//! booked general ledger, and drifted daily snapshots for ARF 210.5.
//!
//! The generator is plain data production: it never calls the engine. The GL
//! is booked from the raw books the way a finance system would, so the gate's
//! tie-out is a real test of the engine's record list, not a tautology.

use std::collections::BTreeMap;

use assembly::{Bank, default_account};
use chrono::{Days, NaiveDate};
use deposits::{Contract, Customer, CustomerKind, DepositAccount, EarlyWithdrawal, EventKind};
use prudence_core::Q;
use prudence_core::money::AudCarrying;
use prudence_core::num::int;
use prudence_core::uncertain::Uncertain;

/// What to generate.
#[derive(Clone, Debug)]
pub struct Config {
    pub seed: u64,
    /// Retail and SME customers; every other book scales from this.
    pub customers: usize,
    /// Percent chance that each `Uncertain` attribute is missing upstream.
    pub unknown_pct: u64,
    pub reporting_date: NaiveDate,
    /// HQLA held per dollar of deposits, in percent (sets the LCR level).
    pub hqla_pct: u64,
}

impl Config {
    pub fn new(seed: u64, customers: usize) -> Self {
        Config {
            seed,
            customers,
            unknown_pct: 10,
            reporting_date: NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            hqla_pct: 28,
        }
    }
}

/// SplitMix64: tiny, deterministic, good enough for test data.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.u64() % (hi - lo + 1)
    }
    pub fn chance(&mut self, pct: u64) -> bool {
        self.u64() % 100 < pct
    }
    pub fn pick<T: Clone>(&mut self, xs: &[T]) -> T {
        xs[(self.u64() % xs.len() as u64) as usize].clone()
    }
    /// Pick by integer weights.
    pub fn weighted<T: Clone>(&mut self, xs: &[(u64, T)]) -> T {
        let total: u64 = xs.iter().map(|(w, _)| w).sum();
        let mut r = self.u64() % total;
        for (w, x) in xs {
            if r < *w {
                return x.clone();
            }
            r -= w;
        }
        unreachable!()
    }
    /// Split `total` dollars into `n` skewed pieces that sum to it exactly.
    pub fn split(&mut self, total: u64, n: usize) -> Vec<u64> {
        let w: Vec<u64> = (0..n.max(1)).map(|_| self.dollars(1, 1000)).collect();
        let sum: u64 = w.iter().sum();
        let mut out: Vec<u64> = w.iter().map(|x| (total as u128 * *x as u128 / sum as u128) as u64).collect();
        out[0] += total - out.iter().sum::<u64>();
        out
    }
    /// Roughly log-uniform dollars in `lo..=hi`: many small, few large.
    pub fn dollars(&mut self, lo: u64, hi: u64) -> u64 {
        let (a, b) = ((lo as f64).ln(), (hi as f64).ln());
        let u = (self.u64() >> 11) as f64 / (1u64 << 53) as f64;
        ((a + u * (b - a)).exp() as u64).clamp(lo, hi)
    }
}

pub fn aud(dollars: u64) -> Q {
    int(dollars as i64)
}

fn days(d: NaiveDate, n: u64) -> NaiveDate {
    d + Days::new(n)
}

/// Generate the bank as at the reporting date.
pub fn generate(cfg: &Config) -> Bank {
    let mut g = Gen { rng: Rng::new(cfg.seed), cfg: cfg.clone(), rd: cfg.reporting_date };
    let mut b = Bank::default();
    // Every book is sized as a share of retail funding (`unit`), roughly the
    // balance sheet of an Australian retail bank: retail 55%, wholesale 25%,
    // issued debt 15%, capital 6% of funding.
    g.retail(&mut b);
    let unit: u64 = b.retail.accounts.iter().map(|a| to_u64(a.balance.amount())).sum::<u64>().max(1_000_000);
    g.wholesale(&mut b, unit * 45 / 100);
    g.funding(&mut b, unit);
    let deposits = unit + b.wholesale_deposits.iter().map(|d| to_u64(&d.balance)).sum::<u64>();
    let hqla = deposits / 100 * cfg.hqla_pct;
    g.holdings(&mut b, hqla);
    g.repos(&mut b, unit);
    let funded = deposits
        + b.funding.securities.iter().chain([]).map(|x| to_u64(&x.amount)).sum::<u64>()
        + b.funding.capital.iter().map(|x| to_u64(&x.amount)).sum::<u64>();
    g.loans(&mut b, funded.saturating_sub(hqla + unit / 50));
    g.commitments(&mut b, unit);
    g.derivatives(&mut b, unit);
    populate_counterparties(&mut b);
    book_gl(&mut b);
    b
}

/// Populate the canonical cross-book `Counterparty` registry from the generated books.
pub fn populate_counterparties(b: &mut Bank) {
    let mut cps = Vec::new();
    for c in &b.retail.customers {
        let sector = match c.kind {
            CustomerKind::NaturalPerson => "NaturalPerson",
            CustomerKind::Smsf => "Smsf",
            CustomerKind::PersonalInvestmentEntity => "PersonalInvestmentEntity",
            CustomerKind::Sme => "Sme",
            CustomerKind::NonFinancialCorporate => "NonFinancialCorporate",
            CustomerKind::FinancialInstitution => "OtherFinancialInstitution",
        };
        cps.push(assembly::Counterparty {
            id: c.id.clone(),
            legal_name: format!("Entity {}", c.id),
            sector: sector.into(),
            adi_tier: None,
            affiliation: "External".into(),
            jurisdiction: "AU_APRA".into(),
            connected_group_id: c.sme_group.known().cloned(),
            sme_group: c.sme_group.known().cloned(),
            established_relationship: c.established_relationship.known().copied(),
            managed_as_retail: c.managed_as_retail.known().copied(),
            pie_qualifies: c.pie_qualifies.known().copied(),
        });
    }
    for c in &b.wholesale_customers {
        let (sector, adi_tier) = match c.kind {
            deposits::wholesale::Kind::NonFinancialCorporate => ("NonFinancialCorporate", None),
            deposits::wholesale::Kind::SovereignCentralBankPseMdb => ("SovereignCentralBankPseMdb", None),
            deposits::wholesale::Kind::Bank => ("RegulatedBank", Some("LcrAdi".to_string())),
            deposits::wholesale::Kind::OtherFinancialInstitution => ("OtherFinancialInstitution", None),
            deposits::wholesale::Kind::OtherLegalEntity => ("OtherLegalEntity", None),
        };
        cps.push(assembly::Counterparty {
            id: c.id.clone(),
            legal_name: format!("Wholesale {}", c.id),
            sector: sector.into(),
            adi_tier,
            affiliation: "External".into(),
            jurisdiction: "AU_APRA".into(),
            connected_group_id: None,
            sme_group: None,
            established_relationship: None,
            managed_as_retail: None,
            pie_qualifies: None,
        });
    }
    b.counterparties = cps;
}

fn to_u64(q: &Q) -> u64 {
    q.to_integer().try_into().unwrap_or(0)
}

/// Book the general ledger from the raw books (what finance would supply).
/// Replaces any existing ledger.
pub fn book_gl(b: &mut Bank) {
    let mut gl: BTreeMap<String, Q> = BTreeMap::new();
    let mut post = |kind: &str, q: &Q| *gl.entry(default_account(kind).into()).or_default() += q.clone();
    for a in &b.retail.accounts {
        post("retail", a.balance.amount());
    }
    for d in &b.wholesale_deposits {
        post("wholesale", &d.balance);
    }
    for h in &b.holdings {
        post("holding", &h.market_value);
    }
    for r in &b.repos {
        post(if r.leg == secured::Leg::Funding { "repo-funding" } else { "repo-lending" }, &r.cash);
    }
    for l in &b.loans {
        post("loan", &l.balance);
    }
    for x in &b.other_inflows.receivables {
        post("receivable", &x.amount);
    }
    for x in &b.funding.securities {
        post("issued", &x.amount);
    }
    for x in &b.funding.capital {
        post("capital", &x.amount);
    }
    for x in &b.funding.placements {
        post("placement", &x.amount);
    }
    for t in &b.derivatives.trades {
        post("derivative", &t.mtm);
    }
    b.gl = gl;
}

/// The same bank as at `back` days before the reporting date: identical
/// records, balances moved by a daily trend per book plus up to +/-1% noise
/// (deterministic per record), so ARF 210.5 movements and the day-on-day
/// attribution are non-trivial. Trend, going back in time: deposits were
/// higher (a slow run-off), loans lower (the book grows), facilities less
/// drawn, securities priced higher (a sell-off). The ledger is rebooked.
pub fn snapshot(cfg: &Config, back: u64) -> (NaiveDate, Bank) {
    let mut b = generate(cfg);
    let mut r = Rng::new(cfg.seed.wrapping_mul(31).wrapping_add(back));
    let back_i = back as i64;
    // `trend_bp`: basis points per day back.
    let mut factor = |trend_bp: i64| {
        let bp = trend_bp * back_i + r.range(0, 200) as i64 - 100;
        Q::new((10_000 + bp).into(), 10_000.into())
    };
    for a in &mut b.retail.accounts {
        let q = (a.balance.amount() * factor(30)).round();
        a.balance = AudCarrying::new(q);
    }
    for d in &mut b.wholesale_deposits {
        d.balance = (&d.balance * factor(150)).round();
    }
    for l in &mut b.loans {
        l.balance = (&l.balance * factor(-20)).round();
    }
    for f in &mut b.commitments.facilities {
        f.undrawn = (&f.undrawn * factor(40)).round();
    }
    for h in &mut b.holdings {
        let k = factor(25);
        h.market_value = (&h.market_value * &k).round();
        h.value_after_rba_margin = (&h.value_after_rba_margin * &k).round();
    }
    book_gl(&mut b);
    (cfg.reporting_date - Days::new(back), b)
}

/// ARF 210.5 history (days 2-5 and five days prior) and an ARF 210.4 plan.
pub fn extras(cfg: &Config) -> assembly::maturity::Extras {
    assembly::maturity::Extras {
        recent: (1..=4).map(|k| snapshot(cfg, k)).collect(),
        prior: Some(snapshot(cfg, 7)),
        plan: [("9.1.1:m12".to_string(), aud(55_000_000))].into(),
    }
}

struct Gen {
    rng: Rng,
    cfg: Config,
    rd: NaiveDate,
}

impl Gen {
    fn unc<T>(&mut self, v: T) -> Uncertain<T> {
        if self.rng.chance(self.cfg.unknown_pct) { Uncertain::unknown("not supplied upstream") } else { Uncertain::Known(v) }
    }
    fn ubool(&mut self, p_true: u64) -> Uncertain<bool> {
        let v = self.rng.chance(p_true);
        self.unc(v)
    }

    fn retail(&mut self, b: &mut Bank) {
        let n = self.cfg.customers.max(1);
        let groups = (n / 40).max(1) as u64;
        for ci in 0..n {
            let kind = self.rng.weighted(&[
                (80, CustomerKind::NaturalPerson),
                (10, CustomerKind::Sme),
                (4, CustomerKind::Smsf),
                (3, CustomerKind::PersonalInvestmentEntity),
                (2, CustomerKind::NonFinancialCorporate),
                (1, CustomerKind::FinancialInstitution),
            ]);
            let id = format!("cust{ci}");
            let mut c = Customer::new(id.clone(), kind);
            c.established_relationship = self.ubool(60);
            c.managed_as_retail = self.ubool(85);
            c.pie_qualifies = self.ubool(70);
            if kind == CustomerKind::Sme {
                // A few SMEs share a group, so the AUD 2m test is non-local.
                let gid = format!("smeg{}", self.rng.range(0, groups));
                c.sme_group = self.unc(gid);
            }
            b.retail.customers.push(c);
            let big = matches!(kind, CustomerKind::Sme | CustomerKind::NonFinancialCorporate | CustomerKind::FinancialInstitution);
            for ai in 0..self.rng.range(1, 3) {
                let (contract, class) = match self.rng.weighted(&[(50, 0), (15, 1), (35, 2)]) {
                    0 => (Contract::Ump, if self.rng.chance(60) { "TXN" } else { "SAVER" }),
                    1 => (
                        Contract::Clm {
                            notice_days: self.rng.pick(&[31, 32, 60, 90]),
                            grace_period_end: if self.rng.chance(10) { Some(days(self.rd, self.rng.range(1, 7))) } else { None },
                        },
                        "NOTICE",
                    ),
                    _ => {
                        let ew = self.rng.weighted(&[
                            (50, EarlyWithdrawal::NoLegalRight),
                            (35, EarlyWithdrawal::SignificantPenalty),
                            (15, EarlyWithdrawal::Permitted),
                        ]);
                        let early = self.unc(ew);
                        let term = self.rng.pick(&[("TD-3M", 90), ("TD-6M", 180), ("TD-12M", 365), ("TD-24M", 730)]);
                        (Contract::Pam { maturity: days(self.rd, self.rng.range(1, term.1)), early_withdrawal: early }, term.0)
                    }
                };
                let bal = if big { self.rng.dollars(20_000, 1_500_000) } else { self.rng.dollars(500, 400_000) };
                let mut a = DepositAccount::new(format!("{id}-a{ai}"), id.clone(), contract, AudCarrying::new(aud(bal)));
                a.product_class = class.into();
                a.fcs_eligible = self.ubool(97);
                a.transactional = self.ubool(if class == "TXN" { 90 } else { 10 });
                a.online = self.ubool(30);
                a.rate_driven = self.ubool(if class == "SAVER" { 40 } else { 10 });
                if let Contract::Clm { notice_days, .. } = a.contract
                    && self.rng.chance(8)
                {
                    let given = self.rd - Days::new(self.rng.range(0, 10));
                    b.retail.ledger.record(
                        EventKind::NoticeGiven {
                            account_id: a.id.clone(),
                            repayment_date: given + Days::new(notice_days as u64),
                        },
                        given,
                        given,
                    );
                }
                b.retail.accounts.push(a);
            }
        }
        // Term-deposit history: hardship breaks are harmless; one non-hardship
        // break taints its whole product class (Att A para 42).
        for _ in 0..(n / 200).max(1) {
            let on = self.rd - Days::new(self.rng.range(1, 300));
            b.retail.ledger.record(
                EventKind::EarlyWithdrawal { account_id: "hist".into(), product_class: "TD-12M".into(), hardship: true },
                on,
                on,
            );
        }
        if self.rng.chance(50) {
            let on = self.rd - Days::new(self.rng.range(1, 60));
            b.retail.ledger.record(
                EventKind::EarlyWithdrawal { account_id: "hist".into(), product_class: "TD-3M".into(), hardship: false },
                on,
                on + Days::new(1),
            );
        }
    }

    fn wholesale(&mut self, b: &mut Bank, total: u64) {
        use deposits::wholesale::{Customer, Deposit, Kind};
        let n = (self.cfg.customers / 25).max(3);
        let mut sizes = self.rng.split(total, n * 2).into_iter();
        for ci in 0..n {
            let kind = self.rng.weighted(&[
                (55, Kind::NonFinancialCorporate),
                (10, Kind::SovereignCentralBankPseMdb),
                (10, Kind::Bank),
                (15, Kind::OtherFinancialInstitution),
                (10, Kind::OtherLegalEntity),
            ]);
            let cid = format!("wcust{ci}");
            b.wholesale_customers.push(Customer { id: cid.clone(), kind });
            for di in 0..self.rng.range(1, 2) {
                let id = format!("{cid}-d{di}");
                let balance = aud(sizes.next().unwrap_or(0).max(10_000));
                let first = ci == 0 && di == 0;
                let maturity = if !first && self.rng.chance(40) { Some(days(self.rd, self.rng.range(1, 400))) } else { None };
                let within = maturity.is_none_or(|m| m <= days(self.rd, 30));
                // The first deposit is always at call with a measured operational
                // balance, so every seed reaches the three-way split (and ARF 210.6 item 8).
                let op_service = if first { Uncertain::Known(true) } else { self.ubool(35) };
                let op_balance = match &op_service {
                    Uncertain::Known(true) if first || self.rng.chance(80 - self.cfg.unknown_pct.min(80)) => {
                        Uncertain::Known(&balance * Q::new(self.rng.range(20, 90).into(), 100.into()))
                    }
                    Uncertain::Known(true) => Uncertain::unknown("operational balance not measured"),
                    _ => Uncertain::Known(int(0)),
                };
                let fcs = self.ubool(if kind == Kind::NonFinancialCorporate { 20 } else { 0 });
                if self.rng.chance(15) {
                    b.currency.insert(id.clone(), self.rng.pick(&["USD", "NZD"]).into());
                }
                b.wholesale_deposits.push(Deposit {
                    id,
                    customer_id: cid.clone(),
                    balance,
                    within_30_days: within,
                    called: !first && self.rng.chance(3),
                    fcs_eligible: fcs,
                    operational_service: op_service,
                    operational_balance: op_balance,
                    maturity,
                });
            }
        }
    }

    fn holdings(&mut self, b: &mut Bank, target: u64) {
        use secured::{AssetKind as K, ClfCategory as C, Holding, Issuer as I};
        let lots = (self.cfg.customers / 100).max(10);
        // (share %, kind, issuer, risk weight, rating grade, index constituent, CLF category)
        let mix = [
            (1, K::NotesAndCoin, I::CentralBank, 0, 1, false, C::NotClfEligible),
            (12, K::RbaBalance, I::CentralBank, 0, 1, false, C::NotClfEligible),
            (42, K::DebtSecurity, I::AustralianGovernment, 0, 1, false, C::NotClfEligible),
            (20, K::DebtSecurity, I::StateCentralBorrowingAuthority, 0, 1, false, C::NotClfEligible),
            (3, K::DebtSecurity, I::Supranational, 0, 1, false, C::SupranationalOrForeignGovernment),
            (8, K::CoveredBond, I::FinancialInstitutionIssuer, 20, 1, false, C::AdiIssued),
            (4, K::Rmbs, I::FinancialInstitutionIssuer, 20, 1, false, C::AssetBacked),
            (4, K::DebtSecurity, I::NonFinancialCorporateIssuer, 50, 2, false, C::OtherPrivate),
            (2, K::Equity, I::NonFinancialCorporateIssuer, 100, 3, true, C::NotClfEligible),
            (4, K::Rmbs, I::FinancialInstitutionIssuer, 35, 1, false, C::SelfSecuritised),
        ];
        let total_share: u64 = mix.iter().map(|m| m.0).sum();
        b.has_clf = true;
        for (k, (share, kind, issuer, rw, grade, idx, clf)) in mix.into_iter().enumerate() {
            let pieces = if matches!(kind, K::NotesAndCoin | K::RbaBalance) { 1 } else { (lots / 10).max(1) };
            let each = target * share / total_share / pieces as u64;
            for p in 0..pieces {
                let id = format!("hold{k}-{p}");
                let mv = aud(self.rng.range(each * 8 / 10, each * 12 / 10).max(1));
                let rba = if clf == C::NotClfEligible { mv.clone() } else { &mv * Q::new(90.into(), 100.into()) };
                let maturity = if matches!(kind, K::NotesAndCoin | K::RbaBalance | K::Equity) {
                    None
                } else {
                    Some(days(self.rd, self.rng.range(20, 3650)))
                };
                let operational = self.ubool(95);
                let drawable = self.ubool(98);
                b.holdings.push(Holding {
                    id,
                    kind,
                    issuer,
                    market_value: mv.round(),
                    risk_weight: rw,
                    rating_grade: grade,
                    domestic_currency: true,
                    index_constituent: idx,
                    own_or_associated: clf == C::SelfSecuritised,
                    encumbered: self.rng.chance(5),
                    operational,
                    drawable_in_stress: drawable,
                    clf_category: clf,
                    value_after_rba_margin: rba.round(),
                    maturity,
                });
            }
        }
    }

    fn repos(&mut self, b: &mut Bank, unit: u64) {
        use secured::{Counterparty, HqlaClass, Leg, Repo};
        for k in 0..(self.cfg.customers / 200).max(4) {
            let leg = if k % 2 == 0 { Leg::Funding } else { Leg::Lending };
            let cash = aud(self.rng.dollars(unit / 400 + 1, unit / 40 + 2));
            let class = self.rng.weighted(&[(60, HqlaClass::Hqla1), (25, HqlaClass::Hqla2a), (10, HqlaClass::RmbsHqla2b), (5, HqlaClass::NonHqla)]);
            let collateral = (&cash * Q::new(self.rng.range(102, 110).into(), 100.into())).round();
            let operational = self.ubool(90);
            b.repos.push(Repo {
                id: format!("repo{k}"),
                leg,
                cash,
                collateral_class: class,
                collateral_value: collateral,
                counterparty: self.rng.weighted(&[(85, Counterparty::Other), (15, Counterparty::DomesticSovereignOrPse)]),
                operational,
                rehypothecated: leg == Leg::Lending && self.rng.chance(30),
                maturity: days(self.rd, self.rng.range(1, 90)),
            });
        }
    }

    fn loans(&mut self, b: &mut Bank, total: u64) {
        use loans::{Affiliation, Borrower, Contract as L, Loan, Product, Receivable};
        let n = (self.cfg.customers / 2).max(4);
        // Mean balance such that the expected book is `total` (kind mix and ranges below).
        let avg = total * 100 / (n as u64 * 155);
        for k in 0..n {
            let id = format!("loan{k}");
            let kind = self.rng.weighted(&[(65, 0), (15, 1), (10, 2), (10, 3)]);
            let (borrower, product, balance, contract, cycle) = match kind {
                0 => {
                    let bal = self.rng.range(avg / 3, avg * 2).max(10_000);
                    // Level monthly payment for 25 years at 6% (data, so floats are fine here).
                    let i = 0.06 / 12.0;
                    let pay = bal as f64 * i / (1.0 - (1.0 + i).powi(-300));
                    (Borrower::RetailCustomer, Product::ResidentialMortgage, bal, L::Ann { payment: aud(pay.ceil() as u64) }, 1)
                }
                1 => {
                    let bal = self.rng.range(avg / 4, avg).max(10_000);
                    (Borrower::Sme, Product::OtherLoan, bal, L::Lam { principal_per_period: aud(bal / 20) }, 3)
                }
                2 => {
                    let bal = self.rng.range(avg, avg * 5).max(10_000);
                    let borrower = self.rng.weighted(&[(80, Borrower::NonFinancialCorporate), (20, Borrower::FinancialInstitution)]);
                    (borrower, Product::OtherLoan, bal, L::Pam { maturity: days(self.rd, self.rng.range(5, 1800)) }, 3)
                }
                _ => {
                    let bal = self.rng.range(1_000, 30_000);
                    (Borrower::RetailCustomer, Product::OtherLoan, bal, L::Ump { minimum_payment: aud(bal / 50 + 25) }, 1)
                }
            };
            let performing = self.ubool(98);
            let lvr = self.ubool(80);
            b.loans.push(Loan {
                id: id.clone(),
                borrower,
                affiliation: if kind == 2 && self.rng.chance(5) { Affiliation::IntraGroupOther } else { Affiliation::External },
                product,
                contract,
                balance: aud(balance),
                rate: Q::new(6.into(), 100.into()),
                next_payment: days(self.rd, self.rng.range(1, 28)),
                cycle_months: cycle,
                performing,
                lvr_at_most_80: lvr,
                risk_weight: if product == Product::ResidentialMortgage { 35 } else { 100 },
                encumbered_until: if product == Product::ResidentialMortgage && self.rng.chance(10) {
                    Some(days(self.rd, 400))
                } else {
                    None
                },
            });
            if kind == 2 && self.rng.chance(20) {
                b.currency.insert(id, "USD".into());
            }
        }
        for k in 0..(n / 50).max(2) {
            let performing = self.ubool(97);
            b.other_inflows.receivables.push(Receivable {
                id: format!("recv{k}"),
                amount: aud(self.rng.dollars(10_000, avg.max(10_001))),
                due: days(self.rd, self.rng.range(1, 45)),
                non_financial_revenue: self.rng.chance(20),
                contingent: self.rng.chance(10),
                performing,
            });
        }
    }

    fn commitments(&mut self, b: &mut Bank, unit: u64) {
        use obligations::{ContingentObligation, Counterparty as C, Facility, FacilityKind, LendingCommitment, ObligationKind as O};
        let cps = [
            (40, C::RetailCustomer),
            (15, C::Sme),
            (25, C::NonFinancialCorporate),
            (5, C::SovereignCentralBankPseMdb),
            (5, C::RegulatedBank),
            (7, C::OtherFinancialInstitution),
            (3, C::OtherLegalEntity),
        ];
        let n = (self.cfg.customers / 20).max(4);
        let limits = self.rng.split(unit * 20 / 100, n);
        for (k, limit) in limits.into_iter().enumerate() {
            let counterparty = self.rng.weighted(&cps);
            let limit = limit.max(10_000);
            let undrawn = self.rng.range(limit / 10, limit);
            let drawable = self.ubool(95);
            b.commitments.facilities.push(Facility {
                id: format!("fac{k}"),
                counterparty,
                kind: if self.rng.chance(20) { FacilityKind::LiquidityFacility } else { FacilityKind::CreditFacility },
                committed: self.rng.chance(75),
                undrawn: aud(undrawn),
                drawable_within_30_days: drawable,
                mortgage_redraw: counterparty == C::RetailCustomer && self.rng.chance(50),
                funds_vehicle: counterparty == C::OtherFinancialInstitution && self.rng.chance(30),
                limit: Some(aud(limit)),
            });
        }
        for k in 0..(self.cfg.customers / 100).max(2) {
            b.commitments.commitments.push(LendingCommitment {
                id: format!("commit{k}"),
                counterparty: self.rng.weighted(&cps),
                amount: aud(self.rng.dollars(unit / 2000 + 1, unit / 200 + 2)),
            });
        }
        let kinds = [O::TradeFinance, O::Guarantee, O::ShortTermDebtBuyback, O::LongTermDebtBuyback, O::OtherContractual];
        for (k, kind) in kinds.into_iter().enumerate() {
            b.commitments.obligations.push(ContingentObligation {
                id: format!("oblig{k}"),
                kind,
                amount: aud(self.rng.dollars(unit / 1000 + 1, unit / 100 + 2)),
            });
        }
    }

    fn derivatives(&mut self, b: &mut Bank, unit: u64) {
        use derivatives::{CollateralFlow, Flow, NettingSet, Trade};
        let scale = (unit / 100).max(100_000);
        for k in 0..(self.cfg.customers / 500).max(3) {
            let set = format!("ns{k}");
            let mna = self.rng.chance(85);
            let posted = aud(self.rng.range(0, scale / 4));
            b.derivatives.sets.push(NettingSet {
                id: set.clone(),
                counterparty: format!("dealer{k}"),
                netting_agreement: Some(mna),
                collateral_received: aud(self.rng.range(0, scale / 4)),
                due_not_called: aud(self.rng.range(0, scale / 50)),
                downgrade_3_notches: aud(self.rng.range(0, scale / 10)),
                variation_margin_posted: posted.clone(),
                initial_margin_posted: aud(self.rng.range(0, scale / 20)),
                ..Default::default()
            });
            for _ in 0..self.rng.range(1, 4) {
                let v = self.rng.range(0, scale) as i64 - (scale / 2) as i64;
                b.derivatives.trades.push(Trade { set: set.clone(), mtm: int(v) });
            }
            // Interest rate swap settlements within 30 days.
            for _ in 0..self.rng.range(1, 3) {
                let v = self.rng.range(0, scale / 10) as i64 - (scale / 20) as i64;
                b.derivatives.flows.push(Flow {
                    set: set.clone(),
                    date: days(self.rd, self.rng.range(1, 30)),
                    amount: int(v),
                    currency: "AUD".into(),
                    fx_principal: false,
                    fx_trading: false,
                    option: false,
                    in_the_money: Uncertain::Known(true),
                });
            }
            // An FX swap: pay AUD, receive USD on the same day.
            if self.rng.chance(60) {
                let d = days(self.rd, self.rng.range(1, 30));
                let aud_leg = self.rng.range(scale / 10, scale) as i64;
                let trading = self.rng.chance(30);
                for (ccy, amt) in [("AUD", -aud_leg), ("USD", aud_leg + aud_leg / 50)] {
                    b.derivatives.flows.push(Flow {
                        set: set.clone(),
                        date: d,
                        amount: int(amt),
                        currency: ccy.into(),
                        fx_principal: true,
                        fx_trading: trading,
                        option: false,
                        in_the_money: Uncertain::Known(true),
                    });
                }
            }
            // A bought option, exercise unknown if data is missing.
            if self.rng.chance(30) {
                let itm = self.ubool(50);
                b.derivatives.flows.push(Flow {
                    set: set.clone(),
                    date: days(self.rd, self.rng.range(1, 30)),
                    amount: int(self.rng.range(1, scale / 20) as i64),
                    currency: "AUD".into(),
                    fx_principal: false,
                    fx_trading: false,
                    option: true,
                    in_the_money: itm,
                });
            }
            // 24 months of realised collateral flows (bitemporal: some recorded late).
            if mna {
                for m in 0..24u64 {
                    let valid = self.rd - Days::new(m * 30 + self.rng.range(0, 20));
                    let lag = if self.rng.chance(10) { self.rng.range(1, 5) } else { 0 };
                    b.derivatives.collateral_flows.push(CollateralFlow {
                        set: set.clone(),
                        amount: int(self.rng.range(0, scale / 5) as i64 - (scale / 10) as i64),
                        currency: "AUD".into(),
                        valid,
                        recorded: valid + Days::new(lag),
                    });
                }
            }
        }
    }

    fn funding(&mut self, b: &mut Bank, unit: u64) {
        use funding::{AdiType, CapitalInstrument, EarlyRepaymentRequest, IssuedSecurity, Placement, SecurityKind as S, Tier};
        let n = (self.cfg.customers / 200).max(4);
        let sizes = self.rng.split(unit * 27 / 100, n);
        for (k, size) in sizes.into_iter().enumerate() {
            let kind = self.rng.weighted(&[(25, S::ShortTerm), (10, S::EuroCommercialPaper), (40, S::LongTerm), (15, S::CoveredBond), (10, S::LongTermSecuritisation)]);
            let short = matches!(kind, S::ShortTerm | S::EuroCommercialPaper);
            let maturity = days(self.rd, if short { self.rng.range(1, 365) } else { self.rng.range(400, 3650) });
            let id = format!("issue{k}");
            b.funding.securities.push(IssuedSecurity {
                id: id.clone(),
                kind,
                amount: aud(size.max(100_000)),
                maturity,
                offshore: kind == S::EuroCommercialPaper,
            });
            if short && self.rng.chance(15) {
                b.funding.early_repayment_requests.push(EarlyRepaymentRequest {
                    security: id,
                    requested: self.rd - Days::new(self.rng.range(0, 4)),
                    amount: aud(self.rng.dollars(100_000, 2_000_000)),
                    approved: self.rng.chance(50),
                });
            }
        }
        b.funding.capital = vec![
            CapitalInstrument { id: "cet1".into(), tier: Tier::CET1, amount: aud(unit / 9), maturity: None },
            CapitalInstrument { id: "at1".into(), tier: Tier::AT1, amount: aud(unit / 60), maturity: None },
            CapitalInstrument {
                id: "t2".into(),
                tier: Tier::T2,
                amount: aud(unit / 40),
                maturity: Some(days(self.rd, self.rng.range(20, 2500))),
            },
        ];
        for k in 0..(n / 2).max(2) {
            let operational = self.ubool(30);
            b.funding.placements.push(Placement {
                id: format!("place{k}"),
                adi: self.rng.pick(&[AdiType::MajorBank, AdiType::LcrAdi, AdiType::OtherAdi, AdiType::ForeignAdi]),
                amount: aud(self.rng.dollars(unit / 1000 + 1, unit / 100 + 2)),
                maturity: if self.rng.chance(50) { Some(days(self.rd, self.rng.range(1, 180))) } else { None },
                operational,
            });
        }
    }
}
