//! An arm as data: `benchmarks/arms/<id>/arm.toml` (the format is specified in `README.md`,
//! "Arms: `arm.toml`", RFC-0009 §1). Nothing else in the runner is arm-specific.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::exec::Normaliser;
use crate::toml_lite;

/// A task family (RFC-0009 §2): what the agent is handed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskFamily {
    /// `spec.tsx`, a React component to port. Development and pilot comparison only.
    UiPort,
    /// `spec.md`, a language-neutral statement of the component. Gating.
    UiSpec,
    /// `spec.md` for an HTTP service, scored by probes. Gating; not runnable yet.
    Backend,
}

impl TaskFamily {
    pub fn parse(s: &str) -> Result<TaskFamily, String> {
        match s {
            "ui-port" => Ok(TaskFamily::UiPort),
            "ui-spec" => Ok(TaskFamily::UiSpec),
            "backend" => Ok(TaskFamily::Backend),
            o => Err(format!(
                "unknown task family `{o}` (expected ui-port, ui-spec or backend)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TaskFamily::UiPort => "ui-port",
            TaskFamily::UiSpec => "ui-spec",
            TaskFamily::Backend => "backend",
        }
    }
}

/// How a task's Rust PascalCase enum names are written for an arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnumCase {
    /// `ButtonSize`.
    Pascal,
    /// `button_size`.
    Snake,
    /// `buttonVariants.size`: the last word is the `cva` variant key, and the words before
    /// it, camel-cased, plus `Variants`, name the `cva` call. The harness's React extractor
    /// reads the same rule backwards (`buttonVariants` + `size` → `button_size`).
    Cva,
}

impl EnumCase {
    fn parse(s: &str) -> Result<EnumCase, String> {
        match s {
            "pascal" => Ok(EnumCase::Pascal),
            "snake" => Ok(EnumCase::Snake),
            "cva" => Ok(EnumCase::Cva),
            o => Err(format!(
                "unknown enum_case `{o}` (expected pascal, snake or cva)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EnumCase::Pascal => "pascal",
            EnumCase::Snake => "snake",
            EnumCase::Cva => "cva",
        }
    }

    pub fn render(self, pascal: &str) -> String {
        match self {
            EnumCase::Pascal => pascal.to_string(),
            EnumCase::Snake => snake_case(pascal),
            EnumCase::Cva => {
                let words = pascal_words(pascal);
                match words.split_last() {
                    Some((key, prefix)) if !prefix.is_empty() => {
                        let mut call = String::new();
                        for (i, w) in prefix.iter().enumerate() {
                            if i == 0 {
                                call.push_str(&w.to_lowercase());
                            } else {
                                call.push_str(w);
                            }
                        }
                        format!("{call}Variants.{}", key.to_lowercase())
                    }
                    // A one-word enum has no call name to split off; it keys a call of its own.
                    _ => format!("variants.{}", pascal.to_lowercase()),
                }
            }
        }
    }
}

/// `ButtonSize` → `["Button", "Size"]`; `HTTPStatus` → `["HTTP", "Status"]`.
fn pascal_words(pascal: &str) -> Vec<String> {
    snake_case(pascal)
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            // Each word capitalised: `button` → `Button`.
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// `ButtonSize` → `button_size`; `HTTPStatus` → `http_status`; `Size2X` → `size2_x`.
pub fn snake_case(pascal: &str) -> String {
    let chars: Vec<char> = pascal.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1).copied();
            let boundary = match prev {
                None => false,
                Some(p) if p.is_lowercase() || p.is_ascii_digit() => true,
                Some(p) if p.is_uppercase() => next.is_some_and(|n| n.is_lowercase()),
                _ => false,
            };
            if boundary && !out.ends_with('_') {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// One arm, as its `arm.toml` declares it, with `{repo}` resolved in `check`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmConfig {
    pub id: String,
    /// `ui` or `backend`.
    pub family: String,
    pub task_families: Vec<TaskFamily>,
    pub language_name: String,
    pub file_kind: String,
    pub extension: String,
    /// Relative to the repo root.
    pub guide: String,
    pub check: Vec<String>,
    pub normaliser: Normaliser,
    /// The scorer's `--arm` value, or `none`.
    pub extractor: String,
    pub enum_case: Option<EnumCase>,
    pub class_accessor: Option<String>,
    /// Relative to the arm directory.
    pub pins: Vec<String>,
}

const KEYS: &[&str] = &[
    "id",
    "family",
    "task_families",
    "language_name",
    "file_kind",
    "extension",
    "guide",
    "check",
    "normaliser",
    "extractor",
    "enum_case",
    "class_accessor",
    "pins",
];

fn is_id(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl ArmConfig {
    /// Parse an `arm.toml`, resolving `{repo}` in `check` to `repo`.
    pub fn parse(src: &str, repo: &Path) -> Result<ArmConfig, String> {
        use toml_lite::Value as V;
        let m = toml_lite::parse(src)?;
        if let Some(k) = m.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(format!("unknown key `{k}`"));
        }
        let string = |k: &str| -> Result<Option<String>, String> {
            match m.get(k) {
                None => Ok(None),
                Some(V::Str(s)) => Ok(Some(s.clone())),
                Some(_) => Err(format!("`{k}` must be a string")),
            }
        };
        let req = |k: &str| -> Result<String, String> {
            string(k)?
                .filter(|s| !s.trim().is_empty())
                .ok_or(format!("`{k}` is required"))
        };
        let array = |k: &str| -> Result<Option<Vec<String>>, String> {
            match m.get(k) {
                None => Ok(None),
                Some(V::Array(a)) => Ok(Some(a.clone())),
                Some(_) => Err(format!("`{k}` must be an array of strings")),
            }
        };
        let id = req("id")?;
        if !is_id(&id) {
            return Err(format!("`id` must match [a-z0-9-]+, not `{id}`"));
        }
        let family = req("family")?;
        if family != "ui" && family != "backend" {
            return Err(format!("`family` must be ui or backend, not `{family}`"));
        }
        let task_families = array("task_families")?
            .ok_or("`task_families` is required")?
            .iter()
            .map(|f| TaskFamily::parse(f))
            .collect::<Result<Vec<_>, _>>()?;
        if task_families.is_empty() {
            return Err("`task_families` must name at least one family".into());
        }
        let extension = req("extension")?;
        if extension.starts_with('.') || extension.contains('/') {
            return Err(format!(
                "`extension` is a bare extension (`rs`), not `{extension}`"
            ));
        }
        let check = array("check")?.ok_or("`check` is required")?;
        if check.is_empty() {
            return Err("`check` must not be empty".into());
        }
        let repo_s = repo.to_string_lossy();
        let check = check.iter().map(|a| a.replace("{repo}", &repo_s)).collect();
        let normaliser = match string("normaliser")? {
            Some(n) => Normaliser::parse(&n)?,
            None => Normaliser::FileName,
        };
        let enum_case = string("enum_case")?
            .map(|s| EnumCase::parse(&s))
            .transpose()?;
        let class_accessor = string("class_accessor")?;
        if family == "ui" && (enum_case.is_none() || class_accessor.is_none()) {
            return Err("a ui arm needs `enum_case` and `class_accessor`".into());
        }
        Ok(ArmConfig {
            id,
            family,
            task_families,
            language_name: req("language_name")?,
            file_kind: req("file_kind")?,
            extension,
            guide: req("guide")?,
            check,
            normaliser,
            extractor: req("extractor")?,
            enum_case,
            class_accessor,
            pins: array("pins")?.unwrap_or_default(),
        })
    }

    /// The directory an arm's files live in.
    pub fn dir(repo: &Path, id: &str) -> PathBuf {
        repo.join("benchmarks/arms").join(id)
    }

    /// Load `benchmarks/arms/<id>/arm.toml` from `repo`.
    pub fn load(repo: &Path, id: &str) -> Result<ArmConfig, String> {
        if !is_id(id) {
            return Err(format!("`{id}` is not an arm id"));
        }
        let path = ArmConfig::dir(repo, id).join("arm.toml");
        let src = std::fs::read_to_string(&path).map_err(|e| {
            format!(
                "unknown arm `{id}`: cannot read {} ({e}); the arms are {}",
                path.display(),
                list_arms(repo).unwrap_or_default().join(", ")
            )
        })?;
        let arm = ArmConfig::parse(&src, repo).map_err(|e| format!("{}: {e}", path.display()))?;
        if arm.id != id {
            return Err(format!(
                "{}: `id = \"{}\"` does not match its directory `{id}`",
                path.display(),
                arm.id
            ));
        }
        Ok(arm)
    }

    pub fn runs(&self, family: TaskFamily) -> bool {
        self.task_families.contains(&family)
    }

    /// An enum name from `task.toml` (Rust PascalCase) in this arm's convention.
    pub fn enum_name(&self, pascal: &str) -> String {
        self.enum_case.unwrap_or(EnumCase::Pascal).render(pascal)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "family": self.family,
            "task_families": self.task_families.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
            "language_name": self.language_name,
            "file_kind": self.file_kind,
            "extension": self.extension,
            "guide": self.guide,
            "check": self.check,
            "normaliser": self.normaliser.as_str(),
            "extractor": self.extractor,
            "enum_case": self.enum_case.map(EnumCase::as_str),
            "class_accessor": self.class_accessor,
            "pins": self.pins,
        })
    }

    pub fn from_json(v: &Value) -> Result<ArmConfig, String> {
        let st = |k: &str| -> Result<String, String> {
            v.get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or(format!("arm_config: missing `{k}`"))
        };
        let arr = |k: &str| -> Vec<String> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        Ok(ArmConfig {
            id: st("id")?,
            family: st("family")?,
            task_families: arr("task_families")
                .iter()
                .map(|f| TaskFamily::parse(f))
                .collect::<Result<_, _>>()?,
            language_name: st("language_name")?,
            file_kind: st("file_kind")?,
            extension: st("extension")?,
            guide: st("guide")?,
            check: arr("check"),
            normaliser: Normaliser::parse(&st("normaliser")?)?,
            extractor: st("extractor")?,
            enum_case: v
                .get("enum_case")
                .and_then(Value::as_str)
                .map(EnumCase::parse)
                .transpose()?,
            class_accessor: v
                .get("class_accessor")
                .and_then(Value::as_str)
                .map(str::to_string),
            pins: arr("pins"),
        })
    }

    /// The arm an episode written before `arm.toml` existed ran with: its `meta.json` names
    /// the arm and has no `arm_config`. The values are the ones the hard-coded `Arm` enum
    /// had until 2026-09-29. The check argv is not reconstructed here, because that
    /// `meta.json` records the argv that ran.
    pub fn legacy(id: &str) -> Result<ArmConfig, String> {
        let (lang, kind, ext, case, accessor, extractor) = match id {
            "mzizi" => (
                "Mzizi",
                "a single Mzizi source file (.mz)",
                "mz",
                EnumCase::Snake,
                "each with a `class` column",
                "mzizi",
            ),
            "dioxus" | "leptos" => (
                if id == "dioxus" { "Dioxus" } else { "Leptos" },
                if id == "dioxus" {
                    "a single Rust source file (.rs) using Dioxus"
                } else {
                    "a single Rust source file (.rs) using Leptos"
                },
                "rs",
                EnumCase::Pascal,
                "each with a `classes()` method returning its Tailwind classes",
                "dioxus",
            ),
            o => {
                return Err(format!(
                    "meta.json names arm `{o}` and has no `arm_config`; only mzizi, dioxus \
                     and leptos episodes predate arm.toml"
                ));
            }
        };
        Ok(ArmConfig {
            id: id.to_string(),
            family: "ui".into(),
            task_families: vec![TaskFamily::UiPort],
            language_name: lang.into(),
            file_kind: kind.into(),
            extension: ext.into(),
            guide: format!("benchmarks/prompts/{id}-guide.md"),
            check: vec![],
            normaliser: Normaliser::None,
            extractor: extractor.into(),
            enum_case: Some(case),
            class_accessor: Some(accessor.into()),
            pins: vec![],
        })
    }
}

/// Every arm id under `repo/benchmarks/arms` that has an `arm.toml`, sorted.
pub fn list_arms(repo: &Path) -> Result<Vec<String>, String> {
    let dir = repo.join("benchmarks/arms");
    let mut ids = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        if e.path().join("arm.toml").is_file() {
            ids.push(e.file_name().to_string_lossy().into_owned());
        }
    }
    ids.sort();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn enum_cases_render() {
        assert_eq!(EnumCase::Snake.render("ButtonSize"), "button_size");
        assert_eq!(EnumCase::Pascal.render("ButtonSize"), "ButtonSize");
        assert_eq!(EnumCase::Cva.render("ButtonSize"), "buttonVariants.size");
        assert_eq!(
            EnumCase::Cva.render("BadgeVariant"),
            "badgeVariants.variant"
        );
        assert_eq!(EnumCase::Cva.render("NodeAccent"), "nodeVariants.accent");
        assert_eq!(
            EnumCase::Cva.render("ChangelogNodeAccent"),
            "changelogNodeVariants.accent"
        );
        assert_eq!(EnumCase::Cva.render("Size"), "variants.size");
        assert_eq!(snake_case("HTTPStatus"), "http_status");
        assert_eq!(snake_case("IconSm"), "icon_sm");
    }

