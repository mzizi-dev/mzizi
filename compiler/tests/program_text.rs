//! RFC-0013 §10 in a program, tracker row C6: the text methods that need no option or list
//! (`length`, `contains`, `starts_with`, `ends_with`, `trim`, `to_upper`, `to_lower`,
//! `replace`, `repeat`), their types and diagnostics, other languages' spellings and their
//! fixes, the lowering, and `mz run` end to end, on non-ASCII text throughout.
//!
//! As in `program_numbers.rs`: each diagnostic with its `exact` fix applied by
//! `apply_exact_fixes` (what `mz fix` runs) and the result checked again; the lowered text;
//! and `mz run` with exact standard output, standard error and exit status. The `mz run`
//! tests need `cargo` on the path; without it they say so on standard error and pass, and
//! CI's `lowering` job runs `examples/text.mz` through the shipped binary regardless.

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

fn program(src: &str) -> mzizi_lang_compiler::program::Program {
    match parse_program(src, "t.mz") {
        (Some(Program::Program(p)), _) => p,
        other => panic!("not a program: {other:?}"),
    }
}

/// `body` as the body of `fn main`.
fn wrap(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!("program t\n\n  fn main\n{body}  end fn main\n\nend program t\n")
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

/// [`one`], then apply every `exact` fix as `mz fix` does and check the result has no
/// errors and holds `want`.
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

/// [`one`], with no fix at all.
fn unfixed(src: &str, code: &str) -> Diagnostic {
    let d = one(src, code);
    assert!(d.fix.is_none(), "{d:#?}");
    d
}

fn clean(src: &str) {
    let report = check(src, "t.mz");
    assert!(
        report.diagnostics.is_empty(),
        "{src}\n{:#?}",
        report.diagnostics
    );
}

// ------------------------------------------------------------------- check

#[test]
fn the_text_example_checks_clean() {
    let src = std::fs::read_to_string(root().join("examples/text.mz")).expect("example");
    clean(&src);
}

#[test]
fn each_method_types_as_rfc_0013_section_10_says() {
    clean(&wrap(
        "let s = \"héllo\"\nlet n: int = s.length()\nlet a: bool = s.contains(\"é\")\nlet b: bool = s.starts_with(\"h\")\nlet c: bool = s.ends_with(\"o\")\nlet t: text = s.trim()\nlet u: text = s.to_upper()\nlet l: text = s.to_lower()\nlet r: text = s.replace(\"l\", by = \"L\")\nlet p: text = s.repeat(2)\nprint(\"{n} {a} {b} {c} {t} {u} {l} {r} {p}\")",
    ));
    // A method chains on the text another returns, and on a literal.
    clean(&wrap("print(\"  Ab \".trim().to_lower().length())"));
    // The result of a method is a value like any other: compared, bound, returned.
    clean(&wrap(
        "let s = \"a\"\nwhen s.to_upper() is \"A\" and s.length() < 2\n  print(\"yes\")\nend",
    ));
}

#[test]
fn replace_writes_its_label_in_canonical_text() {
    let p = program(&wrap("let r = \"aa\".replace(\"a\", by = \"b\")\nprint(r)"));
    let StmtKind::Bind { value, .. } = &p.fns[0].body[0].kind else {
        panic!()
    };
    assert_eq!(canonical(value), "\"aa\".replace(\"a\", by = \"b\")");
}

#[test]
fn mz0905_text_method_arguments() {
    let d = unfixed(&wrap("print(\"a\".contains(1))"), "MZ0905");
    assert!(d.say.contains("takes text"), "{}", d.say);
    let d = unfixed(&wrap("print(\"a\".repeat(\"2\"))"), "MZ0905");
    assert!(d.say.contains("takes int"), "{}", d.say);
    let d = unfixed(&wrap("print(\"a\".trim(1))"), "MZ0905");
    assert!(d.say.contains("takes 0 arguments"), "{}", d.say);
    unfixed(&wrap("print(\"a\".replace(\"a\"))"), "MZ0905");
}

#[test]
fn mz0708_a_method_text_does_not_have() {
    guessed(&wrap("print(\"a\".trimm())"), "MZ0708", "trim");
    let d = unfixed(&wrap("print(\"a\".frobnicate())"), "MZ0708");
    assert!(d.say.contains("`length`"), "{}", d.say);
    // A text method on a number is the number's `MZ0708`.
    one(&wrap("print(true.trim())"), "MZ0708");
}

#[test]
fn mz0915_a_constant_negative_repeat_count() {
    let d = unfixed(&wrap("print(\"ab\".repeat(-1))"), "MZ0915");
    assert!(d.say.contains("negative"), "{}", d.say);
    clean(&wrap("print(\"ab\".repeat(0))"));
}

/// The methods §10 designs that return an option or a list wait for those (C7, and §18.2's
/// "C4 options"): one `MZ0919` each, naming what it returns.
#[test]
fn mz0919_methods_that_return_an_option_or_a_list() {
    for (call, ret) in [
        ("s.slice(0, to = 1)", "option(text)"),
        ("s.find(\"b\")", "option(int)"),
        ("s.parse_int()", "option(int)"),
        ("s.parse_float()", "option(float)"),
        ("s.split(\",\")", "list(text)"),
        ("s.chars()", "list(text)"),
    ] {
        let d = unfixed(&wrap(&format!("let s = \"ab\"\nlet x = {call}")), "MZ0919");
        assert!(d.say.contains(ret), "{call}: {}", d.say);
    }
}

// ------------------------------------------------------------------- idioms

#[test]
fn mz0962_other_languages_lengths_are_length() {
    fixed_by(
        &wrap("let s = \"é\"\nprint(len(s))"),
        "MZ0962",
        "print(s.length())",
    );
    fixed_by(
        &wrap("print(len(\"日本\"))"),
        "MZ0962",
        "print(\"日本\".length())",
    );
    guessed(&wrap("let s = \"é\"\nprint(s.len())"), "MZ0962", "length");
    guessed(&wrap("let s = \"é\"\nprint(s.size())"), "MZ0962", "length");
    guessed(&wrap("let s = \"é\"\nprint(s.count())"), "MZ0962", "length");
    guessed(&wrap("let s = \"é\"\nprint(s.length)"), "MZ0962", "()");
    guessed(&wrap("let s = \"é\"\nprint(s.len)"), "MZ0962", "length()");
    // Rust's `len()` counts bytes, Java's `size()` is a collection's, JavaScript's `length`
    // counts UTF-16 units: "é" is 2 bytes and "🙂" 2 units, but each is one `length()`, so
    // those fixes are guesses. Python's `len(s)` counts as Mzizi does, and stays exact.
    // Python's `s.count(t)` counts occurrences, which Mzizi has no method for.
    one(&wrap("let s = \"é\"\nprint(s.count(\"é\"))"), "MZ0708");
}

#[test]
fn mz0962_other_languages_method_names() {
    for (written, want) in [
        ("s.upper()", "s.to_upper()"),
        ("s.lower()", "s.to_lower()"),
        ("s.to_uppercase()", "s.to_upper()"),
        ("s.to_lowercase()", "s.to_lower()"),
        ("s.startswith(\"a\")", "s.starts_with(\"a\")"),
        ("s.endswith(\"a\")", "s.ends_with(\"a\")"),
        ("s.includes(\"a\")", "s.contains(\"a\")"),
        (
            "s.replace_all(\"a\", \"b\")",
            "s.replace(\"a\", by = \"b\")",
        ),
    ] {
        fixed_by(
            &wrap(&format!("let s = \"Ä\"\nprint({written})")),
            "MZ0962",
            &format!("print({want})"),
        );
    }
    // JavaScript's camelCase spellings: the lexer's `MZ0101` and this are one diagnostic,
    // whose fix replaces the word as written.
    for (written, want) in [
        ("s.toUpperCase()", "s.to_upper()"),
        ("s.toLowerCase()", "s.to_lower()"),
        ("s.replaceAll(\"a\", \"b\")", "s.replace(\"a\", by = \"b\")"),
    ] {
        fixed_by(
            &wrap(&format!("let s = \"Ä\"\nprint({written})")),
            "MZ0962",
            &format!("print({want})"),
        );
    }
    // `startsWith` is `starts_with` once the lexer's own fix is applied.
    fixed_by(
        &wrap("let s = \"Ä\"\nprint(s.startsWith(\"a\"))"),
        "MZ0101",
        "s.starts_with(\"a\")",
    );
    // Python's `strip()` also strips U+001C to U+001F, which `trim()` keeps: a guess.
    guessed(&wrap("let s = \"Ä\"\nprint(s.strip())"), "MZ0962", "trim");
    // Python's `strip(chars)` strips those characters: renaming it would only move the
    // error, so there is no fix.
    unfixed(&wrap("let s = \"Ä\"\nprint(s.strip(\"x\"))"), "MZ0962");
}

#[test]
fn mz0962_methods_waiting_for_options_get_a_guess() {
    // JavaScript's `substring` clamps and swaps its ends, and `slice` is not built: a
    // guess that names the form to write.
    guessed(
        &wrap("let s = \"Ä\"\nprint(s.substring(0, 1))"),
        "MZ0962",
        "s.slice(0, to = 1)",
    );
    guessed(
        &wrap("let s = \"Ä\"\nprint(s.indexOf(\"a\"))"),
        "MZ0962",
        "find",
    );
    unfixed(&wrap("let s = \"Ä\"\nprint(s.substr(0, 1))"), "MZ0962");
}

/// RFC-0013 §3.3: a text is not a collection and is never `none`, so its emptiness is
/// `s is ""`, and every other spelling of the question is `MZ0962` with that `exact` fix,
/// applied in one pass of `mz fix`, never two.
#[test]
fn mz0962_emptiness_is_is_empty_text() {
    let cases = [
        ("print(s.is_empty())", "print(s is \"\")"),
        ("print(not s.is_empty())", "print(s is not \"\")"),
        ("print(s.length() is 0)", "print(s is \"\")"),
        ("print(s.length() is not 0)", "print(s is not \"\")"),
        ("print(s.length() > 0)", "print(s is not \"\")"),
        ("print(not s.length() is 0)", "print(s is not \"\")"),
        ("print(s.len() is 0)", "print(s is \"\")"),
        ("print(s.length() == 0)", "print(s is \"\")"),
        ("print(!s.is_empty())", "print(s is not \"\")"),
        ("print(s.isEmpty())", "print(s is \"\")"),
        (
            "print(s.is_empty() is false)",
            "print((s is \"\") is false)",
        ),
        ("print(s.trim().is_empty())", "print(s.trim() is \"\")"),
        ("print(s.is_empty)", "print(s is \"\")"),
        ("print(len(s) == 0)", "print(s is \"\")"),
        ("print(len(s) > 0)", "print(s is not \"\")"),
        ("print(s.count() is 0)", "print(s is \"\")"),
        ("print(s.length is 0)", "print(s is \"\")"),
        (
            "print((s.length() is 0) is false)",
            "print((s is \"\") is false)",
        ),
        (
            "when s.is_empty() or s.length() is 0\n  print(1)\nend",
            "when s is \"\" or s is \"\"",
        ),
    ];
    for (body, want) in cases {
        let src = wrap(&format!("let s = \"ü\"\n{body}"));
        let report = check(&src, "t.mz");
        let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.iter().all(|c| *c == "MZ0962"),
            "{body}: {codes:?}\n{:#?}",
            report.diagnostics
        );
        let after = apply_exact_fixes(&src, &report);
        let again = check(&after, "t.mz");
        assert_eq!(again.error_count(), 0, "{after}\n{:#?}", again.diagnostics);
        assert!(
            after.contains(want),
            "{body}: expected `{want}` in:\n{after}"
        );
    }
    // Inside an interpolation the fix would put a string literal in `{…}` (§3.6): no fix.
    unfixed(&wrap("let s = \"ü\"\nprint(\"{s.is_empty()}\")"), "MZ0962");
}

