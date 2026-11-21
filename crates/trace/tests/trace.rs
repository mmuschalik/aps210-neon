//! The checks find what they claim to, on a scratch repository with the real
//! APRA pages and a spec with known-bad citations; and the real specs are clean.

use std::path::{Path, PathBuf};
use trace::FindingKind;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const SPEC: &str = r#"
form T("ARF 999.1", "Test form") {
  columns amount
  input amount
  item "1" "Total" {
    amount = [1.1:amount]
  } @cite("sum of items 1.1")
  item "1.1" "A" {
    amount = input
  } @cite("Att A §46(b); APG 210 §109; ARS item 1.1")
  item "1.2" "B" {
    amount = input
  } @cite("APS 210 Att A para 999; Att A para 46(z); Att C Table 9; ARS items 1.x, 77.7")
}
"#;

#[test]
fn findings_on_a_scratch_repo() {
    let dir = std::env::temp_dir().join(format!("trace-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("specs/t")).unwrap();
    for f in ["aps-210.html", "apg-210.html"] {
        std::fs::copy(repo().join(f), dir.join(f)).unwrap();
    }
    std::fs::write(dir.join("specs/t/t.prd"), SPEC).unwrap();
    let r = trace::build(&dir).unwrap();
    std::fs::remove_dir_all(&dir).ok();

    assert!(r.failures.is_empty());
    assert_eq!(r.sites.len(), 3);
    let of = |k: FindingKind| r.findings.iter().filter(|f| f.kind == k).map(|f| f.detail.clone()).collect::<Vec<_>>();
    assert_eq!(of(FindingKind::Dangling), ["APS 210 Att A para 999", "APS 210 Att A para 46(z)", "APS 210 Att C Table 9"]);
    assert_eq!(of(FindingKind::UnknownItem), ["ARS 210.0 item 77.7"]);
    assert_eq!(of(FindingKind::Note), ["sum of items 1.1"]);

    // Para 46 is cited through its sub-paragraph (b); (a) is not cited.
    let p46 = prudence_core::cite::refs("Att A para 46").remove(0);
    let p46a = prudence_core::cite::refs("Att A para 46(a)").remove(0);
    // The dangling 46(z) (site 2) still points into para 46; it is reported above.
    assert_eq!(r.sites_for(&p46), [1, 2]);
    assert!(r.sites_for(&p46a).is_empty());
    let (cited, total) = r.coverage(prudence_core::cite::Doc::Apg210);
    assert_eq!(cited, 1);
    assert!(total > 100);

    let html = trace::html::render(&r, "test");
    assert!(html.contains("\"rev\":\"test\"") && !html.contains("/*DATA*/"));
}

#[test]
fn real_specs_have_no_dangling_citations() {
    let r = trace::build(&repo()).unwrap();
    let bad: Vec<String> = r
        .findings
        .iter()
        .filter(|f| f.kind != FindingKind::Note)
        .map(|f| format!("{}:{}: {}", r.sites[f.site].file, r.sites[f.site].line, f.detail))
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(r.failures.is_empty(), "{:?}", r.failures);
    assert!(r.sites.len() > 500);
}
