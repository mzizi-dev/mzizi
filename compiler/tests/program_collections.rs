//! RFC-0013 §9 (LANGUAGE-TRACKER C7): collections in a function body — `list(T)`,
//! `map(K, V)` and `set(K)`, bracket literals, indexing that returns an option read with
//! `otherwise` (§3.7, §8.1), `in`, emptiness as `is none`, the methods of §9.2, `map`,
//! `filter` and the eight named folds of §9.4, `for each` over a list, and `range` as a list.
//!
//! The four kinds of test the other program suites have: the parse; the check, one test per
//! diagnostic C7 claims (`MZ0909`, `MZ0960`–`MZ0964`) and each reused code these forms reach,
//! with each `exact` fix applied by `mz fix`'s own function and the result checked again;
//! the lowered text; and `mz run` end to end, with exact standard output. The end-to-end
//! tests need `cargo` on the path; without it they say so on standard error and pass, and
//! CI's `lowering` job runs `examples/collections.mz` through the shipped binary regardless.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Severity};
use mzizi_lang_compiler::expr::{BinOp, ExprKind, canonical};
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

/// `body` as the body of `fn main`, in a program with an enum and the helpers a fold takes.
fn wrap(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!(
        "program t\n\n  enum suit\n    hearts\n    spades\n  end\n\n  fn main\n{body}  end fn main\n\n  fn double(n: int): int\n    return n * 2\n  end fn double\n\n  fn is_even(n: int): bool\n    return n % 2 is 0\n  end fn is_even\n\n  fn add(a: int, b: int): int\n    return a + b\n  end fn add\n\n  fn half(x: float): float\n    return x / 2.0\n  end fn half\n\nend program t\n"
    )
}

fn errors(src: &str) -> Vec<Diagnostic> {
    check(src, "t.mz")
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

fn clean(src: &str) {
    let report = check(src, "t.mz");
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics for:\n{src}\ngot {:#?}",
        report.diagnostics
    );
}

/// Check `src`, expect exactly one error with `code`, and return it.
fn one(src: &str, code: &str) -> Diagnostic {
    let errors = errors(src);
    assert_eq!(
        errors.len(),
        1,
        "one diagnostic per true error, for:\n{src}\ngot {errors:#?}"
    );
    assert_eq!(errors[0].code, code, "{:#?}", errors[0]);
    errors[0].clone()
}

/// [`one`], then apply its `exact` fix with `mz fix`'s function, and check that the result
/// has no errors and holds `want`.
fn fixed_by(src: &str, code: &str, want: &str) -> String {
    let d = one(src, code);
    let fix = d.fix.as_ref().expect("an exact fix");
    assert_eq!(fix.confidence, Confidence::Exact, "{d:#?}");
    let after = apply_exact_fixes(src, &check(src, "t.mz"));
    let again = check(&after, "t.mz");
    assert_eq!(
        again.error_count(),
        0,
        "the fix must leave a program that checks:\n{after}\n{:#?}",
        again.diagnostics
    );
    assert!(after.contains(want), "expected `{want}` in:\n{after}");
    after
}

/// [`one`], whose fix is a `guess` writing `want`.
fn guessed(src: &str, code: &str, want: &str) {
    let d = one(src, code);
    let fix = d.fix.as_ref().expect("a guess");
    assert_eq!(
        (fix.confidence, fix.replace.as_str()),
        (Confidence::Guess, want),
        "{d:#?}"
    );
}

// ------------------------------------------------------------------- parse

#[test]
fn literals_indexes_and_the_new_operators_parse_to_their_canonical_text() {
    let p = program(&wrap(
        "let a = [1, 2, 3]\nlet b = [\"x\": 1, \"y\": 2]\nlet c = a[0] otherwise a[1] otherwise 0\nlet d = 1 in a and not 2 in a\nlet e = a[1 + 1] otherwise 0 + 1\nprint(\"{a} {b} {c} {d} {e}\")",
    ));
    let values: Vec<String> = p.fns[0]
        .body
        .iter()
        .filter_map(|s| match &s.kind {
            StmtKind::Bind { value, .. } => Some(canonical(value)),
            _ => None,
        })
        .collect();
    assert_eq!(
        values,
        [
            "[1, 2, 3]",
            "[\"x\": 1, \"y\": 2]",
            "a[0] otherwise a[1] otherwise 0",
            "1 in a and not 2 in a",
            "a[1 + 1] otherwise 0 + 1",
        ]
    );
    // `otherwise` is right-associative (§3.5): `a otherwise (b otherwise 0)`.
    let StmtKind::Bind { value, .. } = &p.fns[0].body[2].kind else {
        panic!()
    };
    let ExprKind::Binary {
        op: BinOp::Otherwise,
        rhs,
        ..
    } = &value.kind
    else {
        panic!("{value:?}")
    };
    assert!(matches!(
        rhs.kind,
        ExprKind::Binary {
            op: BinOp::Otherwise,
            ..
        }
    ));
    // Below arithmetic: `xs[i] otherwise 0 + 1` is `xs[i] otherwise (0 + 1)`.
    let StmtKind::Bind { value, .. } = &p.fns[0].body[4].kind else {
        panic!()
    };
    let ExprKind::Binary {
        op: BinOp::Otherwise,
        rhs,
        ..
    } = &value.kind
    else {
        panic!("{value:?}")
    };
    assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Add, .. }));
}

