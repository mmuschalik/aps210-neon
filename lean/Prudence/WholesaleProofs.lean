/-
  Proofs about the Lean model generated from specs/deposits/wholesale.prd
  (lean/Prudence/Wholesale.lean). Hand-written; the model is not.
-/
import Prudence.Wholesale

namespace WholesaleDeposits

/-- The two nested splits (operational, then FCS-covered) conserve the
    balance: every dollar lands in exactly one portion, whatever the world,
    the mode and the allocation. -/
theorem portions_conserve (W : World) (m : Mode) (d : Deposit) :
    portion W m d true true + portion W m d true false + portion W m d false true + portion W m d false false
      = d.balance := by
  simp only [portion]
  simp
  omega

/-- APG 210 para 135: without an assessment of the operational service, the
    reported (conservative) run treats the whole deposit as excess. -/
theorem unknown_service_is_excess (W : World) (d : Deposit) (h : d.operational_service = .unknown) :
    operational_amount W .Conservative d = 0 ∧ portion W .Conservative d false false = d.balance := by
  simp [operational_amount, portion, h, resolve]

/-- Likewise when the service is known but the operational balance is not. -/
theorem unknown_balance_is_excess (W : World) (d : Deposit) (hb : d.operational_balance = .unknown) (h0 : 0 ≤ d.balance) :
    operational_amount W .Conservative d = 0 := by
  simp only [operational_amount, hb, resolve]
  split <;> omega

end WholesaleDeposits
