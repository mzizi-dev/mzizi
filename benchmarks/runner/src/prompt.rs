//! The one prompt builder both arms and both modes use.
//!
//! Fairness rule: the two arms' user messages differ **only** in [`Arm::language_name`]
//! and [`Arm::file_kind`]. The system message is the arm's guide file, verbatim — so the
//! guide is part of that arm's prompt cost, deliberately (a language that needs a longer
//! guide to be usable pays for it in tokens). The feedback text is arm-independent.

use crate::extract::ExtractError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arm {
    Mzizi,
    Dioxus,
}

impl Arm {
    pub fn parse(s: &str) -> Result<Arm, String> {
        match s {
            "mzizi" => Ok(Arm::Mzizi),
            "dioxus" => Ok(Arm::Dioxus),
            other => Err(format!("unknown arm `{other}` (expected mzizi or dioxus)")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Arm::Mzizi => "mzizi",
            Arm::Dioxus => "dioxus",
        }
    }

    pub fn language_name(self) -> &'static str {
        match self {
            Arm::Mzizi => "Mzizi",
            Arm::Dioxus => "Dioxus",
        }
    }

    /// What the one file in the reply is — the other arm-specific phrase in the prompt.
    pub fn file_kind(self) -> &'static str {
        match self {
            Arm::Mzizi => "a single Mzizi source file (.mz)",
            Arm::Dioxus => "a single Rust source file (.rs) using Dioxus",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Arm::Mzizi => "mz",
            Arm::Dioxus => "rs",
        }
    }

    pub fn guide_file_name(self) -> &'static str {
        match self {
            Arm::Mzizi => "mzizi-guide.md",
            Arm::Dioxus => "dioxus-guide.md",
        }
    }

    /// An enum name from `task.toml` (Rust PascalCase) in this arm's convention.
    pub fn enum_name(self, pascal: &str) -> String {
        match self {
            Arm::Mzizi => snake_case(pascal),
            Arm::Dioxus => pascal.to_string(),
        }
    }

    /// How this arm's enums expose their Tailwind classes — the one other phrase in the
    /// enum-naming sentence that is arm-specific.
    pub fn class_accessor_phrase(self) -> &'static str {
        match self {
            Arm::Mzizi => "each with a `class` column",
            Arm::Dioxus => "each with a `classes()` method returning its Tailwind classes",
        }
    }
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

/// The single enum-naming sentence, present only when the task names its enums. The
/// scorer keys facts by enum name, so without this an agent that picks a different name
/// would be scored on naming, not behaviour — in both arms. Deliberately names only the
/// enums and the class accessor: nothing about heights or any other scored fact.
pub fn enum_sentence(arm: Arm, enums: &[String]) -> Option<String> {
    if enums.is_empty() {
        return None;
    }
    let names: Vec<String> = enums
        .iter()
        .map(|e| format!("`{}`", arm.enum_name(e)))
        .collect();
    Some(format!(
        "Name the variant enums exactly {}, {}.",
        names.join(", "),
        arm.class_accessor_phrase()
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

/// Build the system and user messages for one episode. `enums` is the task's optional
/// `enums = [...]` list (Rust PascalCase); empty means the naming sentence is omitted.
pub fn build_prompt(arm: Arm, guide: &str, spec_tsx: &str, enums: &[String]) -> Prompt {
    let fence = fence_for(spec_tsx);
    let naming = enum_sentence(arm, enums)
        .map(|s| format!(" {s}"))
        .unwrap_or_default();
    let user = format!(
        "Port this React component to {lang}. Keep the same variants, sizes, defaults and \
         behaviour. The result must be {kind}.{naming} Reply with exactly one fenced code \
         block containing the complete file and nothing else.\n\n{fence}tsx\n{spec}{nl}{fence}\n",
        lang = arm.language_name(),
        kind = arm.file_kind(),
        spec = spec_tsx,
        nl = if spec_tsx.ends_with('\n') { "" } else { "\n" },
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

    const SPEC: &str = "export function Button() {\n  return <button />;\n}\n";

    fn enums() -> Vec<String> {
        vec!["ButtonVariant".into(), "ButtonSize".into()]
    }

    fn neutralise(arm: Arm, text: &str) -> String {
        // The file kind contains the language name, so it is replaced first.
        let mut t = text
            .replace(arm.file_kind(), "<KIND>")
            .replace(arm.class_accessor_phrase(), "<ACCESSOR>");
        for e in enums() {
            t = t.replace(&arm.enum_name(&e), "<ENUM>");
        }
        t.replace(arm.language_name(), "<LANG>")
    }

    #[test]
    fn arms_differ_only_in_language_name_and_file_kind() {
        let m = build_prompt(Arm::Mzizi, "G", SPEC, &[]);
        let d = build_prompt(Arm::Dioxus, "G", SPEC, &[]);
        assert_ne!(m.user, d.user);
        assert_eq!(
            neutralise(Arm::Mzizi, &m.user),
            neutralise(Arm::Dioxus, &d.user)
        );
        assert!(!m.user.contains("Name the variant enums"));
    }

    #[test]
    fn enum_sentence_is_the_same_modulo_naming_convention() {
        let m = build_prompt(Arm::Mzizi, "G", SPEC, &enums());
        let d = build_prompt(Arm::Dioxus, "G", SPEC, &enums());
        assert!(m.user.contains(
            "Name the variant enums exactly `button_variant`, `button_size`, each with a \
             `class` column."
        ));
        assert!(d.user.contains(
            "Name the variant enums exactly `ButtonVariant`, `ButtonSize`, each with a \
             `classes()` method returning its Tailwind classes."
        ));
        assert_eq!(
            neutralise(Arm::Mzizi, &m.user),
            neutralise(Arm::Dioxus, &d.user)
        );
        // Exactly one added sentence, and it hints at nothing scored beyond names.
        assert_eq!(m.user.matches("Name the variant enums").count(), 1);
        assert!(!m.user.contains("height") && !d.user.contains("height"));
    }

    #[test]
    fn snake_case_conversion() {
        assert_eq!(snake_case("ButtonSize"), "button_size");
        assert_eq!(snake_case("HTTPStatus"), "http_status");
        assert_eq!(snake_case("Size"), "size");
        assert_eq!(snake_case("IconSm"), "icon_sm");
    }

    #[test]
    fn system_is_the_guide_verbatim() {
        let guide = "# Guide\n\n  indented, trailing spaces   \n\n";
        assert_eq!(build_prompt(Arm::Mzizi, guide, SPEC, &[]).system, guide);
        assert_eq!(
            build_prompt(Arm::Dioxus, guide, SPEC, &enums()).system,
            guide
        );
    }

    #[test]
    fn spec_is_embedded_verbatim() {
        let p = build_prompt(Arm::Mzizi, "", SPEC, &[]);
        assert!(p.user.contains(SPEC));
        assert!(p.user.starts_with("Port this React component to Mzizi."));
    }

    #[test]
    fn spec_containing_a_fence_gets_a_longer_fence() {
        let spec = "// ```inside```\n";
        let p = build_prompt(Arm::Dioxus, "", spec, &[]);
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
