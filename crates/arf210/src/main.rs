//! `cargo run -p arf210`            list forms
//! `cargo run -p arf210 -- 210.1A`  print a spec for review against the PDF
//! `cargo run -p arf210 -- fmt 210.1A`  print the spec as canonical Prudence source

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("fmt") {
        let Some(f) = args.get(1).and_then(|c| arf210::form(c)) else {
            eprintln!("usage: arf210 fmt <form>");
            std::process::exit(1);
        };
        print!("{}", prudence_core::syntax::print::print_form(f, &arf210::SPEC_HEADER));
        return;
    }
    match args.first() {
        None => {
            for f in arf210::forms() {
                println!("{:<11} {}  ({} items, {} inputs)", f.code, f.title, f.items.len(), f.input_refs().len());
            }
        }
        Some(code) => match arf210::form(code) {
            Some(f) => print!("{}", f.describe()),
            None => {
                eprintln!("unknown form {code:?}");
                std::process::exit(1);
            }
        },
    }
}
