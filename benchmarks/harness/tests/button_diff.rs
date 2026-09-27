//! End-to-end tests: run the real `mzizi-benchmark-harness` binary, which itself shells out
//! to the real `mz contract --agent` (compiler/src/main.rs), against real files on disk.
//!
//! This is the kill-criterion-relevant case RFC-0006 §10.1 asks for: a component whose own
//! contract it satisfies (so `mz contract` alone says nothing is wrong) but whose declared
//! height disagrees with the reference's Tailwind class — the exact "compiles cleanly but is
//! behaviourally wrong against the reference" shape CHARTER.md §6 defines as the Phase 0
//! defect.

use std::path::{Path, PathBuf};
use std::process::Output;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn run_diff(mzizi: &Path, reference: &Path) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_mzizi-benchmark-harness"))
        .arg("diff")
        .arg("--mzizi")
        .arg(mzizi)
        .arg("--reference")
        .arg(reference)
        .output()
        .expect("failed to run mzizi-benchmark-harness")
}

/// Find the report row for `variant` with the given `status` ("PASS" or "FAIL"), matching on
/// whitespace-split tokens so the assertion does not depend on the report's column padding.
fn find_row<'a>(stdout: &'a str, status: &str, variant: &str) -> Option<&'a str> {
    stdout.lines().find(|line| {
        let mut tokens = line.split_whitespace();
        tokens.next() == Some(status) && tokens.next() == Some(variant)
    })
}

#[test]
fn the_real_button_component_has_zero_defects_against_its_reference() {
    let mzizi = repo_root().join("primitives/button.mz");
    let reference = fixture("button_reference.rs");

    let output = run_diff(&mzizi, &reference);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "expected a clean diff for primitives/button.mz; got:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("0 defect(s)"),
        "expected zero defects; got:\n{stdout}"
    );

    // The 48px touch floor is the whole point of this component (button.mz's own header
    // comment) — assert the passing rows are actually present, not just that nothing failed.
    for (variant, declared, token) in [
        ("default", 56, "h-14"),
        ("sm", 48, "h-12"),
        ("lg", 56, "h-14"),
        ("icon", 56, "size-14"),
        ("icon_sm", 48, "size-12"),
    ] {
        let row = find_row(&stdout, "PASS", variant)
            .unwrap_or_else(|| panic!("expected a PASS row for {variant}; got:\n{stdout}"));
        assert!(
            row.contains(&format!("declared={declared}px")),
            "row: {row}"
        );
        assert!(row.contains(&format!("derived={declared}px")), "row: {row}");
        assert!(row.contains(&format!("({token})")), "row: {row}");
    }
}

#[test]
fn a_component_that_satisfies_its_own_contract_but_disagrees_with_the_reference_is_caught() {
    let mzizi = fixture("button_broken.mz");
    let reference = fixture("button_reference.rs");

    let output = run_diff(&mzizi, &reference);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The self-consistency prerequisite must have passed — button_broken.mz's own contract
    // was weakened to agree with its own (wrong) declaration, which is exactly the FM-11
    // "parallel truth" scenario RFC-0006 names: `mz contract` alone has nothing to say here.
    assert!(
        stdout.contains("Checking self-consistency") && stdout.contains("OK"),
        "expected the self-consistency prerequisite to pass; got:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !output.status.success(),
        "expected the reference diff to fail on the broken fixture; got:\n{stdout}"
    );
    assert!(
        stdout.contains("1 defect(s)"),
        "expected exactly one defect (sm); got:\n{stdout}"
    );

    let sm_row = find_row(&stdout, "FAIL", "sm")
        .unwrap_or_else(|| panic!("expected a FAIL row for sm; got:\n{stdout}"));
    assert!(sm_row.contains("declared=44px"));
    assert!(sm_row.contains("derived=48px"));
    assert!(sm_row.contains("(h-12)"));
    assert!(sm_row.contains("mismatch"));

    // Every other variant must still pass — this is the one mismatch, not a cascade.
    for variant in ["default", "lg", "icon", "icon_sm"] {
        find_row(&stdout, "PASS", variant)
            .unwrap_or_else(|| panic!("expected a PASS row for {variant}; got:\n{stdout}"));
    }
}
