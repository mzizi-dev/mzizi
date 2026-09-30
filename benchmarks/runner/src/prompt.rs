//! The one prompt builder every arm and both modes use.
//!
//! Fairness rule (RFC-0009 §4.1): within a task family, arms' user messages differ **only**
//! in the arm's `language_name`, its `file_kind`, and the naming sentence's enum names and
//! class accessor, all read from its `arm.toml`. A test proves it for every pair of arms in
//! `benchmarks/arms/`. The system message is the arm's guide file, verbatim. The feedback
//! text is arm-independent.

use crate::arm::{ArmConfig, TaskFamily};
use crate::extract::ExtractError;

pub use crate::arm::snake_case;

/// The single enum-naming sentence, present only when the task names its enums and the arm
/// is a UI arm. The scorer keys facts by enum name, so without this an agent that picks a
/// different name would be scored on naming, not behaviour, in every arm. Deliberately
/// names only the enums and the class accessor: nothing about heights or any other scored
/// fact.
pub fn enum_sentence(arm: &ArmConfig, enums: &[String]) -> Option<String> {
    if enums.is_empty() {
        return None;
    }
    let accessor = arm.class_accessor.as_deref()?;
    let names: Vec<String> = enums
        .iter()
        .map(|e| format!("`{}`", arm.enum_name(e)))
        .collect();
    Some(format!(
        "Name the variant enums exactly {}, {accessor}.",
        names.join(", ")
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

/// Build the system and user messages for one episode. `spec` is the task's input for
/// `family`: `spec.tsx` for `ui-port`, `spec.md` for `ui-spec` and `backend`. `enums` is the
/// task's optional `enums = [...]` list (Rust PascalCase); empty means the naming sentence
/// is omitted.
///
/// The `ui-port` wording is the pilots' wording, byte for byte, so a `ui-port` run can still
/// be compared with them (RFC-0009 §2.1).
pub fn build_prompt(
    arm: &ArmConfig,
    family: TaskFamily,
    guide: &str,
    spec: &str,
    enums: &[String],
) -> Prompt {
    let fence = fence_for(spec);
    let naming = enum_sentence(arm, enums)
        .map(|s| format!(" {s}"))
        .unwrap_or_default();
    let (ask, info) = match family {
        TaskFamily::UiPort => (
            "Port this React component to {lang}. Keep the same variants, sizes, defaults and \
             behaviour.",
            "tsx",
        ),
        TaskFamily::UiSpec => (
            "Implement the component specified below in {lang}. Keep exactly the variants, \
             classes, defaults, slots and elements it specifies.",
            "markdown",
        ),
        TaskFamily::Backend => (
            "Implement the HTTP service specified below in {lang}. Keep exactly the routes, \
             status codes, headers and bodies it specifies.",
            "markdown",
        ),
    };
    let user = format!(
        "{ask} The result must be {kind}.{naming} Reply with exactly one fenced code block \
         containing the complete file and nothing else.\n\n{fence}{info}\n{spec}{nl}{fence}\n",
        ask = ask.replace("{lang}", &arm.language_name),
        kind = arm.file_kind,
        nl = if spec.ends_with('\n') { "" } else { "\n" },
    );
    Prompt {
        system: guide.to_string(),
        user,
    }
}

/// The user message fed back after a compile check reports errors. The diagnostics are
/// appended verbatim — no trimming, no reformatting — so both arms' compilers are judged
/// on exactly what they print.
pub fn compile_error_feedback(diagnostics: &str) -> String {
    format!(
        "The compile check reported errors. Fix them and reply with exactly one fenced code \
         block containing the complete corrected file and nothing else.\n\n{diagnostics}"
    )
}

/// The user message fed back when a reply has no usable code block (Mode 1 only — Mode 2
/// submits a file, so there is nothing to extract).
pub fn extraction_feedback(err: &ExtractError) -> String {
    format!(
        "{err}. Reply with exactly one fenced code block containing the complete file and \
         nothing else."
    )
}

/// A backtick fence longer than any backtick run inside `text`, so a spec that itself
/// contains ``` cannot close the fence early.
fn fence_for(text: &str) -> String {
    let mut longest = 0usize;
    let mut run = 0usize;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat((longest + 1).max(3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const SPEC: &str = "export function Button() {\n  return <button />;\n}\n";

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
    }

    fn arm(id: &str) -> ArmConfig {
        ArmConfig::load(&repo(), id).unwrap()
    }

    fn all_arms() -> Vec<ArmConfig> {
        crate::arm::list_arms(&repo())
            .unwrap()
            .iter()
            .map(|id| arm(id))
            .collect()
    }

    fn enums() -> Vec<String> {
        vec!["ButtonVariant".into(), "ButtonSize".into()]
    }

    /// Replace everything an arm may legitimately differ in with a placeholder.
    fn neutralise(arm: &ArmConfig, text: &str) -> String {
        // The file kind contains the language name, so it is replaced first.
        let mut t = text.replace(&arm.file_kind, "<KIND>");
        if let Some(a) = &arm.class_accessor {
            t = t.replace(a.as_str(), "<ACCESSOR>");
        }
        for e in enums() {
            t = t.replace(&arm.enum_name(&e), "<ENUM>");
        }
        t.replace(&arm.language_name, "<LANG>")
    }

    /// RFC-0009 §4.1: within a family, every pair of arms' user messages is identical once
    /// the arms' own phrases are neutralised, with and without the naming sentence. Runs over
    /// every `arm.toml` there is, so a new arm is covered with no change here.
    #[test]
    fn every_pair_of_arms_differs_only_in_naming() {
        let arms = all_arms();
        assert!(arms.len() >= 3);
        for family in [TaskFamily::UiPort, TaskFamily::UiSpec, TaskFamily::Backend] {
            let runs: Vec<&ArmConfig> = arms.iter().filter(|a| a.runs(family)).collect();
            for en in [vec![], enums()] {
                for (i, a) in runs.iter().enumerate() {
                    for b in &runs[i + 1..] {
                        let pa = build_prompt(a, family, "G", SPEC, &en);
                        let pb = build_prompt(b, family, "G", SPEC, &en);
                        assert_ne!(pa.user, pb.user, "{} vs {}", a.id, b.id);
                        assert_eq!(
                            neutralise(a, &pa.user),
                            neutralise(b, &pb.user),
                            "{} vs {} in {}",
                            a.id,
                            b.id,
                            family.as_str()
                        );
                        assert!(!pa.user.contains("height"), "{}", a.id);
                    }
                }
            }
        }
    }

    #[test]
    fn the_ui_port_message_is_the_pilots_byte_for_byte() {
        // The wording `prompt.rs` produced before arms became data, copied from
        // benchmarks/results/2026-09-27-pilot-2/raw/scored/*/mzizi/button/seed-1/user.txt's
        // first paragraph. A change here would make ui-port incomparable with the pilots.
        let m = build_prompt(&arm("mzizi"), TaskFamily::UiPort, "G", SPEC, &enums());
        assert!(m.user.starts_with(
            "Port this React component to Mzizi. Keep the same variants, sizes, defaults and \
             behaviour. The result must be a single Mzizi source file (.mz). Name the variant \
             enums exactly `button_variant`, `button_size`, each with a `class` column. Reply \
             with exactly one fenced code block containing the complete file and nothing \
             else.\n\n```tsx\n"
        ));
        let d = build_prompt(&arm("dioxus"), TaskFamily::UiPort, "G", SPEC, &enums());
        assert!(d.user.contains(
            "Name the variant enums exactly `ButtonVariant`, `ButtonSize`, each with a \
             `classes()` method returning its Tailwind classes."
        ));
        let l = build_prompt(&arm("leptos"), TaskFamily::UiPort, "G", SPEC, &enums());
        assert_eq!(d.user.replace("Dioxus", "Leptos"), l.user);
        assert_eq!(m.user.matches("Name the variant enums").count(), 1);
        let bare = build_prompt(&arm("mzizi"), TaskFamily::UiPort, "G", SPEC, &[]);
        assert!(!bare.user.contains("Name the variant enums"));
    }

    #[test]
    fn the_pilot_user_messages_are_reproduced_exactly() {
        // The strongest no-behaviour-change check there is: rebuild the message pilot 2
        // recorded for each arm and task from the committed spec and the arm.toml, and
        // compare the bytes.
        let r = repo();
        let raw = r.join("benchmarks/results/2026-09-27-pilot-2/raw/scored");
        let mut checked = 0;
        for model in std::fs::read_dir(&raw).unwrap() {
            let model = model.unwrap().path();
            for id in ["mzizi", "dioxus"] {
                for task in ["button", "badge"] {
                    let user = model.join(id).join(task).join("seed-1/user.txt");
                    let Ok(want) = std::fs::read_to_string(&user) else {
                        continue;
                    };
                    let t = crate::task::load_task(&r.join("benchmarks/tasks").join(task)).unwrap();
                    let spec =
                        std::fs::read_to_string(t.input(TaskFamily::UiPort).unwrap()).unwrap();
                    let got = build_prompt(&arm(id), TaskFamily::UiPort, "", &spec, &t.enums);
                    assert_eq!(got.user, want, "{}", user.display());
                    checked += 1;
                }
            }
        }
        assert!(checked >= 4, "only {checked} pilot messages found");
    }

    #[test]
    fn the_ui_spec_message_embeds_spec_md_verbatim() {
        let md = "# Button\n\n| variant | classes |\n| --- | --- |\n";
        let p = build_prompt(&arm("dioxus"), TaskFamily::UiSpec, "G", md, &[]);
        assert!(
            p.user
                .starts_with("Implement the component specified below in Dioxus.")
        );
        assert!(p.user.contains(&format!("```markdown\n{md}```\n")));
        assert!(!p.user.contains("React"));
    }

    #[test]
    fn system_is_the_guide_verbatim() {
        let guide = "# Guide\n\n  indented, trailing spaces   \n\n";
        for a in all_arms() {
            for f in [TaskFamily::UiPort, TaskFamily::UiSpec] {
                assert_eq!(build_prompt(&a, f, guide, SPEC, &enums()).system, guide);
            }
        }
    }

    #[test]
    fn spec_is_embedded_verbatim() {
        let p = build_prompt(&arm("mzizi"), TaskFamily::UiPort, "", SPEC, &[]);
        assert!(p.user.contains(SPEC));
        assert!(p.user.starts_with("Port this React component to Mzizi."));
    }

    #[test]
    fn spec_containing_a_fence_gets_a_longer_fence() {
        let spec = "// ```inside```\n";
        let p = build_prompt(&arm("dioxus"), TaskFamily::UiPort, "", spec, &[]);
        assert!(p.user.contains("````tsx\n"));
        let block = crate::extract::extract_code_block(&p.user).unwrap();
        assert_eq!(block, spec);
    }

    #[test]
    fn feedback_is_arm_independent_and_verbatim() {
        let diag = "{\"severity\":\"error\"}\n  weird   spacing\n";
        let f = compile_error_feedback(diag);
        assert!(f.ends_with(diag));
        assert!(!f.contains("Mzizi") && !f.contains("Dioxus"));
    }
}
