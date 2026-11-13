//! RFC-0018 wave 1 (LANGUAGE-TRACKER K5): function values in a program. A parameter, a `let` or a
//! `var` may have a function type; a bare `fn` name is a value where one is expected; a value is
//! called like a function; and nothing is captured. One test per diagnostic, and `mz run` of
//! `examples/fn_values.mz` end to end.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::check;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Helpers every test below may use, then `main`'s body.
fn wrap(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!(
        "program t\n  fn double(x: int): int\n    return x * 2\n  end fn double\n  fn half(x: float): float\n    return x / 2.0\n  end fn half\n  fn hello\n    print(1)\n  end fn hello\n  fn apply(f: fn(int): int, x: int): int\n    return f(x)\n  end fn apply\n  fn main\n{body}  end fn main\nend program t\n"
    )
}

fn codes(src: &str) -> Vec<(&'static str, u32)> {
    check(src, "t.mz")
        .diagnostics
        .iter()
        .map(|d| (d.code, d.span.start_line))
        .collect()
}

fn only(src: &str, code: &str) -> String {
    let report = check(src, "t.mz");
    let found: Vec<_> = report.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(found, vec![code], "{src}\n{:#?}", report.diagnostics);
    report.diagnostics[0].say.clone()
}

#[test]
fn a_named_fn_passes_to_a_function_typed_parameter_and_is_called_through_it() {
    let src = wrap(
        "print(apply(double, 4))\nlet g: fn(int): int = double\nprint(g(1))\nvar k: fn(int): int = double\nk = double\nprint(k(2))",
    );
    assert_eq!(codes(&src), vec![], "{src}");
}

#[test]
fn a_function_type_may_return_nothing_and_take_several_parameters() {
    let src = "program t\n  fn show(s: text, n: int)\n    print(\"{s}{n}\")\n  end fn show\n  fn run(f: fn(text, int), g: fn(), h: fn(fn(int): int): int)\n    f(\"a\", 1)\n    g()\n  end fn run\n  fn main\n    run(show, show, show)\n  end fn main\nend program t\n";
    // `g: fn()` is not given a `fn` of no parameters, and `h` takes a function parameter: the
    // types are read, and the mismatched arguments are `MZ0909`.
    let found = codes(src);
    assert!(found.iter().all(|(c, _)| *c == "MZ0909"), "{found:?}");
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_bare_fn_name_where_no_function_type_is_expected_is_mz0909() {
    let say = only(&wrap("let h = double\nprint(1)"), "MZ0909");
    assert!(
        say.contains("passed by name where a function type is expected"),
        "{say}"
    );
}

#[test]
fn an_argument_whose_signature_does_not_fit_quotes_both() {
    let say = only(&wrap("print(apply(half, 1))"), "MZ0909");
    assert!(say.contains("fn half(x: float): float"), "{say}");
    assert!(say.contains("fn(int): int"), "{say}");
}

#[test]
fn a_call_through_a_value_checks_its_arguments_like_any_call() {
    let say = only(
        &wrap("let g: fn(int): int = double\nprint(g(1, 2))"),
        "MZ0905",
    );
    assert!(say.contains("takes 1 argument"), "{say}");
    only(
        &wrap("let g: fn(int): int = double\nprint(g(1.5))"),
        "MZ0905",
    );
    only(&wrap("print(apply(double, 1, 2))"), "MZ0905");
    only(&wrap("print(apply(1, 2))"), "MZ0905");
}

#[test]
fn a_var_of_function_type_takes_only_the_same_type() {
    let say = only(
        &wrap("var k: fn(int): int = double\nk = half\nprint(k(1))"),
        "MZ0909",
    );
    assert!(say.contains("fn half(x: float): float"), "{say}");
}

#[test]
fn a_function_type_in_a_field_a_collection_a_result_or_a_return_is_mz0977() {
    for (decl, position) in [
        (
            "record r\n    field cb: fn(int): int\n  end\n",
            "record's field",
        ),
        (
            "fn a(xs: list(fn(int): int))\n    print(1)\n  end fn a\n",
            "element of a collection",
        ),
        (
            "fn a(m: map(text, fn(int): int))\n    print(1)\n  end fn a\n",
            "element of a collection",
        ),
        (
            "fn a: result(fn(int), text)\n    return error(\"x\")\n  end fn a\n",
            "payload of a result",
        ),
        (
            "fn a: fn(int): int\n    return double\n  end fn a\n",
            "return type",
        ),
    ] {
        let src = format!(
            "program t\n  fn double(x: int): int\n    return x * 2\n  end fn double\n  {decl}  fn main\n    print(1)\n  end fn main\nend program t\n"
        );
        let report = check(&src, "t.mz");
        let found: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == "MZ0977")
            .collect();
        assert_eq!(found.len(), 1, "{decl}\n{:#?}", report.diagnostics);
        assert!(
            found[0].say.contains(position),
            "{}: {}",
            found[0].say,
            position
        );
    }
}

