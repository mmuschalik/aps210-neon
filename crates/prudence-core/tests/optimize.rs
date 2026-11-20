//! `optimize` blocks are general linear programs: any linear objective and
//! any conjunction of linear <=, >=, == constraints (incl. `forall`).

use prudence_core::interp::{Interp, Options, Record, Val, World};
use prudence_core::num::parse_decimal;
use prudence_core::syntax::parse_module;
use prudence_core::uncertain::Mode;

const SRC: &str = r#"
module Knapsack {
  data Owner {
    items : List<Item>
  }
  data Item {
    owner  : Owner
    value  : Money<AUD, Carrying>
    weight : Rate
  }

  // Two coupled budgets: not the "one budget plus bounds" shape.
  alloc take : ConservationSplit(Item.value, per: owner) =
    optimize per owner {
      maximize sum(i in owner.items | take[i])
      subject to
        sum(i in owner.items | take[i]) <= 100.AUD
        sum(i in owner.items | i.weight * take[i]) <= 20.AUD
        forall i in owner.items, 0.AUD <= take[i] <= i.value
    }

  partition i: Item into Out splitting i.value by taken: take[i] {
    case true =>
      out: taken
  }
}
"#;

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
    let m = parse_module(SRC, &["Out"]).map_err(|e| e.join("\n"))?;
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
    // max a + b  s.t. a + b <= 100, 0.5a + 0.1b <= 20, 0 <= a, b <= 80.
    // Optimum 100, attained for a in [20, 25]; ties break towards earlier
    // subjects, so a = 25, b = 75.
    let slices = run(&world("0.5")).unwrap();
    assert_eq!(
        slices,
        vec![("a".into(), true, 25), ("a".into(), false, 55), ("b".into(), true, 75), ("b".into(), false, 5)]
    );
}

#[test]
fn infeasible_programs_are_reported() {
    // Make the bound constraint impossible: value < 0 is not allowed, so
    // force infeasibility with a negative budget instead.
    let src = SRC.replace("<= 100.AUD", "<= -1.AUD");
    let m = parse_module(&src, &["Out"]).unwrap();
    let w = world("0.5");
    let opts = Options { mode: Mode::Conservative, sense: None, params: vec![] };
    let err = Interp::new(&m, &w, opts).unwrap().run().unwrap_err();
    assert!(err.contains("Infeasible"), "{err}");
}
