/-
  Proofs about the Lean model generated from Prudence.Specs.Modules.Derivatives.
-/
import Prudence.Derivatives

namespace Derivatives

/-- The negative part of a sum is at most the sum of the negative parts. -/
theorem neg_part_sum_le (xs : List Int) :
    max 0 (0 - xs.foldr (· + ·) 0) ≤ (xs.map (fun a => max 0 (0 - a))).foldr (· + ·) 0 := by
  induction xs with
  | nil => simp
  | cons a t ih => simp only [List.foldr, List.map]; omega

/-- Netting never adds (the spec's invariant `netting_never_adds`), for
    netting sets under a master netting agreement, whatever the flows: the
    netted outflow is at most the gross outflow of the counted flows. -/
theorem netting_never_adds (W : World) (m : Mode) (s : NettingSet) (h : nets W m s = true) :
    net_out W m s ≤
      (((W.NettingSet_flows s).filter (fun f => counted W m f)).map (fun f => max 0 (0 - f.amount))).foldr (· + ·) 0 := by
  have := neg_part_sum_le (((W.NettingSet_flows s).filter (fun f => counted W m f)).map (·.amount))
  simpa [net_out, h, List.map_map, Function.comp_def] using this

/-- An unrecorded netting agreement means gross settlement in the reported run. -/
theorem unknown_mna_is_gross (W : World) (s : NettingSet) (h : s.netting_agreement = .unknown) :
    nets W .Conservative s = false := by
  simp [nets, resolve, h]

end Derivatives
