//! Registration and publication (RFC-0009 §7): the raw-bundle hash a run publishes on the
//! day, and the machine-written half of the `PLAN.md` it commits before its first episode.
//!
//! **The bundle hash** is the SHA-256 of the text `sha256sum` prints for every file under
//! `<out>/episodes`, one `<hex>  episodes/<path>` line each, sorted by path in byte order.
//! It is exactly what `kill-criterion/run.sh` computed with
//! `find episodes -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum`, so
//! anyone can recompute it from a published bundle with coreutils alone.
//!
//! **`PLAN.md`** is drafted by `mzbench plan`: the code commit, each arm's guide and pins
//! with their hashes, each task's content hash, the models, seeds and budget, and the
//! scorer's version, all read off disk. The parts only a person can write (§6.2's n and why,
//! and anything the run departs from) are left as marked blanks. The draft is written once
//! and never overwritten, because a plan that is edited after the run starts is BM-5.

use std::path::{Path, PathBuf};

use crate::arm::{ArmConfig, TaskFamily};
use crate::sha256;
use crate::task::load_task;

/// Every file under `dir`, recursively, as paths relative to `base`, sorted by their bytes.
fn files_under(dir: &Path, base: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))? {
            let e = e.map_err(|e| e.to_string())?;
            let ty = e.file_type().map_err(|e| e.to_string())?;
            let p = e.path();
            if ty.is_dir() {
                stack.push(p);
            } else if ty.is_file() {
                out.push(
                    p.strip_prefix(base)
                        .map_err(|e| e.to_string())?
                        .to_path_buf(),
                );
            }
        }
    }
    out.sort_by(|a, b| {
        a.as_os_str()
            .as_encoded_bytes()
            .cmp(b.as_os_str().as_encoded_bytes())
    });
    Ok(out)
}

/// The `sha256sum`-format listing of every file under `<out>/episodes`.
pub fn bundle_listing(out: &Path) -> Result<String, String> {
    let episodes = out.join("episodes");
    if !episodes.is_dir() {
        return Err(format!("no episodes directory in {}", out.display()));
    }
    let mut listing = String::new();
    for rel in files_under(&episodes, out)? {
        let name = rel.to_string_lossy();
        if name.contains('\n') || name.contains('\\') {
            // sha256sum escapes such names, and this listing would not match it.
            return Err(format!("unsupported file name in bundle: {name:?}"));
        }
        listing.push_str(&format!("{}  {name}\n", sha256::file_hex(&out.join(&rel))?));
    }
    Ok(listing)
}

/// The raw-bundle hash of a results directory (see the module docs).
pub fn bundle_hash(out: &Path) -> Result<String, String> {
    Ok(sha256::hex(bundle_listing(out)?.as_bytes()))
}

/// A task's identity for a plan: its name, the SHA-256 of the `sha256sum` listing of its
/// files (so a held-out task can be named without its text), and each file's own hash.
pub struct TaskIdentity {
    pub name: String,
    pub hash: String,
    /// `(path relative to the task directory, SHA-256)`, sorted by path.
    pub files: Vec<(String, String)>,
}

pub fn task_identity(dir: &Path) -> Result<TaskIdentity, String> {
    let t = load_task(dir)?;
    let mut files = Vec::new();
    let mut listing = String::new();
    for rel in files_under(&t.dir, &t.dir)? {
        let h = sha256::file_hex(&t.dir.join(&rel))?;
        let name = rel.to_string_lossy().into_owned();
        listing.push_str(&format!("{h}  {name}\n"));
        files.push((name, h));
    }
    Ok(TaskIdentity {
        name: t.name,
        hash: sha256::hex(listing.as_bytes()),
        files,
    })
}

/// The scorer's version: the SHA-256 of the listing of `benchmarks/harness/src`, the code
/// that turns a candidate into facts.
pub fn scorer_version(repo: &Path) -> Result<String, String> {
    let src = repo.join("benchmarks/harness/src");
    let mut listing = String::new();
    for rel in files_under(&src, &src)? {
        listing.push_str(&format!(
            "{}  {}\n",
            sha256::file_hex(&src.join(&rel))?,
            rel.to_string_lossy()
        ));
    }
    Ok(sha256::hex(listing.as_bytes()))
}

/// What `mzbench plan` is told about the run.
pub struct PlanInput {
    pub title: String,
    pub date: String,
    pub commit: String,
    pub dirty: bool,
    pub family: TaskFamily,
    pub arms: Vec<ArmConfig>,
    pub tasks: Vec<PathBuf>,
    /// Name tasks by hash only (a held-out set, RFC-0004 §4.2).
    pub held_out: bool,
    pub models: Vec<String>,
    pub seeds: Vec<u64>,
    pub temperature: String,
    pub max_iters: u32,
    pub max_tokens: u64,
    pub n_ctx: Option<u64>,
}

