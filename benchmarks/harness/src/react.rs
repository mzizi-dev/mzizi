//! The React extractor (RFC-0009 §2.2): the same UI facts the Rust and Mzizi extractors read,
//! read out of a TypeScript/TSX candidate's `cva(…)` calls and `data-slot` attributes.
//!
//! A `class-variance-authority` call is the registry's own variant system (every N2 `.tsx`
//! that has variants uses one), and the React arm's guide requires it, just as the Rust
//! guides require a `classes()` method and the Mzizi guide a `class` column. The shape read:
//!
//! ```text
//! const buttonVariants = cva("<base classes>", {
//!   variants: {
//!     size: { default: "h-14 …", sm: "h-12 …", "icon-sm": "size-12" },
//!   },
//!   defaultVariants: { size: "default" },
//! })
//! ```
//!
//! Each key under `variants` is one enum. Its name is the call's name without its `Variants`
//! suffix, followed by the key, snake-cased: `buttonVariants` + `size` → `button_size`, which
//! is the snake-cased name of the reference's `ButtonSize`. The runner writes the naming
//! sentence by the same rule forwards (`enum_case = "cva"` in `arms/react/arm.toml`). A
//! variant key is snake-cased too, with `-` read as `_` (`"icon-sm"` → `icon_sm` ↔ `IconSm`).
//! The default is `defaultVariants[key]`. Heights come from each class string, by the same
//! [`tailwind_height`] the Rust extractor uses.
//!
//! Like the Rust reader, this is not a parser for the language. It blanks comments, reads
//! string literals (a template literal only when it has no `${…}`), matches brackets, and
//! looks for fixed shapes. A class value that is not one string literal (a variable, a
//! concatenation, an array) is not read. When the reference has a class for that variant,
//! the missing class is then a defect, never a pass (RFC-0006 FM-12).

use std::collections::{BTreeMap, BTreeSet};

use crate::{EnumModel, tailwind_height, to_snake_case};

/// One token of a TypeScript source, as far as this extractor needs.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    /// An identifier or keyword.
    Ident(String),
    /// A string literal's value: `"…"`, `'…'`, or a template literal with no `${`.
    Str(String),
    /// A template literal with a substitution: its value is not known.
    Template,
    /// Any other single character that is not whitespace (`{`, `:`, `,`, `=`, `(`, …).
    Punct(char),
}

fn lex(src: &str) -> Vec<Tok> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if ch == '"' || ch == '\'' || ch == '`' {
            let quote = ch;
            i += 1;
            let mut v = String::new();
            let mut substituted = false;
            while i < c.len() && c[i] != quote {
                if c[i] == '\\' && i + 1 < c.len() {
                    v.push(match c[i + 1] {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                    i += 2;
                    continue;
                }
                if quote == '`' && c[i] == '$' && c.get(i + 1) == Some(&'{') {
                    substituted = true;
                }
                if quote != '`' && c[i] == '\n' {
                    break;
                }
                v.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push(if substituted {
                Tok::Template
            } else {
                Tok::Str(v)
            });
        } else if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '$') {
                i += 1;
            }
            out.push(Tok::Ident(c[start..i].iter().collect()));
        } else {
            out.push(Tok::Punct(ch));
            i += 1;
        }
    }
    out
}

/// The index of the bracket that closes the one at `open`, counting `(`, `[` and `{` alike.
fn close_of(t: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (j, tok) in t.iter().enumerate().skip(open) {
        match tok {
            Tok::Punct('(' | '[' | '{') => depth += 1,
            Tok::Punct(')' | ']' | '}') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
    }
    None
}

/// An object literal `{ … }` spanning `t[open..=close]`: each top-level `key: value` pair,
/// as `(key, index of the value's first token, index one past its last)`. A key is an
/// identifier or a string literal. Spreads, shorthand properties and computed keys are
/// skipped.
fn object_entries(t: &[Tok], open: usize, close: usize) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut i = open + 1;
    while i < close {
        let key = match &t[i] {
            Tok::Ident(s) | Tok::Str(s) => Some(s.clone()),
            _ => None,
        };
        let is_pair = key.is_some() && t.get(i + 1) == Some(&Tok::Punct(':'));
        // Find the end of this entry: the next top-level `,`, or the closing brace.
        let value_start = if is_pair { i + 2 } else { i };
        let mut j = value_start;
        while j < close {
            match t[j] {
                Tok::Punct('(' | '[' | '{') => j = close_of(t, j).map_or(close, |k| k + 1),
                Tok::Punct(',') => break,
                _ => j += 1,
            }
        }
        if let (true, Some(k)) = (is_pair, key) {
            out.push((k, value_start, j.min(close)));
        }
        i = j + 1;
    }
    out
}

