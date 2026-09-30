//! The Mzizi-lang Phase 0 compiler prototype.
//!
//! Implements the front end from [RFC-0001](../../design/RFC-0001-syntax.md) and the
//! design target from [RFC-0002](../../design/RFC-0002-runtime-and-prior-art.md): a lexer,
//! a recursive-descent parser with per-line recovery, a name and type resolver
//! ([RFC-0008](../../design/RFC-0008-types-collections-records.md)), and the agent-facing
//! NDJSON diagnostic protocol. Nothing lowers yet — lowering waits on the content-addressed IR
//! (RFC-0002 §2.1).
//!
//! The point of the prototype is to make the design's two central claims testable:
//!
//! 1. **One diagnostic per real error.** Line-oriented recovery plus `end`-delimited blocks
//!    means a mistake never cascades, so an agent sees the whole error set in one pass
//!    rather than peeling them off one compile at a time.
//! 2. **Fixes as data.** Mechanical errors — naming, missing closers, wrong echoes — carry
//!    an `exact` repair the toolchain can apply with no model in the loop.
//!
//! ```
//! use mzizi_lang_compiler::check;
//!
//! let report = check("component a\nend component a\n", "a.mz");
//! // No errors; the missing `contract` block is a warning.
//! assert_eq!(report.error_count(), 0);
//! ```

#![deny(missing_docs)]

pub mod ast;
pub mod contract;
pub mod diagnostic;
pub mod hash;
pub mod ir;
pub mod lex;
pub mod outline;
pub mod parse;
pub mod resolve;
pub mod serve;
pub mod service;

use diagnostic::{CheckReport, Severity};

/// Parse, then resolve every name and type (RFC-0008), or check the service (RFC-0011).
/// Resolution runs even when parsing reported errors, so an agent sees the whole error set
/// in one pass (FM-5); names a broken line declared are carried as unknown, so that is one
/// diagnostic, not a cascade.
fn front_end_program(
    src: &str,
    file: &str,
) -> (Option<parse::Program>, Vec<diagnostic::Diagnostic>) {
    let (program, mut diagnostics) = parse::parse_program(src, file);
    let resolved = match &program {
        Some(parse::Program::Component(component)) => resolve::resolve(component, file),
        Some(parse::Program::Service(s)) => service::check(s, file),
        None => Vec::new(),
    };
    // The lexer reports a camelCase word as MZ0101 and hands on its snake_case form.
    // If that form still names nothing, the resolver reports the same token again —
    // one mistake, two overlapping fixes. Keep the resolver's: its fix replaces the
    // whole written word with the name that does resolve.
    diagnostics.retain(|d| {
        d.code != "MZ0101"
            || !resolved
                .iter()
                .any(|r| r.severity == Severity::Error && r.span == d.span)
    });
    diagnostics.extend(resolved);
    (program, diagnostics)
}

/// [`front_end_program`], for callers that handle components only.
fn front_end(src: &str, file: &str) -> (Option<ast::Component>, Vec<diagnostic::Diagnostic>) {
    let (program, diagnostics) = front_end_program(src, file);
    match program {
        Some(parse::Program::Component(c)) => (Some(c), diagnostics),
        _ => (None, diagnostics),
    }
}

/// Check one source file and return its diagnostics in deterministic order.
pub fn check(src: &str, file: &str) -> CheckReport {
    let (_program, diagnostics) = front_end_program(src, file);
    let mut report = CheckReport { diagnostics };
    report.disjoint_exact_fixes();
    report.sort();
    report
}

/// Check one file and evaluate its `contract` block (RFC-0006).
///
/// Returns the report — parse diagnostics and contract failures together, in source order —
/// and the tally of how many assertions ran.
///
/// A file that does not compile has its contract **skipped**, deliberately. CHARTER.md §6
/// separates the two things being measured: "a syntax/compile error is not itself a defect
/// for this metric — the defect rate measures what gets _past_ the compiler wrong". Running
/// assertions against a tree the parser had to guess at would blur exactly that line.
pub fn check_contract(src: &str, file: &str) -> (CheckReport, contract::Tally) {
    let (program, diagnostics) = front_end_program(src, file);
    let mut report = CheckReport { diagnostics };
    let mut tally = contract::Tally::default();
    if report.error_count() == 0 {
        match &program {
            Some(parse::Program::Component(component)) => {
                let (evaluated, mut failures) = contract::evaluate(component, file);
                tally = evaluated;
                report.diagnostics.append(&mut failures);
            }
            Some(parse::Program::Service(s)) => {
                let (evaluated, mut failures) = service_contract(s, file);
                tally = evaluated;
                report.diagnostics.append(&mut failures);
            }
            None => {}
        }
    }
    report.disjoint_exact_fixes();
    report.sort();
    (report, tally)
}