/// Render the draft `PLAN.md`.
pub fn render_plan(repo: &Path, p: &PlanInput) -> Result<String, String> {
    let mut s = String::new();
    s.push_str(&format!("# Plan: {}\n\n", p.title));
    s.push_str(&format!(
        "Registered {} before the first episode (RFC-0009 §7.2). The tables are written by \
         `mzbench plan` from the files on disk. The sections marked **To fill** are \
         written by hand before this file is committed, and nothing in it changes after \
         the first episode. A departure from it is stated in `RUN.md`.\n\n",
        p.date
    ));
    s.push_str("## Code\n\n");
    s.push_str(&format!(
        "- Commit: `{}`{}\n- Scorer version (SHA-256 of `benchmarks/harness/src`): `{}`\n- Task family: `{}`\n\n",
        p.commit,
        if p.dirty {
            " (**dirty**: uncommitted changes; commit them before registering)"
        } else {
            ""
        },
        scorer_version(repo)?,
        p.family.as_str()
    ));

    s.push_str("## Arms\n\n");
    s.push_str("| Arm | Check | Guide | Guide bytes | Guide SHA-256 | Pins |\n");
    s.push_str("| --- | ----- | ----- | ----------- | ------------- | ---- |\n");
    for a in &p.arms {
        let guide = repo.join(&a.guide);
        let bytes = std::fs::metadata(&guide)
            .map_err(|e| format!("{}: {e}", guide.display()))?
            .len();
        let mut pins = Vec::new();
        for pin in &a.pins {
            let h = sha256::file_hex(&ArmConfig::dir(repo, &a.id).join(pin))?;
            pins.push(format!("`{pin}` `{h}`"));
        }
        let repo_s = repo.to_string_lossy();
        let check: Vec<String> = a
            .check
            .iter()
            .map(|x| x.replace(repo_s.as_ref(), "{repo}"))
            .collect();
        s.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | `{}` | {} |\n",
            a.id,
            check.join(" "),
            a.guide,
            bytes,
            sha256::file_hex(&guide)?,
            if pins.is_empty() {
                "none (the repo commit)".to_string()
            } else {
                pins.join("<br>")
            }
        ));
    }
    s.push_str(
        "\nGuide token counts, with the headline model's tokenizer (RFC-0009 §4.2): each \
         episode's final line records `guide_tokens`. **To fill:** the counts, and that \
         they are within ±5% of each other.\n\n",
    );

    s.push_str("## Tasks\n\n");
    s.push_str(&format!(
        "{} task(s). A task's hash is the SHA-256 of the `sha256sum` listing of its files.\n\n",
        p.tasks.len()
    ));
    s.push_str("| Task | Content SHA-256 |\n| ---- | --------------- |\n");
    for (i, d) in p.tasks.iter().enumerate() {
        let id = task_identity(d)?;
        let label = if p.held_out {
            format!("held-out #{}", i + 1)
        } else {
            format!("`{}`", id.name)
        };
        s.push_str(&format!("| {label} | `{}` |\n", id.hash));
    }

    let seeds: Vec<String> = p.seeds.iter().map(u64::to_string).collect();
    s.push_str("\n## Models, seeds and budget\n\n");
    s.push_str(&format!(
        "- Models: {}\n- Seeds: {}\n- Temperature: {}\n- `max_iters`: {}\n- `max_tokens` per reply: {}\n- Context: {}\n- Episodes: {} (tasks × arms × seeds × models)\n\n",
        p.models
            .iter()
            .map(|m| format!("`{m}`"))
            .collect::<Vec<_>>()
            .join(", "),
        seeds.join(", "),
        p.temperature,
        p.max_iters,
        p.max_tokens,
        p.n_ctx
            .map(|n| format!("{n} tokens"))
            .unwrap_or_else(|| "**To fill:** the server's `n_ctx`".into()),
        p.tasks.len() * p.arms.len() * p.seeds.len() * p.models.len()
    ));

    s.push_str("## Decision rule\n\n");
    match p.family {
        TaskFamily::UiSpec | TaskFamily::Backend => s.push_str(&format!(
            "This is a gating family. The rule is RFC-0009 §6.2, fixed here before any score \
             is seen. For each metric (tokens, iterations to a clean check, defect rate), the \
             bar is the best value any incumbent arm reached. The family passes when `mzizi` \
             beats the bar on at least two of the three, on the headline model, and a win \
             counts only if a paired bootstrap over (task, seed) pairs gives a 95% interval \
             for the difference that excludes zero in Mzizi's favour.\n\n\
             - n: {} tasks × {} seeds = {} (task, seed) pairs per arm and model.\n\
             - **To fill:** why this n, the bootstrap's resample count and seed, and which \
             model is the headline.\n\n",
            p.tasks.len(),
            p.seeds.len(),
            p.tasks.len() * p.seeds.len()
        )),
        TaskFamily::UiPort => s.push_str(
            "`ui-port` is a development family (RFC-0009 §2). It gates nothing, and no pass \
             or fail is read off it.\n\n",
        ),
    }
    s.push_str("## Departures and notes\n\n**To fill**, or `None.`\n");
    Ok(s)
}

