//! RFC-0013's foundation slice (§18.1): a `program`, `fn`s, bindings, integer and boolean
//! expressions, `print`, the lowering to Rust, and `mz run`.
//!
//! Four kinds of test, as the slice's "Done when" rows ask (LANGUAGE-TRACKER C2, C3, C10):
//! the parse; the check, one test per diagnostic with its `exact` fix applied and the
//! result checked again; the lowered text; and `mz run` end to end on `examples/hello.mz`
//! and `examples/fib.mz`, which builds the lowered package with `cargo`. The end-to-end
//! tests need `cargo` on the path; without it they say so on standard error and pass, and
//! CI's `lowering` job runs the same programs through the shipped binary regardless.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::diagnostic::{Confidence, Severity};
use mzizi_lang_compiler::expr::{ExprKind, canonical};
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

/// `body` as the body of `fn main`, beside two helpers to call.
fn wrap(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!(
        "program t\n\n  fn main\n{body}  end fn main\n\n  fn add(a: int, b: int): int\n    return a + b\n  end fn add\n\n  fn answer: int\n    return 42\n  end fn answer\n\nend program t\n"
    )
}

/// Check `src`, expect exactly one error with `code`, and return it.
fn one(src: &str, code: &str) -> mzizi_lang_compiler::diagnostic::Diagnostic {
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

// ------------------------------------------------------------------- parse

#[test]
fn the_examples_parse_and_check_clean() {
    for name in ["hello.mz", "fib.mz"] {
        let src = example(name);
        let report = check(&src, name);
        assert!(
            report.diagnostics.is_empty(),
            "{name}: {:#?}",
            report.diagnostics
        );
    }
    let fib = program(&example("fib.mz"));
    assert_eq!(fib.name, "fib");
    let names: Vec<&str> = fib.fns.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["main", "fib", "parity"]);
    assert_eq!(fib.fns[1].signature(), "fn fib(n: int): int");
    assert_eq!(fib.fns[2].signature(), "fn parity(n: int): text");
}

