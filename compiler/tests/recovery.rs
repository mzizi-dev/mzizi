//! The recovery guarantee, tested as a guarantee.
//!
//! RFC-0001 §4.1 promises **at most one diagnostic per true author error, never a cascade,
//! and never "fix one to see the next"**. That is the single most load-bearing claim in the
//! design — it is what converts a five-iteration agent loop into one. A promise like that
//! is worth nothing unless a test fails when it breaks, so each test below counts
//! diagnostics rather than merely asserting that some error was reported.

use mzizi_lang_compiler::check;
use mzizi_lang_compiler::diagnostic::{Confidence, Severity};

fn errors(src: &str) -> Vec<(&'static str, String)> {
    check(src, "t.mz")
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code, d.say))
        .collect()
}

#[test]
fn three_independent_errors_produce_exactly_three_diagnostics() {
    // A naming error, a missing type, and an unknown character — three real mistakes on
    // three lines. A cascading parser would report far more than three.
    let src = "\
component connectivity_bar
  prop badName: bool
  prop state
  prop other: bool §
  contract
  end
end component connectivity_bar
";
    let found = errors(src);
    assert_eq!(
        found.len(),
        3,
        "expected exactly 3 diagnostics, got: {found:#?}"
    );
    let codes: Vec<&str> = found.iter().map(|(c, _)| *c).collect();
    assert!(codes.contains(&"MZ0101"), "naming error missing: {codes:?}");
    assert!(
        codes.contains(&"MZ0305"),
        "missing-type error missing: {codes:?}"
    );
    assert!(
        codes.contains(&"MZ0104"),
        "bad-character error missing: {codes:?}"
    );
}

#[test]
fn an_error_on_one_line_does_not_hide_the_next_line() {
    // "Never fix one to see the next": both errors must appear in a single pass.
    let src = "\
component a
  prop firstBad: bool
  prop secondBad: bool
  contract
  end
end component a
";
    let found = errors(src);
    assert_eq!(
        found.len(),
        2,
        "both naming errors must surface in one pass: {found:#?}"
    );
}

#[test]
fn a_missing_closer_is_one_diagnostic_naming_the_opening_line() {
    let src = "\
component a
  contract
  end
";
    let found = errors(src);
    assert_eq!(
        found.len(),
        1,
        "one unclosed block, one diagnostic: {found:#?}"
    );
    assert_eq!(found[0].0, "MZ0204");
    // The diagnostic must name where the block was opened — that is the whole point of the
    // block stack, and what a reader with no file context needs.
    assert!(
        found[0].1.contains("line 1"),
        "must point at the opener: {}",
        found[0].1
    );
}

#[test]
fn a_missing_closer_carries_the_exact_text_that_closes_it() {
    let report = check("component a\n  contract\n  end\n", "t.mz");
    let d = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0204")
        .unwrap();
    let fix = d
        .fix
        .as_ref()
        .expect("an unclosed block has a mechanical repair");
    assert_eq!(fix.confidence, Confidence::Exact);
    assert_eq!(fix.replace, "end component a\n");
}

#[test]
fn a_mismatched_end_name_is_one_diagnostic_with_the_right_name_as_the_fix() {
    // This is the failure mode `end <kind> <name>` exists to catch (RFC-0002 §1: an
    // error-correcting code for a reader that cannot track nesting).
    let src = "\
component connectivity_bar
  contract
  end
end component connectivty_bar
";
    let found = errors(src);
    assert_eq!(found.len(), 1, "a typo'd echo is one error: {found:#?}");
    assert_eq!(found[0].0, "MZ0207");
    let report = check(src, "t.mz");
    let d = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0207")
        .unwrap();
    let fix = d.fix.as_ref().unwrap();
    assert_eq!(fix.replace, "connectivity_bar");
    assert_eq!(fix.confidence, Confidence::Exact);
}

