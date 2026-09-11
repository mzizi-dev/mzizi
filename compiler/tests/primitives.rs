//! Every primitive must parse clean, and keep the properties it was written for.
//!
//! The directory is walked rather than listed, so a new primitive is covered the moment it
//! lands — nobody has to remember to register it here.

use std::path::{Path, PathBuf};

use mzizi_lang_compiler::diagnostic::Severity;
use mzizi_lang_compiler::{check, check_with_ast};

fn primitives_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../primitives")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path:?}: {e}"))
}

fn every_primitive() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let dir = primitives_dir();
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot list {dir:?}: {e}")) {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("mz") {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            out.push((name, read(&path)));
        }
    }
    out.sort();
    assert!(!out.is_empty(), "no primitives found in {dir:?}");
    out
}

#[test]
fn every_primitive_parses_without_errors() {
    for (name, src) in every_primitive() {
        let report = check(&src, &name);
        let errors: Vec<String> = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| format!("[{}] {}", d.code, d.say))
            .collect();
        assert!(
            errors.is_empty(),
            "{name} must parse clean, got: {errors:#?}"
        );
    }
}

#[test]
fn every_primitive_declares_a_contract() {
    // A primitive without a contract is unverified behaviour, and primitives are the
    // components everything else composes — RFC-0001 §1.6 makes the absence a warning in
    // general, but for this directory it is a hard rule.
    for (name, src) in every_primitive() {
        let (component, _) = check_with_ast(&src, &name);
        let c = component.unwrap_or_else(|| panic!("{name} did not parse into a component"));
        assert!(c.has_contract(), "{name} has no contract block");
    }
}

#[test]
fn every_primitive_names_itself_after_its_file() {
    // One component, one file, name-identical (RFC-0001 §7.4). This is what makes a
    // cross-file reference an exact anchor.
    for (name, src) in every_primitive() {
        let (component, _) = check_with_ast(&src, &name);
        let c = component.unwrap();
        let stem = name.trim_end_matches(".mz");
        assert_eq!(c.name, stem, "{name} declares `component {}`", c.name);
    }
}

#[test]
fn every_primitive_documents_itself() {
    for (name, src) in every_primitive() {
        let (component, _) = check_with_ast(&src, &name);
        let c = component.unwrap();
        assert!(!c.docs.is_empty(), "{name} has no `##` doc line");
    }
}

#[test]
fn the_rules_the_corpus_kept_breaking_are_expressed_as_contract_assertions() {
    // These checks used to be re-implemented in Rust here, with a comment saying so: "this
    // test is the check until the contract evaluator lands". That is FM-11 (RFC-0006 §0) —
    // one rule written twice, in two languages, free to drift. The evaluator has landed, so
    // this test no longer restates the rules; it asserts that each primitive still
    // *expresses* them, and `tests/contracts.rs` is what runs them.
    let expected: &[(&str, &[&str])] = &[
        // The 48px touch floor, violated five separate times in the .tsx corpus.
        ("button.mz", &["every button_size height at_least 48"]),
        ("input.mz", &["every input_size height at_least 48"]),
        // `announce` pins the ARIA role to the severity so the two cannot drift apart.
        (
            "alert.mz",
            &[
                "every alert_variant announce in \"status\" \"alert\"",
                "alert_variant.destructive announce is \"alert\"",
            ],
        ),
    ];
    for (file, required) in expected {
        let src = read(&primitives_dir().join(file));
        let (component, _) = check_with_ast(&src, file);
        let contract = component
            .and_then(|c| c.contract)
            .unwrap_or_else(|| panic!("{file} has no contract block"));
        let written: Vec<String> = contract.clauses.iter().map(|c| c.canonical()).collect();
        for clause in *required {
            assert!(
                written.iter().any(|w| w == clause),
                "{file} no longer asserts `{clause}`; it says {written:#?}"
            );
        }
    }
}

#[test]
fn composition_needs_no_import_line() {
    // `confirm_bar` uses `button` and `alert` by name. The flat, uniquely-named namespace
    // plus hash resolution in the manifest means source carries zero import ceremony — see
    // primitives/README.md.
    let src = read(&primitives_dir().join("confirm_bar.mz"));
    let (component, report) = check_with_ast(&src, "confirm_bar.mz");
    assert_eq!(report.error_count(), 0);
    let c = component.unwrap();
    // `use` is reserved for capabilities, so a composing component declares none.
    assert!(
        c.uses.is_empty(),
        "composition must not need a `use` line, found {:?}",
        c.uses
    );
    let view = c.view.expect("confirm_bar has a view");
    let rendered = format!("{view:?}");
    assert!(
        rendered.contains("button"),
        "confirm_bar must compose `button`"
    );
    assert!(
        rendered.contains("alert"),
        "confirm_bar must compose `alert`"
    );
}

#[test]
fn the_whole_primitive_set_checks_in_well_under_the_loop_budget() {
    // Compile speed is a Phase 0 success metric, not an optimization to defer
    // (RFC-0001 §3). Checking the entire set must stay far inside one human-perceptible
    // moment, or the agent loop's dominant cost is the compiler.
    let all = every_primitive();
    let started = std::time::Instant::now();
    for (name, src) in &all {
        let _ = check(src, name);
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_millis() < 250,
        "checking {} primitives took {elapsed:?}, over the 250ms budget",
        all.len()
    );
}
