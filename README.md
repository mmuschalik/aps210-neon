# aps210

APRA APS 210 (Liquidity) as a verified, executable specification, built
bottom-up towards the architecture in `design.txt` (Prudence language, Rust
engine, Lean/SMT proofs, tiny trusted checker).

Requires `z3` and `lean` (via elan) on PATH for the proof tests.

Not yet: the incremental (DBSP) engine, DuckDB stress runs, and the remaining
forms. The Lean backend compiles the deposit module; views, `optimize`
solutions and event queries stay abstract `World` fields rather than being
compiled.

```sh
cargo test                       # all tests
cargo run -p arf210              # list forms
cargo run -p arf210 -- 210.1A    # print a form spec for review against the PDF
```

## Layout

| Path | What |
|---|---|
| `specs/arf210/*.prd` | **Source of truth** for the ARF 210 forms, in Prudence syntax (`form` declarations): every cell is an input or an exact expression, with `@cite`. Canonical formatting: `cargo run -p arf210 -- fmt 210.1A`. |
| `specs/deposits/retail_sme.prd` | **Source of truth** for the step-2 deposit rules: a Prudence `module` in the design.txt §4.A syntax (`view`, `fn`/`match`, `conservative(..)`, `optimize`, `partition`, `invariant`, `theorem`). Parsed, name-resolved, type-checked, proof obligations discharged by Z3, and executed by the interpreter. |
| `crates/prudence-core` | Prudence: exact rationals (no floats anywhere); `syntax/lean` = Lean 4 backend; `lp` = exact two-phase simplex with a primal-dual certificate checker (every `optimize` is a general LP); `syntax/` (lexer, parser, name resolution, type checker, form lowering, printer, `prove` = Z3 proof obligations); the form IR and evaluator; `interp` = module interpreter (views, `optimize` with solution certificate, `partition` with run-time exhaustiveness, invariants, clause-level provenance). |
| `crates/arf210` | Loads the four specs; cross-form rules; CLI; parity and scenario tests. |
| `specs/secured/secured_funding.prd` | **Source of truth** for the LCR numerator: the stock of liquid assets (holdings -> ARF 210.1A items 1.1-3.3, Att A paras 9-12, 22-25; central bank balances only if drawable in stress), the CLF (items 6.1.x, 6.4, and CLF collateral in 11.5/19.1.5), secured funding and lending (repos, reverse repos as ACTUS REP -> items 11 and 19), and the mechanical 30-day unwind of the HQLA stock (`unwind`, Att A para 8). |
| `crates/secured` | Data model and loader for holdings and repos; `run()` produces the Section A, item 11 and item 19 inputs and the unwind per HQLA class, so the whole numerator (adjusted stocks, 15%/40% caps, item 4) is computed from raw data. `cargo run -p secured --bin secured-lean` regenerates `lean/Prudence/SecuredFunding.lean`. |
| `specs/deposits/wholesale.prd` | **Source of truth** for wholesale deposits (ARF 210.1A items 9.2-9.5, ARF 210.6 items 8-12): operational / excess / insured three-way split via a nested `partition ... splitting X by operational: ..., covered: ...`; unknown operational balance -> all excess (APG 210 para 135). Generated Lean model `lean/Prudence/Wholesale.lean` (n-way splits as a chain of portions; `cargo run -p deposits --bin wholesale-lean`), proofs in `lean/Prudence/WholesaleProofs.lean` (portions conserve the balance; unknown operational service or balance is all excess), differential `#guard`s against the interpreter. |
| `specs/loans/loans.prd` | **Source of truth** for loans as ACTUS contracts (ANN, LAM, PAM, UMP): contractual inflows within 30 days (ARF 210.1A items 20.1-20.6; performing only, open-maturity loans only their minimum payment) and NSFR RSF with every repayment bucketed by its own date (ARF 210.6 items 24, 35-37, 39; encumbrance per Att C para 23). Exact annuity schedules over rationals (`cycle`, `pow`). `crates/loans` holds the data model and `run()`. |
| `specs/derivatives/derivatives.prd` | **Source of truth** for derivatives and collateral: 30-day derivative cash flows netted only under an MNA or for same-day FX principal (ARF 210.1A items 13.1, 21.1; options exercised when in the money, unknown resolved per direction), collateral items 13.2-13.5 and 13.7, the 24-month lookback (13.6) as a bitemporal query over realised collateral flows, and NSFR replacement cost and margins (ARF 210.6 items 13.1.x, 40.1.x, 42.1). `crates/derivatives` holds the data model and `run()`. |
| `crates/assembly` | Return assembly: every module's per-record contributions, tagged with the record's currency, summed into ARF 210.1A; the currency split (items 18.x, 23.x) by evaluating the form per currency, with derivative flows under the ARS currency rule (FX funding swaps gross, FX trading excluded). The tie-out of the split to items 18 and 23 is a `check warn`, since the ARS rule means it need not hold. |
| `crates/checker` | The trusted verification gate. From plain data (ledger, records, slices, per-record contributions, lodged inputs, LP certificates, assumptions, spec sources) it re-derives and checks, without the interpreter, the solver or any module: GL tie-out, conservation of every split record, no double counting (stock vs inflows; contributions only to input cells), lodged inputs = contributions (including the currency split), LP primal-dual certificates, every form check and form theorem on the concrete return, and ARF 210.6 vs 210.1A. Only then does it emit the APS 310 audit bundle: SHA-256 (in-crate, FIPS 180-4 vectors) of the data snapshot, each spec and the spec version, certificates, assumptions and returns; the manifest hash is the bundle id. Standalone: `cargo run -p checker --bin prudence-check -- package.json --bundle bundle.json` (exit 0 and a bundle only if every check passes); the package is JSON with exact rationals as strings (`checker::json`). |
| `specs/obligations/commitments.prd` | **Source of truth** for off-balance sheet outflows: committed and uncommitted credit/liquidity facilities (items 14, 16.1; redraw as committed, funding vehicles as FI liquidity facilities), obligations to extend funds (item 15), and contingent funding obligations (items 16.2-16.9). `crates/obligations` holds the data model and `run()`. |
| `lean/Prudence/UnwindProofs.lean` | design.txt §4.A.4's theorem: for every portfolio, the mechanical unwind equals the ARF 210.1A adjustment formulas (items 1.5.1, 2.5.1, 3.4.1, 3.5.1, generated from the form spec) applied to the reported items. |
| `lean/Prudence/RetailSme.lean` | **Generated** Lean 4 model of `retail_sme.prd` (types, records, contracts, rules, partition outputs, and the ARF 210.1A / 210.6 measures from the form specs). Never edited by hand: `cargo run -p deposits --bin prudence-lean`. |
| `lean/Prudence/Proofs.lean` | Machine-checked proofs, about the generated model, of the module's two `theorem`s (`unknowns_are_prudent`, `sme_cliff_never_increases_stable_funding`); no `sorry`, standard axioms only. `cd lean && lake build`. |
| `crates/deposits` | Data model (ACTUS contract types, CDM-style bitemporal events, `Uncertain<T>` attributes), the loader into Prudence records, and `classify()`, which runs `retail_sme.prd` and maps its outputs onto the form specs. No hand-written rules. |