#[test]
fn an_indexed_assignment_is_its_own_statement() {
    let p = program(&wrap("var xs = [1]\nxs[0] = 2\nprint(xs)"));
    assert!(matches!(
        &p.fns[0].body[1].kind,
        StmtKind::IndexAssign { name, .. } if name == "xs"
    ));
    clean(&wrap("var xs = [1]\nxs[0] = 2\nprint(xs)"));
    clean(&wrap(
        "var m: map(text, int) = []\nm[\"a\"] = 1\nm[\"a\"] = 2\nprint(m)",
    ));
}

// ------------------------------------------------------------------- types

#[test]
fn the_example_checks_clean() {
    clean(&example("collections.mz"));
}

#[test]
fn a_bracket_literal_takes_its_type_from_where_it_stands() {
    clean(&wrap(
        "let s: set(int) = [3, 1, 3]\nlet m: map(text, int) = []\nlet e: list(text) = []\nlet n = [[1], []]\nlet o = [[], [1]]\nprint(\"{s} {m} {e} {n} {o}\")",
    ));
    // A parameter, a return type and an argument fix an empty literal too.
    clean(
        "program t\n\n  fn main\n    print(total([]))\n    print(none_yet())\n  end fn main\n\n  fn total(xs: list(int)): int\n    return xs.sum()\n  end fn total\n\n  fn none_yet: map(text, bool)\n    return []\n  end fn none_yet\n\nend program t\n",
    );
    // Bare variants are read against the element type, or resolve on their own.
    clean(&wrap(
        "let a: list(suit) = [hearts, spades]\nlet b = [hearts, spades]\nprint(\"{a} {b} {spades in a}\")",
    ));
}

#[test]
fn indexing_and_first_and_slice_return_options_read_with_otherwise() {
    clean(&wrap(
        "let xs = [1, 2]\nlet a = xs[5] otherwise 0\nlet b = xs.first(is_even) otherwise -1\nlet c = xs.slice(0, to = 1) otherwise []\nlet m = [\"a\": 1]\nlet d = m[\"z\"] otherwise 0\nprint(\"{a} {b} {c} {d}\")",
    ));
    // An option bound with `let` is still an option: used as its value, `MZ0710`.
    let d = one(&wrap("let xs = [1]\nlet x = xs[0]\nprint(x + 1)"), "MZ0710");
    assert!(d.say.contains("x otherwise <int>"), "{}", d.say);
    assert!(d.fix.is_none());
}

#[test]
fn mz0710_an_option_used_as_its_value() {
    for body in [
        "let xs = [1]\nprint(xs[0] + 1)",
        "let xs = [1]\nprint(xs[0])",
        "let xs = [1]\nprint(\"{xs[0]}\")",
        "let xs = [1]\nprint(double(xs[0]))",
        "let xs = [1]\nprint(xs[0] is 1)",
        "let xs = [1]\nprint(xs[0].abs())",
        "let xs = [1]\nlet n: int = xs[0]\nprint(n)",
        "let xs = [true]\nwhen xs[0]\n  print(1)\nend",
        "let xs = [[1]]\nfor each x in xs[0]\n  print(x)\nend",
    ] {
        one(&wrap(body), "MZ0710");
    }
}

/// An `option(T)` written as a return type is not built (`MZ0919`), reported once at the
/// signature. Its `return` is not also `MZ0710`, which would advise a default for a type that
/// cannot be written (RFC-0013 §8, §18.2): `one` holds the program to exactly that one error.
#[test]
fn an_unbuilt_option_return_type_is_reported_once_at_the_signature() {
    for (src, line) in [
        (
            "program t\n\n  fn f(t: text): option(int)\n    return t.parse_int()\n  end fn f\n\n  fn main\n    print(\"x\")\n  end fn main\n\nend program t\n",
            3,
        ),
        (
            "program t\n\n  fn g(xs: list(int)): option(int)\n    return xs[0]\n  end fn g\n\n  fn main\n    print(\"x\")\n  end fn main\n\nend program t\n",
            3,
        ),
    ] {
        let d = one(src, "MZ0919");
        assert_eq!(d.span.start_line, line, "{d:#?}");
    }
}

// ------------------------------------------------------------------- MZ0909

