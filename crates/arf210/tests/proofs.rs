//! Form theorems are proven by Z3 over all inputs.

use prudence_core::syntax::prove::{prove_form, theorem_script, z3};

#[test]
fn every_form_theorem_is_proven() {
    let mut n = 0;
    for f in arf210::forms() {
        for o in prove_form(f).unwrap() {
            assert!(o.proven, "{} NOT proven; counterexample:\n{}", o.obligation, o.counterexample);
            n += 1;
        }
    }
    assert_eq!(n, 10, "4 + 4 LCR, 1 MLH, 1 NSFR theorems");
}

#[test]
fn a_false_theorem_gets_a_counterexample() {
    // Claim HQLA2B is at most 10% after the caps: false (the cap is 15%).
    let f = arf210::lcr_all();
    let t = &f.theorems.iter().find(|t| t.id == "hqla2b-at-most-15pct").unwrap().prop;
    let script = theorem_script(f, t).replace("(* (/ 3.0 20.0)", "(* (/ 1.0 10.0)");
    assert!(script.contains("(/ 1.0 10.0)"), "mutation did not apply");
    let (proven, model) = z3(&script).unwrap();
    assert!(!proven);
    assert!(model.contains("define-fun"), "{model}");
}

#[test]
fn cap_guarantees_need_no_hypotheses() {
    // ARS 210.0 floors adjusted HQLA1 (item 1.5) at zero but not adjusted
    // HQLA2A/2B (items 2.5, 3.4, 3.5). Even so, the 15% and 40% guarantees hold
    // for every possible return, including negative adjusted stocks: the
    // theorems are stated (and proven) with no hypotheses.
    let f = arf210::lcr_all();
    for id in ["hqla2b-at-most-15pct", "hqla2-at-most-40pct"] {
        let t = &f.theorems.iter().find(|t| t.id == id).unwrap().prop;
        assert!(!matches!(t, prudence_core::ir::Prop::Implies(..)), "{id} should be unconditional");
    }
}
