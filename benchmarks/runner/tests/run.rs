//! End-to-end tests: run the real `mzbench` binary through a Mode 2 episode (`episode start`
//! / `submit` / `finish`, then `summarize`) with the *default* check and score commands —
//! the real `mz check --agent` and the real `mzizi-benchmark-harness score`, both via
//! `cargo run`. The unit tests in `src/episode.rs` drive the same state machine with `sh`
//! stand-ins for both; these prove the stand-ins' exit-code and JSON shapes match what the
//! real tools emit.
//!
//! No network and no model: `finish` is pointed at a closed localhost port, so the
//! tokenizer is unavailable and `transcript_tokens` is recorded as null with a reason —
//! the path an agent-mode run with no llama-server takes.

use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn mzbench(args: &[&Path]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_mzbench"))
        .args(args)
        .output()
        .expect("failed to run mzbench")
}

fn text(o: &Output) -> String {
    format!(
        "exit {:?}\n--- stdout\n{}--- stderr\n{}",
        o.status.code(),
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// A results directory scoped to this test and process, removed on drop.
struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Scratch {
        let p = std::env::temp_dir().join(format!("mzbench-it-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `episode start` on the button task, returning the episode dir it prints.
fn start(out: &Path) -> PathBuf {
    let task = repo_root().join("benchmarks/tasks/button");
    let o = mzbench(&[
        "episode".as_ref(),
        "start".as_ref(),
        "--task".as_ref(),
        &task,
        "--arm".as_ref(),
        "mzizi".as_ref(),
        "--model-label".as_ref(),
        "it-scripted".as_ref(),
        "--out".as_ref(),
        out,
        "--max-iters".as_ref(),
        "3".as_ref(),
        // A closed port: `finish` must record the tokenizer as unavailable, not hang.
        "--endpoint".as_ref(),
        "http://127.0.0.1:9".as_ref(),
    ]);
    assert!(o.status.success(), "{}", text(&o));
    PathBuf::from(String::from_utf8(o.stdout).unwrap().trim())
}

fn submit(ep: &Path, candidate: &Path) -> Output {
    mzbench(&["episode".as_ref(), "submit".as_ref(), ep, candidate])
}

fn finish(ep: &Path) -> Value {
    let o = mzbench(&["episode".as_ref(), "finish".as_ref(), ep]);
    assert!(o.status.success(), "{}", text(&o));
    serde_json::from_slice(&o.stdout).expect("finish prints one JSON object")
}

fn syntax_error() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/button_syntax_error.mz")
}

/// Submit the unclosed-block candidate and check the real `mz` rejected it with exit 1 (an
/// ERRORS iteration, not a setup error) and that its diagnostic was fed back verbatim.
fn submit_syntax_error_first(ep: &Path) {
    let o = submit(ep, &syntax_error());
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert!(stdout.contains("MZ0204"), "{stdout}");
    assert!(stdout.contains("ITERATION 1/3: ERRORS"), "{stdout}");
}

#[test]
fn syntax_error_then_clean_but_wrong_candidate_scores_one_real_defect() {
    let out = Scratch::new("defect");
    let ep = start(&out.0);
    submit_syntax_error_first(&ep);

    // The RFC-0006 §10.1 kill-criterion shape: compiles clean, passes its own weakened
    // contract, but declares `sm` at 44px against the reference's `h-12`.
    let broken = repo_root().join("benchmarks/harness/tests/fixtures/button_broken.mz");
    let o = submit(&ep, &broken);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("ITERATION 2/3: CLEAN"),
        "{}",
        text(&o)
    );

    let fin = finish(&ep);
    assert_eq!(fin["iterations"], 2, "{fin}");
    assert_eq!(fin["iterations_to_clean"], 2, "{fin}");
    assert_eq!(fin["clean"], true, "{fin}");
    assert_eq!(fin["scored"], true, "{fin}");
    assert_eq!(fin["score_error"], Value::Null, "{fin}");
    assert_eq!(fin["defects"], 1, "{fin}");
    assert!(fin["facts_checked"].as_u64().unwrap() > 1, "{fin}");
    assert_eq!(fin["transcript_tokens"], Value::Null, "{fin}");
    assert!(
        fin["transcript_tokens_null_reason"]
            .as_str()
            .unwrap()
            .starts_with("tokenizer unavailable"),
        "{fin}"
    );

    // `summarize` reads that final line back: one clean episode, one with a defect.
    let o = mzbench(&["summarize".as_ref(), &out.0]);
    assert!(o.status.success(), "{}", text(&o));
    let md = String::from_utf8_lossy(&o.stdout);
    let cells =
        |line: &str| -> Vec<String> { line.split('|').map(|c| c.trim().to_string()).collect() };
    let header = cells(
        md.lines()
            .find(|l| l.starts_with("| model | arm | n "))
            .expect("by-arm header"),
    );
    let row = cells(
        md.lines()
            .find(|l| l.starts_with("| it-scripted | mzizi | 1 "))
            .expect("by-arm row for this episode"),
    );
    let col = |name: &str| &row[header.iter().position(|h| h == name).expect(name)];
    assert_eq!(col("clean compile"), "1/1 (100.0%)", "{md}");
    assert_eq!(col("iters to clean mean"), "2.00 (n=1)", "{md}");
    assert_eq!(col("defect rate"), "1/1 (100.0%)", "{md}");
    assert_eq!(col("mean defects"), "1.00 (n=1)", "{md}");
}

#[test]
fn syntax_error_then_the_real_button_scores_zero_defects() {
    let out = Scratch::new("clean");
    let ep = start(&out.0);
    submit_syntax_error_first(&ep);

    let o = submit(&ep, &repo_root().join("primitives/button.mz"));
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));

    let fin = finish(&ep);
    assert_eq!(fin["iterations_to_clean"], 2, "{fin}");
    assert_eq!(fin["scored"], true, "{fin}");
    assert_eq!(fin["defects"], 0, "{fin}");
}
