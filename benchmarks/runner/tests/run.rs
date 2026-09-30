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
    start_task(out, "button")
}

fn start_task(out: &Path, task: &str) -> PathBuf {
    let task = repo_root().join("benchmarks/tasks").join(task);
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
    // The default normaliser: the real `mz` was passed an absolute path, and the author
    // sees only the file name (pilot 2 paid ~89 tokens per diagnostic line for it).
    assert!(stdout.contains(r#""file":"candidate.mz""#), "{stdout}");
    assert!(!stdout.contains(&*ep.to_string_lossy()), "{stdout}");
    let recorded = std::fs::read_to_string(ep.join("iter-01/diagnostics.txt")).unwrap();
    assert!(recorded.contains(r#""file":"candidate.mz""#), "{recorded}");
    let meta: Value =
        serde_json::from_str(&std::fs::read_to_string(ep.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta["diagnostics_normaliser"], "file-name");
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

fn score_argv(ep: &Path) -> Vec<String> {
    let meta: Value =
        serde_json::from_str(&std::fs::read_to_string(ep.join("meta.json")).unwrap()).unwrap();
    meta["score"]["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect()
}

/// The changelog task opts in to rename pairing in its `task.toml`, so the scorer runs with
/// `--allow-variant-renames` and the pilot's spec-following candidate pairs `horizontal`…
/// with the reference's `cobalt`…; the button task does not, and its scorer argv lacks it.
#[test]
fn the_changelog_task_opt_in_reaches_the_real_scorer() {
    let out = Scratch::new("renames");
    let ep = start_task(&out.0, "mzizi-changelog-renderer");
    assert_eq!(
        score_argv(&ep).last().map(String::as_str),
        Some("--allow-variant-renames")
    );
    let candidate = repo_root().join(
        "benchmarks/results/2026-09-27-pilot/claude-subagent/mzizi/nyuchi-changelog-renderer/seed-0/iter-01/candidate.mz",
    );
    let o = submit(&ep, &candidate);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let fin = finish(&ep);
    assert_eq!(fin["scored"], true, "{fin}");
    assert_eq!(fin["defects"], 0, "{fin}");
    assert_eq!(fin["facts_checked"], 2, "{fin}");
    assert_eq!(fin["renames"], 4, "{fin}");

    let button = start(&out.0);
    assert!(
        !score_argv(&button)
            .iter()
            .any(|a| a == "--allow-variant-renames"),
        "button does not opt in"
    );
}

/// The card task opts in to slot scoring, so the scorer runs with `--slots` and a port that
/// renders all seven of the reference's `data-slot`s scores 3 facts with no defect.
#[test]
fn the_card_task_scores_its_slot_set() {
    let out = Scratch::new("slots");
    let ep = start_task(&out.0, "card");
    assert!(score_argv(&ep).iter().any(|a| a == "--slots"));
    let candidate = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/card_all_slots.mz");
    let o = submit(&ep, &candidate);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let fin = finish(&ep);
    assert_eq!(fin["scored"], true, "{fin}");
    assert_eq!(fin["facts_checked"], 3, "{fin}");
    assert_eq!(fin["defects"], 0, "{fin}");
    let score = std::fs::read_to_string(ep.join("score.json")).unwrap();
    assert!(score.contains(r#""fact":"slot_set""#), "{score}");

    let button = start(&out.0);
    assert!(!score_argv(&button).iter().any(|a| a == "--slots"));
}

/// A `ui-spec` episode hands the author `spec.md`, not `spec.tsx` (RFC-0009 §2.1), and
/// records the family and the arm's resolved `arm.toml` in `meta.json`. The rest of the loop
/// is the same: the real `mz` checks it and the real harness scores it.
#[test]
fn a_ui_spec_episode_is_given_spec_md_and_records_its_arm() {
    let out = Scratch::new("uispec");
    let task = repo_root().join("benchmarks/tasks/button");
    let o = mzbench(&[
        "episode".as_ref(),
        "start".as_ref(),
        "--task".as_ref(),
        &task,
        "--arm".as_ref(),
        "mzizi".as_ref(),
        "--family".as_ref(),
        "ui-spec".as_ref(),
        "--model-label".as_ref(),
        "it-scripted".as_ref(),
        "--out".as_ref(),
        &out.0,
        "--endpoint".as_ref(),
        "http://127.0.0.1:9".as_ref(),
    ]);
    assert!(o.status.success(), "{}", text(&o));
    let ep = PathBuf::from(String::from_utf8(o.stdout).unwrap().trim());
    let user = std::fs::read_to_string(ep.join("user.txt")).unwrap();
    let spec_md = std::fs::read_to_string(task.join("spec.md")).unwrap();
    assert!(user.starts_with("Implement the component specified below in Mzizi."));
    assert!(user.contains(&spec_md));
    assert!(
        !user.contains("React") && !user.contains("import "),
        "{user}"
    );
    let meta: Value =
        serde_json::from_str(&std::fs::read_to_string(ep.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta["task_family"], "ui-spec");
    assert_eq!(meta["arm"], "mzizi");
    assert_eq!(meta["arm_config"]["extractor"], "mzizi");
    assert_eq!(meta["arm_config"]["extension"], "mz");

    let real = repo_root().join("primitives/button.mz");
    let o = submit(&ep, &real);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let fin = finish(&ep);
    assert_eq!(fin["scored"], true, "{fin}");
    assert_eq!(fin["defects"], 0, "{fin}");
}

/// An arm refuses a family its `arm.toml` does not list, and an unknown arm is refused with
/// the list of arms there are.
#[test]
fn unknown_arms_and_families_are_refused() {
    let out = Scratch::new("refuse");
    let task = repo_root().join("benchmarks/tasks/button");
    let run = |arm: &str, family: &str| {
        mzbench(&[
            "episode".as_ref(),
            "start".as_ref(),
            "--task".as_ref(),
            &task,
            "--arm".as_ref(),
            arm.as_ref(),
            "--family".as_ref(),
            family.as_ref(),
            "--model-label".as_ref(),
            "x".as_ref(),
            "--out".as_ref(),
            &out.0,
        ])
    };
    let o = run("cobol", "ui-port");
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("unknown arm `cobol`"), "{}", text(&o));
    let o = run("mzizi", "backend");
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(
        text(&o).contains("does not run the backend family"),
        "{}",
        text(&o)
    );
}
