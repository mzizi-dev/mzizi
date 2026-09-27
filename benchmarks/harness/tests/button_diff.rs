//! End-to-end tests: run the real `mzizi-benchmark-harness` binary against real files on
//! disk. `diff` itself shells out to the real `mz contract --agent` (compiler/src/main.rs).
//!
//! The reference is `tests/fixtures/button_reference.rs`, a byte-identical copy of the
//! registry's `components/registry/n2-primitives/button.rs` (provenance in
//! `tests/fixtures/README.md`). It has two enums that both have a `Default` variant, a
//! `slug()` beside every `classes()`, and block-bodied arms — the three shapes that made the
//! first, line-oriented extractor report five false defects against it.
//!
//! The broken case is the kill-criterion shape RFC-0006 §10.1 asks for: a component whose own
//! contract it satisfies (so `mz contract` alone says nothing is wrong) but whose declared
//! height disagrees with the reference's Tailwind class — the "compiles cleanly but is
//! behaviourally wrong against the reference" shape CHARTER.md §6 defines as the defect.

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

fn harness(args: &[&std::ffi::OsStr]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_mzizi-benchmark-harness"))
        .args(args)
        .output()
        .expect("failed to run mzizi-benchmark-harness")
}

fn run_diff(mzizi: &Path, reference: &Path) -> Output {
    harness(&[
        "diff".as_ref(),
        "--mzizi".as_ref(),
        mzizi.as_os_str(),
        "--reference".as_ref(),
        reference.as_os_str(),
    ])
}

fn run_score(arm: &str, candidate: &Path, reference: &Path) -> (Output, String) {
    run_score_with(arm, candidate, reference, &[])
}

fn run_score_with(
    arm: &str,
    candidate: &Path,
    reference: &Path,
    extra: &[&str],
) -> (Output, String) {
    let mut args: Vec<&std::ffi::OsStr> = vec![
        "score".as_ref(),
        "--arm".as_ref(),
        arm.as_ref(),
        "--candidate".as_ref(),
        candidate.as_os_str(),
        "--reference".as_ref(),
        reference.as_os_str(),
    ];
    args.extend(extra.iter().map(|a| std::ffi::OsStr::new(*a)));
    let out = harness(&args);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    (out, stdout)
}

/// Find the report row for `variant` with the given `status` ("PASS" or "FAIL"), matching on
/// whitespace-split tokens so the assertion does not depend on the report's column padding.
fn find_row<'a>(stdout: &'a str, status: &str, variant: &str) -> Option<&'a str> {
    stdout.lines().find(|line| {
        let mut tokens = line.split_whitespace();
        tokens.next() == Some(status) && tokens.next() == Some(variant)
    })
}

