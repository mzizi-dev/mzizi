//! SECURITY.md's first promise, tested: the compiler terminates with a diagnostic on every
//! input. A panic, a stack overflow or a hang on a `.mz` file is a vulnerability there,
//! because the input is untrusted and an agent drives `mz` over it in a loop.
//!
//! No fuzzing crate: the compiler takes no dependencies, so the inputs come from a small
//! seeded generator. Each case runs every entry point an untrusted file reaches (`check`,
//! `check_contract`, `apply_exact_fixes` and its re-check, the NDJSON writer, and, when a
//! tree comes back, `outline`, the IR and service lowering), on a thread with a deadline,
//! so a hang fails the test rather than the CI job. A failing case prints its seed and its
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
