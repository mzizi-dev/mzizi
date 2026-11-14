//! `mz check <dir>` and `mz contract <dir>`: every `.mz` file under a directory, in sorted
//! path order, with one summary line. Run against the shipped binary, because the
//! behaviour is the command's exit status and its output, not a library function.

use std::path::{Path, PathBuf};
use std::process::Command;

const GOOD: &str = "component dot\n  prop size: int = 4\n  view\n    box\n      slot = \"dot\"\n    end\n  end\n  contract\n    slot is \"dot\"\n  end\nend component dot\n";
const BAD_CONTRACT: &str = "component dud\n  prop size: int = 4\n  view\n    box\n      slot = \"dud\"\n    end\n  end\n  contract\n    slot is \"other\"\n  end\nend component dud\n";
const BROKEN: &str = "component broken\n";

/// A fresh directory under the system temp dir, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("mz-dir-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }

    fn write(&self, rel: &str, text: &str) {
        let file = self.0.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mz(args: &[&str], dir: &Path) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_mz"))
        .args(args)
        .arg(dir)
        .output()
        .expect("mz runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_clean_directory_checks_and_sums_into_one_summary_line() {
    let d = Dir::new("clean");
    d.write("a.mz", GOOD);
    d.write("nested/deeper/b.mz", &GOOD.replace("dot", "dash"));
    d.write("notes.txt", "not a source file");
    let (code, stdout, _) = mz(&["check", "--agent"], d.path());
    assert_eq!(code, Some(0), "{stdout}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "{stdout}");
    assert!(
        lines[0].starts_with(r#"{"summary":true,"files":2,"errors":0,"#),
        "{stdout}"
    );
}

#[test]
fn diagnostics_come_in_sorted_path_order_each_with_its_own_file() {
    let d = Dir::new("order");
    d.write("z.mz", BROKEN);
    d.write("a/y.mz", BROKEN);
    d.write("a/b.mz", BROKEN);
    let (code, stdout, _) = mz(&["check", "--agent"], d.path());
    assert_eq!(code, Some(1), "{stdout}");
    let files: Vec<&str> = stdout
        .lines()
        .filter(|l| l.starts_with(r#"{"code""#))
        .map(|l| {
            let at = l.find(r#""file":""#).unwrap() + 8;
            let rest = &l[at..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect();
    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(files, sorted, "{stdout}");
    assert!(files.first().unwrap().ends_with("a/b.mz"), "{files:?}");
    assert!(files.last().unwrap().ends_with("z.mz"), "{files:?}");
    let summary = stdout.lines().last().unwrap();
    assert!(summary.contains(r#""files":3"#), "{summary}");
}

#[test]
fn one_failing_file_fails_the_directory_and_the_others_still_run() {
    let d = Dir::new("mixed");
    d.write("a_good.mz", GOOD);
    d.write("b_bad.mz", BAD_CONTRACT);
    d.write("c_good.mz", &GOOD.replace("dot", "dash"));
    let (code, stdout, _) = mz(&["contract", "--agent"], d.path());
    assert_eq!(code, Some(1), "{stdout}");
    let summary = stdout.lines().last().unwrap();
    assert!(summary.contains(r#""files":3"#), "{summary}");
    assert!(summary.contains(r#""contract_clauses":3"#), "{summary}");
    assert!(summary.contains(r#""contract_failures":1"#), "{summary}");
    assert!(stdout.contains("b_bad.mz"), "{stdout}");
}

#[test]
fn the_human_form_ends_with_one_summary_line() {
    let d = Dir::new("human");
    d.write("a.mz", GOOD);
    let (code, stdout, _) = mz(&["contract"], d.path());
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.starts_with("mz: 1 files, 0 errors"), "{stdout}");
    assert!(stdout.contains("1 contract clauses, 0 failed"), "{stdout}");
}

#[test]
fn hidden_directories_and_target_are_skipped() {
    let d = Dir::new("skip");
    d.write("a.mz", GOOD);
    d.write(".git/x.mz", BROKEN);
    d.write("target/y.mz", BROKEN);
    let (code, stdout, _) = mz(&["check"], d.path());
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("1 files"), "{stdout}");
}

#[test]
fn a_directory_with_no_source_file_is_a_usage_problem() {
    let d = Dir::new("empty");
    d.write("readme.txt", "nothing to check");
    let (code, stdout, stderr) = mz(&["check"], d.path());
    assert_eq!(code, Some(2), "{stdout}");
    assert!(stderr.contains("no .mz files under"), "{stderr}");
}

#[test]
fn other_commands_refuse_a_directory() {
    let d = Dir::new("refuse");
    d.write("a.mz", GOOD);
    for word in ["outline", "hash", "ir", "fix"] {
        let (code, _, stderr) = mz(&[word], d.path());
        assert_eq!(code, Some(2), "{word}: {stderr}");
        assert!(stderr.contains("takes one file"), "{word}: {stderr}");
    }
}

#[test]
fn the_repository_primitives_and_examples_check_as_directories() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for sub in ["primitives", "examples"] {
        let (code, stdout, stderr) = mz(&["check", "--agent"], &root.join(sub));
        assert_eq!(code, Some(0), "{sub}: {stdout}{stderr}");
        assert!(stdout.contains(r#""errors":0"#), "{sub}: {stdout}");
    }
}
