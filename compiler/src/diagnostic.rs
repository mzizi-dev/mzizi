//! Diagnostics, and the agent-facing NDJSON protocol from RFC-0001 §4.
//!
//! The protocol is the product here, not an afterthought: a diagnostic is written for a
//! reader holding **zero file context**, so it quotes the offending source inline, and it
//! carries a machine-applicable `fix` whenever one is unambiguous. `mz fix` can then apply
//! every `exact` fix in one pass, deleting a whole class of round-trip from the agent loop.

use std::fmt::Write as _;

/// A half-open source span, 1-indexed lines and columns (RFC-0001 §4.2's `[l, c, l, c]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Line the span starts on, 1-indexed.
    pub start_line: u32,
    /// Column the span starts at, 1-indexed.
    pub start_col: u32,
    /// Line the span ends on, 1-indexed.
    pub end_line: u32,
    /// Column the span ends at, 1-indexed and exclusive.
    pub end_col: u32,
}

impl Span {
    /// A span covering one token on a single line.
    pub fn single(line: u32, col: u32, len: u32) -> Self {
        Span {
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col + len,
        }
    }
}

/// How much an agent should trust a [`Fix`].
///
/// `Exact` means applying it blind is safe — `mz fix` does exactly that. `Guess` means the
/// repair site is right but the replacement is inferred (a nearest-name suggestion), so a
/// reader should look. Diagnostics with no mechanical repair carry no fix at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confidence {
    /// Safe to apply without review.
    Exact,
    /// Right location, inferred replacement.
    Guess,
}

impl Confidence {
    fn as_str(self) -> &'static str {
        match self {
            Confidence::Exact => "exact",
            Confidence::Guess => "guess",
        }
    }
}

/// A machine-applicable repair: replace `span` with `replace`.
#[derive(Clone, Debug, PartialEq)]
pub struct Fix {
    /// The range to replace.
    pub span: Span,
    /// The replacement text. Empty means deletion.
    pub replace: String,
    /// How much to trust this repair.
    pub confidence: Confidence,
}

/// Error severity. Warnings do not fail the check; errors do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Fails the check.
    Error,
    /// Reported, does not fail the check.
    Warning,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One diagnostic — the unit of the agent protocol.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    /// Stable machine code, `MZ` + four digits. Agents may match on this.
    pub code: &'static str,
    /// Severity.
    pub severity: Severity,
    /// Source file the diagnostic belongs to.
    pub file: String,
    /// Where the problem is.
    pub span: Span,
    /// What to tell the reader. Written for zero file context: quotes the offending
    /// source inline so the agent need not re-read the file. Target <= 200 chars.
    pub say: String,
    /// A machine-applicable repair, when one is unambiguous.
    pub fix: Option<Fix>,
}