#[test]
fn precedence_follows_rfc_0013_section_3_5() {
    let p = program(&wrap(
        "let a = 1 + 2 * 3 - -4 % 5\nlet b = (not a < 3 and true) or false\nlet c = (1 + 2) * 3",
    ));
    let values: Vec<String> = p.fns[0]
        .body
        .iter()
        .map(|s| match &s.kind {
            StmtKind::Bind { value, .. } => canonical(value),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        values,
        [
            "1 + 2 * 3 - -4 % 5",
            "(not a < 3 and true) or false",
            "(1 + 2) * 3"
        ]
    );
    // `not` binds looser than `is`: `not (a is 3)`.
    let StmtKind::Bind { value, .. } = &p.fns[0].body[1].kind else {
        panic!()
    };
    let ExprKind::Binary { lhs, .. } = &value.kind else {
        panic!()
    };
    let ExprKind::Binary { lhs: not, .. } = &lhs.kind else {
        panic!()
    };
    assert!(matches!(not.kind, ExprKind::Unary { .. }));
}

#[test]
fn interpolation_holds_an_expression_and_escapes_decode() {
    let p = program(&wrap(
        r#"print("fib({1 + 1}) is {add(1, 2)} \{not\} \"q\"")"#,
    ));
    let StmtKind::Expr(e) = &p.fns[0].body[0].kind else {
        panic!()
    };
    assert_eq!(
        canonical(e),
        r#"print("fib({1 + 1}) is {add(1, 2)} \{not\} \"q\"")"#
    );
}

#[test]
fn a_component_still_reports_an_operator_as_a_character_mzizi_does_not_use() {
    // RFC-0013 §18.1: the lexer reads operators only in a program, so no existing
    // diagnostic changes.
    let src = "component a\n  prop n: int\n  view\n    text \"{n}\"\n  end\n  contract\n    example n 1 renders \"1\"\n  end\nend component a\nx + 1\n";
    let report = check(src, "a.mz");
    assert!(report.diagnostics.iter().any(|d| d.code == "MZ0104"));
}

// ------------------------------------------------------------------- check

#[test]
fn mz0901_statements_outside_a_fn_are_one_diagnostic_and_name_main() {
    let d = one(
        "program t\nprint(\"a\")\nprint(\"b\")\nend program t\n",
        "MZ0901",
    );
    assert!(d.say.contains("lines 2–3"), "{}", d.say);
    assert!(d.say.contains("fn main"));
    one(
        "program t\n  view\n  end\n  fn main\n  end fn main\nend program t\n",
        "MZ0901",
    );
    one("program\n", "MZ0901");
}

#[test]
fn mz0902_the_entry_point() {
    one(
        "program t\n  fn helper\n  end fn helper\nend program t\n",
        "MZ0902",
    );
    one(
        "program t\n  fn main\n  end fn main\n  fn main\n  end fn main\nend program t\n",
        "MZ0902",
    );
    one(
        "program t\n  fn main(n: int)\n    print(n)\n  end fn main\nend program t\n",
        "MZ0902",
    );
    one(
        "program t\n  fn main: int\n    return 1\n  end fn main\nend program t\n",
        "MZ0902",
    );
}

#[test]
fn mz0903_signature_idioms_have_exact_fixes() {
    let base = |sig: &str| {
        format!(
            "program t\n  fn main\n    print(f(1))\n  end fn main\n  {sig}\n    return n\n  end fn f\nend program t\n"
        )
    };
    fixed_by(&base("fn f(n: int) -> int"), "MZ0903", "fn f(n: int): int");
    fixed_by(&base("def f(n: int): int"), "MZ0903", "  fn f(n: int): int");
    fixed_by(
        "program t\n  fn main()\n  end fn main\nend program t\n",
        "MZ0903",
        "  fn main\n",
    );
    fixed_by(
        "program t\n  fn main: none\n  end fn main\nend program t\n",
        "MZ0903",
        "  fn main\n",
    );
    // No fix: the type, or the call sites, are the author's.
    let d = one(&base("fn f(n): int"), "MZ0903");
    assert!(d.fix.is_none());
    let d = one(&base("fn f(n: int = 3): int"), "MZ0903");
    assert!(d.fix.is_none());
}

#[test]
fn mz0904_names_declared_twice() {
    one(
        "program t\n  fn main\n  end fn main\n  fn f\n  end fn f\n  fn f\n  end fn f\nend program t\n",
        "MZ0904",
    );
    one(
        "program t\n  fn main\n  end fn main\n  fn f(a: int, a: int)\n  end fn f\nend program t\n",
        "MZ0904",
    );
    one(
        "program t\n  fn main\n  end fn main\n  fn print(a: int)\n  end fn print\nend program t\n",
        "MZ0904",
    );
}

#[test]
fn mz0905_calls_quote_the_signature() {
    let d = one(&wrap("print(add(1))"), "MZ0905");
    assert!(d.say.contains("fn add(a: int, b: int): int"), "{}", d.say);
    one(&wrap("print(add(1, true))"), "MZ0905");
    one(&wrap("print(add(a = 1, 2))"), "MZ0905");
    one(&wrap("print(main())"), "MZ0905");
}

#[test]
fn mz0906_a_path_without_return_and_the_rust_habit_fix() {
    let src = "program t\n  fn main\n    print(f(1))\n  end fn main\n  fn f(n: int): int\n    n + 1\n  end fn f\nend program t\n";
    fixed_by(src, "MZ0906", "    return n + 1\n");
    let d = one(
        "program t\n  fn main\n    print(f(1))\n  end fn main\n  fn f(n: int): int\n    when n > 0\n      return 1\n    end\n  end fn f\nend program t\n",
        "MZ0906",
    );
    assert!(d.fix.is_none());
    assert!(d.say.contains("end fn f"), "{}", d.say);
}

#[test]
fn mz0907_a_line_after_return_is_deleted() {
    let src = "program t\n  fn main\n    print(f(1))\n  end fn main\n  fn f(n: int): int\n    return n\n    print(n)\n    print(n)\n  end fn f\nend program t\n";
    fixed_by(src, "MZ0907", "    return n\n  end fn f");
}

#[test]
fn mz0908_return_must_fit_its_function() {
    one(&wrap("return 1"), "MZ0908");
    one(
        "program t\n  fn main\n    print(f(1))\n  end fn main\n  fn f(n: int): int\n    return true\n  end fn f\nend program t\n",
        "MZ0908",
    );
    one(
        "program t\n  fn main\n    print(f(1))\n  end fn main\n  fn f(n: int): int\n    return\n  end fn f\nend program t\n",
        "MZ0908",
    );
}

#[test]
fn mz0909_a_zero_parameter_fn_used_as_a_value_gets_its_call() {
    fixed_by(&wrap("print(answer)"), "MZ0909", "print(answer())");
    assert!(one(&wrap("print(add)"), "MZ0909").fix.is_none());
}

#[test]
fn mz0910_operators_from_other_languages() {
    fixed_by(&wrap("print(1 == 1)"), "MZ0910", "print(1 is 1)");
    fixed_by(&wrap("print(1 != 2)"), "MZ0910", "print(1 is not 2)");
    fixed_by(
        &wrap("print(true && false)"),
        "MZ0910",
        "print(true and false)",
    );
    fixed_by(
        &wrap("print(true || false)"),
        "MZ0910",
        "print(true or false)",
    );
    fixed_by(&wrap("print(!true)"), "MZ0910", "print(not true)");
    fixed_by(&wrap("print(2 at_least 1)"), "MZ0910", "print(2 >= 1)");
}

#[test]
fn mz0912_operand_types() {
    one(&wrap("print(1 + true)"), "MZ0912");
    one(&wrap("print(1 is true)"), "MZ0912");
    one(&wrap("print(true < false)"), "MZ0912");
    one(&wrap("print(not 1)"), "MZ0912");
    // `+` on text is the concatenation idiom: interpolation is the fix.
    fixed_by(
        &wrap("let n = 3\nprint(\"n is \" + n + \"!\")"),
        "MZ0912",
        "print(\"n is {n}!\")",
    );
}

#[test]
fn mz0913_comparisons_do_not_chain() {
    let d = one(&wrap("print(1 < 2 < 3)"), "MZ0913");
    let fix = d.fix.expect("a fix");
    assert_eq!(fix.confidence, Confidence::Guess);
    assert_eq!(fix.replace, "1 < 2 and 2 < 3");
}

#[test]
fn mz0915_faults_visible_in_constants() {
    one(&wrap("print(10 / 0)"), "MZ0915");
    one(&wrap("print(7 % (2 - 2))"), "MZ0915");
    one(&wrap("print(9223372036854775807 + 1)"), "MZ0915");
}

#[test]
fn mz0916_a_value_nothing_reads() {
    one(&wrap("add(1, 2)"), "MZ0916");
}

#[test]
fn mz0918_operator_assignment() {
    fixed_by(&wrap("var n = 1\nn += 2\nprint(n)"), "MZ0918", "n = n + 2");
    fixed_by(
        &wrap("var n = 1\nn *= 2 + 3\nprint(n)"),
        "MZ0918",
        "n = n * (2 + 3)",
    );
    fixed_by(&wrap("var n = 1\nn++\nprint(n)"), "MZ0918", "n = n + 1");
}

#[test]
fn mz0919_a_designed_form_not_built_yet_is_one_diagnostic() {
    one(
        &wrap("var n = 3\nwhile n > 0\n  n = n - 1\nend\nprint(n)"),
        "MZ0919",
    );
    one(&wrap("let s = \"abc\".length()"), "MZ0919");
    one(
        &wrap("let n = 1\nwhen n is 1\n  print(1)\nelse when n is 2\n  print(2)\nend"),
        "MZ0919",
    );
    one(
        "program t\n  fn main\n  end fn main\n  contract\n    example output is \"\"\n  end\nend program t\n",
        "MZ0919",
    );
}

#[test]
fn mz0920_use_before_binding_and_after_its_block() {
    let d = one(&wrap("print(n)\nlet n = 1"), "MZ0920");
    assert!(d.say.contains("line"), "{}", d.say);
    let d = one(
        &wrap("let x = 1\nwhen x is 1\n  let label = \"one\"\nend\nprint(label)"),
        "MZ0920",
    );
    let fix = d.fix.expect("a guess fix");
    assert_eq!(fix.confidence, Confidence::Guess);
    assert_eq!(fix.replace, "var label = \"\"\n    ");
}

#[test]
fn mz0921_no_shadowing_and_no_reserved_names() {
    one(
        &wrap("let n = 1\nwhen n is 1\n  let n = 2\n  print(n)\nend"),
        "MZ0921",
    );
    one(&wrap("let and = 1\nprint(and)"), "MZ0921");
    one(&wrap("let mz_x = 1\nprint(mz_x)"), "MZ0921");
    one(&wrap("let add = 1\nprint(add)"), "MZ0921");
    // Sibling blocks may reuse a name.
    let report = check(
        &wrap(
            "let n = 1\nwhen n is 1\n  let m = 2\n  print(m)\nelse\n  let m = 3\n  print(m)\nend",
        ),
        "t.mz",
    );
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
}

#[test]
fn mz0922_assignment_to_a_let_or_a_parameter() {
    fixed_by(&wrap("let n = 1\nn = 2\nprint(n)"), "MZ0922", "var n = 1");
    let d = one(
        "program t\n  fn main\n    print(f(1))\n  end fn main\n  fn f(n: int): int\n    n = 2\n    return n\n  end fn f\nend program t\n",
        "MZ0922",
    );
    assert!(d.fix.is_none());
}

#[test]
fn mz0923_a_first_assignment_gets_let_or_var() {
    fixed_by(&wrap("n = 1\nprint(n)"), "MZ0923", "let n = 1");
    fixed_by(&wrap("n = 1\nn = n + 1\nprint(n)"), "MZ0923", "var n = 1");
    fixed_by(&wrap("n: int = 1\nprint(n)"), "MZ0923", "let n: int = 1");
}

#[test]
fn mz0924_a_var_never_assigned_is_a_warning() {
    let src = wrap("var n = 1\nprint(n)");
    let report = check(&src, "t.mz");
    assert_eq!(report.error_count(), 0);
    let d = &report.diagnostics[0];
    assert_eq!((d.code, d.severity), ("MZ0924", Severity::Warning));
    let after = apply_exact_fixes(&src, &report);
    assert!(after.contains("let n = 1"));
    assert!(check(&after, "t.mz").diagnostics.is_empty());
}

#[test]
fn mz0925_binding_forms_from_other_languages() {
    fixed_by(&wrap("const n = 1\nprint(n)"), "MZ0925", "let n = 1");
    fixed_by(
        &wrap("let mut n = 1\nn = 2\nprint(n)"),
        "MZ0925",
        "var n = 1",
    );
    fixed_by(&wrap("n := 1\nprint(n)"), "MZ0925", "let n = 1");
}

#[test]
fn mz0926_a_binding_with_no_value() {
    one(&wrap("let n: int\nprint(1)"), "MZ0926");
}

#[test]
fn mz0937_a_trailing_colon_on_a_block_line() {
    fixed_by(
        &wrap("when true:\n  print(1)\nend"),
        "MZ0937",
        "when true\n",
    );
}

#[test]
fn mz0980_prints_from_other_languages() {
    fixed_by(
        &wrap("let n = 2\nprint(\"n is\", n)"),
        "MZ0980",
        "print(\"n is {n}\")",
    );
    fixed_by(&wrap("console.log(1)"), "MZ0980", "print(1)");
    fixed_by(&wrap("println!(\"done\")"), "MZ0980", "print(\"done\")");
    fixed_by(&wrap("fmt.Println(1)"), "MZ0980", "print(1)");
    fixed_by(&wrap("print 1 + 1"), "MZ0980", "print(1 + 1)");
    fixed_by(&wrap("print()"), "MZ0980", "print(\"\")");
}

#[test]
fn reused_codes_keep_their_meaning_in_a_program() {
    // MZ0707, a name bound nowhere; MZ0712, truthiness; MZ0407, `if`.
    one(&wrap("print(nope)"), "MZ0707");
    let d = one(&wrap("let n = 1\nwhen n\n  print(n)\nend"), "MZ0712");
    assert_eq!(d.fix.expect("a guess").replace, "n is not 0");
    fixed_by(&wrap("if true\n  print(1)\nend"), "MZ0407", "when true\n");
    // The `end` echo: a `fn` closes with its name.
    fixed_by(
        "program t\n  fn main\n  end\nend program t\n",
        "MZ0208",
        "  end fn main\n",
    );
    fixed_by(
        "program t\n  fn main\n  end fn mian\nend program t\n",
        "MZ0207",
        "end fn main",
    );
    fixed_by(
        "program t\n  fn main\n  end fn main\n",
        "MZ0204",
        "end program t",
    );
}

#[test]
fn the_agent_protocol_carries_program_diagnostics() {
    let report = check(&wrap("print(1 == 1)"), "t.mz");
    let ndjson = report.to_ndjson(0);
    assert!(ndjson.contains(r#""code":"MZ0910""#), "{ndjson}");
    assert!(ndjson.contains(r#""replace":"is","confidence":"exact""#));
}

// ------------------------------------------------------------------- lowering

fn main_rs(src: &str, file: &str) -> String {
    let p = program(src);
    let package = mzizi_lang_compiler::run::lower(&p, file);
    let toml = &package.files[0];
    assert_eq!(toml.0, "Cargo.toml");
    assert!(toml.1.contains("name = \"mz-fib\"") || toml.1.contains("name = \"mz-hello\""));
    assert!(toml.1.contains("\n[workspace]\n"));
    assert!(toml.1.contains("\n[dependencies]\n\n"), "no dependencies");
    package.files[1].1.clone()
}

#[test]
fn the_lowering_is_deterministic_and_dependency_free() {
    let src = example("fib.mz");
    let a = mzizi_lang_compiler::run::lower(&program(&src), "fib.mz");
    let b = mzizi_lang_compiler::run::lower(&program(&src), "fib.mz");
    assert_eq!(a, b);
    let main = main_rs(&src, "fib.mz");
    for banned in [".unwrap()", ".expect(", "panic!", "unsafe", "println!"] {
        assert!(!main.contains(banned), "the lowering emitted `{banned}`");
    }
    assert!(!main.contains("extern crate"));
}

#[test]
fn the_lowering_emits_checked_arithmetic_and_trap_sites() {
    let main = main_rs(&example("fib.mz"), "fib.mz");
    assert!(main.contains("fn fib(n: i64) -> i64 {"), "{main}");
    assert!(main.contains(
        "return mz_add(fib(mz_sub(n, 1i64, &MZ_AT_3)), fib(mz_sub(n, 2i64, &MZ_AT_4)), &MZ_AT_5);"
    ));
    assert!(main.contains(
        "const MZ_AT_5: MzAt = MzAt { file: \"fib.mz\", line: 22, col: 12, text: \"fib(n - 1) + fib(n - 2)\" };"
    ));
    assert!(main.contains("fn mz_main() {"));
    assert!(main.contains("let mut total: i64 = 0i64;"));
    assert!(main.contains("if (mz_rem(n, 2i64, &MZ_AT_6) == 0i64) && (n > 0i64) {"));
    assert!(main.contains(".stack_size(64 * 1024 * 1024)"));
    assert!(main.contains("::std::process::exit(101)"));
    assert!(main.contains("::std::process::exit(141)"));
}

#[test]
fn a_rust_keyword_is_emitted_as_a_raw_identifier() {
    let src = "program t\n  fn main\n    let type = 1\n    print(move(type))\n  end fn main\n  fn move(loop: int): int\n    return loop\n  end fn move\nend program t\n";
    let report = check(src, "t.mz");
    assert_eq!(report.error_count(), 0, "{src}\n{:#?}", report.diagnostics);
    let main = mzizi_lang_compiler::run::lower(&program(src), "t.mz").files[1]
        .1
        .clone();
    assert!(main.contains("let r#type: i64 = 1i64;"), "{main}");
    assert!(main.contains("fn r#move(r#loop: i64) -> i64 {"));
}

// ------------------------------------------------------------------- mz run

/// Run the shipped `mz` binary. `None` when `cargo` is not on the path, which these
/// tests report on standard error rather than pass in silence.
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
    let dir = std::env::temp_dir().join(format!("mz-program-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(name);
    std::fs::write(&path, src).expect("write");
    path
}

#[test]
fn mz_run_hello_prints_hello_world() {
    let path = root().join("examples/hello.mz");
    let Some(out) = mz_run(&["run", path.to_str().unwrap()]) else {
        return;
    };
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "hello, world\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    let expected = std::fs::read_to_string(root().join("examples/hello.expected")).unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
}

#[test]
fn mz_run_fib_recurses_and_prints_exactly() {
    let path = root().join("examples/fib.mz");
    let Some(out) = mz_run(&["run", path.to_str().unwrap()]) else {
        return;
    };
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "fib(9) is 34\nfib(20) is 6765\nfib(10) + fib(11) is 144, and that is fib(12): true\n6765 is odd, or not positive\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    let expected = std::fs::read_to_string(root().join("examples/fib.expected")).unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
}

#[test]
fn mz_run_traps_on_overflow_and_division_by_zero_with_status_101() {
    let src = "program trap\n\n  fn main\n    print(\"before\")\n    print(grow(1, 0))\n    print(\"never\")\n  end fn main\n\n  fn grow(n: int, steps: int): int\n    when steps is 70\n      return n\n    end\n    return grow(n * 2, steps + 1)\n  end fn grow\n\nend program trap\n";
    let path = scratch("trap.mz", src);
    let Some(out) = mz_run(&["run", path.to_str().unwrap()]) else {
        return;
    };
    assert_eq!(String::from_utf8_lossy(&out.stdout), "before\n");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "mz: trap MZ0991 at trap.mz:13:17: integer overflow in `n * 2`\n"
    );
    assert_eq!(out.status.code(), Some(101));

    let src = "program div\n\n  fn main\n    print(-7 / 2)\n    print(-7 % 2)\n    print(least() % -1)\n    print(half(0))\n  end fn main\n\n  fn half(n: int): int\n    return 10 / n\n  end fn half\n\n  fn least: int\n    return -9223372036854775807 - 1\n  end fn least\n\nend program div\n";
    let path = scratch("div.mz", src);
    let Some(out) = mz_run(&["run", path.to_str().unwrap()]) else {
        return;
    };
    // Truncation toward zero, and the dividend's sign (RFC-0013 §4.1).
    // `int` minimum `% -1` is 0, not a trap: only a zero divisor traps a remainder.
    assert_eq!(String::from_utf8_lossy(&out.stdout), "-3\n-1\n0\n");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "mz: trap MZ0991 at div.mz:11:12: integer division by zero in `10 / n`\n"
    );
    assert_eq!(out.status.code(), Some(101));
}

#[test]
fn mz_run_on_a_program_that_does_not_check_exits_3_and_runs_nothing() {
    let path = scratch("broken.mz", &wrap("print(1 == 1)"));
    let Some(out) = mz_run(&["run", path.to_str().unwrap()]) else {
        return;
    };
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("[MZ0910]"));
}

#[test]
fn mz_run_without_a_per_user_cache_directory_refuses_and_writes_nothing_to_temp() {
    // No fallback to the shared temporary directory: a fixed name there is one another
    // account can create first (semgrep rust.lang.security.temp-dir).
    let path = root().join("examples/hello.mz");
    let shared = std::env::temp_dir().join("mz-run");
    let existed = shared.exists();
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(["run", path.to_str().unwrap()])
        .env_remove("MZ_CACHE_DIR")
        .env_remove("XDG_CACHE_HOME")
        .env_remove("HOME")
        .output()
        .expect("mz runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("MZ_CACHE_DIR") && err.contains("XDG_CACHE_HOME") && err.contains("HOME"),
        "{err}"
    );
    assert_eq!(
        shared.exists(),
        existed,
        "mz run must not create {}",
        shared.display()
    );
}

#[test]
fn mz_run_honours_mz_cache_dir() {
    let cache = std::env::temp_dir().join(format!("mz-cache-test-{}", std::process::id()));
    let path = root().join("examples/hello.mz");
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(["run", path.to_str().unwrap()])
        .env("MZ_CACHE_DIR", &cache)
        .output()
        .expect("mz runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let made: Vec<_> = std::fs::read_dir(&cache).expect("cache made").collect();
    assert_eq!(made.len(), 1, "one package directory under MZ_CACHE_DIR");
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn other_commands_on_a_program_behave() {
    let path = root().join("examples/fib.mz");
    let bin = env!("CARGO_BIN_EXE_mz");
    for cmd in ["check", "contract"] {
        let out = Command::new(bin)
            .args([cmd, path.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "mz {cmd}");
    }
    for cmd in ["outline", "ir", "hash"] {
        let out = Command::new(bin)
            .args([cmd, path.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "mz {cmd}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("does not cover programs yet"));
    }
    // `mz run` runs programs only.
    let svc = root().join("examples/registry.mz");
    let out = Command::new(bin)
        .args(["run", svc.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn an_annotation_on_an_assignment_to_a_bound_name_is_deleted() {
    fixed_by(
        &wrap("var n = 1\nn: text = 2\nprint(n)"),
        "MZ0925",
        "    n = 2\n",
    );
}

#[test]
fn a_capitalised_program_word_is_one_diagnostic() {
    // The lexer reads operators in a `Program` file too, so the `+` is not a second error.
    let d = one(
        "Program t\n  fn main\n    print(1 + 2)\n  end fn main\nend program t\n",
        "MZ0101",
    );
    assert_eq!(d.fix.expect("exact").replace, "program");
}

#[test]
fn mz_run_shows_warnings_and_an_inherited_deny_warnings_does_not_fail_the_build() {
    // A `var` never changed is a warning (`MZ0924`), and Rust's `unused_mut` in the
    // lowered code: `RUSTFLAGS="-D warnings"` in the caller's environment must not turn it
    // into an `MZ0990`.
    let path = scratch("warn.mz", &wrap("var n = 1\nlet unused = 2\nprint(n)"));
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(["run", path.to_str().unwrap()])
        .env("RUSTFLAGS", "-D warnings")
        .output()
        .expect("mz runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "1\n");
    assert!(stderr.contains("[MZ0924]"), "{stderr}");
}

// ------------------------------------------------------------------- one error, not three

#[test]
fn a_keyword_named_parameter_is_one_mz0903() {
    // The parameter still counts, so `twice(2)` is not short an argument (`MZ0905`), and
    // `match` in the body is not a second error (`MZ0917`).
    let src = "program t\n\n  fn main\n    print(\"{twice(2)}\")\n  end fn main\n\n  fn twice(match: int): int\n    return match * 2\n  end fn twice\n\nend program t\n";
    let d = one(src, "MZ0903");
    assert!(d.say.contains("`match` is a keyword"), "{d:#?}");
}

#[test]
fn a_keyword_named_function_is_one_mz0903_and_its_call_is_silent() {
    // The call comes before the `fn`, as `main` usually does.
    let src = "program t\n\n  fn main\n    nothing()\n    print(\"{true}\")\n  end fn main\n\n  fn nothing\n    print(\"x\")\n  end fn nothing\n\nend program t\n";
    let d = one(src, "MZ0903");
    assert!(d.say.contains("`nothing` is a keyword"), "{d:#?}");
    // Called as a statement, with a keyword that opens a block, it is still one error.
    let src = "program t\n\n  fn main\n    match(1)\n  end fn main\n\n  fn match(n: int)\n    print(\"{n}\")\n  end fn match\n\nend program t\n";
    one(src, "MZ0903");
    // Used as a value, not called, it is silent too.
    let src = "program t\n\n  fn main\n    let a = nothing\n    print(\"{a}\")\n  end fn main\n\n  fn nothing: int\n    return 1\n  end fn nothing\n\nend program t\n";
    one(src, "MZ0903");
}

#[test]
fn an_oversized_literal_is_one_mz0103() {
    one(&wrap("print(\"{99999999999999999999}\")"), "MZ0103");
    one(
        &wrap("let x = 99999999999999999999 + 1\nprint(\"{x}\")"),
        "MZ0103",
    );
}

#[test]
fn a_program_that_opens_with_another_languages_comment_is_one_mz0911() {
    // It is still lexed as a program, with its operators: no `MZ0104` for the `+`.
    for marker in ["//", "#"] {
        let src = format!(
            "{marker} adds two numbers\nprogram t\n\n  fn main\n    let x = 1 + 2\n    print(\"{{x}}\")\n  end fn main\n\nend program t\n"
        );
        fixed_by(&src, "MZ0911", "## adds two numbers");
    }
    // `///` and `//!` are replaced whole.
    fixed_by(
        &wrap("/// a note\nprint(\"hi\")"),
        "MZ0911",
        "    ## a note",
    );
    fixed_by(
        &wrap("//! a note\nprint(\"hi\")"),
        "MZ0911",
        "    ## a note",
    );
    // `#!` may be a shebang and `#[inline]` is code, which `##` would break: a guess.
    for first in ["#!/usr/bin/env mz run", "#[inline]"] {
        let d = one(
            &format!(
                "{first}\nprogram t\n\n  fn main\n    print(\"hi\")\n  end fn main\n\nend program t\n"
            ),
            "MZ0911",
        );
        assert_eq!(
            d.fix.expect("a fix").confidence,
            Confidence::Guess,
            "{first}"
        );
    }
    // Inside a function body, too.
    fixed_by(&wrap("// a note\nprint(\"hi\")"), "MZ0911", "## a note");
}
