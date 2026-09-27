//! Harness-side reference comparison — the other half of CHARTER.md §6's defect metric
//! (RFC-0006 §10.1).
//!
//! `mz contract` (`compiler/src/contract.rs`) checks a component against its own
//! declarations: a self-consistency check. An agent that writes both a component and its
//! contract can satisfy that check while still diverging from the hand-written Rust the
//! component is meant to port. This crate does the comparison RFC-0006 left to the harness:
//! it reads a `.mz` file's declared per-variant pixel heights and a reference `.rs` file's
//! Tailwind classes for the same variants, and reports every place they disagree.
//!
//! The conversion from a Tailwind class to a pixel height is the one, bounded, well-known
//! linear spacing scale — `value-in-rem = N * 0.25rem`, and browsers default `1rem = 16px`,
//! so `h-N` and `size-N` are both `N * 4` pixels. This is deliberately not a general
//! Tailwind resolver (arbitrary values like `h-[56px]` are `mz contract`'s job, not this
//! one's — RFC-0006 §5) and the `.rs` reader is deliberately not a general Rust parser: it
//! looks for `<path>::<Variant> => "<classes>"` match arms and nothing more elaborate.

#![deny(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// One variant of a Mzizi `enum` that declares both a `class` string and a `height`.
///
/// An enum variant with only a `class` (no `height`) — `button_variant` in `button.mz` — is
/// not part of the size metric and never produces one of these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MzVariant {
    /// The variant's name, as written in the `.mz` source (snake_case by convention).
    pub name: String,
    /// The variant's `class` string, verbatim.
    pub class: String,
    /// The variant's declared `height`, in pixels.
    pub height: u32,
}

/// One `enum <name> ... end` block whose variants carry a `height` — a size enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MzSizeEnum {
    /// The enum's name, as written after `enum`.
    pub name: String,
    /// Its height-bearing variants, in source order.
    pub variants: Vec<MzVariant>,
}

/// Parse every height-bearing `enum` block out of a `.mz` source file.
///
/// An enum with no `height` on any variant produces no entry — there is nothing in a
/// Tailwind-only reference to diff it against.
pub fn parse_mzizi_size_enums(src: &str) -> Vec<MzSizeEnum> {
    let mut enums = Vec::new();
    let mut lines = src.lines();
    while let Some(line) = lines.next() {
        let Some(name) = enum_header(line) else {
            continue;
        };
        let mut variants = Vec::new();
        for body_line in lines.by_ref() {
            if body_line.trim() == "end" {
                break;
            }
            if let Some(variant) = parse_variant_line(body_line) {
                variants.push(variant);
            }
        }
        if !variants.is_empty() {
            enums.push(MzSizeEnum { name, variants });
        }
    }
    enums
}

/// `  enum button_size` → `Some("button_size")`. Anything else is `None`.
fn enum_header(line: &str) -> Option<String> {
    let name = line.trim().strip_prefix("enum ")?.trim();
    (!name.is_empty() && is_identifier(name)).then(|| name.to_string())
}

/// A variant line carries both `class "..."` and `height <N>`, in either order, alongside
/// whatever else the row holds. A line missing either — including a `##` comment line,
/// which has neither — is not a variant line.
fn parse_variant_line(line: &str) -> Option<MzVariant> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("##") {
        return None;
    }
    let tokens = tokenize(line);
    let name = tokens.first()?.clone();
    if !is_identifier(&name) {
        return None;
    }
    let mut class = None;
    let mut height = None;
    let mut i = 1;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "class" if i + 1 < tokens.len() => {
                class = Some(tokens[i + 1].clone());
                i += 2;
            }
            "height" if i + 1 < tokens.len() => {
                height = tokens[i + 1].parse::<u32>().ok();
                i += 2;
            }
            _ => i += 1,
        }
    }
    Some(MzVariant {
        name,
        class: class?,
        height: height?,
    })
}

