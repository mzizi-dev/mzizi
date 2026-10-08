//! Text in a program (RFC-0013 §10, tracker row C6): the text methods and their types, the
//! spellings other languages give them, and the helpers the lowering emits.
//!
//! Lengths, indices and slices count Unicode scalar values, as §10 chooses. The methods that
//! return an option (`slice`, `find`, `parse_int`, `parse_float`, and indexing `s[i]`) or a
//! list (`split`, `chars`) use C7's options and lists.
//!
//! The helpers live in `text/ops.rs`, which is compiled into `mz` (where its unit tests
//! run) and emitted verbatim into every lowered `main.rs`, as `numbers/float_text.rs` is.

use crate::expr::Ty;

mod ops;

pub use ops::{
    mz_text_chars, mz_text_find, mz_text_index, mz_text_length, mz_text_parse_float,
    mz_text_parse_int, mz_text_repeat, mz_text_slice, mz_text_split,
};

/// The source of `ops.rs`, for the lowering to emit whole (RFC-0013 §14.2).
pub const OPS_RUNTIME: &str = include_str!("text/ops.rs");

/// Every text method a program can call (RFC-0013 §10), in alphabetical order.
pub const METHODS: &[&str] = &[
    "chars",
    "contains",
    "ends_with",
    "find",
    "length",
    "parse_float",
    "parse_int",
    "repeat",
    "replace",
    "slice",
    "split",
    "starts_with",
    "to_lower",
    "to_upper",
    "trim",
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
        "slice" => (vec![Ty::Int, Ty::Int], Ty::option(Ty::Text)),
        "find" => (vec![Ty::Text], Ty::option(Ty::Int)),
        "split" => (vec![Ty::Text], Ty::list(Ty::Text)),
        "chars" => (vec![], Ty::list(Ty::Text)),
        "parse_int" => (vec![], Ty::option(Ty::Int)),
        "parse_float" => (vec![], Ty::option(Ty::Float)),
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

/// Every name §10 gives a text method, for the nearest-name fix.
pub fn designed() -> impl Iterator<Item = &'static str> {
    METHODS.iter().copied()
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
        // None of these means quite what Mzizi's does: JavaScript's `substring` clamps and
        // swaps its ends, `indexOf` answers `-1` where Mzizi answers `none`, and Java's
        // `toCharArray` is an array of UTF-16 units, not of scalar values. A guess.
        ("substring" | "substr", _) => ("slice", false),
        ("index_of" | "index", _) => ("find", false),
        ("to_char_array", 0) => ("chars", false),
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
    fn index_and_slice_count_scalar_values_and_never_cut_one() {
        assert_eq!(mz_text_index("héllo", 1), Some("é".to_string()));
        assert_eq!(mz_text_index("日本語", 2), Some("語".to_string()));
        assert_eq!(mz_text_index("🙂a", 0), Some("🙂".to_string()));
        assert_eq!(mz_text_index("héllo", 5), None, "one past the end");
        assert_eq!(mz_text_index("héllo", -1), None);
        assert_eq!(mz_text_index("", 0), None);
        assert_eq!(mz_text_slice("héllo", 1, 3), Some("él".to_string()));
        assert_eq!(mz_text_slice("日本語", 0, 3), Some("日本語".to_string()));
        assert_eq!(mz_text_slice("abc", 3, 3), Some(String::new()));
        assert_eq!(mz_text_slice("abc", 2, 1), None, "a past b");
        assert_eq!(mz_text_slice("abc", -1, 2), None);
        assert_eq!(mz_text_slice("abc", 0, 4), None, "b past the end");
        assert_eq!(mz_text_slice("abc", 0, -1), None);
    }

    #[test]
    fn find_gives_a_scalar_value_index_and_zero_for_empty() {
        assert_eq!(mz_text_find("héllo", "llo"), Some(2));
        assert_eq!(mz_text_find("日本語", "語"), Some(2));
        assert_eq!(
            mz_text_find("a🙂b🙂", "🙂"),
            Some(1),
            "the first, not the last"
        );
        assert_eq!(mz_text_find("abc", ""), Some(0));
        assert_eq!(mz_text_find("", ""), Some(0));
        assert_eq!(mz_text_find("abc", "d"), None);
    }

    #[test]
    fn chars_is_one_text_per_scalar_value() {
        assert_eq!(mz_text_chars("hé🙂"), ["h", "é", "🙂"]);
        assert!(mz_text_chars("").is_empty());
    }

    #[test]
    fn split_keeps_empty_pieces_and_traps_on_an_empty_separator() {
        assert_eq!(
            mz_text_split("a,,b,", ","),
            Ok(vec!["a".into(), String::new(), "b".into(), String::new()])
        );
        assert_eq!(mz_text_split("", ","), Ok(vec![String::new()]));
        assert_eq!(
            mz_text_split("日本語", "本"),
            Ok(vec!["日".into(), "語".into()])
        );
        assert_eq!(mz_text_split("ab", ""), Err("empty separator"));
    }

    #[test]
    fn parse_int_is_the_query_parameter_rule() {
        assert_eq!(mz_text_parse_int("42"), Some(42));
        assert_eq!(mz_text_parse_int("-7"), Some(-7));
        assert_eq!(mz_text_parse_int("-0"), Some(0));
        assert_eq!(
            mz_text_parse_int("9223372036854775807"),
            Some(i64::MAX),
            "the largest int"
        );
        assert_eq!(
            mz_text_parse_int("9223372036854775808"),
            None,
            "one past it"
        );
        assert_eq!(mz_text_parse_int("-9223372036854775808"), Some(i64::MIN));
        for not_an_int in [
            "0x10", "1e2", " 7 ", "1.5", "+1", "", "-", "--1", "٣", "1_000",
        ] {
            assert_eq!(mz_text_parse_int(not_an_int), None, "{not_an_int:?}");
        }
    }

    #[test]
    fn parse_int_agrees_with_the_query_decoder_on_every_input_tried() {
        use crate::resolve::Ty as Rt;
        use crate::serve::{Val, decode_query};
        let inputs = [
            "42",
            "-7",
            "-0",
            "0",
            "+1",
            "",
            "-",
            "--1",
            "0x10",
            "1e2",
            " 7 ",
            "1.5",
            "٣",
            "1_000",
            "9223372036854775807",
            "9223372036854775808",
            "-9223372036854775808",
            "-9223372036854775809",
            "007",
            "héllo",
        ];
        for v in inputs {
            let query = match decode_query(&Rt::Int, v) {
                Val::Int(n) => Some(n),
                _ => None,
            };
            assert_eq!(mz_text_parse_int(v), query, "{v:?}");
        }
    }

    #[test]
    fn parse_float_has_no_exponent_no_plus_and_no_bare_point() {
        assert_eq!(mz_text_parse_float("1.5"), Some(1.5));
        assert_eq!(mz_text_parse_float("-2.25"), Some(-2.25));
        assert_eq!(mz_text_parse_float("3"), Some(3.0));
        assert_eq!(mz_text_parse_float("0.0"), Some(0.0));
        for not_a_float in [
            "1.", ".5", "-.5", "-", "", ".", "+1.0", "1e2", "1E2", "inf", "-inf", "NaN", "1.5.2",
            " 1.5", "1.5 ", "٣.5", "0x1p3", "1,5",
        ] {
            assert_eq!(mz_text_parse_float(not_a_float), None, "{not_a_float:?}");
        }
        let overflows = format!("1{}", "0".repeat(400));
        assert_eq!(
            mz_text_parse_float(&overflows),
            None,
            "infinite is not a float here"
        );
    }

    #[test]
    fn the_method_table_is_rfc_0013_section_10() {
        assert_eq!(method(Ty::Text, "length"), Some((vec![], Ty::Int)));
        assert_eq!(method(Ty::Int, "length"), None);
        assert_eq!(
            method(Ty::Text, "slice"),
            Some((vec![Ty::Int, Ty::Int], Ty::option(Ty::Text)))
        );
        assert_eq!(
            method(Ty::Text, "split"),
            Some((vec![Ty::Text], Ty::list(Ty::Text)))
        );
        assert_eq!(method(Ty::Text, "nope"), None);
        let mut sorted = METHODS.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, METHODS);
        assert!(METHODS.iter().all(|m| method(Ty::Text, m).is_some()));
        assert_eq!(
            idiom("len", 0),
            Some(("length", false)),
            "Rust counts bytes"
        );
        assert_eq!(idiom("strip", 0), Some(("trim", false)));
        assert_eq!(idiom("strip", 1), Some(("trim", false)));
        assert_eq!(idiom("index_of", 1), Some(("find", false)));
        assert_eq!(idiom("to_char_array", 0), Some(("chars", false)));
    }
}
