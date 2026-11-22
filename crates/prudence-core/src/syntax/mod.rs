//! Prudence IR loader, static validation (name resolution, type checking,
//! staging, form lowering), and Z3 proof obligations.
//!
//! Specifications are authored in the idiomatic Lean 4 DSL (`lean/Prudence/Specs/`)
//! and exported to JSON IR (`specs/ir/`) and DuckDB SQL (`sql/`) via
//! `cd lean && lake exe prudence-export ..`.

pub mod ast;
pub mod deps;
pub mod ir_json;
pub mod lower;
pub mod prove;
pub mod resolve;
pub mod typecheck;
pub mod witness;

use crate::ir::Form;

/// Load a JSON-encoded Prudence IR file (exported by the Lean 4 DSL).
pub fn load_file_json(json: &str) -> Result<ast::File, String> {
    ir_json::decode_file(json)
}

/// Load a JSON-encoded `form` IR and lower it to the core form IR.
pub fn load_form_json(json: &str) -> Result<Form, String> {
    let file = load_file_json(json)?;
    let mut forms = file.items.iter().filter_map(|t| match t {
        ast::TopLevel::Form(f) => Some(f),
        _ => None,
    });
    let f = forms.next().ok_or("no `form` declaration")?;
    if forms.next().is_some() {
        return Err("expected exactly one `form` per file".into());
    }
    lower::lower_form(f)
}

/// Load a JSON-encoded `module` IR, resolve names, type-check, and stage it.
pub fn load_module_json(json: &str, known_forms: &[&str]) -> Result<ast::Module, Vec<String>> {
    let file = load_file_json(json).map_err(|e| vec![e])?;
    let mut mods = file.items.into_iter().filter_map(|t| match t {
        ast::TopLevel::Module(m) => Some(m),
        _ => None,
    });
    let m = mods.next().ok_or_else(|| vec!["no `module` declaration".to_string()])?;
    validate_module(m, known_forms)
}

/// Validate a loaded or constructed `ast::Module`.
pub fn validate_module(m: ast::Module, known_forms: &[&str]) -> Result<ast::Module, Vec<String>> {
    resolve::resolve(&m, known_forms)?;
    typecheck::check(&m)?;
    deps::stages(&m).map_err(|e| vec![e])?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::ast::{CellDef, Expr, FormDecl, FormItem, Pos};

    #[test]
    fn rejects_dangling_references_and_cycles() {
        let p = Pos::default();
        let dangling = FormDecl {
            name: "T".into(),
            code: "T".into(),
            title: "t".into(),
            columns: vec!["amount".into()],
            input: "amount".into(),
            items: vec![FormItem {
                id: "1".into(),
                label: "x".into(),
                observer: false,
                cells: vec![("amount".into(), CellDef::Expr(Expr::Cell("9".into(), "amount".into(), p)))],
                anns: vec![],
                pos: p,
            }],
            checks: vec![],
            theorems: vec![],
            anns: vec![],
        };
        assert!(lower::lower_form(&dangling).unwrap_err().contains("missing cell"));

        let cycle = FormDecl {
            items: vec![
                FormItem {
                    id: "1".into(),
                    label: "x".into(),
                    observer: false,
                    cells: vec![("amount".into(), CellDef::Expr(Expr::Cell("2".into(), "amount".into(), p)))],
                    anns: vec![],
                    pos: p,
                },
                FormItem {
                    id: "2".into(),
                    label: "y".into(),
                    observer: false,
                    cells: vec![("amount".into(), CellDef::Expr(Expr::Cell("1".into(), "amount".into(), p)))],
                    anns: vec![],
                    pos: p,
                },
            ],
            ..dangling
        };
        assert!(lower::lower_form(&cycle).unwrap_err().contains("cycle"));
    }
}
