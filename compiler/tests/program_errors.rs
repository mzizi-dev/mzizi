//! RFC-0013 §12, errors in a program (LANGUAGE-TRACKER C9): `enum`s in a program,
//! `result(T, E)`, `return error(e)`, prefix `try`, `match` with `case ok` / `case error`,
//! and `main` returning `result(none, E)`.
//!
//! The same four kinds of test as `program.rs`: the parse; the check, one test per
//! diagnostic, with its `exact` fix applied and the result checked again; the lowered text;
//! and `mz run` end to end on `examples/errors.mz` and on a `main` that returns an error.
//! C9's "Done when" is `examples/errors.mz` (a function returns an error that one caller
//! handles with `match` and another propagates with `try`) and the `MZ0950` tests.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Severity};
use mzizi_lang_compiler::expr::canonical;
use mzizi_lang_compiler::parse::{Program, parse_program};
use mzizi_lang_compiler::program::StmtKind;
use mzizi_lang_compiler::{apply_exact_fixes, check};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn example(name: &str) -> String {
    std::fs::read_to_string(root().join("examples").join(name)).expect("example exists")
}

fn program(src: &str) -> mzizi_lang_compiler::program::Program {
    match parse_program(src, "t.mz") {
        (Some(Program::Program(p)), _) => p,
        other => panic!("not a program: {other:?}"),
    }
}

/// A program with an error enum and a function that fails, around `fns`: more functions,
/// each `fn` … `end fn` already indented by two.
fn with(fns: &str) -> String {
    format!(
        "program t

  enum problem
    negative say \"is below zero\"
    too_big  say \"is too big\"
  end

{fns}
  fn check(n: int): result(int, problem)
    when n < 0
      return error(negative)
    end
    when n > 100
      return error(too_big)
    end
    return n
  end fn check

end program t
"
    )
}

/// [`with`], with `body` as `fn main`'s body; `main` returns nothing.
fn main_body(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    with(&format!("  fn main\n{body}  end fn main\n"))
}

/// [`with`], with `body` as the body of `fn f(n: int): <ret>` beside an empty `main`.
fn in_fn(ret: &str, body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    with(&format!(
        "  fn main\n    print(\"t\")\n  end fn main\n\n  fn f(n: int): {ret}\n{body}  end fn f\n"
    ))
}

fn errors(src: &str) -> Vec<Diagnostic> {
    check(src, "t.mz")
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

/// Check `src`, expect exactly one error with `code`, and return it.
fn one(src: &str, code: &str) -> Diagnostic {
    let errors = errors(src);
    assert_eq!(
        errors.len(),
        1,
        "one diagnostic per true error, for:\n{src}\ngot {errors:#?}"
    );
    assert_eq!(errors[0].code, code, "{src}\n{:#?}", errors[0]);
    errors[0].clone()
}

/// [`one`], with no fix.
fn no_fix(src: &str, code: &str) -> Diagnostic {
    let d = one(src, code);
    assert!(d.fix.is_none(), "{d:#?}");
    d
}

/// [`one`], with a `guess` fix that inserts or replaces with `replace`.
fn guess(src: &str, code: &str, replace: &str) -> Diagnostic {
    let d = one(src, code);
    let fix = d.fix.as_ref().expect("a guess fix");
    assert_eq!(fix.confidence, Confidence::Guess, "{d:#?}");
    assert_eq!(fix.replace, replace, "{d:#?}");
    d
}

/// [`one`], then apply its `exact` fix and check the result has no errors and holds `want`.
fn fixed_by(src: &str, code: &str, want: &str) {
    let d = one(src, code);
    let fix = d.fix.as_ref().expect("an exact fix");
    assert_eq!(fix.confidence, Confidence::Exact, "{d:#?}");
    let report = check(src, "t.mz");
    let after = apply_exact_fixes(src, &report);
    let again = check(&after, "t.mz");
    assert_eq!(
        again.error_count(),
        0,
        "the fix must leave a program that checks:\n{after}\n{:#?}",
        again.diagnostics
    );
    assert!(after.contains(want), "expected `{want}` in:\n{after}");
}

fn clean(src: &str) {
    let errors = errors(src);
    assert!(errors.is_empty(), "{src}\n{errors:#?}");
}

// ------------------------------------------------------------------- parse

#[test]
fn the_example_parses_and_checks_clean() {
    let src = example("errors.mz");
    let report = check(&src, "errors.mz");
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    let p = program(&src);
    assert_eq!(p.enums.len(), 1);
    let e = &p.enums[0];
    assert_eq!(e.name, "age_problem");
    let variants: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(variants, ["negative", "too_old"]);
    assert_eq!(e.variants[1].columns[0].name, "say");
    assert_eq!(
        canonical(&e.variants[1].columns[0].value),
        "\"is above 150\""
    );
    let sigs: Vec<String> = p.fns.iter().map(|f| f.signature()).collect();
    assert_eq!(
        sigs,
        [
            "fn main: result(none, age_problem)",
            "fn check_age(n: int): result(int, age_problem)",
            "fn describe(n: int): text",
            "fn total_age(a: int, b: int): result(int, age_problem)",
        ]
    );
    // `describe` is one `match` (C4's, RFC-0013 §7.2) whose two cases, `ok` and `error`,
    // bind `age` and `problem`.
    let StmtKind::Match {
        scrutinee,
        arms,
        otherwise,
    } = &p.fns[2].body[0].kind
    else {
        panic!("{:#?}", p.fns[2].body[0]);
    };
    assert_eq!(canonical(scrutinee), "check_age(n)");
    assert!(otherwise.is_none());
    let arms: Vec<(String, &str)> = arms
        .iter()
        .map(|a| {
            let values: Vec<String> = a.values.iter().map(canonical).collect();
            (
                values.join(" "),
                a.binding.as_ref().map_or("", |b| b.0.as_str()),
            )
        })
        .collect();
    assert_eq!(
        arms,
        [("ok".to_string(), "age"), ("error".to_string(), "problem")]
    );
    // `try` binds tighter than `+` (§3.5, level 3).
    let StmtKind::Return(Some(v)) = &p.fns[3].body[0].kind else {
        panic!("{:#?}", p.fns[3].body[0]);
    };
    assert_eq!(canonical(v), "try check_age(a) + try check_age(b)");
}

#[test]
fn try_binds_looser_than_a_dot_and_tighter_than_arithmetic() {
    let p = program(&in_fn(
        "result(int, problem)",
        "let a = try check(n) * 2\nlet b = -try check(n)\nlet c = negative.say\nreturn a + b",
    ));
    let values: Vec<String> = p.fns[1]
        .body
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::Bind { value, .. } => Some(canonical(value)),
            _ => None,
        })
        .collect();
    assert_eq!(
        values,
        ["try check(n) * 2", "-(try check(n))", "negative.say"]
    );
}

