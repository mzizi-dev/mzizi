//! End-to-end tests: run the real `mzizi-benchmark-runner` binary, which itself shells out to
//! the real `mz` binary and reads `benchmarks/harness/tests/fixtures/button_broken.mz`, a real
//! fixture from another workspace member.

use std::path::{Path, PathBuf};
use std::process::Output;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn run(args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_mzizi-benchmark-runner"))
        .args(args)
        .output()
        .expect("failed to run mzizi-benchmark-runner")
}

/// A results file scoped to this test process (its PID), deleted before use so repeated runs
/// of this test don't accumulate stale lines across test invocations.
fn scratch_results_path(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mzizi-benchmark-runner-test-{label}-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn a_syntax_error_then_a_clean_but_wrong_candidate_reaches_a_scored_defect() {
    let root = repo_root();
    let syntax_error =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/button_syntax_error.mz");
    let broken = root.join("benchmarks/harness/tests/fixtures/button_broken.mz");
    let reference = root.join("benchmarks/harness/tests/fixtures/button_reference.rs");
    let results = scratch_results_path("defect");

    let output = run(&[
        "run",
        "--component",
        "button",
        "--reference",
        reference.to_str().unwrap(),
        "--candidate",
        syntax_error.to_str().unwrap(),
        "--candidate",
        broken.to_str().unwrap(),
        "--results",
        results.to_str().unwrap(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
        "expected a non-zero exit for a run with a real defect; got:\n{stdout}"
    );
    assert!(stdout.contains("clean after 2 iteration(s)"), "{stdout}");
    assert!(
        stdout.contains("OK — 5 contract clause(s), 0 failures"),
        "{stdout}"
    );
    assert!(stdout.contains("1 defect(s)"), "{stdout}");
    assert!(
        stdout.contains(r#""iterations_to_clean_compile":2"#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""tokens_consumed":null"#), "{stdout}");
    assert!(stdout.contains(r#""defect_count":1"#), "{stdout}");
    assert!(
        stdout.contains(r#""authoring_mode":"scripted-stand-in""#),
        "{stdout}"
    );

    let recorded = std::fs::read_to_string(&results).expect("results file must be written");
    assert_eq!(recorded.lines().count(), 1);
    assert!(recorded.contains(r#""component":"button""#));
    let _ = std::fs::remove_file(&results);
}

#[test]
fn the_real_button_component_reaches_a_clean_zero_defect_run() {
    let root = repo_root();
    let syntax_error =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/button_syntax_error.mz");
    let real_button = root.join("primitives/button.mz");
    let reference = root.join("benchmarks/harness/tests/fixtures/button_reference.rs");
    let results = scratch_results_path("clean");

    let output = run(&[
        "run",
        "--component",
        "button",
        "--reference",
        reference.to_str().unwrap(),
        "--candidate",
        syntax_error.to_str().unwrap(),
        "--candidate",
        real_button.to_str().unwrap(),
        "--results",
        results.to_str().unwrap(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "expected a zero exit for a clean, zero-defect run; got:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("0 defect(s)"), "{stdout}");
    assert!(stdout.contains(r#""defect_count":0"#), "{stdout}");

    let _ = std::fs::remove_file(&results);
}

#[test]
fn a_candidate_sequence_that_never_compiles_cleanly_is_recorded_honestly() {
    let syntax_error =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/button_syntax_error.mz");
    let root = repo_root();
    let reference = root.join("benchmarks/harness/tests/fixtures/button_reference.rs");
    let results = scratch_results_path("nevercleans");

    let output = run(&[
        "run",
        "--component",
        "button",
        "--reference",
        reference.to_str().unwrap(),
        "--candidate",
        syntax_error.to_str().unwrap(),
        "--results",
        results.to_str().unwrap(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!output.status.success());
    assert!(
        stdout.contains("No candidate reached a clean compile."),
        "{stdout}"
    );

    let recorded = std::fs::read_to_string(&results).expect("results file must be written");
    assert!(recorded.contains(r#""iterations_to_clean_compile":null"#));
    assert!(recorded.contains(r#""defect_count":null"#));
    assert!(recorded.contains("none of 1 candidate(s) reached a clean"));

    let _ = std::fs::remove_file(&results);
}
