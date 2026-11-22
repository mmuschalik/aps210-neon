//! `cargo run --release -p steel -- [--seed N] [--customers N] [--unknown-pct P] [--regime lcr|mlh] [--out DIR] [--profile]`
//!
//! Generates a synthetic ADI, runs it end to end through every module, every
//! ARF 210 return and the trusted gate, and prints what happened. With
//! `--out DIR` it writes `package.json` (and `bundle.json` if the gate
//! passed), so the standalone `prudence-check` can be run on it separately.

use std::collections::BTreeSet;
use std::time::Instant;

use prudence_core::num::int;
use prudence_core::uncertain::Mode;
use steel::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let val = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let num = |k: &str, d: u64| val(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    let mut cfg = Config::new(num("--seed", 1), num("--customers", 200) as usize);
    cfg.unknown_pct = num("--unknown-pct", 10);
    let rd = cfg.reporting_date;

    if args.iter().any(|a| a == "--profile") {
        let b = generate(&cfg);
        for (k, n, d) in profile(&b, rd).unwrap() {
            println!("{k:>36}: {n:>7} records {d:>12.2?}");
        }
        return;
    }

    if val("--profile-adi").as_deref() == Some("macquarie") {
        let t0 = Instant::now();
        let (bank, subs) = steel::macquarie::generate_macquarie(&cfg);
        let t_gen = t0.elapsed();
        let total_records = bank.retail.accounts.len()
            + bank.wholesale_deposits.len()
            + bank.holdings.len()
            + bank.repos.len()
            + bank.loans.len()
            + bank.other_inflows.receivables.len()
            + bank.commitments.facilities.len()
            + bank.commitments.commitments.len()
            + bank.commitments.obligations.len()
            + bank.derivatives.trades.len()
            + bank.derivatives.flows.len()
            + bank.funding.securities.len()
            + bank.funding.capital.len()
            + bank.funding.placements.len();
        println!(
            "== Macquarie-Shaped Synthetic ADI (BFS + CGM + Treasury + Level 2, seed {}, {} customers, {} total records), {rd}",
            cfg.seed, cfg.customers, total_records
        );
        for (k, v) in census(&bank) {
            println!("{k:>32}: {v}");
        }
        let t1 = Instant::now();
        let ret = assembly::lcr_return(&bank, rd, Mode::Conservative).unwrap();
        let t_calc = t1.elapsed();
        let lcr_ev = prudence_core::evaluate(
            arf210::lcr_all(),
            ret.inputs.iter().map(|(k, v)| (k.as_str(), v.clone())),
            prudence_core::Missing::Zero,
        )
        .unwrap();
        let nsfr_ev = prudence_core::evaluate(
            arf210::nsfr(),
            ret.nsfr_inputs.iter().map(|(k, v)| (k.as_str(), v.clone())),
            prudence_core::Missing::Zero,
        )
        .unwrap();
        println!("== timings");
        println!("{:>32}: {:.2?}", "generate", t_gen);
        println!(
            "{:>32}: {:.2?} ({:.0} rec/s)",
            "all 7 modules + assembly",
            t_calc,
            total_records as f64 / t_calc.as_secs_f64()
        );
        let r = ratios(&ret).unwrap();
        println!("== headline ratios (Level 1)");
        println!(
            "LCR {} (HQLA {}, outflows {}, capped inflows {}, NCO {}); NSFR {} (ASF {}, RSF {})",
            pct(&r.lcr),
            m(&r.hqla),
            m(&r.outflows),
            m(&r.inflows_capped),
            m(&r.nco),
            pct(&r.nsfr.as_ref().map(|q| q * int(100))),
            m(&r.asf),
            m(&r.rsf)
        );
        // Level 2 trapped-surplus waterfall
        println!("\n== Level 2 Offshore Subsidiaries — APS 210 §29 Trapped-Surplus Waterfall (AUD $M)");
        println!("{:<16} {:>12} {:>12} {:>12} {:>12} {:>12}", "Subsidiary", "TotalSurp", "Trapped", "ExclHQLA1", "ExclHQLA2A", "ConsHQLA1");
        let mut trapped_total = prudence_core::Q::default();
        for sub in &subs {
            let pol = assembly::level2::JurisdictionPolicy::for_jurisdiction(sub.jurisdiction);
            let ex = assembly::level2::exclude_trapped_surplus(sub, &pol);
            trapped_total += &ex.trapped_total;
            println!(
                "{:<16} {:>12} {:>12} {:>12} {:>12} {:>12}",
                ex.entity,
                m(&(&ex.total_surplus / int(1_000_000))),
                m(&(&ex.trapped_total / int(1_000_000))),
                m(&(&ex.excluded_hqla1 / int(1_000_000))),
                m(&(&ex.excluded_hqla2a / int(1_000_000))),
                m(&(&ex.consolidated_hqla1 / int(1_000_000))),
            );
        }
        println!("Total trapped offshore HQLA excluded at Level 2: {}", m(&(&trapped_total / int(1_000_000))));

        // Significant currencies (ARF 210.1A item 35, >=5% of liabilities)
        let mut liab_by_ccy: std::collections::BTreeMap<String, prudence_core::Q> = std::collections::BTreeMap::new();
        for (id, kind, bal, _) in &ret.records {
            if matches!(*kind, "retail" | "wholesale" | "repo-funding" | "issued" | "capital") {
                let c = bank.currency.get(id).cloned().unwrap_or_else(|| "AUD".into());
                *liab_by_ccy.entry(c).or_default() += bal.clone();
            }
        }
        let sigs = assembly::level2::significant_currencies(&ret.parts, &liab_by_ccy);
        println!("\n== Significant Currencies (ARF 210.1A item 35 — >= 5% of total liabilities)");
        println!("{:<8} {:>10} {:>12} {:>14} {:>14} {:>14}", "CCY", "Share", "Significant", "Outflows($M)", "Inflows($M)", "NCO($M)");
        for s in &sigs {
            let sh = num_traits::ToPrimitive::to_f64(&s.share_of_liabilities).unwrap_or(0.0) * 100.0;
            println!(
                "{:<8} {:>9.2}% {:>12} {:>14} {:>14} {:>14}",
                s.currency,
                sh,
                if s.significant { "YES" } else { "no" },
                m(&(&s.total_outflows / int(1_000_000))),
                m(&(&s.total_inflows / int(1_000_000))),
                m(&(&s.net_cash_outflows / int(1_000_000))),
            );
        }

        // APS 210 paras 24-29 Stress Suite
        println!("\n== APS 210 paras 24-29 Stress Suite (LCR & NSFR)");
        println!("{:<28} {:>10} {:>14} {:>10} {:>14}", "Scenario", "LCR", "LCR Surp($M)", "NSFR", "NSFR Surp($M)");
        let suite = assembly::steering::aps210_standard_suite();
        let results = assembly::steering::stress_full(&bank, rd, &suite).unwrap();
        for sr in &results {
            println!(
                "{:<28} {:>10} {:>14} {:>10} {:>14}",
                sr.name,
                pct(&sr.lcr.lcr),
                m(&sr.lcr.surplus()),
                pct(&sr.nsfr.nsfr.as_ref().map(|q| q * int(100))),
                m(&sr.nsfr.surplus()),
            );
        }

        // Print summary ARF 210.1A & ARF 210.6 key rows
        let zero = prudence_core::Q::default();
        let cell_num = |ev: &prudence_core::Evaluation<'_>, id: &str, col: &str| {
            ev.values.get(&prudence_core::Ref::new(id, col)).and_then(|v| v.num()).cloned()
        };
        println!("\n== ARF 210.1A Key Summary (AUD $M)");
        for id in ["1", "2", "3", "25", "8", "9", "10", "11", "13", "14", "15", "16", "27", "19", "20", "21", "28", "29", "30"] {
            if let Some(item) = arf210::lcr_all().items.iter().find(|i| i.id == id) {
                let amt = cell_num(&lcr_ev, id, "amount");
                let val = cell_num(&lcr_ev, id, "weighted").or_else(|| cell_num(&lcr_ev, id, "value"));
                let c1 = amt.as_ref().map(|v| format!("{:.2}", num_traits::ToPrimitive::to_f64(v).unwrap_or(0.0))).unwrap_or_else(|| "-".into());
                let c2 = val.as_ref().map(|v| format!("{:.2}", num_traits::ToPrimitive::to_f64(v).unwrap_or(0.0))).unwrap_or_else(|| "-".into());
                let lbl: String = item.label.chars().take(52).collect();
                println!("{:<6} {:<53} {:>12} {:>12}", id, lbl, c1, c2);
            }
        }
        println!("\n== ARF 210.6 Key Summary (AUD $M)");
        for item in &arf210::nsfr().items {
            let calc = cell_num(&nsfr_ev, &item.id, "calc");
            let val = cell_num(&nsfr_ev, &item.id, "value");
            if calc.as_ref().is_some_and(|v| *v != zero) || val.as_ref().is_some_and(|v| *v != zero) {
                let lbl: String = item.label.chars().take(52).collect();
                let cc = if let Some(v) = val {
                    format!("{:.2}%", num_traits::ToPrimitive::to_f64(&v).unwrap_or(0.0) * 100.0)
                } else {
                    format!("{:.2}", num_traits::ToPrimitive::to_f64(&calc.unwrap()).unwrap_or(0.0))
                };
                println!("{:<8} {:<53} {:>12}", item.id, lbl, cc);
            }
        }
        if let Some(ui_path) = val("--ui") {
            let p = std::path::Path::new(&ui_path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let html = steel::ui::render("Macquarie-Shaped Synthetic ADI", &bank, &ret, &subs, rd, total_records)
                .unwrap_or_else(|e| panic!("ui render: {e}"));
            std::fs::write(p, html).unwrap();
            println!("\nwrote interactive Form & Drilldown UI -> {ui_path}");
        }
        return;
    }

    if args.iter().any(|a| a == "--e2e-forms") {
        let t0 = Instant::now();
        let bank = generate(&cfg);
        let t_gen = t0.elapsed();
        let total_records = bank.retail.accounts.len()
            + bank.wholesale_deposits.len()
            + bank.holdings.len()
            + bank.repos.len()
            + bank.loans.len()
            + bank.other_inflows.receivables.len()
            + bank.commitments.facilities.len()
            + bank.commitments.commitments.len()
            + bank.commitments.obligations.len()
            + bank.derivatives.trades.len()
            + bank.funding.securities.len()
            + bank.funding.capital.len()
            + bank.funding.placements.len();
        println!("== E2E Synthetic ADI (seed {}, {} customers, {} total records), {rd}", cfg.seed, cfg.customers, total_records);
        for (k, v) in census(&bank) {
            println!("{k:>32}: {v}");
        }
        let t1 = Instant::now();
        let ret = assembly::lcr_return(&bank, rd, Mode::Conservative).unwrap();
        let t_calc = t1.elapsed();
        let t2 = Instant::now();
        let lcr_ev = prudence_core::evaluate(
            arf210::lcr_all(),
            ret.inputs.iter().map(|(k, v)| (k.as_str(), v.clone())),
            prudence_core::Missing::Zero,
        )
        .unwrap();
        let nsfr_ev = prudence_core::evaluate(
            arf210::nsfr(),
            ret.nsfr_inputs.iter().map(|(k, v)| (k.as_str(), v.clone())),
            prudence_core::Missing::Zero,
        )
        .unwrap();
        let t_eval = t2.elapsed();
        println!("== timings");
        println!("{:>32}: {:.2?}", "generate", t_gen);
        println!("{:>32}: {:.2?} ({:.0} rec/s)", "all 7 modules + assembly", t_calc, total_records as f64 / t_calc.as_secs_f64());
        println!("{:>32}: {:.2?}", "evaluate ARF 210.1A & 210.6", t_eval);
        println!("{:>32}: {:.2?}", "TOTAL end-to-end", t0.elapsed());
        let zero = prudence_core::Q::default();
        let cell_num = |ev: &prudence_core::Evaluation<'_>, id: &str, col: &str| {
            ev.values.get(&prudence_core::Ref::new(id, col)).and_then(|v| v.num()).cloned()
        };
        println!("\n== ARF 210.1A (Liquidity Coverage Ratio — All Currencies, AUD $M)");
        println!("{:<10} {:<56} {:>14} {:>14}", "Item", "Label", "Amount ($M)", "Weighted ($M)");
        for item in &arf210::lcr_all().items {
            let amt = cell_num(&lcr_ev, &item.id, "amount");
            let wtd = cell_num(&lcr_ev, &item.id, "weighted");
            let val = cell_num(&lcr_ev, &item.id, "value");
            let shown = amt.as_ref().is_some_and(|v| *v != zero)
                || wtd.as_ref().is_some_and(|v| *v != zero)
                || val.as_ref().is_some_and(|v| *v != zero);
            if shown {
                let lbl: String = item.label.chars().take(55).collect();
                let c1 = amt.as_ref().map(|v| format!("{:.2}", num_traits::ToPrimitive::to_f64(v).unwrap_or(0.0))).unwrap_or_else(|| "-".into());
                let c2 = wtd.or(val).as_ref().map(|v| format!("{:.2}", num_traits::ToPrimitive::to_f64(v).unwrap_or(0.0))).unwrap_or_else(|| "-".into());
                println!("{:<10} {:<56} {:>14} {:>14}", item.id, lbl, c1, c2);
            }
        }
        println!("\n== ARF 210.6 (Net Stable Funding Ratio, AUD $M)");
        println!("{:<10} {:<50} {:>10} {:>10} {:>10} {:>12}", "Item", "Label", "<6m", "6m-1y", ">=1y", "Calc ($M)");
        for item in &arf210::nsfr().items {
            let b0 = cell_num(&nsfr_ev, &item.id, "b0");
            let b1 = cell_num(&nsfr_ev, &item.id, "b1");
            let b2 = cell_num(&nsfr_ev, &item.id, "b2");
            let amt = cell_num(&nsfr_ev, &item.id, "amount");
            let calc = cell_num(&nsfr_ev, &item.id, "calc");
            let val = cell_num(&nsfr_ev, &item.id, "value");
            let shown = [&b0, &b1, &b2, &amt, &calc, &val].iter().any(|x| x.as_ref().is_some_and(|v| *v != zero));
            if shown {
                let lbl: String = item.label.chars().take(49).collect();
                let f = |o: &Option<prudence_core::Q>| o.as_ref().map(|v| format!("{:.2}", num_traits::ToPrimitive::to_f64(v).unwrap_or(0.0))).unwrap_or_else(|| "-".into());
                let c0 = if amt.is_some() { f(&amt) } else { f(&b0) };
                let cc = if val.is_some() {
                    val.as_ref().map(|v| format!("{:.2}%", num_traits::ToPrimitive::to_f64(v).unwrap_or(0.0) * 100.0)).unwrap()
                } else {
                    f(&calc)
                };
                println!("{:<10} {:<50} {:>10} {:>10} {:>10} {:>12}", item.id, lbl, c0, f(&b1), f(&b2), cc);
            }
        }
        if let Some(ui_path) = val("--ui") {
            let p = std::path::Path::new(&ui_path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let html = steel::ui::render("Synthetic ADI", &bank, &ret, &[], rd, total_records)
                .unwrap_or_else(|e| panic!("ui render: {e}"));
            std::fs::write(p, html).unwrap();
            println!("\nwrote interactive Form & Drilldown UI -> {ui_path}");
        }
        return;
    }

    let regime = val("--regime").unwrap_or_else(|| "lcr".into());
    let t = run(&cfg, &regime).unwrap_or_else(|e| panic!("{e}"));
    println!("== synthetic ADI (seed {}, {} customers, {}% unknowns), {rd}", cfg.seed, cfg.customers, cfg.unknown_pct);
    for (k, v) in census(&t.bank) {
        println!("{k:>32}: {v}");
    }
    println!("== stages");
    for (k, d) in &t.timings {
        println!("{k:>32}: {d:.2?}");
    }
    println!("== results");
    match &t.ratios {
        Ok(r) => println!(
            "LCR {} (HQLA {}, outflows {}, capped inflows {}, NCO {}); NSFR {} (ASF {}, RSF {})",
            pct(&r.lcr),
            m(&r.hqla),
            m(&r.outflows),
            m(&r.inflows_capped),
            m(&r.nco),
            pct(&r.nsfr.as_ref().map(|q| q * int(100))),
            m(&r.asf),
            m(&r.rsf)
        ),
        Err(e) => println!("ratios: {e}"),
    }
    let forms: BTreeSet<&String> = t.package.optional_inputs.keys().collect();
    println!("returns: ARF 210.1A, ARF 210.6, {}", forms.into_iter().cloned().collect::<Vec<_>>().join(", "));
    println!(
        "contributions {}, certificates {}, assumptions {}",
        t.package.parts.len(),
        t.package.certificates.len(),
        t.package.assumptions.len()
    );
    println!("gate: {}  bundle {}", if t.report.passed() { "PASSED" } else { "FAILED" }, t.bundle_id().unwrap_or("withheld"));
    for f in t.report.failures().iter().take(20) {
        println!("  FAIL {f}");
    }

    if t.report.passed() {
        // What moved since yesterday, and what an incremental update costs.
        let (d0, before) = synth::snapshot(&cfg, 1);
        let s = Instant::now();
        let (start, steps, end) = spikes::explain(&before, d0, &t.bank, rd).unwrap();
        println!("== LCR surplus over 100%, {d0} -> {rd} ({:.2?})", s.elapsed());
        println!("{:>40}: {}", "start", m(&start.surplus()));
        for st in &steps {
            println!("{:>40}: {:>+12}  (LCR {})", st.driver, m(&st.surplus_change), pct(&st.lcr.lcr));
        }
        println!("{:>40}: {}  (residual 0 exactly)", "end", m(&end.surplus()));

        let base = &t.ret;
        let mut after = t.bank.clone();
        after.loans[0].balance += int(1_000_000);
        let scope = [after.loans[0].id.clone()].into();
        let s = Instant::now();
        let full = assembly::lcr_return(&after, rd, Mode::Conservative).unwrap();
        let full_t = s.elapsed();
        let s = Instant::now();
        let (inc, n) = spikes::incremental(base, &after, &scope, rd).unwrap();
        let inc_t = s.elapsed();
        let same = inc == assembly::assemble(&full.parts, &Default::default()).unwrap();
        println!("== one loan edited: full recompute {full_t:.2?}; delta update of {n} record(s) {inc_t:.2?}; identical: {same}");
    }

    if let Some(dir) = val("--export-sql") {
        let p = std::path::Path::new(&dir);
        steel::sql_export::export_duckdb(&t, p).unwrap_or_else(|e| panic!("export-sql: {e}"));
        println!("exported DuckDB CSVs + run.sql to {dir}/");
    }

    if let Some(dir) = val("--out") {
        std::fs::create_dir_all(&dir).unwrap();
        let path = format!("{dir}/package.json");
        std::fs::write(&path, serde_json::to_string_pretty(&checker::json::to_json(&t.package)).unwrap()).unwrap();
        println!("wrote {path}: check it with `cargo run -p checker --bin prudence-check -- {path} --bundle {dir}/bundle.json`");
    }
    if !t.report.passed() {
        std::process::exit(1);
    }
}
