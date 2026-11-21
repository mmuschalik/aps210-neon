//! Generate lean/Prudence/Commitments.lean from the spec.
//!   cargo run -p obligations --bin obligations-lean

fn main() {
    let src = obligations::lean_source().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean/Prudence/Commitments.lean");
    std::fs::write(path, src).unwrap();
    println!("wrote {path}");
}
