//! Exact rational linear programming (design.txt §4.A.5).
//!
//! Two-phase primal simplex over exact rationals with Bland's rule (always
//! terminates). Variables are free; bounds are ordinary rows. The solver is
//! *not* trusted: every solution carries dual values, and `verify` checks the
//! primal-dual certificate independently (primal feasibility, dual
//! feasibility, equal objective values), which proves optimality exactly.

use num_traits::{Signed, Zero};

use crate::num::Q;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rel {
    Le,
    Ge,
    Eq,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    Max,
    Min,
}

/// optimise `objective · x` subject to `row · x (rel) rhs`, x free.
#[derive(Clone, Debug)]
pub struct Lp {
    pub sense: Sense,
    pub objective: Vec<Q>,
    pub rows: Vec<(Vec<Q>, Rel, Q)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LpError {
    Infeasible,
    Unbounded,
}

#[derive(Clone, Debug)]
pub struct Solution {
    pub x: Vec<Q>,
    pub value: Q,
    /// One dual value per row: the change in the optimal value per unit
    /// increase of that row's right-hand side (shadow price).
    pub duals: Vec<Q>,
}

fn dot(a: &[Q], b: &[Q]) -> Q {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

impl Lp {
    pub fn n(&self) -> usize {
        self.objective.len()
    }
}

/// Solve exactly. Returns an optimal vertex with its duals.
pub fn solve(lp: &Lp) -> Result<Solution, LpError> {
    let n = lp.n();
    let m = lp.rows.len();
    // Standard form: x = p - q (p, q >= 0); one slack per inequality; rhs >= 0;
    // one artificial per row. Columns: [p (n) | q (n) | slacks | artificials (m)].
    let n_slack = lp.rows.iter().filter(|(_, r, _)| *r != Rel::Eq).count();
    let (art0, cols) = (2 * n + n_slack, 2 * n + n_slack + m);
    let mut t: Vec<Vec<Q>> = vec![vec![Q::zero(); cols]; m];
    let mut b: Vec<Q> = vec![Q::zero(); m];
    let mut flipped = vec![false; m];
    let mut s = 2 * n;
    for (i, (row, rel, rhs)) in lp.rows.iter().enumerate() {
        for j in 0..n {
            t[i][j] = row[j].clone();
            t[i][n + j] = -row[j].clone();
        }
        match rel {
            Rel::Le => {
                t[i][s] = Q::from_integer(1.into());
                s += 1;
            }
            Rel::Ge => {
                t[i][s] = Q::from_integer((-1).into());
                s += 1;
            }
            Rel::Eq => {}
        }
        b[i] = rhs.clone();
        if b[i].is_negative() {
            flipped[i] = true;
            b[i] = -b[i].clone();
            for v in t[i].iter_mut() {
                *v = -v.clone();
            }
        }
        t[i][art0 + i] = Q::from_integer(1.into());
    }
    let mut basis: Vec<usize> = (art0..art0 + m).collect();
    let mut live = vec![true; m];

    // Phase 1: maximise -(sum of artificials).
    let mut c1 = vec![Q::zero(); cols];
    for c in c1.iter_mut().skip(art0) {
        *c = Q::from_integer((-1).into());
    }
    simplex(&mut t, &mut b, &mut basis, &live, &c1, cols)?;
    let infeas: Q = (0..m).filter(|&i| live[i] && basis[i] >= art0).map(|i| b[i].clone()).sum();
    if infeas.is_positive() {
        return Err(LpError::Infeasible);
    }
    // Drive remaining (zero-valued) artificials out of the basis; drop redundant rows.
    for i in 0..m {
        if basis[i] >= art0 {
            match (0..art0).find(|&j| !t[i][j].is_zero()) {
                Some(j) => pivot(&mut t, &mut b, &mut basis, i, j),
                None => live[i] = false,
            }
        }
    }

    // Phase 2: the real objective (as a maximisation), artificials barred.
    let sign = if lp.sense == Sense::Max { Q::from_integer(1.into()) } else { Q::from_integer((-1).into()) };
    let mut c2 = vec![Q::zero(); cols];
    for j in 0..n {
        c2[j] = &sign * &lp.objective[j];
        c2[n + j] = -(&sign * &lp.objective[j]);
    }
    simplex(&mut t, &mut b, &mut basis, &live, &c2[..art0], art0)?;

    let mut z = vec![Q::zero(); cols];
    for i in 0..m {
        if live[i] {
            z[basis[i]] = b[i].clone();
        }
    }
    let x: Vec<Q> = (0..n).map(|j| &z[j] - &z[n + j]).collect();
    // Duals of the maximisation: y = c_B B^-1; B^-1's columns are the artificial columns.
    let duals: Vec<Q> = (0..m)
        .map(|r| {
            let y: Q = (0..m).filter(|&i| live[i]).map(|i| &c2[basis[i]] * &t[i][art0 + r]).sum();
            let y = if flipped[r] { -y } else { y };
            &sign * y
        })
        .collect();
    let value = dot(&lp.objective, &x);
    Ok(Solution { x, value, duals })
}

fn pivot(t: &mut [Vec<Q>], b: &mut [Q], basis: &mut [usize], r: usize, c: usize) {
    let p = t[r][c].clone();
    for v in t[r].iter_mut() {
        *v /= &p;
    }
    b[r] /= &p;
    for i in 0..t.len() {
        if i != r && !t[i][c].is_zero() {
            let f = t[i][c].clone();
            for j in 0..t[i].len() {
                let d = &f * &t[r][j];
                t[i][j] -= d;
            }
            let d = &f * &b[r];
            b[i] -= d;
        }
    }
    basis[r] = c;
}

/// Maximise `c · z` over the first `ncols` columns (Bland's rule).
fn simplex(t: &mut [Vec<Q>], b: &mut [Q], basis: &mut [usize], live: &[bool], c: &[Q], ncols: usize) -> Result<(), LpError> {
    loop {
        // Entering: lowest index with positive reduced profit.
        let entering = (0..ncols).find(|&j| {
            let zj: Q = (0..t.len()).filter(|&i| live[i]).map(|i| c.get(basis[i]).cloned().unwrap_or_default() * &t[i][j]).sum();
            c[j].clone() - zj > Q::zero()
        });
        let Some(j) = entering else { return Ok(()) };
        // Leaving: minimum ratio; ties to the lowest basis index.
        let mut best: Option<(Q, usize, usize)> = None;
        for i in 0..t.len() {
            if live[i] && t[i][j].is_positive() {
                let ratio = &b[i] / &t[i][j];
                let better = match &best {
                    None => true,
                    Some((r, _, bi)) => ratio < *r || (ratio == *r && basis[i] < *bi),
                };
                if better {
                    best = Some((ratio, i, basis[i]));
                }
            }
        }
        let Some((_, r, _)) = best else { return Err(LpError::Unbounded) };
        pivot(t, b, basis, r, j);
    }
}

/// Independent optimality certificate check (the "tiny checker"):
/// primal feasibility, dual feasibility, and equal primal/dual objectives.
pub fn verify(lp: &Lp, sol: &Solution) -> Result<(), String> {
    let n = lp.n();
    if sol.x.len() != n || sol.duals.len() != lp.rows.len() {
        return Err("certificate has the wrong shape".into());
    }
    for (i, (row, rel, rhs)) in lp.rows.iter().enumerate() {
        let lhs = dot(row, &sol.x);
        let ok = match rel {
            Rel::Le => lhs <= *rhs,
            Rel::Ge => lhs >= *rhs,
            Rel::Eq => lhs == *rhs,
        };
        if !ok {
            return Err(format!("row {i} violated: {lhs} {rel:?} {rhs}"));
        }
    }
    // Work in maximisation form: c' = s c, y' = s y.
    let s = if lp.sense == Sense::Max { Q::from_integer(1.into()) } else { Q::from_integer((-1).into()) };
    for (i, (_, rel, _)) in lp.rows.iter().enumerate() {
        let y = &s * &sol.duals[i];
        let ok = match rel {
            Rel::Le => !y.is_negative(),
            Rel::Ge => !y.is_positive(),
            Rel::Eq => true,
        };
        if !ok {
            return Err(format!("dual of row {i} has the wrong sign"));
        }
    }
    // Free variables: A^T y' = c' exactly.
    for j in 0..n {
        let aty: Q = lp.rows.iter().zip(&sol.duals).map(|((row, _, _), y)| &row[j] * &s * y).sum();
        if aty != &s * &lp.objective[j] {
            return Err(format!("dual infeasible in column {j}"));
        }
    }
    let primal = dot(&lp.objective, &sol.x);
    let dual: Q = lp.rows.iter().zip(&sol.duals).map(|((_, _, rhs), y)| rhs * y).sum();
    if primal != dual || primal != sol.value {
        return Err(format!("duality gap: primal {primal}, dual {dual}"));
    }
    Ok(())
}

/// Solve, then break ties among optimal solutions lexicographically: maximise
/// x[0], then x[1], ... (variables are given in a canonical order, e.g. by
/// subject id). Returns the tie-broken solution with the duals of the
/// original problem; the certificate is verified.
pub fn solve_lexicographic(lp: &Lp) -> Result<Solution, String> {
    let first = solve(lp).map_err(|e| format!("{e:?}"))?;
    verify(lp, &first).map_err(|e| format!("certificate rejected: {e}"))?;
    let n = lp.n();
    let mut rows = lp.rows.clone();
    rows.push((lp.objective.clone(), Rel::Eq, first.value.clone()));
    let mut x = first.x.clone();
    for k in 0..n {
        let mut obj = vec![Q::zero(); n];
        obj[k] = Q::from_integer(1.into());
        let sub = Lp { sense: Sense::Max, objective: obj, rows: rows.clone() };
        let s = solve(&sub).map_err(|e| format!("tie-break {k}: {e:?}"))?;
        verify(&sub, &s).map_err(|e| format!("tie-break certificate rejected: {e}"))?;
        x = s.x;
        let mut fix = vec![Q::zero(); n];
        fix[k] = Q::from_integer(1.into());
        rows.push((fix, Rel::Eq, x[k].clone()));
    }
    let out = Solution { value: dot(&lp.objective, &x), x, duals: first.duals };
    verify(lp, &out).map_err(|e| format!("final certificate rejected: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::int;

    fn q(n: i64) -> Q {
        int(n)
    }

    #[test]
    fn textbook_lp_with_certificate() {
        // max 3x + 5y  s.t. x <= 4, 2y <= 12, 3x + 2y <= 18, x, y >= 0  -> (2, 6), 36
        let lp = Lp {
            sense: Sense::Max,
            objective: vec![q(3), q(5)],
            rows: vec![
                (vec![q(1), q(0)], Rel::Le, q(4)),
                (vec![q(0), q(2)], Rel::Le, q(12)),
                (vec![q(3), q(2)], Rel::Le, q(18)),
                (vec![q(1), q(0)], Rel::Ge, q(0)),
                (vec![q(0), q(1)], Rel::Ge, q(0)),
            ],
        };
        let s = solve(&lp).unwrap();
        assert_eq!(s.x, vec![q(2), q(6)]);
        assert_eq!(s.value, q(36));
        assert_eq!(s.duals, vec![q(0), Q::new(3.into(), 2.into()), q(1), q(0), q(0)]);
        verify(&lp, &s).unwrap();
    }

    #[test]
    fn minimise_with_equalities_free_variables_and_negative_rhs() {
        // min x + y  s.t. x - y = -2, x >= -5, y <= 10  -> x = -5, y = -3, value -8
        let lp = Lp {
            sense: Sense::Min,
            objective: vec![q(1), q(1)],
            rows: vec![
                (vec![q(1), q(-1)], Rel::Eq, q(-2)),
                (vec![q(1), q(0)], Rel::Ge, q(-5)),
                (vec![q(0), q(1)], Rel::Le, q(10)),
            ],
        };
        let s = solve(&lp).unwrap();
        assert_eq!((s.x.clone(), s.value.clone()), (vec![q(-5), q(-3)], q(-8)));
        verify(&lp, &s).unwrap();
    }

    #[test]
    fn infeasible_unbounded_and_bad_certificates() {
        let infeasible = Lp {
            sense: Sense::Max,
            objective: vec![q(1)],
            rows: vec![(vec![q(1)], Rel::Le, q(1)), (vec![q(1)], Rel::Ge, q(2))],
        };
        assert_eq!(solve(&infeasible).unwrap_err(), LpError::Infeasible);
        let unbounded = Lp { sense: Sense::Max, objective: vec![q(1)], rows: vec![(vec![q(1)], Rel::Ge, q(0))] };
        assert_eq!(solve(&unbounded).unwrap_err(), LpError::Unbounded);
        let lp = Lp { sense: Sense::Max, objective: vec![q(1)], rows: vec![(vec![q(1)], Rel::Le, q(3))] };
        let mut s = solve(&lp).unwrap();
        verify(&lp, &s).unwrap();
        s.x[0] = q(2); // feasible but not optimal: the duality gap exposes it
        s.value = q(2);
        assert!(verify(&lp, &s).unwrap_err().contains("duality gap"));
    }

    #[test]
    fn degenerate_and_redundant_rows() {
        // Redundant equalities and a degenerate vertex.
        let lp = Lp {
            sense: Sense::Max,
            objective: vec![q(1), q(1)],
            rows: vec![
                (vec![q(1), q(1)], Rel::Eq, q(5)),
                (vec![q(2), q(2)], Rel::Eq, q(10)),
                (vec![q(1), q(0)], Rel::Ge, q(0)),
                (vec![q(0), q(1)], Rel::Ge, q(0)),
                (vec![q(1), q(0)], Rel::Le, q(5)),
            ],
        };
        let s = solve_lexicographic(&lp).unwrap();
        assert_eq!(s.x, vec![q(5), q(0)]); // ties: maximise x[0] first
        assert_eq!(s.value, q(5));
    }
}
