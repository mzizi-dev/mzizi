//! `mzizi-benchmark-harness diff --mzizi <file.mz> --reference <file.rs>`
//!
//! Prerequisite: `mz contract --agent <file.mz>` must exit 0. `mz contract` is a
//! self-consistency check (RFC-0006), so if a component fails its own declared contract
//! there is no point comparing it against a reference — that failure is reported instead,
//! and the reference diff below never runs.
//!
//! Given a self-consistent component, this diffs the `.mz` file's declared per-variant
//! pixel heights against the heights implied by the reference `.rs` file's Tailwind
//! classes, and reports every mismatch as a defect (RFC-0006 §10.1).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use mzizi_benchmark_harness::{diff_size_enum, parse_mzizi_size_enums, parse_rust_match_arms};

struct Args {
    mzizi: PathBuf,
    reference: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let subcommand = args.next().ok_or_else(usage)?;
    if subcommand != "diff" {
        return Err(format!("{}\nunknown subcommand `{subcommand}`", usage()));
    }

    let mut mzizi = None;
    let mut reference = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--mzizi" => mzizi = Some(PathBuf::from(args.next().ok_or("--mzizi needs a path")?)),
            "--reference" => {
                reference = Some(PathBuf::from(
                    args.next().ok_or("--reference needs a path")?,
                ))
            }
            other => return Err(format!("{}\nunknown flag `{other}`", usage())),
        }
    }

    Ok(Args {
        mzizi: mzizi.ok_or_else(|| format!("{}\n--mzizi is required", usage()))?,
        reference: reference.ok_or_else(|| format!("{}\n--reference is required", usage()))?,
    })
}

fn usage() -> String {
    "usage: mzizi-benchmark-harness diff --mzizi <file.mz> --reference <file.rs>".to_string()
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mzizi-benchmark-harness: {e}");
            return ExitCode::from(2);
        }
    };

    println!(
        "Checking self-consistency: mz contract --agent {}",
        args.mzizi.display()
    );
    if let Err(e) = run_mz_contract(&args.mzizi) {
        println!("  FAILED — {e}");
        println!(
            "mzizi-benchmark-harness: not diffing against the reference; fix the self-consistency failure first"
        );
        return ExitCode::from(1);
    }
    println!("  OK\n");

    let mz_src = match std::fs::read_to_string(&args.mzizi) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "mzizi-benchmark-harness: cannot read {}: {e}",
                args.mzizi.display()
            );
            return ExitCode::from(2);
        }
    };
    let rs_src = match std::fs::read_to_string(&args.reference) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "mzizi-benchmark-harness: cannot read {}: {e}",
                args.reference.display()
            );
            return ExitCode::from(2);
        }
    };

    let size_enums = parse_mzizi_size_enums(&mz_src);
    if size_enums.is_empty() {
        eprintln!(
            "mzizi-benchmark-harness: {} declares no height-bearing enum",
            args.mzizi.display()
        );
        return ExitCode::from(2);
    }
    let rust_variants = parse_rust_match_arms(&rs_src);

    println!("Diffing against reference: {}", args.reference.display());
    let mut total_defects = 0usize;
    for size_enum in &size_enums {
        let report = diff_size_enum(size_enum, &rust_variants);
        println!("  enum {}:", report.enum_name);
        for result in &report.results {
            println!("    {result}");
        }
        total_defects += report.defect_count();
    }
    println!();
    println!("{total_defects} defect(s)");

    if total_defects > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Shell out to the workspace's own `mz` binary — built via `cargo run` against
/// `compiler/Cargo.toml` rather than a guessed target-dir path, so this works whether or
/// not `compiler` happens to already be built.
fn run_mz_contract(mzizi_path: &Path) -> Result<(), String> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let compiler_manifest = Path::new(manifest_dir).join("../../compiler/Cargo.toml");

    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&compiler_manifest)
        .arg("--bin")
        .arg("mz")
        .arg("--")
        .arg("contract")
        .arg("--agent")
        .arg(mzizi_path)
        .output()
        .map_err(|e| {
            format!(
                "failed to run `cargo run --manifest-path {}`: {e}",
                compiler_manifest.display()
            )
        })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "mz contract exited with {:?}\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
