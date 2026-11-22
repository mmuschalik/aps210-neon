//! The Lean-exported IR for the retail/SME deposit rules must load, resolve,
//! type-check, and pass all Z3 proof obligations; mutations breaking each static
//! guarantee must be rejected.

use prudence_core::syntax::ast::{BinOp, DeclKind, Expr, Pat, Pos, Type};
use prudence_core::syntax::{load_module_json, validate_module};

const SRC: &str = include_str!("../../../specs/ir/modules/retail_sme.json");
const FORMS: [&str; 4] = ["Arf210_1A", "Arf210_1B", "Arf210_2", "Arf210_6"];

fn base_module() -> prudence_core::syntax::ast::Module {
    load_module_json(SRC, &FORMS).unwrap_or_else(|errs| panic!("\n{}", errs.join("\n")))
}

#[test]
fn deposit_module_loads_and_resolves() {
    let m = base_module();
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
fn misspelled_names_are_caught() {
    let mut m = base_module();
    for d in &mut m.decls {
        if d.name == "horizon" {
            if let DeclKind::Fn { body: Some(body), .. } = &mut d.kind {
                *body = Expr::Ident("Calld".into(), Pos::default());
            }
        }
    }
    let errs = validate_module(m, &FORMS).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("unknown name `Calld`")), "{errs:?}");

    let mut m = base_module();
    for d in &mut m.decls {
        if let DeclKind::Partition { targets, .. } = &mut d.kind {
            targets[1] = "Arf210_9".into();
        }
    }
    let errs = validate_module(m, &FORMS).unwrap_err();
    assert!(errs.iter().any(|e| e.contains("unknown form `Arf210_9`")), "{errs:?}");
}

/// Each mutation breaks one static guarantee of the design; the checker must
/// reject it with a message naming the problem.
#[test]
fn type_checker_rejects_each_class_of_error() {
    // 1. Unknown data used without conservative(..)
    {
        let mut m = base_module();
        for d in &mut m.decls {
            if d.name == "score" {
                if let DeclKind::Fn { body: Some(body), .. } = &mut d.kind {
                    *body = Expr::If {
                        cond: Box::new(Expr::Member(Box::new(Expr::Ident("d".into(), Pos::default())), "online".into())),
                        then: Box::new(Expr::Num("2".into(), Pos::default())),
                        other: Box::new(Expr::Num("0".into(), Pos::default())),
                    };
                }
            }
        }
        let errs = validate_module(m, &FORMS).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("unknown value")), "{errs:#?}");
    }
    // 2. Money tags must match: market value vs carrying value
    {
        let mut m = base_module();
        for d in &mut m.decls {
            if d.name == "fcs_limit" {
                if let DeclKind::Param { ty: Type::Named { generics, .. }, .. } = &mut d.kind {
                    if let Type::Named { name, .. } = &mut generics[1] {
                        *name = "MarketValue".into();
                    }
                }
            }
        }
        let errs = validate_module(m, &FORMS).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("cannot compare") || e.contains("Money")), "{errs:#?}");
    }
    // 3. Values of different enums never compare
    {
        let mut m = base_module();
        for d in &mut m.decls {
            if let DeclKind::Partition { cases, .. } = &mut d.kind {
                cases[2].cond = Expr::Binary(
                    BinOp::Eq,
                    Box::new(Expr::Call(
                        Box::new(Expr::Ident("class".into(), Pos::default())),
                        vec![prudence_core::syntax::ast::Arg {
                            name: None,
                            value: Expr::Member(Box::new(Expr::Ident("d".into(), Pos::default())), "holder".into()),
                        }],
                    )),
                    Box::new(Expr::Ident("Demand".into(), Pos::default())),
                );
            }
        }
        let errs = validate_module(m, &FORMS).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("cannot compare")), "{errs:#?}");
    }
    // 4. Fields must exist
    {
        let mut m = base_module();
        for d in &mut m.decls {
            if d.name == "score" {
                if let DeclKind::Fn { body: Some(body), .. } = &mut d.kind {
                    *body = Expr::Member(Box::new(Expr::Ident("d".into(), Pos::default())), "nonexistent_field".into());
                }
            }
        }
        let errs = validate_module(m, &FORMS).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("no field `nonexistent_field`")), "{errs:#?}");
    }
    // 5. Patterns only use the scrutinee type's constructors
    {
        let mut m = base_module();
        for d in &mut m.decls {
            if d.name == "class" {
                if let DeclKind::Fn { body: Some(Expr::Match { arms, .. }), .. } = &mut d.kind {
                    arms[1].pat = Pat::Ctor("Demand".into(), vec![]);
                }
            }
        }
        let errs = validate_module(m, &FORMS).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("does not match a value of type Kind")), "{errs:#?}");
    }
    // 6. A Bool condition can't be a number
    {
        let mut m = base_module();
        for d in &mut m.decls {
            if d.name == "score" {
                if let DeclKind::Fn { body: Some(body), .. } = &mut d.kind {
                    *body = Expr::If {
                        cond: Box::new(Expr::Num("42".into(), Pos::default())),
                        then: Box::new(Expr::Num("1".into(), Pos::default())),
                        other: Box::new(Expr::Num("0".into(), Pos::default())),
                    };
                }
            }
        }
        let errs = validate_module(m, &FORMS).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("expected Bool")), "{errs:#?}");
    }
}

/// Partition cases are exhaustive and pairwise disjoint, and every `match`
/// is exhaustive, for all values of the terms they test (Z3).
#[test]
fn module_proof_obligations_hold() {
    let m = base_module();
    let outcomes = prudence_core::syntax::prove::prove_module(&m).unwrap();
    for o in &outcomes {
        assert!(o.proven, "{} NOT proven; counterexample:\n{}", o.obligation, o.counterexample);
    }
    assert_eq!(outcomes.len(), 4 + 7, "{outcomes:#?}");
}

/// Remove a case and Z3 must find an uncovered depositor class.
#[test]
fn a_missing_partition_case_is_found() {
    let mut m = base_module();
    for d in &mut m.decls {
        if let DeclKind::Partition { cases, .. } = &mut d.kind {
            cases[2].cond = Expr::Binary(
                BinOp::And,
                Box::new(cases[2].cond.clone()),
                Box::new(Expr::Ident("false".into(), Pos::default())),
            );
        }
    }
    let m = validate_module(m, &FORMS).unwrap();
    let outcomes = prudence_core::syntax::prove::prove_module(&m).unwrap();
    let exh = outcomes.iter().find(|o| o.obligation.contains("exhaustive") && o.obligation.starts_with("partition")).unwrap();
    assert!(!exh.proven);
    assert!(exh.counterexample.contains("Class_FinancialInstitution"), "{}", exh.counterexample);
}