// ------------------------------------------------------------------- what checks clean

#[test]
fn a_result_held_by_a_let_and_then_matched_or_propagated_is_clean() {
    clean(&main_body(
        "let r = check(5)\nmatch r\n  case ok v\n    print(\"ok {v}\")\n  case error e\n    print(\"error {e}\")\nend",
    ));
    clean(&in_fn(
        "result(int, problem)",
        "let r = check(n)\nlet v = try r\nreturn v + 1",
    ));
    // §12.1: a result of the function's own type is passed through, not wrapped.
    clean(&in_fn("result(int, problem)", "return check(n)"));
    clean(&in_fn("result(int, problem)", "let r = check(n)\nreturn r"));
    // `result(none, E)` returns success with a bare `return` or by reaching `end fn`.
    clean(&in_fn(
        "result(none, problem)",
        "when n is 0\n  return\nend\nlet v = try check(n)\nprint(v)",
    ));
    // Enum values compare with `is`, in either spelling, and order by declaration.
    clean(&in_fn(
        "bool",
        "return problem.too_big is too_big and negative < too_big",
    ));
}

#[test]
fn main_may_return_result_none_and_nothing_else() {
    clean(&with(
        "  fn main: result(none, problem)\n    let v = try check(1)\n    print(v)\n  end fn main\n",
    ));
    let d = no_fix(
        &with("  fn main: result(int, problem)\n    return 1\n  end fn main\n"),
        "MZ0902",
    );
    assert!(d.say.contains("result(none, E)"), "{d:#?}");
}

// ------------------------------------------------------------------- MZ0950

#[test]
fn mz0950_a_discarded_result() {
    // In a function that cannot propagate, there is no fix.
    let d = no_fix(&main_body("check(1)"), "MZ0950");
    assert!(d.say.contains("result(int, problem)"), "{d:#?}");
    // In one with the same error type, the fix is the `guess` `try`, never `exact`.
    guess(
        &in_fn("result(none, problem)", "check(n)"),
        "MZ0950",
        "try ",
    );
}

#[test]
fn mz0950_a_result_used_as_its_success_value() {
    guess(
        &in_fn("result(int, problem)", "return check(n) + 1"),
        "MZ0950",
        "try ",
    );
    no_fix(&main_body("let n = check(1)\nprint(n * 2)"), "MZ0950");
    no_fix(
        &main_body("when check(1) is 3\n  print(\"three\")\nend"),
        "MZ0950",
    );
    no_fix(&main_body("print(-check(1))"), "MZ0950");
}

#[test]
fn mz0950_a_result_printed_interpolated_or_passed() {
    no_fix(&main_body("print(check(1))"), "MZ0950");
    no_fix(&main_body("print(\"{check(1)}\")"), "MZ0950");
    no_fix(
        &with(
            "  fn main\n    show(check(1))\n  end fn main\n\n  fn show(n: int)\n    print(n)\n  end fn show\n",
        ),
        "MZ0950",
    );
}

#[test]
fn mz0950_a_let_whose_result_nothing_matches() {
    let d = no_fix(&main_body("let r = check(1)\nprint(\"done\")"), "MZ0950");
    assert_eq!(d.span.start_line, 9, "{d:#?}");
    assert!(d.say.contains("`r` holds a result(int, problem)"), "{d:#?}");
    // Inside a `when`, it must be handled before that block ends.
    guess(
        &in_fn(
            "result(int, problem)",
            "when n > 1\n  let r = check(n)\nend\nreturn 0",
        ),
        "MZ0950",
        "try ",
    );
    // A `var` cannot hold one at all.
    guess(
        &main_body(
            "var r = check(1)\nmatch r\n  case ok v\n    print(v)\n  case error e\n    print(e)\nend",
        ),
        "MZ0950",
        "let",
    );
}

#[test]
fn mz0950_a_result_returned_where_the_type_differs() {
    let src = with(
        "  fn main\n    print(\"t\")\n  end fn main\n\n  fn g(n: int): result(text, problem)\n    return check(n)\n  end fn g\n",
    );
    let d = guess(&src, "MZ0950", "try ");
    assert!(d.say.contains("returns result(text, problem)"), "{d:#?}");
    no_fix(&in_fn("int", "return check(n)"), "MZ0950");
}