impl Diagnostic {
    /// An error with no mechanical repair.
    pub fn error(code: &'static str, file: &str, span: Span, say: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            file: file.to_string(),
            span,
            say: say.into(),
            fix: None,
        }
    }

    /// A warning with no mechanical repair.
    pub fn warning(code: &'static str, file: &str, span: Span, say: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Warning,
            file: file.to_string(),
            span,
            say: say.into(),
            fix: None,
        }
    }

    /// Attach a repair.
    pub fn with_fix(mut self, span: Span, replace: impl Into<String>, c: Confidence) -> Self {
        self.fix = Some(Fix {
            span,
            replace: replace.into(),
            confidence: c,
        });
        self
    }

    /// Render as one NDJSON line (no trailing newline).
    pub fn to_ndjson(&self) -> String {
        let mut s = String::with_capacity(256);
        s.push('{');
        write!(
            s,
            r#""code":"{}","severity":"{}","#,
            self.code,
            self.severity.as_str()
        )
        .unwrap();
        write!(s, r#""file":{},"#, json_string(&self.file)).unwrap();
        write!(
            s,
            r#""span":[{},{},{},{}],"#,
            self.span.start_line, self.span.start_col, self.span.end_line, self.span.end_col
        )
        .unwrap();
        write!(s, r#""say":{}"#, json_string(&self.say)).unwrap();
        if let Some(fix) = &self.fix {
            write!(
                s,
                r#","fix":{{"span":[{},{},{},{}],"replace":{},"confidence":"{}"}}"#,
                fix.span.start_line,
                fix.span.start_col,
                fix.span.end_line,
                fix.span.end_col,
                json_string(&fix.replace),
                fix.confidence.as_str()
            )
            .unwrap();
        }
        s.push('}');
        s
    }
}

/// Escape a Rust string as a JSON string literal, quotes included.
///
/// Hand-rolled to keep the crate dependency-free, matching the house convention in
/// `mzizi-assurance` and `mzizi-discovery`. Control characters below 0x20 must be escaped
/// or the output is not valid JSON and every consumer breaks on the first tab.
pub fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                write!(out, "\\u{:04x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The result of checking one file: diagnostics in deterministic source order.
#[derive(Debug, Default)]
pub struct CheckReport {
    /// Every diagnostic, sorted by position.
    pub diagnostics: Vec<Diagnostic>,
}

impl CheckReport {
    /// Sort into deterministic source order. The protocol promises stable ordering so an
    /// agent diffing two runs sees real change, not reordering.
    pub fn sort(&mut self) {
        self.diagnostics.sort_by_key(|d| {
            (
                d.span.start_line,
                d.span.start_col,
                d.span.end_line,
                d.span.end_col,
                d.code,
            )
        });
    }

    /// Count of `Error`-severity diagnostics. Non-zero means the check failed.
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// Count of diagnostics carrying an `exact` fix — what `mz fix` would resolve in one pass.
    pub fn exact_fixable(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| matches!(&d.fix, Some(f) if f.confidence == Confidence::Exact))
            .count()
    }

    /// The whole report as NDJSON plus RFC-0001 §4.4's summary line.
    pub fn to_ndjson(&self, elapsed_ms: u128) -> String {
        self.to_ndjson_with(elapsed_ms, None)
    }

    /// The same, with RFC-0001 §4.4's contract counts appended to the summary.
    ///
    /// §4.4 fixes the summary line as `mz: 3 errors (2 exact-fixable), 1 contract failure,
    /// 480ms` — contract results were always part of the protocol. They are two extra keys
    /// rather than a second line, so a consumer that reads the last line still gets
    /// everything, and one written before contracts existed sees the keys it knows.
    pub fn to_ndjson_with(&self, elapsed_ms: u128, contract: Option<(usize, usize)>) -> String {
        let mut out = String::new();
        for d in &self.diagnostics {
            out.push_str(&d.to_ndjson());
            out.push('\n');
        }
        let warnings = self.diagnostics.len() - self.error_count();
        write!(
            out,
            r#"{{"summary":true,"errors":{},"warnings":{},"exact_fixable":{}"#,
            self.error_count(),
            warnings,
            self.exact_fixable(),
        )
        .unwrap();
        if let Some((clauses, failed)) = contract {
            write!(
                out,
                r#","contract_clauses":{clauses},"contract_failures":{failed}"#
            )
            .unwrap();
        }
        writeln!(out, r#","ms":{elapsed_ms}}}"#).unwrap();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_are_escaped_into_valid_json() {
        // A raw tab or newline inside a JSON string is invalid JSON; every consumer would
        // break on the first quoted source line containing one.
        assert_eq!(json_string("a\tb"), r#""a\tb""#);
        assert_eq!(json_string("a\nb"), r#""a\nb""#);
        // A raw 0x01 must emit as its \u escape, not pass through as a literal byte.
        assert_eq!(json_string("a\u{1}b"), "\"a\\u0001b\"");
    }

    #[test]
    fn quotes_and_backslashes_survive_a_round_trip() {
        assert_eq!(json_string(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(json_string(r"back\slash"), r#""back\\slash""#);
    }

    #[test]
    fn a_diagnostic_without_a_fix_emits_no_fix_key() {
        let d = Diagnostic::error("MZ0401", "a.mz", Span::single(1, 1, 3), "bad");
        let line = d.to_ndjson();
        assert!(!line.contains("\"fix\""));
        assert!(line.contains(r#""code":"MZ0401""#));
        assert!(line.contains(r#""span":[1,1,1,4]"#));
    }

    #[test]
    fn a_diagnostic_with_a_fix_carries_it_as_data() {
        let d = Diagnostic::error("MZ0101", "a.mz", Span::single(3, 8, 9), "camelCase").with_fix(
            Span::single(3, 8, 9),
            "on_state",
            Confidence::Exact,
        );
        let line = d.to_ndjson();
        assert!(line.contains(r#""replace":"on_state""#));
        assert!(line.contains(r#""confidence":"exact""#));
    }

    #[test]
    fn the_summary_line_counts_errors_warnings_and_fixables() {
        let mut r = CheckReport::default();
        r.diagnostics.push(
            Diagnostic::error("MZ0101", "a.mz", Span::single(1, 1, 1), "x").with_fix(
                Span::single(1, 1, 1),
                "y",
                Confidence::Exact,
            ),
        );
        r.diagnostics.push(Diagnostic::error(
            "MZ0401",
            "a.mz",
            Span::single(2, 1, 1),
            "y",
        ));
        r.diagnostics.push(Diagnostic::warning(
            "MZ0501",
            "a.mz",
            Span::single(3, 1, 1),
            "z",
        ));
        let out = r.to_ndjson(7);
        assert!(out.contains(r#""errors":2,"warnings":1,"exact_fixable":1,"ms":7"#));
    }

    #[test]
    fn ordering_is_deterministic_by_position() {
        let mut r = CheckReport::default();
        r.diagnostics.push(Diagnostic::error(
            "MZ0401",
            "a.mz",
            Span::single(9, 1, 1),
            "late",
        ));
        r.diagnostics.push(Diagnostic::error(
            "MZ0401",
            "a.mz",
            Span::single(2, 5, 1),
            "early",
        ));
        r.sort();
        assert_eq!(r.diagnostics[0].say, "early");
    }
}
