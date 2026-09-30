//! `mzprobe` — send a backend task's probes to a server and check the facts (RFC-0009 §2.3).
//!
//! ```text
//! mzprobe run    <task-dir> --base http://127.0.0.1:8080   probe a server that is running
//! mzprobe serve  <task-dir> -- <command…>                  start it with PORT set, probe, stop
//! mzprobe verify <task-dir>                                every reference, via its start.sh
//! ```
//!
//! Output is one NDJSON line per fact, then a summary line. Exit status: 0 when every fact
//! held, 1 when one did not (or the server never started), 2 for a usage or setup error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use mzizi_benchmark_probe::{Outcome, fact_json, load_task, probe_all, serve_and_probe, verify};

/// How long a server may take to listen. A reference builds with cargo on first start.
const START_TIMEOUT: Duration = Duration::from_secs(900);

fn usage() -> ExitCode {
    eprintln!(
        "usage: mzprobe run <task-dir> --base <url>\n       mzprobe serve <task-dir> -- <command…>\n       mzprobe verify <task-dir>"
    );
    ExitCode::from(2)
}

fn summary(outcome: &Outcome, label: Option<&str>) {
    for f in &outcome.facts {
        println!("{}", fact_json(f));
    }
    println!(
        "{}",
        serde_json::json!({
            "summary": true,
            "reference": label,
            "facts": outcome.facts.len(),
            "failed": outcome.failed(),
            "start_failed": outcome.start_failed,
        })
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        return usage();
    };
    let Some(dir) = rest.first() else {
        return usage();
    };
    let task = match load_task(Path::new(dir)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("mzprobe: {e}");
            return ExitCode::from(2);
        }
    };
    let outcome_code = |failed: bool| {
        if failed {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        }
    };
    match cmd.as_str() {
        "run" => {
            let base = match rest.iter().position(|a| a == "--base") {
                Some(i) if i + 1 < rest.len() => rest[i + 1].clone(),
                _ => return usage(),
            };
            let outcome = Outcome {
                facts: probe_all(&task, &base),
                start_failed: None,
            };
            summary(&outcome, None);
            outcome_code(outcome.failed() > 0)
        }
        "serve" => {
            let Some(i) = rest.iter().position(|a| a == "--") else {
                return usage();
            };
            let argv = &rest[i + 1..];
            if argv.is_empty() {
                return usage();
            }
            let log =
                std::env::temp_dir().join(format!("mzprobe-serve-{}.log", std::process::id()));
            let here = PathBuf::from(".");
            match serve_and_probe(&task, argv, &here, &log, START_TIMEOUT) {
                Ok(outcome) => {
                    summary(&outcome, None);
                    if let Some(why) = &outcome.start_failed {
                        eprintln!(
                            "mzprobe: start failed: {why}; its output is in {}",
                            log.display()
                        );
                    }
                    outcome_code(outcome.failed() > 0)
                }
                Err(e) => {
                    eprintln!("mzprobe: {e}");
                    ExitCode::from(2)
                }
            }
        }
        "verify" => match verify(&task, START_TIMEOUT) {
            Ok(outcomes) => {
                let mut failed = false;
                for (r, outcome) in &outcomes {
                    summary(outcome, Some(r));
                    if let Some(why) = &outcome.start_failed {
                        eprintln!("mzprobe: {r} did not start: {why}");
                    }
                    failed |= outcome.failed() > 0;
                    eprintln!(
                        "mzprobe: {r}: {} of {} facts held",
                        outcome.facts.len() - outcome.failed(),
                        outcome.facts.len()
                    );
                }
                outcome_code(failed)
            }
            Err(e) => {
                eprintln!("mzprobe: {e}");
                ExitCode::from(2)
            }
        },
        _ => usage(),
    }
}
