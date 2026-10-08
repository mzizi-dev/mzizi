//! SECURITY.md's first promise, tested: the compiler terminates with a diagnostic on every
//! input. A panic, a stack overflow or a hang on a `.mz` file is a vulnerability there,
//! because the input is untrusted and an agent drives `mz` over it in a loop.
//!
//! No fuzzing crate: the compiler takes no dependencies, so the inputs come from a small
//! seeded generator. Each case runs every entry point an untrusted file reaches (`check`,
//! `check_contract`, `apply_exact_fixes` and its re-check, the NDJSON writer, and, when a
//! tree comes back, `outline`, the IR, service lowering and program lowering), on a thread
//! with a deadline, so a hang fails the test rather than the CI job. A failing case prints its seed and its
//! input, so it reproduces.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use mzizi_lang_compiler::diagnostic::Severity;
use mzizi_lang_compiler::ir::{Store, lower, paths};
use mzizi_lang_compiler::outline::outline;
use mzizi_lang_compiler::parse::Program;
use mzizi_lang_compiler::{
    apply_exact_fixes, check, check_contract, check_program, check_with_ast,
};

/// Generous for one file: a case that takes this long is a hang, not a slow machine.
const DEADLINE: Duration = Duration::from_secs(20);

/// xorshift64*: deterministic, so a seed in a failure message replays the case.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Every entry point an untrusted file reaches, in the order `mz` reaches them.
fn exercise(src: &str) {
    let file = "case.mz";
    let report = check(src, file);
    for d in &report.diagnostics {
        let _ = d.to_ndjson();
    }
    let fixed = apply_exact_fixes(src, &report);
    let _ = check(&fixed, file);
    let _ = check_contract(src, file);
    if let (Some(component), _) = check_with_ast(src, file) {
        let _ = outline(&component);
        let mut store = Store::new();
        let root = lower(&component, &mut store);
        let _ = paths(&store, root);
    }
    if let (Some(Program::Service(service)), report) = check_program(src, file)
        && report.error_count() == 0
    {
        let _ = mzizi_lang_compiler::lower::lower(&service, file);
    }
    if let (Some(Program::Program(program)), report) = check_program(src, file)
        && report.error_count() == 0
    {
        let _ = mzizi_lang_compiler::run::lower(&program, file);
    }
}

/// Run one case on its own thread, with a deadline and a 1 MiB stack: the smallest main
/// thread `mz` gets on a supported platform (Windows), so recursion depth is measured
/// against the tightest stack the binary runs with.
fn run(label: &str, src: String) {
    // A stack overflow aborts the whole test process before any panic message, so the
    // case's label (with its seed) is written down first, in a file per test: after an
    // abort, `robustness-last-case-<test>.txt` names the case that did it. Under
    // `--test-threads=1` the tests share one file, which is still right: they run one
    // after another, so the last label written is the case that aborted.
    let test = thread::current()
        .name()
        .unwrap_or("test")
        .replace("::", "-");
    let last =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("robustness-last-case-{test}.txt"));
    let _ = std::fs::write(&last, label);
    // The input moves to the worker; a preview stays for the failure message.
    let preview = show(&src);
    let (tx, rx) = mpsc::channel();
    let worker = thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            exercise(&src);
            let _ = tx.send(());
        })
        .expect("spawn");
    match rx.recv_timeout(DEADLINE) {
        Ok(()) => {
            worker.join().expect("worker");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{label}: the compiler panicked on this input:\n{preview}");
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("{label}: no answer within {DEADLINE:?} on this input:\n{preview}");
        }
    }
}

fn show(src: &str) -> String {
    if src.len() <= 2000 {
        format!("{src:?}")
    } else {
        format!(
            "{:?} … ({} bytes)",
            &src[..src.floor_char_boundary(2000)],
            src.len()
        )
    }
}

/// The repo's own `.mz` files: the seeds every mutation starts from.
fn seeds() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    for dir in ["primitives", "examples"] {
        let mut files: Vec<_> = std::fs::read_dir(root.join(dir))
            .expect("dir")
            .map(|e| e.expect("entry").path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("mz"))
            .collect();
        files.sort();
        for f in files {
            let name = f.file_name().unwrap().to_string_lossy().into_owned();
            out.push((name, std::fs::read_to_string(&f).expect("readable")));
        }
    }
    assert!(
        out.len() >= 10,
        "expected the primitives and examples, found {}",
        out.len()
    );
    out
}

