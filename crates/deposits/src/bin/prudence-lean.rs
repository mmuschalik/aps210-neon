//! Generate lean/Prudence/RetailSme.lean from specs/deposits/retail_sme.prd.
//!   cargo run -p deposits --bin prudence-lean

fn main() {
    let src = deposits::prudence::lean_source().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean/Prudence/RetailSme.lean");
    std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
    std::fs::write(path, src).unwrap();
    println!("wrote {path}");
}