#[test]
fn mz0909_a_function_argument_is_a_named_fn_whose_signature_fits() {
    // A lambda, in each spelling: no fix, the say names the repair.
    for lambda in ["x => x * 2", "(x) => x * 2", "lambda x: x * 2"] {
        let d = one(
            &wrap(&format!("let xs = [1]\nprint(xs.map({lambda}))")),
            "MZ0909",
        );
        assert!(
            d.fix.is_none() && d.say.contains("declare a `fn`"),
            "{d:#?}"
        );
    }
    // A signature that does not fit is quoted beside what was needed.
    let d = one(&wrap("let xs = [1]\nprint(xs.map(half))"), "MZ0909");
    assert!(
        d.say.contains("fn half(x: float): float") && d.say.contains("one int"),
        "{}",
        d.say
    );
    one(&wrap("let xs = [1]\nprint(xs.filter(double))"), "MZ0909");
    one(
        &wrap("let xs = [1]\nprint(xs.fold(0.0, step = add))"),
        "MZ0909",
    );
    one(&wrap("let xs = [1]\nlet f = 2\nprint(xs.map(f))"), "MZ0909");
    one(&wrap("let xs = [1]\nprint(xs.map(nothing_here))"), "MZ0909");
    // A `fn` as a value anywhere else is still `MZ0909` (Wave 0's).
    one(&wrap("let f = double\nprint(1)"), "MZ0909");
}

// ------------------------------------------------------------------- MZ0960

#[test]
fn mz0960_a_mutation_changes_a_var_only() {
    for (body, want) in [
        ("let xs = [1]\nxs.push(2)\nprint(xs)", "var xs = [1]"),
        ("let xs = [1]\nxs[0] = 2\nprint(xs)", "var xs = [1]"),
        (
            "let m = [\"a\": 1]\nm.remove(\"a\")\nprint(m)",
            "var m = [\"a\": 1]",
        ),
        (
            "let s: set(int) = []\ns.insert(1)\nprint(s)",
            "var s: set(int) = []",
        ),
    ] {
        fixed_by(&wrap(body), "MZ0960", want);
    }
    // One report per binding: the fix makes every later change legal.
    one(
        &wrap("let xs = [1]\nxs.push(2)\nxs.push(3)\nprint(xs)"),
        "MZ0960",
    );
    // A parameter, a loop binding and a value no name holds have no fix.
    let src = "program t\n\n  fn main\n    print(grow([1]))\n  end fn main\n\n  fn grow(xs: list(int)): int\n    xs.push(1)\n    return xs.length()\n  end fn grow\n\nend program t\n";
    assert!(one(src, "MZ0960").fix.is_none());
    let d = one(
        &wrap("let xss = [[1]]\nfor each xs in xss\n  xs.push(2)\nend"),
        "MZ0960",
    );
    assert!(d.fix.is_none());
    assert!(
        one(
            &wrap("let xs = [1]\nxs.map(double).push(2)\nprint(xs)"),
            "MZ0960"
        )
        .fix
        .is_none()
    );
    // A `var` that is only mutated is not `MZ0924`.
    clean(&wrap("var xs = [1]\nxs.push(2)\nprint(xs)"));
}

// ------------------------------------------------------------------- MZ0961

#[test]
fn mz0961_a_bracket_literal_that_cannot_be_typed() {
    one(&wrap("let xs = []\nprint(1)"), "MZ0961");
    one(&wrap("print([1, \"two\"])"), "MZ0961");
    one(&wrap("let m = [\"a\": 1, 2]\nprint(1)"), "MZ0961");
    one(&wrap("let m = [\"a\": 1, \"a\": 2]\nprint(m)"), "MZ0961");
    one(&wrap("let m = [1: \"a\", 2: 3]\nprint(m)"), "MZ0961");
    // A set keeps a repeated element once, and is not an error.
    clean(&wrap("let s: set(int) = [1, 1]\nprint(s)"));
    // `[]` where a type that is not a collection is expected is one mismatch.
    one(&wrap("let n: int = []\nprint(n)"), "MZ0711");
}

// ------------------------------------------------------------------- MZ0962

