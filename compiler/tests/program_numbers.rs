//! RFC-0013 §3 and §4 in a program, Wave 1's C1 and C5: `float`, the numeric methods of
//! §4.4, int/float mixing (`MZ0912`), malformed numbers (`MZ0914`), `and` mixed with `or`
//! (`MZ0913`), comments from other languages (`MZ0911`), Python's `**` and free functions,
//! constant faults (`MZ0915`), the lowering, and `mz run` end to end.
//!
//! As in `program.rs`: the parse; each diagnostic with its `exact` fix applied and the
//! result checked again; the lowered text; and `mz run` with exact standard output and
//! standard error. The `mz run` tests need `cargo` on the path; without it they say so on
//! standard error and pass, and CI's `lowering` job runs `examples/numbers.mz` through the
//! shipped binary regardless.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Severity};
use mzizi_lang_compiler::expr::{ExprKind, UnOp, canonical};
use mzizi_lang_compiler::lex::{Tok, lex};
use mzizi_lang_compiler::parse::{Program, parse_program};
use mzizi_lang_compiler::program::StmtKind;
use mzizi_lang_compiler::{apply_exact_fixes, check};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn program(src: &str) -> mzizi_lang_compiler::program::Program {
    match parse_program(src, "t.mz") {
        (Some(Program::Program(p)), _) => p,
        other => panic!("not a program: {other:?}"),
    }
}

/// `body` as the body of `fn main`, beside a float helper to call.
fn wrap(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!(
        "program t\n\n  fn main\n{body}  end fn main\n\n  fn half(x: float): float\n    return x / 2.0\n  end fn half\n\nend program t\n"
    )
}

/// Check `src`, expect exactly one error with `code`, and return it.
fn one(src: &str, code: &str) -> Diagnostic {
    let report = check(src, "t.mz");
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "one diagnostic per true error, for:\n{src}\ngot {:#?}",
        report.diagnostics
    );
    assert_eq!(errors[0].code, code, "{:#?}", errors[0]);
    errors[0].clone()
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

/// [`one`], and its fix is a `guess` that replaces with `want`.
fn guessed(src: &str, code: &str, want: &str) {
    let d = one(src, code);
    let fix = d.fix.as_ref().expect("a guess fix");
    assert_eq!(fix.confidence, Confidence::Guess, "{d:#?}");
    assert_eq!(fix.replace, want, "{d:#?}");
}

fn clean(src: &str) {
    let report = check(src, "t.mz");
    assert!(
        report.diagnostics.is_empty(),
        "{src}\n{:#?}",
        report.diagnostics
    );
}

/// The value of the `n`th statement of `fn main`, a `let`.
fn bound(src: &str, n: usize) -> mzizi_lang_compiler::expr::Expr {
    let p = program(src);
    match &p.fns[0].body[n].kind {
        StmtKind::Bind { value, .. } => value.clone(),
        other => panic!("{other:?}"),
    }
}

// ------------------------------------------------------------------- parse

#[test]
fn the_numbers_example_checks_clean() {
    let src = std::fs::read_to_string(root().join("examples/numbers.mz")).expect("example");
    clean(&src);
    let p = program(&src);
    let sigs: Vec<String> = p.fns.iter().map(|f| f.signature()).collect();
    assert_eq!(
        sigs,
        [
            "fn main",
            "fn pi: float",
            "fn mean(a: int, b: int): float",
            "fn hypotenuse(a: float, b: float): float"
        ]
    );
}

