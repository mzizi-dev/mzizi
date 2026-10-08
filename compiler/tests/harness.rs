//! The language harness's drift tests (RFC-0012 §1.2, the registration rule).
//!
//! The registry in `compiler/src/harness.rs` is what an agent reads, so these tests hold it
//! to what the compiler does:
//!
//! - every diagnostic code the source can emit is registered, or on the documented pending
//!   list, and every registered or pending code is still emitted somewhere;
//! - every registered code's `trigger` makes `mz check` report it, at its severity, with a
//!   fix kind the entry declares;
//! - every entry's examples check with no diagnostic, and every program example that gives
//!   an output prints exactly that under `mz run` (skipped, and said so, without `cargo`);
//! - every statement and expression in those examples names a registered entry;
//! - `mz harness` prints valid, deterministic JSON, and `version` hashes the definition.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::check;
use mzizi_lang_compiler::diagnostic::Confidence;
use mzizi_lang_compiler::expr::{BinOp, Expr, ExprKind, TextPart, Ty, UnOp};
use mzizi_lang_compiler::harness::{
    self, CODES, COMMANDS, Depth, FixKind, Kind, PENDING_CODES, registry,
};
use mzizi_lang_compiler::parse::{Program, parse_program};
use mzizi_lang_compiler::program::{Stmt, StmtKind};

fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `MZ` + four digits on a line of compiler source that is not a comment.
fn emitted_codes() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![src_dir()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            // The registry itself is not evidence that a code is emitted.
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("harness.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for line in text.lines() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                let b = line.as_bytes();
                for i in 0..b.len().saturating_sub(5) {
                    if &b[i..i + 2] == b"MZ" && b[i + 2..i + 6].iter().all(u8::is_ascii_digit) {
                        let code = line[i..i + 6].to_string();
                        out.entry(code)
                            .or_insert_with(|| path.file_name().unwrap().to_string_lossy().into());
                    }
                }
            }
        }
    }
    out
}

#[test]
fn every_code_the_compiler_emits_is_registered_or_pending() {
    let emitted = emitted_codes();
    assert!(
        emitted.len() > 100,
        "the scan found only {} codes",
        emitted.len()
    );
    let missing: Vec<String> = emitted
        .iter()
        .filter(|(c, _)| !harness::is_known(c))
        .map(|(c, f)| format!("{c} (in {f})"))
        .collect();
    assert!(
        missing.is_empty(),
        "codes with no language-harness entry; register each in compiler/src/harness.rs: {missing:?}"
    );
}

#[test]
fn every_registered_or_pending_code_is_still_emitted() {
    let emitted = emitted_codes();
    for c in CODES {
        assert!(
            emitted.contains_key(c.code),
            "{} is registered but nothing emits it",
            c.code
        );
    }
    for c in PENDING_CODES {
        assert!(
            emitted.contains_key(*c),
            "{c} is pending but nothing emits it"
        );
        assert!(
            harness::code_entry(c).is_none(),
            "{c} is both registered and pending"
        );
    }
    let mut seen = BTreeSet::new();
    for c in CODES {
        assert!(seen.insert(c.code), "{} is registered twice", c.code);
    }
    let codes: Vec<&str> = CODES.iter().map(|c| c.code).collect();
    let mut sorted = codes.clone();
    sorted.sort_unstable();
    assert_eq!(codes, sorted, "CODES is kept in code order");
}

#[test]
fn every_program_code_is_registered_not_pending() {
    // The brief for this slice: each `MZ09xx` code the program checker emits has an entry.
    for c in emitted_codes().keys().filter(|c| c.starts_with("MZ09")) {
        assert!(harness::code_entry(c).is_some(), "{c} has no entry");
    }
}

