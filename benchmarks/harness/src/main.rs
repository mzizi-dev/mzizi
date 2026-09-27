//! `mzizi-benchmark-harness diff --mzizi <file.mz> --reference <file.rs>`
//! `mzizi-benchmark-harness score --arm <mzizi|dioxus> --candidate <file> --reference <file.rs>
//!  [--allow-variant-renames]`
//!
//! **`diff`** — prerequisite: `mz contract --agent <file.mz>` must exit 0. `mz contract` is
//! a self-consistency check (RFC-0006), so if a component fails its own declared contract
//! there is no point comparing it against a reference — that failure is reported instead,
//! and the reference diff never runs. Given a self-consistent component, it diffs the `.mz`
//! file's declared per-variant pixel heights against the heights implied by the reference's
//! `classes()` Tailwind strings, as a human-readable report, and exits 1 on any defect.
//!
//! **`score`** — every fact the reference supports (variant sets, defaults, heights; see
//! `mzizi_benchmark_harness::score`), for either arm, as exactly one JSON object on stdout.
//! It does not run `mz contract` or any compiler: whether the candidate compiles is a
//! separate measurement the benchmark takes, and a defect is only meaningful for a candidate
//! that does. Exit 0 whenever scoring ran, defects or not; exit 2 on a usage or IO error.
//! `--allow-variant-renames` turns on class-token pairing of renamed variants
//! (`mzizi_benchmark_harness::rename_map`); `mzbench` passes it only for a task whose
//! `task.toml` sets `allow_variant_renames = true`. Without it, variants match by name.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use mzizi_benchmark_harness::{
    Arm, diff_size_enum, parse_mzizi_size_enums, parse_rust_enums, reference_class_arms, score,
};

enum Cmd {
    Diff {
        mzizi: PathBuf,
        reference: PathBuf,
    },
    Score {
        arm: Arm,
        candidate: PathBuf,
        reference: PathBuf,
        allow_variant_renames: bool,
    },
}

fn parse_args() -> Result<Cmd, String> {
    let mut args = std::env::args().skip(1);
    let subcommand = args.next().ok_or_else(usage)?;
    if subcommand != "diff" && subcommand != "score" {
        return Err(format!("{}\nunknown subcommand `{subcommand}`", usage()));
    }

    let mut flags: Vec<(String, String)> = Vec::new();
    let mut allow_variant_renames = false;
    while let Some(flag) = args.next() {
        if subcommand == "score" && flag == "--allow-variant-renames" {
            allow_variant_renames = true;
            continue;
        }
        let allowed: &[&str] = if subcommand == "diff" {
            &["--mzizi", "--reference"]
        } else {
            &["--arm", "--candidate", "--reference"]
        };
        if !allowed.contains(&flag.as_str()) {
            return Err(format!("{}\nunknown flag `{flag}`", usage()));
        }
        let value = args.next().ok_or(format!("{flag} needs a value"))?;
        flags.push((flag, value));
    }
    let get = |name: &str| {
        flags
            .iter()
            .rev()
            .find(|(f, _)| f == name)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("{}\n{name} is required", usage()))
    };

    if subcommand == "diff" {
        Ok(Cmd::Diff {
            mzizi: PathBuf::from(get("--mzizi")?),
            reference: PathBuf::from(get("--reference")?),
        })
    } else {
        let arm_s = get("--arm")?;
        let arm = Arm::parse(&arm_s).ok_or_else(|| {
            format!(
                "{}\n--arm must be `mzizi` or `dioxus`, not `{arm_s}`",
                usage()
            )
        })?;
        Ok(Cmd::Score {
            arm,
            candidate: PathBuf::from(get("--candidate")?),
            reference: PathBuf::from(get("--reference")?),
            allow_variant_renames,
        })
    }
}

fn usage() -> String {
    "usage: mzizi-benchmark-harness diff --mzizi <file.mz> --reference <file.rs>\n       \
     mzizi-benchmark-harness score --arm <mzizi|dioxus> --candidate <file> --reference <file.rs> \
     [--allow-variant-renames]"
        .to_string()
}

fn read(path: &Path) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!(
            "mzizi-benchmark-harness: cannot read {}: {e}",
            path.display()
        );
        ExitCode::from(2)
    })
}

fn main() -> ExitCode {
    let cmd = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mzizi-benchmark-harness: {e}");
            return ExitCode::from(2);
        }
    };
    let result = match cmd {
        Cmd::Diff { mzizi, reference } => run_diff(&mzizi, &reference),
        Cmd::Score {
            arm,
            candidate,
            reference,
            allow_variant_renames,
        } => run_score(arm, &candidate, &reference, allow_variant_renames),
    };
    result.unwrap_or_else(|code| code)
}

fn run_score(
    arm: Arm,
    candidate: &Path,
    reference: &Path,
    allow_variant_renames: bool,
) -> Result<ExitCode, ExitCode> {
    let candidate_src = read(candidate)?;
    let reference_src = read(reference)?;
    let report = score(
        arm,
        &parse_rust_enums(&reference_src),
        &arm.extract(&candidate_src),
        allow_variant_renames,
    );
    println!("{}", report.to_json());
    Ok(ExitCode::SUCCESS)
}

fn run_diff(mzizi: &Path, reference: &Path) -> Result<ExitCode, ExitCode> {
    println!(
        "Checking self-consistency: mz contract --agent {}",
        mzizi.display()
    );
    if let Err(e) = run_mz_contract(mzizi) {
        println!("  FAILED — {e}");
        println!(
            "mzizi-benchmark-harness: not diffing against the reference; fix the self-consistency failure first"
        );
        return Ok(ExitCode::from(1));
    }
    println!("  OK\n");

    let mz_src = read(mzizi)?;
    let rs_src = read(reference)?;

    let size_enums = parse_mzizi_size_enums(&mz_src);
    if size_enums.is_empty() {
        eprintln!(
            "mzizi-benchmark-harness: {} declares no height-bearing enum",
            mzizi.display()
        );
        return Err(ExitCode::from(2));
    }
    let reference_enums = parse_rust_enums(&rs_src);

    println!("Diffing against reference: {}", reference.display());
    let mut total_defects = 0usize;
    for size_enum in &size_enums {
        let arms = reference_class_arms(&reference_enums, &size_enum.name);
        let report = diff_size_enum(size_enum, &arms);
        println!("  enum {}:", report.enum_name);
        for result in &report.results {
            println!("    {result}");
        }
        total_defects += report.defect_count();
    }
    println!();
    println!("{total_defects} defect(s)");

    Ok(if total_defects > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
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
