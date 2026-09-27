//! `mzizi-benchmark-runner run --component <name> --reference <file.rs> --candidate <file.mz> [--candidate <file.mz> ...] [--results <path>] [--tokens-consumed <n>]`
//!
//! Drives one Phase 0 benchmark run end to end and appends the result to a JSON-lines file.
//! See `src/lib.rs` for what's real here and what's a documented stand-in, and
//! `benchmarks/README.md` for the run-it-yourself instructions.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use mzizi_benchmark_runner::{RunResult, defects_from_diff, now_rfc3339, parse_agent_outcome};

struct Args {
    component: String,
    reference: PathBuf,
    candidates: Vec<PathBuf>,
    results: PathBuf,
    tokens_consumed: Option<u64>,
}

fn usage() -> String {
    "usage: mzizi-benchmark-runner run --component <name> --reference <file.rs> \
--candidate <file.mz> [--candidate <file.mz> ...] [--results <path>] \
[--tokens-consumed <n>]"
        .to_string()
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let subcommand = args.next().ok_or_else(usage)?;
    if subcommand != "run" {
        return Err(format!("{}\nunknown subcommand `{subcommand}`", usage()));
    }

    let mut component = None;
    let mut reference = None;
    let mut candidates = Vec::new();
    let mut results = None;
    let mut tokens_consumed = None;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--component" => {
                component = Some(args.next().ok_or("--component needs a name")?);
            }
            "--reference" => {
                reference = Some(PathBuf::from(
                    args.next().ok_or("--reference needs a path")?,
                ));
            }
            "--candidate" => {
                candidates.push(PathBuf::from(
                    args.next().ok_or("--candidate needs a path")?,
                ));
            }
            "--results" => {
                results = Some(PathBuf::from(args.next().ok_or("--results needs a path")?));
            }
            "--tokens-consumed" => {
                let raw = args.next().ok_or("--tokens-consumed needs a number")?;
                tokens_consumed = Some(
                    raw.parse::<u64>()
                        .map_err(|_| format!("--tokens-consumed: `{raw}` is not a u64"))?,
                );
            }
            other => return Err(format!("{}\nunknown flag `{other}`", usage())),
        }
    }

    if candidates.is_empty() {
        return Err(format!("{}\nat least one --candidate is required", usage()));
    }

    Ok(Args {
        component: component.ok_or_else(|| format!("{}\n--component is required", usage()))?,
        reference: reference.ok_or_else(|| format!("{}\n--reference is required", usage()))?,
        candidates,
        results: results.unwrap_or_else(|| repo_root().join("benchmarks/results/runs.jsonl")),
        tokens_consumed,
    })
}

/// This crate's own manifest directory is `benchmarks/runner`, exactly two levels below the
/// workspace root — the same relationship `benchmarks/harness/src/main.rs` relies on.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn main() -> std::process::ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mzizi-benchmark-runner: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    run(args)
}

