//! `cargo run -p arf210`            list forms
//! `cargo run -p arf210 -- 210.1A`  print a spec for review against the PDF

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
