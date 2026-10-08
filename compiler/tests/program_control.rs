//! RFC-0013 §7 (LANGUAGE-TRACKER C4): control flow in function bodies — `else when`,
//! `match` over an enum, an int, a text or a bool, `when` and `match` used as values,
//! `for each` over `range(a, to = b)`, `while`, `break`, `continue` and early `return`, and the
//! `enum`s a `match` is over.
//!
//! The four kinds of test `tests/program.rs` has for the foundation slice: the parse; the
//! check, one test per diagnostic (`MZ0930`–`MZ0936`, and the reused codes these forms
//! reach), with each `exact` fix applied and the result checked again; the lowered text;
//! and `mz run` end to end, with exact standard output. The end-to-end tests need `cargo`
//! on the path; without it they say so on standard error and pass, and CI's `lowering`
//! job runs `examples/control.mz` through the shipped binary regardless.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Severity, Span};
use mzizi_lang_compiler::expr::ExprKind;
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

/// `body` as the body of `fn main`, in a program with two enums and a helper.
fn wrap(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!(
        "program t\n\n  enum shape\n    circle\n    square\n    triangle\n  end\n\n  enum light\n    red\n    amber\n    green\n  end\n\n  fn main\n{body}  end fn main\n\n  fn twice(n: int): int\n    return n * 2\n  end fn twice\n\nend program t\n"
    )
}