#[test]
fn the_lexer_reads_a_point_after_digits_as_rfc_0013_section_3_1_says() {
    let kinds = |src: &str| -> Vec<Tok> {
        let (toks, diags) = lex(&format!("program t\n{src}\n"), "t.mz");
        assert!(diags.is_empty(), "{src}: {diags:#?}");
        toks.into_iter()
            .map(|t| t.kind)
            .skip_while(|k| *k != Tok::Newline)
            .skip(1)
            .take_while(|k| *k != Tok::Newline)
            .collect()
    };
    assert_eq!(kinds("1.5"), [Tok::Float(1.5)]);
    assert_eq!(kinds("0.25"), [Tok::Float(0.25)]);
    // Digits, `.` and a letter: an int and a method call.
    assert_eq!(
        kinds("2.pow"),
        [Tok::Int(2), Tok::Dot, Tok::Ident("pow".into())]
    );
    assert_eq!(
        kinds("1.0.to_int"),
        [Tok::Float(1.0), Tok::Dot, Tok::Ident("to_int".into())]
    );
    // `**` is one token, so it can be reported as one idiom.
    assert_eq!(kinds("2 ** 3"), [Tok::Int(2), Tok::Op("**"), Tok::Int(3)]);
}

#[test]
fn methods_are_postfix_and_bind_tighter_than_prefix_minus() {
    let e = bound(&wrap("let x = -2.pow(2)\nprint(x)"), 0);
    assert_eq!(canonical(&e), "-2.pow(2)");
    let ExprKind::Unary {
        op: UnOp::Neg,
        operand,
    } = &e.kind
    else {
        panic!("{e:?}")
    };
    assert!(matches!(operand.kind, ExprKind::Method { .. }));
    let e = bound(&wrap("let x = (-2).pow(2)\nprint(x)"), 0);
    assert_eq!(canonical(&e), "(-2).pow(2)");
    let e = bound(&wrap("let x = (1.5 + 2.5).sqrt().round()\nprint(x)"), 0);
    assert_eq!(canonical(&e), "(1.5 + 2.5).sqrt().round()");
    // A float literal's canonical text never has an exponent (§17).
    let e = bound(
        &wrap("let x = 1000000000000000000000.0 * 0.00000015\nprint(x)"),
        0,
    );
    assert_eq!(canonical(&e), "1000000000000000000000.0 * 0.00000015");
}

#[test]
fn float_types_and_values_check() {
    clean(&wrap(
        "let a: float = 2.5\nvar b = half(a) * -1.0\nb = b % 3.0\nprint(\"{a} {b} {a < b} {a is b} {-a}\")\nprint(b)",
    ));
    clean(&wrap(
        "let n = 7\nlet f = n.to_float() / 2.0\nprint(f.to_int() + n.pow(2) + n.abs().min(3).max(1))",
    ));
    clean(&wrap(
        "let f = 2.0\nprint(f.pow(3).sqrt().floor().ceil().round().abs().min(1.0).max(0.5))\nprint(f.is_nan())",
    ));
}

// ------------------------------------------------------------------- diagnostics

#[test]
fn mz0912_int_with_float_converts_the_int_side() {
    // An `int` literal: the `exact` `1.0`.
    fixed_by(&wrap("print(1 + 1.5)"), "MZ0912", "print(1.0 + 1.5)");
    fixed_by(&wrap("print(2.5 * -2)"), "MZ0912", "print(2.5 * -2.0)");
    fixed_by(&wrap("print(2.5 < 3)"), "MZ0912", "print(2.5 < 3.0)");
    fixed_by(&wrap("print(2.5.min(1))"), "MZ0912", "print(2.5.min(1.0))");
    fixed_by(&wrap("print(half(1.0) is 0)"), "MZ0912", "half(1.0) is 0.0");
    // Anything else: the `guess` `x.to_float()`.
    guessed(&wrap("let n = 2\nprint(n * 1.5)"), "MZ0912", "n.to_float()");
    guessed(
        &wrap("let n = 2\nprint((n + 1) * 1.5)"),
        "MZ0912",
        "(n + 1).to_float()",
    );
    guessed(
        &wrap("let n = 2\nprint(n.min(1.5))"),
        "MZ0912",
        "n.to_float()",
    );
    // One diagnostic for a chain: the mixed operation fails, and the rest is silent.
    one(
        &wrap("let n = 2\nlet x = n * 1.5 + 2.0\nprint(x + 1.0)"),
        "MZ0912",
    );
    one(&wrap("print(-true)"), "MZ0912");
}

