//! `mz` — the check loop's entry point.
//!
//! ```text
//! mz check <file.mz>            human-readable diagnostics
//! mz check --agent <file.mz>    NDJSON, one diagnostic per line + a summary line
//! mz contract <file.mz>         evaluate the `contract` block (RFC-0006)
//! mz outline <file.mz>          the interface only, as valid Mzizi (RFC-0003 §4)
//! mz hash <file.mz>             the root hash and the stored node count
//! mz ir <file.mz>               every node with its hash and structural path
//! ```
//!
//! Exit status is 0 when there are no errors (warnings do not fail), 1 when there are, and
//! 2 for a usage or I/O problem — so the loop can branch on status without parsing output.
//! `mz contract` keeps that contract: a failed assertion exits 1, exactly as a compile
//! error does, which is what lets a benchmark harness branch on status alone.
//!
//! `check` and `contract` are separate subcommands rather than one pass, because CHARTER.md
//! §6 measures two different things and says so: "a syntax/compile error is not itself a
//! defect for this metric… the defect rate measures what gets _past_ the compiler wrong".
//! One exit code covering both would make the Phase 0 defect metric unreadable. RFC-0001
//! §1.6's "`mz check` runs them as part of the loop" is narrowed by RFC-0006 §7 for that
//! reason.

use std::process::ExitCode;
use std::time::Instant;

use mzizi_lang_compiler::diagnostic::Severity;
use mzizi_lang_compiler::ir::{Store, lower, paths};
use mzizi_lang_compiler::outline::outline;
use mzizi_lang_compiler::{check, check_contract, check_with_ast};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let agent = args.iter().any(|a| a == "--agent");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    let (command, path) = match positional.as_slice() {
        [cmd, path]
            if matches!(
                cmd.as_str(),
                "check" | "contract" | "outline" | "hash" | "ir"
            ) =>
        {
            (cmd.as_str(), *path)
        }
        [path] => ("check", *path),
        _ => {
            eprintln!("usage: mz <check|contract|outline|hash|ir> [--agent] <file.mz>");
            return ExitCode::from(2);
        }
    };

    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mz: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };

    if command == "contract" {
        let started = Instant::now();
        let (report, tally) = check_contract(&src, path);
        let elapsed = started.elapsed().as_millis();

        if agent {
            print!(
                "{}",
                report.to_ndjson_with(elapsed, Some((tally.clauses, tally.failed)))
            );
        } else {
            print_human(&report);
            println!(
                "mz: {} errors ({} exact-fixable), {} contract clauses, {} failed, {}ms",
                report.error_count(),
                report.exact_fixable(),
                tally.clauses,
                tally.failed,
                elapsed
            );
        }

        return if report.error_count() > 0 {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        };
    }

    // The IR-backed commands all need the tree, so they share one parse.
    if command != "check" {
        let (component, report) = check_with_ast(&src, path);
        let Some(component) = component else {
            eprintln!("mz: {path} does not parse; run `mz check` for diagnostics");
            return ExitCode::from(1);
        };
        match command {
            "outline" => print!("{}", outline(&component)),
            "hash" => {
                let mut store = Store::new();
                let root = lower(&component, &mut store);
                println!(
                    "{}  {}  {} nodes",
                    root.short(),
                    component.name,
                    store.len()
                );
            }
            "ir" => {
                let mut store = Store::new();
                let root = lower(&component, &mut store);
                for (path, hash) in paths(&store, root) {
                    println!("{}  {}", hash.short(), path);
                }
            }
            _ => unreachable!("dispatch guarded above"),
        }
        return if report.error_count() > 0 {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        };
    }

    let started = Instant::now();
    let report = check(&src, path);
    let elapsed = started.elapsed().as_millis();

    if agent {
        print!("{}", report.to_ndjson(elapsed));
    } else {
        print_human(&report);
        println!(
            "mz: {} errors ({} exact-fixable), {}ms",
            report.error_count(),
            report.exact_fixable(),
            elapsed
        );
    }

    if report.error_count() > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// One line per diagnostic, in the same shape for every subcommand.
fn print_human(report: &mzizi_lang_compiler::diagnostic::CheckReport) {
    for d in &report.diagnostics {
        let level = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        println!(
            "{}:{}:{}: {} [{}] {}",
            d.file, d.span.start_line, d.span.start_col, level, d.code, d.say
        );
        if let Some(fix) = &d.fix {
            println!("    fix ({:?}): {:?}", fix.confidence, fix.replace);
        }
    }
}