#[test]
fn mz0962_a_collections_operations_spelt_another_way_get_exact_fixes() {
    for (body, want) in [
        ("let xs = [1]\nprint(len(xs))", "print(xs.length())"),
        ("let xs = [1]\nprint(xs.len())", "print(xs.length())"),
        ("let xs = [1]\nprint(xs.size())", "print(xs.length())"),
        ("let xs = [1]\nprint(xs.count())", "print(xs.length())"),
        ("let xs = [1]\nprint(xs.length)", "print(xs.length())"),
        ("var xs = [1]\nxs.append(2)\nprint(xs)", "xs.push(2)"),
        ("let xs = [1]\nprint(xs.contains(1))", "print(1 in xs)"),
        ("let xs = [1]\nprint(xs.includes(1))", "print(1 in xs)"),
        ("let s: set(int) = [1]\nprint(s.has(1))", "print(1 in s)"),
        (
            "let xs = [1]\nprint(xs.get(0) otherwise 0)",
            "print(xs[0] otherwise 0)",
        ),
        (
            "let m = [\"a\": 1]\nprint(m.get(\"a\") otherwise 0)",
            "print(m[\"a\"] otherwise 0)",
        ),
        ("let xs = [1]\nprint(xs is [])", "print(xs is none)"),
        ("let xs = [1]\nprint(xs is not [])", "print(xs is not none)"),
        ("let xs = [1]\nprint(xs.length() is 0)", "print(xs is none)"),
        // Another language's length compared with 0 goes straight to `none`, so `mz fix`
        // converges in one pass (as C6's text forms do).
        ("let xs = [1]\nprint(len(xs) is 0)", "print(xs is none)"),
        ("let xs = [1]\nprint(xs.count() is 0)", "print(xs is none)"),
        (
            "let xs = [1]\nprint(xs.size() > 0)",
            "print(xs is not none)",
        ),
        (
            "let xs = [1]\nprint(xs.length() is not 0)",
            "print(xs is not none)",
        ),
        ("let xs = [1]\nprint(xs.is_empty())", "print(xs is none)"),
        (
            "let xs = [1]\nprint(xs.is_empty() is true)",
            "print((xs is none) is true)",
        ),
        (
            "let xs = [1]\nprint(not xs.is_empty())",
            "print(xs is not none)",
        ),
        (
            "let xs = [1]\nprint(xs.some(is_even))",
            "print(xs.any(is_even))",
        ),
        (
            "let xs = [1]\nprint(xs.every(is_even))",
            "print(xs.all(is_even))",
        ),
        (
            "let xs = [1]\nprint(xs.sortedBy(double))",
            "print(xs.sort_by(double))",
        ),
        (
            "let xs = [1]\nprint(sorted(xs, key = double))",
            "print(xs.sort_by(double))",
        ),
    ] {
        fixed_by(&wrap(body), "MZ0962", want);
    }
    // With `==`, `!=` or `not a is b` written too, that operator's `MZ0910` is folded into
    // the whole comparison's fix (`lib.rs`): one diagnostic, and one `mz fix` pass.
    for (body, want) in [
        (
            "let xs = [1]\nprint(not xs.length() is 0)",
            "print(xs is not none)",
        ),
        (
            "let xs = [1]\nprint(not xs.length() is not 0)",
            "print(xs is none)",
        ),
        (
            "let xs = [1]\nprint(not len(xs) is 0)",
            "print(xs is not none)",
        ),
        (
            "let xs = [1]\nprint(not xs.count() is 0)",
            "print(xs is not none)",
        ),
        (
            "let xs = [1]\nprint(not xs.size() > 0)",
            "print(xs is none)",
        ),
        ("let xs = [1]\nprint(len(xs) == 0)", "print(xs is none)"),
        (
            "let xs = [1]\nprint(xs.len() != 0)",
            "print(xs is not none)",
        ),
        (
            "let xs = [1]\nprint((len(xs) == 0) is false)",
            "print((xs is none) is false)",
        ),
        (
            "let xs = [1]\nwhen len(xs) == 0\n  print(0)\nend",
            "when xs is none",
        ),
    ] {
        fixed_by(&wrap(body), "MZ0962", want);
    }
    // On a receiver that is not a name or a path the rewrite is a guess, so it folds
    // nothing: the parser's `exact` `MZ0910` stays, and `mz fix` applies that one.
    let src = wrap("let xs = [1]\nprint(not xs.map(double).length() is 0)");
    let codes: Vec<_> = errors(&src).into_iter().map(|d| d.code).collect();
    assert_eq!(codes, ["MZ0910", "MZ0962"], "{src}");
    // JavaScript's `reduce(f, init)` is a guess: without `init` it has no equivalent.
    guessed(
        &wrap("let xs = [1]\nprint(xs.reduce(add, 0))"),
        "MZ0962",
        "xs.fold(0, step = add)",
    );
    // On a value that is not a name or a path, a rewrite around it is a guess.
    guessed(
        &wrap("let xs = [1]\nprint(xs.map(double).is_empty())"),
        "MZ0962",
        "xs.map(double) is none",
    );
    // `in` on text is a substring, C6's method `s.contains(t)` (§3.3).
    fixed_by(
        &wrap("let s = \"abc\"\nprint(\"a\" in s)"),
        "MZ0962",
        "print(s.contains(\"a\"))",
    );
    // A key-less sort is not in the set (§20 Q26): no fix.
    assert!(
        one(&wrap("let xs = [1]\nprint(sorted(xs))"), "MZ0962")
            .fix
            .is_none()
    );
    // Only `key = f` is a sort key: `reverse = true` is a named argument (`MZ0905`), and
    // the sort gets no fix, nor a rewrite that quotes `key`.
    let all = errors(&wrap("let xs = [1]\nprint(sorted(xs, reverse = true))"));
    assert!(all.iter().any(|d| d.code == "MZ0905"), "{all:#?}");
    let sorted = all.iter().find(|d| d.code == "MZ0962").expect("MZ0962");
    assert!(
        sorted.fix.is_none() && !sorted.say.contains("key ="),
        "{sorted:#?}"
    );
    // A key that is not a function of the program is a guess: `sort_by` would refuse it.
    guessed(
        &wrap("let xs = [1]\nprint(sorted(xs, key = 3))"),
        "MZ0962",
        "xs.sort_by(3)",
    );
    // `.first` returns an option where `.find` may not have been read as one: a guess.
    guessed(
        &wrap("let xs = [1]\nprint(xs.find(is_even) otherwise 0)"),
        "MZ0962",
        "first",
    );
}

