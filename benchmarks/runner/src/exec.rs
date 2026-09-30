//! Injectable command templates for the compile check and the scorer.
//!
//! A template is an argv list with `{placeholders}` and a working directory. The defaults
//! are the shared interfaces; `--check-cmd` / `--score-cmd` replace them with a JSON array
//! (used by tests, and by anyone running against a different toolchain). The resolved argv
//! is written into each episode's `meta.json`, so what actually ran is on record.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::Value;

use crate::arm::ArmConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandTemplate {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Clone, Debug)]
pub struct RunOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    /// Exit 0: no errors (warnings allowed).
    Clean,
    /// Exit 1: the candidate has errors — the model's fault, counted as an iteration.
    Errors,
    /// Anything else (2 = setup/usage error, signal, …): the harness's fault, not the
    /// model's. Never counted as an iteration.
    Setup,
}

pub fn classify(code: Option<i32>) -> CheckStatus {
    match code {
        Some(0) => CheckStatus::Clean,
        Some(1) => CheckStatus::Errors,
        _ => CheckStatus::Setup,
    }
}

impl CommandTemplate {
    pub fn render(&self, vars: &[(&str, &str)]) -> Vec<String> {
        self.argv
            .iter()
            .map(|a| {
                let mut s = a.clone();
                for (k, v) in vars {
                    s = s.replace(&format!("{{{k}}}"), v);
                }
                s
            })
            .collect()
    }

    pub fn run(&self, vars: &[(&str, &str)]) -> Result<RunOutput, String> {
        let argv = self.render(vars);
        let (prog, args) = argv
            .split_first()
            .ok_or_else(|| "empty command template".to_string())?;
        let started = Instant::now();
        let out = Command::new(prog)
            .args(args)
            .current_dir(&self.cwd)
            .output()
            .map_err(|e| format!("failed to run `{}`: {e}", argv.join(" ")))?;
        Ok(RunOutput {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            ms: started.elapsed().as_millis() as u64,
        })
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({ "argv": self.argv, "cwd": self.cwd.to_string_lossy() })
    }