#[test]
fn every_trigger_reports_its_code_with_a_declared_fix_kind() {
    let mut untriggered = Vec::new();
    let mut wrong = Vec::new();
    for c in CODES {
        let Some(trigger) = c.trigger else {
            untriggered.push(c.code);
            continue;
        };
        let report = check(trigger, "t.mz");
        let hits: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == c.code)
            .collect();
        if hits.is_empty() {
            let got: Vec<&str> = report.diagnostics.iter().map(|d| d.code).collect();
            wrong.push(format!("{}'s trigger reports {got:?}", c.code));
        }
        for d in hits {
            assert_eq!(d.severity, c.severity, "{}", c.code);
            let kind = FixKind::of(d.fix.as_ref().map(|f| f.confidence));
            if !c.fixes.contains(&kind) {
                wrong.push(format!(
                    "{}'s trigger carries a `{}` fix, and the entry declares {:?}",
                    c.code,
                    kind.as_str(),
                    c.fixes
                ));
            }
        }
        assert!(c.say.len() <= 300, "{}'s say is long", c.code);
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    // `mz check` cannot report these two: one is a compiler bug, the other a trap at run
    // time, which `compiler/tests/program.rs` triggers through `mz run`.
    assert_eq!(untriggered, ["MZ0990", "MZ0991"]);
    let _ = Confidence::Exact;
}

#[test]
fn entries_are_unique_and_every_code_they_name_is_registered() {
    let entries = registry();
    let mut names = BTreeSet::new();
    for e in &entries {
        assert!(names.insert(e.name), "two entries named `{}`", e.name);
        assert!(!e.teach.is_empty(), "`{}` teaches nothing", e.name);
        for code in &e.codes {
            assert!(
                harness::code_entry(code).is_some(),
                "`{}` names {code}, which has no entry",
                e.name
            );
        }
        let is_feature = !matches!(e.kind, Kind::Command | Kind::Diagnostic);
        if is_feature {
            assert!(!e.examples.is_empty(), "`{}` has no example", e.name);
            assert!(!e.grammar.is_empty(), "`{}` has no grammar", e.name);
        }
        if e.depth == Depth::KindOnly {
            assert!(matches!(e.name, "component" | "service"), "{}", e.name);
        }
    }
    // Kinds come in order, so the definition reads the same way every time.
    let kinds: Vec<Kind> = entries.iter().map(|e| e.kind).collect();
    let mut sorted = kinds.clone();
    sorted.sort();
    assert_eq!(kinds, sorted);
}

#[test]
fn operators_and_types_come_from_the_checker() {
    for &op in BinOp::ALL {
        let e = harness::entry(op.text()).expect("an operator entry");
        assert_eq!(e.precedence, Some(op.level()), "{}", op.text());
    }
    for &op in UnOp::ALL {
        let name = match op {
            UnOp::Neg => "prefix -",
            UnOp::Not => "not",
        };
        assert!(harness::entry(name).is_some(), "{name}");
    }
    for t in Ty::surface() {
        let e = harness::entry(t.name()).expect("a type entry");
        assert_eq!(e.kind, Kind::Type);
    }
    // `int`'s entry lists exactly the operators the checker types on two ints.
    let int = harness::entry("int").unwrap();
    assert!(int.types.contains("`%`") && !int.types.contains("`and`"));
}

#[test]
fn every_command_mz_dispatches_is_registered() {
    let words: Vec<&str> = COMMANDS.iter().map(|c| c.word).collect();
    assert_eq!(
        words,
        [
            "check", "fix", "contract", "outline", "hash", "ir", "build", "run", "harness"
        ]
    );
    for c in COMMANDS {
        assert_eq!(c.name, format!("mz {}", c.word));
        assert!(harness::entry(c.name).is_some());
    }
    assert!(!harness::takes_file("harness") && harness::takes_file("run"));
}

