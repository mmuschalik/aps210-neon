import Prudence.SecuredFunding
/-!
  design.txt §4.A.4: "formally prove that classify_HQLA(Portfolio_Unwound)
  equals APRA's ARF 210.1A derived form equations for all valid portfolios".

  About the model GENERATED from specs/secured/secured_funding.prd and the
  formulas GENERATED from specs/arf210/arf_210_1a.prd: for every portfolio of
  repos and reverse repos, the mechanical 30-day unwind of each HQLA class
  (`unwind`) equals the ARF 210.1A adjustment item (1.5.1, 2.5.1, 3.4.1,
  3.5.1) computed from what the portfolio reports in items 11 and 19
  (`contribution`).
-/
namespace SecuredFunding

def total (g : Repo → Int) : List Repo → Int
  | [] => 0
  | r :: rs => g r + total g rs

set_option maxHeartbeats 4000000 in
/-- One transaction, every class: settle it and compare with the form.
    (576 cases: legs x classes x counterparties x re-use x CLF x status.) -/
theorem unwind_one (W : World) (m : Mode) (r : Repo) :
    unwind W m r .HQLA1 = arf_1_5_1 (contribution W m r)
      ∧ unwind W m r .HQLA2A = arf_2_5_1 (contribution W m r)
      ∧ unwind W m r .RmbsHqla2B = arf_3_4_1 (contribution W m r)
      ∧ unwind W m r .NonRmbsHqla2B = arf_3_5_1 (contribution W m r) := by
  unfold unwind arf_1_5_1 arf_2_5_1 arf_3_4_1 arf_3_5_1 contribution qualifies
  generalize maturing W m r = mt
  rcases r with ⟨leg, cash, cls, val, cp, op, rh, ct⟩
  simp only
  generalize resolve m op false (!false) = o
  cases mt <;> cases leg <;> cases cls <;> cases cp <;> cases rh <;> cases o <;> cases hc : W.has_clf <;>
    simp [row, of_which_row, collateral_row, clf_collateral_row, hc] <;> omega

/-- A sum of per-transaction values through a linear form is the linear form
    of the summed items. -/
theorem unwind_portfolio (W : World) (m : Mode) (rs : List Repo) :
    total (fun r => unwind W m r .HQLA1) rs = arf_1_5_1 (fun i => total (fun r => contribution W m r i) rs)
      ∧ total (fun r => unwind W m r .HQLA2A) rs = arf_2_5_1 (fun i => total (fun r => contribution W m r i) rs)
      ∧ total (fun r => unwind W m r .RmbsHqla2B) rs = arf_3_4_1 (fun i => total (fun r => contribution W m r i) rs)
      ∧ total (fun r => unwind W m r .NonRmbsHqla2B) rs = arf_3_5_1 (fun i => total (fun r => contribution W m r i) rs) := by
  induction rs with
  | nil => simp [total, arf_1_5_1, arf_2_5_1, arf_3_4_1, arf_3_5_1]
  | cons r rs ih =>
    obtain ⟨h1, h2, h3, h4⟩ := unwind_one W m r
    obtain ⟨i1, i2, i3, i4⟩ := ih
    simp only [total]
    unfold arf_1_5_1 arf_2_5_1 arf_3_4_1 arf_3_5_1 at *
    dsimp only at *
    refine ⟨?_, ?_, ?_, ?_⟩ <;> omega

end SecuredFunding