/// Split a line on whitespace, keeping a `"..."` run as one dequoted token — enough to read
/// `default   class "h-14 gap-2 px-5"     height 56` as
/// `[default, class, h-14 gap-2 px-5, height, 56]`.
fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else if c == '"' {
            let mut quoted = String::new();
            for inner in chars.by_ref() {
                if inner == '"' {
                    break;
                }
                quoted.push(inner);
            }
            tokens.push(quoted);
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// One `<path>::<Variant> => "<classes>"` match arm read out of a reference `.rs` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RsVariant {
    /// The variant identifier, normalised from Rust's `PascalCase` to the `snake_case` a
    /// Mzizi enum variant would use for the same name (`IconSm` → `icon_sm`).
    pub name: String,
    /// The class string on the right-hand side of `=>`, verbatim.
    pub class: String,
}

/// Scan a reference `.rs` file's `match` arms for `<path>::<Variant> => "<classes>"`, or the
/// bare `<Variant> => "<classes>"` form.
///
/// This is line-oriented and regex-level by design (RFC-0006 §10.1): it does not parse Rust,
/// it looks for the one shape the corpus's `classes()`-style methods use. A wildcard arm
/// (`_ => "..."`) is never mistaken for a variant, because `_` does not start with an
/// uppercase letter.
pub fn parse_rust_match_arms(src: &str) -> Vec<RsVariant> {
    let mut out = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        let Some(arrow) = trimmed.find("=>") else {
            continue;
        };
        let (lhs, rhs) = trimmed.split_at(arrow);
        let Some(class) = extract_quoted(&rhs[2..]) else {
            continue;
        };
        let variant = lhs.trim().rsplit("::").next().unwrap_or("").trim();
        if !variant.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        out.push(RsVariant {
            name: to_snake_case(variant),
            class,
        });
    }
    out
}

fn extract_quoted(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn to_snake_case(ident: &str) -> String {
    let mut out = String::new();
    for (i, c) in ident.chars().enumerate() {
        if c.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The Tailwind linear spacing scale, and nothing past it: `h-N` / `size-N` → `N * 4` px.
/// Returns the matching token alongside the pixel value, for display.
fn tailwind_scale_token(class: &str) -> Option<(String, u32)> {
    class.split_whitespace().find_map(|token| {
        let digits = token
            .strip_prefix("h-")
            .or_else(|| token.strip_prefix("size-"))?;
        let n: u32 = digits.parse().ok()?;
        Some((token.to_string(), n * 4))
    })
}

/// The outcome of comparing one variant name that appears on at least one side of the diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariantResult {
    /// The `.mz` file's declared height matches the reference's derived height.
    Pass {
        /// The variant name.
        name: String,
        /// The height `.mz` declares, in pixels.
        declared: u32,
        /// The height derived from the reference's Tailwind class, in pixels.
        derived: u32,
        /// The Tailwind token the height was derived from (`h-14`, `size-12`, …).
        token: String,
    },
    /// The `.mz` file's declared height does not match the reference's derived height —
    /// the Phase 0 defect this harness exists to find.
    Mismatch {
        /// The variant name.
        name: String,
        /// The height `.mz` declares, in pixels.
        declared: u32,
        /// The height derived from the reference's Tailwind class, in pixels.
        derived: u32,
        /// The Tailwind token the height was derived from.
        token: String,
    },
    /// The `.mz` file declares this variant; the reference has no matching arm.
    MissingInReference {
        /// The variant name.
        name: String,
        /// The height `.mz` declares, in pixels.
        declared: u32,
    },
    /// The reference declares this variant, with a size-scale class; the `.mz` file has no
    /// matching variant in the size enum.
    MissingInMzizi {
        /// The variant name.
        name: String,
        /// The reference's class string.
        class: String,
    },
    /// Both sides name this variant, but no `h-N` / `size-N` token was found in the
    /// reference's class string, so no height could be derived.
    Unevaluable {
        /// The variant name.
        name: String,
        /// The reference's class string.
        class: String,
    },
}

impl VariantResult {
    /// A passing comparison is not a defect; everything else is.
    pub fn is_defect(&self) -> bool {
        !matches!(self, VariantResult::Pass { .. })
    }
}

impl fmt::Display for VariantResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VariantResult::Pass {
                name,
                declared,
                derived,
                token,
            } => write!(
                f,
                "PASS  {name:<10} declared={declared}px  derived={derived}px  ({token})"
            ),
            VariantResult::Mismatch {
                name,
                declared,
                derived,
                token,
            } => write!(
                f,
                "FAIL  {name:<10} declared={declared}px  derived={derived}px  ({token}) — mismatch"
            ),
            VariantResult::MissingInReference { name, declared } => write!(
                f,
                "FAIL  {name:<10} declared={declared}px  — no matching variant in the reference"
            ),
            VariantResult::MissingInMzizi { name, class } => write!(
                f,
                "FAIL  {name:<10} reference class=\"{class}\"  — no matching variant in the .mz file"
            ),
            VariantResult::Unevaluable { name, class } => write!(
                f,
                "FAIL  {name:<10} reference class=\"{class}\"  — no h-N/size-N token found, unevaluable"
            ),
        }
    }
}