    #[test]
    fn the_three_original_arms_load_with_the_values_the_enum_had() {
        // "No behaviour change" (RFC-0009 §9 step 1): each migrated arm.toml gives exactly
        // what the hard-coded `Arm` enum gave, for every value the prompt and the scorer use.
        let r = repo();
        for id in ["mzizi", "dioxus", "leptos"] {
            let a = ArmConfig::load(&r, id).unwrap();
            let l = ArmConfig::legacy(id).unwrap();
            assert_eq!(a.language_name, l.language_name, "{id}");
            assert_eq!(a.file_kind, l.file_kind, "{id}");
            assert_eq!(a.extension, l.extension, "{id}");
            assert_eq!(a.guide, l.guide, "{id}");
            assert_eq!(a.extractor, l.extractor, "{id}");
            assert_eq!(a.enum_case, l.enum_case, "{id}");
            assert_eq!(a.class_accessor, l.class_accessor, "{id}");
            assert_eq!(a.normaliser, Normaliser::FileName, "{id}");
            assert!(a.runs(TaskFamily::UiPort), "{id}");
            assert!(r.join(&a.guide).is_file(), "{id}");
        }
        let rs = r.to_string_lossy();
        assert_eq!(
            ArmConfig::load(&r, "mzizi").unwrap().check,
            [
                "cargo",
                "run",
                "-q",
                "--manifest-path",
                &format!("{rs}/compiler/Cargo.toml"),
                "--bin",
                "mz",
                "--",
                "check",
                "--agent",
                "{file}",
            ]
        );
        for id in ["dioxus", "leptos"] {
            assert_eq!(
                ArmConfig::load(&r, id).unwrap().check,
                [
                    format!("{rs}/benchmarks/arms/{id}/check.sh"),
                    "{file}".into()
                ]
            );
        }
    }