/// Words and punctuation the lexer and parser treat specially, so random edits hit the
/// grammar rather than only identifiers.
const PIECES: &[&str] = &[
    "component",
    "service",
    "end",
    "view",
    "when",
    "is",
    "else",
    "match",
    "for",
    "if",
    "enum",
    "prop",
    "contract",
    "example",
    "route",
    "respond",
    "file",
    "header",
    "slot",
    "=",
    "\"",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    ":",
    ",",
    ".",
    "#",
    "##",
    "\\",
    "\t",
    "\r",
    "\n",
    "  ",
    "0",
    "-1",
    "99999999999999999999999999",
    "é",
    "😀",
    "\u{0}",
    "\u{feff}",
    "\u{202e}",
    "/..",
    "../",
    "{x.class}",
    "event(none)",
    "= sm",
];

fn mutate(rng: &mut Rng, src: &str) -> String {
    let mut lines: Vec<String> = src.lines().map(str::to_string).collect();
    for _ in 0..1 + rng.below(6) {
        if lines.is_empty() {
            lines.push(String::new());
        }
        let i = rng.below(lines.len());
        match rng.below(8) {
            0 => {
                lines.remove(i);
            }
            1 => {
                let l = lines[i].clone();
                lines.insert(i, l);
            }
            2 => {
                let j = rng.below(lines.len());
                lines.swap(i, j);
            }
            3 => {
                let piece = PIECES[rng.below(PIECES.len())];
                let at = lines[i].floor_char_boundary(rng.below(lines[i].len() + 1));
                lines[i].insert_str(at, piece);
            }
            4 => {
                let at = lines[i].floor_char_boundary(rng.below(lines[i].len() + 1));
                lines[i].truncate(at);
            }
            5 => lines[i] = format!("{}{}", " ".repeat(rng.below(40)), lines[i].trim_start()),
            6 => lines.truncate(i),
            _ => lines[i] = PIECES[rng.below(PIECES.len())].repeat(1 + rng.below(30)),
        }
    }
    let mut out = lines.join("\n");
    if rng.below(2) == 0 {
        out.push('\n');
    }
    out
}

#[test]
fn every_repo_file_survives_random_edits() {
    let seeds = seeds();
    let mut rng = Rng(0x6d7a_697a_6921_2026);
    for round in 0..40 {
        for (name, src) in &seeds {
            let seed = rng.0;
            let case = mutate(&mut rng, src);
            run(&format!("{name}, round {round}, seed {seed:#x}"), case);
        }
    }
}

#[test]
fn random_token_soup_terminates() {
    let mut rng = Rng(0x0dd5_eed5_0000_0001);
    for n in 0..300 {
        let seed = rng.0;
        let mut s = String::new();
        for _ in 0..rng.below(400) {
            s.push_str(PIECES[rng.below(PIECES.len())]);
            if rng.below(3) == 0 {
                s.push(' ');
            }
        }
        run(&format!("token soup {n}, seed {seed:#x}"), s);
    }
}

#[test]
fn random_bytes_terminate() {
    let mut rng = Rng(0xb17e_5000_0000_0002);
    for n in 0..200 {
        let seed = rng.0;
        let bytes: Vec<u8> = (0..rng.below(600)).map(|_| rng.next() as u8).collect();
        run(
            &format!("bytes {n}, seed {seed:#x}"),
            String::from_utf8_lossy(&bytes).into_owned(),
        );
    }
}