    pub fn from_json(v: &Value) -> Result<CommandTemplate, String> {
        let argv = v
            .get("argv")
            .and_then(Value::as_array)
            .ok_or("command template missing argv")?
            .iter()
            .map(|a| {
                a.as_str()
                    .map(str::to_string)
                    .ok_or("argv entry not a string")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let cwd = v
            .get("cwd")
            .and_then(Value::as_str)
            .ok_or("command template missing cwd")?;
        Ok(CommandTemplate {
            argv,
            cwd: PathBuf::from(cwd),
        })
    }
}

/// How a check's stdout is rewritten before it is recorded and fed back to the author.
///
/// An arm is three things: a guide (the system message), a check command, and this. The
/// check command's output is the arm's own; the normaliser is where anything in it that
/// describes the harness rather than the candidate comes out, so that no arm pays tokens for
/// the machine it ran on. Pilot 2 (`benchmarks/results/2026-09-27-pilot-2/RUN.md`, "Threats
/// to validity") measured the cost of not doing this: `mz check --agent` echoed the
/// candidate's absolute path on every diagnostic line, about 89 tokens each, while the Dioxus
/// arm's `check.sh` already printed the bare file name. The default applies to every arm, so
/// a new arm's checker gets the same treatment without arm-specific code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Normaliser {
    /// Leave the output exactly as the checker printed it.
    None,
    /// Replace the candidate's path, as passed to the checker, with its file name
    /// (`/…/iter-01/candidate.mz` → `candidate.mz`). The default.
    FileName,
}

impl Normaliser {
    pub fn parse(s: &str) -> Result<Normaliser, String> {
        match s {
            "none" => Ok(Normaliser::None),
            "file-name" => Ok(Normaliser::FileName),
            other => Err(format!(
                "unknown normaliser `{other}` (expected file-name or none)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Normaliser::None => "none",
            Normaliser::FileName => "file-name",
        }
    }

    /// Rewrite `stdout` from a check that was passed `candidate`.
    pub fn apply(self, stdout: &str, candidate: &Path) -> String {
        match self {
            Normaliser::None => stdout.to_string(),
            Normaliser::FileName => {
                let Some(name) = candidate.file_name().map(|n| n.to_string_lossy()) else {
                    return stdout.to_string();
                };
                let full = candidate.to_string_lossy();
                if full.is_empty() || full == name {
                    return stdout.to_string();
                }
                stdout.replace(full.as_ref(), &name)
            }
        }
    }
}

/// Parse a `--check-cmd` / `--score-cmd` value: a JSON array of strings.
pub fn parse_argv_json(s: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(s).map_err(|e| format!("not a JSON array: {e}"))?;
    let arr = v.as_array().ok_or("not a JSON array")?;
    let argv = arr
        .iter()
        .map(|a| {
            a.as_str()
                .map(str::to_string)
                .ok_or("array entries must be strings")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if argv.is_empty() {
        return Err("command array is empty".into());
    }
    Ok(argv)
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// An arm's compile check, from its `arm.toml` (`{repo}` already resolved when the arm
/// loaded). It runs in the repo root. Remaining placeholder: `{file}`.
pub fn default_check(arm: &ArmConfig, repo: &Path) -> CommandTemplate {
    CommandTemplate {
        argv: arm.check.clone(),
        cwd: repo.to_path_buf(),
    }
}

/// The shared scoring interface. Placeholders: `{arm}`, `{candidate}`, `{reference}`.
pub fn default_score(repo: &Path) -> CommandTemplate {
    CommandTemplate {
        argv: s(&[
            "cargo",
            "run",
            "-q",
            "-p",
            "mzizi-benchmark-harness",
            "--",
            "score",
            "--arm",
            "{arm}",
            "--candidate",
            "{candidate}",
            "--reference",
            "{reference}",
        ]),
        cwd: repo.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_every_placeholder() {
        let t = CommandTemplate {
            argv: s(&["x", "{repo}/a/{file}", "{file}"]),
            cwd: PathBuf::from("/"),
        };
        assert_eq!(
            t.render(&[("repo", "/r"), ("file", "c.mz")]),
            s(&["x", "/r/a/c.mz", "c.mz"])
        );
    }

    #[test]
    fn exit_codes_classify() {
        assert_eq!(classify(Some(0)), CheckStatus::Clean);
        assert_eq!(classify(Some(1)), CheckStatus::Errors);
        assert_eq!(classify(Some(2)), CheckStatus::Setup);
        assert_eq!(classify(None), CheckStatus::Setup);
    }

    #[test]
    fn the_file_name_normaliser_strips_only_the_candidate_path() {
        let cand = Path::new("/tmp/results/m/mzizi/badge/seed-1/iter-01/candidate.mz");
        let out = concat!(
            r#"{"code":"MZ0304","file":"/tmp/results/m/mzizi/badge/seed-1/iter-01/candidate.mz","span":[1,1,1,2]}"#,
            "\n",
            r#"{"summary":true,"errors":1}"#,
            "\n"
        );
        let n = Normaliser::FileName.apply(out, cand);
        assert!(n.contains(r#""file":"candidate.mz""#), "{n}");
        assert!(!n.contains("/tmp/results"));
        assert_eq!(Normaliser::None.apply(out, cand), out);
        // Output that never mentions the path (check.sh already prints the bare name) and
        // a candidate passed without a directory are both left alone.
        let bare = "error: candidate.rs:3:1\n";
        assert_eq!(Normaliser::FileName.apply(bare, cand), bare);
        assert_eq!(
            Normaliser::FileName.apply(out, Path::new("candidate.mz")),
            out
        );
        assert_eq!(
            Normaliser::parse("file-name").unwrap(),
            Normaliser::FileName
        );
        assert!(Normaliser::parse("basename").is_err());
    }

    #[test]
    fn json_round_trip_and_argv_parse() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let t = default_check(&ArmConfig::load(&repo, "dioxus").unwrap(), &repo);
        assert_eq!(CommandTemplate::from_json(&t.to_json()).unwrap(), t);
        assert_eq!(parse_argv_json(r#"["sh","-c","exit 0"]"#).unwrap().len(), 3);
        assert!(parse_argv_json("[]").is_err());
        assert!(parse_argv_json("sh -c").is_err());
    }
}
