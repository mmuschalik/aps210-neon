import Prudence.Generated.RetailSme
/-!
  Proofs of the two theorems of Prudence.Specs.Modules.RetailSme, about the Lean
  model GENERATED from that spec (Prudence/RetailSme.lean):

    theorem unknowns_are_prudent
    theorem sme_cliff_never_increases_stable_funding

  Structure: a small finite vocabulary (`stab`, `rateF`, `asfF`, orders) with
  lemmas proven by exhaustion; bridge lemmas showing the generated definitions
  compute exactly that vocabulary; then monotonicity per deposit, lifted to
  books. What the generated `World` leaves abstract (the `optimize` solution,
  calendar buckets, the records behind `Customer_deposits`) is stated as
  hypotheses.
-/
namespace RetailAndSmeDeposits

-- ------------------------------------------------------------------ vocabulary

def sev : Stability → Nat
  | .Stable => 0
  | .LessStable => 1
  | .HigherRunOff => 2

def horLe (a b : Horizon) : Prop := a = b ∨ a = .Excluded
def clsLe (o c : Class) : Prop := o = c ∨ (o = .RetailLessStable ∧ c = .FinancialInstitution)

def scoreN (covered online rel rateD : Bool) : Nat :=
  (if !covered then 2 else 0) + (if online then 2 else 0) + (if !rel then 1 else 0) + (if rateD then 1 else 0)

def stab (canStable rel trans online rateD covered : Bool) : Stability :=
  if covered && canStable && (rel || trans) then .Stable
  else if scoreN covered online rel rateD ≥ 3 then .HigherRunOff
  else .LessStable

def lcrRate : Horizon → Stability → Int
  | .Excluded, _ => 0
  | .Called, _ => 100
  | .Demand, .Stable => 5
  | .Demand, .LessStable => 10
  | .Demand, .HigherRunOff => 25

def rateF (c : Class) (h : Horizon) (s : Stability) (insured : Bool) : Int :=
  match c with
  | .Retail | .RetailLessStable | .SmeRetail => lcrRate h s
  | .NonFinancialCorporate => match h with
    | .Excluded => 0
    | .Called => 100
    | .Demand => if insured then 20 else 40
  | .FinancialInstitution => if h == .Excluded then 0 else 100

def factor (lo mid hi : Int) (b : Nat) : Int := if b = 0 then lo else if b = 1 then mid else hi

def asfF (c : Class) (s : Stability) (b : Nat) : Int :=
  match c with
  | .Retail | .RetailLessStable | .SmeRetail =>
    if s == .Stable then factor 95 95 100 b else factor 90 90 100 b
  | .NonFinancialCorporate => factor 50 50 100 b
  | .FinancialInstitution => factor 0 50 100 b

def isRetail : Class → Bool
  | .Retail | .RetailLessStable | .SmeRetail => true
  | _ => false

/-- NFC "entire deposit insured", for a holder whose only deposit is `d`. -/
def ins (W : World) (m : Mode) (d : Deposit) : Bool :=
  resolve m d.fcs_eligible false true && decide (d.balance ≤ W.fcs_limit)

-- ------------------------------------------------------------------ bridges

theorem stability_eq (W : World) (m : Mode) (d : Deposit) (cv : Bool) :
    stability W m d cv =
      stab («class» W m d.holder != .RetailLessStable) (resolve m d.holder.established_relationship false true)
        (resolve m d.transactional false true) (resolve m d.online true false) (resolve m d.rate_driven true false) cv := by
  unfold stability score stab scoreN
  simp only [Bool.not_false, Bool.not_true, pcmp, ToOpt.toOpt]
  generalize («class» W m d.holder != Class.RetailLessStable) = cs
  generalize resolve m d.holder.established_relationship false true = r
  generalize resolve m d.transactional false true = t
  generalize resolve m d.online true false = o
  generalize resolve m d.rate_driven true false = q
  cases cs <;> cases r <;> cases t <;> cases o <;> cases q <;> cases cv <;> decide