/// Every `{...}` detail object in a score JSON line (the details hold no nested objects).
fn details(json: &str) -> Vec<&str> {
    let start = json.find("\"details\":[").expect("details array") + "\"details\":[".len();
    let end = json
        .rfind("],\"class_token_jaccard\"")
        .expect("jaccard after details");
    json[start..end]
        .split("},{")
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn the_real_button_component_has_zero_height_defects_against_the_real_reference() {
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

#[test]
fn score_mzizi_button_against_the_real_reference() {
    let (output, stdout) = run_score(
        "mzizi",
        &repo_root().join("primitives/button.mz"),
        &fixture("button_reference.rs"),
    );
    assert!(output.status.success(), "{stdout}");
    assert_eq!(stdout.lines().count(), 1, "exactly one JSON line: {stdout}");
    // Two variant sets, two defaults, five heights — and button.mz agrees with the real
    // reference on all nine.
    assert!(
        stdout
            .starts_with(r#"{"arm":"mzizi","facts_checked":9,"defects":0,"renames":0,"details":["#),
        "{stdout}"
    );
    assert_eq!(details(&stdout).len(), 9);
    assert!(
        stdout.contains(r#""enum":"button_size","variant":"sm","fact":"height","expected":"48px","actual":"48px","defect":false"#),
        "{stdout}"
    );
    // The class strings are not identical: button.mz drops the reference's
    // `has-data-[icon=...]` and `aria-expanded:` classes, among others. The run on the real
    // files printed 0.7311; that is reported, not counted (see `class_token_jaccard`'s doc).
    assert!(
        stdout.ends_with("\"class_token_jaccard\":0.7311}\n"),
        "{stdout}"
    );
}

#[test]
fn score_mzizi_broken_button_finds_the_one_height_defect() {
    let (output, stdout) = run_score(
        "mzizi",
        &fixture("button_broken.mz"),
        &fixture("button_reference.rs"),
    );
    assert!(output.status.success(), "defects still exit 0: {stdout}");
    assert!(
        stdout.contains(r#""facts_checked":9,"defects":1,"renames":0,"#),
        "{stdout}"
    );
    let defects: Vec<&str> = details(&stdout)
        .into_iter()
        .filter(|d| d.contains("\"defect\":true"))
        .collect();
    assert_eq!(defects.len(), 1, "{stdout}");
    assert!(
        defects[0].contains(r#""variant":"sm","fact":"height","expected":"48px","actual":"44px""#),
        "{}",
        defects[0]
    );
}

#[test]
fn score_dioxus_reference_against_itself_is_clean() {
    let reference = fixture("button_reference.rs");
    let (output, stdout) = run_score("dioxus", &reference, &reference);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.starts_with(r#"{"arm":"dioxus","facts_checked":9,"defects":0,"renames":0,"#),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("\"class_token_jaccard\":1.0000}\n"),
        "{stdout}"
    );
}

/// The 2026-09-27 pilot's changelog candidates, one per arm, scored against the real task
/// reference. Both followed `spec.tsx`, which keys its four colour classes by axis
/// (`horizontal`, …); the reference names the same four class strings by mineral
/// (`cobalt`, …). Without `--allow-variant-renames` both score 2 defects (`variant_set`,
/// `default`) and a null jaccard — the name-only result the pilot recorded. With it (the
/// task's `task.toml` opts in, so `mzbench` passes it) the variants pair by class.
#[test]
fn score_pilot_changelog_candidates_pair_the_renamed_variants_by_class() {
    let reference = repo_root().join("benchmarks/tasks/mzizi-changelog-renderer/reference.rs");
    let pilot = repo_root().join("benchmarks/results/2026-09-27-pilot/claude-subagent");
    for (arm, file) in [("mzizi", "candidate.mz"), ("dioxus", "candidate.rs")] {
        let candidate = pilot
            .join(arm)
            .join("nyuchi-changelog-renderer/seed-0/iter-01")
            .join(file);
        let (output, stdout) = run_score(arm, &candidate, &reference);
        assert!(output.status.success(), "{arm}: {stdout}");
        assert!(
            stdout.contains(r#""facts_checked":2,"defects":2,"renames":0,"#),
            "{arm}, name-only: {stdout}"
        );
        assert!(
            stdout.ends_with("\"class_token_jaccard\":null}\n"),
            "{arm}, name-only: {stdout}"
        );

        let (output, stdout) =
            run_score_with(arm, &candidate, &reference, &["--allow-variant-renames"]);
        assert!(output.status.success(), "{arm}: {stdout}");
        // variant_set and default are checked; variant_names is a detail, not a fact.
        assert!(
            stdout.contains(r#""facts_checked":2,"defects":0,"renames":4,"#),
            "{arm}: {stdout}"
        );
        assert!(
            stdout.contains(
                r#""fact":"variant_names","expected":"{cobalt, tanzanite, malachite, gold}","actual":"renamed (candidate -> reference): horizontal -> cobalt (jaccard 1.0000), vertical -> tanzanite (jaccard 1.0000), depth -> malachite (jaccard 1.0000), outlier -> gold (jaccard 1.0000)","defect":false"#
            ),
            "{arm}: {stdout}"
        );
        assert!(
            stdout.contains(
                r#""fact":"default","expected":"cobalt","actual":"horizontal -> cobalt","defect":false"#
            ),
            "{arm}: {stdout}"
        );
        assert!(
            stdout.ends_with("\"class_token_jaccard\":1.0000}\n"),
            "{arm}: {stdout}"
        );
    }
}

#[test]
fn score_usage_and_io_errors_exit_2() {
    let reference = fixture("button_reference.rs");
    let (bad_arm, _) = run_score("rust", &reference, &reference);
    assert_eq!(bad_arm.status.code(), Some(2));
    let (missing, stdout) = run_score("mzizi", Path::new("/nonexistent/x.mz"), &reference);
    assert_eq!(missing.status.code(), Some(2));
    assert!(stdout.is_empty(), "no JSON on an IO error: {stdout}");
}