fn walk_expr(e: &Expr, out: &mut BTreeSet<&'static str>) {
    out.insert(harness::expression_entry(&e.kind));
    match &e.kind {
        ExprKind::Text(parts) => {
            for p in parts {
                if let TextPart::Expr(e) = p {
                    walk_expr(e, out);
                }
            }
        }
        ExprKind::Call { args, .. } => args.iter().for_each(|a| walk_expr(a, out)),
        ExprKind::Method { recv, args, .. } => {
            walk_expr(recv, out);
            args.iter().for_each(|a| walk_expr(a, out));
        }
        ExprKind::Unary { operand, .. } => walk_expr(operand, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, out);
            walk_expr(rhs, out);
        }
        ExprKind::When { arms, otherwise } => {
            for (c, v) in arms {
                walk_expr(c, out);
                walk_expr(v, out);
            }
            otherwise.iter().for_each(|o| walk_expr(o, out));
        }
        ExprKind::Match {
            scrutinee,
            arms,
            otherwise,
        } => {
            walk_expr(scrutinee, out);
            for a in arms {
                a.values.iter().for_each(|v| walk_expr(v, out));
                walk_expr(&a.body, out);
            }
            otherwise.iter().for_each(|o| walk_expr(&o.body, out));
        }
        _ => {}
    }
}