#[test]
fn mz0950_a_result_parameter_or_a_result_inside_a_result() {
    no_fix(
        &with(
            "  fn main\n    print(\"t\")\n  end fn main\n\n  fn g(r: result(int, problem)): int\n    return 1\n  end fn g\n",
        ),
        "MZ0950",
    );
    no_fix(
        &in_fn(
            "result(result(int, problem), problem)",
            "return error(negative)",
        ),
        "MZ0950",
    );
}

#[test]
fn mz0950_try_on_a_dotted_read_is_fixed_to_parenthesise_the_try() {
    let src = with(
        "  fn main\n    print(\"t\")\n  end fn main\n\n  fn worst(n: int): result(problem, problem)\n    when n < 0\n      return error(negative)\n    end\n    return too_big\n  end fn worst\n\n  fn label(n: int): result(text, problem)\n    return try worst(n).say\n  end fn label\n",
    );
    fixed_by(&src, "MZ0950", "return (try worst(n)).say");
}

// ------------------------------------------------------------------- MZ0951

#[test]
fn mz0951_try_that_cannot_propagate() {
    // Not on a result.
    let d = no_fix(&in_fn("result(int, problem)", "return try n"), "MZ0951");
    assert!(d.say.contains("`n` is int"), "{d:#?}");
    // In a function that does not return a result.
    let d = no_fix(&main_body("let v = try check(1)\nprint(v)"), "MZ0951");
    assert!(d.say.contains("does not return a result"), "{d:#?}");
    // With a different error type: M1 converts none.
    let d = no_fix(&in_fn("result(int, text)", "return try check(n)"), "MZ0951");
    assert!(d.say.contains("fails with problem"), "{d:#?}");
}

// ------------------------------------------------------------------- MZ0952

#[test]
fn mz0952_rust_result_constructors_have_exact_fixes() {
    fixed_by(
        &in_fn("result(int, problem)", "return Ok(n)"),
        "MZ0952",
        "return n\n",
    );
    fixed_by(
        &in_fn("result(int, problem)", "return ok(n)"),
        "MZ0952",
        "return n\n",
    );
    fixed_by(
        &in_fn("result(int, problem)", "return Err(negative)"),
        "MZ0952",
        "return error(negative)",
    );
}

#[test]
fn mz0952_a_postfix_question_mark_becomes_a_prefix_try() {
    fixed_by(
        &in_fn("result(int, problem)", "let v = check(n)?\nreturn v"),
        "MZ0952",
        "let v = try check(n)",
    );
    // A chain of them is one diagnostic, whose one fix rewrites the chain, so no two
    // `exact` fixes overlap. (`try try` is then `MZ0951`: the inner `try` is an int.)
    let src = in_fn("result(int, problem)", "let v = check(n)??\nreturn v");
    let report = check(&src, "t.mz");
    let idioms: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "MZ0952")
        .collect();
    assert_eq!(idioms.len(), 1, "{:#?}", report.diagnostics);
    let fix = idioms[0].fix.as_ref().expect("a fix");
    assert_eq!(fix.replace, "try try check(n)");
    // In a type, `?` keeps the component's `MZ0104`.
    let d = one(&in_fn("int?", "return n"), "MZ0104");
    assert!(d.say.contains("option(<type>)"), "{d:#?}");
}

#[test]
fn mz0952_throw_and_raise() {
    // `exact` when the thrown value visibly has the function's error type.
    fixed_by(
        &in_fn("result(int, problem)", "throw negative"),
        "MZ0952",
        "return error(negative)",
    );
    fixed_by(
        &in_fn("result(int, problem)", "raise problem.too_big"),
        "MZ0952",
        "return error(problem.too_big)",
    );
    // A `guess` otherwise, and the line says nothing more.
    guess(
        &in_fn("result(int, problem)", "raise \"bad\""),
        "MZ0952",
        "return error(\"bad\")",
    );
    guess(
        &main_body("throw negative"),
        "MZ0952",
        "return error(negative)",
    );
}

#[test]
fn mz0952_exception_blocks_unwrap_and_expect_have_no_fix() {
    let d = no_fix(
        &main_body("try:\n    print(check(1))\nexcept:\n    print(\"no\")\nprint(\"after\")"),
        "MZ0952",
    );
    assert!(d.say.contains("match"), "{d:#?}");
    no_fix(
        &main_body("try {\n  print(1)\n} catch (e) {\n  print(2)\n}\nprint(\"after\")"),
        "MZ0952",
    );
    no_fix(&main_body("let v = check(1).unwrap()\nprint(v)"), "MZ0952");
    no_fix(
        &main_body("let v = check(1).expect(\"x\")\nprint(v)"),
        "MZ0952",
    );
}

// ------------------------------------------------------------------- MZ0953, MZ0954

#[test]
fn mz0953_error_that_does_not_fit_its_function() {
    let d = no_fix(&main_body("print(error(negative))"), "MZ0953");
    assert!(d.say.contains("does not return a result"), "{d:#?}");
    let d = no_fix(
        &in_fn("result(int, problem)", "return error(\"bad\")"),
        "MZ0953",
    );
    assert!(d.say.contains("fails with problem"), "{d:#?}");
}

