//! RFC-0013 §11 (LANGUAGE-TRACKER C8): records and methods in a program body — a record
//! declared with `field`s, built by field name (`point(x = 1.0, y = 2.0)`), copied with
//! `with`, read by field, given methods that read `self`, assigned through a `var`, and
//! held to its `always` clauses at run time.
//!
//! The kinds of test the other program suites have: the check, one test per diagnostic C8
//! claims (`MZ0808`, `MZ0970`, `MZ0971`, `MZ0974`, `MZ0975`, and the reused codes these
//! forms reach), the one `exact` fix (`MZ0960` on a `let` that is assigned a field) applied
//! by `mz fix`'s own function and the result checked again; and `mz run` end to end, with
//! exact standard output and the trap's exit status. The end-to-end tests need `cargo` on
//! the path; without it they say so on standard error and pass, and CI's `lowering` job
//! runs `examples/records.mz` through the shipped binary regardless.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Severity};
use mzizi_lang_compiler::parse::{Program, parse_program};
use mzizi_lang_compiler::{apply_exact_fixes, check};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn example(name: &str) -> String {
    std::fs::read_to_string(root().join("examples").join(name)).expect("example exists")
}

/// A program with `decls` (records, enums, fns) before `fn main`, whose body is `body`.
fn with_decls(decls: &str, body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!("program t\n\n{decls}\n  fn main\n{body}  end fn main\n\nend program t\n")
}

/// The record every case below builds on: a point with a method.
const POINT: &str = "  record point
    field x: float
    field y: float

    fn norm: float
      return (self.x * self.x + self.y * self.y).sqrt()
    end fn norm
  end
";

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

/// Exactly one error, with `code`, for `src`.
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

fn program_of(src: &str) -> mzizi_lang_compiler::program::Program {
    match parse_program(src, "t.mz") {
        (Some(Program::Program(p)), _) => p,
        other => panic!("not a program: {other:?}"),
    }
}

// ------------------------------------------------------------------- declarations

#[test]
fn a_record_with_fields_and_a_method_checks_clean() {
    clean(&with_decls(POINT, "print(point(x = 3.0, y = 4.0).norm())"));
}

#[test]
fn a_record_is_parsed_with_its_fields_methods_and_invariants_in_order() {
    let src = with_decls(
        "  record span
    field low: int
    field high: int

    fn width: int
      return self.high - self.low
    end fn width

    contract
      always low <= high
    end
  end
",
        "print(span(low = 1, high = 2).width())",
    );
    let p = program_of(&src);
    let r = &p.records[0];
    assert_eq!(r.name, "span");
    let fields: Vec<&str> = r.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(fields, ["low", "high"]);
    assert_eq!(r.methods[0].name, "width");
    assert_eq!(r.always.len(), 1);
    clean(&src);
}

#[test]
fn a_record_named_like_a_built_in_type_is_mz0921() {
    one(
        &with_decls("  record int\n    field x: int\n  end\n", "print(1)"),
        "MZ0921",
    );
}

#[test]
fn two_records_with_one_name_are_mz0904() {
    let src = with_decls(
        "  record point\n    field x: int\n  end\n  record point\n    field y: int\n  end\n",
        "print(1)",
    );
    one(&src, "MZ0904");
}

#[test]
fn a_field_declared_twice_is_mz0904() {
    one(
        &with_decls(
            "  record p\n    field x: int\n    field x: int\n  end\n",
            "print(1)",
        ),
        "MZ0904",
    );
}

#[test]
fn a_result_field_is_mz0950() {
    one(
        &with_decls(
            "  record p\n    field r: result(int, text)\n  end\n",
            "print(1)",
        ),
        "MZ0950",
    );
}

#[test]
fn a_record_that_holds_itself_is_not_built_mz0919() {
    one(
        &with_decls(
            "  record node\n    field next: option(node)\n  end\n",
            "print(1)",
        ),
        "MZ0919",
    );
}

#[test]
fn a_record_holding_itself_through_a_list_is_fine() {
    clean(&with_decls(
        "  record node\n    field kids: list(node)\n  end\n",
        "print(node(kids = []).kids.length())",
    ));
}

#[test]
fn a_method_that_changes_self_is_not_built_mz0919_and_its_body_is_skipped() {
    let src = with_decls(
        "  record counter\n    field count: int\n\n    fn bump changes self\n      self.count = self.count + 1\n    end fn bump\n  end\n",
        "print(counter(count = 0).count)",
    );
    one(&src, "MZ0919");
}