#[test]
fn a_function_returned_by_a_function_type_is_mz0977_at_the_inner_type() {
    only(
        "program t\n  fn a(f: fn(int): fn(int): int)\n    print(1)\n  end fn a\n  fn main\n    print(1)\n  end fn main\nend program t\n",
        "MZ0977",
    );
}

#[test]
fn a_function_value_has_no_equality_and_no_text_form() {
    let say = only(
        &wrap("let g: fn(int): int = double\nprint(g is g)"),
        "MZ0977",
    );
    assert!(say.contains("no equality"), "{say}");
    only(&wrap("let g: fn(int): int = double\nprint(g)"), "MZ0905");
    only(
        &wrap("let g: fn(int): int = double\nprint(\"{g}\")"),
        "MZ0711",
    );
}

#[test]
fn a_function_value_has_no_methods() {
    let src = wrap("let g: fn(int): int = double\nprint(g.length())");
    let found = codes(&src);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "MZ0708");
}

#[test]
fn a_function_type_missing_its_parenthesis_is_mz0306() {
    let src = wrap("let g: fn(int = double\nprint(1)");
    let found = codes(&src);
    assert!(found.iter().any(|(c, _)| *c == "MZ0306"), "{found:?}");
}

#[test]
fn the_lowering_is_a_rust_function_pointer_and_a_plain_call() {
    let src = std::fs::read_to_string(root().join("examples/fn_values.mz")).expect("example");
    let report = check(&src, "fn_values.mz");
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    let program = match mzizi_lang_compiler::parse::parse_program(&src, "fn_values.mz") {
        (Some(mzizi_lang_compiler::parse::Program::Program(p)), _) => p,
        other => panic!("not a program: {other:?}"),
    };
    let package = mzizi_lang_compiler::run::lower(&program, &src);
    let main = &package
        .files
        .iter()
        .find(|(name, _)| name.ends_with("main.rs"))
        .expect("main.rs")
        .1;
    assert!(main.contains("f: fn(i64) -> i64"), "{main}");
    assert!(main.contains("say: fn(String)"), "{main}");
    assert!(main.contains("check: fn(i64) -> bool"), "{main}");
    assert!(main.contains("apply_twice(double, 5)"), "{main}");
    assert!(main.contains("f(f(x))"), "{main}");
}

#[test]
fn mz_run_fn_values_prints_its_expected_output() {
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return;
    }
    let cache = std::env::temp_dir().join(format!("mz-fn-values-{}", std::process::id()));
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args([
            "run",
            root()
                .join("examples/fn_values.mz")
                .to_str()
                .expect("utf-8 path"),
        ])
        .env("MZ_CACHE_DIR", &cache)
        .output()
        .expect("mz runs");
    let expected =
        std::fs::read_to_string(root().join("examples/fn_values.expected")).expect("expected");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        expected,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    // Worked out by hand: double(double(5)) = 20, negate(negate(5)) = 5, double(negate(4)) = -8.
    assert!(expected.starts_with("20\n5\n42\n3\nhabari!\ndunia!\n-8\n6\n-3\n"));
}
