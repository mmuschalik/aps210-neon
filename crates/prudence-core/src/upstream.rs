//! Upstream cross-standard helpers consumed by APS 210 (design.txt §1.6):
//! - AASB 121 exact-rational FX conversion (`fx_convert`),
//! - APS 112 ECAI multi-rating selection rule (Att F),
//! - APS 112 LVR <= 80% exact rational check.

use std::collections::BTreeMap;

use crate::num::Q;

/// Exact rational FX conversion under AASB 121.
/// `rates_to_aud` maps ISO currency code -> AUD per 1 unit of that currency.
pub fn fx_convert(amount: &Q, from_ccy: &str, to_ccy: &str, rates_to_aud: &BTreeMap<String, Q>) -> Result<Q, String> {
    if from_ccy == to_ccy {
        return Ok(amount.clone());
    }
    let from_rate = if from_ccy == "AUD" {
        Q::from_integer(1.into())
    } else {
        rates_to_aud.get(from_ccy).cloned().ok_or_else(|| format!("missing AASB 121 FX rate for {from_ccy}"))?
    };
    let to_rate = if to_ccy == "AUD" {
        Q::from_integer(1.into())
    } else {
        rates_to_aud.get(to_ccy).cloned().ok_or_else(|| format!("missing AASB 121 FX rate for {to_ccy}"))?
    };
    if to_rate == Q::default() {
        return Err(format!("zero FX rate for {to_ccy}"));
    }
    Ok(amount * from_rate / to_rate)
}

/// APS 112 Attachment F ECAI multi-rating rule (lower grade number = better credit quality):
/// - 0 ratings -> `None` (unrated)
/// - 1 rating  -> that rating
/// - 2 ratings -> the worse (higher number / higher risk weight) of the two
/// - 3+ ratings -> the worse of the two best (i.e. second-lowest grade number)
pub fn ecai_grade(grades: &[u8]) -> Option<u8> {
    if grades.is_empty() {
        return None;
    }
    let mut sorted = grades.to_vec();
    sorted.sort_unstable();
    match sorted.len() {
        1 => Some(sorted[0]),
        _ => Some(sorted[1]), // for 2: worse of the two; for 3+: second-best
    }
}

/// Exact rational LVR <= 80% test (APS 112 / APS 210 Att C).
/// Returns `None` if property value is zero or negative.
pub fn lvr_at_most_80(loan_balance: &Q, property_value: &Q) -> Option<bool> {
    let zero = Q::default();
    if property_value <= &zero {
        return None;
    }
    let eighty_pct = Q::new(4.into(), 5.into());
    Some(loan_balance / property_value <= eighty_pct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::{int, parse_decimal};

    #[test]
    fn aasb121_fx_conversion_is_exact() {
        let mut rates = BTreeMap::new();
        rates.insert("USD".into(), parse_decimal("1.50").unwrap());
        rates.insert("NZD".into(), parse_decimal("0.90").unwrap());
        assert_eq!(fx_convert(&int(100), "USD", "AUD", &rates).unwrap(), int(150));
        assert_eq!(fx_convert(&int(90), "USD", "NZD", &rates).unwrap(), int(150));
        assert_eq!(fx_convert(&int(42), "AUD", "AUD", &rates).unwrap(), int(42));
        assert!(fx_convert(&int(10), "EUR", "AUD", &rates).is_err());
    }

    #[test]
    fn aps112_ecai_multi_rating_rule() {
        assert_eq!(ecai_grade(&[]), None);
        assert_eq!(ecai_grade(&[1]), Some(1));
        // 2 ratings: take the worse (higher number)
        assert_eq!(ecai_grade(&[1, 3]), Some(3));
        assert_eq!(ecai_grade(&[3, 1]), Some(3));
        // 3+ ratings: take the second-best (worse of the two best)
        assert_eq!(ecai_grade(&[1, 2, 4]), Some(2));
        assert_eq!(ecai_grade(&[4, 1, 1]), Some(1));
    }

    #[test]
    fn aps112_lvr_boundary_is_exact() {
        assert_eq!(lvr_at_most_80(&int(800_000), &int(1_000_000)), Some(true));
        assert_eq!(lvr_at_most_80(&int(800_001), &int(1_000_000)), Some(false));
        assert_eq!(lvr_at_most_80(&int(100), &int(0)), None);
    }
}
