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
        assert!(c.has_contract, "{name} has no contract block");
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
fn the_interactive_primitives_hold_the_forty_eight_pixel_floor() {
    // The corpus violated this in five separate components because the height lived only
    // in a Tailwind class string. Here it is data on the variant, so it is checkable — and
    // this test is the check until the contract evaluator lands with the IR.
    for file in ["button.mz", "input.mz"] {
        let src = read(&primitives_dir().join(file));
        let (component, _) = check_with_ast(&src, file);
        let c = component.unwrap();
        let size_enum = c
            .enums
            .iter()
            .find(|e| e.name.ends_with("_size"))
            .unwrap_or_else(|| panic!("{file} must declare a size enum"));
        assert!(!size_enum.variants.is_empty());
        for v in &size_enum.variants {
            let height = v
                .columns
                .iter()
                .find(|(col, _)| col == "height")
                .unwrap_or_else(|| panic!("{file}: `{}` has no height column", v.name));
            let value: i64 = height.1.parse().unwrap_or_else(|_| {
                panic!("{file}: `{}` height `{}` is not a number", v.name, height.1)
            });
            assert!(
                value >= 48,
                "{file}: `{}` is {value}px, below the 48px floor",
                v.name
            );
        }
    }
}

#[test]
fn the_alert_pins_its_aria_role_to_its_severity() {
    // `announce` is a non-styling column: the a11y role and the colour are one decision per
    // severity, so they cannot drift apart the way parallel maps did.
    let src = read(&primitives_dir().join("alert.mz"));
    let (component, _) = check_with_ast(&src, "alert.mz");
    let c = component.unwrap();
    let variants = &c
        .enums
        .iter()
        .find(|e| e.name == "alert_variant")
        .unwrap()
        .variants;
    for v in variants {
        let announce = v
            .columns
            .iter()
            .find(|(col, _)| col == "announce")
            .unwrap_or_else(|| panic!("`{}` has no announce column", v.name));
        assert!(
            announce.1 == "\"status\"" || announce.1 == "\"alert\"",
            "`{}` announces {}, which is not an ARIA live role",
            v.name,
            announce.1
        );
    }
    let destructive = variants.iter().find(|v| v.name == "destructive").unwrap();
    let announce = &destructive
        .columns
        .iter()
        .find(|(c, _)| c == "announce")
        .unwrap()
        .1;
    assert_eq!(
        announce, "\"alert\"",
        "a destructive alert must announce assertively"
    );
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