/// A `fn len` the program declares is its own function, not Python's.
#[test]
fn a_program_of_its_own_len_is_not_the_idiom() {
    let src = "program t\n\n  fn main\n    print(len(\"a\") is 0)\n  end fn main\n\n  fn len(s: text): int\n    return 1\n  end fn len\n\nend program t\n";
    clean(src);
}

#[test]
fn mz0927_replace_labels_its_second_argument() {
    fixed_by(
        &wrap("print(\"aä\".replace(\"a\", \"b\"))"),
        "MZ0927",
        "\"aä\".replace(\"a\", by = \"b\")",
    );
    fixed_by(
        &wrap("print(\"aä\".replace(\"a\", by: \"b\"))"),
        "MZ0927",
        "\"aä\".replace(\"a\", by = \"b\")",
    );
    // Another name in the label's place is a misspelling of it.
    fixed_by(
        &wrap("print(\"aä\".replace(\"a\", with = \"b\"))"),
        "MZ0927",
        "\"aä\".replace(\"a\", by = \"b\")",
    );
    // On a number, `replace` is not a method at all: that is the one diagnostic.
    one(&wrap("print(3.replace(1, 2))"), "MZ0708");
    // `slice` is not built: its one diagnostic is `MZ0919`, labelled or not.
    unfixed(&wrap("let s = \"ab\"\nlet x = s.slice(0, 1)"), "MZ0919");
}

