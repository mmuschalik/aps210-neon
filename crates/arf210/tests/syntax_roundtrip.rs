//! Every form IR exported from Lean 4 loads cleanly and has a generated DuckDB SQL script.

use std::path::Path;

#[test]
fn every_form_loads_from_lean_ir_and_has_sql() {
    assert_eq!(arf210::forms().len(), 8);
    let sql_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sql/forms");
    for stem in ["arf_210_1a", "arf_210_1b", "arf_210_2", "arf_210_3_1", "arf_210_3_2", "arf_210_4", "arf_210_5", "arf_210_6"] {
        let sql = std::fs::read_to_string(sql_dir.join(format!("{stem}.sql"))).unwrap();
        assert!(sql.contains("CREATE OR REPLACE VIEW"), "{stem}.sql missing view");
    }
}