// ------------------------------------------------------------------- literals

#[test]
fn a_literal_gives_every_field_by_name_in_declaration_order() {
    clean(&with_decls(
        POINT,
        "let p = point(x = 1.0, y = 2.0)\nprint(p.x)",
    ));
}

#[test]
fn a_positional_literal_is_mz0808() {
    one(
        &with_decls(POINT, "let p = point(1.0, 2.0)\nprint(p)"),
        "MZ0808",
    );
}

#[test]
fn an_unknown_field_is_mz0808_with_the_nearest_name_as_a_guess() {
    guessed(
        &with_decls(POINT, "let p = point(x = 1.0, yy = 2.0)\nprint(p)"),
        "MZ0808",
        "y",
    );
}

#[test]
fn a_field_left_out_is_mz0808() {
    one(
        &with_decls(POINT, "let p = point(x = 1.0)\nprint(p)"),
        "MZ0808",
    );
}

#[test]
fn a_field_given_twice_is_mz0808() {
    one(
        &with_decls(POINT, "let p = point(x = 1.0, x = 2.0, y = 3.0)\nprint(p)"),
        "MZ0808",
    );
}

#[test]
fn fields_out_of_declaration_order_are_mz0808() {
    one(
        &with_decls(POINT, "let p = point(y = 2.0, x = 1.0)\nprint(p)"),
        "MZ0808",
    );
}

#[test]
fn a_field_of_the_wrong_type_is_mz0808() {
    one(
        &with_decls(POINT, "let p = point(x = 1, y = 2.0)\nprint(p)"),
        "MZ0808",
    );
}

#[test]
fn a_literal_that_breaks_an_always_clause_is_mz0975_at_check_time() {
    let src = with_decls(
        "  record span
    field low: int
    field high: int

    contract
      always low <= high
    end
  end
",
        "print(span(low = 3, high = 1))",
    );
    one(&src, "MZ0975");
}

#[test]
fn a_literal_that_holds_its_always_clause_checks_clean() {
    clean(&with_decls(
        "  record span
    field low: int
    field high: int

    contract
      always low <= high
    end
  end
",
        "print(span(low = 1, high = 3))",
    ));
}

// ------------------------------------------------------------------- with

#[test]
fn with_copies_a_record_and_the_source_is_unchanged() {
    clean(&with_decls(
        POINT,
        "let p = point(x = 1.0, y = 2.0)\nlet q = p with (y = 0.0)\nprint(p)\nprint(q)",
    ));
}

#[test]
fn with_on_a_value_that_is_not_a_record_is_mz0974() {
    one(
        &with_decls("", "let n = 3\nprint(n with (x = 1))"),
        "MZ0974",
    );
}

#[test]
fn with_naming_no_field_is_mz0974() {
    one(
        &with_decls(POINT, "let p = point(x = 1.0, y = 2.0)\nprint(p with ())"),
        "MZ0974",
    );
}

#[test]
fn with_naming_an_unknown_field_is_mz0974_with_its_nearest_name() {
    guessed(
        &with_decls(
            POINT,
            "let p = point(x = 1.0, y = 2.0)\nprint(p with (xx = 1.0))",
        ),
        "MZ0974",
        "x",
    );
}

#[test]
fn with_replacing_a_field_twice_or_out_of_order_is_mz0974() {
    one(
        &with_decls(
            POINT,
            "let p = point(x = 1.0, y = 2.0)\nprint(p with (y = 1.0, x = 3.0))",
        ),
        "MZ0974",
    );
}

#[test]
fn with_a_value_of_the_wrong_type_is_mz0974() {
    one(
        &with_decls(
            POINT,
            "let p = point(x = 1.0, y = 2.0)\nprint(p with (x = 1))",
        ),
        "MZ0974",
    );
}

// ------------------------------------------------------------------- self and methods

#[test]
fn self_as_a_parameter_of_a_method_is_mz0970() {
    let src = with_decls(
        "  record point
    field x: float

    fn twice(self): float
      return self.x * 2.0
    end fn twice
  end
",
        "print(point(x = 1.0).twice())",
    );
    one(&src, "MZ0970");
}

