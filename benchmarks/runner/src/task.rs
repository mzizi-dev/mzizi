//! A task directory: `task.toml`, `spec.tsx` (the only source the author sees) and
//! `reference.rs` (never shown; scoring only).
//!
//! `task.toml` is read with a deliberately tiny reader: top-level `key = "string"` pairs
//! before the first `[table]` header. The `[source]` table (provenance) is not needed to
//! run an episode, so it is not interpreted — only the top-level `name`, `spec`,
//! `reference` and optional `enums` keys are. `spec` and `reference` are paths relative to
//! the task directory, defaulting to `spec.tsx` and `reference.rs`. `enums` is a
//! single-line array of Rust PascalCase enum names (see `prompt::enum_sentence`).

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Task {
    pub name: String,
    pub dir: PathBuf,
    pub spec_path: PathBuf,
    pub reference_path: PathBuf,
    /// Optional `enums = [...]` (Rust PascalCase): the enum names the prompt asks for.
    pub enums: Vec<String>,
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
    Ok(Task {
        name: safe_component(&name)?,
        dir,
        spec_path,
        reference_path,
        enums,
    })
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
    fn safe_component_rewrites_and_rejects() {
        assert_eq!(safe_component("qwen2.5/7b q4").unwrap(), "qwen2.5_7b_q4");
        assert!(safe_component("..").is_err());
        assert!(safe_component("").is_err());
    }
}
