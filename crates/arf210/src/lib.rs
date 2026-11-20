//! ARF 210 reporting forms (ARS 210.0), defined as Prudence specs in
//! `specs/arf210/*.prd` and evaluated by `prudence-core`.

use std::sync::OnceLock;

use prudence_core::Form;
use prudence_core::syntax::parse_form;

pub mod crossform;

/// Header comment for the canonical spec files.
pub const SPEC_HEADER: [&str; 3] = [
    "Prudence form spec. Source of truth: edit this file, not code.",
    "Source: ARS 210.0 (F2023L00417) forms and instructions; APS 210 (July 2025).",
    "Format with: cargo run -p arf210 -- fmt <form>",
];

/// (form code, source) of every form spec.
pub const SPECS: [(&str, &str); 8] = [
    ("ARF 210.1A", include_str!("../../../specs/arf210/arf_210_1a.prd")),
    ("ARF 210.1B", include_str!("../../../specs/arf210/arf_210_1b.prd")),
    ("ARF 210.2", include_str!("../../../specs/arf210/arf_210_2.prd")),
    ("ARF 210.3.1", include_str!("../../../specs/arf210/arf_210_3_1.prd")),
    ("ARF 210.3.2", include_str!("../../../specs/arf210/arf_210_3_2.prd")),
    ("ARF 210.4", include_str!("../../../specs/arf210/arf_210_4.prd")),
    ("ARF 210.5", include_str!("../../../specs/arf210/arf_210_5.prd")),
    ("ARF 210.6", include_str!("../../../specs/arf210/arf_210_6.prd")),
];

/// All forms, parsed and statically validated once.
pub fn forms() -> &'static [Form] {
    static FORMS: OnceLock<Vec<Form>> = OnceLock::new();
    FORMS.get_or_init(|| {
        SPECS
            .iter()
            .map(|(code, src)| {
                let f = parse_form(src).unwrap_or_else(|e| panic!("{code}: {e}"));
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

/// The ARF 210.1A weights and ARF 210.6 factors (whole percentages) that
/// measure a module's partition in its generated Lean model.
pub fn lean_targets() -> Result<Vec<(String, prudence_core::syntax::lean::Measure)>, String> {
    use prudence_core::ir::{Cell, Expr};
    use prudence_core::syntax::lean::Measure;
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
    let mut weights = BTreeMap::new();
    for it in &lcr_all().items {
        if let Some(q) = constant(it.cell("weight")) {
            weights.insert(it.id.clone(), pct(&q)?);
        }
    }
    let mut factors = BTreeMap::new();
    for it in &nsfr().items {
        for (b, col) in ["f0", "f1", "f2"].iter().enumerate() {
            if let Some(q) = constant(it.cell(col)) {
                factors.insert((it.id.clone(), b as u32), pct(&q)?);
            }
        }
    }
    Ok(vec![("Arf210_1A".into(), Measure::Weight(weights)), ("Arf210_6".into(), Measure::Factor(factors))])
}