#[test]
fn self_read_outside_a_method_is_mz0970() {
    one(&with_decls("", "print(self)"), "MZ0970");
}

#[test]
fn a_method_assigning_to_self_is_mz0971() {
    let src = with_decls(
        "  record counter
    field count: int

    fn bump
      self.count = self.count + 1
    end fn bump
  end
",
        "print(1)",
    );
    one(&src, "MZ0971");
}

#[test]
fn a_method_calls_another_method_of_its_record() {
    clean(&with_decls(
        "  record point
    field x: float
    field y: float

    fn norm: float
      return (self.x * self.x + self.y * self.y).sqrt()
    end fn norm

    fn length_twice: float
      return self.norm() * 2.0
    end fn length_twice
  end
",
        "print(point(x = 3.0, y = 4.0).length_twice())",
    ));
}

#[test]
fn a_method_with_the_wrong_number_of_arguments_is_mz0905() {
    let src = with_decls(
        "  record scale
    field k: float

    fn apply(v: float): float
      return v * self.k
    end fn apply
  end
",
        "print(scale(k = 2.0).apply(1.0, 2.0))",
    );
    one(&src, "MZ0905");
}

#[test]
fn a_method_argument_of_the_wrong_type_is_mz0905() {
    let src = with_decls(
        "  record scale
    field k: float

    fn apply(v: float): float
      return v * self.k
    end fn apply
  end
",
        "print(scale(k = 2.0).apply(1))",
    );
    one(&src, "MZ0905");
}

#[test]
fn a_method_named_without_its_parentheses_is_mz0708() {
    one(
        &with_decls(POINT, "let p = point(x = 1.0, y = 2.0)\nprint(p.norm)"),
        "MZ0708",
    );
}

#[test]
fn a_field_that_is_no_field_is_mz0708_with_its_nearest_name() {
    guessed(
        &with_decls(POINT, "let p = point(x = 1.0, y = 2.0)\nprint(p.xx)"),
        "MZ0708",
        "x",
    );
}

// ------------------------------------------------------------------- field assignment

#[test]
fn a_field_of_a_var_is_assigned() {
    clean(&with_decls(
        POINT,
        "var p = point(x = 1.0, y = 2.0)\np.x = 5.0\nprint(p)",
    ));
}

#[test]
fn a_field_of_a_let_is_mz0960_and_the_exact_fix_makes_it_a_var() {
    let src = with_decls(
        POINT,
        "let p = point(x = 1.0, y = 2.0)\np.x = 5.0\nprint(p)",
    );
    let after = fixed_by(&src, "MZ0960", "var p = point");
    assert!(after.contains("p.x = 5.0"), "{after}");
}

#[test]
fn a_field_assigned_a_value_of_the_wrong_type_is_mz0711() {
    let src = with_decls(POINT, "var p = point(x = 1.0, y = 2.0)\np.x = 5\nprint(p)");
    one(&src, "MZ0711");
}

#[test]
fn a_field_assigned_on_a_parameter_is_mz0960() {
    let src = format!(
        "program t\n\n{POINT}\n  fn bump(p: point): point\n    p.x = 2.0\n    return p\n  end fn bump\n\n  fn main\n    print(bump(point(x = 1.0, y = 2.0)))\n  end fn main\n\nend program t\n"
    );
    one(&src, "MZ0960");
}

// ------------------------------------------------------------------- always

#[test]
fn an_always_clause_that_is_not_a_bool_is_mz0712() {
    let src = with_decls(
        "  record span
    field low: int

    contract
      always low + 1
    end
  end
",
        "print(span(low = 1))",
    );
    one(&src, "MZ0712");
}

#[test]
fn an_always_clause_that_calls_a_fn_is_mz0975() {
    let src = "program t

  fn positive(n: int): bool
    return n > 0
  end fn positive

  record span
    field low: int

    contract
      always positive(low)
    end
  end

  fn main
    print(span(low = 1))
  end fn main

end program t
";
    one(src, "MZ0975");
}

#[test]
fn an_always_clause_may_read_only_the_fields() {
    let src = with_decls(
        "  record span
    field low: int

    contract
      always low <= ceiling
    end
  end
",
        "print(span(low = 1))",
    );
    one(&src, "MZ0707");
}

// ------------------------------------------------------------------- equality, text, copies