    #[test]
    fn every_arm_file_loads_and_its_pins_exist() {
        let r = repo();
        let ids = list_arms(&r).unwrap();
        assert!(ids.len() >= 3, "{ids:?}");
        for id in ids {
            let a = ArmConfig::load(&r, &id).unwrap();
            for p in &a.pins {
                assert!(ArmConfig::dir(&r, &id).join(p).is_file(), "{id}: pin {p}");
            }
            assert_eq!(ArmConfig::from_json(&a.to_json()).unwrap(), a);
        }
    }

    #[test]
    fn rejects_bad_arm_files() {
        let good = r#"id = "x"
family = "ui"
task_families = ["ui-port"]
language_name = "X"
file_kind = "a single X file (.x)"
extension = "x"
guide = "g.md"
check = ["{repo}/c.sh", "{file}"]
extractor = "none"
enum_case = "pascal"
class_accessor = "each with classes"
"#;
        let a = ArmConfig::parse(good, Path::new("/r")).unwrap();
        assert_eq!(a.check, ["/r/c.sh", "{file}"]);
        assert_eq!(a.normaliser, Normaliser::FileName);
        for (bad, why) in [
            (format!("{good}typo = \"x\"\n"), "unknown key"),
            (good.replace("id = \"x\"", "id = \"X Y\""), "`id`"),
            (good.replace("\"ui-port\"", "\"ui-spex\""), "task family"),
            (
                good.replace("family = \"ui\"", "family = \"web\""),
                "`family`",
            ),
            (
                good.replace("extension = \"x\"", "extension = \".x\""),
                "extension",
            ),
            (good.replace("enum_case = \"pascal\"\n", ""), "enum_case"),
            (
                good.replace("check = [\"{repo}/c.sh\", \"{file}\"]", "check = []"),
                "check",
            ),
            (
                good.replace("guide = \"g.md\"\n", ""),
                "`guide` is required",
            ),
        ] {
            let e = ArmConfig::parse(&bad, Path::new("/r")).unwrap_err();
            assert!(e.contains(why), "{why}: {e}");
        }
        // A backend arm does not need the UI naming keys.
        let be = good
            .replace("family = \"ui\"", "family = \"backend\"")
            .replace("[\"ui-port\"]", "[\"backend\"]")
            .replace("enum_case = \"pascal\"\n", "")
            .replace("class_accessor = \"each with classes\"\n", "");
        assert!(ArmConfig::parse(&be, Path::new("/r")).is_ok());
    }

    #[test]
    fn an_unknown_arm_names_the_ones_there_are() {
        let e = ArmConfig::load(&repo(), "cobol").unwrap_err();
        assert!(
            e.contains("unknown arm `cobol`") && e.contains("dioxus"),
            "{e}"
        );
        assert!(ArmConfig::load(&repo(), "../x").is_err());
    }
}
