//! `mzbench` — the Phase 0 benchmark runner. See `benchmarks/runner/README.md`.
//!
//! ```text
//! mzbench run --task <dir> --arm <mzizi|dioxus|leptos> --model-label <s> --seed <n>
//!             --temperature <f> --out <results> [--endpoint <url>] [--max-iters 5]
//!             [--max-tokens 4096] [--timeout-secs 3600] [common options]
//! mzbench episode start --task <dir> --arm <a> --model-label <s> --out <results>
//!             [--max-iters 5] [--seed 0] [--endpoint <url>] [common options]
//! mzbench episode submit <episode dir> <candidate file>
//! mzbench episode finish <episode dir> [--endpoint <url>]
//! mzbench summarize <results dir>
//!
//! common options: --repo <dir> --guide <file> --check-cmd <json argv> --score-cmd <json argv>
//! ```
//!
//! Exit status: 0 success (`submit`: CLEAN), 1 `submit` recorded ERRORS, 2 refused / usage /
//! setup error.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use mzizi_benchmark_runner::endpoint::{GenParams, HttpEndpoint};
use mzizi_benchmark_runner::episode::{
    EpisodeMeta, Mode, create_episode, episode_dir, finish, fnv1a64, load_state, run_model_episode,
    submit, submit_output,
};
use mzizi_benchmark_runner::exec::{
    CommandTemplate, default_check, default_score, parse_argv_json,
};
use mzizi_benchmark_runner::prompt::{Arm, Prompt, build_prompt};
use mzizi_benchmark_runner::summary::{collect, render};
use mzizi_benchmark_runner::task::load_task;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8080";

const USAGE: &str = "usage:
  mzbench run --task <dir> --arm <mzizi|dioxus|leptos> --model-label <s> --seed <n> --temperature <f>
              --out <results> [--endpoint <url>] [--max-iters 5] [--max-tokens 4096]
              [--timeout-secs 3600] [common]
  mzbench episode start --task <dir> --arm <a> --model-label <s> --out <results>
              [--max-iters 5] [--seed 0] [--endpoint <url>] [common]
  mzbench episode submit <episode dir> <candidate file>
  mzbench episode finish <episode dir> [--endpoint <url>]
  mzbench summarize <results dir>
common: --repo <dir> --guide <file> --check-cmd '<json argv>' --score-cmd '<json argv>'";

struct Flags {
    named: HashMap<String, String>,
    positional: Vec<String>,
}

fn parse_flags(args: &[String]) -> Result<Flags, String> {
    let mut named = HashMap::new();
    let mut positional = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(name) = a.strip_prefix("--") {
            let v = it.next().ok_or(format!("--{name} needs a value"))?;
            if named.insert(name.to_string(), v.clone()).is_some() {
                return Err(format!("--{name} given twice"));
            }
        } else {
            positional.push(a.clone());
        }
    }
    Ok(Flags { named, positional })
}

impl Flags {
    fn take(&mut self, k: &str) -> Option<String> {
        self.named.remove(k)
    }
    fn req(&mut self, k: &str) -> Result<String, String> {
        self.take(k).ok_or(format!("--{k} is required"))
    }
    fn num<T: std::str::FromStr>(&mut self, k: &str, default: Option<T>) -> Result<T, String> {
        match self.take(k) {
            Some(v) => v
                .parse()
                .map_err(|_| format!("--{k}: `{v}` is not a number")),
            None => default.ok_or(format!("--{k} is required")),
        }
    }
    fn done(self, max_positional: usize) -> Result<Vec<String>, String> {
        if let Some(k) = self.named.keys().next() {
            return Err(format!("unknown flag --{k}"));
        }
        if self.positional.len() > max_positional {
            return Err(format!(
                "unexpected argument `{}`",
                self.positional[max_positional]
            ));
        }
        Ok(self.positional)
    }
}

