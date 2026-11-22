//! `cargo run -p trace`            write target/trace/index.html and print the checks
//! `cargo run -p trace -- --check` also exit non-zero on dangling citations or unknown ARS items

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use trace::FindingKind;

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repository root");
    let report = match trace::build(&root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("trace: {e}");
            return ExitCode::FAILURE;
        }
    };
    let rev = Command::new("git")
        .args(["-C", &root.to_string_lossy(), "describe", "--always", "--dirty"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let out = root.join("target/trace/index.html");
    std::fs::create_dir_all(out.parent().unwrap()).expect("create target/trace");
    std::fs::write(&out, trace::html::render(&report, &rev)).expect("write index.html");

    println!("{} citations in {} specs", report.sites.len(), trace::sites::spec_files(&root).len());
    for s in &report.sources {
        let (c, t) = report.coverage(s.doc);
        println!("  {:<9} {c:>3} / {t:<3} paragraphs cited", s.doc.name());
    }
    for f in &report.failures {
        println!("not parsed: {}: {}", f.file, f.error);
    }
    let mut bad = 0;
    for f in &report.findings {
        if f.kind == FindingKind::Note {
            continue;
        }
        bad += 1;
        let s = &report.sites[f.site];
        println!("{}: {}:{}: {} ({})", f.kind.name(), s.file, s.line, f.detail, s.title);
    }
    let notes = report.findings.iter().filter(|f| f.kind == FindingKind::Note).count();
    println!("{notes} notes written as @cite (see the Checks tab)");
    println!("wrote {}", out.display());
    if check && (bad > 0 || !report.failures.is_empty()) { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
