//! Generate lean/Prudence/Loans.lean from specs/loans/loans.prd.
//!   cargo run -p loans --bin loans-lean

fn main() {
    let src = loans::lean_source().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean/Prudence/Loans.lean");
    std::fs::write(path, src).unwrap();
    println!("wrote {path}");
}