// ------------------------------------------------------------------- lowering

fn main_rs(src: &str) -> String {
    let p = program(src);
    mzizi_lang_compiler::run::lower(&p, "t.mz").files[1]
        .1
        .clone()
}

/// Every method the checker types has a lowering arm (`run.rs` writes a `compile_error!`
/// for one that has none).
#[test]
fn every_text_method_lowers() {
    for name in mzizi_lang_compiler::text::METHODS {
        let (params, _) =
            mzizi_lang_compiler::text::method(mzizi_lang_compiler::expr::Ty::Text, name)
                .expect("typed");
        let args: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(k, t)| {
                let v = if t.name() == "int" { "2" } else { "\"b\"" };
                match mzizi_lang_compiler::text::label(name, k) {
                    Some(l) => format!("{l} = {v}"),
                    None => v.to_string(),
                }
            })
            .collect();
        let src = wrap(&format!("print(\"ab\".{name}({}))", args.join(", ")));
        clean(&src);
        let main = main_rs(&src);
        assert!(!main.contains("compile_error!"), "{name}:\n{main}");
    }
}

#[test]
fn the_lowering_of_text_methods() {
    let main = main_rs(&wrap(
        "let s = \"héllo\"\nprint(s.length())\nprint(s.contains(\"é\"))\nprint(s.trim().to_upper())\nprint(s.replace(\"l\", by = \"L\"))\nprint(s.repeat(2))\nprint(\"{s.to_lower()}!\".ends_with(\"!\"))",
    ));
    for want in [
        "mz_print(&mz_text_length(s.as_str()));",
        "mz_print(&s.as_str().contains(\"é\"));",
        "mz_print(&(s.as_str().trim().to_string()).as_str().to_uppercase());",
        "mz_print(&s.as_str().replace(\"l\", \"L\"));",
        "mz_print(&mz_repeat(s.as_str(), 2i64, &MZ_AT_1));",
        "pub fn mz_text_length(s: &str) -> i64 {",
        "pub fn mz_text_repeat(s: &str, n: i64) -> Result<String, &'static str> {",
        "const MZ_AT_1: MzAt = MzAt { file: \"t.mz\", line: 9, col: 11, text: \"s.repeat(2)\" };",
    ] {
        assert!(main.contains(want), "`{want}` not in:\n{main}");
    }
    // §14.3: no unwrap, expect, panic or unsafe, and no byte indexing of a string.
    for banned in [
        "unwrap", "expect(", "panic!", "unsafe", "println!", ".len()", "[0", "..",
    ] {
        let body = main.split("fn mz_main").nth(1).unwrap_or_default();
        assert!(!body.contains(banned), "the lowering emitted `{banned}`");
    }
    for banned in ["unwrap", "expect(", "panic!", "unsafe"] {
        assert!(!main.contains(banned), "the runtime holds `{banned}`");
    }
}

