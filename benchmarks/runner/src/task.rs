//! A task directory: `task.toml`, `spec.tsx` (the only source the author sees) and
//! `reference.rs` (never shown; scoring only).
//!
//! `task.toml` is read with a deliberately tiny reader: top-level `key = "string"` pairs
//! before the first `[table]` header. The `[source]` table (provenance) is not needed to
//! run an episode, so it is not interpreted — only the top-level `name`, `spec`,
//! `reference`, and optional `enums`, `allow_variant_renames` and `rename_reason` keys are.
//! `spec` and `reference` are paths relative to the task directory, defaulting to
//! `spec.tsx` and `reference.rs`. `enums` is a single-line array of Rust PascalCase enum
//! names (see `prompt::enum_sentence`).
//!
//! `allow_variant_renames = true` opts the task in to the scorer pairing renamed variants
//! by class string (`mzizi-benchmark-harness score --allow-variant-renames`). It is off by
//! default — a candidate that renames against its spec must score the rename — and a task
//! that sets it must also give a non-empty `rename_reason` saying why its spec and
//! reference disagree on names, or the task does not load.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Task {
    pub name: String,
    pub dir: PathBuf,
    pub spec_path: PathBuf,
    pub reference_path: PathBuf,
    /// Optional `enums = [...]` (Rust PascalCase): the enum names the prompt asks for.
    pub enums: Vec<String>,
    /// `allow_variant_renames = true`: score with rename pairing. `false` when absent.
    pub allow_variant_renames: bool,
    /// Why the task allows renames. `Some` exactly when `allow_variant_renames` is set.
    pub rename_reason: Option<String>,
}

pub fn load_task(dir: &Path) -> Result<Task, String> {
    let dir = std::fs::canonicalize(dir).map_err(|e| format!("task dir {}: {e}", dir.display()))?;
    let toml_path = dir.join("task.toml");
    let toml = std::fs::read_to_string(&toml_path)
        .map_err(|e| format!("cannot read {}: {e}", toml_path.display()))?;

    let name = top_level_string(&toml, "name").unwrap_or_else(|| {
        dir.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let spec = top_level_string(&toml, "spec").unwrap_or_else(|| "spec.tsx".into());
    let reference = top_level_string(&toml, "reference").unwrap_or_else(|| "reference.rs".into());

    let spec_path = dir.join(spec);
    let reference_path = dir.join(reference);
    for p in [&spec_path, &reference_path] {
        if !p.is_file() {
            return Err(format!("task file missing: {}", p.display()));
        }
    }
    let enums = top_level_string_array(&toml, "enums")
        .map_err(|e| format!("{}: {e}", toml_path.display()))?
        .unwrap_or_default();
    let allow_variant_renames = top_level_bool(&toml, "allow_variant_renames")
        .map_err(|e| format!("{}: {e}", toml_path.display()))?
        .unwrap_or(false);
    let rename_reason = top_level_string(&toml, "rename_reason").filter(|r| !r.trim().is_empty());
    match (allow_variant_renames, &rename_reason) {
        (true, None) => {
            return Err(format!(
                "{}: `allow_variant_renames = true` needs a non-empty `rename_reason`",
                toml_path.display()
            ));
        }
        (false, Some(_)) => {
            return Err(format!(
                "{}: `rename_reason` without `allow_variant_renames = true`",
                toml_path.display()
            ));
        }
        _ => {}
    }
    Ok(Task {
        name: safe_component(&name)?,
        dir,
        spec_path,
        reference_path,
        enums,
        allow_variant_renames,
        rename_reason,
    })
}

/// A top-level `key = true|false`. `Ok(None)` if absent; any other value is an error.
pub fn top_level_bool(toml: &str, key: &str) -> Result<Option<bool>, String> {
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return Ok(None);
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        // A trailing `# comment` is allowed; nothing else is.
        let v = v.split('#').next().unwrap_or("").trim();
        return match v {
            "true" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            _ => Err(format!("`{key}` must be `true` or `false`, not `{v}`")),
        };
    }
    Ok(None)
}

/// A top-level single-line `key = ["A", "B"]` array of strings. `Ok(None)` if absent.
pub fn top_level_string_array(toml: &str, key: &str) -> Result<Option<Vec<String>>, String> {
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return Ok(None);
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let v = v.trim();
        let inner = v
            .strip_prefix('[')
            .and_then(|s| s.split_once(']'))
            .map(|(s, _)| s)
            .ok_or(format!("`{key}` must be a single-line array of strings"))?;
        let mut out = Vec::new();
        for item in inner.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let s = item
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| item.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .ok_or(format!("`{key}`: `{item}` is not a quoted string"))?;
            out.push(s.to_string());
        }
        return Ok(Some(out));
    }
    Ok(None)
}

