//! prudence-check <package.json> [--bundle <out.json>] [--allow-partial]
//!
//! Runs the trusted checks on a package the engine wrote out. Exit status 0
//! and an audit bundle if every check passes; exit status 1 otherwise.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(input), rest) = (args.first(), &args[1.min(args.len())..]) else {
        eprintln!("usage: prudence-check <package.json> [--bundle <out.json>] [--allow-partial]");
        return ExitCode::from(2);
    };
    let mut out = None;
    let mut partial = false;
    let mut flags = rest.iter();
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--allow-partial" if !partial => partial = true,
            "--bundle" if out.is_none() => {
                let Some(path) = flags.next() else {
                    eprintln!("--bundle requires a path");
                    return ExitCode::from(2);
                };
                out = Some(path.clone());
            }
            _ => {
                eprintln!("unknown or duplicate argument {flag}");
                return ExitCode::from(2);
            }
        }
    }
    let pkg = std::fs::read_to_string(input)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .and_then(|v| checker::json::from_json(&v));
    let pkg = match pkg {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{input}: {e}");
            return ExitCode::from(2);
        }
    };
    let report = if partial { checker::check(&pkg) } else { checker::check_complete(&pkg) };
    for (name, failures) in &report.checks {
        println!("{} {name}", if failures.is_empty() { "PASS" } else { "FAIL" });
        for f in failures {
            println!("     {f}");
        }
    }
    for w in &report.warnings {
        println!("WARN {w}");
    }
    match &report.bundle {
        Some(b) => {
            println!("bundle {}", b["bundle_id"].as_str().unwrap_or_default());
            if let Some(path) = out
                && let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(b).unwrap())
            {
                eprintln!("{path}: {e}");
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        None => {
            println!("no bundle: the return must not be signed");
            ExitCode::from(1)
        }
    }
}