#[test]
fn mz0914_malformed_numbers() {
    fixed_by(&wrap("let x = 1.\nprint(x)"), "MZ0914", "let x = 1.0\n");
    fixed_by(
        &wrap("let x = 1. + 2.0\nprint(x)"),
        "MZ0914",
        "let x = 1.0 + 2.0",
    );
    fixed_by(&wrap("let x = .5\nprint(x)"), "MZ0914", "let x = 0.5");
    fixed_by(&wrap("print(1_000 + 1)"), "MZ0914", "print(1000 + 1)");
    fixed_by(&wrap("print(1_000.5)"), "MZ0914", "print(1000.5)");
    for bad in ["0x10", "0b101", "1e3", "1.5e-7", "2E10"] {
        let d = one(&wrap(&format!("print({bad})")), "MZ0914");
        assert!(d.fix.is_none(), "{bad}: {d:#?}");
        assert!(d.say.contains(bad), "{}", d.say);
    }
    // A float literal too large for a float has no value to repair to.
    let huge = format!("print({}.0)", "9".repeat(400));
    let d = one(&wrap(&huge), "MZ0914");
    assert!(d.fix.is_none());
    // RFC-0001: a `say` stays within 200 characters, however long the literal.
    assert!(d.say.chars().count() <= 200, "{}", d.say);
    assert!(d.say.contains("99999999999999999999…"), "{}", d.say);
}

#[test]
fn mz0913_and_mixed_with_or_gets_the_parentheses_the_precedence_implies() {
    fixed_by(
        &wrap("print(true and false or true)"),
        "MZ0913",
        "print((true and false) or true)",
    );
    fixed_by(
        &wrap("let a = 1\nprint(a is 1 or a > 2 and a < 5)"),
        "MZ0913",
        "print(a is 1 or (a > 2 and a < 5))",
    );
    // The idioms inside are folded into the one fix: one diagnostic, one `exact` fix.
    fixed_by(
        &wrap("let a = 1\nprint(a == 1 && a > 0 || false)"),
        "MZ0913",
        "print((a is 1 and a > 0) or false)",
    );
    // Written with parentheses, or `and` alone, or `or` alone: nothing to say.
    clean(&wrap("print((true and false) or true)"));
    clean(&wrap("print(true or (false and true))"));
    clean(&wrap("print(true and false and true)"));
    clean(&wrap("print(true or false or true)"));
    // In an interpolation too.
    fixed_by(
        &wrap("print(\"{true and false or true}\")"),
        "MZ0913",
        "print(\"{(true and false) or true}\")",
    );
}

#[test]
fn mz0911_comments_from_other_languages() {
    // `/* … */` on a line of its own: `exact` `##`.
    fixed_by(&wrap("/* a note */\nprint(1)"), "MZ0911", "    ## a note\n");
    // After code on the same line: a `guess` that moves the comment above.
    guessed(
        &wrap("let x = 7 // the answer\nprint(x)"),
        "MZ0911",
        "    ## the answer\n    let x = 7",
    );
    guessed(
        &wrap("let x = 7 # the answer\nprint(x)"),
        "MZ0911",
        "    ## the answer\n    let x = 7",
    );
    guessed(
        &wrap("let x = 7 /* the answer */\nprint(x)"),
        "MZ0911",
        "    ## the answer\n    let x = 7",
    );
    // An unclosed `/*` has no fix: the next line is not a comment.
    assert!(one(&wrap("/* a note\nprint(1)"), "MZ0911").fix.is_none());
}

#[test]
fn mz0910_pythons_power_operator_reads_as_pow() {
    guessed(&wrap("print(2 ** 10)"), "MZ0910", "2.pow(10)");
    guessed(&wrap("print(-2 ** 2)"), "MZ0910", "2.pow(2)");
    guessed(&wrap("print(2.0 ** 3 ** 2)"), "MZ0910", "2.0.pow(3.pow(2))");
    // Repaired in the tree, so the line type-checks as `pow`: the float exponent is MZ0905.
    let report = check(&wrap("print(2.0 ** 0.5)"), "t.mz");
    let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["MZ0910", "MZ0905"], "{:#?}", report.diagnostics);
}

