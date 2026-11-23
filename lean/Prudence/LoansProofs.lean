/-
  Proofs about the Lean model generated from Prudence.Specs.Modules.Loans
  (lean/Prudence/Loans.lean). Hand-written; the model is not.
-/
import Prudence.Loans

namespace Loans

/-- Bucketing each repayment by its date (< 6 months, 6-12 months, >= 1 year)
    conserves the balance exactly, over the rationals, whatever the schedule. -/
theorem portions_conserve (W : World) (m : Mode) (l : Loan) :
    portion W m l true true + portion W m l true false + portion W m l false true + portion W m l false false
      = l.balance := by
  simp only [portion]
  generalize principal_before W m l (addMonths W.reporting_date 6) = a
  generalize principal_before W m l (addMonths W.reporting_date 12) = b
  simp
  grind

/-- APS 210 Att A para 54: no inflow from a loan not known to be fully
    performing, in the reported (conservative) run. -/
theorem unknown_performing_no_inflow (W : World) (l : Loan) (h : l.performing = .unknown) :
    inflow_30d W .Conservative l = 0 := by
  simp [inflow_30d, h, resolve]

/-- An open-maturity loan only ever contributes its minimum payment (and at
    most its balance), whatever the payment cycle (Att A para 60). -/
theorem open_maturity_minimum_payment (W : World) (m : Mode) (l : Loan) (mp : Rat)
    (hc : l.contract = .UMP mp) (k : Int) :
    cash W m l k = if k = 0 then min mp l.balance else 0 := by
  simp [cash, hc]

end Loans