fn run(args: Args) -> std::process::ExitCode {
    let started = Instant::now();
    let root = repo_root();

    let reference_src = match std::fs::read_to_string(&args.reference) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "mzizi-benchmark-runner: cannot read reference {}: {e}",
                args.reference.display()
            );
            return std::process::ExitCode::from(2);
        }
    };

    println!("Component: {}", args.component);
    println!(
        "Authoring mode: scripted-stand-in ({} hand-authored candidate(s) standing in for \
successive agent iterations — see benchmarks/README.md)",
        args.candidates.len()
    );
    println!();

    let mut clean: Option<(usize, &PathBuf, String)> = None;
    for (i, candidate) in args.candidates.iter().enumerate() {
        let n = i + 1;
        println!(
            "Candidate {n}/{}: {}",
            args.candidates.len(),
            candidate.display()
        );
        let output = match run_mz(&root, "check", candidate) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("mzizi-benchmark-runner: {e}");
                return std::process::ExitCode::from(2);
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let Some(outcome) = parse_agent_outcome(&stdout) else {
            eprintln!(
                "mzizi-benchmark-runner: `mz check --agent {}` produced no summary line:\n{stdout}",
                candidate.display()
            );
            return std::process::ExitCode::from(2);
        };
        if outcome.errors == 0 {
            println!(
                "  mz check --agent -> 0 errors, {} warning(s) — clean after {n} iteration(s)\n",
                outcome.warnings
            );
            clean = Some((n, candidate, stdout));
            break;
        }
        println!(
            "  mz check --agent -> {} error(s), {} warning(s) — not clean\n{}\n",
            outcome.errors, outcome.warnings, stdout
        );
    }

    let Some((iterations, clean_candidate, _check_stdout)) = clean else {
        let elapsed_ms = started.elapsed().as_millis();
        let result = RunResult {
            component: args.component,
            timestamp: now_rfc3339(),
            iterations_to_clean_compile: None,
            tokens_consumed: args.tokens_consumed,
            contract_clauses: None,
            contract_failures: None,
            defect_count: None,
            defects: vec![],
            elapsed_ms,
            authoring_mode: "scripted-stand-in".to_string(),
            clean_candidate: None,
            notes: Some(format!(
                "none of {} candidate(s) reached a clean `mz check`",
                args.candidates.len()
            )),
        };
        println!("No candidate reached a clean compile.");
        if let Err(e) = append_result(&args.results, &result) {
            eprintln!("mzizi-benchmark-runner: {e}");
            return std::process::ExitCode::from(2);
        }
        println!("Wrote result to {}", args.results.display());
        return std::process::ExitCode::from(1);
    };

    println!(
        "Checking self-consistency: mz contract --agent {}",
        clean_candidate.display()
    );
    let contract_output = match run_mz(&root, "contract", clean_candidate) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("mzizi-benchmark-runner: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let contract_stdout = String::from_utf8_lossy(&contract_output.stdout).into_owned();
    let Some(contract_outcome) = parse_agent_outcome(&contract_stdout) else {
        eprintln!(
            "mzizi-benchmark-runner: `mz contract --agent {}` produced no summary line:\n{contract_stdout}",
            clean_candidate.display()
        );
        return std::process::ExitCode::from(2);
    };
    let contract_failures = contract_outcome.contract_failures.unwrap_or(0);
    let contract_clauses = contract_outcome.contract_clauses.unwrap_or(0);

    if !contract_output.status.success() {
        println!(
            "  FAILED — {} contract clause(s), {contract_failures} failure(s)\n{contract_stdout}",
            contract_clauses
        );
        let elapsed_ms = started.elapsed().as_millis();
        let result = RunResult {
            component: args.component,
            timestamp: now_rfc3339(),
            iterations_to_clean_compile: Some(iterations as u64),
            tokens_consumed: args.tokens_consumed,
            contract_clauses: Some(contract_clauses),
            contract_failures: Some(contract_failures),
            defect_count: None,
            defects: vec![],
            elapsed_ms,
            authoring_mode: "scripted-stand-in".to_string(),
            clean_candidate: Some(display_relative(&root, clean_candidate)),
            notes: Some(
                "mz contract --agent failed self-consistency; reference diff not run".to_string(),
            ),
        };
        if let Err(e) = append_result(&args.results, &result) {
            eprintln!("mzizi-benchmark-runner: {e}");
            return std::process::ExitCode::from(2);
        }
        println!("Wrote result to {}", args.results.display());
        return std::process::ExitCode::from(1);
    }
    println!("  OK — {contract_clauses} contract clause(s), 0 failures\n");

    println!("Diffing against reference: {}", args.reference.display());
    let mz_src = match std::fs::read_to_string(clean_candidate) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "mzizi-benchmark-runner: cannot read {}: {e}",
                clean_candidate.display()
            );
            return std::process::ExitCode::from(2);
        }
    };
    let (defect_count, defects) = defects_from_diff(&mz_src, &reference_src);
    for d in &defects {
        println!("  {d}");
    }
    println!("  {defect_count} defect(s)\n");

    let elapsed_ms = started.elapsed().as_millis();
    let result = RunResult {
        component: args.component,
        timestamp: now_rfc3339(),
        iterations_to_clean_compile: Some(iterations as u64),
        tokens_consumed: args.tokens_consumed,
        contract_clauses: Some(contract_clauses),
        contract_failures: Some(contract_failures),
        defect_count: Some(defect_count),
        defects,
        elapsed_ms,
        authoring_mode: "scripted-stand-in".to_string(),
        clean_candidate: Some(display_relative(&root, clean_candidate)),
        notes: None,
    };
    println!("{}", result.to_json_line());
    if let Err(e) = append_result(&args.results, &result) {
        eprintln!("mzizi-benchmark-runner: {e}");
        return std::process::ExitCode::from(2);
    }
    println!("\nWrote result to {}", args.results.display());

    if defect_count == 0 {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::from(1)
    }
}

/// Shell out to the workspace's own `mz` binary — `cargo run` against `compiler/Cargo.toml`,
/// exactly the pattern `benchmarks/harness/src/main.rs` already uses for the same reason: it
/// works whether or not `compiler` happens to already be built, and it exercises the actual
/// NDJSON CLI surface an agent would drive, not a linked-in function call.
fn run_mz(root: &Path, subcommand: &str, file: &Path) -> Result<Output, String> {
    let compiler_manifest = root.join("compiler/Cargo.toml");
    Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&compiler_manifest)
        .arg("--bin")
        .arg("mz")
        .arg("--")
        .arg(subcommand)
        .arg("--agent")
        .arg(file)
        .output()
        .map_err(|e| {
            format!(
                "failed to run `cargo run --manifest-path {} -- {subcommand} --agent {}`: {e}",
                compiler_manifest.display(),
                file.display()
            )
        })
}

fn display_relative(root: &Path, path: &Path) -> String {
    match path.canonicalize() {
        Ok(abs) => match abs.strip_prefix(root) {
            Ok(rel) => rel.display().to_string(),
            Err(_) => abs.display().to_string(),
        },
        Err(_) => path.display().to_string(),
    }
}

fn append_result(path: &Path, result: &RunResult) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    writeln!(file, "{}", result.to_json_line())
        .map_err(|e| format!("cannot write to {}: {e}", path.display()))
}