theorem weight_eq (W : World) (m : Mode) (d : Deposit) (cv : Bool) (hd : W.Customer_deposits d.holder = [d]) :
    weight_Arf210_1A (out_lcr W m d cv) = rateF («class» W m d.holder) (horizon W m d) (stability W m d cv) (ins W m d) := by
  unfold out_lcr ins
  rw [hd]
  simp only [List.all, List.map, List.foldr, pcmp, ToOpt.toOpt, Bool.not_false, Int.add_zero, Bool.and_true]
  generalize «class» W m d.holder = c
  generalize horizon W m d = h
  generalize stability W m d cv = s
  generalize resolve m d.fcs_eligible false true = f
  generalize decide (d.balance ≤ W.fcs_limit) = l
  cases c <;> cases h <;> cases s <;> cases cv <;> cases f <;> cases l <;> decide

theorem factor_eq (W : World) (m : Mode) (d : Deposit) (cv : Bool) (hb : W.bucket (earliest_redemption W m d) ≤ 2) :
    factor_Arf210_6 (out_nsfr W m d cv) = asfF («class» W m d.holder) (stability W m d cv) (W.bucket (earliest_redemption W m d)) := by
  unfold out_nsfr
  generalize «class» W m d.holder = c
  generalize stability W m d cv = s
  generalize W.bucket (earliest_redemption W m d) = b at hb ⊢
  have : b = 0 ∨ b = 1 ∨ b = 2 := by omega
  rcases this with rfl | rfl | rfl <;> cases c <;> cases s <;> decide

-- ------------------------------------------------------------------ arithmetic

theorem shift_le (r₁ r₂ s₁ s₂ c d b : Int) (hc : 0 ≤ c) (hcd : c ≤ d) (hdb : d ≤ b)
    (h₁ : r₁ ≤ s₁) (h₂ : r₂ ≤ s₂) (hs : s₁ ≤ s₂) :
    r₁ * d + r₂ * (b - d) ≤ s₁ * c + s₂ * (b - c) := by
  have e1 : r₁ * d ≤ s₁ * d := Int.mul_le_mul_of_nonneg_right h₁ (by omega)
  have e2 : r₂ * (b - d) ≤ s₂ * (b - d) := Int.mul_le_mul_of_nonneg_right h₂ (by omega)
  have e3 : s₁ * (d - c) ≤ s₂ * (d - c) := Int.mul_le_mul_of_nonneg_right hs (by omega)
  have x1 : s₁ * c + s₁ * (d - c) = s₁ * d := by rw [← Int.mul_add]; congr 1; omega
  have x2 : s₂ * (b - d) + s₂ * (d - c) = s₂ * (b - c) := by rw [← Int.mul_add]; congr 1; omega
  omega

theorem shift_ge (r₁ r₂ s₁ s₂ c d b : Int) (hc : 0 ≤ c) (hcd : c ≤ d) (hdb : d ≤ b)
    (h₁ : r₁ ≤ s₁) (h₂ : r₂ ≤ s₂) (hs : s₂ ≤ s₁) :
    r₁ * c + r₂ * (b - c) ≤ s₁ * d + s₂ * (b - d) := by
  have e1 : r₁ * c ≤ s₁ * c := Int.mul_le_mul_of_nonneg_right h₁ (by omega)
  have e2 : r₂ * (b - c) ≤ s₂ * (b - c) := Int.mul_le_mul_of_nonneg_right h₂ (by omega)
  have e3 : s₂ * (d - c) ≤ s₁ * (d - c) := Int.mul_le_mul_of_nonneg_right hs (by omega)
  have x1 : s₁ * c + s₁ * (d - c) = s₁ * d := by rw [← Int.mul_add]; congr 1; omega
  have x2 : s₂ * (b - d) + s₂ * (d - c) = s₂ * (b - c) := by rw [← Int.mul_add]; congr 1; omega
  omega

