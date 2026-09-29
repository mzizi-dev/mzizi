//! The React idioms an agent brings with it, each answered with one diagnostic.
//!
//! Pilot 2 (`benchmarks/results/2026-09-27-pilot-2/RUN.md`) traced the 7B model's Mzizi
//! failures to idioms that are everywhere in the shadcn/Radix specs the tasks come from and
//! that have no Mzizi form: `...props` spreading and `asChild`. The compiler rejected them
//! with diagnostics that told the model nothing it could act on (`MZ0304 prop needs a name,
//! found .`), and in 15 of 16 repair attempts it resubmitted the same file. Each test here
//! pins one idiom to one diagnostic that names it and carries the repair.

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Severity};
use mzizi_lang_compiler::{apply_exact_fixes, check};

fn errors(src: &str) -> Vec<Diagnostic> {
    check(src, "t.mz")
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

/// A component whose only mistakes are the lines passed in, placed where `at` says.
fn badge(props: &str, view_extra: &str) -> String {
    format!(
        "component badge
  enum badge_variant
    default  class \"bg-primary\"
    outline  class \"border-border\"
  end
  prop variant: badge_variant = default
{props}
  view
    row
      slot = \"badge\"
      class = \"inline-flex {{variant.class}}\"
{view_extra}
    end
  end
  contract
    slot is \"badge\"
  end
end component badge
"
    )
}

/// Assert one error with `code`, whose `exact` fix, applied, leaves a clean file.
fn one_exact_repair(src: &str, code: &str) -> Diagnostic {
    let found = errors(src);
    assert_eq!(found.len(), 1, "one idiom, one diagnostic: {found:#?}");
    let d = found.into_iter().next().unwrap();
    assert_eq!(d.code, code, "{d:#?}");
    let fix = d.fix.as_ref().expect("the idiom has a mechanical repair");
    assert_eq!(fix.confidence, Confidence::Exact);
    let report = check(src, "t.mz");
    let fixed = apply_exact_fixes(src, &report);
    let left = errors(&fixed);
    assert!(left.is_empty(), "after the fix:\n{fixed}\n{left:#?}");
    d
}

#[test]
fn a_prop_spread_is_one_diagnostic_that_deletes_the_line() {
    // Byte for byte what the 7B model wrote in every pilot-2 badge episode.
    let d = one_exact_repair(&badge("  prop ...props: event(none)", ""), "MZ0106");
    assert!(d.say.contains("`...props` is a spread"), "{}", d.say);
    let fix = d.fix.unwrap();
    assert_eq!(fix.replace, "");
    assert_eq!((fix.span.start_col, fix.span.end_col), (1, 1));
    assert_eq!(fix.span.end_line, fix.span.start_line + 1);
}

#[test]
fn a_view_spread_is_one_diagnostic_in_each_spelling() {
    for line in ["      ...props", "      {...props}", "      ..attributes"] {
        let d = one_exact_repair(&badge("", line), "MZ0106");
        assert!(d.say.contains("has no equivalent"), "{line}: {}", d.say);
    }
}

#[test]
fn a_spread_inside_a_longer_line_deletes_only_the_spread() {
    let src = badge("", "      span {...props}\n      end");
    let found = errors(&src);
    let d = found
        .iter()
        .find(|d| d.code == "MZ0106")
        .expect("the spread is reported");
    let fix = d.fix.as_ref().unwrap();
    assert_eq!(fix.span.start_line, fix.span.end_line, "only the spread");
    let fixed = apply_exact_fixes(&src, &check(&src, "t.mz"));
    assert!(fixed.contains("      span \n      end"), "{fixed}");
    assert!(errors(&fixed).is_empty(), "{fixed}");
}

#[test]
fn a_dotted_path_is_not_a_spread() {
    assert!(errors(&badge("", "")).is_empty());
    let src = badge("", "      data_variant = variant.class");
    assert!(errors(&src).iter().all(|d| d.code != "MZ0106"));
}

#[test]
fn an_unused_as_child_is_a_warning_whose_fix_deletes_it() {
    // Two of pilot 2's clean 7B buttons declared `as_child` and never read it. That
    // compiles to the reference's behaviour with the flag off, so it must still compile.
    let src = badge("  prop as_child: bool = false", "");
    let report = check(&src, "t.mz");
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
    let w = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0312")
        .expect("the idiom is named");
    assert_eq!(w.severity, Severity::Warning);
    assert!(w.say.contains("asChild"), "{}", w.say);
    let fix = w.fix.as_ref().unwrap();
    assert_eq!(
        (fix.confidence, fix.replace.as_str()),
        (Confidence::Exact, "")
    );
    let fixed = apply_exact_fixes(&src, &report);
    assert!(!fixed.contains("as_child"), "{fixed}");
    assert_eq!(check(&fixed, "t.mz").diagnostics.len(), 0, "{fixed}");
}

#[test]
fn an_as_child_the_view_reads_names_the_branch_and_has_no_fix() {
    let src = badge(
        "  prop as_child: bool = false",
        "      when as_child\n        span\n        end\n      end",
    );
    let report = check(&src, "t.mz");
    let w = report
        .diagnostics
        .iter()
        .find(|d| d.code == "MZ0312")
        .unwrap();
    assert!(w.say.contains("the branch on line 12"), "{}", w.say);
    assert!(
        w.fix.is_none(),
        "deleting the prop alone would leave `when as_child` unknown"
    );
    // `mz fix` therefore cannot turn a clean file into a broken one.
    assert_eq!(apply_exact_fixes(&src, &report), src);
}

#[test]
fn a_camel_case_as_child_is_still_a_naming_error() {
    let src = badge("  prop asChild: bool = false", "");
    let found = errors(&src);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].code, "MZ0101");
    // The line deletion and the rename overlap; the deletion, which starts first, is the
    // one `mz fix` applies.
    let fixed = apply_exact_fixes(&src, &check(&src, "t.mz"));
    assert!(
        !fixed.contains("as_child") && !fixed.contains("asChild"),
        "{fixed}"
    );
}
