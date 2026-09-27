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

use crate::prompt::Arm;

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

/// The shared compile-check interface for each arm, with `{repo}` already resolved.
/// Remaining placeholder: `{file}`.
pub fn default_check(arm: Arm, repo: &Path) -> CommandTemplate {
    let argv = match arm {
        Arm::Mzizi => s(&[
            "cargo",
            "run",
            "-q",
            "--manifest-path",
            "{repo}/compiler/Cargo.toml",
            "--bin",
            "mz",
            "--",
            "check",
            "--agent",
            "{file}",
        ]),
        Arm::Dioxus => s(&["{repo}/benchmarks/arms/dioxus/check.sh", "{file}"]),
    };
    let t = CommandTemplate {
        argv,
        cwd: repo.to_path_buf(),
    };
    CommandTemplate {
        argv: t.render(&[("repo", &repo.to_string_lossy())]),
        cwd: t.cwd,
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
    fn json_round_trip_and_argv_parse() {
        let t = default_check(Arm::Dioxus, Path::new("/repo"));
        assert_eq!(CommandTemplate::from_json(&t.to_json()).unwrap(), t);
        assert_eq!(parse_argv_json(r#"["sh","-c","exit 0"]"#).unwrap().len(), 3);
        assert!(parse_argv_json("[]").is_err());
        assert!(parse_argv_json("sh -c").is_err());
    }
}
