//! The Prudence surface language (design.txt §4.A): lexer, parser, name
//! resolution, lowering of `form` declarations to the core IR, and a printer.

pub mod ast;
pub mod lean;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod print;
pub mod prove;
pub mod resolve;
pub mod typecheck;

use crate::ir::Form;

/// Parse a file containing exactly one `form` and lower it to the core IR.
pub fn parse_form(src: &str) -> Result<Form, String> {
    let file = parser::parse_file(src)?;
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

/// Parse a file containing exactly one `module` and resolve its names.
pub fn parse_module(src: &str, known_forms: &[&str]) -> Result<ast::Module, Vec<String>> {
    let file = parser::parse_file(src).map_err(|e| vec![e])?;
    let mut mods = file.items.into_iter().filter_map(|t| match t {
        ast::TopLevel::Module(m) => Some(m),
        _ => None,
    });
    let m = mods.next().ok_or_else(|| vec!["no `module` declaration".to_string()])?;
    resolve::resolve(&m, known_forms)?;
    typecheck::check(&m)?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Cell;

    const FORM: &str = r#"
// comment
@reporting("X")
form T("T", "Test form") {
  columns amount, weight, weighted
  input amount
  item "1" "Total" {
    amount = [1.1:amount] + [1.2:amount]
  } @cite("sum")
  item "1.1" "A" {
    amount = input
    weight = 0.85
    weighted = [1.1:amount] * [1.1:weight]
  }
  item "1.2" "B \"quoted\"" {
    amount = input(default: -2.5)
  }
  item "2" "Group" {}
  item "3" "Cap" {
    amount = max([1:amount] - 15 / 85 * [1.1:amount], 0)
    weighted = if [1:amount] > 10 then -[1.2:amount] else min(1, 2)
  }
  check "c1" "B small": [1.2:amount] <= [1:amount] @cite("ARS p.1")
}
"#;

    #[test]
    fn parses_and_lowers_a_form() {
        let f = parse_form(FORM).unwrap();
        assert_eq!(f.items.len(), 5);
        assert_eq!(f.item("1.2").unwrap().label, "B \"quoted\"");
        assert!(f.item("2").unwrap().header);
        assert_eq!(f.item("1").unwrap().cite.as_deref(), Some("sum"));
        assert_eq!(f.checks[0].cite.as_deref(), Some("ARS p.1"));
        let Cell::Derived(e) = f.item("3").unwrap().cell("amount").unwrap() else { panic!() };
        assert_eq!(e.to_string(), "max([1:amount] - 15 / 85 * [1.1:amount], 0)");
    }

    #[test]
    fn rejects_dangling_references_and_cycles() {
        let dangling = "form T(\"T\", \"t\") {\n columns amount\n input amount\n item \"1\" \"x\" {\n amount = [9:amount]\n }\n}";
        assert!(parse_form(dangling).unwrap_err().contains("missing cell"));
        let cycle = "form T(\"T\", \"t\") {\n columns amount\n input amount\n item \"1\" \"x\" { amount = [2:amount] }\n item \"2\" \"y\" { amount = [1:amount] }\n}";
        assert!(parse_form(cycle).unwrap_err().contains("cycle"));
    }

    #[test]
    fn reports_positions() {
        let bad = "form T(\"T\", \"t\") {\n columns amount\n input amount\n item \"1\" \"x\" {\n  amount = 1 +\n }\n}";
        assert!(parse_form(bad).unwrap_err().starts_with("6:"), "{}", parse_form(bad).unwrap_err());
    }

    #[test]
    fn newline_rule_separates_match_arms_from_calls() {
        let e = parser::parse_expr("match x {\n  A => f(1)\n  (B, C) => 2\n}").unwrap();
        let ast::Expr::Match { arms, .. } = e else { panic!() };
        assert_eq!(arms.len(), 2);
    }

    #[test]
    fn chained_comparison_desugars_to_and() {
        let e = parser::parse_expr("0 <= a <= b").unwrap();
        assert!(matches!(e, ast::Expr::Binary(ast::BinOp::And, ..)));
    }
}
