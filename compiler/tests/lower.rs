//! RFC-0011 §8: `mz build` and the lowering to Rust and axum.
//!
//! The generated package is compiled and its tests run in CI's `lowering` job, which needs
//! crates.io. These tests stay offline: they check what the lowering writes, and that the
//! shipped binary builds only what it should.

use std::path::PathBuf;
use std::process::Command;

use mzizi_lang_compiler::lower::{PINS, lower};
use mzizi_lang_compiler::parse::{Program, parse_program};
use mzizi_lang_compiler::service::{ClauseKind, Service};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn registry() -> Service {
    let path = root().join("examples/registry.mz");
    let src = std::fs::read_to_string(&path).unwrap();
    let (program, _) = parse_program(&src, path.to_str().unwrap());
    let Some(Program::Service(s)) = program else {
        panic!("a service")
    };
    s
}

fn main_rs(s: &Service) -> String {
    let p = lower(s, "registry.mz");
    p.files
        .into_iter()
        .find(|(n, _)| n == "src/main.rs")
        .unwrap()
        .1
}

#[test]
fn the_lowering_is_deterministic() {
    let s = registry();
    let a = lower(&s, "registry.mz");
    let b = lower(&s, "registry.mz");
    assert_eq!(a.files, b.files);
    assert_eq!(a.fixtures, ["fixtures/ui.json"]);
}

#[test]
fn every_example_becomes_one_oneshot_test_and_no_ensure_does() {
    let s = registry();
    let main = main_rs(&s);
    let clauses = &s.contract.as_ref().unwrap().clauses;
    let examples = clauses
        .iter()
        .filter(|c| matches!(c.kind, ClauseKind::Example { .. }))
        .count();
    assert_eq!(main.matches("#[tokio::test]").count(), examples);
    assert_eq!(examples, 18);
    assert!(main.contains("app().oneshot(req)"));
}

#[test]
fn the_route_table_and_handlers_are_generated() {
    let main = main_rs(&registry());
    assert!(main.contains(
        "RouteDef { method: Method::Get, segs: &[Seg::Lit(\"v1\"), Seg::Lit(\"ui\"), Seg::Param], id: 2 }"
    ));
    assert!(main.contains("fn route_ui_item(p_name: String) -> Reply {"));
    assert!(main.contains("if p_name == \"badge\" || p_name == \"button\" {"));
    assert!(main.contains("(\"x-mzizi-source\", \"fixture\"),"));
    assert!(main.contains("fn fallback() -> Reply {"));
}

#[test]
fn no_request_path_can_panic() {
    // RFC-0011 §5: the generated code holds no `unwrap`, `expect`, indexing or `panic!`
    // outside the tests.
    let main = main_rs(&registry());
    let (serving, tests) = main.split_once("#[cfg(test)]").unwrap();
    for banned in [
        ".unwrap()",
        ".expect(",
        "panic!(",
        "unreachable!(",
        "todo!(",
    ] {
        assert!(!serving.contains(banned), "{banned} in the serving code");
    }
    assert!(!serving.contains("params["), "indexing in the serving code");
    assert!(tests.contains("expect("), "the tests may expect");
}

#[test]
fn the_package_pins_its_dependencies_exactly_and_is_its_own_workspace() {
    let p = lower(&registry(), "registry.mz");
    let cargo = &p.files.iter().find(|(n, _)| n == "Cargo.toml").unwrap().1;
    assert!(cargo.contains("name = \"mz-registry\""));
    assert!(cargo.contains("\n[workspace]\n"));
    for (name, version) in PINS {
        assert!(version.starts_with('='), "{name} is not pinned exactly");
        assert!(cargo.contains(version), "{name} {version} missing");
    }
}

