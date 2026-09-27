//! Phase 0 run orchestration — the "run" half of `design/ROADMAP.md`'s gap table, next to
//! `benchmarks/harness/`'s "score" half.
//!
//! Given a named component, a sequence of candidate `.mz` sources (one per authoring
//! iteration), and a reference `.rs` file, this crate's binary (`src/main.rs`) drives the
//! actual pipeline the Phase 0 defect metric needs:
//!
//! 1. Run `mz check --agent` against each candidate in order until one compiles cleanly —
//!    that candidate's 1-based position is `iterations_to_clean_compile`.
//! 2. Run `mz contract --agent` against the clean candidate. It must exit 0 (self-consistency
//!    — RFC-0006) before anything else runs.
//! 3. Hand the clean candidate and the reference to this workspace's `mzizi-benchmark-harness`
//!    (RFC-0006 §10.1) for the reference diff, and count its results as defects.
//!
//! **What this crate does not do, honestly:** it does not author the candidates. A live
//! agent run — one process that writes `.mz` source, reads `mz check`'s diagnostics back,
//! and revises — is not wired up here (`benchmarks/README.md` says why: this session found
//! shelling out to `claude` from inside its own container reused this very session's model
//! context and billing rather than running an isolated agent, which would make any resulting
//! token or iteration count measure this orchestrator's own overhead, not a component
//! author's). So the candidate sequence is supplied by the caller — hand-authored files that
//! stand in for successive agent attempts — and `tokens_consumed` is always `None` unless the
//! caller passes a real measured count in. Nothing here fabricates a number for either.

#![deny(missing_docs)]

use std::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mzizi_benchmark_harness::{diff_size_enum, parse_mzizi_size_enums, parse_rust_match_arms};

/// The outcome of running `mz check --agent` or `mz contract --agent` against one file:
/// the raw NDJSON stdout, and the two counts every summary line carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCheckOutcome {
    /// The full NDJSON stdout, one diagnostic per line plus the trailing summary line.
    pub raw_ndjson: String,
    /// `errors` from the summary line. Zero means a clean compile.
    pub errors: u64,
    /// `warnings` from the summary line.
    pub warnings: u64,
    /// `contract_clauses` from the summary line, present only for `mz contract --agent`.
    pub contract_clauses: Option<u64>,
    /// `contract_failures` from the summary line, present only for `mz contract --agent`.
    pub contract_failures: Option<u64>,
}