/// A program whose `fn f(s: shape): int` has `body`, called from `main`.
fn with_f(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!(
        "program t\n\n  enum shape\n    circle\n    square\n    triangle\n  end\n\n  fn main\n    print(f(circle))\n  end fn main\n\n  fn f(s: shape): int\n{body}  end fn f\n\nend program t\n"
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

/// [`one`], then apply its `exact` fix and check the result has no errors and holds `want`.
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

/// Apply one fix, whatever its confidence, by its line and column span.
fn apply(src: &str, at: Span, replace: &str) -> String {
    let lines: Vec<&str> = src.split_inclusive('\n').collect();
    let offset = |line: u32, col: u32| -> usize {
        let before: usize = lines[..(line as usize - 1).min(lines.len())]
            .iter()
            .map(|l| l.len())
            .sum();
        let in_line: usize = lines.get(line as usize - 1).map_or(0, |l| {
            l.chars().take(col as usize - 1).map(char::len_utf8).sum()
        });
        before + in_line
    };
    let (a, b) = (
        offset(at.start_line, at.start_col),
        offset(at.end_line, at.end_col),
    );
    format!("{}{replace}{}", &src[..a], &src[b..])
}

// ------------------------------------------------------------------- parse

#[test]
fn the_control_example_parses_and_checks_clean() {
    let src = example("control.mz");
    clean(&src);
    let p = program(&src);
    assert_eq!(p.enums.len(), 1);
    assert_eq!(p.enums[0].name, "shape");
    let variants: Vec<&str> = p.enums[0]
        .variants
        .iter()
        .map(|v| v.name.as_str())
        .collect();
    assert_eq!(variants, ["circle", "square", "triangle"]);
    let kinds: Vec<&str> = p
        .fns
        .iter()
        .flat_map(|f| f.body.iter())
        .map(|s| match &s.kind {
            StmtKind::For { .. } => "for",
            StmtKind::While { .. } => "while",
            StmtKind::Match { .. } => "match",
            StmtKind::When { .. } => "when",
            StmtKind::Return(_) => "return",
            StmtKind::Bind { .. } => "bind",
            StmtKind::Assign { .. } | StmtKind::IndexAssign { .. } => "assign",
            StmtKind::Expr(_) => "expr",
            StmtKind::Break | StmtKind::Continue => "jump",
        })
        .collect();
    for k in ["for", "while", "match", "when", "return"] {
        assert!(kinds.contains(&k), "no `{k}` in {kinds:?}");
    }
}

#[test]
fn an_else_when_chain_is_one_flat_when_sharing_one_end() {
    let p = program(&wrap(
        "let n = 2\nwhen n is 1\n  print(\"one\")\nelse when n is 2\n  print(\"two\")\nelse when n is 3\n  print(\"three\")\nelse\n  print(\"many\")\nend",
    ));
    let s = &p.fns[0].body[1];
    let StmtKind::When {
        else_whens,
        otherwise,
        ..
    } = &s.kind
    else {
        panic!("{s:?}")
    };
    assert_eq!(else_whens.len(), 2);
    assert!(otherwise.is_some());
    // The `when` covers its lines through the shared `end`.
    assert_eq!(s.last_line, s.span.start_line + 8);
}

#[test]
fn a_long_else_when_chain_costs_no_nesting() {
    // 200 links, far past the 32 levels of the nesting cap: a chain is flat.
    let mut body = String::from("let n = 7\nwhen n is 0\n  print(0)\n");
    for k in 1..200 {
        body.push_str(&format!("else when n is {k}\n  print({k})\n"));
    }
    body.push_str("end");
    clean(&wrap(&body));
}

#[test]
fn when_and_match_as_values_are_expressions_spanning_their_block() {
    let p = program(&wrap(
        "let n = 3\nlet size = when n < 2\n  \"small\"\nelse when n < 5\n  \"medium\"\nelse\n  \"large\"\nend\nlet s = match n\n  case 0\n    circle\n  else\n    square\nend\nprint(\"{size} {s}\")",
    ));
    let body = &p.fns[0].body;
    let StmtKind::Bind { value, .. } = &body[1].kind else {
        panic!()
    };
    let ExprKind::When { arms, otherwise } = &value.kind else {
        panic!("{value:?}")
    };
    assert_eq!(arms.len(), 2);
    assert!(otherwise.is_some());
    // The binding's statement runs to its `end`, so a fix that deletes it deletes all of it.
    assert_eq!(body[1].last_line, body[1].span.start_line + 6);
    let StmtKind::Bind { value, .. } = &body[2].kind else {
        panic!()
    };
    assert!(matches!(value.kind, ExprKind::Match { .. }), "{value:?}");
    clean(&wrap(
        "let n = 3\nlet size = when n < 2\n  \"small\"\nelse\n  \"large\"\nend\nprint(size)",
    ));
}

#[test]
fn a_variant_is_bare_or_named_with_its_enum() {
    clean(&wrap(
        "let a = circle\nlet b = shape.square\nprint(\"{a} {b} {a < b} {a is shape.circle}\")",
    ));
}

// ------------------------------------------------------------------- MZ0930 – MZ0937

#[test]
fn mz0930_a_match_on_an_enum_names_the_variants_it_misses() {
    let d = one(
        &with_f("match s\n  case circle\n    return 0\nend\nreturn 1"),
        "MZ0930",
    );
    assert!(d.say.contains("`square`, `triangle`"), "{}", d.say);
    assert!(d.fix.is_none(), "the bodies are the author's");
    // An `else` covers what is left.
    clean(&with_f(
        "match s\n  case circle\n    return 0\n  else\n    return 1\nend",
    ));
}

#[test]
fn mz0930_a_match_on_an_int_or_a_text_ends_with_else() {
    let d = one(
        &wrap("let n = 1\nmatch n\n  case 1\n    print(1)\nend"),
        "MZ0930",
    );
    assert!(d.say.contains("end it with `else`"), "{}", d.say);
    one(
        &wrap("let t = \"a\"\nmatch t\n  case \"a\"\n    print(1)\nend"),
        "MZ0930",
    );
    // A bool's two values cover it.
    one(
        &wrap("let b = true\nmatch b\n  case true\n    print(1)\nend"),
        "MZ0930",
    );
    clean(&wrap(
        "let b = true\nmatch b\n  case true\n    print(1)\n  case false\n    print(0)\nend",
    ));
}

#[test]
fn mz0931_a_case_that_can_never_run_is_deleted() {
    // A value listed twice: the second goes.
    fixed_by(
        &wrap("let n = 1\nmatch n\n  case 1 2 1\n    print(1)\n  else\n    print(0)\nend"),
        "MZ0931",
        "      case 1 2\n",
    );
    // A case all of whose values an earlier case takes: the whole case goes.
    let after = fixed_by(
        &wrap(
            "let n = 1\nmatch n\n  case 1\n    print(1)\n  case 1\n    print(2)\n  else\n    print(0)\nend",
        ),
        "MZ0931",
        "      case 1\n        print(1)\n      else\n",
    );
    assert!(!after.contains("print(2)"));
    // A case after the `else`.
    let after = fixed_by(
        &wrap(
            "let n = 1\nmatch n\n  case 1\n    print(1)\n  else\n    print(0)\n  case 2\n    print(2)\nend",
        ),
        "MZ0931",
        "      else\n        print(0)\n    end\n",
    );
    assert!(!after.contains("case 2"));
    // An `else` on a match that already covers every variant.
    let after = fixed_by(
        &with_f(
            "match s\n  case circle\n    return 0\n  case square triangle\n    return 1\n  else\n    return 2\nend",
        ),
        "MZ0931",
        "case square triangle\n        return 1\n    end\n",
    );
    assert!(!after.contains("return 2"));
}

#[test]
fn mz0932_a_block_used_as_a_value_where_it_cannot_be() {
    // Inside a larger expression: one diagnostic, and the block's lines are skipped.
    let d = one(
        &wrap("let n = 1\nlet x = 1 + when n is 1\n  2\nelse\n  3\nend\nprint(x)"),
        "MZ0932",
    );
    assert!(
        d.say.contains("never part of a larger expression"),
        "{}",
        d.say
    );
    one(
        &wrap("let n = 1\nprint(match n\n  case 1\n    \"a\"\n  else\n    \"b\"\nend)"),
        "MZ0932",
    );
    // No `else`.
    let d = one(
        &wrap("let n = 1\nlet x = when n is 1\n  2\nend\nprint(x)"),
        "MZ0932",
    );
    assert!(d.say.contains("needs an `else`"), "{}", d.say);
    // A branch of two lines, and a branch that is a statement.
    one(
        &wrap("let n = 1\nlet x = when n is 1\n  2\n  3\nelse\n  4\nend\nprint(x)"),
        "MZ0932",
    );
    one(
        &wrap("let n = 1\nlet x = when n is 1\n  let y = 2\nelse\n  4\nend\nprint(x)"),
        "MZ0932",
    );
    // Branches of two types, and a branch that returns nothing.
    let d = one(
        &wrap("let n = 1\nlet x = when n is 1\n  2\nelse\n  \"two\"\nend\nprint(x)"),
        "MZ0932",
    );
    assert!(d.say.contains("the first is int"), "{}", d.say);
    one(
        &wrap("let n = 1\nlet x = when n is 1\n  print(1)\nelse\n  2\nend\nprint(x)"),
        "MZ0932",
    );
    // A `match` used as a value covers every variant.
    let d = one(
        &wrap("let s = circle\nlet n = match s\n  case circle\n    0\nend\nprint(n)"),
        "MZ0932",
    );
    assert!(d.say.contains("`square`, `triangle`"), "{}", d.say);
}

#[test]
fn mz0932_blocks_are_values_in_four_places_only() {
    // The value of a `let`, a `var`, an assignment and a `return`.
    clean(&wrap(
        "let n = 1\nvar a = when n is 1\n  1\nelse\n  2\nend\na = match n\n  case 1\n    3\n  else\n    4\nend\nprint(a)",
    ));
    clean(&with_f(
        "return match s\n  case circle\n    0\n  case square\n    4\n  case triangle\n    3\nend",
    ));
}

#[test]
fn mz0933_conditionals_from_other_languages() {
    let chain = |second: &str| {
        wrap(&format!(
            "let n = 2\nwhen n is 1\n  print(1)\n{second} n is 2\n  print(2)\nend"
        ))
    };
    fixed_by(&chain("elif"), "MZ0933", "    else when n is 2\n");
    fixed_by(&chain("else if"), "MZ0933", "    else when n is 2\n");
    let m = |head: &str, other: &str| {
        wrap(&format!(
            "let n = 2\n{head} n\n  case 1\n    print(1)\n  {other}\n    print(0)\nend"
        ))
    };
    fixed_by(&m("switch", "else"), "MZ0933", "    match n\n");
    fixed_by(
        &m("match", "default:"),
        "MZ0933",
        "      else\n        print(0)",
    );
    fixed_by(
        &m("match", "default"),
        "MZ0933",
        "      else\n        print(0)",
    );
    fixed_by(
        &m("match", "case _"),
        "MZ0933",
        "      else\n        print(0)",
    );
    fixed_by(
        &m("match", "_ =>"),
        "MZ0933",
        "      else\n        print(0)",
    );
    // Rust's arm with its branch on the same line: the fix gives it a line of its own.
    fixed_by(
        &wrap("let n = 2\nmatch n\n  case 1\n    print(1)\n  _ => print(0)\nend"),
        "MZ0933",
        "      else\n        print(0)\n",
    );
}

#[test]
fn range_labels_its_end_and_the_positional_form_waits_for_labels() {
    // RFC-0013 §6.5 and §7.3 write `range(a, to = b)`. The positional `range(a, b)` is
    // still read, and lowers the same, until §6.5's labels are built with `MZ0927`.
    clean(&wrap("for each i in range(0, to = 3)\n  print(i)\nend"));
    clean(&wrap("for each i in range(0, 3)\n  print(i)\nend"));
    let labelled = main_rs(&wrap("for each i in range(0, to = 3)\n  print(i)\nend"));
    let positional = main_rs(&wrap("for each i in range(0, 3)\n  print(i)\nend"));
    assert_eq!(labelled, positional);
    assert!(labelled.contains("for i in 0i64..3i64 {"), "{labelled}");
    // Another label is a named argument, which calls do not take yet.
    one(
        &wrap("for each i in range(0, stop = 3)\n  print(i)\nend"),
        "MZ0905",
    );
}

#[test]
fn mz0934_loops_from_other_languages() {
    fixed_by(
        &wrap("for i in range(0, to = 3)\n  print(i)\nend"),
        "MZ0934",
        "    for each i in range(0, to = 3)\n",
    );
    fixed_by(
        &wrap("for each i in range(3)\n  print(i)\nend"),
        "MZ0934",
        "for each i in range(0, to = 3)",
    );
    fixed_by(
        &wrap("for (const i of range(0, to = 3))\n  print(i)\nend"),
        "MZ0934",
        "    for each i in range(0, to = 3)\n",
    );
    fixed_by(
        &wrap("var n = 0\nloop\n  n = n + 1\n  when n > 3\n    break\n  end\nend\nprint(n)"),
        "MZ0934",
        "    while true\n",
    );
    // No fix: the rewrite is the author's.
    let d = one(
        &wrap("for (i = 0; i < 3; i++)\n  print(i)\nend\nprint(1)"),
        "MZ0934",
    );
    assert!(d.fix.is_none());
    assert!(d.say.contains("range(a, to = b)"), "{}", d.say);
    let d = one(&wrap("do\n  print(1)\nend\nprint(2)"), "MZ0934");
    assert!(d.fix.is_none());
}

#[test]
fn mz0935_break_and_continue_outside_a_loop() {
    let d = one(&wrap("break"), "MZ0935");
    assert!(d.fix.is_none());
    one(&wrap("let n = 1\nwhen n is 1\n  continue\nend"), "MZ0935");
    // Inside a `when` inside a loop, both are fine.
    clean(&wrap(
        "for each i in range(0, to = 5)\n  when i is 1\n    continue\n  else when i is 3\n    break\n  end\n  print(i)\nend",
    ));
}

#[test]
fn mz0936_an_else_when_chain_over_one_enums_variants_is_a_match() {
    let src = wrap(
        "let s = square\nwhen s is circle\n  print(0)\nelse when s is square\n  let k = 4\n  print(k)\nelse\n  print(3)\nend",
    );
    let d = one(&src, "MZ0936");
    let fix = d.fix.expect("a guess fix");
    assert_eq!(fix.confidence, Confidence::Guess);
    assert_eq!(
        fix.replace,
        "match s\n      case circle\n        print(0)\n      case square\n        let k = 4\n        print(k)\n      else\n        print(3)\n    end\n"
    );
    // The guess applied gives a program that checks clean.
    clean(&apply(&src, fix.span, &fix.replace));
    // A chain without `else` that misses a variant becomes a `match` that is `MZ0930`:
    // that is why the fix is a guess.
    let src = wrap(
        "let s = square\nwhen s is circle\n  print(0)\nelse when s is shape.square\n  print(4)\nend",
    );
    let fix = one(&src, "MZ0936").fix.expect("a fix");
    one(&apply(&src, fix.span, &fix.replace), "MZ0930");
    // A single variant test, with or without a plain `else`, stays legal, and so does a
    // chain that tests anything else.
    clean(&wrap(
        "let s = square\nwhen s is circle\n  print(0)\nelse\n  print(1)\nend",
    ));
    clean(&wrap(
        "let s = square\nlet n = 1\nwhen s is circle\n  print(0)\nelse when n is 1\n  print(1)\nend",
    ));
}

#[test]
fn mz0937_a_trailing_colon_on_a_control_line() {
    fixed_by(
        &wrap("var n = 0\nwhile n < 3:\n  n = n + 1\nend\nprint(n)"),
        "MZ0937",
        "    while n < 3\n",
    );
    fixed_by(
        &wrap("let n = 1\nmatch n\n  case 1:\n    print(1)\n  else\n    print(0)\nend"),
        "MZ0937",
        "      case 1\n",
    );
}

// ------------------------------------------------------------------- reused codes

#[test]
fn early_return_paths_through_match_and_while_true() {
    // An exhaustive `match` whose every case returns, and a `while true` no `break`
    // leaves, both end every path: no `MZ0906`.
    clean(&with_f(
        "match s\n  case circle\n    return 0\n  case square\n    return 4\n  case triangle\n    return 3\nend",
    ));
    clean(&with_f(
        "var n = 0\nwhile true\n  n = n + 1\n  when n > 9\n    return n\n  end\nend",
    ));
    // A `while true` with a `break` can fall out of the loop.
    let d = one(
        &with_f("var n = 0\nwhile true\n  n = n + 1\n  when n > 9\n    break\n  end\nend"),
        "MZ0906",
    );
    assert!(d.say.contains("end fn f"), "{}", d.say);
    // A `while` over a condition can always end, as can a `for each`.
    one(&with_f("while true is false\n  return 1\nend"), "MZ0906");
    one(
        &with_f("for each i in range(0, to = 3)\n  return i\nend"),
        "MZ0906",
    );
}

#[test]
fn mz0907_a_line_after_break_or_continue_never_runs() {
    fixed_by(
        &wrap("for each i in range(0, to = 3)\n  break\n  print(i)\nend"),
        "MZ0907",
        "      break\n    end\n",
    );
    fixed_by(
        &wrap(
            "for each i in range(0, to = 3)\n  when i is 1\n    continue\n    print(i)\n  end\nend",
        ),
        "MZ0907",
        "        continue\n      end\n",
    );
}

#[test]
fn a_loop_binding_cannot_be_assigned_or_shadowed() {
    let d = one(
        &wrap("for each i in range(0, to = 3)\n  i = 2\n  print(i)\nend"),
        "MZ0922",
    );
    assert!(d.fix.is_none());
    one(
        &wrap("let i = 1\nfor each i in range(0, to = 3)\n  print(i)\nend"),
        "MZ0921",
    );
    // The binding ends with the loop.
    one(
        &wrap("for each i in range(0, to = 3)\n  print(i)\nend\nprint(i)"),
        "MZ0920",
    );
}

#[test]
fn mz0708_a_variant_the_enum_does_not_have() {
    // Against the other side of `is`, and in a `case`, the nearest variant is the fix.
    fixed_by(
        &wrap("let s = circle\nwhen s is squar\n  print(1)\nend"),
        "MZ0708",
        "when s is square",
    );
    fixed_by(
        &with_f(
            "match s\n  case circle\n    return 0\n  case squre\n    return 4\n  case triangle\n    return 3\nend",
        ),
        "MZ0708",
        "case square",
    );
    fixed_by(
        &wrap("let s = shape.circel\nprint(s)"),
        "MZ0708",
        "shape.circle",
    );
    // A variant two enums share: read against `is`'s other side, it needs nothing; on its
    // own it needs its enum.
    let two = |body: &str| {
        format!(
            "program t\n\n  enum a\n    on\n    off\n  end\n\n  enum b\n    on\n    idle\n  end\n\n  fn main\n{body}  end fn main\n\nend program t\n"
        )
    };
    clean(&two("    let x = a.off\n    print(x is on)\n"));
    let d = one(&two("    let x = on\n    print(x)\n"), "MZ0708");
    let fix = d.fix.expect("a guess");
    assert_eq!(
        (fix.confidence, fix.replace.as_str()),
        (Confidence::Guess, "a.on")
    );
}

#[test]
fn enum_declarations_are_checked() {
    let decl = |e: &str| {
        format!("program t\n\n  {e}\n\n  fn main\n    print(1)\n  end fn main\n\nend program t\n")
    };
    // A variant listed twice: the line goes.
    fixed_by(
        &decl("enum shape\n    circle\n    circle\n    square\n  end"),
        "MZ0704",
        "    circle\n    square\n",
    );
    one(
        &decl("enum shape\n    circle\n  end\n\n  enum shape\n    square\n  end"),
        "MZ0704",
    );
    one(&decl("enum print\n    a\n  end"), "MZ0921");
    one(&decl("enum shape\n    while\n  end"), "MZ0921");
    one(&decl("enum shape\n    mz_a\n  end"), "MZ0921");
    // `ok` and `error` may name a variant (RFC-0013 §1).
    clean(&decl("enum status\n    ok\n    error\n  end"));
    // A variant's columns are built (RFC-0013 §12.1, C9): every variant has each one.
    clean(&decl(
        "enum shape\n    circle label \"round\"\n    square label \"four sides\"\n  end",
    ));
    one(
        &decl("enum shape\n    circle label \"round\"\n    square\n  end"),
        "MZ0303",
    );
    // A binding cannot take an enum's or a variant's name.
    one(&wrap("let circle = 1\nprint(circle)"), "MZ0921");
    one(&wrap("let shape = 1\nprint(shape)"), "MZ0921");
    // An `enum` closes with a bare `end`.
    fixed_by(
        &decl("enum shape\n    circle\n  end enum shape"),
        "MZ0206",
        "    circle\n  end\n",
    );
}

#[test]
fn conditions_and_sources_have_their_types() {
    let d = one(&wrap("let n = 3\nwhile n\n  print(n)\nend"), "MZ0712");
    assert_eq!(d.fix.expect("a guess").replace, "n is not 0");
    let d = one(
        &wrap("let n = 3\nfor each i in n\n  print(i)\nend"),
        "MZ0711",
    );
    let fix = d.fix.expect("a guess");
    assert_eq!(
        (fix.confidence, fix.replace.as_str()),
        (Confidence::Guess, "range(0, to = n)")
    );
    one(
        &wrap("for each i in range(0, to = \"3\")\n  print(i)\nend"),
        "MZ0905",
    );
    // `range` outside a `for each` is a list (C7, RFC-0013 §9).
    assert!(errors(&wrap("let r = range(0, to = 3)\nprint(r)")).is_empty());
    // A case of the wrong type, and a case that is not a literal.
    one(
        &wrap("let n = 1\nmatch n\n  case \"a\"\n    print(1)\n  else\n    print(0)\nend"),
        "MZ0912",
    );
    one(
        &wrap("let n = 1\nmatch n\n  case twice(1)\n    print(1)\n  else\n    print(0)\nend"),
        "MZ0917",
    );
}

#[test]
fn an_unclosed_block_is_one_mz0204() {
    one(&wrap("var n = 0\nwhile n < 3\n  n = n + 1\n"), "MZ0204");
    one(
        &wrap("let n = 0\nmatch n\n  case 0\n    print(0)\n  else\n    print(1)\n"),
        "MZ0204",
    );
}

// ------------------------------------------------------------------- lowering

fn main_rs(src: &str) -> String {
    let p = program(src);
    let package = mzizi_lang_compiler::run::lower(&p, "control.mz");
    package.files[1].1.clone()
}

#[test]
fn the_lowering_of_control_flow_is_plain_rust_with_no_panics() {
    let main = main_rs(&example("control.mz"));
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
    for want in [
        "#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]\nenum Shape {\n    Circle,\n    Square,\n    Triangle,\n}",
        "            Shape::Circle => String::from(\"circle\"),",
        "    for n in 1i64..16i64 {",
        "    } else if mz_rem(n, 3i64, &MZ_AT_2) == 0i64 {",
        "    return match i { 0i64 => Shape::Circle, 1i64 => Shape::Square, _ => Shape::Triangle };",
        "    let label: String = if s == Shape::Circle { String::from(\"round\") } else { String::from(\"angular\") };",
        "    loop {",
        "            break;",
        "            continue;",
        "    while n != 1i64 {",
        "        6i64 | 7i64 => {",
        "    match (lang.clone()).as_str() {",
        "        \"fr\" => {",
    ] {
        assert!(main.contains(want), "expected\n{want}\nin\n{main}");
    }
    // An exhaustive `match` on an enum has no `_` arm, so `rustc` checks it again.
    let corners = main
        .split("fn corners(")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .expect("fn corners");
    assert!(!corners.contains("_ =>"), "{corners}");
}

#[test]
fn an_enum_is_emitted_clear_of_rusts_prelude_and_of_its_own_names() {
    let src = "program t\n\n  enum vec\n    a1\n    a_1\n  end\n\n  fn main\n    let v = a1\n    print(\"{v} {a_1}\")\n  end fn main\n\nend program t\n";
    clean(src);
    let main = main_rs(src);
    assert!(
        main.contains("enum MzUserVec {\n    A1,\n    MzUser2A1,\n}"),
        "{main}"
    );
    assert!(main.contains("let v: MzUserVec = MzUserVec::A1;"), "{main}");
}

// ------------------------------------------------------------------- mz run

/// Run the shipped `mz` binary. `None` when `cargo` is not on the path, which these tests
/// report on standard error rather than pass in silence.
fn mz_run(path: &str) -> Option<std::process::Output> {
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return None;
    }
    Some(
        Command::new(env!("CARGO_BIN_EXE_mz"))
            .args(["run", path])
            .output()
            .expect("mz runs"),
    )
}

#[test]
fn mz_run_control_prints_fizzbuzz_and_the_rest_exactly() {
    let path = root().join("examples/control.mz");
    let Some(out) = mz_run(path.to_str().expect("utf-8 path")) else {
        return;
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let fizzbuzz = "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n";
    let rest = "circle has 0 corners and is round\nsquare has 4 corners and is angular\ntriangle has 3 corners and is angular\nfirst multiple of 7 above 50 is 56\nthe odd numbers below 10 sum to 25\ncollatz(27) takes 111 steps\n6 is a weekend day\n9 is not a day of the week\nbonjour\nhello\n";
    assert_eq!(
        stdout,
        format!("{fizzbuzz}{rest}"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    let expected = std::fs::read_to_string(root().join("examples/control.expected"))
        .expect("control.expected");
    assert_eq!(stdout, expected);
}

#[test]
fn mz_run_nested_loops_break_the_inner_one_and_enums_order_by_declaration() {
    let src = "program loops\n\n  enum light\n    red\n    amber\n    green\n  end\n\n  fn main\n    for each i in range(0, to = 3)\n      var j = 0\n      while true\n        j = j + 1\n        when j > i\n          break\n        end\n      end\n      print(\"{i}:{j}\")\n    end\n    let l = amber\n    print(\"{red < l} {l < green} {l is amber} {light.green is l}\")\n    let word = match l < green\n      case true\n        \"before green\"\n      case false\n        \"green\"\n    end\n    print(word)\n    print(sign(-4))\n    print(sign(0))\n    print(sign(9))\n  end fn main\n\n  fn sign(n: int): text\n    return when n < 0\n      \"negative\"\n    else when n is 0\n      \"zero\"\n    else\n      \"positive\"\n    end\n  end fn sign\n\nend program loops\n";
    clean(src);
    let dir = std::env::temp_dir().join(format!("mz-control-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("loops.mz");
    std::fs::write(&path, src).expect("write");
    let Some(out) = mz_run(path.to_str().expect("utf-8 path")) else {
        return;
    };
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "0:1\n1:2\n2:3\ntrue true true false\nbefore green\nnegative\nzero\npositive\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Write `src` to a fresh file in this test process's temp directory, under `name`.
fn scratch(name: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mz-control-traps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join(name);
    std::fs::write(&path, src).expect("write");
    path
}

#[test]
fn mz_run_a_trap_inside_a_loop_or_a_match_is_mz0991_with_status_101() {
    // Overflow on a `var` doubled in a `while true` that only a trap leaves.
    let src = "program grow\n\n  fn main\n    var n = 1\n    var steps = 0\n    while true\n      n = n * 2\n      steps = steps + 1\n      when steps is 60\n        print(\"halfway\")\n      end\n    end\n  end fn main\n\nend program grow\n";
    clean(src);
    let path = scratch("grow.mz", src);
    let Some(out) = mz_run(path.to_str().expect("utf-8 path")) else {
        return;
    };
    assert_eq!(String::from_utf8_lossy(&out.stdout), "halfway\n");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "mz: trap MZ0991 at grow.mz:7:11: integer overflow in `n * 2`\n"
    );
    assert_eq!(out.status.code(), Some(101));

    // Division by zero in a `match` used as a value, reached from a `for each`.
    let src = "program share\n\n  fn main\n    for each i in range(0, to = 3)\n      print(share(2 - i))\n    end\n  end fn main\n\n  fn share(n: int): int\n    return match n\n      case 1\n        12 / n\n      else\n        12 / n\n    end\n  end fn share\n\nend program share\n";
    clean(src);
    let path = scratch("share.mz", src);
    let Some(out) = mz_run(path.to_str().expect("utf-8 path")) else {
        return;
    };
    assert_eq!(String::from_utf8_lossy(&out.stdout), "6\n12\n");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "mz: trap MZ0991 at share.mz:14:9: integer division by zero in `12 / n`\n"
    );
    assert_eq!(out.status.code(), Some(101));
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn deep_nesting_of_loops_and_matches_is_one_mz0411() {
    // 40 levels, past the program's cap of 32: one diagnostic for the whole tower.
    let mut body = String::new();
    for d in 0..40 {
        let pad = "  ".repeat(d);
        match d % 3 {
            0 => body.push_str(&format!("{pad}while true\n")),
            1 => body.push_str(&format!("{pad}for each i{d} in range(0, to = 2)\n")),
            _ => body.push_str(&format!(
                "{pad}match {d}\n{pad}  case 1\n{pad}    print(1)\n{pad}  else\n"
            )),
        }
    }
    body.push_str(&format!("{}break\n", "  ".repeat(40)));
    for d in (0..40).rev() {
        body.push_str(&format!("{}end\n", "  ".repeat(d)));
    }
    let src = wrap(&body);
    let errors = errors(&src);
    let deep: Vec<_> = errors.iter().filter(|d| d.code == "MZ0411").collect();
    assert_eq!(deep.len(), 1, "{errors:#?}");
}

// ------------------------------------------------------------------- found in review

#[test]
fn a_misspelt_case_does_not_make_the_line_after_the_match_unreachable() {
    // The `match` may miss `triangle`, so `return 3` is needed: no `MZ0907`, whose
    // `exact` fix would have deleted it, and no `MZ0930` for the same mistake.
    let src =
        with_f("match s\n  case circle\n    return 1\n  case squar\n    return 2\nend\nreturn 3");
    one(&src, "MZ0708");
    let after = apply_exact_fixes(&src, &check(&src, "t.mz"));
    assert!(
        after.contains("case square") && after.contains("return 3"),
        "{after}"
    );
    // With the variant spelt right, the next check names what the `match` misses.
    let d = one(&after, "MZ0930");
    assert!(d.say.contains("`triangle`"), "{}", d.say);
}

#[test]
fn an_enum_or_variant_whose_pascal_case_is_self_is_emitted_clear_of_it() {
    let src = "program t\n\n  enum self_\n    self_\n    b\n  end\n\n  fn main\n    let x: self_ = b\n    print(x)\n  end fn main\n\nend program t\n";
    clean(src);
    let main = main_rs(src);
    assert!(
        main.contains("enum MzUserSelf {\n    MzUser1Self,\n    B,\n}"),
        "{main}"
    );
    assert!(
        main.contains("let x: MzUserSelf = MzUserSelf::B;"),
        "{main}"
    );
    assert!(!main.contains("enum Self"), "{main}");
}

#[test]
fn a_word_that_opens_a_block_elsewhere_is_a_name_when_used_as_one() {
    // `do` is a binding here, so the skipped C-style `for` ends at its own `end`, and the
    // rest of the program reads as written: one `MZ0934`, nothing after it.
    one(
        &wrap("var do = 0\nfor (i = 0; i < 3; i++)\n  do = do + 1\nend\nprint(do)"),
        "MZ0934",
    );
}

#[test]
fn a_bare_variant_reads_against_the_type_expected_where_it_stands() {
    let src = "program t\n\n  enum level\n    low\n    high\n  end\n\n  enum other\n    low\n    mid\n  end\n\n  fn main\n    let a: level = low\n    var b: level = high\n    b = low\n    print(\"{a < high} {b is low} {lowest() is a} {name(low)}\")\n  end fn main\n\n  fn lowest: level\n    return low\n  end fn lowest\n\n  fn name(l: level): text\n    return \"{l}\"\n  end fn name\n\nend program t\n";
    clean(src);
    let main = main_rs(src);
    for want in [
        "let a: Level = Level::Low;",
        "b = Level::Low;",
        "return Level::Low;",
        "name(Level::Low)",
        "a < Level::High",
    ] {
        assert!(main.contains(want), "expected `{want}` in\n{main}");
    }
    // The branches of a block used as a value read against the same expected type.
    let src = src.replace(
        "    let a: level = low\n",
        "    let a: level = when 1 < 2\n      low\n    else\n      high\n    end\n",
    );
    clean(&src);
    assert!(
        main_rs(&src)
            .contains("let a: Level = if 1i64 < 2i64 { Level::Low } else { Level::High };"),
        "{}",
        main_rs(&src)
    );
}

/// A program whose `fn main` holds `body`, followed by a second `fn` the recovery must
/// not swallow.
fn then_other(body: &str) -> String {
    let body: String = body.lines().map(|l| format!("    {l}\n")).collect();
    format!(
        "program t\n\n  fn main\n{body}  end fn main\n\n  fn other: int\n    return 1\n  end fn other\n\nend program t\n"
    )
}

#[test]
fn recovery_from_a_foreign_loop_stops_at_the_end_of_its_function() {
    // `do … while` ends at its `while` line, not at the function's `end fn`.
    one(
        &then_other("var x = 0\ndo\n  x = x + 1\nwhile x < 3\nprint(x)"),
        "MZ0934",
    );
    // A C-style loop written with braces has no `end`: the skip stops at `end fn`.
    one(
        &then_other("for (i = 0; i < 3; i++) {\n  print(i)\n}\nprint(1)"),
        "MZ0934",
    );
}

#[test]
fn a_stray_break_is_one_mz0935_and_deletes_nothing() {
    let d = one(&wrap("break\nprint(1)"), "MZ0935");
    assert!(d.fix.is_none());
}

#[test]
fn typescripts_for_in_iterates_keys_so_its_rewrite_is_a_guess() {
    let d = one(
        &wrap("for (const i in range(0, to = 3))\n  print(i)\nend"),
        "MZ0934",
    );
    assert_eq!(d.fix.expect("a fix").confidence, Confidence::Guess);
}

#[test]
fn an_unclosed_when_inside_a_case_names_no_end_fn() {
    let d = one(
        &wrap(
            "let n = 1\nmatch n\n  case 1\n    when n is 1\n      print(1)\n  case 2\n    print(2)\n  else\n    print(0)\nend",
        ),
        "MZ0204",
    );
    assert!(!d.say.contains("end fn"), "{}", d.say);
}

#[test]
fn an_elif_in_a_match_is_one_mz0917_however_long_its_body() {
    one(
        &wrap(
            "let n = 1\nmatch n\n  case 1\n    print(1)\n  elif n is 2\n    print(2)\n    print(3)\n    print(4)\n  else\n    print(0)\nend",
        ),
        "MZ0917",
    );
}

#[test]
fn gos_short_binding_takes_a_block_used_as_a_value() {
    fixed_by(
        &wrap("x := when 1 < 2\n  3\nelse\n  4\nend\nprint(x)"),
        "MZ0925",
        "let x = when 1 < 2",
    );
    // The block is checked as the fix leaves it: a missing `else` is reported now.
    let errors = errors(&wrap("x := when 1 < 2\n  3\nend\nprint(x)"));
    let codes: Vec<&str> = errors.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["MZ0925", "MZ0932"], "{errors:#?}");
}

// ------------------------------------------------------------------- found in the second review

#[test]
fn a_loop_binding_read_after_its_loop_is_mz0920_with_no_fix() {
    // A `var i` before the loop would clash with the loop's own `i` (`MZ0921`).
    let d = one(
        &wrap("for each i in range(0, to = 3)\n  print(i)\nend\nprint(i)"),
        "MZ0920",
    );
    assert!(d.fix.is_none(), "{d:#?}");
    assert!(d.say.contains("`for each` binding"), "{d:#?}");
}

#[test]
fn a_binding_named_default_is_a_branch_value_not_an_else() {
    clean(&wrap(
        "let default = 5\nlet x = twice(1)\nlet y = match x\n  case 1\n    default\n  else\n    0\nend\nprint(y)",
    ));
    // JavaScript's `default:` keeps its colon, and is still `MZ0933`.
    fixed_by(
        &wrap(
            "let x = twice(1)\nlet y = match x\n  case 1\n    5\n  default:\n    0\nend\nprint(y)",
        ),
        "MZ0933",
        "  else\n",
    );
}

#[test]
fn a_binding_from_a_closed_block_compared_with_an_enum_is_mz0920() {
    let d = one(
        &wrap(
            "let c = shape.circle\nwhen true\n  let other = square\nend\nwhen c is other\n  print(1)\nend",
        ),
        "MZ0920",
    );
    assert!(d.say.contains("block that ended"), "{d:#?}");
}

#[test]
fn a_fn_named_like_a_variant_is_one_mz0921_and_the_variant_still_resolves() {
    let src = "program t\n\n  enum color\n    red\n    blue\n  end\n\n  enum light\n    red\n    green\n  end\n\n  fn main\n    let c: color = red\n    when c is red\n      print(\"red\")\n    end\n  end fn main\n\n  fn red: int\n    return 1\n  end fn red\n\nend program t\n";
    let d = one(src, "MZ0921");
    assert!(d.say.contains("variant of `enum color`"), "{d:#?}");
}

#[test]
fn mz0936_keeps_a_case_after_else_after_it() {
    let src = wrap(
        "let c = shape.circle\nlet n = twice(1)\nwhen c is circle\n  match n\n    case 1\n      print(1)\n    else\n      print(0)\n    case 2\n      print(2)\n  end\nelse when c is square\n  print(4)\nend",
    );
    let errors = errors(&src);
    let chain = errors.iter().find(|d| d.code == "MZ0936").expect("MZ0936");
    let fix = chain.fix.as_ref().expect("a guess fix");
    let text = &fix.replace;
    let else_at = text.find("else").expect("else");
    let case2 = text.find("case 2").expect("case 2");
    assert!(case2 > else_at, "case 2 must stay after the else:\n{text}");
    assert!(errors.iter().any(|d| d.code == "MZ0931"));
}

#[test]
fn an_empty_case_is_one_diagnostic() {
    let src = with_f("match s\n  case circle\n    return 1\n  case\n    return 2\nend\nreturn 3");
    let errors = errors(&src);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].code, "MZ0917");
}

#[test]
fn a_binding_named_switch_is_a_name() {
    clean(&wrap("var switch = 0\nswitch = switch + 1\nprint(switch)"));
    let errors = errors(&wrap("var switch = 0\nswitch += 1\nprint(switch)"));
    assert!(errors.iter().all(|d| d.code != "MZ0933"), "{errors:#?}");
}

#[test]
fn a_match_over_a_float_is_mz0711_and_floats_work_in_loops_and_block_values() {
    let d = one(
        &wrap("let x = 1.5\nmatch x\n  case 1.5\n    print(1)\n  else\n    print(0)\nend"),
        "MZ0711",
    );
    assert!(d.say.contains("compare a float with `when`"), "{d:#?}");
    one(
        &wrap("let n = twice(1)\nmatch n\n  case 1.5\n    print(1)\n  else\n    print(0)\nend"),
        "MZ0912",
    );
    clean(&wrap(
        "var total = 0.0\nfor each i in range(0, to = 4)\n  total = total + i.to_float() / 2.0\nend\nlet half = when total > 2.0\n  total / 2.0\nelse\n  total\nend\nprint(half)",
    ));
}

// ------------------------------------------------------------------- review fixes (#89)

#[test]
fn mz0936_a_when_used_as_a_value_over_one_enums_variants_is_a_match() {
    // RFC-0013 §7.1 and §7.4: the chain is the same mistake whether its branches are
    // statements or values.
    let src = wrap(
        "let s = square\nlet t = when s is circle\n  \"round\"\nelse when s is square\n  \"four\"\nelse\n  \"three\"\nend\nprint(t)",
    );
    let d = one(&src, "MZ0936");
    let fix = d.fix.expect("a guess fix");
    assert_eq!(fix.confidence, Confidence::Guess);
    assert_eq!(
        fix.replace,
        "match s\n      case circle\n        \"round\"\n      case square\n        \"four\"\n      else\n        \"three\"\n    end\n"
    );
    let after = apply(&src, fix.span, &fix.replace);
    assert!(after.contains("    let t = match s\n"), "{after}");
    clean(&after);
    // As a `return`'s value and an assignment's, and with the enum written out.
    let src =
        with_f("return when s is circle\n  0\nelse when s is shape.square\n  4\nelse\n  3\nend");
    let fix = one(&src, "MZ0936").fix.expect("a fix");
    clean(&apply(&src, fix.span, &fix.replace));
    let src = wrap(
        "let s = square\nvar n = 0\nn = when s is circle\n  0\nelse when s is triangle\n  3\nelse\n  4\nend\nprint(n)",
    );
    let fix = one(&src, "MZ0936").fix.expect("a fix");
    clean(&apply(&src, fix.span, &fix.replace));
    // One test of a variant, or a chain over anything else, stays a `when`.
    clean(&wrap(
        "let s = square\nlet t = when s is circle\n  \"round\"\nelse\n  \"angular\"\nend\nprint(t)",
    ));
    clean(&wrap(
        "let n = 2\nlet t = when n is 1\n  \"one\"\nelse when n is 2\n  \"two\"\nelse\n  \"many\"\nend\nprint(t)",
    ));
}

#[test]
fn two_bare_variants_compare_against_the_one_whose_enum_is_unambiguous() {
    // RFC-0013 §18.6: the other side of a comparison supplies the expected type, and a
    // bare variant that belongs to one enum is a side that does.
    let prog = |body: &str| {
        format!(
            "program t\n\n  enum color\n    red\n    blue\n  end\n\n  enum light\n    blue\n    amber\n  end\n\n  fn main\n{body}  end fn main\n\nend program t\n"
        )
    };
    let src =
        prog("    print(amber is blue)\n    print(blue < amber)\n    print(red is not blue)\n");
    clean(&src);
    let main = main_rs(&src);
    for want in [
        "Light::Amber == Light::Blue",
        "Light::Blue < Light::Amber",
        "Color::Red != Color::Blue",
    ] {
        assert!(main.contains(want), "expected `{want}` in\n{main}");
    }
    // Against that side, a misspelt variant gets the nearest of that enum.
    fixed_by(
        &prog("    print(amber is bleu)\n"),
        "MZ0708",
        "amber is blue",
    );
    // Two variants both enums share still need one named.
    one(&prog("    print(blue is blue)\n"), "MZ0708");
}

#[test]
fn a_list_after_for_each_in_is_a_list_literal() {
    // `[` and `]` were `MZ0104` until C7 built lists; a `for each` over a list literal now
    // checks clean.
    let found = errors(&wrap(
        "var x = 0\nfor each k in [1, 2]\n  x = x + k\nend\nprint(x)",
    ));
    assert!(found.is_empty(), "{found:#?}");
    // TypeScript's loop over a list literal is `MZ0934` with its exact rewrite, which the
    // lexer no longer respells as a type (`MZ0105`).
    let src = wrap("let n = 1\nfor (const k of [n])\n  print(k)\nend");
    let d = one(&src, "MZ0934");
    assert_eq!(d.fix.expect("exact").replace, "for each k in [n]");
    fixed_by(&src, "MZ0934", "for each k in [n]");
}

#[test]
fn mz0927_ranges_label_written_with_a_colon_is_one_exact_fix() {
    // RFC-0013 §16: `name: v` is `MZ0927`, `exact` `name = v`; the label is read, so the
    // call is not also an unbound `to` and a leftover `:`.
    let src = wrap("for each i in range(0, to: 3)\n  print(i)\nend");
    let d = one(&src, "MZ0927");
    assert_eq!(d.fix.as_ref().map(|f| f.replace.as_str()), Some("to ="));
    fixed_by(&src, "MZ0927", "for each i in range(0, to = 3)");
}

#[test]
fn mz0924_waits_while_lines_of_the_function_were_skipped_unread() {
    // A `do … while` or a C-style `for` is skipped as one `MZ0934`; it may assign the
    // `var`, so `var` → `let` is neither warned nor fixed until it is rewritten.
    for body in [
        "var x = 0\ndo\n  x = x + 1\nwhile x < 5\nprint(x)",
        "var x = 0\ndo\n  x = x + 1\nend while x < 5\nprint(x)",
        "var x = 0\nfor (var i = 0; i < 3; i++)\n  x = x + i\nend\nprint(x)",
    ] {
        let report = check(&wrap(body), "t.mz");
        let codes: Vec<&str> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, ["MZ0934"], "{body}\n{:#?}", report.diagnostics);
    }
    // With nothing skipped, the warning and its fix stand.
    let report = check(&wrap("var x = 0\nprint(x)"), "t.mz");
    assert_eq!(report.diagnostics.len(), 1, "{:#?}", report.diagnostics);
    assert_eq!(report.diagnostics[0].code, "MZ0924");
    assert_eq!(
        report.diagnostics[0].fix.as_ref().map(|f| f.confidence),
        Some(Confidence::Exact)
    );
}

#[test]
fn mz0708_names_a_variant_listed_twice_once() {
    let src = "program t\n\n  enum color\n    red\n    green\n    green\n  end\n\n  fn main\n    let c = red\n    match c\n      case reed\n        print(1)\n      else\n        print(2)\n    end\n  end fn main\n\nend program t\n";
    let found = errors(src);
    let codes: Vec<&str> = found.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["MZ0704", "MZ0708"], "{found:#?}");
    assert!(
        found[1]
            .say
            .contains("whose variants are `red`, `green` — did you mean `red`?"),
        "{}",
        found[1].say
    );
    // `MZ0930` names a missing variant once too.
    let src = src.replace(
        "      case reed\n        print(1)\n      else\n        print(2)\n",
        "      case red\n        print(1)\n",
    );
    let found = errors(&src);
    let miss = found.iter().find(|d| d.code == "MZ0930").expect("MZ0930");
    assert!(miss.say.contains("misses `green` —"), "{}", miss.say);
}
