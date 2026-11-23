# aps210

APRA APS 210 (Liquidity) as a verified, executable specification, built
bottom-up towards the architecture in `design.txt` (Prudence language, Rust
engine, Lean/SMT proofs, tiny trusted checker).

Requires `z3` and `lean` (via elan) on PATH for the proof tests.

Not yet: the incremental (DBSP) engine, DuckDB stress runs, and production data generation for the maturity, funding-plan and daily
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
| `lean/Prudence/DSL.lean`, `JSON.lean`, `SQL.lean`, `ModelGen.lean`, `Export.lean` | **Idiomatic Lean 4 Prudence DSL**, JSON IR exporter, **DuckDB SQL compiler**, and **Lean 4 proof-model generator**. Run `cd lean && lake exe prudence-export ..` to regenerate `specs/ir/**/*.json`, `sql/**/*.sql`, and all 7 `lean/Prudence/*.lean` proof models from a single source of truth. |
| `lean/Prudence/Specs/Forms/*.lean` | **Source of truth** for the 8 ARF 210 forms, in idiomatic Lean 4 (`FormDecl`). Exported to `specs/ir/forms/*.json` and `sql/forms/*.sql`. |
| `lean/Prudence/Specs/Modules/*.lean` | **Source of truth** for the 7 APS 210 rule modules (`RetailSme`, `Wholesale`, `SecuredFunding`, `Loans`, `Derivatives`, `Funding`, `Commitments`) in idiomatic Lean 4 (`Module`). Exported to `specs/ir/modules/*.json`, `sql/modules/*.sql`, and `lean/Prudence/*.lean`. |
| `sql/forms/*.sql`, `sql/modules/*.sql` | **Generated DuckDB SQL** compiled directly from the Lean 4 IR (layered CTEs + check views for all 8 forms; DDL, macros, views, and partitions for modules, with LP `optimize` solutions as external input tables). |
| `crates/prudence-core` | Prudence core runtime: exact rationals (no floats anywhere); `syntax/ir_json` = Lean IR JSON loader; `lp` = exact two-phase simplex with a primal-dual certificate checker (every `optimize` is a general LP); `syntax/` (name resolution, type checker, form lowering, `prove` = Z3 proof obligations, `witness` = Z3 branch-witness generator); the form IR and evaluator; `interp` = module interpreter (views, `optimize` with solution certificate, `partition` with run-time exhaustiveness, invariants, clause-level provenance). |
| `crates/arf210` | Loads the eight form specs; cross-form rules; CLI; parity and scenario tests. |
| `crates/secured` | Data model and loader for holdings and repos (`SecuredFunding.lean`); `run()` produces ARF 210.1A Section A, items 11 & 19, mechanical 30-day unwind, and ARF 210.6 NSFR RSF/ASF outputs (`hqla_rules`, `pool`). |
| `crates/deposits` | Retail/SME (`RetailSme.lean`) and wholesale (`Wholesale.lean`) deposits: ACTUS contracts, CDM-style bitemporal events, `Uncertain<T>` attributes, FCS LP allocation, and LCR/NSFR classification. |
| `crates/loans` | Loans as ACTUS contracts (`Loans.lean`: ANN, LAM, PAM, UMP): 30-day contractual inflows (ARF 210.1A item 20) and per-repayment NSFR RSF bucketing (ARF 210.6 items 24, 35–37, 39). |
| `crates/derivatives` | Derivatives and collateral (`Derivatives.lean`, `collateral.rs`): bilateral OTC, CCP-cleared, and exchange-traded futures, tiered CSA substitution, lot-level posted collateral, 24-month HLBA lookback (13.6), and NSFR items 13.1/40.1/42.1–42.3. |
| `crates/funding` | Debt securities issued, regulatory capital, and placements with other ADIs (`Funding.lean`: ARF 210.1A items 10, 12.1, 20.5; ARF 210.6 items 1, 12, 24, 33; ARF 210.2 item 6). |
| `crates/obligations` | Off-balance sheet commitments, facilities, redraw, trade finance & guarantees (`Commitments.lean`, `lookback.rs`: ARF 210.1A items 14–16; ARF 210.6 items 48–53). |
| `crates/assembly` | Return assembly across all 7 modules, currency splits, maturity ladders (`maturity.rs`), APS 210 paras 24–29 stress testing & survival horizon (`steering.rs`), Level 2 consolidation & significant currencies (`level2.rs`), statutory rules (`aps210_rules.rs`), and behavioural models (`behavioural.rs`). |
| Linear pools and observers | Design 4.A.2. `@linear data` types (holdings, retail and wholesale deposits, loans) must be placed into each form by exactly one `partition`, whose cases Z3 proves exhaustive and disjoint - a holding takes one role (HQLA stock, CLF, maturing inflow, none), so an asset in the stock can never also flow in (Att A para 31). Form items marked `observer` (the ARS "of which" / memo rows) can never be summed into their own totals: building the form fails. Both are mutation-tested. |
| Overlays and effective dates | Design 4.A.6. APRA discretions (e.g. the ARF 210.1A item 29.1 net cash outflow overlay) and data patches are `checker::Overlay`s: kind, cell, amount, required APRA instrument (discretions), approver, effective and expiry dates, HMAC-SHA256 signature (RFC 4231-tested). `checker::check_signed` rejects forged, expired, not-yet-effective, uncited or unlodged overlays. |
| `crates/checker` | The trusted verification gate: GL tie-out, conservation, no double counting, lodged inputs = contributions, LP primal-dual certificates, form checks/theorems, cross-form rules, and SHA-256 APS 310 audit bundle. Standalone: `cargo run -p checker --bin prudence-check -- package.json --bundle bundle.json`. |
| `crates/steel` | **Steel thread & UI.** Synthetic ADI generator (`synth.rs`, `macquarie.rs`), DuckDB CSV/SQL exporter (`--export-sql`), end-to-end scale runner (`--e2e-forms`), and interactive **Form & Drilldown HTML UI** (`--ui target/ui/index.html`). Example: `cargo run --release -p steel -- --profile-adi macquarie --customers 10000 --ui target/ui/index.html`. |
| `lean/Prudence/Generated/*.lean` | **Auto-generated** Lean 4 proof models (`RetailSme`, `Wholesale`, `SecuredFunding`, `Loans`, `Derivatives`, `Funding`, `Commitments`) emitted by `Prudence.ModelGen` (`cd lean && lake exe prudence-export ..`), separated from hand-authored specs (`Specs/`) and proofs (`*Proofs.lean`). |