#[test]
fn mz0915_constant_faults_in_numeric_methods() {
    one(&wrap("print(2.pow(-1))"), "MZ0915");
    one(&wrap("let n = 2\nprint(n.pow(-1))"), "MZ0915");
    one(&wrap("print(2.pow(64))"), "MZ0915");
    one(&wrap("print((-9223372036854775807 - 1).abs())"), "MZ0915");
    // A float never traps, so a float divided by a literal zero is not a fault (§4.2).
    clean(&wrap("print(1.0 / 0.0)\nprint(2.0.pow(-1))"));
}

#[test]
fn mz0962_free_functions_and_bare_methods_from_other_languages() {
    fixed_by(&wrap("print(float(3))"), "MZ0962", "print(3.to_float())");
    fixed_by(&wrap("print(int(2.5))"), "MZ0962", "print(2.5.to_int())");
    fixed_by(&wrap("print(abs(-3))"), "MZ0962", "print((-3).abs())");
    // A fix that changes behaviour is a `guess` (RFC-0013 §16): Python's `round(2.5)` is
    // `2`, Mzizi's `2.5.round()` is `3.0`; Python's `pow(2, -1)` is `0.5`, Mzizi's traps.
    guessed(&wrap("print(round(2.5))"), "MZ0962", "2.5.round()");
    guessed(&wrap("let n = 1\nprint(pow(2, n))"), "MZ0962", "2.pow(n)");
    fixed_by(&wrap("print(max(2, 3))"), "MZ0962", "print(2.max(3))");
    fixed_by(&wrap("print(pow(2.0, 3))"), "MZ0962", "print(2.0.pow(3))");
    fixed_by(&wrap("print(float(2.5))"), "MZ0962", "print(2.5)");
    fixed_by(
        &wrap("let f = 2.5\nprint(f.round)"),
        "MZ0962",
        "print(f.round())",
    );
    fixed_by(
        &wrap("let f = 2.5\nprint(f.to_string())"),
        "MZ0962",
        "print(\"{f}\")",
    );
    let d = one(&wrap("print(sqrt(true))"), "MZ0962");
    assert!(d.fix.is_none());
    // `str(x)`: a value's text is interpolation (RFC-0013 §3.6).
    fixed_by(
        &wrap("let n = 3\nprint(str(n + 1))"),
        "MZ0962",
        "print(\"{n + 1}\")",
    );
    fixed_by(&wrap("print(str(2.5))"), "MZ0962", "print(\"{2.5}\")");
    fixed_by(&wrap("print(str(\"x\"))"), "MZ0962", "print(\"x\")");
    // An interpolation holds no string literal, so there is nothing exact to write.
    let d = one(&wrap("print(str(\"x\" is \"y\"))"), "MZ0962");
    assert!(d.fix.is_none(), "{d:#?}");
}