fn default_repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Build meta + prompt + dir for either mode, from the flags both share. The prompt comes
/// from the one shared builder, so `run` and `episode start` cannot word it differently.
fn prepare(f: &mut Flags, mode: Mode, seed: u64) -> Result<(EpisodeMeta, PathBuf, Prompt), String> {
    let task = load_task(Path::new(&f.req("task")?))?;
    let arm = Arm::parse(&f.req("arm")?)?;
    let model = f.req("model-label")?;
    let out = PathBuf::from(f.req("out")?);
    let max_iters: u32 = f.num("max-iters", Some(5))?;
    if max_iters == 0 {
        return Err("--max-iters must be at least 1".into());
    }
    let endpoint = f
        .take("endpoint")
        .unwrap_or_else(|| DEFAULT_ENDPOINT.into());
    let repo = std::fs::canonicalize(
        f.take("repo")
            .map(PathBuf::from)
            .unwrap_or_else(default_repo),
    )
    .map_err(|e| format!("--repo: {e}"))?;
    let guide_path = f
        .take("guide")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("benchmarks/prompts").join(arm.guide_file_name()));
    let guide = std::fs::read_to_string(&guide_path)
        .map_err(|e| format!("guide {}: {e}", guide_path.display()))?;
    let spec = std::fs::read_to_string(&task.spec_path)
        .map_err(|e| format!("spec {}: {e}", task.spec_path.display()))?;

    let template =
        |flag: Option<String>, default: CommandTemplate| -> Result<CommandTemplate, String> {
            Ok(match flag {
                Some(json) => CommandTemplate {
                    argv: parse_argv_json(&json)?,
                    cwd: repo.clone(),
                },
                None => default,
            })
        };
    let check = template(f.take("check-cmd"), default_check(arm, &repo))?;
    let mut score = template(f.take("score-cmd"), default_score(&repo))?;
    // Appended to a custom `--score-cmd` too, so the task's opt-in cannot be lost by
    // overriding the scorer; the argv as written to meta.json is what ran.
    if task.allow_variant_renames {
        score.argv.push("--allow-variant-renames".into());
    }

    let prompt = build_prompt(arm, &guide, &spec, &task.enums);
    let meta = EpisodeMeta {
        mode,
        task: task.name.clone(),
        enums: task.enums.clone(),
        rename_reason: task.rename_reason.clone(),
        task_dir: task.dir.clone(),
        reference_path: task.reference_path.clone(),
        arm,
        model: model.clone(),
        seed,
        temperature: None,
        max_tokens: None,
        max_iters,
        endpoint,
        guide_path: std::fs::canonicalize(&guide_path).unwrap_or(guide_path),
        guide_bytes: guide.len() as u64,
        guide_fnv1a64: fnv1a64(guide.as_bytes()),
        check,
        score,
    };
    let dir = episode_dir(&out, &model, arm, &task.name, seed)?;
    Ok((meta, dir, prompt))
}

fn cmd_run(args: &[String]) -> Result<ExitCode, String> {
    let mut f = parse_flags(args)?;
    let seed: u64 = f.num("seed", None)?;
    let temperature: f64 = f.num("temperature", None)?;
    let max_tokens: u64 = f.num("max-tokens", Some(4096))?;
    let timeout: u64 = f.num("timeout-secs", Some(3600))?;
    let (mut meta, dir, prompt) = prepare(&mut f, Mode::Model, seed)?;
    f.done(0)?;
    meta.temperature = Some(temperature);
    meta.max_tokens = Some(max_tokens);
    let dir = create_episode(&dir, &meta, &prompt)?;
    let ep = HttpEndpoint::new(&meta.endpoint, Duration::from_secs(timeout));
    let params = GenParams {
        model: meta.model.clone(),
        temperature,
        seed,
        max_tokens,
    };
    eprintln!("mzbench: episode {}", dir.display());
    let fin = run_model_episode(&dir, &prompt, &ep, &ep, &params)?;
    println!("{fin}");
    Ok(ExitCode::SUCCESS)
}

fn cmd_episode(args: &[String]) -> Result<ExitCode, String> {
    let (sub, rest) = args
        .split_first()
        .ok_or("episode needs start|submit|finish")?;
    match sub.as_str() {
        "start" => {
            let mut f = parse_flags(rest)?;
            let seed: u64 = f.num("seed", Some(0))?;
            let (meta, dir, prompt) = prepare(&mut f, Mode::Agent, seed)?;
            f.done(0)?;
            let dir = create_episode(&dir, &meta, &prompt)?;
            println!("{}", dir.display());
            eprintln!(
                "mzbench: prompt at {}/prompt.md; submit with `mzbench episode submit {} <file>` \
                 (max {} iterations)",
                dir.display(),
                dir.display(),
                meta.max_iters
            );
            Ok(ExitCode::SUCCESS)
        }
        "submit" => {
            let f = parse_flags(rest)?;
            let pos = f.done(2)?;
            let [dir, file] = pos.as_slice() else {
                return Err("episode submit <episode dir> <candidate file>".into());
            };
            let o = submit(Path::new(dir), Path::new(file))?;
            let st = load_state(Path::new(dir))?;
            print!("{}", submit_output(&o, &st.dir));
            Ok(if o.clean {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        "finish" => {
            let mut f = parse_flags(rest)?;
            let endpoint = f.take("endpoint");
            let pos = f.done(1)?;
            let [dir] = pos.as_slice() else {
                return Err("episode finish <episode dir> [--endpoint <url>]".into());
            };
            let st = load_state(Path::new(dir))?;
            let url = endpoint.unwrap_or(st.meta.endpoint.clone());
            let tok = HttpEndpoint::new(&url, Duration::from_secs(120));
            let fin = finish(&st.dir, &tok)?;
            println!("{fin}");
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown episode subcommand `{other}`")),
    }
}

fn cmd_summarize(args: &[String]) -> Result<ExitCode, String> {
    let f = parse_flags(args)?;
    let pos = f.done(1)?;
    let [root] = pos.as_slice() else {
        return Err("summarize <results dir>".into());
    };
    let (finals, unfinished) = collect(Path::new(root))?;
    print!("{}", render(&finals, &unfinished));
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.split_first() {
        Some((c, rest)) if c == "run" => cmd_run(rest),
        Some((c, rest)) if c == "episode" => cmd_episode(rest),
        Some((c, rest)) if c == "summarize" => cmd_summarize(rest),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mzbench: {e}");
            ExitCode::from(2)
        }
    }
}
