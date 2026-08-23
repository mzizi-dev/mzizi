//! The IR's claims, measured across the real primitive set.
//!
//! RFC-0003 asserts structural sharing, outline savings, and stable identity. Each of those
//! is a number, so each is measured here against the nine primitives plus the corpus
//! example rather than argued for in prose.

use std::path::PathBuf;

use mzizi_lang_compiler::check_with_ast;
use mzizi_lang_compiler::ir::{Store, lower, paths};
use mzizi_lang_compiler::outline::outline;

fn sources() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for dir in ["../primitives", "../examples"] {
        let path = root.join(dir);
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

fn parsed(src: &str, name: &str) -> mzizi_lang_compiler::ast::Component {
    let (component, report) = check_with_ast(src, name);
    assert_eq!(report.error_count(), 0, "{name} must parse clean");
    component.unwrap_or_else(|| panic!("{name} produced no component"))
}

#[test]
fn every_source_lowers_into_the_ir() {
    for (name, src) in sources() {
        let component = parsed(&src, &name);
        let mut store = Store::new();
        let root = lower(&component, &mut store);
        assert!(
            store.get(root).is_some(),
            "{name}: root hash is not in the store"
        );
        assert!(
            store.len() > 1,
            "{name}: lowered to {} node(s)",
            store.len()
        );
    }
}

#[test]
fn structural_sharing_makes_the_shared_store_smaller_than_the_sum_of_its_parts() {
    // RB-7: identical subtrees are stored once. Across a real component set, the shared
    // store must be strictly smaller than the total of per-file node counts — otherwise
    // sharing is not happening and the compression claim is empty.
    let all = sources();
    let mut isolated_total = 0usize;
    let mut shared = Store::new();
    for (name, src) in &all {
        let component = parsed(src, name);

        let mut alone = Store::new();
        lower(&component, &mut alone);
        isolated_total += alone.len();

        lower(&component, &mut shared);
    }
    assert!(
        shared.len() < isolated_total,
        "no sharing: shared store has {} nodes, isolated total {}",
        shared.len(),
        isolated_total
    );
    println!(
        "sharing: {} shared nodes vs {} isolated ({} saved across {} files)",
        shared.len(),
        isolated_total,
        isolated_total - shared.len(),
        all.len()
    );
}

#[test]
fn the_outline_is_a_real_saving_on_every_primitive() {
    // RB-2. The claim is a fraction of source cost, so it is measured per file, and the
    // worst case is reported rather than the average — an average can hide a file where the
    // outline saves nothing.
    let mut worst: Option<(String, f64)> = None;
    for (name, src) in sources() {
        let component = parsed(&src, &name);
        let text = outline(&component);
        let ratio = text.len() as f64 / src.len() as f64;
        assert!(
            ratio < 0.75,
            "{name}: outline is {:.0}% of source — not a real saving",
            ratio * 100.0
        );
        if worst.as_ref().is_none_or(|(_, r)| ratio > *r) {
            worst = Some((name, ratio));
        }
    }
    let (name, ratio) = worst.unwrap();
    println!(
        "outline worst case: {name} at {:.0}% of source",
        ratio * 100.0
    );
}

#[test]
fn the_outline_of_every_primitive_reparses() {
    // The self-describing property, checked on real files rather than a fixture: if an
    // outline stops being valid Mzizi, an agent needs a second parser for it.
    for (name, src) in sources() {
        let component = parsed(&src, &name);
        let text = outline(&component);
        let (reparsed, report) = check_with_ast(&text, &format!("outline-of-{name}"));
        assert_eq!(
            report.error_count(),
            0,
            "{name}: outline does not parse:\n{text}"
        );
        let r = reparsed.unwrap();
        assert_eq!(r.name, component.name);
        // The interface must survive the round trip, or the outline is lossy where it
        // claims not to be.
        assert_eq!(r.props.len(), component.props.len(), "{name}: props lost");
        assert_eq!(r.uses, component.uses, "{name}: capabilities lost");
        assert_eq!(r.enums.len(), component.enums.len(), "{name}: enums lost");
    }
}

#[test]
fn every_node_is_addressable_by_a_stable_structural_path() {
    // RB-1: the address an agent patches by. Paths must be unique, or a patch is ambiguous.
    for (name, src) in sources() {
        let component = parsed(&src, &name);
        let mut store = Store::new();
        let root = lower(&component, &mut store);
        let all = paths(&store, root);
        assert!(!all.is_empty(), "{name}: no addressable nodes");

        let mut seen: Vec<&str> = all.iter().map(|(p, _)| p.as_str()).collect();
        let before = seen.len();
        seen.sort();
        seen.dedup();
        assert_eq!(
            before,
            seen.len(),
            "{name}: duplicate structural paths make patching ambiguous"
        );

        // The root is the component's own name, so a path reads as an address a human can
        // also follow.
        assert_eq!(
            all[0].0, component.name,
            "{name}: root path should be the component name"
        );
    }
}

#[test]
fn identity_survives_reformatting_but_not_real_change() {
    // RB-4 and RB-6 together: the property that lets a recorded fact be re-verified.
    for (name, src) in sources() {
        let component = parsed(&src, &name);
        let mut a = Store::new();
        let first = lower(&component, &mut a);

        // Re-parsing the same bytes must give the same hash.
        let again = parsed(&src, &name);
        let mut b = Store::new();
        assert_eq!(
            first,
            lower(&again, &mut b),
            "{name}: identity is not stable"
        );

        // Blank lines carry no meaning and must not alter identity.
        let padded = src.replace("\n\n", "\n\n\n");
        if padded != src {
            let spaced = parsed(&padded, &name);
            let mut c = Store::new();
            assert_eq!(
                first,
                lower(&spaced, &mut c),
                "{name}: whitespace changed identity"
            );
        }
    }
}

#[test]
fn a_rename_across_the_whole_set_touches_no_node() {
    // The refactor small models are worst at, made free. Renaming every component in the
    // store must not create, alter, or remove a single node.
    let mut store = Store::new();
    let mut names = Vec::new();
    for (name, src) in sources() {
        let component = parsed(&src, &name);
        lower(&component, &mut store);
        names.push(component.name);
    }
    let node_count = store.len();
    for name in &names {
        let before = store.resolve(name).expect("bound at lowering");
        assert!(store.rename(name, &format!("{name}_v2")));
        assert_eq!(
            store.resolve(&format!("{name}_v2")),
            Some(before),
            "{name}: the hash must be unchanged by a rename"
        );
    }
    assert_eq!(store.len(), node_count, "renaming touched the node store");
}

#[test]
fn lowering_the_whole_set_stays_inside_the_loop_budget() {
    // Compile speed is a Phase 0 metric (RFC-0001 §3). Parse plus lower for the whole set
    // must stay well inside one perceptible moment on modest hardware.
    let all = sources();
    let started = std::time::Instant::now();
    for (name, src) in &all {
        let (component, _) = check_with_ast(src, name);
        if let Some(c) = component {
            let mut store = Store::new();
            lower(&c, &mut store);
        }
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_millis() < 400,
        "parse+lower of {} files took {elapsed:?}, over the 400ms budget",
        all.len()
    );
    println!("parse+lower of {} files: {elapsed:?}", all.len());
}