/// Recursive descent's obvious attack: nesting far deeper than any real file.
#[test]
fn deep_nesting_terminates() {
    for depth in [100usize, 1_000, 10_000] {
        let mut s = String::from("component deep\n  view\n");
        for d in 0..depth {
            s.push_str(&"  ".repeat(d.min(200) + 2));
            s.push_str("row\n");
        }
        for d in (0..depth).rev() {
            s.push_str(&"  ".repeat(d.min(200) + 2));
            s.push_str("end\n");
        }
        s.push_str("  end\nend component deep\n");
        run(&format!("{depth} nested rows"), s);

        let mut w = String::from(
            "component deep\n  enum s\n    a class \"x\"\n  end\n  prop v: s = a\n  view\n    row\n",
        );
        for _ in 0..depth {
            w.push_str("      when v is a\n");
        }
        for _ in 0..depth {
            w.push_str("      end\n");
        }
        w.push_str("    end\n  end\nend component deep\n");
        run(&format!("{depth} nested `when`"), w);

        run(
            &format!("{depth} nested handler `when`"),
            deep_service("when", depth),
        );
        run(
            &format!("{depth} nested handler `if`"),
            deep_service("if", depth),
        );

        run(&format!("{depth} unclosed blocks"), "view\n".repeat(depth));
        run(
            &format!("{depth} open braces"),
            format!(
                "component x\n  view\n    row\n      class = \"{}\"\n",
                "{".repeat(depth)
            ),
        );
    }
}

/// A service whose one handler nests `kw` (`when`, or the `if` written for it) `depth`
/// deep.
fn deep_service(kw: &str, depth: usize) -> String {
    let mut s = String::from(
        "service deep\n  record item\n    field name: text\n  end\n  route r\n    get \"/x/{name}\"\n",
    );
    s.push_str(&format!("    {kw} name in \"a\"\n").repeat(depth));
    s.push_str("    respond 200 json item name name\n");
    s.push_str(&"    end\n".repeat(depth));
    s.push_str("    respond 404 json item name \"no\"\n  end\nend service deep\n");
    s
}

fn errors_after(src: &str, line: u32) -> Vec<String> {
    check(src, "case.mz")
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.span.start_line > line)
        .map(|d| format!("{} at line {}: {}", d.code, d.span.start_line, d.say))
        .collect()
}

fn count(src: &str, code: &str) -> usize {
    check(src, "case.mz")
        .diagnostics
        .iter()
        .filter(|d| d.code == code)
        .count()
}

/// Rows nested `depth` deep, each holding every kind of line the skip must classify the
/// way the parser does: attributes with and without `=`, a string and a doc line,
/// `nothing`, a `when … else … end`, and a `match` with its arms. A sibling row and a
/// contract follow. Returns the source and the line that closes the outer row.
fn odd_rows(depth: usize) -> (String, u32) {
    let mut lines = vec![
        "component deep".to_string(),
        "  enum s".into(),
        "    a class \"x\"".into(),
        "  end".into(),
        "  prop v: s = a".into(),
        "  view".into(),
        "    row".into(),
    ];
    for _ in 0..depth {
        for l in [
            "      row",
            "      class = \"x\"",
            "      class \"y\"",
            "      \"hello\"",
            "      ## a doc line",
            "      nothing",
            "      when v is a",
            "        nothing",
            "      else",
            "        nothing",
            "      end",
            "      match v",
            "        case a",
            "          nothing",
            "      end",
        ] {
            lines.push(l.to_string());
        }
    }
    lines.extend(std::iter::repeat_n("      end".to_string(), depth));
    lines.push("    end".into());
    let outer_end = lines.len() as u32;
    for l in [
        "    row",
        "      slot = \"after\"",
        "    end",
        "  end",
        "  contract",
        "    slot is \"after\"",
        "  end",
        "end component deep",
    ] {
        lines.push(l.to_string());
    }
    (lines.join("\n") + "\n", outer_end)
}

/// Past the cap the parser says so once and stops descending (`MZ0411`); under it, the
/// same shapes check with no `MZ0411`. Either way the skip ends where the parser would,
/// so nothing after the deep block gets an error and no block is reported unclosed.
#[test]
fn nesting_past_the_cap_is_one_mz0411() {
    for (depth, mz0411) in [(50, 0), (100, 1), (5_000, 1)] {
        let (src, outer_end) = odd_rows(depth);
        assert_eq!(count(&src, "MZ0411"), mz0411, "{depth} nested rows");
        assert_eq!(
            count(&src, "MZ0204"),
            0,
            "{depth} nested rows: no unclosed block"
        );
        assert_eq!(
            errors_after(&src, outer_end),
            Vec::<String>::new(),
            "{depth} nested rows"
        );
    }
    for kw in ["when", "if"] {
        assert_eq!(
            count(&deep_service(kw, 50), "MZ0411"),
            0,
            "50 nested `{kw}`"
        );
        assert_eq!(
            count(&deep_service(kw, 5_000), "MZ0411"),
            1,
            "5,000 nested `{kw}`"
        );
    }
}

