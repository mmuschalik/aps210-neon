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
pub const SPECS: [(&str, &str); 4] = [
    ("ARF 210.1A", include_str!("../../../specs/arf210/arf_210_1a.prd")),
    ("ARF 210.1B", include_str!("../../../specs/arf210/arf_210_1b.prd")),
    ("ARF 210.2", include_str!("../../../specs/arf210/arf_210_2.prd")),
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

pub use prudence_core::Missing;
