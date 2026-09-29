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

#[test]
fn three_broken_contract_clauses_produce_exactly_three_diagnostics() {
    // The recovery guarantee, extended to the contract sub-grammar. A contract block is
    // where an agent writes the least familiar syntax in the language, so a cascade here
    // would be expensive exactly where it can least be afforded.
    let src = "\
component a
  enum e
    one k \"1\"
  end
  contract
    every e
    e.one k 1
    k wobble \"x\"
  end
end component a
";
    let found = errors(src);
    assert_eq!(
        found.len(),
        3,
        "expected exactly 3 diagnostics, got: {found:#?}"
    );
}

#[test]
fn a_bare_operand_in_a_contract_clause_carries_the_exact_repair() {
    // `button_size.default height 56` is how the corpus wrote it. RFC-0001 §1.2 allows one
    // form per intent, so the abbreviation is an error — but a purely mechanical one, so it
    // must be fixable with no model in the loop.
    let src = "\
component a
  enum e
    one k 56
  end
  contract
    e.one k 56
  end
end component a
";
    let report = check(src, "t.mz");
    let d = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0602")
        .expect("a missing predicate must be reported");
    let fix = d.fix.as_ref().expect("and must carry a repair");
    assert_eq!(fix.replace, "is ");
    assert_eq!(fix.confidence, Confidence::Exact);
    // A pure insertion: it adds the missing word and deletes nothing.
    assert_eq!(fix.span.start_col, fix.span.end_col);
}

#[test]
fn a_broken_contract_clause_does_not_swallow_the_rest_of_the_block() {
    // "Never fix one to see the next", inside a contract: a clause the grammar rejects must
    // not cost the assertions written after it.
    let src = "\
component a
  enum e
    one k \"1\"
  end
  contract
    e.one k wobble
    every e k not_empty
  end
end component a
";
    let (report, tally) = mzizi_lang_compiler::check_contract(src, "t.mz");
    assert_eq!(report.error_count(), 1, "{:#?}", report.diagnostics);
    // The file does not compile, so nothing was evaluated — but the parser still saw the
    // second clause, which is what keeps the next iteration to one round trip.
    assert_eq!(tally.clauses, 0);
}

// ---------------------------------------------------------------------------------------
// Cascades pilot 2 found (benchmarks/results/2026-09-27-pilot-2/RUN.md), each pinned to
// one diagnostic whose fix, where it is `exact`, repairs the file.
// ---------------------------------------------------------------------------------------

use mzizi_lang_compiler::apply_exact_fixes;

fn component_with_view(view: &str) -> String {
    format!(
        "component q\n  prop label: text\n  prop open: bool = false\n  view\n{view}\n  end\n  contract\n    slot is \"q\"\n  end\nend component q\n"
    )
}

/// Exactly one error, with `code`; applying the `exact` fixes leaves no error.
fn one_error_fixed_by_mz_fix(src: &str, code: &str) -> String {
    let found = errors(src);
    assert_eq!(found.len(), 1, "one mistake, one diagnostic: {found:#?}");
    assert_eq!(found[0].0, code, "{found:#?}");
    let fixed = apply_exact_fixes(src, &check(src, "t.mz"));
    assert!(errors(&fixed).is_empty(), "{fixed}\n{:#?}", errors(&fixed));
    found[0].1.clone()
}

#[test]
fn a_missing_equals_is_one_diagnostic_on_the_line_that_is_wrong() {
    // Divergence 4: `class "flex"` produced four errors, at `contract` and at the last
    // line, and none on line 6.
    let src = component_with_view(
        "    row\n      slot = \"q\"\n      class \"flex\"\n      text = label\n    end",
    );
    let say = one_error_fixed_by_mz_fix(&src, "MZ0406");
    assert!(say.contains("`class \"flex\"` is missing its `=`"), "{say}");
    let d = check(&src, "t.mz")
        .diagnostics
        .into_iter()
        .find(|d| d.code == "MZ0406")
        .unwrap();
    assert_eq!(d.span.start_line, 7);
}

#[test]
fn a_missing_equals_after_an_unknown_word_is_only_a_guess() {
    let src = component_with_view("    row\n      slot = \"q\"\n      tooltip \"hi\"\n    end");
    let report = check(&src, "t.mz");
    let d = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0406")
        .unwrap();
    assert_eq!(d.fix.as_ref().unwrap().confidence, Confidence::Guess);
    // Still one diagnostic: the line is read as the attribute, not as a block opener.
    assert_eq!(report.error_count(), 1, "{:#?}", report.diagnostics);
}