fn walk(stmts: &[Stmt], out: &mut BTreeSet<&'static str>) {
    for s in stmts {
        out.insert(harness::statement_entry(&s.kind));
        match &s.kind {
            StmtKind::Bind { value, .. } | StmtKind::Assign { value, .. } => walk_expr(value, out),
            StmtKind::Return(v) => v.iter().for_each(|v| walk_expr(v, out)),
            StmtKind::When {
                cond,
                then,
                else_whens,
                otherwise,
            } => {
                walk_expr(cond, out);
                walk(then, out);
                for w in else_whens {
                    walk_expr(&w.cond, out);
                    walk(&w.body, out);
                }
                otherwise.iter().for_each(|o| walk(o, out));
            }
            StmtKind::Expr(e) => walk_expr(e, out),
            StmtKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                walk_expr(scrutinee, out);
                for a in arms {
                    a.values.iter().for_each(|v| walk_expr(v, out));
                    walk(&a.body, out);
                }
                otherwise.iter().for_each(|o| walk(&o.body, out));
            }
            StmtKind::For { source, body, .. } => {
                walk_expr(source, out);
                walk(body, out);
            }
            StmtKind::While { cond, body } => {
                walk_expr(cond, out);
                walk(body, out);
            }
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
}

/// Every example source, once, with its expected output if any.
fn examples() -> BTreeMap<&'static str, Option<&'static str>> {
    let mut out = BTreeMap::new();
    for e in registry() {
        for x in &e.examples {
            let prev = out.insert(x.source, x.output);
            if let Some(prev) = prev {
                assert_eq!(prev, x.output, "one example, two outputs:\n{}", x.source);
            }
        }
    }
    out
}

#[test]
fn every_example_checks_clean_and_uses_only_registered_constructs() {
    let mut used = BTreeSet::new();
    for source in examples().keys() {
        let report = check(source, "example.mz");
        assert!(
            report.diagnostics.is_empty(),
            "an example must check with no diagnostic:\n{source}\n{:#?}",
            report.diagnostics
        );
        if let (Some(Program::Program(p)), _) = parse_program(source, "example.mz") {
            used.insert("program");
            for f in &p.fns {
                used.insert("fn");
                walk(&f.body, &mut used);
            }
        }
    }
    for name in &used {
        assert!(
            harness::entry(name).is_some(),
            "an example uses `{name}`, which has no entry"
        );
    }
    // The examples between them use every statement form and operator the slice has.
    for &op in BinOp::ALL {
        assert!(used.contains(op.text()), "no example uses `{}`", op.text());
    }
    for name in [
        "let",
        "var",
        "assignment",
        "when",
        "return",
        "print",
        "prefix -",
        "not",
        "names",
        "int",
        "bool",
        "float",
        "text literal",
        "fn",
        "else when",
        "match",
        "for each",
        "while",
        "break",
        "continue",
        "when or match as a value",
        "enum",
    ] {
        assert!(used.contains(name), "no example uses `{name}`");
    }
    for name in mzizi_lang_compiler::numbers::METHODS {
        assert!(used.contains(name), "no example calls `.{name}()`");
    }
}

#[test]
fn the_repository_examples_use_only_registered_constructs() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "mz") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        if let (Some(Program::Program(p)), _) = parse_program(&src, "x.mz") {
            let mut used = BTreeSet::new();
            for f in &p.fns {
                walk(&f.body, &mut used);
            }
            for name in used {
                assert!(
                    harness::entry(name).is_some(),
                    "{}: `{name}`",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn program_examples_print_their_expected_output() {
    if Command::new("cargo").arg("--version").output().is_err() {
        eprintln!("SKIPPED: `cargo` is not on the path, so `mz run` cannot build anything");
        return;
    }
    let dir = std::env::temp_dir().join(format!("mz-harness-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (i, (source, output)) in examples().into_iter().enumerate() {
        let Some(output) = output else { continue };
        let path = dir.join(format!("example_{i}.mz"));
        std::fs::write(&path, source).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_mz"))
            .args(["run", path.to_str().unwrap()])
            .env("MZ_CACHE_DIR", dir.join("cache"))
            .output()
            .expect("mz runs");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            output,
            "{source}\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(0));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------- mz harness

fn mz(args: &[&str]) -> (String, Option<i32>) {
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(args)
        .output()
        .expect("mz runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
    )
}

/// A small JSON reader: enough to prove the hand-written JSON is valid, and to compare the
/// indented and one-line forms value for value.
#[derive(Debug, PartialEq)]
enum Json {
    Str(String),
    Num(u64),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

fn parse_json(text: &str) -> Json {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let v = value(&chars, &mut i);
    skip(&chars, &mut i);
    assert_eq!(i, chars.len(), "trailing text after the JSON value");
    v
}

fn skip(c: &[char], i: &mut usize) {
    while *i < c.len() && c[*i].is_whitespace() {
        *i += 1;
    }
}

fn value(c: &[char], i: &mut usize) -> Json {
    skip(c, i);
    match c[*i] {
        '"' => Json::Str(string(c, i)),
        '[' => {
            *i += 1;
            let mut items = Vec::new();
            skip(c, i);
            if c[*i] == ']' {
                *i += 1;
                return Json::Arr(items);
            }
            loop {
                items.push(value(c, i));
                skip(c, i);
                match c[*i] {
                    ',' => *i += 1,
                    ']' => {
                        *i += 1;
                        return Json::Arr(items);
                    }
                    other => panic!("expected , or ] at {i}, found {other:?}"),
                }
            }
        }
        '{' => {
            *i += 1;
            let mut fields = Vec::new();
            loop {
                skip(c, i);
                let k = string(c, i);
                skip(c, i);
                assert_eq!(c[*i], ':');
                *i += 1;
                fields.push((k, value(c, i)));
                skip(c, i);
                match c[*i] {
                    ',' => *i += 1,
                    '}' => {
                        *i += 1;
                        return Json::Obj(fields);
                    }
                    other => panic!("expected , or }} at {i}, found {other:?}"),
                }
            }
        }
        d if d.is_ascii_digit() => {
            let mut n = 0u64;
            while c[*i].is_ascii_digit() {
                n = n * 10 + u64::from(c[*i].to_digit(10).unwrap());
                *i += 1;
            }
            Json::Num(n)
        }
        other => panic!("unexpected {other:?} at {i}"),
    }
}

fn string(c: &[char], i: &mut usize) -> String {
    assert_eq!(c[*i], '"');
    *i += 1;
    let mut out = String::new();
    loop {
        match c[*i] {
            '"' => {
                *i += 1;
                return out;
            }
            '\\' => {
                *i += 1;
                match c[*i] {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'u' => {
                        let hex: String = c[*i + 1..*i + 5].iter().collect();
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
                        *i += 4;
                    }
                    other => panic!("bad escape \\{other}"),
                }
                *i += 1;
            }
            ch => {
                assert!((ch as u32) >= 0x20, "a raw control character in a string");
                out.push(ch);
                *i += 1;
            }
        }
    }
}

#[test]
fn mz_harness_definition_is_valid_deterministic_json() {
    let (pretty, status) = mz(&["harness", "definition"]);
    assert_eq!(status, Some(0));
    let (agent, status) = mz(&["harness", "definition", "--agent"]);
    assert_eq!(status, Some(0));
    assert_eq!(agent.lines().count(), 1, "--agent is one line");
    assert_eq!(agent, harness::definition(false));
    assert_eq!(pretty, harness::definition(true));
    assert_eq!(harness::definition(false), harness::definition(false));
    let a = parse_json(&agent);
    assert_eq!(a, parse_json(&pretty), "the two forms hold the same value");
    let Json::Obj(fields) = a else { panic!() };
    let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["protocol", "language", "entries", "pending_codes"]);
    let Json::Arr(entries) = &fields[2].1 else {
        panic!()
    };
    assert_eq!(entries.len(), registry().len());
    assert_eq!(fields[0].1, Json::Num(u64::from(harness::PROTOCOL)));
}

#[test]
fn mz_harness_version_hashes_the_definition() {
    let (out, status) = mz(&["harness", "version"]);
    assert_eq!(status, Some(0));
    let Json::Obj(fields) = parse_json(&out) else {
        panic!()
    };
    assert_eq!(fields[0], ("protocol".into(), Json::Num(1)));
    assert_eq!(
        fields[1],
        ("language".into(), Json::Str(harness::LANGUAGE.into()))
    );
    let hash = mzizi_lang_compiler::hash::sha256(harness::definition(false).as_bytes()).to_hex();
    assert_eq!(fields[2], ("definition_sha256".into(), Json::Str(hash)));
}

#[test]
fn mz_harness_entry_prints_one_entry_and_rejects_an_unknown_name() {
    let (out, status) = mz(&["harness", "entry", "is", "not", "--agent"]);
    assert_eq!(status, Some(0));
    let Json::Obj(fields) = parse_json(&out) else {
        panic!()
    };
    assert_eq!(fields[0], ("name".into(), Json::Str("is not".into())));
    let (out, status) = mz(&["harness", "entry", "MZ0905"]);
    assert_eq!(status, Some(0));
    assert!(out.contains("\"diagnostic\": {"), "{out}");
    let (out, status) = mz(&["harness", "entry", "mz", "run"]);
    assert_eq!(status, Some(0));
    assert!(out.contains("\"kind\": \"command\""), "{out}");
    for e in registry() {
        let text = harness::entry_text(e.name, false).unwrap();
        parse_json(&text);
    }
    let (out, status) = mz(&["harness", "entry", "do"]);
    assert_eq!((out.as_str(), status), ("", Some(2)));
    let (_, status) = mz(&["harness"]);
    assert_eq!(status, Some(2));
    let (_, status) = mz(&["harness", "entry"]);
    assert_eq!(status, Some(2));
}

#[test]
fn types_are_exactly_the_surface_types_and_the_parser_reads_them() {
    // Every type an author writes has an entry, and nothing else is a type entry: a surface
    // type whose `type_teach` arm is `None` has no entry, and fails here.
    let surface: Vec<&str> = Ty::surface().map(Ty::name).collect();
    let entries: Vec<&str> = registry()
        .into_iter()
        .filter(|e| e.kind == Kind::Type)
        .map(|e| e.name)
        .collect();
    assert_eq!(entries, surface);
    assert_eq!(surface, ["int", "float", "bool", "text"]);
    for t in Ty::ALL {
        assert_eq!(
            Ty::from_name(t.name()).is_some(),
            t.is_surface(),
            "{}",
            t.name()
        );
    }
    // The program parser accepts exactly those names as types.
    for name in &surface {
        let src = format!(
            "program t\n  fn main\n    print(1)\n  end fn main\n  fn f(x: {name}): {name}\n    return x\n  end fn f\nend program t\n"
        );
        let report = check(&src, "t.mz");
        assert!(
            report.diagnostics.is_empty(),
            "{name}: {:#?}",
            report.diagnostics
        );
    }
    let report = check(
        "program t\n  fn main\n    print(1)\n  end fn main\n  fn f(x: nothing): int\n    return 1\n  end fn f\nend program t\n",
        "t.mz",
    );
    assert!(report.diagnostics.iter().any(|d| d.code == "MZ0701"));
}

#[test]
fn every_numeric_method_the_checker_types_has_an_entry() {
    let methods: Vec<&str> = registry()
        .into_iter()
        .filter(|e| e.kind == Kind::Method)
        .map(|e| e.name)
        .collect();
    assert_eq!(methods, mzizi_lang_compiler::numbers::METHODS);
    let pow = harness::entry("pow").unwrap();
    assert_eq!(pow.types, "int.pow(int) → int; float.pow(int) → float");
}

#[test]
fn every_operator_the_lexer_reads_names_an_entry_and_is_reported_so() {
    let lexed: Vec<&str> = harness::LEXED_OPERATORS.iter().map(|(op, _)| *op).collect();
    assert_eq!(lexed, mzizi_lang_compiler::lex::OPERATORS);
    for (op, name) in harness::LEXED_OPERATORS {
        assert!(harness::entry(name).is_some(), "`{op}` names `{name}`");
        if !name.starts_with("MZ") {
            continue;
        }
        // A spelling from another language is reported with the code the entry names.
        let body = match *op {
            "->" => {
                "program t\n  fn main\n    print(f(1))\n  end fn main\n  fn f(a: int) -> int\n    return a\n  end fn f\nend program t\n".to_string()
            }
            "=>" => "program t\n  fn main\n    let f = 1 => 2\n    print(f)\n  end fn main\nend program t\n".to_string(),
            "+=" | "-=" | "*=" | "/=" => format!(
                "program t\n  fn main\n    var n = 1\n    n {op} 2\n    print(n)\n  end fn main\nend program t\n"
            ),
            "++" | "--" => format!(
                "program t\n  fn main\n    var n = 1\n    n{op}\n    print(n)\n  end fn main\nend program t\n"
            ),
            "!" => "program t\n  fn main\n    print(!true)\n  end fn main\nend program t\n".to_string(),
            "&&" | "||" => format!(
                "program t\n  fn main\n    print(true {op} false)\n  end fn main\nend program t\n"
            ),
            _ => format!("program t\n  fn main\n    print(2 {op} 3)\n  end fn main\nend program t\n"),
        };
        let report = check(&body, "t.mz");
        assert!(
            report.diagnostics.iter().any(|d| d.code == *name),
            "`{op}` should be {name}:\n{body}\n{:#?}",
            report.diagnostics
        );
    }
}

/// `PENDING_CODES` on 2026-10-08. The list may only shrink: a code leaves it when its entry
/// is registered, and a new code is registered, never added here.
const PENDING_SNAPSHOT: &[&str] = &[
    "MZ0201", "MZ0202", "MZ0203", "MZ0209", "MZ0301", "MZ0302", "MZ0303", "MZ0304", "MZ0305",
    "MZ0307", "MZ0308", "MZ0309", "MZ0312", "MZ0313", "MZ0401", "MZ0402", "MZ0403", "MZ0404",
    "MZ0405", "MZ0406", "MZ0408", "MZ0409", "MZ0410", "MZ0501", "MZ0502", "MZ0601", "MZ0602",
    "MZ0603", "MZ0605", "MZ0606", "MZ0611", "MZ0612", "MZ0613", "MZ0702", "MZ0703", "MZ0704",
    "MZ0705", "MZ0706", "MZ0709", "MZ0710", "MZ0713", "MZ0715", "MZ0716", "MZ0801", "MZ0802",
    "MZ0803", "MZ0804", "MZ0805", "MZ0806", "MZ0807", "MZ0808", "MZ0809", "MZ0810", "MZ0811",
    "MZ0812",
];

#[test]
fn the_pending_list_is_sorted_and_only_shrinks() {
    let mut sorted = PENDING_CODES.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        PENDING_CODES,
        sorted.as_slice(),
        "PENDING_CODES is kept sorted"
    );
    let grown: Vec<&&str> = PENDING_CODES
        .iter()
        .filter(|c| !PENDING_SNAPSHOT.contains(c))
        .collect();
    assert!(
        grown.is_empty(),
        "codes added to PENDING_CODES; register them in CODES instead: {grown:?}"
    );
    assert!(PENDING_CODES.iter().all(|c| !c.starts_with("MZ09")));
}

#[test]
fn every_literal_names_its_own_types_entry() {
    // Literals, names and the methods each name the entry that teaches them, not merely some
    // entry that exists: a literal mapped to another type's entry fails here.
    let mut seen = BTreeSet::new();
    for source in examples().keys() {
        if let (Some(Program::Program(p)), _) = parse_program(source, "example.mz") {
            for f in &p.fns {
                let mut exprs = Vec::new();
                collect(&f.body, &mut exprs);
                for e in exprs {
                    let want = match &e.kind {
                        ExprKind::Int(_) => Some("int"),
                        ExprKind::Float(_) => Some("float"),
                        ExprKind::Bool(_) => Some("bool"),
                        ExprKind::Text(_) => Some("text literal"),
                        ExprKind::Name(_) => Some("names"),
                        ExprKind::Method { name, .. } => Some(name.as_str()),
                        _ => None,
                    };
                    if let Some(want) = want {
                        let got = harness::expression_entry(&e.kind);
                        assert_eq!(got, want, "{e:?}");
                        let entry = harness::entry(got).expect("registered");
                        if matches!(
                            e.kind,
                            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_)
                        ) {
                            assert_eq!(entry.kind, Kind::Type, "{got}");
                        }
                        seen.insert(want.to_string());
                    }
                }
            }
        }
    }
    for name in ["int", "float", "bool", "text literal", "names"] {
        assert!(seen.contains(name), "no example has a `{name}` expression");
    }
}

fn collect<'a>(stmts: &'a [Stmt], out: &mut Vec<&'a Expr>) {
    fn expr<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
        out.push(e);
        match &e.kind {
            ExprKind::Text(parts) => {
                for p in parts {
                    if let TextPart::Expr(e) = p {
                        expr(e, out);
                    }
                }
            }
            ExprKind::Call { args, .. } => args.iter().for_each(|a| expr(a, out)),
            ExprKind::Method { recv, args, .. } => {
                expr(recv, out);
                args.iter().for_each(|a| expr(a, out));
            }
            ExprKind::Unary { operand, .. } => expr(operand, out),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, out);
                expr(rhs, out);
            }
            ExprKind::When { arms, otherwise } => {
                for (c, v) in arms {
                    expr(c, out);
                    expr(v, out);
                }
                otherwise.iter().for_each(|o| expr(o, out));
            }
            ExprKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                expr(scrutinee, out);
                for a in arms {
                    a.values.iter().for_each(|v| expr(v, out));
                    expr(&a.body, out);
                }
                otherwise.iter().for_each(|o| expr(&o.body, out));
            }
            _ => {}
        }
    }
    for s in stmts {
        match &s.kind {
            StmtKind::Bind { value, .. } | StmtKind::Assign { value, .. } => expr(value, out),
            StmtKind::Return(v) => v.iter().for_each(|v| expr(v, out)),
            StmtKind::When {
                cond,
                then,
                else_whens,
                otherwise,
            } => {
                expr(cond, out);
                collect(then, out);
                for w in else_whens {
                    expr(&w.cond, out);
                    collect(&w.body, out);
                }
                otherwise.iter().for_each(|o| collect(o, out));
            }
            StmtKind::Expr(e) => expr(e, out),
            StmtKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                expr(scrutinee, out);
                for a in arms {
                    a.values.iter().for_each(|v| expr(v, out));
                    collect(&a.body, out);
                }
                otherwise.iter().for_each(|o| collect(&o.body, out));
            }
            StmtKind::For { source, body, .. } => {
                expr(source, out);
                collect(body, out);
            }
            StmtKind::While { cond, body } => {
                expr(cond, out);
                collect(body, out);
            }
            StmtKind::Break | StmtKind::Continue => {}
        }
    }
}
