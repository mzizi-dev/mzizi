//! The RFC-0001 worked example must parse clean.
//!
//! The example is a real corpus component (`nyuchi-connectivity-bar`, N7) whose TypeScript
//! reference and Rust port both exist, so its contract is known ground truth. If the
//! grammar cannot express the corpus, the grammar is wrong — this test is the gate that
//! keeps the RFC honest.

use std::path::PathBuf;

use mzizi_lang_compiler::check_with_ast;

fn example(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(format!("{name}.mz"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path:?}: {e}"))
}

#[test]
fn the_rfc_worked_example_parses_without_errors() {
    let src = example("connectivity_bar");
    let (component, report) = check_with_ast(&src, "connectivity_bar.mz");
    let errors: Vec<&str> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == mzizi_lang_compiler::diagnostic::Severity::Error)
        .map(|d| d.say.as_str())
        .collect();
    assert!(
        errors.is_empty(),
        "the RFC example must parse clean, got: {errors:#?}"
    );
    assert!(component.is_some());
}

#[test]
fn the_component_identity_matches_the_corpus_name() {
    let src = example("connectivity_bar");
    let (component, _) = check_with_ast(&src, "connectivity_bar.mz");
    let c = component.unwrap();
    // The corpus name is `nyuchi-connectivity-bar`; RFC-0001 §2's fixed rule maps it to
    // this source name.
    assert_eq!(c.name, "connectivity_bar");
}

#[test]
fn the_capability_declaration_is_captured() {
    let src = example("connectivity_bar");
    let (component, _) = check_with_ast(&src, "connectivity_bar.mz");
    // RFC-0001 §1.7: an agent reading the first lines knows the blast radius.
    assert_eq!(component.unwrap().uses, vec!["motion"]);
}

#[test]
fn all_four_connection_states_carry_both_columns() {
    let src = example("connectivity_bar");
    let (component, _) = check_with_ast(&src, "connectivity_bar.mz");
    let c = component.unwrap();
    let e = c
        .enums
        .iter()
        .find(|e| e.name == "connection_state")
        .expect("enum must parse");
    assert_eq!(e.variants.len(), 4);
    for v in &e.variants {
        let cols: Vec<&str> = v.columns.iter().map(|(c, _)| c.as_str()).collect();
        assert!(
            cols.contains(&"label"),
            "variant {} lost its label column",
            v.name
        );
        assert!(
            cols.contains(&"color"),
            "variant {} lost its color column",
            v.name
        );
    }
}

#[test]
fn the_column_values_match_the_corpus_ground_truth() {
    let src = example("connectivity_bar");
    let (component, _) = check_with_ast(&src, "connectivity_bar.mz");
    let c = component.unwrap();
    let e = c
        .enums
        .iter()
        .find(|e| e.name == "connection_state")
        .unwrap();
    let offline = e.variants.iter().find(|v| v.name == "offline").unwrap();
    let label = &offline
        .columns
        .iter()
        .find(|(c, _)| c == "label")
        .unwrap()
        .1;
    let color = &offline
        .columns
        .iter()
        .find(|(c, _)| c == "color")
        .unwrap()
        .1;
    // These are the strings the Rust port and the .tsx both assert.
    assert_eq!(label, "\"You are offline\"");
    assert_eq!(color, "\"bg-terracotta\"");
}

#[test]
fn every_prop_from_the_rust_port_is_present() {
    let src = example("connectivity_bar");
    let (component, _) = check_with_ast(&src, "connectivity_bar.mz");
    let c = component.unwrap();
    let names: Vec<&str> = c.props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["state", "visible", "on_state_change"]);
    // `visible` is host-controlled with a default, matching the Rust port's decision to
    // replace the .tsx's internal auto-hide timer with a prop.
    let visible = c.props.iter().find(|p| p.name == "visible").unwrap();
    assert!(visible.has_default);
    assert_eq!(visible.ty, "bool");
    // The event prop carries its payload type.
    let on_change = c
        .props
        .iter()
        .find(|p| p.name == "on_state_change")
        .unwrap();
    assert_eq!(on_change.ty, "event(connection_state)");
}

#[test]
fn the_view_tree_and_contract_are_both_present() {
    let src = example("connectivity_bar");
    let (component, _) = check_with_ast(&src, "connectivity_bar.mz");
    let c = component.unwrap();
    assert!(c.has_contract(), "the example declares a contract");
    // The body is retained, not just its presence — the four assertions RFC-0001 §1 shows.
    assert_eq!(c.contract.as_ref().unwrap().clauses.len(), 4);
    assert_eq!(c.fns, vec!["retry"]);
    let view = c.view.expect("the example has a view block");
    assert!(!view.is_empty(), "the view must have elements");
}

#[test]
fn a_component_with_no_contract_warns_but_does_not_fail() {
    // RFC-0001 §1.6: contracts carry the Phase 0 defect metric, so their absence is
    // surfaced — but it is a warning, because it is not a syntax error.
    let report = mzizi_lang_compiler::check("component a\nend component a\n", "a.mz");
    assert_eq!(report.error_count(), 0);
    assert!(report.diagnostics.iter().any(|d| d.code == "MZ0501"));
}
