//! Canonical deposit data.
//!
//! Contracts follow ACTUS contract types (actusfrf.org): the economics of a
//! deposit are fully described by its type and terms. Lifecycle facts are an
//! append-only, bitemporal event log in the style of the FINOS CDM event model:
//! each event has the date it took effect (`valid`) and the date the bank
//! learned of it (`recorded`), so any past report can be replayed exactly as it
//! was known at the time.
//!
//! Regulatory attributes that upstream systems may not supply are
//! `Uncertain<T>`; the classifier resolves unknowns conservatively.

use chrono::NaiveDate;
use prudence_core::money::AudCarrying;
use prudence_core::uncertain::Uncertain;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CustomerKind {
    /// Retail: APS 210 Att A para 34.
    NaturalPerson,
    /// Self-managed super fund: retail, always less stable (APG 210 para 118).
    Smsf,
    /// Personal investment entity, e.g. a family trust: retail and less stable
    /// if it qualifies under APG 210 para 119, otherwise a financial institution.
    PersonalInvestmentEntity,
    /// Small or medium enterprise: retail treatment possible (Att A para 46).
    Sme,
    NonFinancialCorporate,
    FinancialInstitution,
}

#[derive(Clone, Debug)]
pub struct Customer {
    pub id: String,
    pub kind: CustomerKind,
    /// Other established relationships making withdrawal highly unlikely
    /// (Att A para 37(a); APG 210 para 111). Conservative: false.
    pub established_relationship: Uncertain<bool>,
    /// SME: consolidation group for the AUD 2m test (Att A para 46(b)).
    /// Conservative when unknown: the group total cannot be shown to be
    /// below the threshold, so retail treatment is denied.
    pub sme_group: Uncertain<String>,
    /// SME: managed as a retail exposure with retail-like liquidity risk
    /// (Att A para 46(a); APG 210 paras 129-130). Conservative: false.
    pub managed_as_retail: Uncertain<bool>,
    /// PIE: a small number of related individuals, family-operated, solely for
    /// their benefit (APG 210 para 119). Conservative: false.
    pub pie_qualifies: Uncertain<bool>,
}

impl Customer {
    pub fn new(id: impl Into<String>, kind: CustomerKind) -> Self {
        Customer {
            id: id.into(),
            kind,
            established_relationship: Uncertain::unknown("not supplied"),
            sme_group: Uncertain::unknown("not supplied"),
            managed_as_retail: Uncertain::unknown("not supplied"),
            pie_qualifies: Uncertain::unknown("not supplied"),
        }
    }
}

/// Early-withdrawal terms of a term deposit (Att A para 41).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EarlyWithdrawal {
    /// No legal right to withdraw within the term.
    NoLegalRight,
    /// Withdrawal possible but with a penalty materially greater than loss of interest.
    SignificantPenalty,
    /// Withdrawal possible without a significant penalty.
    Permitted,
}

/// ACTUS contract types used for deposits.
#[derive(Clone, Debug, PartialEq)]
pub enum Contract {
    /// UMP, Undefined Maturity Profile: at-call / transaction / savings.
    Ump,
    /// CLM, Call Money: repayable a fixed notice period after being called
    /// (e.g. 31-day notice saver, APG 210 paras 120-122).
    Clm {
        notice_days: u32,
        /// End of a post-rollover grace period during which the depositor can
        /// withdraw freely (APG 210 para 122).
        grace_period_end: Option<NaiveDate>,
    },
    /// PAM, Principal At Maturity: term deposit.
    Pam { maturity: NaiveDate, early_withdrawal: Uncertain<EarlyWithdrawal> },
}

#[derive(Clone, Debug)]
pub struct DepositAccount {
    pub id: String,
    pub customer_id: String,
    /// Product category, the unit of term-deposit tainting (Att A para 42).
    pub product_class: String,
    pub contract: Contract,
    pub balance: AudCarrying,
    /// Covered by the Financial Claims Scheme. Conservative: false.
    pub fcs_eligible: Uncertain<bool>,
    /// Used for day-to-day banking (Att A para 37(b); APG 210 para 112). Conservative: false.
    pub transactional: Uncertain<bool>,
    /// Internet integral to design, marketing and use (APG 210 para 116). Conservative: true.
    pub online: Uncertain<bool>,
    /// Heavily rate-driven / promotional (APG 210 para 117). Conservative: true.
    pub rate_driven: Uncertain<bool>,
}

impl DepositAccount {
    pub fn new(id: impl Into<String>, customer_id: impl Into<String>, contract: Contract, balance: AudCarrying) -> Self {
        let u = || Uncertain::unknown("not supplied");
        DepositAccount {
            id: id.into(),
            customer_id: customer_id.into(),
            product_class: "default".into(),
            contract,
            balance,
            fcs_eligible: u(),
            transactional: u(),
            online: u(),
            rate_driven: u(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// Depositor gave notice on a call-money (CLM) account.
    NoticeGiven { account_id: String, repayment_date: NaiveDate },
    /// A term deposit in `product_class` was broken early.
    EarlyWithdrawal { account_id: String, product_class: String, hardship: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    /// When it took effect.
    pub valid: NaiveDate,
    /// When the bank recorded it.
    pub recorded: NaiveDate,
}

/// Append-only bitemporal event log.
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    events: Vec<Event>,
}

impl Ledger {
    /// `recorded` may precede `valid` (e.g. a notice logged before it takes effect).
    pub fn record(&mut self, kind: EventKind, valid: NaiveDate, recorded: NaiveDate) {
        self.events.push(Event { kind, valid, recorded });
    }

    /// Events in effect on `valid_at`, as known on `known_at`.
    pub fn as_of(&self, valid_at: NaiveDate, known_at: NaiveDate) -> impl Iterator<Item = &Event> {
        self.events.iter().filter(move |e| e.valid <= valid_at && e.recorded <= known_at)
    }
}

/// Everything the classifier needs for one reporting date.
#[derive(Clone, Debug, Default)]
pub struct Book {
    pub customers: Vec<Customer>,
    pub accounts: Vec<DepositAccount>,
    pub ledger: Ledger,
}