/// Cases a review of this change found: each was a wrong fix or a wrong reading.
#[test]
fn review_cases_a_range_a_square_root_an_inline_comment_and_a_hoisted_float() {
    // `0..10` is not `0.` and `.10`: no `exact` MZ0914 that would write `0.00.10`.
    let report = check(&wrap("let x = 0..10\nprint(x)"), "t.mz");
    assert!(
        report.diagnostics.iter().all(|d| d.code != "MZ0914"),
        "{:#?}",
        report.diagnostics
    );
    // Python's `x ** 0.5` is a square root, never `x.pow(0)`.
    let report = check(&wrap("let x = 2.0\nprint(x ** 0.5)"), "t.mz");
    let fixes: Vec<(&str, String)> = report
        .diagnostics
        .iter()
        .filter_map(|d| d.fix.as_ref().map(|f| (d.code, f.replace.clone())))
        .collect();
    assert_eq!(
        fixes,
        [
            ("MZ0910", "x.pow(0.5)".to_string()),
            ("MZ0905", "x.sqrt()".to_string())
        ]
    );
    // A computed float exponent has no `int` that means the same: no fix.
    let report = check(&wrap("let x = 2.0\nlet e = 0.25\nprint(x.pow(e))"), "t.mz");
    assert_eq!(report.diagnostics.len(), 1, "{:#?}", report.diagnostics);
    assert!(report.diagnostics[0].fix.is_none());
    // A `/* … */` with code after it: the code is still read, so `x` is bound.
    guessed(
        &wrap("/* temp */ let x = 1\nprint(x)"),
        "MZ0911",
        "    ## temp\n    let x = 1",
    );
    guessed(
        &wrap("let x = 1 /* one */ + 2\nprint(x)"),
        "MZ0911",
        "    ## one\n    let x = 1 + 2",
    );
    // `.to_string()` on any value with a text form, and on text it is the value itself.
    fixed_by(
        &wrap("let flag = true\nprint(flag.to_string())"),
        "MZ0962",
        "print(\"{flag}\")",
    );
    fixed_by(
        &wrap("let s = \"a\"\nprint(s.to_string())"),
        "MZ0962",
        "print(s)",
    );
    // A binding hoisted out of its block by MZ0920's fix gets its checked type's zero.
    let d = one(
        &wrap("let a = 1.5\nwhen a > 1.0\n  let y = a * a\nend\nprint(y)"),
        "MZ0920",
    );
    assert_eq!(d.fix.expect("a guess fix").replace, "var y = 0.0\n    ");
}

/// A receiver that folds to a negative constant keeps its parentheses in the lowered Rust,
/// where `-4i64.min(n)` would read `-(4i64.min(n))`.
#[test]
fn mz_run_a_negative_receiver_is_not_negated_after_the_call() {
    let main = main_rs(&wrap(
        "let n = 3\nprint((-4).min(n))\nprint((0 - 4).max(n))",
    ));
    assert!(main.contains("(-4i64).min(n)"), "{main}");
    let Some((out, err, code)) = run_program(
        "negative.mz",
        &wrap("let n = 3\nprint((-4).min(n))\nprint((0 - 4).max(-n))\nprint((-2).pow(3))"),
    ) else {
        return;
    };
    assert_eq!(out, "-4\n-3\n-8\n", "stderr: {err}");
    assert_eq!(code, Some(0));
}

#[test]
fn mz0910_not_a_is_b_is_a_is_not_b() {
    fixed_by(
        &wrap("let a = 1\nprint(not a is 1)"),
        "MZ0910",
        "print(a is not 1)",
    );
    fixed_by(
        &wrap("let a = 1.5\nprint(not a is not 1.5)"),
        "MZ0910",
        "print(a is 1.5)",
    );
    // `==` inside it is folded into the one fix.
    fixed_by(
        &wrap("let a = 1\nprint(not a == 1)"),
        "MZ0910",
        "print(a is not 1)",
    );
    // And inside an `and` / `or` mix, into `MZ0913`'s.
    fixed_by(
        &wrap("let a = 1\nprint(not a is 1 and true or false)"),
        "MZ0913",
        "print((a is not 1 and true) or false)",
    );
    // Written with parentheses, the author said which they meant.
    clean(&wrap("let a = 1\nprint(not (a is 1))"));
    clean(&wrap("let a = 1\nprint(not a < 1)"));
}

#[test]
fn mz0708_a_method_a_number_does_not_have() {
    guessed(
        &wrap("let n = 4\nprint(n.sqrt())"),
        "MZ0708",
        "n.to_float().sqrt()",
    );
    guessed(&wrap("let f = 4.0\nprint(f.to_float())"), "MZ0708", "f");
    guessed(&wrap("let f = 4.0\nprint(f.sqr())"), "MZ0708", "sqrt");
    let d = one(&wrap("print(true.abs())"), "MZ0708");
    assert!(d.say.contains("no method `abs`"), "{}", d.say);
    // Methods on text are C6's (`tests/program_text.rs`); a number has none of them.
    clean(&wrap("print(\"abc\".length())"));
    let d = one(&wrap("print(3.length())"), "MZ0708");
    assert!(d.say.contains("no method `length`"), "{}", d.say);
}

