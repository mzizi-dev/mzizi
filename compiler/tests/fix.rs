//! `mz fix`: every `exact` fix, applied in one pass (RFC-0001 §4.3).
//!
//! Diagnostics have called their repairs `exact` since the first commit, and RFC-0001 §4.3
//! promised a command that applies them without a model in the loop. Until 2026-09-29 the
//! command did not exist: `mz fix` printed usage and exited 2 (pilot 2's RUN.md, divergence
//! 2). These tests hold the promise that makes the word `exact` mean something: applying
//! every `exact` fix blind leaves a file with no error an `exact` fix was attached to.

use std::process::Command;

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Span};
use mzizi_lang_compiler::{apply_exact_fixes, check};

/// The Mzizi guide's own wrong-on-purpose file (`benchmarks/prompts/mzizi-guide.md`,
/// "Reading diagnostics"): three mistakes, each with an `exact` fix.
const TAG: &str = "\
## WRONG ON PURPOSE: three mistakes.
component tag

  enum tag_size
    snug  class \"h-13 px-3\"  height 52
  end

  prop onPick: event(none)
  prop size: tag_size = snug

  view
    row
      slot = \"tag\"
      class = \"rounded-full {size.class}\"
      tap = on_pick
    end
  end

  contract
    slot is \"tag\"
    tag_size.snug height 52
  end

end
";

#[test]
fn the_guides_three_mistakes_are_repaired_in_one_pass() {
    let before = check(TAG, "tag.mz");
    assert_eq!(before.error_count(), 3, "{:#?}", before.diagnostics);
    assert_eq!(before.exact_fixable(), 3);
    let fixed = apply_exact_fixes(TAG, &before);
    let after = check(&fixed, "tag.mz");
    assert_eq!(after.error_count(), 0, "{fixed}\n{:#?}", after.diagnostics);
    assert!(fixed.contains("prop on_pick: event(none)"));
    assert!(fixed.contains("tag_size.snug height is 52"));
    assert!(fixed.trim_end().ends_with("end component tag"));
}

#[test]
fn insertions_at_one_point_land_in_diagnostic_order() {
    // Three blocks left open at EOF: three `MZ0204` insertions at the same point, innermost
    // first. Applied in that order, they close the blocks in the order they must close.
    let src = "component a\n  view\n    row\n      slot = \"a\"\n";
    let report = check(src, "a.mz");
    let fixed = apply_exact_fixes(src, &report);
    assert!(
        fixed.ends_with("      slot = \"a\"\nend\nend\nend component a\n"),
        "{fixed:?}"
    );
    assert_eq!(check(&fixed, "a.mz").error_count(), 0, "{fixed}");
}

#[test]
fn guess_fixes_are_never_applied() {
    let src = "a b c\n";
    let mut report = check("component x\nend component x\n", "x.mz");
    report.diagnostics = vec![
        Diagnostic::error("MZ0000", "x.mz", Span::single(1, 1, 1), "g").with_fix(
            Span::single(1, 1, 1),
            "Z",
            Confidence::Guess,
        ),
        Diagnostic::error("MZ0000", "x.mz", Span::single(1, 5, 1), "e").with_fix(
            Span::single(1, 5, 1),
            "Q",
            Confidence::Exact,
        ),
    ];
    assert_eq!(apply_exact_fixes(src, &report), "a b Q\n");
}

#[test]
fn a_whole_line_deletion_works_on_the_last_line_without_a_newline() {
    let src = "one\ntwo";
    let mut report = check("component x\nend component x\n", "x.mz");
    report.diagnostics = vec![
        Diagnostic::error("MZ0000", "x.mz", Span::single(2, 1, 3), "d").with_fix(
            Span {
                start_line: 2,
                start_col: 1,
                end_line: 3,
                end_col: 1,
            },
            "",
            Confidence::Exact,
        ),
    ];
    assert_eq!(apply_exact_fixes(src, &report), "one\n");
}

#[test]
fn columns_count_characters_not_bytes() {
    // `§` is two bytes and one character; the lexer counts characters, so must the fixer.
    let src = "## §§\ncomponent a\n  prop badName: bool\n  contract\n  end\nend component a\n";
    let report = check(src, "a.mz");
    let fixed = apply_exact_fixes(src, &report);
    assert!(fixed.contains("prop bad_name: bool"), "{fixed}");
}

#[test]
fn the_shipped_binary_fixes_in_place_and_reports_what_is_left() {
    let path = std::env::temp_dir().join(format!("mz-fix-{}.mz", std::process::id()));
    std::fs::write(&path, TAG).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .arg("fix")
        .arg("--agent")
        .arg(&path)
        .output()
        .expect("mz runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains(r#""errors":0"#), "{stdout}");
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("end component tag"), "{written}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_unclosed_block_on_a_last_line_without_a_newline_closes_after_it() {
    let src = "component a\n  contract\n  end";
    let fixed = apply_exact_fixes(src, &check(src, "a.mz"));
    assert_eq!(fixed, "component a\n  contract\n  end\nend component a");
    assert_eq!(check(&fixed, "a.mz").error_count(), 0);
}
