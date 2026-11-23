/-
  Proofs about the Lean model generated from Prudence.Specs.Modules.Commitments.
-/
import Prudence.Generated.Commitments

namespace Commitments

/-- Unknown drawability is treated as drawable in the reported run: the
    facility reports exactly as if it were known to be drawable. -/
theorem unknown_drawability_is_drawable (W : World) (f : Facility) (h : f.drawable_within_30_days = .unknown) :
    facility_row W .Conservative f = facility_row W .Conservative { f with drawable_within_30_days := .known true } := by
  simp [facility_row, resolve, h]

/-- A facility that cannot be drawn within 30 days reports nowhere. -/
theorem undrawable_reports_nowhere (W : World) (m : Mode) (f : Facility) (h : f.drawable_within_30_days = .known false) :
    facility_row W m f = "none" := by
  simp [facility_row, resolve, h]

end Commitments