#[test]
fn mz0905_method_arguments() {
    let d = one(&wrap("print(2.5.min())"), "MZ0905");
    assert!(d.say.contains("takes 1 argument (float)"), "{}", d.say);
    guessed(&wrap("print(2.5.pow(2.0))"), "MZ0905", "2");
    // A computed float exponent: no `int` means the same, so no fix (`e.to_int()` would
    // truncate `0.5` to `0`).
    assert!(
        one(&wrap("let e = 0.5\nprint(2.5.pow(e))"), "MZ0905")
            .fix
            .is_none()
    );
}

#[test]
fn float_conditions_types_and_the_bindings_they_repair() {
    guessed(
        &wrap("when 1.5\n  print(1)\nend"),
        "MZ0712",
        "1.5 is not 0.0",
    );
    guessed(
        "program t\n  fn main\n    print(f(1.0))\n  end fn main\n  fn f(x: f64): float\n    return x\n  end fn f\nend program t\n",
        "MZ0701",
        "float",
    );
    one(&wrap("let x: int = 1.5\nprint(x)"), "MZ0711");
    one(&wrap("var x = 1\nx = 1.5\nprint(x)"), "MZ0711");
}

// ------------------------------------------------------------------- lowering

// ------------------------------------------------------------- int's minimum

/// RFC-0013 §3.1: `-9223372036854775808` is `int`'s minimum, read under a unary `-` and
/// nowhere else. A postfix method on it, a binary minus, a second minus and a bare literal all
/// stay `MZ0103`.
#[test]
fn the_int_minimum_is_read_under_a_unary_minus_and_nowhere_else() {
    clean(&wrap(
        "let m: int = -9223372036854775808\nprint(m)\nprint(-9223372036854775808)\nprint(-9223372036854775808 + 1)\nprint(\"{-9223372036854775808}\")\nprint(-9223372036854775808 is -9223372036854775808)",
    ));
    for body in [
        "print(9223372036854775808)",
        "let x = 1\nprint(x -9223372036854775808)",
        "let x = 1\nprint(x - -9223372036854775808)",
        "print(-9223372036854775808.abs())",
        "print(- -9223372036854775808)",
        "print(\"{9223372036854775808}\")",
    ] {
        one(&wrap(body), "MZ0103");
    }
}

/// The minimum lowers to `i64::MIN`, a `match` on it is an arm of that path, and `mz run`
/// prints it, with the trap on the next `int` operation that overflows.
#[test]
fn the_int_minimum_lowers_to_i64_min_and_runs() {
    let src = "program t\n\n  fn main\n    let m: int = -9223372036854775808\n    print(m)\n    print(\"{-9223372036854775808}\")\n    print(-9223372036854775808 + 1)\n    print(kind(m))\n    print(kind(5))\n  end fn main\n\n  fn kind(n: int): text\n    match n\n      case -9223372036854775808\n        return \"min\"\n      else\n        return \"other\"\n    end\n  end fn kind\n\nend program t\n";
    let main = main_rs(src);
    assert!(main.contains("let m: i64 = i64::MIN;"), "{main}");
    assert!(main.contains("i64::MIN => {"), "{main}");
    assert!(!main.contains("9223372036854775808i64"), "{main}");
    let Some((stdout, stderr, status)) = run_program("int_minimum.mz", src) else {
        return;
    };
    assert_eq!(status, Some(0), "stderr: {stderr}");
    assert_eq!(
        stdout,
        "-9223372036854775808\n-9223372036854775808\n-9223372036854775807\nmin\nother\n"
    );
}

// ------------------------------------------------------------------- lowering

fn main_rs(src: &str) -> String {
    let p = program(src);
    mzizi_lang_compiler::run::lower(&p, "t.mz").files[1]
        .1
        .clone()
}

