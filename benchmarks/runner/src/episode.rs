//! The episode state machine, shared by Mode 1 (`run`) and Mode 2 (`episode …`).
//!
//! An episode directory holds:
//!
//! ```text
//! meta.json            what ran: task, arm, model label, seed, commands, guide identity
//! system.txt user.txt  the exact messages from prompt::build_prompt (byte-for-byte)
//! prompt.md            the same two texts between marker lines, for a human or agent
//! iter-NN/             reply.md (Mode 1), candidate.<mz|rs>, diagnostics.txt,
//!                      stderr.txt, feedback.txt (only when feedback was actually sent)
//! score.json           the scorer's stdout, when a clean candidate was scored
//! episode.jsonl        one line per iteration, then one `kind:"final"` line
//! ```
//!
//! Both modes write iterations through [`record_iteration`] and close through [`finish`],
//! so "what counts as an iteration", "what is fed back" and "what is scored" cannot drift
//! between them.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{Value, json};

use crate::endpoint::{ChatModel, GenParams, Message, Tokenizer};
use crate::exec::{CheckStatus, CommandTemplate, Normaliser, classify};
use crate::extract::{ExtractError, extract_code_block};
use crate::prompt::{Arm, Prompt, compile_error_feedback, extraction_feedback};
use crate::task::safe_component;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Mode 1: the runner calls a model endpoint.
    Model,
    /// Mode 2: an external agent submits files.
    Agent,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Model => "model",
            Mode::Agent => "agent",
        }
    }
    fn parse(s: &str) -> Result<Mode, String> {
        match s {
            "model" => Ok(Mode::Model),
            "agent" => Ok(Mode::Agent),
            o => Err(format!("unknown mode `{o}` in meta.json")),
        }
    }
    pub fn token_source(self) -> &'static str {
        match self {
            Mode::Model => "endpoint_usage",
            Mode::Agent => "tokenizer_transcript_proxy",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EpisodeMeta {
    pub mode: Mode,
    pub task: String,
    /// The task's `enums` list, as passed to the prompt builder.
    pub enums: Vec<String>,
    /// The task's `rename_reason`, when it opts in to `allow_variant_renames` (the scorer
    /// argv then carries `--allow-variant-renames`). Recorded so the opt-in and its reason
    /// sit beside the score they changed.
    pub rename_reason: Option<String>,
    pub task_dir: PathBuf,
    pub reference_path: PathBuf,
    pub arm: Arm,
    pub model: String,
    pub seed: u64,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    pub max_iters: u32,
    /// Chat endpoint (Mode 1) and, in both modes, the `/tokenize` endpoint used for
    /// `transcript_tokens`.
    pub endpoint: String,
    pub guide_path: PathBuf,
    pub guide_bytes: u64,
    pub guide_fnv1a64: String,
    pub check: CommandTemplate,
    /// What is done to the check's stdout before it is recorded and fed back (see
    /// [`Normaliser`]). An episode whose `meta.json` predates the field ran without one, so
    /// it reads back as `none`.
    pub normaliser: Normaliser,
    pub score: CommandTemplate,
}

impl EpisodeMeta {
    pub fn to_json(&self) -> Value {
        json!({
            "mode": self.mode.as_str(),
            "task": self.task,
            "enums": self.enums,
            "allow_variant_renames": self.rename_reason.is_some(),
            "rename_reason": self.rename_reason,
            "task_dir": self.task_dir.to_string_lossy(),
            "reference_path": self.reference_path.to_string_lossy(),
            "arm": self.arm.as_str(),
            "model": self.model,
            "seed": self.seed,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "max_iters": self.max_iters,
            "endpoint": self.endpoint,
            "guide_path": self.guide_path.to_string_lossy(),
            "guide_bytes": self.guide_bytes,
            "guide_fnv1a64": self.guide_fnv1a64,
            "check": self.check.to_json(),
            "diagnostics_normaliser": self.normaliser.as_str(),
            "score": self.score.to_json(),
            // Recorded, not hidden: in agent mode the runner cannot stop the agent from
            // compiling outside `submit`, or from reading files outside the episode dir.
            "iteration_count_enforced_by_runner": self.mode == Mode::Model,
        })
    }

    pub fn from_json(v: &Value) -> Result<EpisodeMeta, String> {
        let st = |k: &str| -> Result<String, String> {
            v.get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or(format!("meta.json: missing `{k}`"))
        };
        let un = |k: &str| -> Result<u64, String> {
            v.get(k)
                .and_then(Value::as_u64)
                .ok_or(format!("meta.json: missing `{k}`"))
        };
        Ok(EpisodeMeta {
            mode: Mode::parse(&st("mode")?)?,
            task: st("task")?,
            enums: v
                .get("enums")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            rename_reason: v
                .get("rename_reason")
                .and_then(Value::as_str)
                .map(str::to_string),
            task_dir: PathBuf::from(st("task_dir")?),
            reference_path: PathBuf::from(st("reference_path")?),
            arm: Arm::parse(&st("arm")?)?,
            model: st("model")?,
            seed: un("seed")?,
            temperature: v.get("temperature").and_then(Value::as_f64),
            max_tokens: v.get("max_tokens").and_then(Value::as_u64),
            max_iters: un("max_iters")? as u32,
            endpoint: st("endpoint")?,
            guide_path: PathBuf::from(st("guide_path")?),
            guide_bytes: un("guide_bytes")?,
            guide_fnv1a64: st("guide_fnv1a64")?,
            check: CommandTemplate::from_json(v.get("check").ok_or("meta.json: no check")?)?,
            normaliser: match v.get("diagnostics_normaliser").and_then(Value::as_str) {
                Some(n) => Normaliser::parse(n)?,
                None => Normaliser::None,
            },
            score: CommandTemplate::from_json(v.get("score").ok_or("meta.json: no score")?)?,
        })
    }
}

/// FNV-1a 64-bit — identifies which guide text an episode ran with, without a dependency.
pub fn fnv1a64(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `<out>/<model>/<arm>/<task>/seed-<n>`
pub fn episode_dir(
    out: &Path,
    model: &str,
    arm: Arm,
    task: &str,
    seed: u64,
) -> Result<PathBuf, String> {
    Ok(out
        .join(safe_component(model)?)
        .join(arm.as_str())
        .join(safe_component(task)?)
        .join(format!("seed-{seed}")))
}

pub const PROMPT_SYSTEM_BEGIN: &str = "<!-- mzbench: SYSTEM MESSAGE BEGIN -->";
pub const PROMPT_SYSTEM_END: &str = "<!-- mzbench: SYSTEM MESSAGE END -->";
pub const PROMPT_USER_BEGIN: &str = "<!-- mzbench: USER MESSAGE BEGIN -->";
pub const PROMPT_USER_END: &str = "<!-- mzbench: USER MESSAGE END -->";

pub fn prompt_markdown(p: &Prompt) -> String {
    format!(
        "{PROMPT_SYSTEM_BEGIN}\n{}\n{PROMPT_SYSTEM_END}\n\n{PROMPT_USER_BEGIN}\n{}\n{PROMPT_USER_END}\n",
        p.system, p.user
    )
}

/// Create a fresh episode directory. Refuses to reuse an existing one: results are never
/// overwritten. Returns the canonical (absolute) path.
pub fn create_episode(dir: &Path, meta: &EpisodeMeta, prompt: &Prompt) -> Result<PathBuf, String> {
    if dir.exists() {
        return Err(format!(
            "{} already exists; refusing to overwrite results (use another --seed or --out)",
            dir.display()
        ));
    }
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let dir = fs::canonicalize(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let meta_text = serde_json::to_string_pretty(&meta.to_json()).map_err(|e| e.to_string())?;
    write(&dir.join("meta.json"), &format!("{meta_text}\n"))?;
    write(&dir.join("system.txt"), &prompt.system)?;
    write(&dir.join("user.txt"), &prompt.user)?;
    write(&dir.join("prompt.md"), &prompt_markdown(prompt))?;
    write(&dir.join("episode.jsonl"), "")?;
    Ok(dir)
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn append_line(dir: &Path, v: &Value) -> Result<(), String> {
    let path = dir.join("episode.jsonl");
    let mut f = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    writeln!(f, "{v}").map_err(|e| format!("append {}: {e}", path.display()))
}

pub struct EpisodeState {
    pub dir: PathBuf,
    pub meta: EpisodeMeta,
    pub iterations: Vec<Value>,
    pub final_line: Option<Value>,
}

impl EpisodeState {
    pub fn clean_iteration(&self) -> Option<u64> {
        self.iterations
            .iter()
            .find(|l| l["compile_ok"] == Value::Bool(true))
            .and_then(|l| l["iter"].as_u64())
    }
}

pub fn load_state(dir: &Path) -> Result<EpisodeState, String> {
    let dir = fs::canonicalize(dir).map_err(|e| format!("episode dir {}: {e}", dir.display()))?;
    let meta_text = fs::read_to_string(dir.join("meta.json"))
        .map_err(|e| format!("{}: not an episode dir ({e})", dir.display()))?;
    let meta_v: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta.json: {e}"))?;
    let meta = EpisodeMeta::from_json(&meta_v)?;
    let jsonl = fs::read_to_string(dir.join("episode.jsonl")).unwrap_or_default();
    let mut iterations = Vec::new();
    let mut final_line = None;
    for (n, line) in jsonl.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("episode.jsonl line {}: {e}", n + 1))?;
        match v["kind"].as_str() {
            Some("iteration") => iterations.push(v),
            Some("final") => final_line = Some(v),
            _ => return Err(format!("episode.jsonl line {}: unknown kind", n + 1)),
        }
    }
    Ok(EpisodeState {
        dir,
        meta,
        iterations,
        final_line,
    })
}

pub fn iter_dir(dir: &Path, iter: u32) -> PathBuf {
    dir.join(format!("iter-{iter:02}"))
}

/// One authored attempt, however it was obtained.
pub struct Submission<'a> {
    /// The raw model reply (Mode 1 only).
    pub reply: Option<&'a str>,
    /// The candidate source, or why none could be extracted from the reply.
    pub candidate: Result<&'a str, ExtractError>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub gen_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationOutcome {
    pub iter: u32,
    pub max_iters: u32,
    pub clean: bool,
    /// The user message to send next. `None` when clean, or when this was the last
    /// allowed iteration (nothing further is sent, so nothing is counted or shown).
    pub feedback: Option<String>,
}

/// Record iteration `iter`: write the candidate, run the arm's check, write diagnostics,
/// decide the feedback, append the JSONL line. A check that exits with anything other
/// than 0 or 1 is a harness setup error: it is returned as `Err` and **not** recorded as
/// an iteration, because it says nothing about the candidate.
pub fn record_iteration(
    dir: &Path,
    meta: &EpisodeMeta,
    iter: u32,
    sub: Submission<'_>,
) -> Result<IterationOutcome, String> {
    let idir = iter_dir(dir, iter);
    fs::create_dir_all(&idir).map_err(|e| format!("create {}: {e}", idir.display()))?;
    if let Some(reply) = sub.reply {
        write(&idir.join("reply.md"), reply)?;
    }

    let (compile_ok, diagnostics, check_ms, check_exit, extract_error, feedback) =
        match sub.candidate {
            Err(e) => {
                let d = e.to_string();
                write(&idir.join("diagnostics.txt"), &d)?;
                (
                    false,
                    d,
                    None,
                    None,
                    Some(e.to_string()),
                    Some(extraction_feedback(&e)),
                )
            }
            Ok(code) => {
                let cand = idir.join(format!("candidate.{}", meta.arm.extension()));
                write(&cand, code)?;
                let mut out = meta.check.run(&[("file", &cand.to_string_lossy())])?;
                // What the author sees, and what is recorded as what they saw, is the
                // normalised text; stderr is never fed back and is kept as printed.
                out.stdout = meta.normaliser.apply(&out.stdout, &cand);
                write(&idir.join("diagnostics.txt"), &out.stdout)?;
                write(&idir.join("stderr.txt"), &out.stderr)?;
                match classify(out.code) {
                    CheckStatus::Setup => {
                        return Err(format!(
                            "compile check setup error (exit {:?}); not counted as an \
                             iteration. stderr:\n{}",
                            out.code, out.stderr
                        ));
                    }
                    CheckStatus::Clean => (true, out.stdout, Some(out.ms), out.code, None, None),
                    CheckStatus::Errors => {
                        let fb = compile_error_feedback(&out.stdout);
                        (false, out.stdout, Some(out.ms), out.code, None, Some(fb))
                    }
                }
            }
        };

    let last = iter >= meta.max_iters;
    let feedback = if compile_ok || last { None } else { feedback };
    if let Some(fb) = &feedback {
        write(&idir.join("feedback.txt"), fb)?;
    }

    append_line(
        dir,
        &json!({
            "kind": "iteration",
            "iter": iter,
            "prompt_tokens": sub.prompt_tokens,
            "completion_tokens": sub.completion_tokens,
            "compile_ok": compile_ok,
            "diagnostics_chars": diagnostics.chars().count(),
            "check_ms": check_ms,
            "gen_ms": sub.gen_ms,
            "check_exit": check_exit,
            "extract_error": extract_error,
        }),
    )?;

    Ok(IterationOutcome {
        iter,
        max_iters: meta.max_iters,
        clean: compile_ok,
        feedback,
    })
}

/// Mode 2: record `candidate_file` as the next iteration.
pub fn submit(dir: &Path, candidate_file: &Path) -> Result<IterationOutcome, String> {
    let st = load_state(dir)?;
    if st.meta.mode != Mode::Agent {
        return Err("this episode was started by `mzbench run`, not `episode start`".into());
    }
    if st.final_line.is_some() {
        return Err("episode already finished; no further submissions".into());
    }
    if let Some(n) = st.clean_iteration() {
        return Err(format!(
            "iteration {n} already compiled cleanly; no further submissions. Run `mzbench \
             episode finish {}`",
            st.dir.display()
        ));
    }
    let done = st.iterations.len() as u32;
    if done >= st.meta.max_iters {
        return Err(format!(
            "max iterations ({}) reached; no further submissions. Run `mzbench episode finish {}`",
            st.meta.max_iters,
            st.dir.display()
        ));
    }
    let code = fs::read_to_string(candidate_file)
        .map_err(|e| format!("cannot read {}: {e}", candidate_file.display()))?;
    record_iteration(
        &st.dir,
        &st.meta,
        done + 1,
        Submission {
            reply: None,
            candidate: Ok(&code),
            prompt_tokens: None,
            completion_tokens: None,
            gen_ms: None,
        },
    )
}

/// What `episode submit` prints: the feedback Mode 1 would send (verbatim), then the
/// status line, then what to do next.
pub fn submit_output(o: &IterationOutcome, dir: &Path) -> String {
    let mut s = String::new();
    if let Some(fb) = &o.feedback {
        s.push_str(fb);
        if !fb.ends_with('\n') {
            s.push('\n');
        }
    }
    let status = if o.clean { "CLEAN" } else { "ERRORS" };
    s.push_str(&format!("ITERATION {}/{}: {status}\n", o.iter, o.max_iters));
    if o.clean || o.iter >= o.max_iters {
        s.push_str(&format!(
            "No further submissions. Run: mzbench episode finish {}\n",
            dir.display()
        ));
    }
    s
}

/// Tokenize the transcript: system + user, every recorded candidate, every feedback
/// message actually sent. Same files, same tokenizer, in both modes.
pub fn transcript_tokens(st: &EpisodeState, tok: &dyn Tokenizer) -> Result<u64, String> {
    let mut texts = vec![
        fs::read_to_string(st.dir.join("system.txt")).map_err(|e| e.to_string())?,
        fs::read_to_string(st.dir.join("user.txt")).map_err(|e| e.to_string())?,
    ];
    for line in &st.iterations {
        let n = line["iter"].as_u64().ok_or("iteration line without iter")? as u32;
        let idir = iter_dir(&st.dir, n);
        let cand = idir.join(format!("candidate.{}", st.meta.arm.extension()));
        for p in [cand, idir.join("feedback.txt")] {
            if p.is_file() {
                texts.push(fs::read_to_string(&p).map_err(|e| e.to_string())?);
            }
        }
    }
    let mut total = 0u64;
    for t in &texts {
        total += tok.count_tokens(t)?;
    }
    Ok(total)
}

struct ScoreResult {
    facts_checked: Option<u64>,
    defects: Option<u64>,
    class_token_jaccard: Option<f64>,
    /// Variants the scorer paired by class string under a different name. `None` when the
    /// scorer predates the field (or failed).
    renames: Option<u64>,
    error: Option<String>,
}

fn score(st: &EpisodeState, clean_iter: u32) -> Result<ScoreResult, String> {
    let cand = iter_dir(&st.dir, clean_iter).join(format!("candidate.{}", st.meta.arm.extension()));
    let out = st.meta.score.run(&[
        ("arm", st.meta.arm.harness_arm_str()),
        ("candidate", &cand.to_string_lossy()),
        ("reference", &st.meta.reference_path.to_string_lossy()),
    ])?;
    write(&st.dir.join("score.json"), &out.stdout)?;
    write(&st.dir.join("score.stderr.txt"), &out.stderr)?;
    let failed = |why: String| ScoreResult {
        facts_checked: None,
        defects: None,
        class_token_jaccard: None,
        renames: None,
        error: Some(why),
    };
    if out.code != Some(0) {
        return Ok(failed(format!("scorer exited {:?}", out.code)));
    }
    let parsed: Result<Value, _> = serde_json::from_str(out.stdout.trim());
    let v = match parsed {
        Ok(v) if v.is_object() => v,
        _ => return Ok(failed("scorer stdout is not one JSON object".into())),
    };
    match (v["facts_checked"].as_u64(), v["defects"].as_u64()) {
        (Some(f), Some(d)) => Ok(ScoreResult {
            facts_checked: Some(f),
            defects: Some(d),
            class_token_jaccard: v["class_token_jaccard"].as_f64(),
            renames: v["renames"].as_u64(),
            error: None,
        }),
        _ => Ok(failed("scorer JSON lacks facts_checked/defects".into())),
    }
}

fn sum_field(lines: &[Value], key: &str) -> Option<u64> {
    if lines.is_empty() {
        return None;
    }
    lines.iter().map(|l| l[key].as_u64()).sum()
}

/// Close an episode: score the clean candidate (if any) and append the final line.
/// A candidate that never compiled cleanly is never scored, so it is never defect-free.
pub fn finish(dir: &Path, tok: &dyn Tokenizer) -> Result<Value, String> {
    finish_ended(dir, tok, None)
}

/// Whether a chat error says the conversation no longer fits the model's context: the
/// error type llama.cpp's server reports (`exceed_context_size_error`), or the code an
/// OpenAI-compatible server uses (`context_length_exceeded`).
pub fn is_context_overflow(error: &str) -> bool {
    error.contains("exceed_context_size") || error.contains("context_length_exceeded")
}

/// [`finish`], recording why the episode stopped early when it did. `ended_by` is written
/// to the final line (`null` for an episode that ran its course).
pub fn finish_ended(
    dir: &Path,
    tok: &dyn Tokenizer,
    ended_by: Option<&str>,
) -> Result<Value, String> {
    let st = load_state(dir)?;
    if st.final_line.is_some() {
        return Err("episode already finished".into());
    }
    let clean_iter = st.clean_iteration();
    let sc = match clean_iter {
        Some(n) => Some(score(&st, n as u32)?),
        None => None,
    };
    let (transcript, transcript_reason) = match transcript_tokens(&st, tok) {
        Ok(n) => (Some(n), None),
        Err(e) => (None, Some(format!("tokenizer unavailable: {e}"))),
    };
    let (tp, tc) = match st.meta.mode {
        Mode::Model => (
            sum_field(&st.iterations, "prompt_tokens"),
            sum_field(&st.iterations, "completion_tokens"),
        ),
        Mode::Agent => (None, None),
    };
    let m = &st.meta;
    let v = json!({
        "kind": "final",
        "mode": m.mode.as_str(),
        "task": m.task,
        "arm": m.arm.as_str(),
        "model": m.model,
        "seed": m.seed,
        "temperature": m.temperature,
        "max_iters": m.max_iters,
        "iterations": st.iterations.len(),
        "iterations_to_clean": clean_iter,
        "total_prompt_tokens": tp,
        "total_completion_tokens": tc,
        "token_source": m.mode.token_source(),
        "transcript_tokens": transcript,
        "transcript_tokens_null_reason": transcript_reason,
        "clean": clean_iter.is_some(),
        "scored": sc.as_ref().is_some_and(|s| s.error.is_none()),
        "score_error": sc.as_ref().and_then(|s| s.error.clone()),
        "defects": sc.as_ref().and_then(|s| s.defects),
        "facts_checked": sc.as_ref().and_then(|s| s.facts_checked),
        "class_token_jaccard": sc.as_ref().and_then(|s| s.class_token_jaccard),
        "renames": sc.as_ref().and_then(|s| s.renames),
        "ended_by": ended_by,
    });
    append_line(&st.dir, &v)?;
    Ok(v)
}

/// Mode 1: drive a model through up to `max_iters` iterations, then finish.
pub fn run_model_episode(
    dir: &Path,
    prompt: &Prompt,
    model: &dyn ChatModel,
    tok: &dyn Tokenizer,
    params: &GenParams,
) -> Result<Value, String> {
    let st = load_state(dir)?;
    if st.meta.mode != Mode::Model || !st.iterations.is_empty() {
        return Err("run_model_episode needs a fresh model-mode episode".into());
    }
    let mut messages = vec![
        Message::new("system", &prompt.system),
        Message::new("user", &prompt.user),
    ];
    for iter in 1..=st.meta.max_iters {
        let started = Instant::now();
        let reply = match model.chat(&messages, params) {
            Ok(r) => r,
            // The conversation outgrew the model's context after the model's own attempts
            // and the arm's feedback: that is the edit loop failing, so the episode finishes
            // as not clean. Pilot 2's 7B Mzizi badge (seed 2) stopped this way, was left
            // unfinished by the runner, and had to be counted as not clean by hand in
            // RUN.md. On the first request the prompt alone is too big for the server,
            // which is a setup problem, and still aborts.
            Err(e) if iter > 1 && is_context_overflow(&e) => {
                let _ = write(&st.dir.join("error.txt"), &e);
                return finish_ended(&st.dir, tok, Some("context_exceeded"));
            }
            Err(e) => {
                let _ = write(&st.dir.join("error.txt"), &e);
                return Err(format!("iteration {iter}: model call failed: {e}"));
            }
        };
        let gen_ms = started.elapsed().as_millis() as u64;
        let extracted = extract_code_block(&reply.content);
        let outcome = record_iteration(
            &st.dir,
            &st.meta,
            iter,
            Submission {
                reply: Some(&reply.content),
                candidate: extracted.as_deref().map_err(Clone::clone),
                prompt_tokens: reply.prompt_tokens,
                completion_tokens: reply.completion_tokens,
                gen_ms: Some(gen_ms),
            },
        )
        .inspect_err(|e| {
            let _ = write(&st.dir.join("error.txt"), e);
        })?;
        if outcome.clean {
            break;
        }
        messages.push(Message::new("assistant", &reply.content));
        match outcome.feedback {
            Some(fb) => messages.push(Message::new("user", &fb)),
            None => break,
        }
    }
    finish(&st.dir, tok)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::endpoint::ChatReply;
    use crate::prompt::build_prompt;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let p = std::env::temp_dir().join(format!(
                "mzbench-test-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Fake check: clean iff the candidate contains "OK"; prints a diagnostic otherwise.
    fn fake_check() -> CommandTemplate {
        CommandTemplate {
            argv: vec![
                "sh".into(),
                "-c".into(),
                "if grep -q OK \"$1\"; then echo '{\"summary\":0}'; exit 0; else \
                 echo '{\"severity\":\"error\",\"message\":\"no OK\"}'; exit 1; fi"
                    .into(),
                "fake-check".into(),
                "{file}".into(),
            ],
            cwd: std::env::temp_dir(),
        }
    }

    fn fake_score(json_out: &str) -> CommandTemplate {
        CommandTemplate {
            argv: vec![
                "sh".into(),
                "-c".into(),
                format!(
                    "test \"$1\" = mzizi && test -f \"$2\" && test -f \"$3\" && echo '{json_out}'"
                ),
                "fake-score".into(),
                "{arm}".into(),
                "{candidate}".into(),
                "{reference}".into(),
            ],
            cwd: std::env::temp_dir(),
        }
    }

    struct FakeTok;
    impl Tokenizer for FakeTok {
        fn count_tokens(&self, text: &str) -> Result<u64, String> {
            Ok(text.split_whitespace().count() as u64)
        }
    }
    struct DownTok;
    impl Tokenizer for DownTok {
        fn count_tokens(&self, _: &str) -> Result<u64, String> {
            Err("connection refused".into())
        }
    }

    fn meta(tmp: &Path, mode: Mode, score: CommandTemplate) -> EpisodeMeta {
        let reference = tmp.join("reference.rs");
        fs::write(&reference, "// reference\n").unwrap();
        EpisodeMeta {
            mode,
            task: "button".into(),
            enums: vec![],
            rename_reason: None,
            task_dir: tmp.to_path_buf(),
            reference_path: reference,
            arm: Arm::Mzizi,
            model: "test-model".into(),
            seed: 1,
            temperature: if mode == Mode::Model { Some(0.0) } else { None },
            max_tokens: None,
            max_iters: 3,
            endpoint: "http://unused".into(),
            guide_path: tmp.join("guide.md"),
            guide_bytes: 5,
            guide_fnv1a64: fnv1a64(b"guide"),
            check: fake_check(),
            normaliser: Normaliser::FileName,
            score,
        }
    }

    const SCORE_OK: &str = r#"{"arm":"mzizi","facts_checked":5,"defects":1,"renames":2,"details":[],"class_token_jaccard":0.8}"#;

    fn start(tmp: &Path, mode: Mode, score: CommandTemplate) -> PathBuf {
        let m = meta(tmp, mode, score);
        let p = build_prompt(Arm::Mzizi, "the guide", "spec body", &[]);
        let dir = episode_dir(&tmp.join("out"), &m.model, m.arm, &m.task, m.seed).unwrap();
        create_episode(&dir, &m, &p).unwrap()
    }

    fn cand(tmp: &Path, name: &str, body: &str) -> PathBuf {
        let p = tmp.join(name);
        fs::write(&p, body).unwrap();
        p
    }

    fn jsonl(dir: &Path) -> Vec<Value> {
        fs::read_to_string(dir.join("episode.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn submit_errors_then_clean_then_refuses_then_finishes() {
        let t = TempDir::new("sm");
        let dir = start(&t.0, Mode::Agent, fake_score(SCORE_OK));
        assert!(dir.ends_with("out/test-model/mzizi/button/seed-1"));
        assert!(
            fs::read_to_string(dir.join("prompt.md"))
                .unwrap()
                .contains("the guide")
        );

        let o1 = submit(&dir, &cand(&t.0, "a.mz", "broken")).unwrap();
        assert!(!o1.clean);
        let fb = o1.feedback.clone().unwrap();
        assert!(fb.ends_with("{\"severity\":\"error\",\"message\":\"no OK\"}\n"));
        let out = submit_output(&o1, &dir);
        assert!(out.starts_with(&fb));
        assert!(out.ends_with("ITERATION 1/3: ERRORS\n"));

        let o2 = submit(&dir, &cand(&t.0, "b.mz", "now OK")).unwrap();
        assert!(o2.clean && o2.feedback.is_none());
        assert!(submit_output(&o2, &dir).starts_with("ITERATION 2/3: CLEAN\n"));

        let refused = submit(&dir, &cand(&t.0, "c.mz", "OK again")).unwrap_err();
        assert!(refused.contains("finish"), "{refused}");

        let fin = finish(&dir, &FakeTok).unwrap();
        assert_eq!(fin["clean"], true);
        assert_eq!(fin["iterations_to_clean"], 2);
        assert_eq!(fin["defects"], 1);
        assert_eq!(fin["facts_checked"], 5);
        assert_eq!(fin["class_token_jaccard"], 0.8);
        assert_eq!(fin["renames"], 2);
        assert_eq!(fin["token_source"], "tokenizer_transcript_proxy");
        assert_eq!(fin["total_prompt_tokens"], Value::Null);
        // system(2) + user + cand1(1) + feedback1 + cand2(2); feedback only when sent.
        let expect = FakeTok.count_tokens("the guide").unwrap()
            + FakeTok
                .count_tokens(&fs::read_to_string(dir.join("user.txt")).unwrap())
                .unwrap()
            + 1
            + FakeTok.count_tokens(&fb).unwrap()
            + 2;
        assert_eq!(fin["transcript_tokens"], expect);

        assert!(
            finish(&dir, &FakeTok).is_err(),
            "double finish must be refused"
        );
        assert!(submit(&dir, &cand(&t.0, "d.mz", "x")).is_err());
    }

    #[test]
    fn max_iters_reached_without_clean_is_never_scored() {
        let t = TempDir::new("max");
        // A scorer that would say "0 defects" if it were ever (wrongly) called.
        let dir = start(
            &t.0,
            Mode::Agent,
            fake_score(r#"{"facts_checked":9,"defects":0}"#),
        );
        for i in 1..=3 {
            let o = submit(&dir, &cand(&t.0, "x.mz", "bad")).unwrap();
            assert_eq!(o.iter, i);
            // The last iteration's diagnostics are not fed back (Mode 1 parity).
            assert_eq!(o.feedback.is_some(), i < 3);
        }
        let out = submit_output(
            &IterationOutcome {
                iter: 3,
                max_iters: 3,
                clean: false,
                feedback: None,
            },
            &dir,
        );
        assert!(out.contains("ITERATION 3/3: ERRORS") && out.contains("finish"));
        assert!(
            submit(&dir, &cand(&t.0, "y.mz", "OK"))
                .unwrap_err()
                .contains("max")
        );
        let fin = finish(&dir, &FakeTok).unwrap();
        assert_eq!(fin["clean"], false);
        assert_eq!(fin["iterations_to_clean"], Value::Null);
        assert_eq!(fin["defects"], Value::Null);
        assert_eq!(fin["scored"], false);
        assert!(!dir.join("score.json").exists());
    }

    #[test]
    fn tokenizer_down_records_null_with_reason() {
        let t = TempDir::new("tok");
        let dir = start(&t.0, Mode::Agent, fake_score(SCORE_OK));
        submit(&dir, &cand(&t.0, "a.mz", "OK")).unwrap();
        let fin = finish(&dir, &DownTok).unwrap();
        assert_eq!(fin["transcript_tokens"], Value::Null);
        assert!(
            fin["transcript_tokens_null_reason"]
                .as_str()
                .unwrap()
                .contains("connection refused")
        );
    }

    #[test]
    fn failing_scorer_leaves_clean_episode_unscored_not_defect_free() {
        let t = TempDir::new("sc");
        let mut bad = fake_score(SCORE_OK);
        bad.argv[2] = "exit 3".into();
        let dir = start(&t.0, Mode::Agent, bad);
        submit(&dir, &cand(&t.0, "a.mz", "OK")).unwrap();
        let fin = finish(&dir, &FakeTok).unwrap();
        assert_eq!(fin["clean"], true);
        assert_eq!(fin["scored"], false);
        assert_eq!(fin["defects"], Value::Null);
        assert!(fin["score_error"].as_str().unwrap().contains("exited"));
    }

    #[test]
    fn setup_error_is_not_an_iteration() {
        let t = TempDir::new("setup");
        let m = meta(&t.0, Mode::Agent, fake_score(SCORE_OK));
        let mut m2 = m.clone();
        m2.check.argv = vec!["sh".into(), "-c".into(), "exit 2".into()];
        let dir = create_episode(
            &t.0.join("ep"),
            &m2,
            &build_prompt(Arm::Mzizi, "g", "s", &[]),
        )
        .unwrap();
        assert!(submit(&dir, &cand(&t.0, "a.mz", "OK")).is_err());
        assert!(jsonl(&dir).is_empty());
    }

    #[test]
    fn existing_episode_dir_is_never_overwritten() {
        let t = TempDir::new("exists");
        start(&t.0, Mode::Agent, fake_score(SCORE_OK));
        let m = meta(&t.0, Mode::Agent, fake_score(SCORE_OK));
        let dir = episode_dir(&t.0.join("out"), &m.model, m.arm, &m.task, m.seed).unwrap();
        assert!(create_episode(&dir, &m, &build_prompt(Arm::Mzizi, "g", "s", &[])).is_err());
    }

    struct ScriptedModel {
        replies: RefCell<Vec<&'static str>>,
        seen: RefCell<Vec<Vec<Message>>>,
    }
    impl ChatModel for ScriptedModel {
        fn chat(&self, messages: &[Message], _: &GenParams) -> Result<ChatReply, String> {
            self.seen.borrow_mut().push(messages.to_vec());
            let r = self.replies.borrow_mut().remove(0);
            Ok(ChatReply {
                content: r.into(),
                prompt_tokens: Some(100 * messages.len() as u64),
                completion_tokens: Some(10),
            })
        }
    }

    /// Replies from a script, then the error llama.cpp's server returns once the history
    /// no longer fits its context.
    struct OverflowingModel {
        replies: RefCell<Vec<&'static str>>,
        error: &'static str,
    }
    impl ChatModel for OverflowingModel {
        fn chat(&self, messages: &[Message], _: &GenParams) -> Result<ChatReply, String> {
            match self.replies.borrow_mut().pop() {
                Some(r) => Ok(ChatReply {
                    content: r.into(),
                    prompt_tokens: Some(100 * messages.len() as u64),
                    completion_tokens: Some(10),
                }),
                None => Err(self.error.to_string()),
            }
        }
    }

    const LLAMA_OVERFLOW: &str = r#"POST http://127.0.0.1:8080/v1/chat/completions: HTTP 400: {"error":{"code":400,"message":"request (16568 tokens) exceeds the available context size (16384 tokens), try increasing it","type":"exceed_context_size_error","n_prompt_tokens":16568,"n_ctx":16384}}"#;

    fn params() -> GenParams {
        GenParams {
            model: "test-model".into(),
            temperature: 0.0,
            seed: 1,
            max_tokens: 64,
        }
    }

    #[test]
    fn a_context_overflow_after_the_first_reply_finishes_the_episode_not_clean() {
        let t = TempDir::new("overflow");
        let dir = start(&t.0, Mode::Model, fake_score(SCORE_OK));
        let model = OverflowingModel {
            replies: RefCell::new(vec!["```mz\nbroken\n```"]),
            error: LLAMA_OVERFLOW,
        };
        let p = build_prompt(Arm::Mzizi, "g", "s", &[]);
        let fin = run_model_episode(&dir, &p, &model, &FakeTok, &params()).unwrap();
        assert_eq!(fin["clean"], false);
        assert_eq!(fin["iterations"], 1);
        assert_eq!(fin["ended_by"], "context_exceeded");
        assert!(dir.join("error.txt").is_file());
        // A finished episode, so `summarize` counts it in the clean-compile denominator.
        assert_eq!(jsonl(&dir).last().unwrap()["kind"], "final");
    }

    #[test]
    fn a_first_request_that_does_not_fit_is_a_setup_error() {
        let t = TempDir::new("overflow1");
        let dir = start(&t.0, Mode::Model, fake_score(SCORE_OK));
        let model = OverflowingModel {
            replies: RefCell::new(vec![]),
            error: LLAMA_OVERFLOW,
        };
        let p = build_prompt(Arm::Mzizi, "g", "s", &[]);
        let e = run_model_episode(&dir, &p, &model, &FakeTok, &params()).unwrap_err();
        assert!(e.contains("iteration 1"), "{e}");
        assert!(jsonl(&dir).is_empty(), "no final line: the episode aborted");
        // Any other endpoint error, at any iteration, still aborts.
        assert!(!is_context_overflow("POST …: connection refused"));
        assert!(is_context_overflow(r#"{"code":"context_length_exceeded"}"#));
    }

    #[test]
    fn model_loop_shape_and_jsonl() {
        let t = TempDir::new("m1");
        let dir = start(&t.0, Mode::Model, fake_score(SCORE_OK));
        let model = ScriptedModel {
            replies: RefCell::new(vec![
                "Sure! No code here.",
                "```mz\nbroken\n```",
                "```mz\nnow OK\n```",
            ]),
            seen: RefCell::new(vec![]),
        };
        let p = build_prompt(Arm::Mzizi, "the guide", "spec body", &[]);
        let params = GenParams {
            model: "test-model".into(),
            temperature: 0.0,
            seed: 1,
            max_tokens: 64,
        };
        let fin = run_model_episode(&dir, &p, &model, &FakeTok, &params).unwrap();

        // The third call saw: system, user, a1, fb1, a2, fb2 — diagnostics verbatim.
        let seen = model.seen.borrow();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0][0].content, "the guide");
        assert_eq!(seen[2].len(), 6);
        assert!(
            seen[2][3]
                .content
                .starts_with("Your reply contained no fenced code block")
        );
        assert!(
            seen[2][5]
                .content
                .ends_with("{\"severity\":\"error\",\"message\":\"no OK\"}\n")
        );

        let lines = jsonl(&dir);
        assert_eq!(lines.len(), 4);
        for (i, l) in lines[..3].iter().enumerate() {
            for k in [
                "iter",
                "prompt_tokens",
                "completion_tokens",
                "compile_ok",
                "diagnostics_chars",
                "check_ms",
                "gen_ms",
            ] {
                assert!(l.get(k).is_some(), "iteration line missing {k}: {l}");
            }
            assert_eq!(l["kind"], "iteration");
            assert_eq!(l["iter"], i as u64 + 1);
        }
        assert_eq!(lines[0]["check_ms"], Value::Null, "no candidate, no check");
        assert_eq!(lines[0]["compile_ok"], false);
        assert_eq!(lines[2]["compile_ok"], true);
        for k in [
            "task",
            "arm",
            "model",
            "seed",
            "temperature",
            "iterations_to_clean",
            "total_prompt_tokens",
            "total_completion_tokens",
            "clean",
            "defects",
            "facts_checked",
            "class_token_jaccard",
            "token_source",
            "transcript_tokens",
        ] {
            assert!(fin.get(k).is_some(), "final line missing {k}");
        }
        assert_eq!(lines[3], fin);
        assert_eq!(fin["kind"], "final");
        assert_eq!(fin["token_source"], "endpoint_usage");
        assert_eq!(fin["iterations_to_clean"], 3);
        assert_eq!(fin["total_prompt_tokens"], 200 + 400 + 600);
        assert_eq!(fin["total_completion_tokens"], 30);
        assert!(dir.join("iter-01/reply.md").is_file());
        assert!(!dir.join("iter-01/candidate.mz").exists());
        assert!(dir.join("iter-03/candidate.mz").is_file());
        assert!(!dir.join("iter-03/feedback.txt").exists());
        // Mode 2 guard: a model-mode episode refuses agent submissions.
        assert!(submit(&dir, &cand(&t.0, "z.mz", "OK")).is_err());
    }

    #[test]
    fn fnv_known_vector() {
        assert_eq!(fnv1a64(b""), "cbf29ce484222325");
        assert_eq!(fnv1a64(b"a"), "af63dc4c8601ec8c");
    }
}