#[test]
fn mz0953_a_bare_ok_or_error_variant_in_a_result_function_is_qualified() {
    let src = "program t\n\n  enum status\n    ok\n    error\n  end\n\n  fn main\n    print(\"t\")\n  end fn main\n\n  fn pick(n: int): result(status, text)\n    when n is 0\n      return error(\"zero\")\n    end\n    return error\n  end fn pick\n\nend program t\n";
    fixed_by(src, "MZ0953", "return status.error\n");
    // Outside a result function, the bare variant is just the variant.
    clean(
        "program t\n\n  enum status\n    ok\n    error\n  end\n\n  fn main\n    print(error)\n  end fn main\n\nend program t\n",
    );
}

#[test]
fn mz0954_return_try_of_the_functions_own_result_type() {
    fixed_by(
        &in_fn("result(int, problem)", "return try check(n)"),
        "MZ0954",
        "return check(n)\n",
    );
    // Not the case when the `try` is part of a larger value.
    clean(&in_fn("result(int, problem)", "return try check(n) + 1"));
}

// ------------------------------------------------------------------- match on a result

#[test]
fn mz0930_a_match_on_a_result_handles_both_cases() {
    let d = no_fix(
        &main_body("match check(1)\n  case ok v\n    print(v)\nend"),
        "MZ0930",
    );
    assert!(d.say.contains("misses `case error`"), "{d:#?}");
    // `else` is never a case of a result (§12): it is `MZ0931`, whose exact fix deletes
    // it, and the case it stood in for is still missing.
    let all = errors(&main_body(
        "match check(1)\n  case ok v\n    print(v)\n  else\n    print(\"no\")\nend",
    ));
    let codes: Vec<&str> = all.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["MZ0930", "MZ0931"], "{all:#?}");
    // MZ0930 points at the matched value, as for every `match`.
    assert_eq!(all[0].span.start_col, 11, "{all:#?}");
    // `case ok x` / `else` / `case error e`: the `else` goes, `case error` stays.
    fixed_by(
        &main_body(
            "match check(1)\n  case ok v\n    print(v)\n  else\n    print(\"no\")\n  case error e\n    print(e)\nend",
        ),
        "MZ0931",
        "        print(v)\n      case error e\n",
    );
}

#[test]
fn mz0931_a_case_that_is_never_reached_is_deleted() {
    fixed_by(
        &main_body(
            "match check(1)\n  case ok v\n    print(v)\n  case error e\n    print(e)\n  case error f\n    print(f)\nend",
        ),
        "MZ0931",
        "    print(e)\n    end\n",
    );
    fixed_by(
        &main_body(
            "match check(1)\n  case ok v\n    print(v)\n  case error e\n    print(e)\n  else\n    print(\"never\")\nend",
        ),
        "MZ0931",
        "    print(e)\n    end\n",
    );
}

#[test]
fn a_case_binds_its_name_in_its_own_arm_only() {
    // Read after the `match`, the name is `MZ0920`: its block ended.
    let d = no_fix(
        &main_body(
            "match check(1)\n  case ok v\n    print(v)\n  case error e\n    print(e)\nend\nprint(v)",
        ),
        "MZ0920",
    );
    assert!(d.say.contains("in a block that ended"), "{d:#?}");
    // `case ok` binds nothing when the success type is `none`, and must bind otherwise.
    clean(&with(
        "  fn main\n    match act(1)\n      case ok\n        print(\"done\")\n      case error e\n        print(e)\n    end\n  end fn main\n\n  fn act(n: int): result(none, problem)\n    let v = try check(n)\n    print(v)\n  end fn act\n",
    ));
    no_fix(
        &main_body("match check(1)\n  case ok\n    print(1)\n  case error e\n    print(e)\nend"),
        "MZ0917",
    );
}

#[test]
fn a_function_whose_match_returns_on_both_cases_returns() {
    clean(&in_fn(
        "int",
        "match check(n)\n  case ok v\n    return v\n  case error e\n    return 0\nend",
    ));
    // A line after it can never run (`MZ0907`).
    fixed_by(
        &in_fn(
            "int",
            "match check(n)\n  case ok v\n    return v\n  case error e\n    return 0\nend\nprint(1)",
        ),
        "MZ0907",
        "    end\n  end fn f",
    );
}

#[test]
fn a_match_on_a_result_is_the_one_match() {
    // C4's `match` over an enum and this one over a result are one statement (RFC-0013
    // §7.2, §12.2): an enum's `match` checks clean beside a result's.
    clean(&main_body(
        "match negative\n  case negative\n    print(1)\n  case too_big\n    print(2)\nend\nmatch check(1)\n  case ok v\n    print(v)\n  case error e\n    print(e)\nend",
    ));
    // A case that binds a name belongs to a result's `match` only.
    let d = no_fix(
        &main_body("match negative\n  case ok v\n    print(1)\n  else\n    print(2)\nend"),
        "MZ0917",
    );
    assert!(d.say.contains("only a `match` on a result"), "{d:#?}");
    // With an enum whose variants are `ok` and `error`, `case ok error` lists two
    // variants: no binding takes a variant's name (`MZ0921`).
    clean(
        "program t\n\n  enum status\n    ok\n    error\n    unknown\n  end\n\n  fn main\n    let s = unknown\n    match s\n      case ok error\n        print(1)\n      case unknown\n        print(2)\n    end\n  end fn main\n\nend program t\n",
    );
}

// ------------------------------------------------------------------- enums and names