/// A top-level `key = "value"` from a TOML document, stopping at the first table header.
pub fn top_level_string(toml: &str, key: &str) -> Option<String> {
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return None;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let v = v.trim();
        if let Some(inner) = v.strip_prefix('"').and_then(|s| s.split_once('"')) {
            return Some(inner.0.to_string());
        }
        if let Some(inner) = v.strip_prefix('\'').and_then(|s| s.split_once('\'')) {
            return Some(inner.0.to_string());
        }
    }
    None
}

/// Make a label safe to use as one path component. Anything outside `[A-Za-z0-9._-]`
/// becomes `_`; `.`/`..`/empty are rejected rather than rewritten.
pub fn safe_component(s: &str) -> Result<String, String> {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() || out == "." || out == ".." {
        return Err(format!("`{s}` is not usable as a path component"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_top_level_keys_and_ignores_tables() {
        let t =
            "name = \"button\"\nspec = 'spec.tsx'\n\n[source]\nname = \"other\"\nrepo = \"x\"\n";
        assert_eq!(top_level_string(t, "name").as_deref(), Some("button"));
        assert_eq!(top_level_string(t, "spec").as_deref(), Some("spec.tsx"));
        assert_eq!(top_level_string(t, "repo"), None);
    }

    #[test]
    fn reads_optional_enums_array() {
        let t = "name = \"b\"\nenums = [\"ButtonVariant\", 'ButtonSize']\n[source]\n";
        assert_eq!(
            top_level_string_array(t, "enums").unwrap(),
            Some(vec!["ButtonVariant".to_string(), "ButtonSize".to_string()])
        );
        assert_eq!(
            top_level_string_array("name = \"b\"\n", "enums").unwrap(),
            None
        );
        assert!(top_level_string_array("enums = [Bare]\n", "enums").is_err());
        assert_eq!(
            top_level_string_array("[source]\nenums = [\"X\"]\n", "enums").unwrap(),
            None
        );
    }

    #[test]
    fn reads_optional_bool() {
        assert_eq!(top_level_bool("a = true\n", "a").unwrap(), Some(true));
        assert_eq!(
            top_level_bool("a = false # no\n", "a").unwrap(),
            Some(false)
        );
        assert_eq!(top_level_bool("b = true\n", "a").unwrap(), None);
        assert_eq!(top_level_bool("[t]\na = true\n", "a").unwrap(), None);
        assert!(top_level_bool("a = \"true\"\n", "a").is_err());
    }

    fn task_dir(toml: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("mzbench-task-{}-{}", std::process::id(), fnv(toml)));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("task.toml"), toml).unwrap();
        std::fs::write(d.join("spec.tsx"), "").unwrap();
        std::fs::write(d.join("reference.rs"), "").unwrap();
        d
    }

    fn fnv(s: &str) -> u64 {
        s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    #[test]
    fn variant_renames_are_off_by_default_and_need_a_reason() {
        let t = load_task(&task_dir("name = \"a\"\n")).unwrap();
        assert!(!t.allow_variant_renames);
        assert_eq!(t.rename_reason, None);

        let t = load_task(&task_dir(
            "name = \"b\"\nallow_variant_renames = true\nrename_reason = \"spec and reference disagree\"\n",
        ))
        .unwrap();
        assert!(t.allow_variant_renames);
        assert_eq!(
            t.rename_reason.as_deref(),
            Some("spec and reference disagree")
        );

        let e = load_task(&task_dir("name = \"c\"\nallow_variant_renames = true\n")).unwrap_err();
        assert!(e.contains("needs a non-empty `rename_reason`"), "{e}");
        let e = load_task(&task_dir(
            "name = \"d\"\nallow_variant_renames = true\nrename_reason = \" \"\n",
        ))
        .unwrap_err();
        assert!(e.contains("needs a non-empty `rename_reason`"), "{e}");
        let e = load_task(&task_dir("name = \"e\"\nrename_reason = \"x\"\n")).unwrap_err();
        assert!(e.contains("without `allow_variant_renames = true`"), "{e}");
    }

    #[test]
    fn only_the_changelog_task_opts_in_to_renames() {
        let tasks = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tasks");
        for (name, allowed) in [
            ("button", false),
            ("badge", false),
            ("nyuchi-changelog-renderer", true),
        ] {
            let t = load_task(&tasks.join(name)).unwrap();
            assert_eq!(t.allow_variant_renames, allowed, "{name}");
        }
    }

    #[test]
    fn safe_component_rewrites_and_rejects() {
        assert_eq!(safe_component("qwen2.5/7b q4").unwrap(), "qwen2.5_7b_q4");
        assert!(safe_component("..").is_err());
        assert!(safe_component("").is_err());
    }
}