/// Every comparison produced for one `.mz` size enum against one reference file's variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffReport {
    /// The `.mz` enum's name.
    pub enum_name: String,
    /// One result per variant name that appears on either side.
    pub results: Vec<VariantResult>,
}

impl DiffReport {
    /// How many of this report's results are defects.
    pub fn defect_count(&self) -> usize {
        self.results.iter().filter(|r| r.is_defect()).count()
    }
}

/// Diff one `.mz` size enum against the variants read out of a reference `.rs` file.
///
/// Every variant the `.mz` enum declares gets a result — `Pass`, `Mismatch`, or
/// `MissingInReference`. A reference variant with a size-scale class that the `.mz` enum
/// never names also gets a result (`MissingInMzizi`) — a variant dropped on the Mzizi side
/// is as much a defect as a mismatched one, and RFC-0006 §10.1 asks that missing variants on
/// either side be reported, not silently skipped.
pub fn diff_size_enum(mz_enum: &MzSizeEnum, rust_variants: &[RsVariant]) -> DiffReport {
    let rust_map: BTreeMap<&str, &str> = rust_variants
        .iter()
        .map(|v| (v.name.as_str(), v.class.as_str()))
        .collect();
    let mut seen_rust: BTreeSet<&str> = BTreeSet::new();
    let mut results = Vec::new();

    for v in &mz_enum.variants {
        match rust_map.get(v.name.as_str()) {
            None => results.push(VariantResult::MissingInReference {
                name: v.name.clone(),
                declared: v.height,
            }),
            Some(class) => {
                seen_rust.insert(v.name.as_str());
                match tailwind_scale_token(class) {
                    None => results.push(VariantResult::Unevaluable {
                        name: v.name.clone(),
                        class: (*class).to_string(),
                    }),
                    Some((token, derived)) if derived == v.height => {
                        results.push(VariantResult::Pass {
                            name: v.name.clone(),
                            declared: v.height,
                            derived,
                            token,
                        })
                    }
                    Some((token, derived)) => results.push(VariantResult::Mismatch {
                        name: v.name.clone(),
                        declared: v.height,
                        derived,
                        token,
                    }),
                }
            }
        }
    }

    for rv in rust_variants {
        if seen_rust.contains(rv.name.as_str()) {
            continue;
        }
        if tailwind_scale_token(&rv.class).is_some() {
            results.push(VariantResult::MissingInMzizi {
                name: rv.name.clone(),
                class: rv.class.clone(),
            });
        }
    }

    DiffReport {
        enum_name: mz_enum.name.clone(),
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTON_MZ: &str = r#"component button

  enum button_variant
    default      class "bg-primary text-primary-foreground hover:bg-primary/80"
  end

  ## Sizes carry their own touch height as data.
  enum button_size
    default   class "h-14 gap-2 px-5"     height 56
    sm        class "h-12 gap-1.5 px-4"   height 48
    lg        class "h-14 gap-2 px-6"     height 56
    icon      class "size-14"             height 56
    icon_sm   class "size-12"             height 48
  end

end component button
"#;

    const BUTTON_RS: &str = r#"
impl ButtonSize {
    pub fn classes(&self) -> &'static str {
        match self {
            ButtonSize::Default => "h-14 gap-2 px-5",
            ButtonSize::Sm => "h-12 gap-1.5 px-4",
            ButtonSize::Lg => "h-14 gap-2 px-6",
            ButtonSize::Icon => "size-14",
            ButtonSize::IconSm => "size-12",
        }
    }
}
"#;

    #[test]
    fn only_the_height_bearing_enum_is_collected() {
        let enums = parse_mzizi_size_enums(BUTTON_MZ);
        assert_eq!(
            enums.len(),
            1,
            "button_variant has no height and must not appear"
        );
        assert_eq!(enums[0].name, "button_size");
        assert_eq!(enums[0].variants.len(), 5);
    }

    #[test]
    fn variant_fields_are_read_correctly() {
        let enums = parse_mzizi_size_enums(BUTTON_MZ);
        let sm = enums[0].variants.iter().find(|v| v.name == "sm").unwrap();
        assert_eq!(sm.class, "h-12 gap-1.5 px-4");
        assert_eq!(sm.height, 48);
    }

    #[test]
    fn rust_match_arms_are_read_and_normalised_to_snake_case() {
        let variants = parse_rust_match_arms(BUTTON_RS);
        assert_eq!(variants.len(), 5);
        let icon_sm = variants.iter().find(|v| v.name == "icon_sm").unwrap();
        assert_eq!(icon_sm.class, "size-12");
    }

    #[test]
    fn a_wildcard_arm_is_never_read_as_a_variant() {
        let variants =
            parse_rust_match_arms(r#"match self { ButtonSize::Sm => "h-12", _ => "h-14" }"#);
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].name, "sm");
    }

    #[test]
    fn the_real_button_declarations_match_the_real_tailwind_classes() {
        let enums = parse_mzizi_size_enums(BUTTON_MZ);
        let rust_variants = parse_rust_match_arms(BUTTON_RS);
        let report = diff_size_enum(&enums[0], &rust_variants);
        assert_eq!(report.defect_count(), 0, "{:#?}", report.results);
    }

    #[test]
    fn a_declared_height_that_disagrees_with_the_tailwind_class_is_a_mismatch() {
        let broken = BUTTON_MZ.replace(
            "sm        class \"h-12 gap-1.5 px-4\"   height 48",
            "sm        class \"h-12 gap-1.5 px-4\"   height 44",
        );
        let enums = parse_mzizi_size_enums(&broken);
        let rust_variants = parse_rust_match_arms(BUTTON_RS);
        let report = diff_size_enum(&enums[0], &rust_variants);
        assert_eq!(report.defect_count(), 1);
        let sm_result = report
            .results
            .iter()
            .find(|r| matches!(r, VariantResult::Mismatch { name, .. } if name == "sm"));
        assert!(matches!(
            sm_result,
            Some(VariantResult::Mismatch {
                declared: 44,
                derived: 48,
                token,
                ..
            }) if token == "h-12"
        ));
    }

    #[test]
    fn a_variant_missing_from_the_reference_is_reported_not_skipped() {
        let mz = r#"component x
  enum s
    only  class "h-14"  height 56
  end
end component x
"#;
        let enums = parse_mzizi_size_enums(mz);
        let report = diff_size_enum(&enums[0], &[]);
        assert_eq!(report.defect_count(), 1);
        assert!(matches!(
            &report.results[0],
            VariantResult::MissingInReference { name, declared: 56 } if name == "only"
        ));
    }

    #[test]
    fn a_reference_variant_missing_from_mzizi_is_reported_not_skipped() {
        let rust_variants = vec![RsVariant {
            name: "xl".to_string(),
            class: "h-16".to_string(),
        }];
        let mz = r#"component x
  enum s
    only  class "h-14"  height 56
  end
end component x
"#;
        let enums = parse_mzizi_size_enums(mz);
        let report = diff_size_enum(&enums[0], &rust_variants);
        assert_eq!(report.defect_count(), 2);
        assert!(
            report
                .results
                .iter()
                .any(|r| matches!(r, VariantResult::MissingInMzizi { name, .. } if name == "xl"))
        );
    }

    #[test]
    fn an_arbitrary_bracket_value_is_unevaluable_not_guessed() {
        let rust_variants = vec![RsVariant {
            name: "only".to_string(),
            class: "h-[56px]".to_string(),
        }];
        let mz = r#"component x
  enum s
    only  class "h-14"  height 56
  end
end component x
"#;
        let enums = parse_mzizi_size_enums(mz);
        let report = diff_size_enum(&enums[0], &rust_variants);
        assert_eq!(report.defect_count(), 1);
        assert!(matches!(
            &report.results[0],
            VariantResult::Unevaluable { .. }
        ));
    }
}
