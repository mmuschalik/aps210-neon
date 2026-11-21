//! The Lean model generated from the spec, and the proofs about it.

use std::path::PathBuf;
use std::process::Command;

fn lean_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean"))
}

fn lake(args: &[&str]) -> (bool, String) {
    let out = Command::new("lake").args(args).current_dir(lean_dir()).output().expect("`lake` (elan) must be installed");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success() && !text.contains("error:"), text)
}

#[test]
fn generated_model_is_up_to_date() {
    let want = obligations::lean_source().unwrap();
    let have = std::fs::read_to_string(lean_dir().join("Prudence/Commitments.lean")).unwrap();
    assert!(have == want, "lean/Prudence/Commitments.lean is stale: run `cargo run -p obligations --bin obligations-lean`");
}

#[test]
fn proofs_check_with_standard_axioms_only() {
    let proofs = std::fs::read_to_string(lean_dir().join("Prudence/CommitmentsProofs.lean")).unwrap();
    assert!(!proofs.contains("sorry") && !proofs.contains("admit") && !proofs.contains("native_decide"));
    let (ok, out) = lake(&["build"]);
    assert!(ok, "lake build failed:\n{out}");
    let dir = std::env::temp_dir().join(format!("prudence-obligations-lean-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let thms = ["unknown_drawability_is_drawable", "undrawable_reports_nowhere"];
    let src: String = "import Prudence\n".to_string() + &thms.iter().map(|t| format!("#print axioms Commitments.{t}\n")).collect::<String>();
    let path = dir.join("Axioms.lean");
    std::fs::write(&path, src).unwrap();
    let (ok, out) = lake(&["env", "lean", path.to_str().unwrap()]);
    assert!(ok, "{out}");
    let flat = out.split_whitespace().collect::<Vec<_>>().join(" ");
    for t in thms {
        assert!(flat.contains(&format!("'Commitments.{t}'")), "{t}: {out}");
        for ax in ["sorryAx", "Lean.ofReduceBool", "Lean.trustCompiler"] {
            assert!(!flat.contains(ax), "{t} uses {ax}: {out}");
        }
    }
}
