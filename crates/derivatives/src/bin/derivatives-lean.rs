//! Generate lean/Prudence/Derivatives.lean from the spec.
//!   cargo run -p derivatives --bin derivatives-lean

fn main() {
    let src = derivatives::lean_source().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean/Prudence/Derivatives.lean");
    std::fs::write(path, src).unwrap();
    println!("wrote {path}");
}
