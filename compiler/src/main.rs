//! `mz` — the check loop's entry point.
//!
//! ```text
//! mz check <file.mz>            human-readable diagnostics
//! mz check --agent <file.mz>    NDJSON, one diagnostic per line + a summary line
//! mz fix <file.mz>              apply every `exact` fix in place, then check again
//! mz contract <file.mz>         evaluate the `contract` block (RFC-0006)
//! mz outline <file.mz>          the interface only, as valid Mzizi (RFC-0003 §4)
//! mz hash <file.mz>             the root hash and the stored node count
//! mz ir <file.mz>               every node with its hash and structural path
//! mz build <file.mz> --out <dir> lower a service to an axum package (RFC-0011 §8), or a
//!                               program to a dependency-free Rust package (RFC-0013 §14)
//! mz run [--release] <file.mz>  check, lower, build and run a program (RFC-0013 §13)
//! ```
//!
//! Exit status is 0 when there are no errors (warnings do not fail), 1 when there are, and
//! 2 for a usage or I/O problem — so the loop can branch on status without parsing output.
//! `mz run` is the exception, because 1 belongs to the program it runs (RFC-0013 §13.1): it
//! exits with the program's own status (0, or 101 for a trap, or 141 for a closed standard
//! output), 2 for a usage problem, and 3 when the program did not compile.
//! `mz contract` keeps that contract: a failed assertion exits 1, exactly as a compile
//! error does, which is what lets a benchmark harness branch on status alone.
//!
//! `check` and `contract` are separate subcommands rather than one pass, because CHARTER.md
//! §6 measures two different things and says so: "a syntax/compile error is not itself a
//! defect for this metric… the defect rate measures what gets _past_ the compiler wrong".
//! One exit code covering both would make the Phase 0 defect metric unreadable. RFC-0001
//! §1.6's "`mz check` runs them as part of the loop" is narrowed by RFC-0006 §7 for that
//! reason.

#![forbid(unsafe_code)]

use std::io::Write;
use std::process::ExitCode;
use std::time::Instant;