#[test]
fn a_binding_never_takes_an_enums_or_a_variants_name() {
    // A variant's name (§1, `MZ0921`), even one that two enums declare.
    no_fix(&main_body("let negative = 1\nprint(negative)"), "MZ0921");
    let two = "program t\n\n  enum a\n    red\n  end\n\n  enum b\n    red\n  end\n\n  fn main\n    let red = 1\n    print(red)\n  end fn main\n\nend program t\n";
    no_fix(two, "MZ0921");
    // An enum's name, so `problem.say` after the block can only mean the enum's variant.
    let d = no_fix(
        &main_body(
            "match check(1)\n  case ok v\n    print(v)\n  case error problem\n    print(problem)\nend",
        ),
        "MZ0921",
    );
    assert!(d.say.contains("is an enum of this program"), "{d:#?}");
    no_fix(
        &with(
            "  fn main\n    print(\"t\")\n  end fn main\n\n  fn g(problem: int): int\n    return problem\n  end fn g\n",
        ),
        "MZ0921",
    );
    // An enum that takes a built-in type's name is one `MZ0921`; `int` still means `int`.
    let src = "program t\n\n  enum int\n    one\n  end\n\n  fn main\n    let x: int = 5\n    print(x)\n  end fn main\n\nend program t\n";
    no_fix(src, "MZ0921");
}

#[test]
fn a_dot_on_a_result_is_mz0950_with_the_try_parenthesised() {
    let d = guess(
        &in_fn("result(text, problem)", "return check(n).say"),
        "MZ0950",
        "(try check(n)).say",
    );
    assert!(
        d.say.contains("reads `.say` off a result(int, problem)"),
        "{d:#?}"
    );
    // The guess, applied, checks only if `check`'s success had a `say` column, so the
    // fix is a guess; with an enum success it does.
    let src = with(
        "  fn main\n    print(\"t\")\n  end fn main\n\n  fn worst(n: int): result(problem, problem)\n    return too_big\n  end fn worst\n\n  fn label(n: int): result(text, problem)\n    return worst(n).say\n  end fn label\n",
    );
    let d = guess(&src, "MZ0950", "(try worst(n)).say");
    let fix = d.fix.expect("a fix");
    let fixed = src.replacen("worst(n).say", &fix.replace, 1);
    clean(&fixed);
}

#[test]
fn enums_and_their_names() {
    // A variant in two enums, where nothing expected settles which, is named with its enum
    // (C4's rule, RFC-0013 §18.6).
    let src = "program t\n\n  enum a\n    x\n  end\n\n  enum b\n    x\n  end\n\n  fn main\n    print(x)\n    print(a.x)\n  end fn main\n\nend program t\n";
    one(src, "MZ0708");
    // Every variant has every column (RFC-0001 §1.3).
    one(
        "program t\n\n  enum e\n    a say \"x\"\n    b\n  end\n\n  fn main\n    print(a)\n  end fn main\n\nend program t\n",
        "MZ0303",
    );
    // An unknown variant through its enum gets the nearest name.
    let d = one(&main_body("print(problem.negativ)"), "MZ0708");
    assert_eq!(d.fix.as_ref().map(|f| f.replace.as_str()), Some("negative"));
    // A column that does not exist.
    one(&main_body("print(negative.color)"), "MZ0708");
    // `enum result` would stand where the type `result` does.
    one(
        "program t\n\n  enum result\n    a\n  end\n\n  fn main\n    print(a)\n  end fn main\n\nend program t\n",
        "MZ0921",
    );
}

// ------------------------------------------------------------------- the lowering

fn lowered(src: &str) -> String {
    let p = program(src);
    assert_eq!(errors(src), Vec::new());
    let pkg = mzizi_lang_compiler::run::lower(&p, "t.mz");
    pkg.files
        .iter()
        .find(|(n, _)| n == "src/main.rs")
        .expect("main.rs")
        .1
        .clone()
}

#[test]
fn results_lower_to_rust_result_and_question_mark_with_no_unwrap() {
    let main = lowered(&example("errors.mz"));
    for absent in ["unwrap", "expect(", "panic!", "unsafe", "[0]"] {
        assert!(!main.contains(absent), "`{absent}` in:\n{main}");
    }
    for present in [
        "#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]\nenum AgeProblem {",
        "            AgeProblem::TooOld => String::from(\"is above 150\"),",
        "            AgeProblem::TooOld => String::from(\"too_old\"),",
        "fn check_age(n: i64) -> Result<i64, AgeProblem> {",
        "        return Err(AgeProblem::Negative);",
        "    return Ok(n);",
        "    return Ok(mz_add(check_age(a)?, check_age(b)?, &MZ_AT_1));",
        "    match check_age(n) {\n        Ok(age) => {",
        "        Err(problem) => {",
        "fn mz_main_result() -> Result<(), AgeProblem> {",
        "    let total: i64 = total_age(20i64, 22i64)?;",
        "    Ok(())\n}",
        "    if let Err(e) = mz_main_result() {",
        "MZ0992",
        "::std::process::exit(1)",
    ] {
        assert!(main.contains(present), "`{present}` not in:\n{main}");
    }
}

#[test]
fn a_passed_through_result_is_returned_as_it_is() {
    let main = lowered(&in_fn("result(int, problem)", "let r = check(n)\nreturn r"));
    assert!(main.contains("    return r.clone();"), "{main}");
    assert!(!main.contains("Ok(r.clone"), "{main}");
}