#[test]
fn records_compare_every_field_with_is() {
    clean(&with_decls(
        POINT,
        "let p = point(x = 1.0, y = 2.0)\nprint(p is point(x = 1.0, y = 2.0))\nprint(p is not point(x = 1.0, y = 3.0))",
    ));
}

#[test]
fn a_record_is_an_argument_and_a_result() {
    let src = format!(
        "program t\n\n{POINT}\n  fn scaled(p: point, k: float): point\n    return p with (x = p.x * k, y = p.y * k)\n  end fn scaled\n\n  fn main\n    print(scaled(point(x = 1.0, y = 2.0), 3.0))\n  end fn main\n\nend program t\n"
    );
    clean(&src);
    let Some(out) = mz_run_src("argument_result", &src) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        "point(x = 3.0, y = 6.0)\n",
        "stderr: {}",
        stderr(&out)
    );
}

// ------------------------------------------------------------------- run

/// Run the shipped `mz` binary on `src`, written to a file. `None` when `cargo` is not on
/// the path, which these tests report on standard error rather than pass in silence.
fn mz_run_src(name: &str, src: &str) -> Option<std::process::Output> {
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return None;
    }
    let dir = std::env::temp_dir().join(format!("mz-records-tests-{}-{name}", std::process::id()));
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

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn mz_run_records_prints_what_its_expected_file_holds() {
    let Some(out) = mz_run_src("records", &example("records.mz")) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        example("records.expected"),
        "stderr: {}",
        stderr(&out)
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn a_record_text_form_quotes_its_text_fields_and_nests() {
    let src = with_decls(
        "  record person
    field name: text
    field age: int

  end

  record pair
    field first: person
    field tags: list(text)
  end
",
        "let p = person(name = \"Ada\", age = 36)\nprint(p)\nprint(pair(first = p, tags = [\"x\", \"y\"]))\nprint([p])",
    );
    let Some(out) = mz_run_src("text_form", &src) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        "person(name = \"Ada\", age = 36)\npair(first = person(name = \"Ada\", age = 36), tags = [\"x\", \"y\"])\n[person(name = \"Ada\", age = 36)]\n",
        "stderr: {}",
        stderr(&out)
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn a_copy_leaves_its_source_and_a_read_is_a_copy() {
    let src = with_decls(
        POINT,
        "var p = point(x = 1.0, y = 2.0)\nlet q = p with (x = 9.0)\np.y = 7.0\nprint(p)\nprint(q)",
    );
    let Some(out) = mz_run_src("copy", &src) else {
        return;
    };
    assert_eq!(
        stdout(&out),
        "point(x = 1.0, y = 7.0)\npoint(x = 9.0, y = 2.0)\n",
        "stderr: {}",
        stderr(&out)
    );
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn a_field_assignment_that_breaks_an_always_clause_traps_with_exit_101() {
    let src = with_decls(
        "  record span
    field low: int
    field high: int

    contract
      always low <= high
    end
  end
",
        "var s = span(low = 2, high = 9)\ns.low = 20\nprint(s)",
    );
    let Some(out) = mz_run_src("field_trap", &src) else {
        return;
    };
    assert_eq!(out.status.code(), Some(101), "stderr: {}", stderr(&out));
    assert!(stdout(&out).is_empty(), "nothing is printed past the trap");
    let err = stderr(&out);
    assert!(
        err.contains("MZ0991") && err.contains("record `span` breaks `always low <= high`"),
        "{err}"
    );
    assert!(
        err.contains("s.low = 20"),
        "the trap names the assignment: {err}"
    );
}

#[test]
fn a_with_that_breaks_an_always_clause_traps_with_exit_101() {
    let src = with_decls(
        "  record span
    field low: int
    field high: int

    contract
      always low <= high
    end
  end
",
        "let s = span(low = 2, high = 9)\nprint(s with (low = 20))",
    );
    let Some(out) = mz_run_src("with_trap", &src) else {
        return;
    };
    assert_eq!(out.status.code(), Some(101), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("MZ0991"), "{}", stderr(&out));
}

#[test]
fn a_record_is_not_a_map_key() {
    let src = with_decls(POINT, "let m = [point(x = 1.0, y = 2.0): 1]\nprint(m)");
    let report = errors(&src);
    assert!(
        report.iter().any(|d| d.code == "MZ0964"),
        "a record is not an ordered key: {report:#?}"
    );
}