#[test]
fn narrowing_lowers_to_if_let_and_options_serialise_as_null() {
    let src = "service t\n  record page\n    field limit: option(int)\n  end\n  route r\n    get \"/\"\n    query limit: option(int)\n    when limit is none\n      respond 200 json page limit limit\n    else\n      respond 200 text \"{limit}\"\n    end\n  end\n  contract\n    example get \"/\" status is 200\n  end\nend service t\n";
    let (program, diags) = parse_program(src, "t.mz");
    assert!(diags.is_empty(), "{diags:#?}");
    let Some(Program::Service(s)) = program else {
        panic!()
    };
    let main = main_rs(&s);
    assert!(main.contains("fn route_r(p_limit: Option<i64>) -> Reply {"));
    assert!(main.contains("if let Some(p_limit) = p_limit.clone() {"));
    assert!(main.contains("format!(\"{}\", p_limit)"));
    assert!(main.contains("None => b.push_str(\"null\")"));
}

fn mz(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(args)
        .output()
        .expect("mz runs")
}

#[test]
fn mz_build_writes_the_package_and_its_fixtures() {
    let out = std::env::temp_dir().join(format!("mz-build-{}", std::process::id()));
    let src = root().join("examples/registry.mz");
    let o = mz(&[
        "build",
        src.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    for f in ["Cargo.toml", "src/main.rs", "fixtures/ui.json"] {
        assert!(out.join(f).is_file(), "{f} not written");
    }
    assert_eq!(
        std::fs::read(out.join("fixtures/ui.json")).unwrap(),
        std::fs::read(root().join("examples/fixtures/ui.json")).unwrap()
    );
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn mz_build_refuses_what_it_cannot_lower() {
    let out = std::env::temp_dir().join(format!("mz-build-no-{}", std::process::id()));
    let out = out.to_str().unwrap();
    // A component: nothing lowers it yet.
    let button = root().join("primitives/button.mz");
    assert_eq!(
        mz(&["build", button.to_str().unwrap(), "--out", out])
            .status
            .code(),
        Some(2)
    );
    // No `--out`.
    let reg = root().join("examples/registry.mz");
    assert_eq!(mz(&["build", reg.to_str().unwrap()]).status.code(), Some(2));
    // A service with an error: exit 1, and nothing written.
    let bad = std::env::temp_dir().join(format!("mz-bad-{}.mz", std::process::id()));
    std::fs::write(
        &bad,
        "service t\n  route r\n    get \"/r\"\n  end\nend service t\n",
    )
    .unwrap();
    assert_eq!(
        mz(&["build", bad.to_str().unwrap(), "--out", out])
            .status
            .code(),
        Some(1)
    );
    assert!(!PathBuf::from(out).exists());
    std::fs::remove_file(bad).unwrap();
}

/// The `.mz` file's name goes into a comment in both generated files. A name with a
/// newline used to end the comment and put the rest of the name into the code: a file
/// called `x\nfn injected() {}\n.mz` produced a `main.rs` with `fn injected() {}` in it.
#[test]
fn a_file_name_cannot_write_into_the_generated_code() {
    let s = registry();
    let p = lower(&s, "x\nfn injected() {}\r\n[dependencies.evil]\n.mz");
    for (name, text) in &p.files {
        for line in text.lines() {
            assert!(
                !line.trim_start().starts_with("fn injected")
                    && !line.trim_start().starts_with("[dependencies.evil]"),
                "{name} carries a line from the file name: {line:?}"
            );
        }
        assert!(
            text.contains(r"x\nfn injected() {}\r\n[dependencies.evil]\n.mz"),
            "{name} should name the file with its newlines escaped"
        );
    }
    // An ordinary name is written as it is, and a literal backslash cannot pass for an
    // escaped newline.
    assert!(main_rs(&s).starts_with("//! Generated by `mz build` from `registry.mz`."));
    let slash = lower(&s, r"x\nfn.mz");
    assert!(
        slash.files[1]
            .1
            .starts_with(r"//! Generated by `mz build` from `x\\nfn.mz`.")
    );
}
