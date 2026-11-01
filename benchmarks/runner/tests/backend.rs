//! End-to-end tests of the `backend` family (RFC-0009 §2.3): the real `mzbench` binary runs a
//! Mode 2 episode on `b1-routing` with the `mzizi-be` arm, and a clean candidate is scored by
//! starting it (`mz build`, then cargo, then the server) and sending the task's 59-fact
//! probes. These need the toolchain and, on a cold cache, crates.io for the pinned `axum` and
//! `tokio` that every lowered service builds against. The test that starts a real service is
//! `#[ignore]`d for CI's `benchmarks` job, which lacks them, and run by its `lowering` job. No
//! model and no tokenizer: `finish` is pointed at a closed port.
//!
//! What they show is that the scorer tells a candidate that holds every fact from one that
//! does not, and counts a candidate that never listens as failing every fact. They measure
//! nothing about any model or about Mzizi.

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

/// A directory scoped to this test and process, removed on drop.
struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Scratch {
        let p = std::env::temp_dir().join(format!("mzbench-be-{label}-{}", std::process::id()));
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

fn b1() -> PathBuf {
    repo_root().join("benchmarks/tasks/b1-routing")
}

/// `episode start` for b1 with the `mzizi-be` arm, plus any extra flags.
fn start(out: &Path, extra: &[&str]) -> PathBuf {
    let task = b1();
    let extra: Vec<&Path> = extra.iter().map(Path::new).collect();
    let mut args: Vec<&Path> = vec![
        "episode".as_ref(),
        "start".as_ref(),
        "--task".as_ref(),
        &task,
        "--arm".as_ref(),
        "mzizi-be".as_ref(),
        "--family".as_ref(),
        "backend".as_ref(),
        "--model-label".as_ref(),
        "it-scripted".as_ref(),
        "--out".as_ref(),
        out,
        "--max-iters".as_ref(),
        "2".as_ref(),
        "--endpoint".as_ref(),
        "http://127.0.0.1:9".as_ref(),
    ];
    args.extend(extra);
    let o = mzbench(&args);
    assert!(o.status.success(), "{}", text(&o));
    PathBuf::from(String::from_utf8(o.stdout).unwrap().trim())
}

/// Submit `candidate`, require it to be clean, finish, and return the final line.
fn run(ep: &Path, candidate: &Path) -> Value {
    let o = mzbench(&["episode".as_ref(), "submit".as_ref(), ep, candidate]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let o = mzbench(&["episode".as_ref(), "finish".as_ref(), ep]);
    assert!(o.status.success(), "{}", text(&o));
    serde_json::from_slice(&o.stdout).expect("finish prints one JSON object")
}

fn reference() -> String {
    std::fs::read_to_string(b1().join("reference-mzizi/registry.mz")).unwrap()
}

#[test]
fn the_default_scorer_for_a_backend_episode_is_the_arms_serve_script() {
    let out = Scratch::new("meta");
    let ep = start(&out.0, &[]);
    let meta: Value =
        serde_json::from_str(&std::fs::read_to_string(ep.join("meta.json")).unwrap()).unwrap();
    assert_eq!(meta["task_family"], "backend");
    let argv = meta["score"]["argv"].as_array().unwrap();
    assert!(
        argv.iter()
            .any(|a| a.as_str().unwrap().ends_with("arms/mzizi-be/serve.sh")),
        "{meta}"
    );
}

// Builds a lowered service with `--offline`, so it needs `axum` and `tokio` already in the cargo
// cache. CI's `benchmarks` job has no such cache; the `lowering` job fetches them and runs this
// with `-- --ignored`. Locally: `cargo test -p mzizi-benchmark-runner --test backend -- --ignored`.
#[test]
#[ignore = "needs axum and tokio in the cargo cache; the lowering job runs it"]
fn a_known_good_candidate_holds_every_fact_and_a_known_bad_one_does_not() {
    let out = Scratch::new("goodbad");

    // Known good: the task's own Mzizi reference, submitted as the candidate.
    let good = out.0.join("good.mz");
    std::fs::write(&good, reference()).unwrap();
    let ep = start(&out.0.join("results-good"), &[]);
    let fin = run(&ep, &good);
    assert_eq!(fin["clean"], true, "{fin}");
    assert_eq!(fin["scored"], true, "{fin}");
    assert_eq!(fin["score_error"], Value::Null, "{fin}");
    assert_eq!(fin["facts_checked"], 59, "{fin}");
    assert_eq!(fin["defects"], 0, "{fin}");
    assert_eq!(fin["start_failed"], Value::Null, "{fin}");
    let facts = std::fs::read_to_string(ep.join("score.json")).unwrap();
    assert_eq!(facts.lines().count(), 60, "59 facts and a summary line");

    // Known bad: it checks clean and serves, but the not-found text is wrong, which breaks
    // every fact that reads `/error`. A defect is a failing fact on a clean candidate.
    let bad = out.0.join("bad.mz");
    std::fs::write(&bad, reference().replace("\"Not found\"", "\"Missing\"")).unwrap();
    let ep = start(&out.0.join("results-bad"), &[]);
    let fin = run(&ep, &bad);
    assert_eq!(fin["clean"], true, "{fin}");
    assert_eq!(fin["scored"], true, "{fin}");
    assert_eq!(fin["facts_checked"], 59, "{fin}");
    let defects = fin["defects"].as_u64().unwrap();
    // The five probes whose expectation reads `/error` (see probes.toml).
    assert_eq!(defects, 5, "{fin}");
    assert_eq!(fin["start_failed"], Value::Null, "{fin}");
    let facts = std::fs::read_to_string(ep.join("score.json")).unwrap();
    let failed: Vec<Value> = facts
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["ok"] == false)
        .collect();
    assert_eq!(failed.len() as u64, defects, "{facts}");
    assert!(failed.iter().all(|v| v["fact"] == "json /error"), "{facts}");
}

#[test]
fn a_clean_candidate_that_never_listens_fails_every_fact() {
    let out = Scratch::new("nostart");
    let good = out.0.join("good.mz");
    std::fs::write(&good, reference()).unwrap();
    // A serve command that exits at once stands in for a candidate that passed its check
    // and then did not start (RFC-0009 §2.3's `start_failed`).
    let ep = start(
        &out.0.join("results"),
        &["--score-cmd", r#"["sh","-c","exit 3"]"#],
    );
    let fin = run(&ep, &good);
    assert_eq!(fin["clean"], true, "{fin}");
    assert_eq!(fin["scored"], true, "{fin}");
    assert_eq!(fin["facts_checked"], 59, "{fin}");
    assert_eq!(fin["defects"], 59, "{fin}");
    assert!(fin["start_failed"].as_str().is_some(), "{fin}");
}