/// A service's contract, evaluated by running the service in process (RFC-0011 §7).
fn service_contract(
    s: &service::Service,
    file: &str,
) -> (contract::Tally, Vec<diagnostic::Diagnostic>) {
    serve::evaluate(s, file)
}

/// Apply every `exact` fix in `report` to `src` in one pass (RFC-0001 §4.3), and return
/// the new text.
///
/// [`CheckReport::disjoint_exact_fixes`] has already made the `exact` fixes pairwise
/// disjoint, so the result does not depend on the order they are applied in. Two insertions
/// at the same point land in diagnostic order, which is how missing closers have always
/// been reported. `guess` fixes are never applied. A span is 1-indexed lines and columns in
/// characters, the way the lexer counts; a position past the end of a line or of the file
/// is clamped to it, so a fix that deletes the file's last line needs no trailing newline.
///
/// [`CheckReport::disjoint_exact_fixes`]: diagnostic::CheckReport::disjoint_exact_fixes
pub fn apply_exact_fixes(src: &str, report: &CheckReport) -> String {
    let chars: Vec<char> = src.chars().collect();
    // Char offset of the start of each line, and the offset just past each line's text
    // (before its `\r\n` or `\n`), so a column never reaches into a line ending.
    let mut starts = vec![0usize];
    for (i, c) in chars.iter().enumerate() {
        if *c == '\n' {
            starts.push(i + 1);
        }
    }
    let line_text_end = |l: usize| -> usize {
        let end = starts.get(l + 1).map_or(chars.len(), |&s| s - 1);
        if end > starts[l] && chars.get(end - 1) == Some(&'\r') {
            end - 1
        } else {
            end
        }
    };
    let offset = |line: u32, col: u32| -> usize {
        let l = (line as usize).saturating_sub(1);
        if l >= starts.len() {
            return chars.len();
        }
        (starts[l] + (col as usize).saturating_sub(1)).min(line_text_end(l).max(starts[l]))
    };
    let mut edits: Vec<(usize, usize, usize, &str)> = report
        .diagnostics
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let f = d.fix.as_ref()?;
            if f.confidence != diagnostic::Confidence::Exact {
                return None;
            }
            let start = offset(f.span.start_line, f.span.start_col);
            // An end on the line after the last is the end of the file.
            let end = if f.span.end_col == 1 && f.span.end_line > f.span.start_line {
                let l = f.span.end_line as usize - 1;
                starts.get(l).copied().unwrap_or(chars.len())
            } else {
                offset(f.span.end_line, f.span.end_col)
            };
            Some((start, end.max(start), i, f.replace.as_str()))
        })
        .collect();
    // Last edit first, so earlier offsets stay valid; at one point, the later diagnostic
    // first, so the earlier one's text ends up in front.
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.2.cmp(&a.2)));
    let mut out = chars;
    for (start, end, _, replace) in edits {
        out.splice(start..end, replace.chars());
    }
    out.into_iter().collect()
}

/// Check a file and also return its parsed declaration, component or service.
pub fn check_program(src: &str, file: &str) -> (Option<parse::Program>, CheckReport) {
    let (program, diagnostics) = front_end_program(src, file);
    let mut report = CheckReport { diagnostics };
    report.disjoint_exact_fixes();
    report.sort();
    (program, report)
}

/// Check a file and also return the parsed component, for callers that need the tree.
pub fn check_with_ast(src: &str, file: &str) -> (Option<ast::Component>, CheckReport) {
    let (component, diagnostics) = front_end(src, file);
    let mut report = CheckReport { diagnostics };
    report.disjoint_exact_fixes();
    report.sort();
    (component, report)
}