/// Today's date, UTC, as `YYYY-MM-DD`, from the system clock (no date library).
pub fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_date((secs / 86_400) as i64)
}

/// Days since 1970-01-01 → `YYYY-MM-DD` (Howard Hinnant's `civil_from_days`).
pub fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tmp(label: &str) -> Tmp {
        let p = std::env::temp_dir().join(format!("mzbench-plan-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn the_bundle_hash_is_the_coreutils_pipeline() {
        let t = tmp("bundle");
        let ep = t.0.join("episodes/m/mzizi/button/seed-1");
        std::fs::create_dir_all(ep.join("iter-01")).unwrap();
        std::fs::write(ep.join("meta.json"), "{}\n").unwrap();
        std::fs::write(ep.join("iter-01/candidate.mz"), "component x\n").unwrap();
        std::fs::write(t.0.join("episodes/B"), "upper case sorts first\n").unwrap();
        std::fs::write(t.0.join("summary.md"), "not in the bundle\n").unwrap();
        let listing = bundle_listing(&t.0).unwrap();
        let names: Vec<&str> = listing.lines().map(|l| &l[66..]).collect();
        assert_eq!(
            names,
            [
                "episodes/B",
                "episodes/m/mzizi/button/seed-1/iter-01/candidate.mz",
                "episodes/m/mzizi/button/seed-1/meta.json",
            ]
        );
        assert!(listing.starts_with(&format!(
            "{}  episodes/B\n",
            sha256::hex(b"upper case sorts first\n")
        )));
        // When coreutils is present (it is on CI's runner, and it is what a reader will use
        // to check a published bundle), the two must agree byte for byte.
        let sh = std::process::Command::new("sh")
            .arg("-c")
            .arg(
                "find episodes -type f -print0 | LC_ALL=C sort -z | xargs -0 -r sha256sum | \
                 sha256sum | cut -d' ' -f1",
            )
            .current_dir(&t.0)
            .output();
        if let Ok(o) = sh
            && o.status.success()
            && !o.stdout.is_empty()
        {
            assert_eq!(
                String::from_utf8_lossy(&o.stdout).trim(),
                bundle_hash(&t.0).unwrap()
            );
        }
        assert!(bundle_hash(&t.0.join("episodes")).is_err());
    }

    #[test]
    fn a_task_is_identified_by_its_files_hash() {
        let id = task_identity(&repo().join("benchmarks/tasks/button")).unwrap();
        assert_eq!(id.name, "button");
        assert_eq!(id.hash.len(), 64);
        let names: Vec<&str> = id.files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["reference.rs", "spec.md", "spec.tsx", "task.toml"]);
    }

    #[test]
    fn the_plan_names_every_arm_pin_and_task_and_leaves_the_human_parts_blank() {
        let r = repo();
        let arms: Vec<ArmConfig> = ["mzizi", "dioxus"]
            .iter()
            .map(|a| ArmConfig::load(&r, a).unwrap())
            .collect();
        let input = PlanInput {
            title: "test".into(),
            date: "2026-09-29".into(),
            commit: "abc".into(),
            dirty: false,
            family: TaskFamily::UiSpec,
            arms,
            tasks: vec![
                r.join("benchmarks/tasks/button"),
                r.join("benchmarks/tasks/card"),
            ],
            held_out: true,
            models: vec!["qwen".into()],
            seeds: vec![1, 2, 3, 4, 5],
            temperature: "0.7".into(),
            max_iters: 5,
            max_tokens: 4096,
            n_ctx: Some(32768),
        };
        let p = render_plan(&r, &input).unwrap();
        assert!(p.contains("| `dioxus` | `{repo}/benchmarks/arms/dioxus/check.sh {file}` |"));
        assert!(p.contains("`sandbox/Cargo.lock` `"));
        assert!(p.contains("| held-out #2 |"));
        assert!(
            !p.contains("`button`"),
            "a held-out plan must not name its tasks"
        );
        assert!(p.contains("2 tasks × 5 seeds = 10 (task, seed) pairs"));
        assert!(p.contains("Episodes: 20"));
        assert!(p.contains("32768 tokens"));
        assert!(p.matches("**To fill:**").count() >= 2);
        assert!(
            !p.contains(&*r.to_string_lossy()),
            "no machine paths in a plan"
        );
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_725), "2026-09-29");
        assert_eq!(civil_date(11_016), "2000-02-29");
    }
}