#[test]
fn if_is_one_diagnostic_whose_fix_is_when() {
    // Divergence 5: `if open` compiled silently, as an element named `if`.
    let src = component_with_view(
        "    row\n      slot = \"q\"\n      if open\n        span\n          text = label\n        end\n      end\n    end",
    );
    let say = one_error_fixed_by_mz_fix(&src, "MZ0407");
    assert!(say.contains("no `if`"), "{say}");
}

#[test]
fn an_element_word_with_more_on_its_line_is_an_error_not_a_silent_tail() {
    let src = component_with_view(
        "    row\n      slot = \"q\"\n      span class = \"x\"\n        text = label\n      end\n    end",
    );
    let found = errors(&src);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "MZ0409");
}

#[test]
fn attributes_directly_under_view_are_one_diagnostic_not_a_cascade() {
    // Every 7B badge episode: `slot = …` lines straight under `view`, each read as an
    // element whose block swallowed the rest of the file.
    let src = component_with_view(
        "    slot = \"q\"\n    class = \"inline-flex\"\n    row\n      text = label\n    end",
    );
    let found = errors(&src);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "MZ0408");
    assert!(found[0].1.contains("lines 5–6"), "{}", found[0].1);
}

#[test]
fn contract_inside_an_unclosed_view_is_one_diagnostic_and_the_contract_still_parses() {
    let src = "component g\n  view\n    row\n      slot = \"g\"\n  end\n  contract\n    slot is \"g\"\n  end\nend component g\n";
    let report = check(src, "t.mz");
    let found = errors(src);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "MZ0204");
    assert!(
        found[0].1.contains("`view` opened on line 2"),
        "{}",
        found[0].1
    );
    // The contract after it was parsed as a contract, not as view lines.
    assert!(report.diagnostics.iter().all(|d| d.code != "MZ0501"));
}

#[test]
fn end_component_with_blocks_still_open_is_one_diagnostic_not_two_conflicting_fixes() {
    let src = "component g\n  view\n    row\n      slot = \"g\"\n    end\nend component g\n";
    let found = errors(src);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, "MZ0204");
    assert!(
        found[0].1.contains("an `end` is missing above"),
        "{}",
        found[0].1
    );
}

#[test]
fn every_closer_fix_rewrites_the_whole_closer() {
    // Divergence 7: the fixes replaced only the `end` word, so the echo stayed behind it.
    for (closer, code) in [
        ("    end view", "MZ0206"),     // was `end element view`
        ("    end enum row", "MZ0206"), // was `end element enum row`
        ("    end fn x", "MZ0206"),
    ] {
        let src = format!(
            "component g\n  view\n    row\n      slot = \"g\"\n{closer}\n  end\n  contract\n    slot is \"g\"\n  end\nend component g\n"
        );
        one_error_fixed_by_mz_fix(&src, code);
    }
    // A nameless `end component` was repaired to `end component g component`.
    one_error_fixed_by_mz_fix("component g\n  contract\n  end\nend component\n", "MZ0208");
    // `end enum g` for `enum g_tone`: an inner block needs no echo, so the repair is `end`.
    let src = "component g\n  enum g_tone\n    a class \"x\"\n  end enum g\n  prop tone: g_tone = a\n  contract\n  end\nend component g\n";
    one_error_fixed_by_mz_fix(src, "MZ0207");
    let fixed = apply_exact_fixes(src, &check(src, "t.mz"));
    assert!(fixed.contains("\n  end\n  prop tone"), "{fixed}");
}

#[test]
fn the_7b_models_seed_1_badge_names_each_mistake_once() {
    // Byte for byte what the ~7B model wrote in all five iterations of pilot 2's badge
    // seed 1. On `a9c928d` it produced 13 errors, 9 of them `exact`-fixable, and the only
    // one near the spreads said `prop needs a name, found .`; seven of the 13 were
    // unclosed-block reports at the last line. Every error now names a real mistake.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../benchmarks/results/2026-09-27-pilot-2/raw/scored/qwen2.5-coder-7b-instruct-q4km/mzizi/badge/seed-1/iter-01/candidate.mz",
    );
    let src = std::fs::read_to_string(path).unwrap();
    let codes: Vec<&str> = errors(&src).iter().map(|(c, _)| *c).collect();
    assert_eq!(codes, ["MZ0106", "MZ0408", "MZ0106", "MZ0106"], "{codes:?}");
    // The contract block is parsed as a contract again.
    let report = check(&src, "t.mz");
    assert!(report.diagnostics.iter().all(|d| d.code != "MZ0501"));
}
