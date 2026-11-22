//! Export a generated `steel::Thread` (bank, LP allocations, form inputs, and
//! Rust exact-rational oracle outputs) to CSVs + a ready-to-run DuckDB `run.sql`
//! driver script.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use num_traits::ToPrimitive;
use prudence_core::uncertain::Uncertain;
use prudence_core::{Missing, Q, evaluate};

use crate::Thread;

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn opt_bool(b: Option<bool>) -> &'static str {
    match b {
        Some(true) => "true",
        Some(false) => "false",
        None => "",
    }
}

fn unc_bool(u: &Uncertain<bool>) -> &'static str {
    opt_bool(u.known().copied())
}

fn q_dec(q: &Q) -> String {
    let f = q.to_f64().unwrap_or(0.0);
    if q.is_integer() {
        q.to_integer().to_string()
    } else {
        format!("{f:.10}")
    }
}

/// Export all canonical tables, module CSVs, LP allocation CSVs, form input CSVs,
/// expected oracle cells, and a `run.sql` DuckDB script into `dir`.
pub fn export_duckdb(t: &Thread, dir: &Path) -> Result<(), String> {
    let data = dir.join("data");
    for sub in ["", "retail_sme", "wholesale", "secured_funding", "loans", "derivatives", "funding", "commitments", "forms"] {
        fs::create_dir_all(data.join(sub)).map_err(|e| e.to_string())?;
    }
    let rd = t.cfg.reporting_date;
    let b = &t.bank;

    // 1. Canonical Counterparty table
    {
        let mut s = String::from("id,legal_name,sector,adi_tier,affiliation,jurisdiction,connected_group_id,sme_group,established_relationship,managed_as_retail,pie_qualifies\n");
        for cp in &b.counterparties {
            writeln!(
                s,
                "{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(&cp.id),
                csv_escape(&cp.legal_name),
                cp.sector,
                cp.adi_tier.as_deref().unwrap_or(""),
                cp.affiliation,
                cp.jurisdiction,
                cp.connected_group_id.as_deref().unwrap_or(""),
                cp.sme_group.as_deref().unwrap_or(""),
                opt_bool(cp.established_relationship),
                opt_bool(cp.managed_as_retail),
                opt_bool(cp.pie_qualifies),
            )
            .unwrap();
        }
        fs::write(data.join("counterparty.csv"), s).map_err(|e| e.to_string())?;
    }

    // 2. Retail & SME module
    {
        fs::write(
            data.join("retail_sme/params.csv"),
            format!("mode,reporting_date,known_at,fcs_limit,sme_threshold\nConservative,{rd},{rd},250000,2000000\n"),
        )
        .map_err(|e| e.to_string())?;

        let mut cust = String::from("id,kind,established_relationship,sme_group,managed_as_retail,pie_qualifies\n");
        for c in &b.retail.customers {
            writeln!(
                cust,
                "{},{:?},{},{},{},{}",
                csv_escape(&c.id),
                c.kind,
                unc_bool(&c.established_relationship),
                c.sme_group.known().map(String::as_str).unwrap_or(""),
                unc_bool(&c.managed_as_retail),
                unc_bool(&c.pie_qualifies),
            )
            .unwrap();
        }
        fs::write(data.join("retail_sme/customer.csv"), cust).map_err(|e| e.to_string())?;

        let mut dep = String::from("id,holder,product_class,balance,fcs_eligible,transactional,online,rate_driven,contract_kind,notice_days,grace_end,maturity,early_withdrawal\n");
        for a in &b.retail.accounts {
            let (ck, nd, ge, mat, ew) = match &a.contract {
                deposits::Contract::Ump => ("UMP", String::new(), String::new(), String::new(), String::new()),
                deposits::Contract::Clm { notice_days, grace_period_end } => (
                    "CLM",
                    notice_days.to_string(),
                    grace_period_end.map(|d| d.to_string()).unwrap_or_default(),
                    String::new(),
                    String::new(),
                ),
                deposits::Contract::Pam { maturity, early_withdrawal } => (
                    "PAM",
                    String::new(),
                    String::new(),
                    maturity.to_string(),
                    early_withdrawal.known().map(|w| format!("{w:?}")).unwrap_or_default(),
                ),
            };
            writeln!(
                dep,
                "{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(&a.id),
                csv_escape(&a.customer_id),
                csv_escape(&a.product_class),
                q_dec(a.balance.amount()),
                unc_bool(&a.fcs_eligible),
                unc_bool(&a.transactional),
                unc_bool(&a.online),
                unc_bool(&a.rate_driven),
                ck,
                nd,
                ge,
                mat,
                ew,
            )
            .unwrap();
        }
        fs::write(data.join("retail_sme/deposit.csv"), dep).map_err(|e| e.to_string())?;

        // LP allocations (`alloc_covered`) from `t.ret.allocation_slices`
        let mut covered_by_id: BTreeMap<&str, Q> = BTreeMap::new();
        for s in &t.ret.allocation_slices {
            if s.covered {
                *covered_by_id.entry(s.source.as_str()).or_default() += s.amount.clone();
            }
        }
        let mut alloc = String::from("record_id,mode,amount\n");
        for a in &b.retail.accounts {
            let amt = covered_by_id.get(a.id.as_str()).cloned().unwrap_or_default();
            writeln!(alloc, "{},Conservative,{}", csv_escape(&a.id), q_dec(&amt)).unwrap();
        }
        fs::write(data.join("retail_sme/alloc_covered.csv"), alloc).map_err(|e| e.to_string())?;
    }

    // 3. Wholesale module
    {
        fs::write(
            data.join("wholesale/params.csv"),
            format!("mode,reporting_date,fcs_limit\nConservative,{rd},250000\n"),
        )
        .map_err(|e| e.to_string())?;

        let mut cust = String::from("id,kind\n");
        for c in &b.wholesale_customers {
            writeln!(cust, "{},{:?}", csv_escape(&c.id), c.kind).unwrap();
        }
        fs::write(data.join("wholesale/customer.csv"), cust).map_err(|e| e.to_string())?;

        let mut dep = String::from("id,holder,balance,within_30_days,called,fcs_eligible,operational_service,operational_balance\n");
        for d in &b.wholesale_deposits {
            writeln!(
                dep,
                "{},{},{},{},{},{},{},{}",
                csv_escape(&d.id),
                csv_escape(&d.customer_id),
                q_dec(&d.balance),
                d.within_30_days,
                d.called,
                unc_bool(&d.fcs_eligible),
                unc_bool(&d.operational_service),
                d.operational_balance.known().map(q_dec).unwrap_or_default(),
            )
            .unwrap();
        }
        fs::write(data.join("wholesale/deposit.csv"), dep).map_err(|e| e.to_string())?;

        let mut insured_by_id: BTreeMap<&str, Q> = BTreeMap::new();
        for s in &t.ret.allocation_slices {
            if s.covered {
                *insured_by_id.entry(s.source.as_str()).or_default() += s.amount.clone();
            }
        }
        let mut alloc = String::from("record_id,mode,amount\n");
        for d in &b.wholesale_deposits {
            let amt = insured_by_id.get(d.id.as_str()).cloned().unwrap_or_default();
            writeln!(alloc, "{},Conservative,{}", csv_escape(&d.id), q_dec(&amt)).unwrap();
        }
        fs::write(data.join("wholesale/alloc_insured.csv"), alloc).map_err(|e| e.to_string())?;
    }

    // 4. Form inputs & exact-rational Rust oracle for ARF 210.1A and ARF 210.6
    let write_form_io = |stem: &str, form: &prudence_core::Form, inputs: &BTreeMap<String, Q>| -> Result<(), String> {
        let mut inp = String::from("item_id,col,val\n");
        for (k, v) in inputs {
            let (item, col) = k.split_once(':').unwrap_or((k.as_str(), form.default_input_col.as_str()));
            writeln!(inp, "{},{},{}", csv_escape(item), csv_escape(col), q_dec(v)).unwrap();
        }
        fs::write(data.join(format!("forms/{stem}_inputs.csv")), inp).map_err(|e| e.to_string())?;

        let ev = evaluate(form, inputs.iter().map(|(k, v)| (k.as_str(), v.clone())), Missing::Zero).map_err(|e| format!("{e:?}"))?;
        let mut exp = String::from("item_id,col,expected_val\n");
        for it in &form.items {
            for (col, _) in &it.cells {
                let k = format!("{}:{col}", it.id);
                if let Some(q) = ev.get(&k).num() {
                    writeln!(exp, "{},{},{}", csv_escape(&it.id), csv_escape(col), q_dec(q)).unwrap();
                }
            }
        }
        fs::write(data.join(format!("forms/{stem}_expected.csv")), exp).map_err(|e| e.to_string())?;
        Ok(())
    };
    write_form_io("arf_210_1a", arf210::lcr_all(), &t.ret.inputs)?;
    write_form_io("arf_210_6", arf210::nsfr(), &t.ret.nsfr_inputs)?;

    // 5. Ready-to-run DuckDB driver script
    let run_sql = r#"-- DuckDB end-to-end driver script generated by `steel --export-sql`
-- Run with: duckdb < run.sql (from the repository root or adjust paths)

CREATE SCHEMA IF NOT EXISTS canonical;
USE canonical;
.read sql/canonical_counterparty.sql
COPY counterparty FROM 'data/counterparty.csv' (HEADER, NULL '');

CREATE SCHEMA IF NOT EXISTS retail_sme;
USE retail_sme;
.read sql/modules/retail_sme.sql
COPY retailandsmedeposits_params FROM 'data/retail_sme/params.csv' (HEADER, NULL '');
COPY customer                    FROM 'data/retail_sme/customer.csv' (HEADER, NULL '');
COPY deposit                     FROM 'data/retail_sme/deposit.csv' (HEADER, NULL '');
COPY alloc_covered               FROM 'data/retail_sme/alloc_covered.csv' (HEADER, NULL '');

CREATE SCHEMA IF NOT EXISTS wholesale;
USE wholesale;
.read sql/modules/wholesale.sql
COPY wholesaledeposits_params FROM 'data/wholesale/params.csv' (HEADER, NULL '');
COPY customer                 FROM 'data/wholesale/customer.csv' (HEADER, NULL '');
COPY deposit                  FROM 'data/wholesale/deposit.csv' (HEADER, NULL '');
COPY alloc_insured            FROM 'data/wholesale/alloc_insured.csv' (HEADER, NULL '');

CREATE SCHEMA IF NOT EXISTS forms;
USE forms;
.read sql/forms/arf_210_1a.sql
.read sql/forms/arf_210_6.sql
COPY arf210_1a_inputs FROM 'data/forms/arf_210_1a_inputs.csv' (HEADER, NULL '');
COPY arf210_6_inputs  FROM 'data/forms/arf_210_6_inputs.csv' (HEADER, NULL '');

-- Differential check: DuckDB vs Rust exact-rational oracle (should return 0 rows)
CREATE TABLE arf210_1a_expected (item_id VARCHAR, col VARCHAR, expected_val DECIMAL(38, 10));
COPY arf210_1a_expected FROM 'data/forms/arf_210_1a_expected.csv' (HEADER, NULL '');

SELECT c.item_id, c.col, c.val AS duckdb_val, e.expected_val AS rust_val,
       ABS(c.val - e.expected_val) AS diff
FROM arf210_1a_cells c
JOIN arf210_1a_expected e ON c.item_id = e.item_id AND c.col = e.col
WHERE ABS(c.val - e.expected_val) > 1e-6;

-- Form validation checks (should return 0 rows)
SELECT * FROM arf210_1a_checks WHERE NOT passed;
SELECT * FROM arf210_6_checks  WHERE NOT passed;
"#;
    fs::write(dir.join("run.sql"), run_sql).map_err(|e| e.to_string())?;
    Ok(())
}
