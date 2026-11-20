//! Generate lean/Prudence/Wholesale.lean from specs/deposits/wholesale.prd.
//!   cargo run -p deposits --bin wholesale-lean

fn main() {
    let src = deposits::wholesale::lean_source().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean/Prudence/Wholesale.lean");
    std::fs::write(path, src).unwrap();
    println!("wrote {path}");
}
