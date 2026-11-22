//! Additional APS 210 statutory rules completing full regulatory coverage:
//! - **Item 4**: NSFR posted variation margin no-double-count (APS 210 Att C fn 12)
//! - **Item 5**: NSFR CLF/ALA 10% RSF quota spillover into 65%/85% RSF (APS 210 Att C §32)
//! - **Item 6**: Interdependent assets & liabilities (0% ASF / 0% RSF, APS 210 Att C §37)
//! - **Item 8**: Continuous daily quarterly compliance stats (min / max / mean / breaches, APS 210 §12)

use chrono::NaiveDate;
use prudence_core::Q;

// ------------------------------------------------------------------ Item 4: Posted VM (Att C fn 12)

/// Collateral posted as variation margin that is deducted from NSFR derivative
/// liabilities (ARF 210.6 item 13.1.2) and must therefore be removed from
/// on-balance-sheet RSF assets (APS 210 Att C fn 12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostedVariationMargin {
    pub netting_set_id: String,
    pub asset_record_id: String,
    /// The ARF 210.6 RSF cell (e.g. `"25:b0"`, `"35.1:b2"`) from which to remove the posted VM.
    pub rsf_cell: String,
    /// Carrying amount in AUD.
    pub amount: Q,
}

/// Verify that total posted VM removed from RSF assets matches the VM deducted
/// in ARF 210.6 item 13.1.2 (no double counting).
pub fn verify_posted_vm_conservation(posted: &[PostedVariationMargin], total_vm_13_1_2_aud: &Q) -> Result<(), String> {
    let removed: Q = posted.iter().map(|p| p.amount.clone()).sum();
    if &removed > total_vm_13_1_2_aud {
        return Err(format!(
            "posted VM removed from RSF ({removed}) exceeds derivative VM deducted in 13.1.2 ({total_vm_13_1_2_aud})"
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------ Item 5: CLF 10% RSF quota spillover (Att C §32)

/// Fallback RSF bucket for CLF/ALA assets exceeding the LCR CLF cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClfSpilloverBucket {
    /// 65% RSF (e.g. qualifying residential mortgages / self-securitised RMBS).
    Rsf65,
    /// 85% RSF (other non-HQLA securities).
    Rsf85,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClfAsset {
    pub id: String,
    /// Carrying value in AUD.
    pub carrying_value: Q,
    pub fallback: ClfSpilloverBucket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClfRsfSpillover {
    pub quota_cap: Q,
    /// Amount allocated to 10% RSF (ARF 210.6 items 26/27).
    pub eligible_10pct: Q,
    /// Excess spilling into 65% RSF.
    pub spillover_65pct: Q,
    /// Excess spilling into 85% RSF.
    pub spillover_85pct: Q,
}

/// Allocate CLF assets up to `lcr_clf_cap` into the 10% RSF quota (preferring
/// 85%-fallback assets first to minimize total RSF), spilling any excess into
/// 65% or 85% RSF while preserving exact conservation.
pub fn apply_clf_rsf_quota(assets: &[ClfAsset], lcr_clf_cap: &Q) -> ClfRsfSpillover {
    let zero = Q::default();
    let mut rem_cap = lcr_clf_cap.clone().max(zero.clone());
    let mut eligible_10pct = zero.clone();
    let mut spillover_65pct = zero.clone();
    let mut spillover_85pct = zero.clone();

    // Put 85%-fallback assets into the 10% quota first (saves 75% RSF vs 55%).
    let mut ordered: Vec<&ClfAsset> = assets.iter().collect();
    ordered.sort_by_key(|a| match a.fallback {
        ClfSpilloverBucket::Rsf85 => 0u8,
        ClfSpilloverBucket::Rsf65 => 1u8,
    });

    for a in ordered {
        let amt = a.carrying_value.clone().max(zero.clone());
        let in_quota = amt.clone().min(rem_cap.clone());
        rem_cap -= &in_quota;
        eligible_10pct += &in_quota;
        let excess = &amt - &in_quota;
        match a.fallback {
            ClfSpilloverBucket::Rsf65 => spillover_65pct += excess,
            ClfSpilloverBucket::Rsf85 => spillover_85pct += excess,
        }
    }
    ClfRsfSpillover {
        quota_cap: lcr_clf_cap.clone().max(zero),
        eligible_10pct,
        spillover_65pct,
        spillover_85pct,
    }
}

// ------------------------------------------------------------------ Item 6: Interdependent assets & liabilities (Att C §37)

/// Matched asset/liability pair qualifying for 0% ASF (item 16) and 0% RSF (item 45)
/// under APS 210 Attachment C §37.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterdependentPair {
    pub id: String,
    pub asset_record_id: String,
    pub liability_record_id: String,
    /// Matched carrying amount in AUD.
    pub amount: Q,
    pub asset_maturity: NaiveDate,
    pub liability_maturity: NaiveDate,
    pub pass_through_cashflows: bool,
    pub apra_approved: bool,
}

/// Validate APS 210 Att C §37 interdependence criteria on every pair.
pub fn validate_interdependent_pairs(pairs: &[InterdependentPair]) -> Result<(), String> {
    let zero = Q::default();
    for p in pairs {
        if p.amount <= zero {
            return Err(format!("interdependent pair `{}` must have positive amount", p.id));
        }
        if p.asset_maturity != p.liability_maturity {
            return Err(format!(
                "interdependent pair `{}` has mismatched maturities ({} vs {})",
                p.id, p.asset_maturity, p.liability_maturity
            ));
        }
        if !p.pass_through_cashflows {
            return Err(format!("interdependent pair `{}` lacks pass-through cashflows (Att C §37)", p.id));
        }
        if !p.apra_approved {
            return Err(format!("interdependent pair `{}` lacks APRA approval (Att C §37)", p.id));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ Item 8: Quarterly daily compliance stats (APS 210 §12)

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DailyObservation {
    pub date: NaiveDate,
    /// Ratio as a percentage (e.g. `115` for 115% LCR, or `12` for 12% MLH).
    pub ratio_pct: Q,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarterlyComplianceStats {
    pub count: usize,
    pub min_ratio_pct: Q,
    pub min_date: NaiveDate,
    pub max_ratio_pct: Q,
    pub max_date: NaiveDate,
    pub mean_ratio_pct: Q,
    pub breaches: Vec<DailyObservation>,
}

/// Exact-rational quarterly daily compliance statistics (APS 210 §12).
/// `floor_pct`: `100` for LCR, `9` for MLH.
pub fn quarterly_compliance_stats(obs: &[DailyObservation], floor_pct: &Q) -> Result<QuarterlyComplianceStats, String> {
    if obs.is_empty() {
        return Err("no daily observations supplied".into());
    }
    let mut min_o = &obs[0];
    let mut max_o = &obs[0];
    let mut sum = Q::default();
    let mut breaches = Vec::new();
    for o in obs {
        if o.ratio_pct < min_o.ratio_pct {
            min_o = o;
        }
        if o.ratio_pct > max_o.ratio_pct {
            max_o = o;
        }
        sum += o.ratio_pct.clone();
        if &o.ratio_pct < floor_pct {
            breaches.push(o.clone());
        }
    }
    let mean = sum / Q::from_integer((obs.len() as i64).into());
    Ok(QuarterlyComplianceStats {
        count: obs.len(),
        min_ratio_pct: min_o.ratio_pct.clone(),
        min_date: min_o.date,
        max_ratio_pct: max_o.ratio_pct.clone(),
        max_date: max_o.date,
        mean_ratio_pct: mean,
        breaches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prudence_core::num::int;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn clf_rsf_quota_spills_excess_and_conserves_total() {
        let assets = vec![
            ClfAsset { id: "a65".into(), carrying_value: int(40), fallback: ClfSpilloverBucket::Rsf65 },
            ClfAsset { id: "a85".into(), carrying_value: int(50), fallback: ClfSpilloverBucket::Rsf85 },
        ];
        // Cap = 60 -> takes all 50 of 85%-fallback + 10 of 65%-fallback; 30 spills to 65% RSF.
        let sp = apply_clf_rsf_quota(&assets, &int(60));
        assert_eq!(sp.eligible_10pct, int(60));
        assert_eq!(sp.spillover_85pct, int(0));
        assert_eq!(sp.spillover_65pct, int(30));
        assert_eq!(&sp.eligible_10pct + &sp.spillover_65pct + &sp.spillover_85pct, int(90));
    }

    #[test]
    fn interdependent_pair_validation_enforces_att_c_37() {
        let ok = InterdependentPair {
            id: "ip1".into(),
            asset_record_id: "a1".into(),
            liability_record_id: "l1".into(),
            amount: int(100),
            asset_maturity: d(2028, 6, 30),
            liability_maturity: d(2028, 6, 30),
            pass_through_cashflows: true,
            apra_approved: true,
        };
        assert!(validate_interdependent_pairs(std::slice::from_ref(&ok)).is_ok());

        let mut bad_mat = ok.clone();
        bad_mat.liability_maturity = d(2027, 6, 30);
        assert!(validate_interdependent_pairs(&[bad_mat]).unwrap_err().contains("mismatched maturities"));

        let mut unapproved = ok;
        unapproved.apra_approved = false;
        assert!(validate_interdependent_pairs(&[unapproved]).unwrap_err().contains("APRA approval"));
    }

    #[test]
    fn quarterly_stats_are_exact_and_detect_breaches() {
        let obs = vec![
            DailyObservation { date: d(2026, 7, 1), ratio_pct: int(110) },
            DailyObservation { date: d(2026, 7, 2), ratio_pct: int(98) },
            DailyObservation { date: d(2026, 7, 3), ratio_pct: int(122) },
        ];
        let st = quarterly_compliance_stats(&obs, &int(100)).unwrap();
        assert_eq!(st.count, 3);
        assert_eq!(st.min_ratio_pct, int(98));
        assert_eq!(st.min_date, d(2026, 7, 2));
        assert_eq!(st.max_ratio_pct, int(122));
        assert_eq!(st.mean_ratio_pct, int(110));
        assert_eq!(st.breaches.len(), 1);
    }
}
