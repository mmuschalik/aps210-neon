//! `cargo run -p prudence-core --bin prudence-witness`
//!
//! Runs Z3 branch-witness synthesis across all 7 Lean-exported Prudence modules
//! (`specs/ir/modules/*.json`), printing branch coverage & any unreachable
//! branches, and writing concrete SQL `INSERT` witness datasets to
//! `sql/witnesses/<module>.sql`.

use std::fs;
use std::path::Path;

use prudence_core::syntax::load_module_json;
use prudence_core::syntax::witness::generate_witnesses;

fn main() {
    let modules: &[(&str, &[&str])] = &[
        ("retail_sme", &["Arf210_1A", "Arf210_1B", "Arf210_2", "Arf210_6"]),
        ("wholesale", &["Arf210_1A", "Arf210_1B", "Arf210_2", "Arf210_6"]),
        ("secured_funding", &["Arf210_1A"]),
        ("loans", &["Arf210_1A", "Arf210_6"]),
        ("derivatives", &[]),
        ("funding", &[]),
        ("commitments", &[]),
    ];

    fs::create_dir_all("sql/witnesses").unwrap();
    let (mut tot, mut sat, mut unsat) = (0usize, 0usize, 0usize);

    println!("{:<22} {:>8} {:>10} {:>12}", "Module", "Branches", "SAT (rows)", "UNSAT (dead)");
    println!("{}", "-".repeat(56));

    for (stem, forms) in modules {
        let path = format!("specs/ir/modules/{stem}.json");
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let m = load_module_json(&src, forms).unwrap_or_else(|e| panic!("{path}:\n{}", e.join("\n")));
        let rep = generate_witnesses(&m).unwrap_or_else(|e| panic!("{stem}: {e}"));
        let out_path = Path::new("sql/witnesses").join(format!("{stem}.sql"));
        fs::write(&out_path, &rep.sql).unwrap();

        println!("{:<22} {:>8} {:>10} {:>12}", rep.module_name, rep.total_branches, rep.reachable, rep.unreachable);
        for w in rep.witnesses.iter().filter(|w| !w.reachable) {
            println!("   [UNSAT] {} => {}", w.path_label, w.outcome);
        }
        tot += rep.total_branches;
        sat += rep.reachable;
        unsat += rep.unreachable;
    }

    println!("{}", "-".repeat(56));
    println!("{:<22} {:>8} {:>10} {:>12}", "TOTAL", tot, sat, unsat);
    println!("\nWrote SQL witness datasets to sql/witnesses/*.sql");
}