`prudence-core` also holds two design primitives used by step 2: `Money<Currency,
Basis>` (mixing market and carrying values is a compile error) and
`Uncertain<T>` with a logging `Resolver` (reported vs. optimistic runs; the
difference prices missing data).

Forms covered: ARF 210.1A (LCR all currencies), 210.1B (LCR AUD), 210.2 (MLH),
210.3.1/210.3.2 (maturity), 210.4 (funding plan), 210.5 (daily), and 210.6
(NSFR). Not yet: 210.4 companion data and the free-form currency table in
210.1A item 35.

Checker packages accept an optional `optional_inputs` object keyed by canonical
form code (for example, `"ARF 210.1B": {"10.1": "40"}`); values are input maps
with exact rational strings. Omission remains compatible with older packages.
Each supplied return is evaluated, checked, and hashed into the audit bundle.
The checker applies the 1A/1B, daily/LCR, daily/MLH, funding-plan/maturity,
and NSFR/LCR rules whenever their forms are available. Either maturity form
can be supplied independently. Assembly includes 210.1B from AUD currency-split
contributions, including the FX split treatment.

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
- **Proven in Lean** (`lean/Prudence/*Proofs.lean`, checked by `cargo test`):
  all module theorems, over all inputs, for whole books, about the Lean models
  **generated directly from the Lean 4 DSL specs** (`lean/Prudence/ModelGen.lean`
  via `cd lean && lake exe prudence-export ..`). What the generated `World`
  leaves abstract (the `optimize` solution, calendar buckets, event queries)
  appears as explicit hypotheses. Changing a rule in the spec breaks the proof.
  Differential `#guard` tests also check the generated models against the
  interpreter on generated books.
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

## Checker assurance boundary

The CLI defaults to `check_complete`: every supplied return needs contribution
evidence and reporting context; every certificate needs supported FCS problem
evidence and account/output bindings. Missing evidence fails the gate and prevents
a bundle. Legacy diagnostics require `--allow-partial` (library: `check`).

Schema 3 labels bundles `complete_evidence_consistency` or `scoped_consistency`.
Neither is a digital signature or an assertion that upstream facts are correct.
The full gate verifies the package's internal evidence relationships. Source
completeness, classification truth, and signed regulatory overlays remain outside
that proof, and are listed explicitly in coverage.

Every contribution must reference a ledger record or an explicitly declared
`external_sources` entry. Off-ledger commitments and aggregate netting sets are
declared separately and labelled as not GL reconciled. Assembly supplies both LCR
streams and source-level NSFR evidence. 210.1A/1B are reconstructed from the
contribution streams; NSFR and other optional returns use `form_contributions`:

```json
{"ARF 210.3.1": [{"source": "cash-account", "cell": "1.1:at_call", "amount": "10"}]}
```

Amounts use exact rational strings in reporting units. An empty evidence array
asserts zero inputs; missing zero cells are equivalent to explicit zeros.
Unknown or derived contribution cells fail. The rates map accepts weight cells
only. Undefined LCRs are null with an explicit status rather than a panic.

Reporting `contexts` are keyed by form code, each with `entity`,
`reporting_date`, `consolidation` (`level1` or `level2`), and `regime`
(`lcr` or `mlh`). All contexts must match each other and the package date.
Assembly callers supply these using `package_with_context`; the engine does not
invent an entity or consolidation level. Daily rules select the declared regime.
Level 2 comparisons omit Level 1 day-1 ratios. Coverage lists every implemented
cross-form rule group as evaluated or skipped, with the reason. These rules do
not assert that all regulatory returns required of a particular ADI were supplied.

FCS certificates retain account IDs in decision-variable order and resolved
eligibility, operational amounts and counterfactual rates. The checker reconstructs
the 250,000 AUD budget, account bounds, and objective independently of the supplied
LP matrix. It verifies primal-dual optimality, compares covered slices to the
solution, conserves account balances, and reconciles slice outputs to LCR/NSFR
contributions. A valid certificate for a substituted problem fails. The resolved
attributes and counterfactual classifications are evidence, not independently
reclassified raw contracts. Generic LPs without supported problem evidence are
available only in partial diagnostics.

`specs` and `spec_version` identify the compiled form sources used by the checker.
Caller sources appear separately as `declared_sources`; `forms/<canonical code>`
declarations must match the compiled source exactly. `package_sha256` binds all
serialized input evidence, including rates, contexts, allocation data and source
declarations. Coverage records evaluated forms, contribution reconciliation,
allocation verification, missing evidence, and unsupported guarantees.

The manifest also fingerprints checker, JSON, cross-form, evaluator and LP verifier
source files. These are implementation identifiers, not a reproducible-build
attestation. Undefined operands do not establish equality in cross-form rules;
the coverage policy records that skip behavior and the primary ratio has an
explicit defined/undefined status.