#[test]
fn the_lowering_of_floats_and_numeric_methods() {
    let main = main_rs(&wrap(
        "let n = 7\nlet f = n.to_float() / 2.0\nprint(f.to_int())\nprint(n.pow(2))\nprint(n.abs())\nprint(f.pow(n))\nprint(-f)\nprint(f.sqrt().min(1.0))",
    ));
    for want in [
        "let f: f64 = ((n as f64) / 2.0f64);",
        "mz_print(&mz_to_int(f, &MZ_AT_1));",
        "mz_print(&mz_pow(n, 2i64, &MZ_AT_2));",
        "mz_print(&mz_abs(n, &MZ_AT_3));",
        "mz_print(&f.powf(n as f64));",
        "mz_print(&(-f));",
        "mz_print(&f.sqrt().min(1.0f64));",
        "fn half(x: f64) -> f64 {",
        "return (x / 2.0f64);",
        "const MZ_AT_1: MzAt = MzAt { file: \"t.mz\", line: 6, col: 11, text: \"f.to_int()\" };",
        "pub fn mz_float_text(x: f64) -> String {",
        "impl MzText for f64 {",
    ] {
        assert!(main.contains(want), "`{want}` not in:\n{main}");
    }
    for banned in [
        ".unwrap()",
        ".expect(",
        "panic!",
        "unsafe",
        "println!",
        "powi(",
    ] {
        assert!(!main.contains(banned), "the lowering emitted `{banned}`");
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
    let dir = std::env::temp_dir().join(format!("mz-numbers-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(name);
    std::fs::write(&path, src).expect("write");
    path
}

fn run_program(name: &str, src: &str) -> Option<(String, String, Option<i32>)> {
    let path = scratch(name, src);
    let out = mz_run(&["run", path.to_str().expect("utf-8 path")])?;
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    ))
}

#[test]
fn mz_run_numbers_prints_its_expected_output() {
    let path = root().join("examples/numbers.mz");
    let Some(out) = mz_run(&["run", path.to_str().expect("utf-8 path")]) else {
        return;
    };
    let expected =
        std::fs::read_to_string(root().join("examples/numbers.expected")).expect("expected");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        expected,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(expected.contains("0.1 + 0.2 is 0.30000000000000004\n"));
    assert!(expected.contains("1.0 / 0.0 is inf, -1.0 / 0.0 is -inf, 0.0 / 0.0 is nan\n"));
}

/// C5's "Done when": float division by zero and `nan` follow IEEE 754 and do not trap;
/// §3.8's text form; truncation, rounding and the conversions of §4.4.
#[test]
fn mz_run_floats_follow_ieee_754_and_never_trap() {
    let src = wrap(
        "let zero = 0.0\nprint(1.0 / zero)\nprint(-1.0 / zero)\nlet nan = zero / zero\nprint(nan)\nprint(nan is nan)\nprint(nan is not nan)\nprint(nan.is_nan())\nprint(-zero)\nprint(0.1 + 0.2)\nprint(1.0)\nprint(100000000000000000000.0 * 10.0)\nprint(0.000001)\nprint(0.0000001)\nprint(5.0 % 3.0)\nprint(-5.0 % 3.0)\nprint(2.5.round())\nprint((-2.5).round())\nprint(-2.7.to_int())\nprint((-2.7).to_int())\nprint(9007199254740993.to_float())\nprint(2.0.pow(-1))\nprint((-1.0).sqrt())\nprint(nan.min(1.0))\nprint(\"{half(3.0)} and {1.0 / 3.0}\")",
    );
    let Some((out, err, code)) = run_program("ieee.mz", &src) else {
        return;
    };
    assert_eq!(
        out,
        "inf\n-inf\nnan\nfalse\ntrue\ntrue\n-0.0\n0.30000000000000004\n1.0\n1.0e21\n0.000001\n1.0e-7\n2.0\n-2.0\n3.0\n-3.0\n-2\n-2\n9007199254740992.0\n0.5\nnan\n1.0\n1.5 and 0.3333333333333333\n",
        "stderr: {err}"
    );
    assert_eq!(err, "");
    assert_eq!(code, Some(0));
}

/// C5's "Done when": overflow stops the program with `MZ0991` and exit status 101, and so
/// does every other numeric trap of RFC-0013 §4.3 that a float or a method reaches.
#[test]
fn mz_run_numeric_traps_exit_101_with_mz0991() {
    let cases = [
        (
            "let n = 9223372036854775807\nprint(\"before\")\nprint(n + id(1))",
            "integer overflow in `n + id(1)`",
            "6:11",
        ),
        (
            "print(\"before\")\nprint(id(0) / id(0))",
            "integer division by zero in `id(0) / id(0)`",
            "5:11",
        ),
        (
            "let zero = 0.0\nprint(\"before\")\nprint((zero / zero).to_int())",
            "float to int out of range in `(zero / zero).to_int()`",
            "6:11",
        ),
        (
            "print(\"before\")\nprint(10000000000000000000.0.to_int())",
            "float to int out of range in `10000000000000000000.0.to_int()`",
            "5:11",
        ),
        (
            "print(\"before\")\nprint(2.pow(id(-1)))",
            "negative exponent in `2.pow(id(-1))`",
            "5:11",
        ),
        (
            "print(\"before\")\nprint(3.pow(id(40)))",
            "integer overflow in `3.pow(id(40))`",
            "5:11",
        ),
        (
            "print(\"before\")\nprint((id(-9223372036854775807) - 1).abs())",
            "integer overflow in `(id(-9223372036854775807) - 1).abs()`",
            "5:11",
        ),
    ];
    for (k, (body, what, at)) in cases.iter().enumerate() {
        let src = wrap(body).replace(
            "\nend program t\n",
            "\n  fn id(n: int): int\n    return n\n  end fn id\n\nend program t\n",
        );
        let name = format!("trap{k}.mz");
        let Some((out, err, code)) = run_program(&name, &src) else {
            return;
        };
        assert_eq!(out, "before\n", "{src}\nstderr: {err}");
        assert_eq!(
            err,
            format!("mz: trap MZ0991 at {name}:{at}: {what}\n"),
            "{src}"
        );
        assert_eq!(code, Some(101), "{src}");
    }
}

// ------------------------------------------------------------------- robustness

/// A method chain or a `**` chain 100,000 long is one `MZ0411` (and, for `**`, the one
/// `MZ0910`), and checking and lowering it fits in a 1 MiB stack, as
/// `compiler/tests/robustness.rs` holds every other shape of program to.
#[test]
fn long_method_and_power_chains_are_capped() {
    let n = 100_000;
    let cases = [
        (format!("1{}", ".abs()".repeat(n)), vec!["MZ0411"]),
        (format!("2{}", " ** 2".repeat(n)), vec!["MZ0411", "MZ0910"]),
        (format!("1.5{}", ".sqrt()".repeat(n)), vec!["MZ0411"]),
    ];
    for (value, want) in cases {
        let src = format!(
            "program deep\n\n  fn main\n    let x = {value}\n    print(\"{{x}}\")\n  end fn main\n\nend program deep\n"
        );
        let worker = std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                let mut codes: Vec<&str> = check(&src, "case.mz")
                    .diagnostics
                    .iter()
                    .map(|d| d.code)
                    .collect();
                codes.sort();
                if let (Some(Program::Program(p)), _) = parse_program(&src, "case.mz") {
                    let _ = mzizi_lang_compiler::run::lower(&p, "case.mz");
                }
                codes
            })
            .expect("thread");
        assert_eq!(worker.join().expect("no stack overflow"), want);
    }
}

#[test]
fn mz0962_str_of_a_value_with_no_text_form_gets_no_fix() {
    // `"{f()}"` would be `MZ0711` again: a function that returns nothing has no text.
    let src = "program t\n\n  fn main\n    print(str(log_one()))\n  end fn main\n\n  fn log_one\n    print(1)\n  end fn log_one\n\nend program t\n";
    let d = one(src, "MZ0962");
    assert!(d.fix.is_none(), "{d:#?}");
}
