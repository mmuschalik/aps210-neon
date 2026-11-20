//! Prudence core: the smallest useful slice of the Prudence language from
//! `design.txt` - a total expression IR over regulatory form cells, exact
//! rational arithmetic, a `.prd` spec parser with static validation, and an
//! evaluator.

pub mod date;
pub mod eval;
pub mod interp;
pub mod lp;
pub mod ir;
pub mod money;
pub mod num;
pub mod syntax;
pub mod uncertain;

pub use eval::{Evaluation, Finding, InputError, Missing, Value, evaluate};
pub use ir::{Cell, Expr, Form, Item, Ref};
pub use num::Q;
pub use syntax::parse_form;
