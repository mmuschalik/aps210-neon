//! The Prudence rules reproduce the retired Rust implementation's output on
//! 150 generated books x 8 configurations (tests/golden_slices.tsv, captured
//! before the Rust rules were deleted; books are regenerated from the seed).

use chrono::{Days, NaiveDate};
use deposits::testgen::Gen;
use deposits::*;
use prudence_core::uncertain::Mode;

#[test]
fn prudence_rules_reproduce_the_retired_rust_rules() {
    let rd = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
    let want: Vec<&str> = include_str!("golden_slices.tsv").lines().skip(1).collect();
    let mut got = Vec::new();
    let mut g = Gen::new(0x901d);
    for n in 0..150 {
        let book = g.book(rd);
        for mode in [Mode::Conservative, Mode::Optimistic] {
            for fcs in [FcsAllocation::Conservative, FcsAllocation::Optimised] {
                for (k, known_at) in [rd, rd + Days::new(10)].into_iter().enumerate() {
                    let c = classify(&book, &Params { mode, fcs, known_at, ..Params::new(rd) })
                        .unwrap_or_else(|e| panic!("book {n}: {e}"));
                    let mut rows: Vec<String> = c
                        .slices
                        .iter()
                        .map(|s| {
                            format!("{n}\t{mode:?}\t{fcs:?}\t{k}\t{}\t{}\t{}\t{}\t{}",
                                s.account_id, s.amount.amount(), s.lcr_item.unwrap_or("-"), s.nsfr_item, s.nsfr_bucket)
                        })
                        .collect();
                    rows.sort();
                    got.extend(rows);
                }
            }
        }
    }
    assert_eq!(got.len(), want.len());
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "row {i}");
    }
}
