//! Certified collateral pool encumbrance assignment (design.txt §1.3 & §4.A.5).
//!
//! Uses `prudence-core::lp` (exact two-phase simplex + primal-dual certificate
//! verification) to assign an unallocated collateral pool's encumbrance requirement:
//! - **LCR**: lowest-liquidity-first (NonHQLA -> CLF -> HQLA2B -> HQLA2A -> HQLA1)
//!   to maximize unencumbered HQLA.
//! - **NSFR**: highest-RSF-first (ARS 210.0 p. 150) with owned-before-borrowed
//!   rehypothecation tie-breaking.

use prudence_core::Q;
use prudence_core::lp::{Lp, Rel, Sense, Solution, solve, verify};

use crate::HqlaClass;

#[derive(Clone, Debug)]
pub struct CollateralLot {
    pub id: String,
    pub market_value: Q,
    pub carrying_value: Q,
    pub hqla_class: HqlaClass,
    /// Unencumbered NSFR RSF factor (e.g. 0.05 for HQLA1, 0.15 for HQLA2A, 0.50 for HQLA2B, 0.85 for other).
    pub rsf_factor: Q,
    /// True if owned outright by the ADI; false if borrowed/rehypothecated.
    pub owned: bool,
}

#[derive(Clone, Debug)]
pub struct PoolAssignment {
    pub lot_ids: Vec<String>,
    /// Encumbered amount assigned to each lot (exact rationals).
    pub encumbered: Vec<Q>,
    /// Verified primal-dual LP certificate.
    pub lp: Lp,
    pub solution: Solution,
}

fn lcr_liquidity_weight(c: HqlaClass) -> Q {
    let n: i64 = match c {
        HqlaClass::Hqla1 => 100,
        HqlaClass::Hqla2a => 85,
        HqlaClass::RmbsHqla2b => 75,
        HqlaClass::NonRmbsHqla2b => 50,
        HqlaClass::ClfEligible => 20,
        HqlaClass::NonHqla => 1,
    };
    Q::from_integer(n.into())
}

fn build_rows(caps: &[Q], req: Q) -> Vec<(Vec<Q>, Rel, Q)> {
    let n = caps.len();
    let one = Q::from_integer(1.into());
    let zero = Q::default();
    let mut rows = vec![(vec![one.clone(); n], Rel::Eq, req)];
    for (i, cap) in caps.iter().enumerate() {
        let mut r_lo = vec![zero.clone(); n];
        r_lo[i] = one.clone();
        rows.push((r_lo, Rel::Ge, zero.clone()));

        let mut r_hi = vec![zero.clone(); n];
        r_hi[i] = one.clone();
        rows.push((r_hi, Rel::Le, cap.clone()));
    }
    rows
}

/// Solve LCR lowest-liquidity-first collateral pool encumbrance with a verified
/// primal-dual certificate.
pub fn assign_lcr_encumbrance(lots: &[CollateralLot], required_encumbrance: &Q) -> Result<PoolAssignment, String> {
    if lots.is_empty() {
        return Err("empty collateral pool".into());
    }
    let caps: Vec<Q> = lots.iter().map(|l| l.market_value.clone()).collect();
    let total_mv: Q = caps.iter().cloned().sum();
    let req = required_encumbrance.clone().min(total_mv);

    let lp = Lp {
        sense: Sense::Min,
        objective: lots.iter().map(|l| lcr_liquidity_weight(l.hqla_class)).collect(),
        rows: build_rows(&caps, req),
    };
    let solution = solve(&lp).map_err(|e| format!("collateral pool LP failed: {e:?}"))?;
    verify(&lp, &solution).map_err(|e| format!("collateral pool certificate invalid: {e}"))?;

    Ok(PoolAssignment {
        lot_ids: lots.iter().map(|l| l.id.clone()).collect(),
        encumbered: solution.x.clone(),
        lp,
        solution,
    })
}

/// Solve NSFR highest-RSF-first collateral pool encumbrance (preferring owned
/// securities before borrowed ones on ties) with a verified primal-dual certificate.
pub fn assign_nsfr_encumbrance(lots: &[CollateralLot], required_encumbrance: &Q) -> Result<PoolAssignment, String> {
    if lots.is_empty() {
        return Err("empty collateral pool".into());
    }
    let caps: Vec<Q> = lots.iter().map(|l| l.carrying_value.clone()).collect();
    let total_cv: Q = caps.iter().cloned().sum();
    let req = required_encumbrance.clone().min(total_cv);
    let zero = Q::default();
    let owned_bonus = Q::new(1.into(), 10_000.into());

    let lp = Lp {
        sense: Sense::Max,
        objective: lots
            .iter()
            .map(|l| {
                let bonus = if l.owned { owned_bonus.clone() } else { zero.clone() };
                &l.rsf_factor + &bonus
            })
            .collect(),
        rows: build_rows(&caps, req),
    };
    let solution = solve(&lp).map_err(|e| format!("NSFR collateral pool LP failed: {e:?}"))?;
    verify(&lp, &solution).map_err(|e| format!("NSFR collateral pool certificate invalid: {e}"))?;

    Ok(PoolAssignment {
        lot_ids: lots.iter().map(|l| l.id.clone()).collect(),
        encumbered: solution.x.clone(),
        lp,
        solution,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prudence_core::num::int;

    #[test]
    fn lcr_encumbers_lowest_liquidity_first_with_certificate() {
        let lots = vec![
            CollateralLot { id: "hqla1".into(), market_value: int(100), carrying_value: int(100), hqla_class: HqlaClass::Hqla1, rsf_factor: Q::new(5.into(), 100.into()), owned: true },
            CollateralLot { id: "hqla2b".into(), market_value: int(60), carrying_value: int(60), hqla_class: HqlaClass::NonRmbsHqla2b, rsf_factor: Q::new(50.into(), 100.into()), owned: true },
            CollateralLot { id: "nonhqla".into(), market_value: int(50), carrying_value: int(50), hqla_class: HqlaClass::NonHqla, rsf_factor: Q::new(85.into(), 100.into()), owned: false },
        ];
        let a = assign_lcr_encumbrance(&lots, &int(80)).unwrap();
        assert_eq!(a.encumbered, vec![int(0), int(30), int(50)]);
    }

    #[test]
    fn nsfr_encumbers_highest_rsf_and_owned_first_with_certificate() {
        let lots = vec![
            CollateralLot { id: "borrowed_85".into(), market_value: int(50), carrying_value: int(50), hqla_class: HqlaClass::NonHqla, rsf_factor: Q::new(85.into(), 100.into()), owned: false },
            CollateralLot { id: "owned_85".into(), market_value: int(50), carrying_value: int(50), hqla_class: HqlaClass::NonHqla, rsf_factor: Q::new(85.into(), 100.into()), owned: true },
            CollateralLot { id: "hqla1".into(), market_value: int(100), carrying_value: int(100), hqla_class: HqlaClass::Hqla1, rsf_factor: Q::new(5.into(), 100.into()), owned: true },
        ];
        let a = assign_nsfr_encumbrance(&lots, &int(60)).unwrap();
        assert_eq!(a.encumbered, vec![int(10), int(50), int(0)]);
    }
}