/// `mz fix` once, then `mz check`, on the shipped binary: every exact fix for another
/// language's emptiness lands in one pass, and the result checks clean.
#[test]
fn mz_fix_converges_in_one_pass_on_another_languages_emptiness() {
    let src = wrap(
        "let xs = [1]\nwhen len(xs) == 0\n  print(0)\nend\nprint(xs.count() is 0)\nprint(len(xs) == 0)\nprint(xs.size() > 0)\nprint(sorted(xs, key = double))\nprint(not xs.length() is 0)\nprint(not xs.length() is not 0)\nprint(not len(xs) is 0)",
    );
    let dir = std::env::temp_dir().join(format!("mz-collections-fix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("t.mz");
    std::fs::write(&path, &src).expect("write");
    let mz = |cmd: &str| {
        Command::new(env!("CARGO_BIN_EXE_mz"))
            .args([cmd, path.to_str().expect("utf-8")])
            .output()
            .expect("mz runs")
    };
    let fixed = mz("fix");
    let after = std::fs::read_to_string(&path).expect("read");
    let checked = mz("check");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(fixed.status.code(), Some(0), "{fixed:?}");
    assert_eq!(
        checked.status.code(),
        Some(0),
        "one `mz fix` must leave a program that checks:\n{after}\n{}",
        String::from_utf8_lossy(&checked.stdout)
    );
    for want in [
        "when xs is none",
        "print(xs is none)",
        "print(xs is not none)",
        "print(xs.sort_by(double))",
    ] {
        assert!(after.contains(want), "expected `{want}` in:\n{after}");
    }
    // Every length spelling is gone, not left half-rewritten for a second pass.
    for gone in ["len(", ".length()", ".count()", ".size()", "not xs"] {
        assert!(!after.contains(gone), "`{gone}` left in:\n{after}");
    }
}

// ------------------------------------------------------------------- MZ0963, MZ0964

#[test]
fn mz0963_a_tuple_has_no_fix() {
    let d = one(&wrap("let p = (1, 2)\nprint(1)"), "MZ0963");
    assert!(d.fix.is_none() && d.say.contains("record"), "{d:#?}");
    // Parentheses around one value are not a tuple.
    clean(&wrap("let p = (1 + 2) * 3\nprint(p)"));
}

#[test]
fn mz0964_a_key_type_with_no_order() {
    for ty in [
        "map(float, int)",
        "set(float)",
        "set(list(int))",
        "map(map(int, int), int)",
    ] {
        one(
            &format!(
                "program t\n\n  fn main\n    print(1)\n  end fn main\n\n  fn f(x: {ty}): int\n    return 1\n  end fn f\n\nend program t\n"
            ),
            "MZ0964",
        );
    }
    one(&wrap("let m = [1.5: 1]\nprint(m)"), "MZ0964");
    one(&wrap("let xs = [1.5]\nprint(xs.sort_by(half))"), "MZ0964");
    one(&wrap("let xs = [1.5]\nprint(xs.group_by(half))"), "MZ0964");
    // An enum, a bool, an int and a text are keys.
    clean(&wrap(
        "let a: map(suit, int) = [hearts: 1]\nlet b: set(bool) = [true]\nlet c: map(int, text) = [1: \"a\"]\nprint(\"{a} {b} {c}\")",
    ));
}

// ------------------------------------------------------------------- reused codes

#[test]
fn a_list_type_spelt_with_brackets_is_mz0105_in_the_type_parser() {
    fixed_by(
        &wrap("let xs: [int] = [1]\nprint(xs)"),
        "MZ0105",
        "let xs: list(int) = [1]",
    );
    fixed_by(
        &wrap("let xs: int[] = [1]\nprint(xs)"),
        "MZ0105",
        "let xs: list(int) = [1]",
    );
    // Nested, it is one diagnostic for the whole type, so no two fixes overlap.
    fixed_by(
        &wrap("let xs: [[int]] = [[1]]\nprint(xs)"),
        "MZ0105",
        "let xs: list(list(int)) = [[1]]",
    );
    fixed_by(
        &wrap("let xs: list(int)[] = [[1]]\nprint(xs)"),
        "MZ0105",
        "let xs: list(list(int)) = [[1]]",
    );
}

#[test]
fn reused_codes_reach_the_collection_forms() {
    // MZ0915: a literal negative index, Python's count from the end.
    guessed(
        &wrap("let xs = [1]\nprint(xs[-1] otherwise 0)"),
        "MZ0915",
        "xs[xs.length() - 1]",
    );
    // MZ0711: a map or a set iterated directly; the fix iterates in key order.
    fixed_by(
        &wrap("let m = [\"a\": 1]\nfor each k in m\n  print(k)\nend"),
        "MZ0711",
        "for each k in m.keys()",
    );
    fixed_by(
        &wrap("let s: set(int) = [1]\nfor each k in s\n  print(k)\nend"),
        "MZ0711",
        "for each k in s.to_list()",
    );
    one(&wrap("var xs = [1]\nxs[0] = \"a\"\nprint(xs)"), "MZ0711");
    one(
        &wrap("let xs = [1]\nprint(xs[\"a\"] otherwise 0)"),
        "MZ0711",
    );
    one(&wrap("var s: set(int) = []\ns[0] = 1\nprint(s)"), "MZ0711");
    one(&wrap("let xs = [\"a\"]\nprint(xs.sum())"), "MZ0711");
    one(&wrap("let xs = [1]\nprint(xs.join(\",\"))"), "MZ0711");
    // MZ0910: Python's `not in`.
    fixed_by(
        &wrap("let xs = [1]\nprint(1 not in xs)"),
        "MZ0910",
        "print(not 1 in xs)",
    );
    // MZ0712: truthiness; the guess asks whether it has elements.
    guessed(
        &wrap("let xs = [1]\nwhen xs\n  print(1)\nend"),
        "MZ0712",
        "xs is not none",
    );
    // MZ0912: `otherwise` on a value that is never `none`, a default of another type, `in`
    // on another type, `is none` on a value that is not a collection.
    one(&wrap("let n = 1\nprint(n otherwise 2)"), "MZ0912");
    one(
        &wrap("let xs = [1]\nprint(xs[0] otherwise \"x\")"),
        "MZ0912",
    );
    one(&wrap("let xs = [1]\nprint(\"a\" in xs)"), "MZ0912");
    one(&wrap("let n = 1\nprint(n is none)"), "MZ0912");
    // MZ0708: a method a collection does not have, with the nearest as a guess.
    guessed(&wrap("let xs = [1]\nprint(xs.sumx())"), "MZ0708", "sum");
    one(
        &wrap("let m = [\"a\": 1]\nprint(m.first(is_even))"),
        "MZ0708",
    );
    // MZ0905: a method's arguments.
    one(
        &wrap("let xs = [1]\nprint(xs.map(double, double))"),
        "MZ0905",
    );
    one(
        &wrap("let xs = [1]\nprint(xs.slice(\"a\", to = 1) otherwise [])"),
        "MZ0905",
    );
    // MZ0950: a result is never an element.
    one(
        "program t\n\n  fn main\n    print(1)\n  end fn main\n\n  fn f(x: list(result(int, text))): int\n    return 1\n  end fn f\n\nend program t\n",
        "MZ0950",
    );
}

#[test]
fn mz0919_still_names_what_c7_leaves_to_the_c4_options_follow_up() {
    one(&wrap("let x: option(int) = 1\nprint(1)"), "MZ0919");
    one(&wrap("let x = none\nprint(1)"), "MZ0919");
    one(
        &wrap("let xs = [1]\nlet x = xs[0]\nprint(x is none)"),
        "MZ0919",
    );
}

// ------------------------------------------------------------------- lowering

fn main_rs(src: &str) -> String {
    let p = program(src);
    let package = mzizi_lang_compiler::run::lower(&p, "collections.mz");
    package.files[1].1.clone()
}

#[test]
fn the_lowering_of_collections_is_plain_rust_with_no_panics() {
    let main = main_rs(&example("collections.mz"));
    for banned in [
        ".unwrap()",
        ".expect(",
        "panic!",
        "unsafe",
        "println!",
        "unreachable!",
    ] {
        assert!(!main.contains(banned), "the lowering emitted `{banned}`");
    }
    // No Rust indexing on a value: only the helper, which returns an `Option`.
    assert!(!main.contains("primes["), "{main}");
    for want in [
        "    let primes: Vec<i64> = vec![2i64, 3i64, 5i64, 7i64, 11i64];",
        "mz_index(&primes, 0i64)",
        "match mz_index(&primes, 0i64) { Some(mz_v) => mz_v, None => 0i64 }",
        "    let mut ages: ::std::collections::BTreeMap<String, i64> = ::std::collections::BTreeMap::from([(String::from(\"ada\"), 36i64), (String::from(\"alan\"), 41i64)]);",
        "    let mut seen: ::std::collections::BTreeSet<i64> = ::std::collections::BTreeSet::from([3i64, 1i64, 3i64, 2i64]);",
        "mz_fold(&(mz_map(&(mz_filter(&xs, is_even)), square, &MZ_AT_",
        "mz_set_index(&mut hands, mz_i, mz_v, &MZ_AT_",
        "    for p in primes.clone() {",
        "mz_range(1i64, 11i64, &MZ_AT_",
        "hands.contains(&Suit::Spades)",
        "ages.get(&String::from(\"grace\")).cloned()",
        "mz_sum_int(&",
    ] {
        assert!(main.contains(want), "expected\n{want}\nin\n{main}");
    }
}

// ------------------------------------------------------------------- mz run

/// Run the shipped `mz` binary on `src`, written to a file. `None` when `cargo` is not on
/// the path, which these tests report on standard error rather than pass in silence.
fn mz_run_src(name: &str, src: &str) -> Option<std::process::Output> {
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return None;
    }
    let dir = std::env::temp_dir().join(format!(
        "mz-collections-tests-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(format!("{name}.mz"));
    std::fs::write(&path, src).expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(["run", path.to_str().expect("utf-8")])
        .env("MZ_CACHE_DIR", dir.join("cache"))
        .output()
        .expect("mz runs");
    let _ = std::fs::remove_dir_all(&dir);
    Some(out)
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn mz_run_collections_prints_what_its_expected_file_holds() {
    let Some(out) = mz_run_src("collections", &example("collections.mz")) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        example("collections.expected"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn each_fold_on_an_empty_and_a_non_empty_list_and_an_index_out_of_range_is_none() {
    let src = wrap(
        "let e: list(int) = []\nlet xs = [3, 4, 5]\nprint(\"{e.count(is_even)} {e.sum()} {e.any(is_even)} {e.all(is_even)} {e.first(is_even) otherwise -1} {e.fold(7, step = add)} {e.sort_by(double)} {e.group_by(is_even)}\")\nprint(\"{xs.count(is_even)} {xs.sum()} {xs.any(is_even)} {xs.all(is_even)} {xs.first(is_even) otherwise -1} {xs.fold(7, step = add)} {xs.sort_by(double)} {xs.group_by(is_even)}\")\nlet fs: list(float) = []\nprint(\"{fs.sum()} {[0.5, 0.25].sum()}\")\nprint(\"{xs[3] otherwise -1} {xs[-0] otherwise -1} {e[0] otherwise -1} {xs.slice(2, to = 4) otherwise [0]}\")",
    );
    clean(&src);
    let Some(out) = mz_run_src("folds", &src) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        "0 0 false true -1 7 [] []\n1 12 true false 4 19 [3, 4, 5] [false: [3, 5], true: [4]]\n0.0 0.75\n-1 3 -1 [0]\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn collections_are_values_and_a_loop_iterates_the_list_it_began_with() {
    let src = "program t\n\n  fn main\n    var a = [1, 2]\n    let b = a\n    a.push(3)\n    print(\"{a} {b}\")\n    var xs = [1, 2]\n    for each x in xs\n      xs.push(x * 10)\n    end\n    print(xs)\n    print(\"{grow(xs)} {xs.length()}\")\n    var m: map(text, list(int)) = [\"k\": [1]]\n    let before = m\n    m[\"k\"] = [2]\n    print(\"{before} {m}\")\n  end fn main\n\n  fn grow(v: list(int)): int\n    var w = v\n    w.push(0)\n    return w.length()\n  end fn grow\n\nend program t\n";
    clean(src);
    let Some(out) = mz_run_src("values", src) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        "[1, 2, 3] [1, 2]\n[1, 2, 10, 20]\n5 4\n[\"k\": [1]] [\"k\": [2]]\n"
    );
}

#[test]
fn maps_and_sets_iterate_in_key_order_and_print_their_text_forms() {
    let src = wrap(
        "let m = [\"pear\": 2, \"apple\": 1, \"fig\": 3]\nprint(m)\nprint(m.keys())\nlet s: set(suit) = [spades, hearts, spades]\nprint(s)\nlet t = [\"a\\\"b\", \"{1}\", \"x\\ny\"]\nprint(t)\nprint([[1.0, 0.5], []])\nlet xs = [1]\nprint([xs[0], xs[1]])\nprint(m is [\"apple\": 1, \"fig\": 3, \"pear\": 2])",
    );
    clean(&src);
    let Some(out) = mz_run_src("order", &src) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        "[\"apple\": 1, \"fig\": 3, \"pear\": 2]\n[\"apple\", \"fig\", \"pear\"]\n[hearts, spades]\n[\"a\\\"b\", \"1\", \"x\\ny\"]\n[[1.0, 0.5], []]\n[1, none]\ntrue\n"
    );
}

#[test]
fn otherwise_evaluates_its_default_only_on_none() {
    // The default traps when it runs: a present value never runs it, an absent one does.
    let src = "program t\n\n  fn main\n    let xs = [5]\n    print(xs[0] otherwise boom())\n    print(xs[1] otherwise boom())\n  end fn main\n\n  fn boom: int\n    return 1 / zero()\n  end fn boom\n\n  fn zero: int\n    return 0\n  end fn zero\n\nend program t\n";
    clean(src);
    let Some(out) = mz_run_src("otherwise", src) else {
        return;
    };
    assert_eq!(stdout(&out), "5\n");
    assert_eq!(out.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("mz: trap MZ0991"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn an_index_out_of_range_an_int_sum_overflow_and_a_range_too_long_trap_with_101() {
    for (name, body, want) in [
        (
            "assign",
            "var xs = [1]\nxs[3] = 2\nprint(xs)",
            "index out of range in `xs[3]`",
        ),
        (
            "sum",
            "let xs = [9223372036854775807, 1]\nprint(xs.sum())",
            "integer overflow in `xs.sum()`",
        ),
        // A range as a list reserves its length first: one too long for `usize` or for
        // memory is `MZ0991`, not a capacity-overflow panic or the allocator's abort.
        (
            "range_max",
            "print(range(0, to = 9223372036854775807).length())",
            "a list too long to hold in memory in `range(0, to = 9223372036854775807)`",
        ),
        (
            "range_huge",
            "print(range(0, to = 100000000000).length())",
            "a list too long to hold in memory in `range(0, to = 100000000000)`",
        ),
    ] {
        let src = wrap(body);
        clean(&src);
        let Some(out) = mz_run_src(name, &src) else {
            return;
        };
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(101), "{err}");
        assert!(
            err.contains("mz: trap MZ0991 at") && err.contains(want),
            "{err}"
        );
    }
}

// ------------------------------------------------------------------- review cases

#[test]
fn a_literal_branch_of_a_block_value_takes_the_type_of_the_branches_before_it() {
    clean(&wrap(
        "let n = 3\nlet xs = when n > 2\n  [1]\nelse\n  []\nend\nprint(xs)",
    ));
    // The first branch has nothing before it to take a type from.
    one(
        &wrap("let n = 3\nlet xs = when n > 2\n  []\nelse\n  [1]\nend\nprint(xs)"),
        "MZ0961",
    );
    // A map literal's first entry with no `[]` in it fixes the rest, as a list's does.
    clean(&wrap("let m = [\"a\": [], \"b\": [1]]\nprint(m)"));
}

#[test]
fn an_idiom_on_a_value_that_is_not_a_collection_reports_its_operand_once() {
    // `double(true)` is one `MZ0905`, reported once; `[]` against an int is one mismatch.
    let found = errors(&wrap("let n = double(true) is []\nprint(n)"));
    let codes: Vec<&str> = found.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["MZ0905", "MZ0711"], "{found:#?}");
    // An option on the left of `in` is its one `MZ0710`; the literal is read on its own.
    one(
        &wrap("let m = [\"a\": 1]\nprint(m[\"a\"] in [1, 2])"),
        "MZ0710",
    );
    // `len` on text is C6's `length()`, which counts scalar values as Python's `len` does.
    fixed_by(
        &wrap("let s = \"hello\"\nprint(len(s))"),
        "MZ0962",
        "print(s.length())",
    );
    guessed(
        &wrap("let xs = [1]\nprint(len(xs.map(double)))"),
        "MZ0962",
        "xs.map(double).length()",
    );
}

#[test]
fn an_operator_assignment_through_an_index_is_one_mz0918_with_a_guess() {
    // `xs[i]` is an option, so the default is the author's: a guess, and the `var` counts
    // as changed, so it is not also `MZ0924`.
    guessed(
        &wrap("var xs = [1, 2]\nxs[0] += 1\nprint(xs)"),
        "MZ0918",
        "xs[0] = (xs[0] otherwise 0) + 1",
    );
    guessed(
        &wrap("var xs = [1.5]\nxs[0]++\nprint(xs)"),
        "MZ0918",
        "xs[0] = (xs[0] otherwise 0) + 1",
    );
    assert!(
        one(
            &wrap("var xs = [1]\nlet n = 2\nxs[0] *= n\nprint(xs)"),
            "MZ0918"
        )
        .fix
        .is_none()
    );
}

#[test]
fn an_option_of_an_option_takes_a_default_of_either_type() {
    let src = wrap(
        "let m = [\"a\": 1, \"c\": 3]\nlet xs = [m[\"a\"], m[\"b\"]]\nlet y = xs[0] otherwise m[\"c\"]\nlet z = xs[1] otherwise m[\"c\"]\nlet w = xs[5] otherwise m[\"c\"]\nprint([y, z, w])\nprint(xs[5] otherwise 0 is 0)",
    );
    one(&src, "MZ0912");
    let src = wrap(
        "let m = [\"a\": 1, \"c\": 3]\nlet xs = [m[\"a\"], m[\"b\"]]\nlet y = xs[0] otherwise m[\"c\"]\nlet z = xs[1] otherwise m[\"c\"]\nlet w = xs[5] otherwise m[\"c\"]\nprint([y, z, w])",
    );
    clean(&src);
    let Some(out) = mz_run_src("nested_options", &src) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        "[1, none, 3]\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
