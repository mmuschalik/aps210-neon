//! The Prudence source for these rules (specs/deposits/retail_sme.prd) must
//! parse and every name in it must resolve.

use prudence_core::syntax::ast::DeclKind;
use prudence_core::syntax::parse_module;

const SRC: &str = include_str!("../../../specs/deposits/retail_sme.prd");
const FORMS: [&str; 4] = ["Arf210_1A", "Arf210_1B", "Arf210_2", "Arf210_6"];

#[test]
fn deposit_module_parses_and_resolves() {
    let m = parse_module(SRC, &FORMS).unwrap_or_else(|errs| panic!("\n{}", errs.join("\n")));
    assert_eq!(m.name, "RetailAndSmeDeposits");
    let count = |pred: fn(&DeclKind) -> bool| m.decls.iter().filter(|d| pred(&d.kind)).count();
    assert_eq!(count(|k| matches!(k, DeclKind::Fn { body: Some(_), .. })), 7);
    assert_eq!(count(|k| matches!(k, DeclKind::Theorem(_))), 2);
    let Some(DeclKind::Partition { cases, .. }) =
        m.decls.iter().map(|d| &d.kind).find(|k| matches!(k, DeclKind::Partition { .. }))
    else {
        panic!("no partition")
    };
    assert_eq!(cases.len(), 3);
}

#[test]
fn misspelled_names_are_caught_with_positions() {
    let bad = SRC.replace("SmeGroups[g]", "SmeGroup[g]").replace("=> Called", "=> Calld");
    let errs = parse_module(&bad, &FORMS).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("unknown name `SmeGroup`")), "{errs:?}");
    let bad = SRC.replace("into Arf210_1A, Arf210_6", "into Arf210_1A, Arf210_9");
    let errs = parse_module(&bad, &FORMS).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("unknown form `Arf210_9`")), "{errs:?}");
}

/// Each mutation breaks one static guarantee of the design; the checker must
/// reject it with a message naming the problem.
#[test]
fn type_checker_rejects_each_class_of_error() {
    let cases: [(&str, &str, &str); 8] = [
        // Unknown data used without conservative(..)
        ("conservative(d.online, worst: true)", "d.online", "unknown value"),
        // Money tags must match: market value vs carrying value
        ("param fcs_limit      : Money<AUD, Carrying>", "param fcs_limit      : Money<AUD, MarketValue>", "cannot compare"),
        // Values of different enums never compare
        ("case class(d.holder) == FinancialInstitution", "case class(d.holder) == Demand", "cannot compare"),
        // Fields must exist
        ("c.managed_as_retail, worst: false)", "c.managed_as_retaill, worst: false)", "no field `managed_as_retaill`"),
        // Match arms agree with the declared return type
        ("Stable       => 5%", "Stable       => \"5%\"", "match arms differ"),
        // Patterns only use the scrutinee type's constructors
        ("Smsf                     => RetailLessStable", "Demand                   => RetailLessStable", "does not match a value of type Kind"),
        // conservative(..)'s fallback must have the attribute's type
        ("best: solo(c)", "best: c", "must be a GroupId"),
        // A Bool condition can't be a number
        ("if n >= 31 then Excluded", "if n then Excluded", "expected Bool"),
    ];
    for (from, to, want) in cases {
        assert!(SRC.contains(from), "mutation anchor missing: {from}");
        let bad = SRC.replacen(from, to, 1);
        let errs = parse_module(&bad, &FORMS).err().unwrap_or_default();
        assert!(errs.iter().any(|e| e.contains(want)), "`{to}`: expected an error containing {want:?}, got {errs:#?}");
    }
}

/// Partition cases are exhaustive and pairwise disjoint, and every `match`
/// is exhaustive, for all values of the terms they test (Z3).
#[test]
fn module_proof_obligations_hold() {
    let m = parse_module(SRC, &FORMS).unwrap();
    let outcomes = prudence_core::syntax::prove::prove_module(&m).unwrap();
    for o in &outcomes {
        assert!(o.proven, "{} NOT proven; counterexample:\n{}", o.obligation, o.counterexample);
    }
    // 1 exhaustiveness + 3 disjointness for the partition, plus 7 matches:
    // class, horizon, earliest_redemption, lcr_rate (x2), partition lcr (x2).
    assert_eq!(outcomes.len(), 4 + 7, "{outcomes:#?}");
}

/// Remove a case and Z3 must find an uncovered depositor class.
#[test]
fn a_missing_partition_case_is_found() {
    let bad = SRC.replace(
        "case class(d.holder) == FinancialInstitution =>",
        "case class(d.holder) == FinancialInstitution && false =>",
    );
    let m = parse_module(&bad, &FORMS).unwrap();
    let outcomes = prudence_core::syntax::prove::prove_module(&m).unwrap();
    let exh = outcomes.iter().find(|o| o.obligation.contains("exhaustive") && o.obligation.starts_with("partition")).unwrap();
    assert!(!exh.proven);
    assert!(exh.counterexample.contains("Class_FinancialInstitution"), "{}", exh.counterexample);
}