/// A block past the cap that is short of its `end`s, followed by `end component`: the
/// skip leaves that line to the parser, which reports the open blocks once (`MZ0204`), as
/// it does under the cap, and its fix inserts exactly the missing `end`s. It used to read
/// on to the end of the file, and every open block came back as its own `MZ0204`.
#[test]
fn end_component_still_ends_a_short_deep_block() {
    for depth in [40, 100] {
        let src = format!(
            "component deep\n  view\n{}{}end component deep\n",
            "    row\n".repeat(depth),
            "    end\n".repeat(10)
        );
        assert_eq!(count(&src, "MZ0204"), 1, "{depth} rows, 10 ends");
        // The one fix closes every open block: the rows not yet closed, and the view.
        let fix = check(&src, "case.mz")
            .diagnostics
            .into_iter()
            .find(|d| d.code == "MZ0204")
            .and_then(|d| d.fix)
            .expect("MZ0204 carries a fix");
        assert_eq!(
            fix.replace.matches("end\n").count(),
            depth - 10 + 1,
            "{depth} rows"
        );
    }
}

/// The skip in a handler stops at the next route, as the parser does, even when the
/// deep block is short an `end`: the route after it is still read.
#[test]
fn a_deep_handler_does_not_swallow_the_next_route() {
    for missing in [0, 1] {
        let mut src = deep_service("when", 200);
        let tail = "  end\nend service deep\n";
        assert!(src.ends_with(tail));
        src.truncate(src.len() - tail.len());
        for _ in 0..missing {
            let at = src.rfind("    end\n").expect("an end");
            src.replace_range(at..at + "    end\n".len(), "");
        }
        src.push_str("  end\n  route r2\n    get \"/y\"\n    respond 200 json item name \"b\"\n  end\nend service deep\n");
        let (program, _) = check_program(&src, "case.mz");
        let Some(Program::Service(service)) = program else {
            panic!("missing {missing}: no service came back");
        };
        let names: Vec<_> = service.routes.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.contains(&"r2"),
            "missing {missing}: routes are {names:?}"
        );
    }
}

#[test]
fn long_and_odd_lines_terminate() {
    run("one 1 MiB line", "a".repeat(1 << 20));
    run("1 MiB of newlines", "\n".repeat(1 << 20));
    run(
        "an unterminated string",
        format!(
            "component x\n  view\n    row\n      slot = \"{}",
            "x".repeat(100_000)
        ),
    );
    run("only CRs", "\r".repeat(10_000));
    run("a NUL file", "\u{0}".repeat(10_000));
    run("a BOM and nothing", "\u{feff}".to_string());
    run("empty", String::new());
}

/// A `program` whose `fn main` holds `body`.
fn deep_program(body: &str) -> String {
    format!("program deep\n\n  fn main\n{body}  end fn main\n\nend program deep\n")
}

/// A program's expressions and `when` blocks under the same cap (`MZ0411`): 100,000
/// nested parentheses, `not`s, prefix `-`s, `+`s and `<`s on one line, and 5,000 nested
/// `when`s, each give exactly one diagnostic, `MZ0411`. Before the cap a debug build
/// aborted at 2,000 parentheses and at 3,000 `when`s.
#[test]
fn a_deep_program_is_one_mz0411() {
    let n = 100_000;
    let cases = [
        (
            "100,000 nested parentheses",
            format!("{}1{}", "(".repeat(n), ")".repeat(n)),
        ),
        ("100,000 `not`s", format!("{}true", "not ".repeat(n))),
        ("100,000 prefix `-`s", format!("{}1", "- ".repeat(n))),
        ("100,000 `+`s", format!("1{}", " + 1".repeat(n))),
        ("100,000 chained `<`s", format!("1{}", " < 1".repeat(n))),
        (
            "100,000 nested parentheses in an interpolation",
            format!("\"{{{}1{}}}\"", "(".repeat(n), ")".repeat(n)),
        ),
    ];
    for (label, value) in cases {
        let src = deep_program(&format!("    let x = {value}\n    print(\"{{x}}\")\n"));
        let codes: Vec<_> = check(&src, "case.mz")
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect();
        assert_eq!(codes, ["MZ0411"], "{label}");
        run(label, src);
    }
    let src = deep_program(&format!(
        "{}    print(\"in\")\n{}    print(\"after\")\n",
        "    when true\n".repeat(5_000),
        "    end\n".repeat(5_000)
    ));
    let codes: Vec<_> = check(&src, "case.mz")
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(codes, ["MZ0411"], "5,000 nested `when`s");
    run("5,000 nested `when`s", src);
}

