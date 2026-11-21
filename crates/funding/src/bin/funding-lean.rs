//! Generate lean/Prudence/Funding.lean from the spec.
//!   cargo run -p funding --bin funding-lean

fn main() {
    let src = funding::lean_source().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean/Prudence/Funding.lean");
    std::fs::write(path, src).unwrap();
    println!("wrote {path}");
}
