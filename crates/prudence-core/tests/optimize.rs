//! `optimize` blocks are general linear programs: any linear objective and
//! any conjunction of linear <=, >=, == constraints (incl. `forall`).

use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::num::parse_decimal;
use prudence_core::syntax::ast::*;
use prudence_core::syntax::validate_module;
use prudence_core::uncertain::Mode;

fn named(name: &str, generics: Vec<Type>, params: Vec<TypeParam>) -> Type {
    Type::Named { name: name.into(), generics, params, pos: Pos::default() }
}

fn id(s: &str) -> Expr {
    Expr::Ident(s.into(), Pos::default())
}

fn aud(n: &str) -> Expr {
    Expr::Member(Box::new(Expr::Num(n.into(), Pos::default())), "AUD".into())
}

fn knapsack_module(budget: &str) -> Module {
    let take_i = || Expr::Index(Box::new(id("take")), Box::new(id("i")));
    let owner_items = || Expr::Member(Box::new(id("owner")), "items".into());
    let sum_comp = |body: Expr| {
        Expr::Call(
            Box::new(id("sum")),
            vec![Arg {
                name: None,
                value: Expr::Comprehension {
                    var: "i".into(),
                    source: Box::new(owner_items()),
                    filter: None,
                    body: Some(Box::new(body)),
                },
            }],
        )
    };
    let aud_ty = || named("Money", vec![named("AUD", vec![], vec![]), named("Carrying", vec![], vec![])], vec![]);
    let budget_expr = if let Some(rest) = budget.strip_prefix('-') {
        Expr::Unary(UnOp::Neg, Box::new(aud(rest)))
    } else {
        aud(budget)
    };

    Module {
        name: "Knapsack".into(),
        params: vec![],
        anns: vec![],
        decls: vec![
            Decl {
                name: "Owner".into(),
                kind: DeclKind::Data {
                    base: None,
                    fields: vec![Field {
                        name: "items".into(),
                        ty: named("List", vec![named("Item", vec![], vec![])], vec![]),
                        anns: vec![],
                    }],
                },
                anns: vec![],
                pos: Pos::default(),
            },
            Decl {
                name: "Item".into(),
                kind: DeclKind::Data {
                    base: None,
                    fields: vec![
                        Field { name: "owner".into(), ty: named("Owner", vec![], vec![]), anns: vec![] },
                        Field { name: "value".into(), ty: aud_ty(), anns: vec![] },
                        Field { name: "weight".into(), ty: named("Rate", vec![], vec![]), anns: vec![] },
                    ],
                },
                anns: vec![],
                pos: Pos::default(),
            },
            Decl {
                name: "take".into(),
                kind: DeclKind::Alloc {
                    ty: named(
                        "ConservationSplit",
                        vec![],
                        vec![
                            TypeParam::Expr(Expr::Member(Box::new(id("Item")), "value".into())),
                            TypeParam::Field("per".into(), named("owner", vec![], vec![])),
                        ],
                    ),
                    body: Expr::Optimize {
                        per: "owner".into(),
                        filter: None,
                        sense: Sense::Maximize,
                        objective: Box::new(sum_comp(take_i())),
                        constraints: vec![
                            Expr::Binary(BinOp::Le, Box::new(sum_comp(take_i())), Box::new(budget_expr)),
                            Expr::Binary(
                                BinOp::Le,
                                Box::new(sum_comp(Expr::Binary(
                                    BinOp::Mul,
                                    Box::new(Expr::Member(Box::new(id("i")), "weight".into())),
                                    Box::new(take_i()),
                                ))),
                                Box::new(aud("20")),
                            ),
                            Expr::Forall {
                                var: "i".into(),
                                domain: Domain::In(Box::new(owner_items())),
                                body: Box::new(Expr::Binary(
                                    BinOp::And,
                                    Box::new(Expr::Binary(BinOp::Le, Box::new(aud("0")), Box::new(take_i()))),
                                    Box::new(Expr::Binary(
                                        BinOp::Le,
                                        Box::new(take_i()),
                                        Box::new(Expr::Member(Box::new(id("i")), "value".into())),
                                    )),
                                )),
                            },
                        ],
                    },
                },
                anns: vec![],
                pos: Pos::default(),
            },
            Decl {
                name: "partition".into(),
                kind: DeclKind::Partition {
                    var: "i".into(),
                    source: named("Item", vec![], vec![]),
                    targets: vec!["Out".into()],
                    split: Expr::Member(Box::new(id("i")), "value".into()),
                    split_var: "taken".into(),
                    split_by: take_i(),
                    more_splits: vec![],
                    cases: vec![Case {
                        cond: id("true"),
                        lets: vec![],
                        outputs: vec![("out".into(), id("taken"))],
                    }],
                },
                anns: vec![],
                pos: Pos::default(),
            },
        ],
    }
}

fn world(weight_a: &str) -> World {
    let mut w = World::default();
    let o = w.insert("Owner", Record { id: "o".into(), fields: vec![("items".into(), Val::List(vec![]))] });
    let mk = |id: &str, wt: &str| Record {
        id: id.into(),
        fields: vec![
            ("owner".into(), o.clone()),
            ("value".into(), Val::num(80)),
            ("weight".into(), Val::Num(parse_decimal(wt).unwrap())),
        ],
    };
    let a = w.insert("Item", mk("a", weight_a));
    let b = w.insert("Item", mk("b", "0.1"));
    w.set_field("Owner", 0, "items", Val::List(vec![a, b]));
    w
}

fn run(w: &World) -> Result<Vec<(String, bool, i64)>, String> {
    let m = validate_module(knapsack_module("100"), &["Out"]).map_err(|e| e.join("\n"))?;
    let opts = Options { mode: Mode::Conservative, sense: None, params: vec![] };
    let out = Interp::new(&m, w, opts)?.run()?;
    Ok(out
        .slices
        .iter()
        .map(|s| (s.subject.clone(), s.split, i64::try_from(s.amount.to_integer()).unwrap()))
        .collect())
}

#[test]
fn coupled_budgets_with_ties_are_solved_exactly() {
    let slices = run(&world("0.5")).unwrap();
    assert_eq!(
        slices,
        vec![("a".into(), true, 25), ("a".into(), false, 55), ("b".into(), true, 75), ("b".into(), false, 5)]
    );
}

#[test]
fn infeasible_programs_are_reported() {
    let m = validate_module(knapsack_module("-1"), &["Out"]).unwrap();
    let w = world("0.5");
    let opts = Options { mode: Mode::Conservative, sense: None, params: vec![] };
    let err = Interp::new(&m, &w, opts).unwrap().run().unwrap_err();
    assert!(err.contains("Infeasible"), "{err}");
}