// ------------------------------------------------------------------- mz run

fn mz_run(args: &[&str], cache: &std::path::Path) -> Option<std::process::Output> {
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return None;
    }
    Some(
        Command::new(env!("CARGO_BIN_EXE_mz"))
            .args(args)
            .env("MZ_CACHE_DIR", cache)
            .output()
            .expect("mz runs"),
    )
}

fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mz-text-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn run_program(name: &str, src: &str) -> Option<(String, String, Option<i32>)> {
    let dir = scratch_dir();
    let path = dir.join(name);
    std::fs::write(&path, src).expect("write");
    let out = mz_run(
        &["run", path.to_str().expect("utf-8 path")],
        &dir.join("cache"),
    )?;
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    ))
}

#[test]
fn mz_run_text_prints_its_expected_output() {
    let path = root().join("examples/text.mz");
    let Some(out) = mz_run(
        &["run", path.to_str().expect("utf-8 path")],
        &scratch_dir().join("cache"),
    ) else {
        return;
    };
    let expected =
        std::fs::read_to_string(root().join("examples/text.expected")).expect("expected");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        expected,
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(expected.contains("[Zürich] has 6 characters; with its spaces, 10\n"));
    assert!(expected.contains("straße in upper case is STRASSE\n"));
}

/// C6's "Done when": one test per method, with the scalar-value cases. Every method on
/// ASCII, accented Latin, CJK, an emoji and a two-scalar flag, run through the lowering.
#[test]
fn mz_run_every_method_counts_and_cuts_by_scalar_value() {
    let src = wrap(
        "let e = \"\"\nprint(e.length())\nprint(\"héllo\".length())\nprint(\"日本語\".length())\nprint(\"🙂\".length())\nprint(\"🇰🇪\".length())\nprint(\"a\\tb\\n\".length())\nprint(\"héllo\".contains(\"él\"))\nprint(\"héllo\".contains(\"\"))\nprint(\"héllo\".contains(\"e\"))\nprint(\"日本語\".starts_with(\"日本\"))\nprint(\"日本語\".starts_with(\"本\"))\nprint(\"🙂ok\".ends_with(\"ok\"))\nprint(\"🙂ok\".ends_with(\"🙂\"))\nlet padded = \"\\u{3000}\\t x \\n\".trim()\nprint(\"[{padded}]\")\nprint(\"Ünïcödé\".to_upper())\nprint(\"ÀÉÎ İ\".to_lower())\nprint(\"ΣΑΣ\".to_lower())\nprint(\"straße\".to_upper())\nprint(\"a-b-a\".replace(\"a\", by = \"ä\"))\nprint(\"日日\".replace(\"日\", by = \"月\"))\nprint(\"ab\".replace(\"\", by = \"-\"))\nprint(\"ab\".replace(\"x\", by = \"y\"))\nprint(\"é\".repeat(3))\nprint(\"é\".repeat(0) is \"\")\nprint(e.repeat(1000000) is \"\")",
    );
    // `\u{3000}` is not a Mzizi escape; write the ideographic space itself.
    let src = src.replace("\\u{3000}", "\u{3000}");
    let Some((out, err, code)) = run_program("scalars.mz", &src) else {
        return;
    };
    assert_eq!(
        out,
        "0\n5\n3\n1\n2\n4\ntrue\ntrue\nfalse\ntrue\nfalse\ntrue\nfalse\n[x]\nÜNÏCÖDÉ\nàéî i̇\nσας\nSTRASSE\nä-b-ä\n月月\n-a-b-\nab\nééé\ntrue\ntrue\n",
        "stderr: {err}"
    );
    assert_eq!(err, "");
    assert_eq!(code, Some(0));
}

/// `repeat`'s traps (RFC-0013 §4.3, as this pull request records in §18.6): a negative
/// count, and a result too long for a text, stop the program with `MZ0991` and exit 101.
#[test]
fn mz_run_repeat_traps_exit_101_with_mz0991() {
    let cases = [
        (
            "print(\"before\")\nprint(\"ab\".repeat(id(-1)))",
            "negative repeat count in `\"ab\".repeat(id(-1))`",
            "5:11",
        ),
        (
            "print(\"before\")\nprint(\"ab\".repeat(id(9223372036854775807)))",
            "text too long in `\"ab\".repeat(id(9223372036854775807))`",
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