`prudence-core` also holds two design primitives used by step 2: `Money<Currency,
Basis>` (mixing market and carrying values is a compile error) and
`Uncertain<T>` with a logging `Resolver` (reported vs. optimistic runs; the
difference prices missing data).

Forms covered: ARF 210.1A (LCR all currencies), 210.1B (LCR AUD), 210.2 (MLH),
210.6 (NSFR). Not yet: 210.3.x, 210.4, 210.5, and the free-form currency table
in 210.1A item 35.

## How it was verified

- Item lists in each spec equal the lists extracted mechanically from
  `ars-210.0.pdf` (`crates/arf210/tests/fixtures`).
- The 23 derived-item formulas (images in the PDF) were transcribed and each is
  covered by a hand-worked scenario test.
- The specs were first implemented and tested in Python, then exported to
  `.prd`. The Rust evaluator reproduces every cell of 48 golden returns from that
  implementation (`crates/arf210/tests/golden`), after which the Python was
  retired.
- The deposit rules were first hand-written in Rust; the interpreted Prudence
  module matched them on 400 generated books x 8 configurations, then
  reproduced 11,725 golden slices (`crates/deposits/tests/golden_slices.tsv`)
  before the Rust rules were deleted.
- **Proven by Z3** (`cargo test`): every form `theorem` (HQLA2B <= 15% and
  HQLA2 <= 40% after the caps, for every possible return; caps only remove;
  75% inflow cap; MLH netting never adds; NSFR derivatives net on one side),
  and for the deposit module: partition cases exhaustive and pairwise
  disjoint, every `match` exhaustive.