/// A kind of expression nesting: its name, and the expression `n` levels of it deep.
type Shape = (&'static str, fn(usize) -> String);

/// The deepest programs the cap lets through check clean and lower on the 1 MiB stack, so
/// the checker's and the lowering's walks stay within the stack too. A program's blocks
/// and expressions share one budget of 32 levels: the program, the `fn` and the `let`'s
/// expression take three, and the other 29 are split between nested `when`s and one kind
/// of expression nesting, each around a `+` chain that makes the operator tree 64 deep
/// (with `not`, the chain is one shorter and `> 0` is the 64th level). One level more,
/// of the shape or of `when`s, is `MZ0411`.
#[test]
fn the_deepest_program_under_the_cap_checks_and_lowers() {
    let shapes: [Shape; 4] = [
        ("parentheses", |n| {
            format!("{}{}{}", "(".repeat(n), chain(), ")".repeat(n))
        }),
        ("calls", |n| {
            format!("{}{}{}", "f(".repeat(n), chain(), ")".repeat(n))
        }),
        ("prefix `-`", |n| format!("{}({})", "- ".repeat(n), chain())),
        ("`not`", |n| {
            format!("{}({} > 0)", "not ".repeat(n), chain_of(62))
        }),
    ];
    for (name, shape) in shapes {
        // The parentheses a prefix operator's operand needs are one level of their own.
        let room = if name == "parentheses" || name == "calls" {
            29
        } else {
            28
        };
        // The `print("{x}")` beside the `let` takes three levels: the statement's
        // expression, the call's argument and the `{x}`.
        for whens in [0, 13, 27] {
            let value = shape(room - whens);
            let src = deep_main(whens, &value);
            let errors: Vec<_> = check(&src, "case.mz")
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| format!("{} {}", d.code, d.say))
                .collect();
            assert_eq!(errors, Vec::<String>::new(), "{whens} `when`s, {name}");
            let (program, _) = check_program(&src, "case.mz");
            assert!(matches!(program, Some(Program::Program(_))));
            run(&format!("{whens} `when`s around {name}"), src);
            // One level more of the shape is past the cap; one more `when`, too.
            let over = deep_main(whens, &shape(room - whens + 1));
            assert_eq!(count(&over, "MZ0411"), 1, "{whens} `when`s, {name} + 1");
            let over = deep_main(whens + 1, &value);
            assert_eq!(count(&over, "MZ0411"), 1, "{} `when`s, {name}", whens + 1);
        }
    }
}

/// `1 + 1 + …`, 64 levels deep: the longest chain the cap lets through.
fn chain() -> String {
    chain_of(63)
}

/// `1` and `n` more `+ 1`s: a tree `n + 1` levels deep.
fn chain_of(n: usize) -> String {
    format!("1{}", " + 1".repeat(n))
}

/// `fn main` holding `whens` nested `when`s around `let x = value`, and an `fn f`.
fn deep_main(whens: usize, value: &str) -> String {
    format!(
        "program deep\n\n  fn main\n{}      let x = {value}\n      print(\"{{x}}\")\n{}  end fn main\n\n  fn f(n: int): int\n    return n\n  end fn f\n\nend program deep\n",
        "    when true\n".repeat(whens),
        "    end\n".repeat(whens)
    )
}

