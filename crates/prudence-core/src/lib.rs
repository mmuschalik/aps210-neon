//! Prudence core: the smallest useful slice of the Prudence language from
//! `design.txt` - a total expression IR over regulatory form cells, exact
//! rational arithmetic, a `.prd` spec parser with static validation, and an
//! evaluator.

pub mod cite;
pub mod date;
pub mod eval;
pub mod interp;
pub mod lp;
pub mod ir;
pub mod money;
pub mod num;
pub mod syntax;
pub mod uncertain;
pub mod upstream;

pub use eval::{Evaluation, Finding, InputError, Missing, Value, evaluate};
pub use ir::{Cell, Expr, Form, Item, Ref};
pub use num::Q;
pub use syntax::{load_file_json, load_form_json, load_module_json, validate_module};