#[test]
fn distinct_names_stay_distinct_in_rust() {
    // `x_1` and `x1`, `a_` and `a`: dropping every `_` would give each pair one Rust name.
    let src = "program t\n\n  enum level\n    x_1\n    x1\n    a_\n    a\n  end\n\n  enum m_\n    one\n  end\n\n  enum m\n    one\n  end\n\n  fn main\n    print(level.x_1)\n    print(m_.one)\n  end fn main\n\nend program t\n";
    let main = lowered(src);
    // C4's naming (RFC-0013 §18.6): a numbered `MzUser` name where two would be one.
    let level: Vec<&str> = main
        .split("enum Level {\n")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or("")
        .lines()
        .collect();
    let mut unique = level.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(level.len(), 4, "{main}");
    assert_eq!(unique.len(), 4, "{main}");
    let enums = main.lines().filter(|l| l.starts_with("enum ")).count();
    assert_eq!(enums, 3, "{main}");
    let path = scratch("names.mz", src);
    if let Some(out) = mz_run(&["run", path.to_str().expect("utf-8 path")]) {
        assert_eq!(String::from_utf8_lossy(&out.stdout), "x_1\none\n");
        assert_eq!(out.status.code(), Some(0));
    }
}

// ------------------------------------------------------------------- mz run

fn mz_run(args: &[&str]) -> Option<std::process::Output> {
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return None;
    }
    Some(
        Command::new(env!("CARGO_BIN_EXE_mz"))
            .args(args)
            .output()
            .expect("mz runs"),
    )
}

fn scratch(name: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mz-program-errors-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(name);
    std::fs::write(&path, src).expect("write");
    path
}

#[test]
fn mz_run_errors_handles_and_propagates_exactly() {
    let path = root().join("examples/errors.mz");
    let Some(out) = mz_run(&["run", path.to_str().expect("utf-8 path")]) else {
        return;
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout,
        "age 42 is fine\n-3 is rejected: negative is below zero\n200 is rejected: too_old is above 150\n20 and 22 make 42\n20 and 151 were rejected: too_old is above 150\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout, example("errors.expected"));
}

#[test]
fn mz_run_a_main_that_returns_an_error_exits_1_with_mz0992() {
    let src = "program fails\n\n  enum problem\n    negative\n  end\n\n  fn main: result(none, problem)\n    print(\"before\")\n    let v = try check(-1)\n    print(\"never {v}\")\n  end fn main\n\n  fn check(n: int): result(int, problem)\n    when n < 0\n      return error(negative)\n    end\n    return n\n  end fn check\n\nend program fails\n";
    let path = scratch("fails.mz", src);
    let Some(out) = mz_run(&["run", path.to_str().expect("utf-8 path")]) else {
        return;
    };
    assert_eq!(String::from_utf8_lossy(&out.stdout), "before\n");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "mz: error MZ0992: main returned an error: negative\n"
    );
    assert_eq!(out.status.code(), Some(1));
}

// ------------------------------------------------------------------- review of PR #87

/// A program with `check` and its `problem` enum, plus `worst`, which fails or succeeds
/// with a `problem`, so a dotted read after `try` has a column to read.
fn with_worst(more: &str) -> String {
    with(&format!(
        "  fn main\n    print(\"t\")\n  end fn main\n\n  fn worst(n: int): result(problem, problem)\n    return too_big\n  end fn worst\n\n{more}"
    ))
}

#[test]
fn a_postfix_question_mark_before_a_dot_is_fixed_with_parentheses() {
    // `worst(n)?.say` reads `.say` off the success value. Without the parentheses the fix
    // would read it off the result, a different program and a new error after `mz fix`.
    fixed_by(
        &with_worst(
            "  fn label(n: int): result(text, problem)\n    return worst(n)?.say\n  end fn label\n",
        ),
        "MZ0952",
        "return (try worst(n)).say\n",
    );
}

#[test]
fn a_var_that_holds_a_result_is_one_diagnostic() {
    // Not also `MZ0924` (never assigned), whose fix would be the same `let`.
    let src = main_body("var r = check(1)");
    let all = check(&src, "t.mz").diagnostics;
    assert_eq!(all.len(), 1, "{all:#?}");
    assert_eq!(all[0].code, "MZ0950");
}

#[test]
fn an_unknown_return_type_is_one_diagnostic() {
    // Not also `MZ0906`, with an `exact` fix inserting `return` before a `print`.
    no_fix(&in_fn("foo", "print(\"x\")"), "MZ0701");
    no_fix(
        &with("  fn main: result(none, none)\n    print(\"x\")\n  end fn main\n"),
        "MZ0701",
    );
}

#[test]
fn throw_of_something_mzizi_cannot_read_is_one_diagnostic() {
    // `throw new Error("bad")` is one `MZ0952`, with no fix to build from `new`.
    no_fix(
        &in_fn("result(int, problem)", "throw new Error(\"bad\")"),
        "MZ0952",
    );
    // A thrown value that already failed to parse adds nothing, and no fix writes `…`.
    no_fix(
        &in_fn("result(int, problem)", "throw check(n).unwrap()"),
        "MZ0952",
    );
}

#[test]
fn throw_error_of_a_value_is_not_wrapped_twice() {
    // The lexer reads `Error` as `error`, so the thrown value is already `error("bad")`.
    fixed_by(
        &in_fn("result(int, text)", "throw Error(\"bad\")"),
        "MZ0952",
        "return error(\"bad\")\n",
    );
}

