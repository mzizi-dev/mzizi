//! Contract evaluation, measured against the real corpus.
//!
//! The charter's Phase 0 defect metric is "code that compiles cleanly but is behaviourally
//! wrong" (CHARTER.md §6). That sentence names two runs of the compiler, so these tests
//! check both: every mutation below must leave `mz check` reporting **zero errors** while
//! `mz contract` fails. A mutation that broke the parse would prove nothing — it would be
//! ordinary compile friction, which §6 explicitly excludes from the metric.
//!
//! The mutations are drawn from defect classes the corpus actually produced (RFC-0006 §8),
//! never invented: a size below the touch floor, an ARIA role that drifted from its
//! severity, a design token dropped out of a class string, a composed component removed.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::contract::Tally;
use mzizi_lang_compiler::{check, check_contract};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The nine primitives plus the corpus example, sorted for determinism.
fn corpus() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in ["primitives", "examples"] {
        let path = root().join(dir);
        for entry in std::fs::read_dir(&path).unwrap_or_else(|e| panic!("{path:?}: {e}")) {
            let file = entry.expect("entry").path();
            if file.extension().and_then(|e| e.to_str()) == Some("mz") {
                let name = file.file_name().unwrap().to_string_lossy().to_string();
                out.push((name, std::fs::read_to_string(&file).expect("readable")));
            }
        }
    }
    out.sort();
    assert!(
        out.len() >= 10,
        "expected the primitive set plus the example, got {}",
        out.len()
    );
    out
}

fn source(name: &str) -> String {
    let dir = if name == "connectivity_bar.mz" {
        "examples"
    } else {
        "primitives"
    };
    std::fs::read_to_string(root().join(dir).join(name)).expect("readable corpus file")
}

/// Apply one textual mutation and assert it still compiles — the precondition for it
/// counting as a Phase 0 defect at all.
fn mutate(name: &str, from: &str, to: &str) -> String {
    let src = source(name);
    assert!(
        src.contains(from),
        "{name}: the text this mutation edits is gone — update the test, not the corpus:\n{from}"
    );
    let mutated = src.replacen(from, to, 1);
    let report = check(&mutated, name);
    assert_eq!(
        report.error_count(),
        0,
        "{name}: the mutation must compile cleanly or it is not a behavioural defect: {:#?}",
        report.diagnostics
    );
    mutated
}

fn failures(name: &str, src: &str) -> Vec<String> {
    let (report, _) = check_contract(src, name);
    report
        .diagnostics
        .iter()
        .filter(|d| d.code == "MZ0603" || d.code == "MZ0605")
        .map(|d| d.say.clone())
        .collect()
}

#[test]
fn every_corpus_contract_holds() {
    for (name, src) in corpus() {
        let (report, tally) = check_contract(&src, &name);
        assert_eq!(
            report.error_count(),
            0,
            "{name}: contract evaluation reported errors: {:#?}",
            report.diagnostics
        );
        assert!(
            tally.clauses >= 2,
            "{name}: only {} assertion(s) — a contract this thin is not verifying much",
            tally.clauses
        );
        assert_eq!(
            tally.failed, 0,
            "{name}: {} assertion(s) failed",
            tally.failed
        );
    }
}

#[test]
fn every_corpus_clause_is_actually_evaluated() {
    // The property that makes the whole feature worth having: a clause the evaluator
    // cannot understand must not be able to hide inside a green run. This is the test
    // that would have caught `card_radius uses "--radius-lg"`, which named a subject that
    // did not exist anywhere in `card.mz`.
    let mut total = 0usize;
    for (name, src) in corpus() {
        let (_, tally) = check_contract(&src, &name);
        let written = src
            .lines()
            .skip_while(|l| l.trim() != "contract")
            .skip(1)
            .take_while(|l| l.trim() != "end")
            .filter(|l| !l.trim().is_empty())
            .count();
        assert_eq!(
            tally.clauses, written,
            "{name}: {written} assertion(s) written, {} evaluated",
            tally.clauses
        );
        total += tally.clauses;
    }
    // Measured, so RFC-0006 §9 can quote a number rather than assert one.
    println!("corpus contract coverage: {total} assertions across 10 files, all evaluated");
    assert!(total >= 30, "only {total} assertions in the whole corpus");
}

#[test]
fn a_size_below_the_touch_floor_compiles_clean_and_fails_its_contract() {
    // The corpus defect this feature exists for: the 48px floor, violated five separate
    // times in the TypeScript because the height lived only in a class string.
    let mutated = mutate(
        "button.mz",
        "sm        class \"h-12 gap-1.5 px-4\"   height 48",
        "sm        class \"h-12 gap-1.5 px-4\"   height 40",
    );
    let said = failures("button.mz", &mutated);
    assert_eq!(
        said.len(),
        2,
        "expected the floor and the cell to fail: {said:?}"
    );
    assert!(
        said.iter().any(|s| s.contains("`sm` is 40, below 48")),
        "the diagnostic must name the variant and both numbers: {said:?}"
    );
}

#[test]
fn an_aria_role_that_drifts_from_its_severity_is_caught() {
    // RFC-0001 §1.3's whole argument: the role and the colour are one decision per
    // severity. `alert`'s contract is what holds them together.
    let mutated = mutate(
        "alert.mz",
        "destructive  class \"bg-card text-destructive\"                        announce \"alert\"",
        "destructive  class \"bg-card text-destructive\"                        announce \"quiet\"",
    );
    let said = failures("alert.mz", &mutated);
    assert!(
        said.iter().any(|s| s.contains("in \"status\" \"alert\"")),
        "the closed-set assertion must fail: {said:?}"
    );
    assert!(
        said.iter().any(|s| s.contains("alert_variant.destructive")),
        "the exact-value assertion must fail too: {said:?}"
    );
}

