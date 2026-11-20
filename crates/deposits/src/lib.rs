//! APS 210 step 2: raw deposit data -> ARF 210 form inputs, with provenance,
//! conservation checks and conservative handling of missing data.

pub mod classify;
pub mod model;
pub mod prudence;
#[doc(hidden)]
pub mod testgen;

pub use classify::{Classification, ClassifyError, FcsAllocation, Params, Slice, classify};
pub use model::{Book, Contract, Customer, CustomerKind, DepositAccount, EarlyWithdrawal, EventKind, Ledger};

use prudence_core::uncertain::Mode;
use prudence_core::money::AudCarrying;

/// Cost of missing data: reported LCR outflow minus the outflow if every unknown
/// attribute were favourable. Usually positive. It can be negative where the
/// evidence-based default happens to carry a lower LCR rate: an SME that can't
/// be shown to qualify for retail is wholesale (fully insured: 20%), which is
/// below a high-scoring retail deposit (25%).
pub fn data_quality_cost(book: &Book, params: &Params) -> Result<AudCarrying, ClassifyError> {
    let conservative = classify(book, &Params { mode: Mode::Conservative, ..params.clone() })?;
    let optimistic = classify(book, &Params { mode: Mode::Optimistic, ..params.clone() })?;
    Ok(conservative.lcr_outflow() - optimistic.lcr_outflow())
}