/// RFC-0013 §7's blocks share the program's nesting budget: 5,000 nested `while`s,
/// `for each`es, `match`es or `when`s used as values each give exactly one `MZ0411`, and
/// the deepest nesting under the cap checks and lowers on the 1 MiB stack. An `else when`
/// chain is flat, so 10,000 links cost no nesting at all.
#[test]
fn deep_control_flow_is_one_mz0411_and_a_long_chain_is_flat() {
    let n = 5_000;
    let shapes: [(&str, String, String); 4] = [
        (
            "`while`",
            "    while true\n".repeat(n),
            "    end\n".repeat(n),
        ),
        (
            "`for each`",
            (0..n)
                .map(|k| format!("    for each i{k} in range(0, to = 1)\n"))
                .collect(),
            "    end\n".repeat(n),
        ),
        (
            "`match`",
            "    match 1\n      case 1\n".repeat(n),
            "      else\n    end\n".repeat(n),
        ),
        (
            "`when`s around a `when` used as a value",
            format!("{}    let x = when true\n", "    when true\n".repeat(n)),
            format!(
                "      1\n    else\n      2\n    end\n{}",
                "    end\n".repeat(n)
            ),
        ),
    ];
    for (label, open, close) in shapes {
        let src = deep_program(&format!("{open}    print(\"in\")\n{close}"));
        let codes: Vec<_> = check(&src, "case.mz")
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect();
        assert_eq!(codes, ["MZ0411"], "5,000 nested {label}");
        run(&format!("5,000 nested {label}"), src);
    }
    // Under the cap: the program and `fn main` take two levels, each loop one, and the
    // `print`'s statement, argument and `{i0}` three.
    let loops = 27;
    let src = deep_program(&format!(
        "{}    print(\"{{i0}}\")\n{}",
        (0..loops)
            .map(|k| format!("    for each i{k} in range(0, to = 1)\n"))
            .collect::<String>(),
        "    end\n".repeat(loops)
    ));
    assert_eq!(count(&src, "MZ0411"), 0);
    assert_eq!(check(&src, "case.mz").error_count(), 0);
    run("27 nested `for each`es", src);
    let mut chain = String::from("    let n = 7\n    when n is 0\n      print(0)\n");
    for k in 1..10_000 {
        chain.push_str(&format!("    else when n is {k}\n      print({k})\n"));
    }
    chain.push_str("    end\n");
    let src = deep_program(&chain);
    assert_eq!(check(&src, "case.mz").error_count(), 0);
    run("an `else when` chain of 10,000 links", src);
}

/// RFC-0013 §12's forms under the same cap: 100,000 prefix `try`s, `.name`s and postfix
/// `?`s on one line, and 5,000 nested `match`es on results, each give exactly one
/// diagnostic, `MZ0411`, and every pass after the parser stays within the 1 MiB stack.
#[test]
fn deep_errors_are_one_mz0411() {
    let n = 100_000;
    let program = |body: &str| {
        format!(
            "program deep\n\n  fn main: result(none, text)\n{body}  end fn main\n\n  fn check(n: int): result(int, text)\n    return n\n  end fn check\n\nend program deep\n"
        )
    };
    let cases = [
        (
            "100,000 prefix `try`s",
            format!("{}check(1)", "try ".repeat(n)),
        ),
        ("100,000 `.name`s", format!("(1){}", ".a".repeat(n))),
        ("100,000 postfix `?`s", format!("check(1){}", "?".repeat(n))),
    ];
    for (label, value) in cases {
        let src = program(&format!("    let x = {value}\n    print(\"{{x}}\")\n"));
        let codes: Vec<_> = check(&src, "case.mz")
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect();
        assert_eq!(codes, ["MZ0411"], "{label}");
        run(label, src);
    }
    let depth = 5_000;
    let mut body = String::new();
    for i in 0..depth {
        body.push_str(&format!("    match check({i})\n    case ok v{i}\n"));
    }
    body.push_str("    print(\"in\")\n");
    for i in (0..depth).rev() {
        body.push_str(&format!("    case error e{i}\n    print(e{i})\n    end\n"));
    }
    let src = program(&body);
    let codes: Vec<_> = check(&src, "case.mz")
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(codes, ["MZ0411"], "5,000 nested `match`es");
    run("5,000 nested `match`es", src);
}