#[test]
fn a_dropped_design_token_is_caught() {
    let mutated = mutate(
        "card.mz",
        "rounded-[var(--radius-lg,14px)]",
        "rounded-[14px]",
    );
    let said = failures("card.mz", &mutated);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("var(--radius-lg"), "{said:?}");
}

#[test]
fn a_component_that_stops_composing_what_it_promised_is_caught() {
    // `confirm_bar` exists to prove composition works with no import line. Deleting the
    // `alert` it composes is a silent behaviour change with nothing else to catch it —
    // there is no import to go stale and no type to complain.
    let src = source("confirm_bar.mz");
    let start = src
        .find("      when destructive")
        .expect("the destructive branch");
    let end = src
        .find("      when not destructive")
        .expect("the default branch");
    let mutated = format!("{}{}", &src[..start], &src[end..]);
    let mutated = mutated.replace(
        "      when not destructive\n        alert\n          variant = default\n          title = \"Please confirm\"\n          message = message\n        end\n      end\n",
        "",
    );
    let report = check(&mutated, "confirm_bar.mz");
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
    let said = failures("confirm_bar.mz", &mutated);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("`alert`"), "{said:?}");
}

#[test]
fn a_guarded_branch_that_stops_rendering_its_control_is_caught() {
    // `when offline shows button "Retry"` — the one assertion in the corpus that is about
    // the view's *shape* rather than a table cell.
    let mutated = mutate(
        "connectivity_bar.mz",
        "text = \"Retry\"",
        "text = \"Try again\"",
    );
    let said = failures("connectivity_bar.mz", &mutated);
    assert_eq!(
        said.len(),
        2,
        "the shows and the min_height clause both key off the text: {said:?}"
    );
    assert!(said.iter().any(|s| s.contains("shows button")), "{said:?}");
}

#[test]
fn a_touch_floor_dropped_from_a_class_string_is_caught() {
    let mutated = mutate("connectivity_bar.mz", "min-h-[48px]", "min-h-[44px]");
    let said = failures("connectivity_bar.mz", &mutated);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("44px"), "{said:?}");
}

#[test]
fn a_file_that_does_not_compile_has_its_contract_skipped_not_failed() {
    // CHARTER.md §6 separates the two metrics: a compile error is friction, not a defect.
    // Evaluating assertions against a tree the parser had to guess at would blur that line.
    let (report, tally) = check_contract(
        "component a\n  enum e\n    one k \"1\"\n  end\n  contract\n    every e k not_empty\n  end\n",
        "a.mz",
    );
    assert!(report.error_count() > 0, "the fixture must not compile");
    assert_eq!(
        tally,
        Tally::default(),
        "no assertion may run on a broken tree"
    );
    assert!(
        !report.diagnostics.iter().any(|d| d.code == "MZ0603"),
        "a compile failure must not be reported as a behavioural defect"
    );
}

#[test]
fn evaluating_the_whole_corpus_stays_inside_the_loop_budget() {
    // Compile speed is a Phase 0 success metric (RFC-0001 §4.6), and the benchmark will run
    // `mz contract` once per iteration on top of `mz check`. If it is not cheap it changes
    // the number it is supposed to be measuring.
    let all = corpus();
    let started = std::time::Instant::now();
    for (name, src) in &all {
        let _ = check_contract(src, name);
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_millis() < 250,
        "parse + evaluate of {} files took {elapsed:?}, over the 250ms budget",
        all.len()
    );
    println!("parse+evaluate of {} files: {elapsed:?}", all.len());
}

#[test]
fn the_shipped_binary_exits_zero_on_a_holding_contract_and_one_on_a_broken_one() {
    // The binary, not the harness. CI's `mz check` steps exist for the same reason: the
    // claim the charter makes is about the thing this project ships.
    let ok = Command::new(env!("CARGO_BIN_EXE_mz"))
        .arg("contract")
        .arg(root().join("primitives/button.mz"))
        .output()
        .expect("mz runs");
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    let broken = std::env::temp_dir().join("mz-contract-broken.mz");
    std::fs::write(
        &broken,
        mutate(
            "button.mz",
            "sm        class \"h-12 gap-1.5 px-4\"   height 48",
            "sm        class \"h-12 gap-1.5 px-4\"   height 40",
        ),
    )
    .expect("writable temp file");

    let checked = Command::new(env!("CARGO_BIN_EXE_mz"))
        .arg("check")
        .arg(&broken)
        .output()
        .expect("mz runs");
    assert_eq!(
        checked.status.code(),
        Some(0),
        "the defect must compile cleanly: {}",
        String::from_utf8_lossy(&checked.stdout)
    );

    let failed = Command::new(env!("CARGO_BIN_EXE_mz"))
        .arg("contract")
        .arg(&broken)
        .output()
        .expect("mz runs");
    assert_eq!(failed.status.code(), Some(1));
    let _ = std::fs::remove_file(&broken);
}

#[test]
fn the_agent_summary_carries_the_contract_counts() {
    // RFC-0001 §4.4 fixed the summary line as carrying contract results from the start.
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(["contract", "--agent"])
        .arg(root().join("primitives/alert.mz"))
        .output()
        .expect("mz runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let summary = text.lines().last().expect("a summary line");
    assert!(
        summary.contains(r#""contract_clauses":4"#),
        "summary was: {summary}"
    );
    assert!(summary.contains(r#""contract_failures":0"#), "{summary}");
}