/// The value `t[start..end]` as a single string literal, or `None`. `as const` after it is
/// allowed.
fn single_string(t: &[Tok], start: usize, end: usize) -> Option<String> {
    match &t[start..end] {
        [Tok::Str(s)] => Some(s.clone()),
        [Tok::Str(s), Tok::Ident(a), Tok::Ident(c)] if a == "as" && c == "const" => Some(s.clone()),
        _ => None,
    }
}

/// `-` read as `_`, then snake-cased: `"icon-sm"` → `icon_sm`, `iconSm` → `icon_sm`.
fn variant_name(key: &str) -> String {
    to_snake_case(&key.replace('-', "_"))
}

/// The enum name for `key` in the `cva` call bound to `call`: `buttonVariants` + `size` →
/// `button_size`. A call named just `variants` gives the key alone. A call whose name does
/// not end in `Variants` keeps its whole name as the prefix.
fn enum_name(call: &str, key: &str) -> String {
    let prefix = call
        .strip_suffix("Variants")
        .or_else(|| call.strip_suffix("variants"))
        .unwrap_or(call);
    let key = variant_name(key);
    if prefix.is_empty() {
        key
    } else {
        format!("{}_{key}", to_snake_case(prefix))
    }
}

/// Read every `cva(…)` call bound to a name (`const x = cva(…)`, `let`, `var`, or
/// `export const`), and return one [`EnumModel`] per key of its `variants` object.
pub fn parse_react_enums(src: &str) -> Vec<EnumModel> {
    let t = lex(src);
    let mut out = Vec::new();
    for i in 0..t.len() {
        // `<name> = cva (`
        let (Some(Tok::Ident(call)), Some(Tok::Punct('=')), Some(Tok::Ident(f))) =
            (t.get(i), t.get(i + 1), t.get(i + 2))
        else {
            continue;
        };
        if f != "cva" {
            continue;
        }
        let open = i + 3;
        if t.get(open) != Some(&Tok::Punct('(')) {
            continue;
        }
        let Some(close) = close_of(&t, open) else {
            continue;
        };
        // The config is the first `{` at depth 1 inside the call, after the base argument.
        let mut j = open + 1;
        let mut config = None;
        while j < close {
            match t[j] {
                Tok::Punct('{') => {
                    config = Some(j);
                    break;
                }
                Tok::Punct('(' | '[') => j = close_of(&t, j).map_or(close, |k| k + 1),
                _ => j += 1,
            }
        }
        let Some(cfg_open) = config else { continue };
        let Some(cfg_close) = close_of(&t, cfg_open) else {
            continue;
        };
        let entries = object_entries(&t, cfg_open, cfg_close);
        let obj = |name: &str| {
            entries
                .iter()
                .find(|(k, s, _)| k == name && t.get(*s) == Some(&Tok::Punct('{')))
                .and_then(|(_, s, _)| close_of(&t, *s).map(|c| (*s, c)))
        };
        let mut defaults = BTreeMap::new();
        if let Some((o, c)) = obj("defaultVariants") {
            for (k, s, e) in object_entries(&t, o, c) {
                if let Some(v) = single_string(&t, s, e) {
                    defaults.insert(k, variant_name(&v));
                }
            }
        }
        let Some((vo, vc)) = obj("variants") else {
            continue;
        };
        for (key, s, _) in object_entries(&t, vo, vc) {
            if t.get(s) != Some(&Tok::Punct('{')) {
                continue;
            }
            let Some(gc) = close_of(&t, s) else { continue };
            let mut e = EnumModel {
                name: enum_name(call, &key),
                default: defaults.get(&key).cloned(),
                ..EnumModel::default()
            };
            for (variant, vs, ve) in object_entries(&t, s, gc) {
                let name = variant_name(&variant);
                if let Some(class) = single_string(&t, vs, ve) {
                    if let Some((_, h)) = tailwind_height(&class) {
                        e.heights.insert(name.clone(), h);
                    }
                    e.classes.insert(name.clone(), class);
                }
                e.variants.push(name);
            }
            out.push(e);
        }
    }
    out
}

