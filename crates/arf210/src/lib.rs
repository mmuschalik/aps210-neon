//! ARF 210 reporting forms (ARS 210.0), defined in the Lean 4 Prudence DSL
//! (`lean/Prudence/Specs/Forms/*.lean`), exported to JSON IR (`specs/ir/forms/*.json`),
//! and evaluated by `prudence-core`.

use std::sync::OnceLock;

use prudence_core::Form;
use prudence_core::syntax::load_form_json;

pub mod crossform;

/// (form code, JSON IR source) of every form spec.
pub const SPECS: [(&str, &str); 8] = [
    ("ARF 210.1A", include_str!("../../../specs/ir/forms/arf_210_1a.json")),
    ("ARF 210.1B", include_str!("../../../specs/ir/forms/arf_210_1b.json")),
    ("ARF 210.2", include_str!("../../../specs/ir/forms/arf_210_2.json")),
    ("ARF 210.3.1", include_str!("../../../specs/ir/forms/arf_210_3_1.json")),
    ("ARF 210.3.2", include_str!("../../../specs/ir/forms/arf_210_3_2.json")),
    ("ARF 210.4", include_str!("../../../specs/ir/forms/arf_210_4.json")),
    ("ARF 210.5", include_str!("../../../specs/ir/forms/arf_210_5.json")),
    ("ARF 210.6", include_str!("../../../specs/ir/forms/arf_210_6.json")),
];

/// All forms, loaded from Lean-exported IR JSON and statically validated once.
pub fn forms() -> &'static [Form] {
    static FORMS: OnceLock<Vec<Form>> = OnceLock::new();
    FORMS.get_or_init(|| {
        SPECS
            .iter()
            .map(|(code, src)| {
                let f = load_form_json(src).unwrap_or_else(|e| panic!("{code}: {e}"));
                assert_eq!(&f.code, code, "spec file declares the wrong form code");
                f
            })
            .collect()
    })
}

/// Look up a form by code, accepting "ARF 210.1A", "210.1A" or "210.1a".
pub fn form(code: &str) -> Option<&'static Form> {
    let want = code.trim().to_ascii_uppercase();
    let want = want.strip_prefix("ARF").unwrap_or(&want).trim();
    forms().iter().find(|f| f.code.trim_start_matches("ARF ").eq_ignore_ascii_case(want))
}

pub fn lcr_all() -> &'static Form {
    form("210.1A").unwrap()
}
pub fn lcr_aud() -> &'static Form {
    form("210.1B").unwrap()
}
pub fn mlh() -> &'static Form {
    form("210.2").unwrap()
}
pub fn nsfr() -> &'static Form {
    form("210.6").unwrap()
}
pub fn maturity_assets() -> &'static Form {
    form("210.3.1").unwrap()
}
pub fn maturity_liabilities() -> &'static Form {
    form("210.3.2").unwrap()
}
pub fn funding_plan() -> &'static Form {
    form("210.4").unwrap()
}
pub fn daily() -> &'static Form {
    form("210.5").unwrap()
}

pub use prudence_core::Missing;

/// NSFR factors (whole percentages) per (item id, maturity bucket).
pub fn nsfr_factors() -> Result<std::collections::BTreeMap<(String, u32), i64>, String> {
    use prudence_core::ir::{Cell, Expr};
    use std::collections::BTreeMap;
    let pct = |q: &prudence_core::Q| -> Result<i64, String> {
        let p = q * prudence_core::Q::from_integer(100.into());
        if !p.is_integer() {
            return Err(format!("{q} is not a whole percentage"));
        }
        num_traits::ToPrimitive::to_i64(&p.to_integer()).ok_or("overflow".into())
    };
    let constant = |c: Option<&Cell>| match c {
        Some(Cell::Derived(Expr::Const(q))) => Some(q.clone()),
        _ => None,
    };
    let mut factors = BTreeMap::new();
    for it in &nsfr().items {
        for (b, col) in ["f0", "f1", "f2"].iter().enumerate() {
            if let Some(q) = constant(it.cell(col)) {
                factors.insert((it.id.clone(), b as u32), pct(&q)?);
                if let Some(prudence_core::ir::Cell::Derived(e)) = it.cell(&format!("b{b}")) {
                    let mut refs = Vec::new();
                    e.refs(&mut refs);
                    for r in refs.iter().filter(|r| r.col == "amount") {
                        factors.insert((r.item.clone(), 9), pct(&q)?);
                    }
                }
            }
        }
    }
    Ok(factors)
}