theorem split_ge (r₁ r₂ g c b : Int) (hc : 0 ≤ c) (hcb : c ≤ b) (h₁ : g ≤ r₁) (h₂ : g ≤ r₂) :
    g * b ≤ r₁ * c + r₂ * (b - c) := by
  have e1 : g * c ≤ r₁ * c := Int.mul_le_mul_of_nonneg_right h₁ hc
  have e2 : g * (b - c) ≤ r₂ * (b - c) := Int.mul_le_mul_of_nonneg_right h₂ (by omega)
  have x : g * c + g * (b - c) = g * b := by rw [← Int.mul_add]; congr 1; omega
  omega

-- ------------------------------------------------------------------ finite lemmas (exhaustive)

theorem stab_cover (cs rel tr on rd : Bool) : sev (stab cs rel tr on rd true) ≤ sev (stab cs rel tr on rd false) := by
  cases cs <;> cases rel <;> cases tr <;> cases on <;> cases rd <;> decide

theorem stab_mono (cs ro rc to tc oo oc dd dc cv : Bool)
    (hr : rc = true → ro = true) (ht : tc = true → to = true)
    (ho : oo = true → oc = true) (hd : dd = true → dc = true) :
    sev (stab cs ro to oo dd cv) ≤ sev (stab cs rc tc oc dc cv) := by
  cases cs <;> cases ro <;> cases rc <;> cases to <;> cases tc <;> cases oo <;> cases oc <;>
    cases dd <;> cases dc <;> cases cv <;> simp_all <;> decide

theorem rateF_props (c : Class) (h : Horizon) (s₁ s₂ : Stability) (i : Bool) (hs : sev s₁ ≤ sev s₂) :
    0 ≤ rateF c h s₂ i ∧ rateF c h s₁ i ≤ rateF c h s₂ i := by
  cases c <;> cases h <;> cases s₁ <;> cases s₂ <;> cases i <;> simp_all [sev] <;> decide

theorem rateF_mono (co cc : Class) (ho hc : Horizon) (so sc : Stability) (io ic : Bool)
    (hcl : clsLe co cc) (hh : horLe ho hc) (hs : co = cc → sev so ≤ sev sc) (hi : ic = true → io = true) :
    rateF co ho so io ≤ rateF cc hc sc ic := by
  unfold clsLe at hcl; unfold horLe at hh
  cases co <;> cases cc <;> simp at hcl <;> cases ho <;> cases hc <;> simp at hh <;>
    cases so <;> cases sc <;> (try simp [sev] at hs) <;> cases io <;> cases ic <;> simp at hi <;> decide

theorem asfF_cover (c : Class) (s₁ s₂ : Stability) (b : Nat) (hs : sev s₁ ≤ sev s₂) :
    asfF c s₂ b ≤ asfF c s₁ b := by
  unfold asfF factor
  cases c <;> cases s₁ <;> cases s₂ <;> simp_all [sev] <;> split <;> (try split) <;> decide