#[test]
fn a_constructor_idiom_around_an_unread_value_has_no_placeholder_fix() {
    let src = in_fn("result(int, problem)", "return Ok(check(n).unwrap())");
    let d = no_fix(&src, "MZ0952");
    assert!(d.say.contains("unwrap"), "{d:#?}");
    let report = check(&src, "t.mz");
    assert!(!apply_exact_fixes(&src, &report).contains('…'));
}

#[test]
fn a_tail_value_in_a_result_function_is_mz0906_alone() {
    // A result of the function's own type: passed through by `return`, one diagnostic.
    fixed_by(
        &in_fn("result(int, problem)", "check(n)"),
        "MZ0906",
        "return check(n)\n",
    );
    // The success type: `return` wraps it, so the same `exact` fix applies.
    fixed_by(
        &in_fn("result(int, problem)", "n + 1"),
        "MZ0906",
        "return n + 1\n",
    );
}

#[test]
fn a_try_that_cannot_propagate_says_nothing_more() {
    // Not also `MZ0908` for returning the `try`'s int from a function returning nothing.
    no_fix(
        &with(
            "  fn main\n    print(\"t\")\n  end fn main\n\n  fn b(n: int)\n    return try check(n)\n  end fn b\n",
        ),
        "MZ0951",
    );
}

#[test]
fn match_arms_are_reported_on_their_case_line() {
    let d = one(
        &main_body(
            "match check(1)\n  case ok v\n    print(v)\n  case error e\n    print(e)\n  case ok w\n    print(w)\n    print(w)\nend",
        ),
        "MZ0931",
    );
    assert_eq!(d.span.start_line, d.span.end_line, "{d:#?}");
    // An `else` in a match on a result says it takes none; it does not say that the
    // two cases cover every result, which `MZ0930` on the same match denies.
    let all = errors(&main_body(
        "match check(1)\n  case ok v\n    print(v)\n  else\n    print(1)\nend",
    ));
    assert!(
        all.iter()
            .any(|d| d.code == "MZ0931" && d.say.contains("takes no `else`")),
        "{all:#?}"
    );
    assert!(
        all.iter().all(|d| !d.say.contains("cover every result")),
        "{all:#?}"
    );
}

#[test]
fn dotted_reads_and_methods_after_the_numbers_wave() {
    // On a number, `x.abs` is still the numbers wave's method without `()`.
    fixed_by(&main_body("let x = -3\nprint(x.abs)"), "MZ0962", "x.abs()");
    // An enum has columns, read without parentheses, and no methods.
    let d = no_fix(&main_body("print(negative.say())"), "MZ0708");
    assert!(d.say.contains("without parentheses"), "{d:#?}");
    // A method on a result: `MZ0950`, with the `try` parenthesised as for a column.
    guess(
        &in_fn("result(int, problem)", "return check(n).abs()"),
        "MZ0950",
        "(try check(n)).abs()",
    );
    clean(&in_fn(
        "result(int, problem)",
        "return (try check(n)).abs()",
    ));
}

#[test]
fn results_work_in_every_form_c4_built() {
    // A `match` on a result used as a value whose branches read the names its cases bind,
    // `try` in a `for each`, a result's `match` in a `while true` that `break`s, columns
    // read in a C4 `match` used as a value, and `main` failing at the end.
    let src = "program k1\n\n  enum problem\n    odd  say \"is odd\"  weight 1\n    big  say \"too big\" weight 5\n  end\n\n  fn half(n: int): result(int, problem)\n    when n > 100\n      return error(big)\n    end\n    when n % 2 is 1\n      return error(odd)\n    end\n    return n / 2\n  end fn half\n\n  fn label(n: int): text\n    let t = match half(n)\n      case ok v\n        \"half {v}\"\n      case error e\n        \"{e}: {e.say}\"\n    end\n    return t\n  end fn label\n\n  fn sum_halves(limit: int): result(int, problem)\n    var total = 0\n    for each i in range(0, to = limit)\n      total = total + try half(i * 2)\n    end\n    return total\n  end fn sum_halves\n\n  fn first_odd(from: int): int\n    var n = from\n    while true\n      match half(n)\n        case ok v\n          n = n + 1\n        case error e\n          when e is odd\n            break\n          end\n          return -1\n      end\n    end\n    return n\n  end fn first_odd\n\n  fn weigh(p: problem): int\n    return match p\n      case odd\n        p.weight\n      case big\n        p.weight * 10\n    end\n  end fn weigh\n\n  fn main: result(none, problem)\n    print(label(8))\n    print(label(7))\n    print(label(400))\n    print(try sum_halves(5))\n    print(first_odd(4))\n    print(first_odd(200))\n    print(weigh(big))\n    print(weigh(odd))\n    let r = half(9)\n    let w = match r\n      case ok v\n        v\n      case error e\n        e.weight\n    end\n    print(w)\n    return error(problem.big)\n  end fn main\n\nend program k1\n";
    let path = scratch("forms.mz", src);
    let Some(out) = mz_run(&["run", path.to_str().expect("utf-8 path")]) else {
        return;
    };
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "half 4\nodd: is odd\nbig: too big\n10\n5\n-1\n50\n1\n1\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "mz: error MZ0992: main returned an error: big\n"
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn a_malformed_result_case_is_one_diagnostic() {
    // One `MZ0917` for the line, and the names it lists say nothing more in its arm.
    for case in ["case ok v w", "case okay v", "case 1"] {
        let src = main_body(&format!(
            "match check(1)\n  {case}\n    print(1)\n  case error e\n    print(e)\nend"
        ));
        let d = no_fix(&src, "MZ0917");
        assert!(d.say.contains("this case reads"), "{d:#?}");
    }
    let d = no_fix(
        &main_body(
            "match check(1)\n  case ok v w\n    print(v)\n  case error e\n    print(e)\nend",
        ),
        "MZ0917",
    );
    assert_eq!(d.span.start_line, d.span.end_line, "{d:#?}");
    // A second `case ok`: `MZ0931`, whose exact fix deletes it.
    fixed_by(
        &main_body(
            "match check(1)\n  case ok v\n    print(v)\n  case error e\n    print(e)\n  case ok w\n    print(w)\nend",
        ),
        "MZ0931",
        "        print(e)\n    end\n",
    );
}

// ------------------------------------------------------------------- review of the merge

#[test]
fn an_unread_result_in_a_for_each_is_mz0950() {
    // `main` returns nothing, so there is no `try` to offer.
    no_fix(
        &main_body("for each i in range(0, to = 3)\n  let r = check(i)\nend"),
        "MZ0950",
    );
    // Not while lines of the function were skipped unread: they may read it, as they may
    // assign a `var` (`MZ0924` waits the same way).
    let src = main_body("let r = check(1)\ntry:\n    print(r)\nexcept:\n    print(2)");
    let all = errors(&src);
    let codes: Vec<&str> = all.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["MZ0952"], "{all:#?}");
}