- **Proven in Lean** (`lean/Prudence/Proofs.lean`, checked by `cargo test`):
  both deposit theorems, over all inputs, for whole books, about the Lean model
  **generated from the `.prd`** (`syntax/lean.rs`). What the generated `World`
  leaves abstract (the `optimize` solution, calendar buckets, event queries)
  appears as explicit hypotheses. Changing a rule in the spec breaks the proof.
  A differential test also checks the generated model against the interpreter
  on generated books.
- `optimize` blocks are solved as general LPs by an exact simplex; each
  solution's primal-dual certificate is verified, and ties between optimal
  solutions break towards earlier subjects. The FCS allocation reproduces the
  golden results exactly.
- The type checker is tested by mutation: unresolved unknowns, mixed money
  bases, cross-enum comparisons, missing fields, mistyped arms/patterns/
  fallbacks and non-Bool conditions are each rejected.

## Sources and versions

- APS 210 and APG 210: July 2025.
- ARS 210.0: March 2023 (determination No. 54 of 2023, F2023L00417). Checked on
  legislation.gov.au: still the only in-force version, no known amendments. Some
  cross-references use pre-2025 APS paragraph numbers (e.g. item 29.1 cites
  "paragraph 70", which is ELA in the July 2025 APS) - follow the substance.

## Interpretations and open questions

To confirm with APRA / auditors:

1. **HQLA caps on post-haircut amounts.** Items 4.1/4.2 use weighted adjusted
   amounts, per APS 210 Att A para 7 ("after haircuts").
2. **No APS 210 rate for 210.1A items 11.7 and 19.1.6**; the weight is reported.
3. **Buyback run-offs (16.4/16.5)** default to 10%/5%, overridable with APRA
   approval; 16.6/16.8 are always reported.
4. **210.6 single-factor rows**: the maturity column of a lone printed factor was
   read from the PDF layout.
5. **210.6 item 28.1 <= 210.1A item 6.5** is only a necessary condition; the RBNZ
   part of 29.1 <= item 5 is not checked.
6. **FCS $250k allocation across a depositor's accounts**: APG 210 paras 109/114
   don't say the ADI may choose the split. Default: cover goes first where it
   changes the LCR least (`FcsAllocation::Conservative`); `Optimised` exists but
   needs sign-off.
7. **Term-deposit tainting** (APS 210 Att A para 42) is treated as permanent once
   a non-hardship withdrawal is recorded; the standard gives no cure period.

## Findings that refine design.txt

- **The unwind theorem caught a spec bug.** The first draft reported reverse
  repos with offshore central banks in item 11.7.x (a funding row); proving
  unwind = ARF formula failed until the "of which"/collateral rows were made
  leg-specific.

- **The design sketch's theorem `monotonicity_sme_cliff` is false for the LCR**
  (25% retail -> 20% fully insured wholesale). The spec states and keeps the
  NSFR version, which holds.
- **The HQLA cap formulas need no sign assumptions.** ARS 210.0 floors adjusted
  HQLA1 at zero but not adjusted HQLA2A/2B; Z3 shows the 15%/40% guarantees hold
  for every return regardless.

- **"Every unknown has a conservative direction" is not quite true.** Where a
  favourable treatment needs evidence (SME retail status), the default is the
  treatment that applies without it. A fully insured wholesale SME deposit runs
  off at 20%, below a high-scoring retail one at 25%, so the evidence-based
  default can *lower* the LCR outflow (it always lowers NSFR stable funding).
- **Missing-data cost is not monotone when FCS cover is shared.** One depositor's
  $250k cover is split across accounts, so learning that another account is
  FCS-eligible can move cover away from a stable-capable account and lower ASF.
  This holds for any allocation rule. The invariant is tested where it is true
  (no shared cover, no SMEs) and the counterexamples are pinned as tests.
