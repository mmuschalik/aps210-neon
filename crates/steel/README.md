# steel: the steel thread

One synthetic ADI, generated from a seed, run end to end: every Prudence
module, all eight ARF 210 returns, the trusted gate, the content-addressed
APS 310 bundle, and the standalone checker reading it back from JSON. Alongside
that, one test per capability the full build relies on. The point is not to
implement all of APS 210 here. It is to show that the foundations carry the
rest.

```sh
cargo test -p steel                                    # the proof (13 tests, ~10 s)
cargo run --release -p steel -- --customers 1000       # one run, with timings, waterfall, delta update
cargo run --release -p steel -- --regime mlh           # an MLH ADI (ARF 210.2)
cargo run --release -p steel -- --out DIR              # write package.json for prudence-check
cargo run --release -p steel -- --customers 4000 --profile   # time per module
```

## The generator (`src/synth.rs`)

`Config { seed, customers, unknown_pct, reporting_date, hqla_pct }` maps to a
`Bank` covering every book the engine reads:

- retail/SME customers and accounts (UMP / CLM / PAM)
- shared SME groups
- notices and term-deposit breaks in the bitemporal ledger
- wholesale deposits with operational splits
- HQLA1/2A/2B and CLF holdings
- repos in both directions
- annuity, amortising, bullet and revolving loans
- receivables
- facilities, commitments and contingents
- netting sets with IRS flows, FX swaps and options
- 24 months of collateral flows, some recorded late
- issued debt, capital and placements
- USD/NZD positions

Every book is sized as a share of retail funding, so the ratios stay
plausible at any size. `unknown_pct` sets how often each `Uncertain`
attribute is missing upstream. The general ledger is booked from the raw books
(`book_gl`), not from engine output, so the GL tie-out is a real test.
`snapshot(cfg, k)` gives the same bank k days earlier, with a trend per book
plus noise, to feed the ARF 210.5 history and the day-on-day attribution. The
generator guarantees that every seed reaches the hard paths, for example an
operational wholesale deposit with a measured balance.

## What the tests prove

| Capability (design.txt) | Test | Result |
|---|---|---|
| End to end on generated data: modules → 8 returns → strict gate → bundle | `every_return_from_generated_data_passes_the_strict_gate`, `an_mlh_adi_…` | 3 seeds pass under LCR and 1 under MLH. Every cross-form rule is evaluated (`daily_vs_mlh` is skipped by regime). |
| Generator reaches the hard rules | `generated_data_reaches_the_hard_paths` | Shared SME groups (the AUD 2m cliff), FCS LPs, operational three-way split into ARF 210.6 item 8, tainting events, late-recorded collateral, currency split, logged unknowns. |
| Determinism; TCB separable from the engine | `same_seed_same_bundle_and_the_package_survives_json` | The same seed gives the same bundle id. Package → JSON → `check_complete` gives the same id. The CLI `--out` plus `prudence-check` reproduces it from disk. |
| The gate is not a rubber stamp (4.B TCB) | `the_gate_rejects_every_injected_fault` | 9 corruptions of a real package are each caught by the expected check: GL break, missing record, broken slice conservation, an HQLA asset also counted as an inflow, input ≠ contributions, a tampered LP certificate, spec drift, 210.4 ≠ maturity ladder, mixed entity. |
| Exact move attribution (4.C.2) | `day_on_day_movement_is_attributed_with_zero_residual` | Time roll-down plus 9 book drivers; start + Σ = end exactly. Per record, each input's movement equals the sum of its records' movements. |
| Incremental engine feasible (4.B DBSP) | `outputs_are_local_and_a_delta_update_equals_the_full_recompute`, `an_event_affects_exactly_its_product_class` | For 6 edit kinds (loan, retail, SME group, holding, wholesale, facility), only the edited record's group changes. Splicing a recompute of that group into the old return equals the full recompute. 1,000 customers: 5 ms vs 0.99 s. |
| Bitemporal replay | `a_return_replays_exactly_as_known_at_an_earlier_date` | A notice keyed in late changes only its account. A late collateral outflow is invisible to the 24-month lookback as known before it was recorded. |
| Stress, data-quality drag, FTP (4.A.3–5) | `stress_and_data_quality_run_on_generated_data` | `assembly::steering` on generated data: every scenario lowers the surplus, and drag is attributed per module. |
| Signed, expiring APRA overlays (4.A.6) | `a_signed_apra_overlay_flows_through_every_return` | An item 29.1 overlay lowers the LCR, 210.5 still ties out, and an expired or wrongly signed overlay gets no bundle. |

## Defects found on generated data (all fixed)

Hand-built test banks missed all three. Each is guarded by a test.

1. Operational wholesale deposits went to ARF 210.6 `[8:b0]`, which is a
   derived cell (8 = 8.1 + 8.2). Any bank with a measured operational balance
   could not be evaluated. Fixed in 91238bc.
2. The NSFR key for allocation slices (`8.2:amount`) differed from the lodged
   key (`8.2`), so the strict gate failed allocation bindings. Fixed in 54d8724.
3. Wholesale unknowns were resolved but never logged, so the bundle's
   assumption log and the data-quality drag were missing them. Fixed in 8da06fc.

## Gaps: what the foundations do not yet carry

- **Engine speed.** The interpreter is the reference semantics, and it is
  superlinear. Retail classification takes 1.4 s for 2k accounts, 8 s for 8k
  and 58 s for 32k. Derivatives takes 0.1 s for 9 netting sets, 1.7 s for 24
  and 128 s for 126. The locality tests show what a compiled or incremental
  backend can rely on (group keys: customer plus SME group, product class via
  the shared FCS cover, netting set, record). It is not built yet. ARF 210.5
  re-runs the whole engine for each of 5 snapshots, which is the obvious first
  use of delta updates.
- **Whole-return replay.** `deposits` and `derivatives` take `known_at`, but
  `assembly::lcr_return` fixes it to the reporting date. A restated return as
  known at an earlier date needs `known_at` threaded through.
- **The assumption log is evidence, not re-derived.** Dropping an entry
  changes the bundle id but passes the gate
  (`a_dropped_assumption_changes_the_bundle_but_is_not_rejected`).
- **Level 2 and jurisdictions.** `consolidation: level2` is only a label:
  there is no intragroup elimination, no trapped-surplus exclusion and no
  `JurisdictionPolicy`. The `module(jurisdiction: AU_APRA)` parameter is
  parsed but unused.
- **Maturity-convention types** (EarliestRedemption vs LatestMaturity,
  design 4.A.1) are not in the type system. The asymmetry exists only in the
  rules.
- **Optimisation beyond FCS.** The certified LP machinery exists, and FCS
  uses it (933 certificates at 1,000 customers). Collateral pool assignment
  and LCR/NSFR rehypothecation consistency are not yet `optimize` blocks.
- **Generator coverage.** All holdings are domestic-currency, there are few
  intragroup flows, the ARF 210.4 plan has a single cell, and there are no
  offshore subsidiaries.
