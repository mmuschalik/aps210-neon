//! Generate lean/Prudence/SecuredFunding.lean.
//!   cargo run -p secured --bin secured-lean
fn main() {
    let src = secured::lean_source().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean/Prudence/SecuredFunding.lean");
    std::fs::write(path, src).unwrap();
    println!("wrote {path}");
}
