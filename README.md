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