/// Parse one `mz check --agent` / `mz contract --agent` NDJSON stream into its counts.
///
/// The summary line is always last (RFC-0001 §4.4) and always starts `{"summary":true,`, so
/// this looks for that line specifically rather than assuming it is the final line verbatim
/// — a trailing blank line from the subprocess's stdout should not break parsing.
pub fn parse_agent_outcome(stdout: &str) -> Option<AgentCheckOutcome> {
    let summary = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with(r#"{"summary":true"#))?;
    Some(AgentCheckOutcome {
        raw_ndjson: stdout.to_string(),
        errors: extract_u64_field(summary, "errors")?,
        warnings: extract_u64_field(summary, "warnings")?,
        contract_clauses: extract_u64_field(summary, "contract_clauses"),
        contract_failures: extract_u64_field(summary, "contract_failures"),
    })
}

/// Read an unsigned integer field out of one hand-rolled JSON object line, e.g.
/// `extract_u64_field(r#"{"summary":true,"errors":3,...}"#, "errors")` → `Some(3)`.
///
/// This is deliberately not a JSON parser: it looks for `"<key>":` and reads the digits that
/// follow, which is exactly what the compiler's own hand-rolled emitter (`diagnostic.rs`)
/// guarantees for these integer fields.
pub fn extract_u64_field(line: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let digits: String = line[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Diff a clean candidate's declared sizes against the reference, via
/// `mzizi-benchmark-harness`'s own public API — no subprocess needed since that crate is a
/// workspace-local library as well as a binary.
///
/// Returns the total defect count and one formatted line per defect (`"<enum>: <result>"`),
/// ready to drop into a `RunResult`'s `defects` field.
pub fn defects_from_diff(mz_src: &str, reference_src: &str) -> (u64, Vec<String>) {
    let rust_variants = parse_rust_match_arms(reference_src);
    let mut count = 0u64;
    let mut defects = Vec::new();
    for size_enum in parse_mzizi_size_enums(mz_src) {
        let report = diff_size_enum(&size_enum, &rust_variants);
        for result in &report.results {
            if result.is_defect() {
                count += 1;
                defects.push(format!("{}: {result}", report.enum_name));
            }
        }
    }
    (count, defects)
}

/// One recorded Phase 0 run, matching the JSON-lines schema in `benchmarks/results/runs.jsonl`.
#[derive(Clone, Debug, Default)]
pub struct RunResult {
    /// The component name, e.g. `button`.
    pub component: String,
    /// RFC 3339 UTC timestamp of when the run was recorded.
    pub timestamp: String,
    /// 1-based index of the first candidate that reached a clean `mz check --agent`, or
    /// `None` if no candidate did.
    pub iterations_to_clean_compile: Option<u64>,
    /// Tokens spent authoring the candidates, if a live agent produced them and reported a
    /// real count. `None` — never a fabricated number — when the candidates are a scripted
    /// stand-in, which is the only mode this crate currently drives end to end.
    pub tokens_consumed: Option<u64>,
    /// `contract_clauses` from `mz contract --agent` on the clean candidate.
    pub contract_clauses: Option<u64>,
    /// `contract_failures` from `mz contract --agent` on the clean candidate.
    pub contract_failures: Option<u64>,
    /// Total defects the harness's reference diff found.
    pub defect_count: Option<u64>,
    /// One formatted line per defect the harness's reference diff found.
    pub defects: Vec<String>,
    /// Wall-clock time for the whole run (every candidate's `mz check`, `mz contract`, and
    /// the harness diff), in milliseconds.
    pub elapsed_ms: u128,
    /// `"scripted-stand-in"` (candidates were hand-authored, not produced by a live agent
    /// loop) or `"live-agent"` (a real agent produced the candidates and `tokens_consumed`
    /// is a measured number). See this module's own doc comment for why every run so far is
    /// the former.
    pub authoring_mode: String,
    /// The candidate file that reached a clean compile, if any.
    pub clean_candidate: Option<String>,
    /// Free-text note on where the pipeline stopped, if it didn't reach a scored diff.
    pub notes: Option<String>,
}

impl RunResult {
    /// Serialize as one JSON object, no trailing newline — the caller adds one line per
    /// call when appending to `runs.jsonl`.
    pub fn to_json_line(&self) -> String {
        let mut out = String::new();
        out.push('{');
        write!(out, r#""component":{},"#, json_string(&self.component)).unwrap();
        write!(out, r#""timestamp":{},"#, json_string(&self.timestamp)).unwrap();
        write!(
            out,
            r#""iterations_to_clean_compile":{},"#,
            opt_u64(self.iterations_to_clean_compile)
        )
        .unwrap();
        write!(
            out,
            r#""tokens_consumed":{},"#,
            opt_u64(self.tokens_consumed)
        )
        .unwrap();
        write!(
            out,
            r#""contract_clauses":{},"#,
            opt_u64(self.contract_clauses)
        )
        .unwrap();
        write!(
            out,
            r#""contract_failures":{},"#,
            opt_u64(self.contract_failures)
        )
        .unwrap();
        write!(out, r#""defect_count":{},"#, opt_u64(self.defect_count)).unwrap();
        out.push_str(r#""defects":["#);
        for (i, d) in self.defects.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&json_string(d));
        }
        out.push_str("],");
        write!(out, r#""elapsed_ms":{},"#, self.elapsed_ms).unwrap();
        write!(
            out,
            r#""authoring_mode":{},"#,
            json_string(&self.authoring_mode)
        )
        .unwrap();
        write!(
            out,
            r#""clean_candidate":{},"#,
            match &self.clean_candidate {
                Some(s) => json_string(s),
                None => "null".to_string(),
            }
        )
        .unwrap();
        write!(
            out,
            r#""notes":{}"#,
            match &self.notes {
                Some(s) => json_string(s),
                None => "null".to_string(),
            }
        )
        .unwrap();
        out.push('}');
        out
    }
}

fn opt_u64(v: Option<u64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

/// Escape a Rust string as a JSON string literal, quotes included — the same hand-rolled
/// approach `compiler/src/diagnostic.rs::json_string` uses, kept local rather than pulled in
/// as a dependency on the compiler crate for one function (this crate's whole point is to
/// stay dependency-free, per its `Cargo.toml`).
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

/// Format the current time as an RFC 3339 UTC timestamp (`2026-09-27T14:03:11Z`), with no
/// `chrono` dependency: `SystemTime` gives seconds since the epoch, and civil-calendar
/// conversion from a day count is a well-known, easily-checked algorithm (Howard Hinnant's
/// `civil_from_days`), reproduced here in full so it stays auditable.
pub fn now_rfc3339() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    rfc3339_from_unix_secs(since_epoch.as_secs())
}

/// The same, from an explicit Unix timestamp — split out so it can be unit-tested against
/// known dates without depending on the wall clock.
pub fn rfc3339_from_unix_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days since the Unix epoch (1970-01-01) → (year, month, day). Howard Hinnant's
/// `civil_from_days`, a widely-used, proleptic-Gregorian, dependency-free algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_field_from_a_summary_line() {
        let line = r#"{"summary":true,"errors":3,"warnings":1,"exact_fixable":2,"ms":7}"#;
        assert_eq!(extract_u64_field(line, "errors"), Some(3));
        assert_eq!(extract_u64_field(line, "warnings"), Some(1));
        assert_eq!(extract_u64_field(line, "ms"), Some(7));
        assert_eq!(extract_u64_field(line, "contract_clauses"), None);
    }

    #[test]
    fn parses_a_check_agent_stream_with_one_error() {
        let stdout = "{\"code\":\"MZ0204\",\"severity\":\"error\",\"file\":\"a.mz\",\"span\":[1,1,1,1],\"say\":\"x\"}\n{\"summary\":true,\"errors\":1,\"warnings\":0,\"exact_fixable\":1,\"ms\":0}\n";
        let outcome = parse_agent_outcome(stdout).expect("summary line present");
        assert_eq!(outcome.errors, 1);
        assert_eq!(outcome.warnings, 0);
        assert_eq!(outcome.contract_clauses, None);
    }

    #[test]
    fn parses_a_contract_agent_stream_with_clause_counts() {
        let stdout = "{\"summary\":true,\"errors\":0,\"warnings\":0,\"exact_fixable\":0,\"contract_clauses\":5,\"contract_failures\":1,\"ms\":0}\n";
        let outcome = parse_agent_outcome(stdout).expect("summary line present");
        assert_eq!(outcome.errors, 0);
        assert_eq!(outcome.contract_clauses, Some(5));
        assert_eq!(outcome.contract_failures, Some(1));
    }

    #[test]
    fn a_stream_with_no_summary_line_parses_to_none() {
        assert_eq!(parse_agent_outcome("not ndjson at all\n"), None);
    }

    const BUTTON_MZ: &str = r#"component button
  enum button_size
    default   class "h-14 gap-2 px-5"     height 56
    sm        class "h-12 gap-1.5 px-4"   height 48
  end
end component button
"#;

    const BUTTON_BROKEN_MZ: &str = r#"component button
  enum button_size
    default   class "h-14 gap-2 px-5"     height 56
    sm        class "h-12 gap-1.5 px-4"   height 44
  end
end component button
"#;

    const BUTTON_RS: &str = r#"
match self {
    ButtonSize::Default => "h-14 gap-2 px-5",
    ButtonSize::Sm => "h-12 gap-1.5 px-4",
}
"#;

    #[test]
    fn a_correct_candidate_diffs_to_zero_defects() {
        let (count, defects) = defects_from_diff(BUTTON_MZ, BUTTON_RS);
        assert_eq!(count, 0, "{defects:?}");
        assert!(defects.is_empty());
    }

    #[test]
    fn a_mismatched_candidate_diffs_to_one_named_defect() {
        let (count, defects) = defects_from_diff(BUTTON_BROKEN_MZ, BUTTON_RS);
        assert_eq!(count, 1);
        assert_eq!(defects.len(), 1);
        assert!(defects[0].contains("button_size"));
        assert!(defects[0].contains("sm"));
        assert!(defects[0].contains("mismatch"));
    }

    #[test]
    fn run_result_serializes_null_for_absent_optional_fields() {
        let result = RunResult {
            component: "button".to_string(),
            timestamp: "2026-09-27T00:00:00Z".to_string(),
            iterations_to_clean_compile: None,
            tokens_consumed: None,
            contract_clauses: None,
            contract_failures: None,
            defect_count: None,
            defects: vec![],
            elapsed_ms: 12,
            authoring_mode: "scripted-stand-in".to_string(),
            clean_candidate: None,
            notes: Some("never reached a clean compile".to_string()),
        };
        let json = result.to_json_line();
        assert!(json.contains(r#""iterations_to_clean_compile":null"#));
        assert!(json.contains(r#""tokens_consumed":null"#));
        assert!(json.contains(r#""clean_candidate":null"#));
        assert!(json.contains(r#""defects":[]"#));
        assert!(json.contains(r#""notes":"never reached a clean compile""#));
    }

    #[test]
    fn run_result_serializes_present_fields_and_escapes_defects() {
        let result = RunResult {
            component: "button".to_string(),
            timestamp: "2026-09-27T00:00:00Z".to_string(),
            iterations_to_clean_compile: Some(2),
            tokens_consumed: None,
            contract_clauses: Some(5),
            contract_failures: Some(0),
            defect_count: Some(1),
            defects: vec!["button_size: FAIL  sm mismatch \"h-12\"".to_string()],
            elapsed_ms: 340,
            authoring_mode: "scripted-stand-in".to_string(),
            clean_candidate: Some("fixtures/button_broken.mz".to_string()),
            notes: None,
        };
        let json = result.to_json_line();
        assert!(json.contains(r#""iterations_to_clean_compile":2"#));
        assert!(json.contains(r#""defect_count":1"#));
        assert!(json.contains(r#""notes":null"#));
        assert!(json.contains(r#"\"h-12\""#));
    }

    #[test]
    fn rfc3339_matches_known_unix_timestamps() {
        assert_eq!(rfc3339_from_unix_secs(0), "1970-01-01T00:00:00Z");
        // 2026-09-27T00:00:00Z, checked against `date -u -d "2026-09-27T00:00:00Z" +%s`.
        assert_eq!(
            rfc3339_from_unix_secs(1_790_467_200),
            "2026-09-27T00:00:00Z"
        );
    }
}