/// Every literal `data-slot` value a TSX file renders: `data-slot="x"`, `data-slot={"x"}`,
/// or an object key `"data-slot": "x"`. A computed value is not a literal and is not read.
pub fn parse_react_slots(src: &str) -> BTreeSet<String> {
    let t = lex(src);
    let mut out = BTreeSet::new();
    for i in 0..t.len() {
        // JSX: `data` `-` `slot` `=` value
        let jsx = matches!(
            (t.get(i), t.get(i + 1), t.get(i + 2), t.get(i + 3)),
            (Some(Tok::Ident(a)), Some(Tok::Punct('-')), Some(Tok::Ident(b)), Some(Tok::Punct('=')))
                if a == "data" && b == "slot"
        );
        let value_at = if jsx {
            Some(i + 4)
        } else if t.get(i) == Some(&Tok::Str("data-slot".into()))
            && t.get(i + 1) == Some(&Tok::Punct(':'))
        {
            Some(i + 2)
        } else {
            None
        };
        let Some(v) = value_at else { continue };
        match (t.get(v), t.get(v + 1), t.get(v + 2)) {
            (Some(Tok::Str(s)), _, _) => {
                out.insert(s.clone());
            }
            (Some(Tok::Punct('{')), Some(Tok::Str(s)), Some(Tok::Punct('}'))) => {
                out.insert(s.clone());
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTON_SPEC: &str = include_str!("../../tasks/button/spec.tsx");
    const BADGE_SPEC: &str = include_str!("../../tasks/badge/spec.tsx");
    const BUTTON_REF: &str = include_str!("../../tasks/button/reference.rs");
    const BADGE_REF: &str = include_str!("../../tasks/badge/reference.rs");

    #[test]
    fn the_registry_tsx_reads_as_its_own_rust_reference() {
        // The registry's own `.tsx` and its hand-written `.rs` port state the same facts,
        // so the React extractor on the one and the Rust extractor on the other must agree
        // exactly: zero defects is the proof the extractor reads what the reference means.
        for (tsx, rs, facts) in [(BUTTON_SPEC, BUTTON_REF, 9), (BADGE_SPEC, BADGE_REF, 2)] {
            let r = crate::parse_rust_enums(rs);
            let c = parse_react_enums(tsx);
            let report = crate::score(crate::Arm::React, &r, &c, false);
            assert_eq!(report.defects(), 0, "{}", report.to_json());
            assert_eq!(report.facts_checked(), facts, "{}", report.to_json());
        }
    }

    #[test]
    fn reads_the_button_cva_call() {
        let e = parse_react_enums(BUTTON_SPEC);
        let names: Vec<&str> = e.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["button_variant", "button_size"]);
        let size = &e[1];
        assert_eq!(size.variants, ["default", "sm", "lg", "icon", "icon_sm"]);
        assert_eq!(size.default.as_deref(), Some("default"));
        assert_eq!(size.heights["icon_sm"], 48);
        assert_eq!(size.heights["default"], 56);
        assert_eq!(size.classes["icon"], "size-14");
    }

    #[test]
    fn slots_in_every_literal_spelling() {
        let src = r#"
            <div data-slot="card" />
            <div data-slot={"card-title"} />
            <div data-slot={slotName} />
            const attrs = { "data-slot": "card-footer" }
            // <div data-slot="commented-out" />
        "#;
        let s: Vec<String> = parse_react_slots(src).into_iter().collect();
        assert_eq!(s, ["card", "card-footer", "card-title"]);
        let spec: Vec<String> = parse_react_slots(BUTTON_SPEC).into_iter().collect();
        assert_eq!(spec, ["button"]);
        // The registry's card.tsx renders exactly the seven slots its Rust port does.
        let card_tsx = include_str!("../../tasks/card/spec.tsx");
        let card_rs = include_str!("../../tasks/card/reference.rs");
        assert_eq!(
            parse_react_slots(card_tsx),
            crate::parse_rust_slots(card_rs)
        );
        assert_eq!(parse_react_slots(card_tsx).len(), 7);
    }

    #[test]
    fn what_is_not_one_literal_is_not_read() {
        let src = r#"
            const sizes = { sm: "h-12" }
            export const cardVariants = cva(["a", "b"], {
              variants: {
                size: {
                  default: `h-${n}`,
                  sm: sizes.sm,
                  lg: "h-" + "14",
                  xl: "h-16" as const,
                },
                ...rest,
              },
              defaultVariants: { size: "xl" },
            })
            const other = notCva({ variants: { x: { a: "h-1" } } })
        "#;
        let e = parse_react_enums(src);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].name, "card_size");
        assert_eq!(e[0].variants, ["default", "sm", "lg", "xl"]);
        assert_eq!(e[0].classes.len(), 1);
        assert_eq!(e[0].heights["xl"], 64);
        assert_eq!(e[0].default.as_deref(), Some("xl"));
    }

    #[test]
    fn enum_names_follow_the_runner_rule() {
        assert_eq!(enum_name("buttonVariants", "size"), "button_size");
        assert_eq!(
            enum_name("changelogNodeVariants", "accent"),
            "changelog_node_accent"
        );
        assert_eq!(enum_name("variants", "size"), "size");
        assert_eq!(enum_name("badge", "variant"), "badge_variant");
        assert_eq!(variant_name("icon-sm"), "icon_sm");
        assert_eq!(variant_name("iconSm"), "icon_sm");
    }
}