#[test]
fn a_missing_name_echo_on_a_top_level_block_is_reported_with_the_full_closer() {
    let src = "\
component a
  contract
  end
end
";
    let found = errors(src);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "MZ0208");
    let report = check(src, "t.mz");
    let fix = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0208")
        .unwrap()
        .fix
        .clone()
        .unwrap();
    assert_eq!(fix.replace, "end component a");
}

#[test]
fn a_stray_end_is_reported_once_and_deletable() {
    let src = "\
component a
  contract
  end
end component a
end
";
    let found = errors(src);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "MZ0205");
    let report = check(src, "t.mz");
    let fix = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0205")
        .unwrap()
        .fix
        .clone()
        .unwrap();
    assert_eq!(fix.replace, "", "a stray closer is deleted, not replaced");
}

#[test]
fn a_missing_enum_column_is_caught_which_is_the_whole_point_of_columns() {
    // The corpus's parallel `Record` maps drifted exactly this way — a variant missing its
    // entry, discovered only at runtime. Columns-on-variants makes it a compile error.
    let src = "\
component a
  enum state
    online   label \"Back online\"  color \"bg-malachite\"
    offline  label \"You are offline\"
  end
  contract
  end
end component a
";
    let found = errors(src);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "MZ0303");
    assert!(
        found[0].1.contains("color"),
        "must name the missing column: {}",
        found[0].1
    );
    assert!(
        found[0].1.contains("offline"),
        "must name the variant: {}",
        found[0].1
    );
}

#[test]
fn a_file_that_does_not_start_with_component_fails_once_not_repeatedly() {
    let found = errors("prop a: bool\nprop b: bool\nprop c: bool\n");
    assert_eq!(
        found.len(),
        1,
        "one structural error, not one per line: {found:#?}"
    );
    assert_eq!(found[0].0, "MZ0201");
}

#[test]
fn every_diagnostic_quotes_source_so_a_reader_needs_no_file() {
    // RFC-0001 §4.2: `say` is written for a reader holding zero file context.
    let src = "component a\n  prop badName: bool\nend component a\n";
    let report = check(src, "t.mz");
    let d = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0101")
        .unwrap();
    assert!(
        d.say.contains("badName"),
        "must quote the offending source: {}",
        d.say
    );
    assert!(
        d.say.contains("bad_name"),
        "must show the repair: {}",
        d.say
    );
}

#[test]
fn diagnostics_stay_within_the_two_hundred_character_density_budget() {
    // Density is the budget (RFC-0001 §4.2). A diagnostic that sprawls is a diagnostic that
    // costs an agent context for no added information.
    let src = "\
component connectivity_bar
  prop someBadName: bool
  prop state
  enum s
    a x \"1\"
    b
  end
end
";
    let report = check(src, "t.mz");
    assert!(!report.diagnostics.is_empty());
    for d in &report.diagnostics {
        assert!(
            d.say.chars().count() <= 200,
            "[{}] is {} chars, over budget: {}",
            d.code,
            d.say.chars().count(),
            d.say
        );
    }
}

#[test]
fn the_ndjson_output_is_one_line_per_diagnostic_plus_a_summary() {
    let src = "component a\n  prop badName: bool\nend component a\n";
    let report = check(src, "t.mz");
    let out = report.to_ndjson(3);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.len(),
        report.diagnostics.len() + 1,
        "diagnostics + summary"
    );
    assert!(lines.last().unwrap().contains(r#""summary":true"#));
    // Every line must be independently parseable — that is what NDJSON buys.
    for line in &lines {
        assert!(
            line.starts_with('{') && line.ends_with('}'),
            "not a JSON object: {line}"
        );
    }
}

#[test]
fn the_summary_reports_how_many_errors_mz_fix_would_resolve() {
    // The number an agent actually acts on: if every error is exact-fixable, the right move
    // is `mz fix` and re-check, with no generation at all.
    let src = "component a\n  prop badName: bool\n  prop otherBad: bool\nend component a\n";
    let report = check(src, "t.mz");
    assert_eq!(report.error_count(), 2);
    assert_eq!(report.exact_fixable(), 2);
    assert!(report.to_ndjson(1).contains(r#""errors":2"#));
}
