//! Versioned, model-risk-governed behavioural model interfaces (APS 210 Att A §32;
//! design.txt §4.C.3).
//!
//! Cleanly separates behavioural/statistical models from statutory rules:
//! - `ModelMetadata`: mandatory version, approver, effective & review-due dates
//!   (expired models are rejected at `reporting_date`).
//! - `OperationalDepositModel`: estimates operational deposit balance & excess
//!   from historical daily balance time series (APG 210 §135).
//! - `PrepaymentModel`: transforms contractual mortgage cashflows across maturity
//!   buckets using a CPR/SMM prepayment curve while conserving total principal
//!   exactly (ARF 210.3.1 item 10.5).

use chrono::NaiveDate;
use prudence_core::Q;

/// Governance metadata required for every behavioural model (APS 210 Att A §32).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelMetadata {
    pub model_id: String,
    pub version: String,
    pub approved_by: String,
    pub effective_from: NaiveDate,
    pub review_due: NaiveDate,
}

impl ModelMetadata {
    pub fn validate(&self, reporting_date: NaiveDate) -> Result<(), String> {
        if self.model_id.trim().is_empty() || self.approved_by.trim().is_empty() {
            return Err("behavioural model missing model_id or approver".into());
        }
        if reporting_date < self.effective_from {
            return Err(format!(
                "behavioural model `{}` v{} not effective until {}",
                self.model_id, self.version, self.effective_from
            ));
        }
        if reporting_date > self.review_due {
            return Err(format!(
                "behavioural model `{}` v{} review expired on {}",
                self.model_id, self.version, self.review_due
            ));
        }
        Ok(())
    }
}

/// Behavioural model estimating operational vs. excess deposit balances (APG 210 §135).
pub trait OperationalDepositModel {
    fn metadata(&self) -> &ModelMetadata;

    /// Given `current_balance` and historical daily EOD balances, returns
    /// `(operational_balance, excess_balance)` satisfying
    /// `operational + excess == current_balance`, both `>= 0`.
    fn estimate(&self, current_balance: &Q, daily_history: &[Q], reporting_date: NaiveDate) -> Result<(Q, Q), String>;
}

/// Conservative rolling-minimum operational floor model:
/// takes the `k`-th lowest historical daily balance (e.g. 5th percentile),
/// capped at `current_balance`. Empty history -> 0 operational (all excess).
#[derive(Clone, Debug)]
pub struct RollingFloorOperationalModel {
    pub meta: ModelMetadata,
    /// Percentile in `(0, 100]`, e.g. `5` for the 5th-percentile floor.
    pub percentile: usize,
}

impl OperationalDepositModel for RollingFloorOperationalModel {
    fn metadata(&self) -> &ModelMetadata {
        &self.meta
    }

    fn estimate(&self, current_balance: &Q, daily_history: &[Q], reporting_date: NaiveDate) -> Result<(Q, Q), String> {
        self.meta.validate(reporting_date)?;
        let zero = Q::default();
        let cur = current_balance.clone().max(zero.clone());
        if daily_history.is_empty() {
            return Ok((zero, cur));
        }
        let mut sorted: Vec<Q> = daily_history.iter().map(|q| q.clone().max(zero.clone())).collect();
        sorted.sort();
        let idx = ((sorted.len().saturating_sub(1)) * self.percentile) / 100;
        let op = sorted[idx].clone().min(cur.clone());
        let excess = &cur - &op;
        Ok((op, excess))
    }
}

/// Behavioural mortgage prepayment model for ARF 210.3.1 item 10.5.
pub trait PrepaymentModel {
    fn metadata(&self) -> &ModelMetadata;

    /// Apply prepayment assumptions to contractual principal cashflows across
    /// ordered time buckets, returning behavioural cashflows with exact
    /// principal conservation (`sum(out) == sum(contractual)`).
    fn apply_prepayment(&self, contractual_buckets: &[Q], reporting_date: NaiveDate) -> Result<Vec<Q>, String>;
}

/// Constant per-bucket prepayment rate model (exact rational SMM):
/// in each bucket `i` (before the final bucket), a fraction `smm_per_bucket` of
/// remaining future principal prepays into bucket `i`, conserving total principal.
#[derive(Clone, Debug)]
pub struct ConstantPrepaymentModel {
    pub meta: ModelMetadata,
    /// Fraction of remaining future principal prepaid per bucket (`0 <= smm <= 1`).
    pub smm_per_bucket: Q,
}

impl PrepaymentModel for ConstantPrepaymentModel {
    fn metadata(&self) -> &ModelMetadata {
        &self.meta
    }

    fn apply_prepayment(&self, contractual_buckets: &[Q], reporting_date: NaiveDate) -> Result<Vec<Q>, String> {
        self.meta.validate(reporting_date)?;
        let zero = Q::default();
        let one = Q::from_integer(1.into());
        if self.smm_per_bucket < zero || self.smm_per_bucket > one {
            return Err("smm_per_bucket must be in [0, 1]".into());
        }
        let n = contractual_buckets.len();
        if n == 0 {
            return Ok(vec![]);
        }
        let mut remaining: Vec<Q> = contractual_buckets.to_vec();
        let mut out = vec![zero.clone(); n];
        for i in 0..n {
            // Contractual cashflow due in bucket i
            out[i] += remaining[i].clone();
            remaining[i] = zero.clone();
            // Prepay `smm` of every future bucket `j > i` into bucket `i`
            if i + 1 < n {
                for rem_j in remaining.iter_mut().skip(i + 1) {
                    let prepay = &*rem_j * &self.smm_per_bucket;
                    *rem_j -= &prepay;
                    out[i] += prepay;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prudence_core::num::int;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn meta() -> ModelMetadata {
        ModelMetadata {
            model_id: "op-dep-floor".into(),
            version: "2.1".into(),
            approved_by: "ALCO".into(),
            effective_from: d(2026, 1, 1),
            review_due: d(2026, 12, 31),
        }
    }

    #[test]
    fn operational_model_conserves_balance_and_rejects_expired_model() {
        let m = RollingFloorOperationalModel { meta: meta(), percentile: 5 };
        let hist = vec![int(100), int(80), int(120), int(90), int(110)];
        let (op, ex) = m.estimate(&int(105), &hist, d(2026, 9, 30)).unwrap();
        assert_eq!(op, int(80));
        assert_eq!(ex, int(25));
        assert_eq!(&op + &ex, int(105));

        // Expired model is rejected
        assert!(m.estimate(&int(105), &hist, d(2027, 1, 1)).unwrap_err().contains("expired"));
    }

    #[test]
    fn prepayment_model_accelerates_cashflows_and_conserves_principal_exactly() {
        let m = ConstantPrepaymentModel {
            meta: meta(),
            smm_per_bucket: Q::new(1.into(), 10.into()), // 10% per bucket
        };
        let contractual = vec![int(10), int(20), int(70)];
        let beh = m.apply_prepayment(&contractual, d(2026, 9, 30)).unwrap();
        let total_before: Q = contractual.iter().cloned().sum();
        let total_after: Q = beh.iter().cloned().sum();
        assert_eq!(total_before, total_after);
        // Bucket 0 gets contractual 10 + 10% of (20 + 70) = 19
        assert_eq!(beh[0], int(19));
        assert!(beh[2] < contractual[2]);
    }
}