#[test]
fn a_returned_bare_variant_is_read_against_the_success_enum() {
    clean(
        "program t\n\n  enum color\n    red\n    blue\n  end\n\n  enum mood\n    red\n    calm\n  end\n\n  fn main\n    match pick(1)\n      case ok c\n        print(c)\n      case error e\n        print(e)\n    end\n  end fn main\n\n  fn pick(n: int): result(color, text)\n    when n > 1\n      return error(\"no\")\n    end\n    return red\n  end fn pick\n\nend program t\n",
    );
}

#[test]
fn a_result_parameter_is_one_diagnostic_at_the_parameter() {
    let src = with(
        "  fn main\n    print(g(check(1)))\n    print(g(3))\n  end fn main\n\n  fn g(r: result(int, problem)): int\n    return 1\n  end fn g\n",
    );
    no_fix(&src, "MZ0950");
}

#[test]
fn a_try_column_fix_is_never_built_over_a_placeholder() {
    let src = in_fn(
        "result(text, problem)",
        "let s = try check(1 +).say\nreturn s",
    );
    let report = check(&src, "t.mz");
    assert!(
        report.diagnostics.iter().all(|d| d.code != "MZ0950"),
        "{:#?}",
        report.diagnostics
    );
    assert!(!apply_exact_fixes(&src, &report).contains('…'));
}

#[test]
fn a_bare_ok_or_error_variant_is_qualified_wherever_it_resolves() {
    let status = "  enum status\n    ok\n    error\n  end\n\n";
    // An annotated binding, and `error(…)`'s argument, read a bare variant against their
    // type; in a result function each is `MZ0953`, with the exact fix naming its enum.
    let src = format!(
        "program t\n\n{status}  fn main\n    print(\"t\")\n  end fn main\n\n  fn f(n: int): result(int, text)\n    let s: status = ok\n    print(s)\n    return n\n  end fn f\n\nend program t\n"
    );
    fixed_by(&src, "MZ0953", "let s: status = status.ok\n");
    let src = format!(
        "program t\n\n{status}  fn main\n    print(\"t\")\n  end fn main\n\n  fn h(n: int): result(int, status)\n    return error(error)\n  end fn h\n\nend program t\n"
    );
    fixed_by(&src, "MZ0953", "return error(status.error)\n");
}

#[test]
fn a_column_missing_from_one_variant_blames_that_variant() {
    // The first variant forgets `rank`: it is the one reported, not every other.
    let src = "program t\n\n  enum e\n    a say \"x\"\n    b say \"y\" rank 2\n    c say \"z\" rank 3\n  end\n\n  fn main\n    print(a)\n  end fn main\n\nend program t\n";
    let d = one(src, "MZ0303");
    assert_eq!(d.span.start_line, 4, "{d:#?}");
    // A column without a literal is `MZ0302`, and its variant's line is not compared.
    let src = "program t\n\n  enum e\n    a say 1.5 rank 2\n    b say \"y\" rank 3\n  end\n\n  fn main\n    print(a)\n  end fn main\n\nend program t\n";
    one(src, "MZ0302");
}

#[test]
fn a_result_bound_with_another_annotation_is_mz0950() {
    guess(
        &in_fn("result(int, problem)", "let x: int = check(2)\nreturn x"),
        "MZ0950",
        "try ",
    );
}

#[test]
fn a_result_as_a_condition_offers_try_only_for_a_bool() {
    // `when try check(n)` would be an int: no fix.
    no_fix(
        &in_fn(
            "result(int, problem)",
            "when check(n)\n  return 1\nend\nreturn 2",
        ),
        "MZ0950",
    );
    let src = with(
        "  fn main\n    print(\"t\")\n  end fn main\n\n  fn ok_n(n: int): result(bool, problem)\n    return n > 0\n  end fn ok_n\n\n  fn f(n: int): result(int, problem)\n    when ok_n(n)\n      return 1\n    end\n    return 2\n  end fn f\n",
    );
    guess(&src, "MZ0950", "try ");
}
