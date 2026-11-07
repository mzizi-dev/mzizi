//! The components written in Mzizi (`examples/components/`), the tracker's evidence for what a
//! server component needs from the language (LANGUAGE-TRACKER, "Components written in
//! Mzizi", issue #153). Each is a `program`: it checks with no diagnostic, and `mz run` of it
//! prints what its committed `.expected` file holds. CI's `lowering` job runs the same files
//! through the shipped binary.
//!
//! The `.expected` files are what the programs printed. The tests below pin the lines that carry
//! the claim, each worked out by hand from the algorithm the component ports (the registry's
//! `mzizi-resilience` breaker and rate limiter, RFC 4648 §10's test vectors), so a change to the
//! output has to change a test by hand.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::check;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn component(name: &str) -> PathBuf {
    root().join("examples/components").join(name)
}

fn components() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root().join("examples/components"))
        .expect("examples/components exists")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".mz"))
        .collect();
    names.sort();
    names
}

fn run(name: &str) -> Option<String> {
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return None;
    }
    let cache = std::env::temp_dir().join(format!("mz-components-{}", std::process::id()));
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(["run", component(name).to_str().expect("utf-8 path")])
        .env("MZ_CACHE_DIR", &cache)
        .output()
        .expect("mz runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{name}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn expected(name: &str) -> String {
    std::fs::read_to_string(component(name).with_extension("expected"))
        .unwrap_or_else(|_| panic!("{name} has no .expected file"))
}

#[test]
fn there_are_components_and_each_has_an_expected_file() {
    let names = components();
    assert!(names.len() >= 3, "{names:?}");
    for n in &names {
        assert!(component(n).with_extension("expected").exists(), "{n}");
    }
}

#[test]
fn every_component_checks_clean() {
    for name in components() {
        let src = std::fs::read_to_string(component(&name)).expect("component");
        let report = check(&src, &name);
        assert!(
            report.diagnostics.is_empty(),
            "{name}: {:#?}",
            report.diagnostics
        );
    }
}

#[test]
fn every_component_runs_and_prints_its_expected_output() {
    for name in components() {
        if let Some(out) = run(&name) {
            assert_eq!(out, expected(&name), "{name}");
        }
    }
}

/// RFC 4648 §10's vectors, without padding: `foobar` is `Zm9vYmFy`.
#[test]
fn base64url_matches_the_rfc_vectors_and_the_url_safe_alphabet() {
    let want = expected("base64url.mz")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for (prefix, text) in [
        ("empty: []", ""),
        ("f: [Zg]", "Zg"),
        ("fo: [Zm8]", "Zm8"),
        ("foo: [Zm9v]", "Zm9v"),
        ("foob: [Zm9vYg]", "Zm9vYg"),
        ("fooba: [Zm9vYmE]", "Zm9vYmE"),
        ("foobar: [Zm9vYmFy]", "Zm9vYmFy"),
    ] {
        assert!(
            want.iter()
                .any(|l| l.starts_with(prefix) && l.ends_with("round trip true")),
            "{text}: {want:?}"
        );
    }
    // 0xFB 0xFF 0xBE is `+/++` in standard base64, and `-_--` in base64url.
    assert!(want.iter().any(|l| l.contains("[-_--]")), "{want:?}");
    assert!(want.contains(&"Zg== rejected: = is not in the base64url alphabet".to_owned()));
    assert!(want.contains(&"Zm9vY rejected: a length of 4n + 1 is not base64url".to_owned()));
}

/// The breaker's tests in `mzizi-resilience/src/breaker.rs`, as numbers: three failures
/// open the circuit on the third call, and the cooldown left at the next millisecond is
/// 499 of 500.
#[test]
fn circuit_breaker_opens_on_the_failing_call_and_admits_one_probe() {
    let out = expected("circuit_breaker.mz");
    assert!(out.contains("t=15 closed allowed retry_after=0 failures=2\n"));
    assert!(out.contains("t=21 open turned_away retry_after=499 failures=0\n"));
    assert!(out.contains("t=600 half_open allowed retry_after=0 failures=0\n"));
    assert!(out.contains("t=600 half_open probe_in_flight retry_after=0 failures=0\n"));
    assert!(out.contains("t=602 closed allowed retry_after=0 failures=0\n"));
    assert!(out.contains("t=601 open turned_away retry_after=499 failures=0\n"));
    assert!(out.contains("t=5 open turned_away retry_after=498 failures=0\n"));
    assert!(out.starts_with("rejected: failure_threshold must be at least 1\n"));
}

/// 15 tokens drained, then one window refills exactly `limit` (10), whatever the burst; a
/// denial at 40 ms counts the 0.4 of a token already refilled (100 ms for one whole token).
#[test]
fn rate_gate_sustains_limit_per_window_and_counts_the_part_of_a_token() {
    let out = expected("rate_gate.mz");
    assert!(out.contains("drained: 0 left\n"));
    assert!(out.contains("one window later 10 were allowed\n"));
    assert!(out.contains("at 0: denied, wait 100 ms\n"));
    assert!(out.contains("at 40: denied, wait 60 ms\n"));
    assert!(out.contains("\nimpossible\n"));
    assert!(out.ends_with("12\n"));
}
