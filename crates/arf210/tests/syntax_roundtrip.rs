//! Printing a form as Prudence source and parsing it back yields the same IR.

use prudence_core::syntax::{parse_form, print::print_form};

#[test]
fn every_form_round_trips_through_prudence_source() {
    for f in arf210::forms() {
        let src = print_form(f, &arf210::SPEC_HEADER);
        let back = parse_form(&src).unwrap_or_else(|e| panic!("{}: {e}", f.code));
        assert_eq!(back.code, f.code);
        assert_eq!(back.title, f.title);
        assert_eq!(back.columns, f.columns);
        assert_eq!(back.default_input_col, f.default_input_col);
        assert_eq!(format!("{:?}", back.items), format!("{:?}", f.items), "{}", f.code);
        assert_eq!(format!("{:?}", back.checks), format!("{:?}", f.checks), "{}", f.code);
    }
}
