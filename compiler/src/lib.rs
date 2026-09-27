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

use diagnostic::CheckReport;

/// Parse, then resolve every name and type (RFC-0008). Resolution runs even when parsing
/// reported errors, so an agent sees the whole error set in one pass (FM-5); names a
/// broken line declared are carried as unknown, so that is one diagnostic, not a cascade.
fn front_end(src: &str, file: &str) -> (Option<ast::Component>, Vec<diagnostic::Diagnostic>) {
    let (component, mut diagnostics) = parse::parse(src, file);
    if let Some(component) = &component {
        diagnostics.append(&mut resolve::resolve(component, file));
    }
    (component, diagnostics)
}

/// Check one source file and return its diagnostics in deterministic order.
pub fn check(src: &str, file: &str) -> CheckReport {
    let (_component, diagnostics) = front_end(src, file);
    let mut report = CheckReport { diagnostics };
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
    let (component, diagnostics) = front_end(src, file);
    let mut report = CheckReport { diagnostics };
    let mut tally = contract::Tally::default();
    if report.error_count() == 0
        && let Some(component) = &component
    {
        let (evaluated, mut failures) = contract::evaluate(component, file);
        tally = evaluated;
        report.diagnostics.append(&mut failures);
    }
    report.sort();
    (report, tally)
}

/// Check a file and also return the parsed component, for callers that need the tree.
pub fn check_with_ast(src: &str, file: &str) -> (Option<ast::Component>, CheckReport) {
    let (component, diagnostics) = front_end(src, file);
    let mut report = CheckReport { diagnostics };
    report.sort();
    (component, report)
}
