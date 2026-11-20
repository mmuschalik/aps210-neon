//! Deterministic pseudo-random books for property and differential tests.

use chrono::{Days, NaiveDate};
use num_traits::FromPrimitive;
use prudence_core::Q;
use prudence_core::money::AudCarrying;
use prudence_core::uncertain::Uncertain;

use crate::model::*;

pub struct Gen(u64);

impl Gen {
    pub fn new(seed: u64) -> Self {
        Gen(seed)
    }
    pub fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
    fn tri(&mut self) -> Uncertain<bool> {
        match self.next(3) {
            0 => Uncertain::unknown("x"),
            1 => Uncertain::Known(true),
            _ => Uncertain::Known(false),
        }
    }

    /// A book with up to 6 customers of mixed kinds and up to 4 deposits each,
    /// plus notice and early-withdrawal events around `rd`.
    pub fn book(&mut self, rd: NaiveDate) -> Book {
        let mut b = Book::default();
        for ci in 0..(1 + self.next(6)) {
            let kind = [
                CustomerKind::NaturalPerson,
                CustomerKind::Smsf,
                CustomerKind::PersonalInvestmentEntity,
                CustomerKind::Sme,
                CustomerKind::NonFinancialCorporate,
                CustomerKind::FinancialInstitution,
            ][self.next(6) as usize];
            let mut c = Customer::new(format!("c{ci}"), kind);
            c.established_relationship = self.tri();
            c.managed_as_retail = self.tri();
            c.pie_qualifies = self.tri();
            if self.next(3) > 0 {
                c.sme_group = format!("g{}", self.next(2)).into();
            }
            for ai in 0..(1 + self.next(4)) {
                let contract = match self.next(3) {
                    0 => Contract::Ump,
                    1 => Contract::Clm {
                        notice_days: [7, 31, 60][self.next(3) as usize],
                        grace_period_end: if self.next(4) == 0 { Some(rd + Days::new(self.next(5))) } else { None },
                    },
                    _ => Contract::Pam {
                        maturity: rd + Days::new(self.next(800)),
                        early_withdrawal: [
                            Uncertain::unknown("x"),
                            EarlyWithdrawal::NoLegalRight.into(),
                            EarlyWithdrawal::SignificantPenalty.into(),
                            EarlyWithdrawal::Permitted.into(),
                        ][self.next(4) as usize]
                            .clone(),
                    },
                };
                let bal = AudCarrying::new(Q::from_u64(self.next(3_000_000)).unwrap());
                let mut a = DepositAccount::new(format!("c{ci}a{ai}"), format!("c{ci}"), contract, bal);
                a.product_class = ["TD-A", "TD-B"][self.next(2) as usize].into();
                a.fcs_eligible = self.tri();
                a.transactional = self.tri();
                a.online = self.tri();
                a.rate_driven = self.tri();
                b.accounts.push(a);
            }
            b.customers.push(c);
        }
        for _ in 0..self.next(3) {
            let ids: Vec<String> = b.accounts.iter().map(|a| a.id.clone()).collect();
            let valid = rd - Days::new(self.next(20)) + Days::new(self.next(5));
            let recorded = valid + Days::new(self.next(10));
            if self.next(2) == 0 {
                let acc = ids[self.next(ids.len() as u64) as usize].clone();
                b.ledger.record(EventKind::NoticeGiven { account_id: acc, repayment_date: rd + Days::new(self.next(60)) }, valid, recorded);
            } else {
                b.ledger.record(
                    EventKind::EarlyWithdrawal {
                        account_id: "x".into(),
                        product_class: ["TD-A", "TD-B"][self.next(2) as usize].into(),
                        hardship: self.next(2) == 0,
                    },
                    valid,
                    recorded,
                );
            }
        }
        b
    }
}