use mzizi_lang_compiler::diagnostic::Severity;
use mzizi_lang_compiler::ir::{Store, lower, paths};
use mzizi_lang_compiler::outline::outline;
use mzizi_lang_compiler::parse::Program;
use mzizi_lang_compiler::{
    apply_exact_fixes, check, check_contract, check_program, check_with_ast,
};

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let agent = args.iter().any(|a| a == "--agent");
    let release = args.iter().any(|a| a == "--release");
    // `--out <dir>` is `mz build`'s only option with a value.
    let out = match args.iter().position(|a| a == "--out") {
        Some(i) if i + 1 < args.len() => {
            let dir = args.remove(i + 1);
            args.remove(i);
            Some(dir)
        }
        Some(_) => {
            eprintln!("mz: `--out` needs a directory");
            return ExitCode::from(2);
        }
        None => None,
    };
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    let (command, path) = match positional.as_slice() {
        [cmd, path]
            if matches!(
                cmd.as_str(),
                "check" | "fix" | "contract" | "outline" | "hash" | "ir" | "build" | "run"
            ) =>
        {
            (cmd.as_str(), *path)
        }
        [path] => ("check", *path),
        _ => {
            eprintln!(
                "usage: mz <check|fix|contract|outline|hash|ir> [--agent] <file.mz>\n       mz build <file.mz> --out <dir>\n       mz run [--release] <program.mz>"
            );
            return ExitCode::from(2);
        }
    };
    if command == "build" && out.is_none() {
        eprintln!("usage: mz build <file.mz> --out <dir>");
        return ExitCode::from(2);
    }

    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mz: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };

    // `mz fix`: one pass of every `exact` fix (RFC-0001 §4.3), written back in place, then
    // the ordinary check of the result — so the output and the exit status describe the
    // file as it now is. `guess` fixes are never applied. A fix that was demoted to
    // `guess` because it overlapped another may be `exact` on the next check; `mz fix`
    // does one pass, as promised, and the re-check says what is left.
    if command == "fix" {
        let before = check(&src, path);
        let applied = before.exact_fixable();
        if applied > 0 {
            let fixed = apply_exact_fixes(&src, &before);
            if let Err(e) = std::fs::write(path, fixed) {
                eprintln!("mz: cannot write {path}: {e}");
                return ExitCode::from(2);
            }
        }
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("mz: cannot read {path}: {e}");
                return ExitCode::from(2);
            }
        };
        let started = Instant::now();
        let report = check(&src, path);
        let elapsed = started.elapsed().as_millis();
        if agent {
            print!("{}", report.to_ndjson(elapsed));
        } else {
            print_human(&report);
            println!(
                "mz: applied {applied} exact fixes; {} errors ({} exact-fixable) remain, {elapsed}ms",
                report.error_count(),
                report.exact_fixable(),
            );
        }
        return if report.error_count() > 0 {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        };
    }

    if command == "run" {
        return run(&src, path, agent, release);
    }

    // `mz build`: check, then lower a service to an axum package (RFC-0011 §8). Only a
    // file with no errors is lowered, so the generated code never guesses at a tree.
    if command == "build" {
        let (program, report) = check_program(&src, path);
        if report.error_count() > 0 {
            print_human(&report);
            println!(
                "mz: {} errors ({} exact-fixable); nothing built",
                report.error_count(),
                report.exact_fixable()
            );
            return ExitCode::from(1);
        }
        let dir = std::path::PathBuf::from(out.unwrap_or_default());
        if let Some(Program::Program(p)) = &program {
            let package = mzizi_lang_compiler::run::lower(p, &file_name(path));
            if let Err(e) = mzizi_lang_compiler::run::write(&package, &dir) {
                eprintln!("mz: {e}");
                return ExitCode::from(2);
            }
            println!(
                "mz: built `program {}` into {} ({} functions); run it with `cargo run --manifest-path {}`",
                p.name,
                dir.display(),
                p.fns.len(),
                dir.join("Cargo.toml").display()
            );
            return ExitCode::SUCCESS;
        }
        let Some(Program::Service(service)) = program else {
            eprintln!(
                "mz: `build` lowers a service or a program; components do not lower yet (RFC-0007 G2.1)"
            );
            return ExitCode::from(2);
        };
        let package = mzizi_lang_compiler::lower::lower(&service, &file_name(path));
        let from = std::path::Path::new(path)
            .parent()
            .unwrap_or(std::path::Path::new(""));
        let mut written = Vec::new();
        for (rel, text) in &package.files {
            written.push((dir.join(rel), text.clone().into_bytes()));
        }
        for rel in &package.fixtures {
            match std::fs::read(from.join(rel)) {
                Ok(bytes) => written.push((dir.join(rel), bytes)),
                Err(e) => {
                    eprintln!("mz: cannot read fixture {rel}: {e}");
                    return ExitCode::from(2);
                }
            }
        }
        for (file, bytes) in written {
            let made = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&file, bytes));
            if let Err(e) = made {
                eprintln!("mz: cannot write {}: {e}", file.display());
                return ExitCode::from(2);
            }
        }
        println!(
            "mz: built `service {}` into {} ({} routes, {} fixtures); run it with `cargo run --release --manifest-path {}`",
            service.name,
            dir.display(),
            service.routes.len(),
            package.fixtures.len(),
            dir.join("Cargo.toml").display()
        );
        return ExitCode::SUCCESS;
    }

    if command == "contract" {
        let started = Instant::now();
        let (report, tally) = check_contract(&src, path);
        let elapsed = started.elapsed().as_millis();

        if agent {
            print!(
                "{}",
                report.to_ndjson_with(elapsed, Some((tally.clauses, tally.failed)), tally.tested)
            );
        } else {
            print_human(&report);
            // RFC-0010 C-4: generated cases are reported as tested, with their count.
            let tested = tally
                .tested
                .map(|n| format!(", tested over {n} generated requests"))
                .unwrap_or_default();
            println!(
                "mz: {} errors ({} exact-fixable), {} contract clauses, {} failed{tested}, {}ms",
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
        if component.is_none() {
            // Services and programs have no IR yet (RFC-0011 §13, RFC-0013), so these
            // commands have nothing to print. Saying so is a usage error, not a silent
            // success.
            let kind = match check_program(&src, path) {
                (Some(Program::Service(_)), _) => Some("services"),
                (Some(Program::Program(_)), _) => Some("programs"),
                _ => None,
            };
            if let Some(kind) = kind {
                eprintln!(
                    "mz: `{command}` does not cover {kind} yet; `check`, `fix` and `contract` do"
                );
                return ExitCode::from(2);
            }
        }
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

/// `mz run` (RFC-0013 §13.1): check, lower into the build cache, `cargo build --offline`,
/// then run the binary with this process's standard input, output and error, and exit
/// with its status. `mz`'s own messages go to standard error, which leaves standard output
/// to the program.
fn run(src: &str, path: &str, agent: bool, release: bool) -> ExitCode {
    use mzizi_lang_compiler::run::{RunError, build, cache_dir, lower, write};
    if agent {
        eprintln!(
            "mz: `run --agent` is designed (RFC-0013 §13.1) but not built yet; run `mz check --agent` for NDJSON diagnostics"
        );
        return ExitCode::from(2);
    }
    let (program, report) = check_program(src, path);
    // Warnings too: a run that succeeds still says what `mz check` would.
    print_human_to(&report, &mut std::io::stderr().lock());
    if report.error_count() > 0 {
        eprintln!(
            "mz: {} errors ({} exact-fixable); nothing ran",
            report.error_count(),
            report.exact_fixable()
        );
        return ExitCode::from(3);
    }
    let Some(Program::Program(p)) = program else {
        eprintln!("mz: `run` runs a program (`program <name>` … `end program <name>`)");
        return ExitCode::from(2);
    };
    let package = lower(&p, &file_name(path));
    let binary = match cache_dir(&package, std::path::Path::new(path))
        .and_then(|dir| write(&package, &dir).map(|()| dir))
        .and_then(|dir| build(&package, &dir, release))
    {
        Ok(b) => b,
        Err(e @ RunError::Build(_)) => {
            // Lowered code that rustc rejects is a compiler bug, by construction (P5).
            eprintln!(
                "{path}:{}:{}: error [MZ0990] the Rust lowered from `program {}` did not compile — a bug in mz, not in the program: {e}",
                p.name_span.start_line, p.name_span.start_col, p.name,
            );
            return ExitCode::from(3);
        }
        Err(e) => {
            eprintln!("mz: {e}");
            return ExitCode::from(2);
        }
    };
    match std::process::Command::new(&binary).status() {
        Ok(status) => ExitCode::from(exit_byte(status)),
        Err(e) => {
            eprintln!("mz: cannot run {}: {e}", binary.display());
            ExitCode::from(2)
        }
    }
}

/// A child's exit status as one byte: its code, or 128 plus the signal that ended it.
fn exit_byte(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return u8::try_from(code & 0xff).unwrap_or(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(sig) = status.signal() {
            return u8::try_from((128 + sig) & 0xff).unwrap_or(1);
        }
    }
    1
}

/// The file name alone, for the generated package's header.
fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// One line per diagnostic, in the same shape for every subcommand.
fn print_human(report: &mzizi_lang_compiler::diagnostic::CheckReport) {
    print_human_to(report, &mut std::io::stdout().lock());
}

/// [`print_human`], to any stream: `mz run` writes it to standard error.
fn print_human_to(report: &mzizi_lang_compiler::diagnostic::CheckReport, out: &mut dyn Write) {
    for d in &report.diagnostics {
        let level = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let _ = writeln!(
            out,
            "{}:{}:{}: {} [{}] {}",
            d.file, d.span.start_line, d.span.start_col, level, d.code, d.say
        );
        if let Some(fix) = &d.fix {
            let _ = writeln!(out, "    fix ({:?}): {:?}", fix.confidence, fix.replace);
        }
    }
}