theorem asfF_mono (co cc : Class) (so sc : Stability) (bc bo : Nat)
    (hcl : clsLe co cc) (hs : co = cc → sev so ≤ sev sc) (hb : bc ≤ bo) :
    asfF cc sc bc ≤ asfF co so bo := by
  unfold clsLe at hcl
  unfold asfF factor
  cases co <;> cases cc <;> simp at hcl <;> cases so <;> cases sc <;> (try simp [sev] at hs) <;>
    simp <;> (repeat' split) <;> first | decide | omega

theorem asfF_retail_ge_nfc (c : Class) (s s' : Stability) (b : Nat) (hc : c ≠ .FinancialInstitution) :
    asfF .NonFinancialCorporate s' b ≤ asfF c s b := by
  unfold asfF factor
  cases c <;> simp at hc <;> cases s <;> simp <;> (repeat' split) <;> decide

-- ------------------------------------------------------------------ the generated rules, reported vs optimistic

theorem res_false (x : U Bool) :
    resolve .Conservative x false true = true → resolve .Optimistic x false true = true := by
  cases x <;> simp [resolve]

theorem res_true (x : U Bool) :
    resolve .Optimistic x true false = true → resolve .Conservative x true false = true := by
  cases x <;> simp [resolve]

theorem class_le (W : World) (c : Customer) (hk : c.kind ≠ .Sme) :
    clsLe («class» W .Optimistic c) («class» W .Conservative c) := by
  unfold clsLe «class»
  rcases c with ⟨k, _, _, _, pq⟩
  cases k <;> simp_all <;> cases pq <;> simp [resolve] <;> rename_i b <;> cases b <;> simp

theorem horizon_le (W : World) (d : Deposit) : horLe (horizon W .Optimistic d) (horizon W .Conservative d) := by
  unfold horLe horizon
  rcases d with ⟨_, pc, _, _, _, _, _, ct⟩
  cases ct with
  | UMP => simp
  | CLM n g => simp
  | PAM mat ew =>
    simp only
    cases ew with
    | known t => simp [resolve]
    | unknown => (simp only [resolve]; split <;> simp) <;> exact Classical.em _

theorem earliest_le (W : World) (d : Deposit) :
    earliest_redemption W .Conservative d ≤ earliest_redemption W .Optimistic d := by
  unfold earliest_redemption
  rcases d with ⟨_, pc, _, _, _, _, _, ct⟩
  cases ct with
  | UMP => simp
  | CLM n g => simp
  | PAM mat ew =>
    simp only
    cases ew with
    | known t => simp [resolve]
    | unknown => simp [resolve, pcmp, ToOpt.toOpt]; split <;> simp_all <;> omega

theorem stability_mono (W : World) (d : Deposit) (cv : Bool)
    (hcs : «class» W .Optimistic d.holder = «class» W .Conservative d.holder) :
    sev (stability W .Optimistic d cv) ≤ sev (stability W .Conservative d cv) := by
  rw [stability_eq, stability_eq, hcs]
  exact stab_mono _ _ _ _ _ _ _ _ _ _ (res_false _) (res_false _) (res_true _) (res_true _)

-- ------------------------------------------------------------------ per deposit

/-- What a deposit in a book with one deposit per holder and no SME needs from
    the world: its holder's records, the `optimize` solution for one deposit
    (cover = min(limit, balance) if a retail class and FCS-eligible), and
    monotone buckets 0..2. -/
structure Prudent (W : World) (d : Deposit) : Prop where
  no_sme : d.holder.kind ≠ .Sme
  own : W.Customer_deposits d.holder = [d]
  bal : 0 ≤ d.balance
  limit : 0 ≤ W.fcs_limit
  cover : ∀ m, W.covered m d =
    if isRetail («class» W m d.holder) && resolve m d.fcs_eligible false true then min W.fcs_limit d.balance else 0
  bucket_mono : ∀ a b, a ≤ b → W.bucket a ≤ W.bucket b
  bucket_le : ∀ a, W.bucket a ≤ 2

theorem rate_cover (W : World) (m : Mode) (d : Deposit) (h : Prudent W d) :
    0 ≤ weight_Arf210_1A (out_lcr W m d false) ∧ weight_Arf210_1A (out_lcr W m d true) ≤ weight_Arf210_1A (out_lcr W m d false) := by
  rw [weight_eq W m d true h.own, weight_eq W m d false h.own]
  exact rateF_props _ _ _ _ _ (by rw [stability_eq, stability_eq]; exact stab_cover _ _ _ _ _)

theorem rate_mono (W : World) (d : Deposit) (h : Prudent W d) (cv : Bool) :
    weight_Arf210_1A (out_lcr W .Optimistic d cv) ≤ weight_Arf210_1A (out_lcr W .Conservative d cv) := by
  rw [weight_eq W _ d cv h.own, weight_eq W _ d cv h.own]
  apply rateF_mono _ _ _ _ _ _ _ _ (class_le W d.holder h.no_sme) (horizon_le W d) (fun hc => stability_mono W d cv hc)
  unfold ins
  intro hc
  simp only [Bool.and_eq_true] at hc ⊢
  exact ⟨res_false _ hc.1, hc.2⟩

theorem asf_cover (W : World) (m : Mode) (d : Deposit) (h : Prudent W d) :
    factor_Arf210_6 (out_nsfr W m d false) ≤ factor_Arf210_6 (out_nsfr W m d true) := by
  rw [factor_eq W m d true (h.bucket_le _), factor_eq W m d false (h.bucket_le _)]
  exact asfF_cover _ _ _ _ (by rw [stability_eq, stability_eq]; exact stab_cover _ _ _ _ _)

theorem asf_mono (W : World) (d : Deposit) (h : Prudent W d) (cv : Bool) :
    factor_Arf210_6 (out_nsfr W .Conservative d cv) ≤ factor_Arf210_6 (out_nsfr W .Optimistic d cv) := by
  rw [factor_eq W _ d cv (h.bucket_le _), factor_eq W _ d cv (h.bucket_le _)]
  exact asfF_mono _ _ _ _ _ _ (class_le W d.holder h.no_sme) (fun hc => stability_mono W d cv hc)
    (h.bucket_mono _ _ (earliest_le W d))

theorem covered_props (W : World) (d : Deposit) (h : Prudent W d) :
    0 ≤ W.covered .Conservative d ∧ W.covered .Conservative d ≤ W.covered .Optimistic d
      ∧ W.covered .Optimistic d ≤ d.balance := by
  have hl := class_le W d.holder h.no_sme
  have hf := res_false d.fcs_eligible
  have hb := h.bal
  have hm := h.limit
  rw [h.cover, h.cover]
  unfold clsLe at hl
  rcases hl with e | ⟨eo, ec⟩
  · rw [e]
    by_cases hr : isRetail («class» W .Conservative d.holder) = true <;>
      by_cases a : resolve .Conservative d.fcs_eligible false true = true <;>
      by_cases b : resolve .Optimistic d.fcs_eligible false true = true <;> simp_all <;> omega
  · rw [eo, ec]
    by_cases b : resolve .Optimistic d.fcs_eligible false true = true <;> simp_all [isRetail] <;> omega

theorem unknowns_are_prudent_deposit (W : World) (d : Deposit) (h : Prudent W d) :
    measure_Arf210_1A W .Optimistic d ≤ measure_Arf210_1A W .Conservative d
      ∧ measure_Arf210_6 W .Conservative d ≤ measure_Arf210_6 W .Optimistic d := by
  obtain ⟨c0, ccd, cdb⟩ := covered_props W d h
  unfold measure_Arf210_1A measure_Arf210_6 portion
  simp only [ite_true, Bool.false_eq_true, ite_false]
  constructor
  · exact shift_le _ _ _ _ _ _ _ c0 ccd cdb (rate_mono W d h true) (rate_mono W d h false)
      (rate_cover W .Conservative d h).2
  · exact shift_ge _ _ _ _ _ _ _ c0 ccd cdb (asf_mono W d h true) (asf_mono W d h false)
      (asf_cover W .Optimistic d h)

-- ------------------------------------------------------------------ books

def total (g : Deposit → Int) : List Deposit → Int
  | [] => 0
  | d :: ds => g d + total g ds

/-- **theorem unknowns_are_prudent** (`RetailSme`): in a book with one
    deposit per holder and no SME, missing data never lowers the reported LCR
    outflow (ARF 210.1A) and never raises the reported ASF (ARF 210.6). -/
theorem unknowns_are_prudent (W : World) (ds : List Deposit) (h : ∀ d ∈ ds, Prudent W d) :
    total (measure_Arf210_1A W .Optimistic) ds ≤ total (measure_Arf210_1A W .Conservative) ds
      ∧ total (measure_Arf210_6 W .Conservative) ds ≤ total (measure_Arf210_6 W .Optimistic) ds := by
  induction ds with
  | nil => simp [total]
  | cons d ds ih =>
    have hd := unknowns_are_prudent_deposit W d (h d (by simp))
    have ht := ih (fun e he => h e (by simp [he]))
    simp only [total]
    omega

-- ------------------------------------------------------------------ the SME cliff

/-- The world after the SME's group crossed AUD 2m: group totals and the
    `optimize` solution change; nothing else does. -/
def crossed (W : World) (groups : Mode → Option GroupId → SmeGroups_row) (cover : Mode → Deposit → Int) : World :=
  { W with SmeGroups := groups, covered := cover }

theorem class_crossed (W : World) (m : Mode) (G : Mode → Option GroupId → SmeGroups_row) (C : Mode → Deposit → Int)
    (c : Customer) (hk : c.kind = .Sme) (hover : ∀ m g, W.sme_threshold ≤ (G m g).total) :
    «class» (crossed W G C) m c = .NonFinancialCorporate := by
  unfold «class» crossed
  rw [hk]
  simp only [pcmp, ToOpt.toOpt]
  have hn := Int.not_lt.mpr (hover m (resolve m (U.map some c.sme_group) none (some (W.solo c))))
  simp [hn]

theorem class_sme_ne_fi (W : World) (m : Mode) (c : Customer) (hk : c.kind = .Sme) :
    «class» W m c ≠ .FinancialInstitution := by
  unfold «class»
  simp only [hk]
  split <;> simp

/-- **theorem sme_cliff_never_increases_stable_funding** (`RetailSme`), per
    deposit: when the SME's group total crosses AUD 2m, the deposit's ASF does
    not increase, in either mode, whatever the FCS cover before. -/
theorem sme_cliff_deposit (W : World) (m : Mode) (G : Mode → Option GroupId → SmeGroups_row)
    (C : Mode → Deposit → Int) (d : Deposit) (hk : d.holder.kind = .Sme)
    (hover : ∀ m g, W.sme_threshold ≤ (G m g).total)
    (hcov : 0 ≤ W.covered m d ∧ W.covered m d ≤ d.balance) (hb : ∀ a, W.bucket a ≤ 2) :
    measure_Arf210_6 (crossed W G C) m d ≤ measure_Arf210_6 W m d := by
  have hc := class_crossed W m G C d.holder hk hover
  have he : earliest_redemption (crossed W G C) m d = earliest_redemption W m d := rfl
  have hbk : (crossed W G C).bucket = W.bucket := rfl
  unfold measure_Arf210_6 portion
  simp only [ite_true, Bool.false_eq_true, ite_false]
  rw [factor_eq _ m d true (by rw [he, hbk]; exact hb _), factor_eq _ m d false (by rw [he, hbk]; exact hb _),
    factor_eq W m d true (hb _), factor_eq W m d false (hb _), hc, he, hbk]
  have after : ∀ s s' : Stability, ∀ x : Int,
      asfF .NonFinancialCorporate s (W.bucket (earliest_redemption W m d)) * x
        + asfF .NonFinancialCorporate s' (W.bucket (earliest_redemption W m d)) * (d.balance - x)
        = asfF .NonFinancialCorporate s (W.bucket (earliest_redemption W m d)) * d.balance := by
    intro s s' x
    unfold asfF
    rw [← Int.mul_add]
    congr 1
    omega
  rw [after]
  have hfi := class_sme_ne_fi W m d.holder hk
  exact split_ge _ _ _ _ _ hcov.1 hcov.2 (asfF_retail_ge_nfc _ _ _ _ hfi) (asfF_retail_ge_nfc _ _ _ _ hfi)

/-- Book level: every deposit of the SMEs whose group crossed the threshold. -/
theorem sme_cliff_never_increases_stable_funding (W : World) (m : Mode)
    (G : Mode → Option GroupId → SmeGroups_row) (C : Mode → Deposit → Int) (ds : List Deposit)
    (hover : ∀ m g, W.sme_threshold ≤ (G m g).total) (hb : ∀ a, W.bucket a ≤ 2)
    (h : ∀ d ∈ ds, d.holder.kind = .Sme ∧ 0 ≤ W.covered m d ∧ W.covered m d ≤ d.balance) :
    total (measure_Arf210_6 (crossed W G C) m) ds ≤ total (measure_Arf210_6 W m) ds := by
  induction ds with
  | nil => simp [total]
  | cons d ds ih =>
    obtain ⟨a, b, c⟩ := h d (by simp)
    have hd := sme_cliff_deposit W m G C d a hover ⟨b, c⟩ hb
    have ht := ih (fun e he => h e (by simp [he]))
    simp only [total]
    omega

end RetailAndSmeDeposits
