//! Text in a program (RFC-0013 §10, tracker row C6): the text methods and their types, the
//! spellings other languages give them, and the helpers the lowering emits.
//!
//! Lengths count Unicode scalar values, as §10 chooses. The methods that return an option
//! (`slice`, `find`, `parse_int`, `parse_float`, and indexing `s[i]`) or a list (`split`,
//! `chars`) wait for options and lists (C7 and §18.2's "C4 options"): they are
//! [`WAITING`], and a call to one is `MZ0919`.
//!
//! The helpers live in `text/ops.rs`, which is compiled into `mz` (where its unit tests
//! run) and emitted verbatim into every lowered `main.rs`, as `numbers/float_text.rs` is.

use crate::expr::Ty;

mod ops;

pub use ops::{mz_text_length, mz_text_repeat};

/// The source of `ops.rs`, for the lowering to emit whole (RFC-0013 §14.2).
pub const OPS_RUNTIME: &str = include_str!("text/ops.rs");

/// Every text method a program can call (RFC-0013 §10), in alphabetical order.
pub const METHODS: &[&str] = &[
    "contains",
    "ends_with",
    "length",
    "repeat",
    "replace",
    "starts_with",
    "to_lower",
    "to_upper",
    "trim",
];

/// §10's methods that are designed and not built, with what each returns: an option or a
/// list, which a function body cannot hold yet.
pub const WAITING: &[(&str, &str)] = &[
    ("chars", "list(text)"),
    ("find", "option(int)"),
    ("parse_float", "option(float)"),
    ("parse_int", "option(int)"),
    ("slice", "option(text)"),
    ("split", "list(text)"),
];

/// A text method's signature on a receiver of type `recv`: its parameters' types and what
/// it returns, or `None` when `recv` is not `text` or has no built method `name`.
pub fn method(recv: Ty, name: &str) -> Option<(Vec<Ty>, Ty)> {
    if recv != Ty::Text {
        return None;
    }
    Some(match name {
        "length" => (vec![], Ty::Int),
        "contains" | "starts_with" | "ends_with" => (vec![Ty::Text], Ty::Bool),
        "trim" | "to_upper" | "to_lower" => (vec![], Ty::Text),
        "replace" => (vec![Ty::Text, Ty::Text], Ty::Text),
        "repeat" => (vec![Ty::Int], Ty::Text),
        _ => return None,
    })
}

/// The label a method's argument at `index` is written with (RFC-0013 §6.5): `by` on
/// `replace`'s second, and `to` on `slice`'s (§9.2, §10). The parser reads it, and the
/// canonical text writes it.
pub fn label(name: &str, index: usize) -> Option<&'static str> {
    match (name, index) {
        ("replace", 1) => Some("by"),
        ("slice", 1) => Some("to"),
        _ => None,
    }
}

/// What a waiting method (§10) returns, or `None` when `name` is not one.
pub fn waiting(name: &str) -> Option<&'static str> {
    WAITING.iter().find(|(m, _)| *m == name).map(|(_, r)| *r)
}

/// Every name §10 gives a text method, built or waiting, for the nearest-name fix.
pub fn designed() -> impl Iterator<Item = &'static str> {
    METHODS
        .iter()
        .copied()
        .chain(WAITING.iter().map(|(m, _)| *m))
}

/// Another language's spelling of a text method (RFC-0013 §16, `MZ0962`): the Mzizi name
/// it means and whether swapping the name keeps its behaviour, given how many arguments
/// were written. The lexer has already turned a camelCase word into snake_case
/// (`toUpperCase` is `to_upper_case` here). `None` when `name` is not such a spelling.
pub fn idiom(name: &str, argc: usize) -> Option<(&'static str, bool)> {
    Some(match (name, argc) {
        // Rust's `.len()` counts bytes, Java's `.size()` and a bare `count()` (§9.4's rule
        // for lists, here for text) are no text length at all: `"é".len()` is 2 in Rust
        // and `"é".length()` is 1 here, so the fix may change the meaning, and is a guess.
        // Python's `len(s)`, which counts scalar values as Mzizi does, is not a method and
        // keeps its `exact` fix.
        ("len" | "size" | "count", 0) => ("length", false),
        // Python's `strip()` also strips U+001C to U+001F, which `trim()` keeps: a guess.
        // With an argument it strips those characters instead.
        ("strip", 0) => ("trim", false),
        ("strip", _) => ("trim", false),
        ("upper" | "uppercase" | "to_uppercase" | "to_upper_case", 0) => ("to_upper", true),
        ("lower" | "lowercase" | "to_lowercase" | "to_lower_case", 0) => ("to_lower", true),
        ("startswith", 1) => ("starts_with", true),
        ("endswith", 1) => ("ends_with", true),
        // JavaScript's `includes`, Rust's and Java's `contains` already match.
        ("includes", 1) => ("contains", true),
        // JavaScript's `replaceAll` and Rust's `replace` replace every occurrence, as
        // Mzizi's does; the fix writes the label.
        ("replace_all", 2) => ("replace", true),
        // Each of these is a method §10 designs and this compiler does not build yet, and
        // none means quite what Mzizi's does: JavaScript's `substring` clamps and swaps its
        // ends, `indexOf` answers `-1`. A guess, which names the form to write.
        ("substring" | "substr", _) => ("slice", false),
        ("index_of" | "index", _) => ("find", false),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_counts_unicode_scalar_values() {
        assert_eq!(mz_text_length(""), 0);
        assert_eq!(mz_text_length("héllo"), 5);
        assert_eq!("héllo".len(), 6, "bytes, which Mzizi does not count");
        assert_eq!(mz_text_length("日本語"), 3);
        // One emoji is one scalar value; a flag is two, since §10 counts scalars, not
        // grapheme clusters.
        assert_eq!(mz_text_length("🙂"), 1);
        assert_eq!(mz_text_length("🇰🇪"), 2);
    }

    #[test]
    fn repeat_traps_on_a_negative_count_and_a_length_that_cannot_fit() {
        assert_eq!(mz_text_repeat("ab", 3), Ok("ababab".to_string()));
        assert_eq!(mz_text_repeat("ab", 0), Ok(String::new()));
        assert_eq!(mz_text_repeat("", i64::MAX), Ok(String::new()));
        assert_eq!(mz_text_repeat("ab", -1), Err("negative repeat count"));
        assert_eq!(mz_text_repeat("ab", i64::MAX), Err("text too long"));
        assert_eq!(mz_text_repeat("", -1), Err("negative repeat count"));
    }

    #[test]
    fn the_method_table_is_rfc_0013_section_10() {
        assert_eq!(method(Ty::Text, "length"), Some((vec![], Ty::Int)));
        assert_eq!(method(Ty::Int, "length"), None);
        assert_eq!(method(Ty::Text, "split"), None);
        assert_eq!(waiting("split"), Some("list(text)"));
        let mut sorted = METHODS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, METHODS);
        assert!(METHODS.iter().all(|m| method(Ty::Text, m).is_some()));
        assert!(WAITING.iter().all(|(m, _)| method(Ty::Text, m).is_none()));
        assert_eq!(
            idiom("len", 0),
            Some(("length", false)),
            "Rust counts bytes"
        );
        assert_eq!(idiom("strip", 0), Some(("trim", false)));
        assert_eq!(idiom("strip", 1), Some(("trim", false)));
    }
}
