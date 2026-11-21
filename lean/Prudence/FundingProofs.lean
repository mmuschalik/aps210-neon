/-
  Proofs about the Lean model generated from specs/funding/funding.prd.
-/
import Prudence.Funding

namespace FundingAndPlacements

/-- APS 210 Att A para 63: a placement of unknown purpose is treated as
    operational in the reported run - no inflow, and the 50% RSF row. -/
theorem unknown_placement_is_operational (W : World) (p : Placement) (h : p.operational = .unknown) :
    placement_lcr_row W .Conservative p = "none" ∧ placement_nsfr_item W .Conservative p = "33.1" := by
  simp [placement_lcr_row, placement_nsfr_item, operational, resolve, h]

/-- Common Equity Tier 1 and Additional Tier 1 are always regulatory capital
    (ARF 210.6 item 1) and never an LCR outflow. -/
theorem tier1_is_capital (W : World) (m : Mode) (c : CapitalInstrument) (h : c.tier ≠ .T2) :
    capital_nsfr_item W m c = "1" ∧ capital_lcr_row W m c = "none" := by
  cases ht : c.tier <;> simp_all [capital_nsfr_item, capital_lcr_row]

end FundingAndPlacements
