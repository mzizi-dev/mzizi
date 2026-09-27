//! Harness-side reference comparison — the other half of CHARTER.md §6's defect metric
//! (RFC-0006 §10.1).
//!
//! `mz contract` (`compiler/src/contract.rs`) checks a component against its own
//! declarations: a self-consistency check. An agent that writes both a component and its
//! contract can satisfy that check while still diverging from the hand-written Rust the
//! component is meant to port. This crate does the comparison RFC-0006 left to the harness.
//!
//! Two extractors feed one scorer:
//!
//! - [`parse_rust_enums`] reads a Rust file — the hand-written reference, and also a Dioxus
//!   candidate, so both arms of the benchmark are scored against the reference by the same
//!   code. It finds `enum <Type> { ... }` declarations (variant set, `#[default]` variant)
//!   and, inside `impl <Type> { ... }` blocks, the match arms of `fn classes(`, and only
//!   that method, as variant → string literal.
//! - [`parse_mzizi_enums`] reads a `.mz` file: every variant of every `enum ... end` block,
//!   with its `class` and `height` columns, and each enum's default from a
//!   `prop <x>: <enum> = <variant>` line.
//!
//! A Rust type maps to a Mzizi enum by snake-casing its name (`ButtonSize` ↔ `button_size`),
//! and variants the same way (`IconSm` ↔ `icon_sm`).
//!
//! The conversion from a Tailwind class to a pixel height is the one, bounded, well-known
//! linear spacing scale — `value-in-rem = N * 0.25rem`, and browsers default `1rem = 16px`,
//! so `h-N` and `size-N` are both `N * 4` pixels. This is deliberately not a general
//! Tailwind resolver (arbitrary values like `h-[56px]` are `mz contract`'s job — RFC-0006 §5)
//! and the Rust reader is deliberately not a general Rust parser (RFC-0006 §10.1): it blanks
//! comments and string contents so braces can be matched, and then looks for a handful of
//! fixed shapes. What it does not recognise, it does not read — and a reference fact the
//! candidate cannot be shown to satisfy is a defect, never a silent pass (RFC-0006 FM-12).

#![deny(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ---------------------------------------------------------------------------------------
// The shared model both extractors produce
// ---------------------------------------------------------------------------------------

/// One enum, as either extractor sees it, with every name normalised to snake_case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnumModel {
    /// The enum's snake_case name (`button_size`). A Rust `ButtonSize` is snake-cased; a
    /// Mzizi name is taken as written.
    pub name: String,
    /// Every variant, snake_case, in source order.
    pub variants: Vec<String>,
    /// The default variant: Rust's `#[default]` (or a `impl Default` whose `fn default` body
    /// is a single `Self::Variant` path), or Mzizi's `prop <x>: <enum> = <variant>`.
    pub default: Option<String>,
    /// Variant → class string. For Rust, read only from the arms of `fn classes(` whose
    /// body is a single string literal; for Mzizi, the variant row's `class` column.
    pub classes: BTreeMap<String, String>,
    /// Variant → pixel height. For Rust, derived from the class string's first `h-N` /
    /// `size-N` token; for Mzizi, the row's declared `height` column. The scorer compares
    /// the reference's derived heights against whichever of these the candidate carries.
    pub heights: BTreeMap<String, u32>,
}

impl EnumModel {
    fn has_variant(&self, v: &str) -> bool {
        self.variants.iter().any(|x| x == v)
    }
}

/// `ButtonSize` → `button_size`, `IconSm` → `icon_sm`, `HTTPServer` → `http_server`.
///
/// An underscore goes before an uppercase letter that follows a lowercase letter or digit,
/// or that starts a new word after an acronym (an uppercase letter followed by lowercase,
/// preceded by uppercase). A name already in snake_case comes back unchanged.
pub fn to_snake_case(ident: &str) -> String {
    let chars: Vec<char> = ident.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1).copied();
            let boundary = match prev {
                None | Some('_') => false,
                Some(p) if p.is_lowercase() || p.is_ascii_digit() => true,
                Some(p) if p.is_uppercase() => next.is_some_and(|n| n.is_lowercase()),
                _ => false,
            };
            if boundary {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The Tailwind linear spacing scale, and nothing past it: the first `h-N` / `size-N` token
/// with an integer `N` → (`token`, `N * 4` px). Fractional, bracketed and prefixed forms
/// (`h-1.5`, `h-[56px]`, `md:h-10`) are not read.
pub fn tailwind_height(class: &str) -> Option<(String, u32)> {
    class.split_whitespace().find_map(|token| {
        let digits = token
            .strip_prefix("h-")
            .or_else(|| token.strip_prefix("size-"))?;
        let n: u32 = digits.parse().ok()?;
        Some((token.to_string(), n * 4))
    })
}

// ---------------------------------------------------------------------------------------
// The Rust extractor
// ---------------------------------------------------------------------------------------

/// A string literal found by [`lex`]: its byte span in the source (quotes and any `r#`
/// prefix included) and its unescaped value.
struct Lit {
    start: usize,
    end: usize,
    value: String,
}

/// The source with every comment blanked to spaces (`code`), the same again with every
/// string and char literal's contents blanked to `_` (`mask`), and the literals themselves.
/// `code` and `mask` have the source's byte offsets, so structure is found in `mask` —
/// where no brace, `=>` or keyword can hide inside a string or comment — and read in `code`.
struct Lexed {
    code: Vec<u8>,
    mask: Vec<u8>,
    lits: Vec<Lit>,
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn lex(src: &str) -> Lexed {
    let b = src.as_bytes();
    let n = b.len();
    let mut code = b.to_vec();
    let mut mask = b.to_vec();
    let mut lits = Vec::new();
    let blank = |code: &mut Vec<u8>, mask: &mut Vec<u8>, from: usize, to: usize| {
        for k in from..to {
            if b[k] != b'\n' {
                code[k] = b' ';
                mask[k] = b' ';
            }
        }
    };
    let mut i = 0;
    while i < n {
        let prev_ident = i > 0 && is_ident_byte(b[i - 1]);
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            let end = b[i..].iter().position(|&c| c == b'\n').map_or(n, |p| i + p);
            blank(&mut code, &mut mask, i, end);
            i = end;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut depth = 0usize;
            let mut j = i;
            while j < n {
                if b[j] == b'/' && b.get(j + 1) == Some(&b'*') {
                    depth += 1;
                    j += 2;
                } else if b[j] == b'*' && b.get(j + 1) == Some(&b'/') {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            blank(&mut code, &mut mask, i, j.min(n));
            i = j;
        } else if !prev_ident && let Some((hashes, open)) = raw_string_start(b, i) {
            // r"..." / r#"..."# / br"...": contents verbatim, ended by `"` + the same hashes.
            let content_start = open + 1;
            let mut j = content_start;
            let close = loop {
                if j >= n {
                    break n;
                }
                if b[j] == b'"'
                    && b[j + 1..]
                        .iter()
                        .take(hashes)
                        .filter(|&&c| c == b'#')
                        .count()
                        == hashes
                {
                    break j;
                }
                j += 1;
            };
            let end = (close + 1 + hashes).min(n);
            for m in &mut mask[content_start..close] {
                *m = b'_';
            }
            lits.push(Lit {
                start: i,
                end,
                value: src[content_start..close].to_string(),
            });
            i = end;
        } else if b[i] == b'"' {
            let start = if i > 0 && b[i - 1] == b'b' && !(i > 1 && is_ident_byte(b[i - 2])) {
                i - 1
            } else {
                i
            };
            let mut j = i + 1;
            while j < n && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            let close = j.min(n);
            for m in &mut mask[i + 1..close] {
                *m = b'_';
            }
            lits.push(Lit {
                start,
                end: (close + 1).min(n),
                value: unescape(&src[i + 1..close]),
            });
            i = close + 1;
        } else if b[i] == b'\'' {
            // A char literal (`'"'`, `'\''`, `'{'`) is masked so it cannot open a string or a
            // brace; a lifetime (`'static`) has no closing quote where one would be and is
            // left alone.
            let end = if b.get(i + 1) == Some(&b'\\') {
                b[i + 2..]
                    .iter()
                    .take(10)
                    .position(|&c| c == b'\'')
                    .map(|p| i + 2 + p + 1)
            } else {
                src[i + 1..].chars().next().and_then(|c| {
                    let after = i + 1 + c.len_utf8();
                    (b.get(after) == Some(&b'\'')).then_some(after + 1)
                })
            };
            match end {
                Some(end) => {
                    for m in &mut mask[i + 1..end - 1] {
                        *m = b'_';
                    }
                    i = end;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    Lexed { code, mask, lits }
}

/// At `i`, `r"`, `r#"`, `br"`, … → (`hash count`, index of the opening `"`).
fn raw_string_start(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if b.get(j) == Some(&b'b') {
        j += 1;
    }
    if b.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while b.get(j) == Some(&b'#') {
        hashes += 1;
        j += 1;
    }
    (b.get(j) == Some(&b'"')).then_some((hashes, j))
}

/// The escapes a class string can plausibly contain, including a `\`-newline continuation,
/// which drops the newline and the next line's leading whitespace.
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('\n') => {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn skip_ws(mask: &[u8], mut i: usize) -> usize {
    while i < mask.len() && mask[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Every index in `from..to` where `kw` appears as a whole word.
fn keyword_positions(mask: &[u8], from: usize, to: usize, kw: &str) -> Vec<usize> {
    let kw = kw.as_bytes();
    let mut out = Vec::new();
    let mut i = from;
    while i + kw.len() <= to {
        if &mask[i..i + kw.len()] == kw
            && (i == 0 || !is_ident_byte(mask[i - 1]))
            && mask.get(i + kw.len()).is_none_or(|&c| !is_ident_byte(c))
        {
            out.push(i);
            i += kw.len();
        } else {
            i += 1;
        }
    }
    out
}

fn read_ident(mask: &[u8], i: usize) -> Option<(String, usize)> {
    let start = i;
    let mut j = i;
    while j < mask.len() && is_ident_byte(mask[j]) {
        j += 1;
    }
    (j > start && !mask[start].is_ascii_digit())
        .then(|| (String::from_utf8_lossy(&mask[start..j]).into_owned(), j))
}

/// Given the index of an opening `{`, `(` or `[`, the index of its matching closer. All
/// three kinds share one depth counter, which is enough once strings and comments are masked.
fn matching(mask: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, &c) in mask.iter().enumerate().skip(open) {
        match c {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// Given the index of an opening `<`, the index just past its matching `>` (an `->` does not
/// count as a closer).
fn skip_generics(mask: &[u8], open: usize) -> usize {
    let mut depth = 0usize;
    let mut k = open;
    while k < mask.len() {
        match mask[k] {
            b'<' => depth += 1,
            b'>' if k == 0 || mask[k - 1] != b'-' => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            b'{' | b';' => return k,
            _ => {}
        }
        k += 1;
    }
    k
}

/// The first index in `from..to` holding `target` at bracket depth 0.
fn find_at_depth0(mask: &[u8], from: usize, to: usize, target: &[u8]) -> Option<usize> {
    let mut depth = 0isize;
    let mut k = from;
    while k < to {
        if depth == 0 && mask[k..].starts_with(target) {
            return Some(k);
        }
        match mask[k] {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            _ => {}
        }
        k += 1;
    }
    None
}

/// The trimmed `start..end` of a byte range.
fn trim_range(mask: &[u8], mut start: usize, mut end: usize) -> (usize, usize) {
    while start < end && mask[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && mask[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    (start, end)
}

/// `Self::IconSm`, `ButtonSize::IconSm`, `IconSm`, `Self::IconSm { .. }` → `IconSm`, if the
/// last path segment starts with an uppercase letter. `_`, bindings and literals → `None`.
fn pattern_variant(pat: &str) -> Option<String> {
    let last = pat.trim().rsplit("::").next()?.trim();
    let ident: String = last
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    ident
        .starts_with(|c: char| c.is_ascii_uppercase())
        .then_some(ident)
}

/// One `enum` declaration: Rust name, variants (as written), `#[default]` variant.
struct RsEnumDecl {
    rust_name: String,
    variants: Vec<String>,
    default: Option<String>,
}

fn parse_enum_decls(lx: &Lexed) -> Vec<RsEnumDecl> {
    let m = &lx.mask;
    let mut out = Vec::new();
    for pos in keyword_positions(m, 0, m.len(), "enum") {
        let i = skip_ws(m, pos + 4);
        let Some((rust_name, mut j)) = read_ident(m, i) else {
            continue;
        };
        j = skip_ws(m, j);
        if m.get(j) == Some(&b'<') {
            j = skip_generics(m, j);
        }
        // Skip a `where` clause, if any, up to the body.
        let Some(open) = (j..m.len()).find(|&k| m[k] == b'{' || m[k] == b';') else {
            continue;
        };
        if m[open] != b'{' {
            continue;
        }
        let Some(close) = matching(m, open) else {
            continue;
        };
        let mut variants = Vec::new();
        let mut default = None;
        let mut k = open + 1;
        while k < close {
            let piece_end = find_at_depth0(m, k, close, b",").unwrap_or(close);
            let mut p = skip_ws(m, k);
            let mut is_default = false;
            while p < piece_end && m[p] == b'#' {
                let q = skip_ws(m, p + 1);
                if m.get(q) != Some(&b'[') {
                    break;
                }
                let Some(qc) = matching(m, q) else { break };
                let attr = String::from_utf8_lossy(&lx.code[q + 1..qc]);
                if attr.trim() == "default" {
                    is_default = true;
                }
                p = skip_ws(m, qc + 1);
            }
            if let Some((v, _)) = read_ident(m, p).filter(|_| p < piece_end) {
                if is_default {
                    default = Some(v.clone());
                }
                variants.push(v);
            }
            k = piece_end + 1;
        }
        out.push(RsEnumDecl {
            rust_name,
            variants,
            default,
        });
    }
    out
}

/// What an `impl` block contributes to one type: its `classes()` arms, and — for an
/// `impl Default for T` — the variant `fn default` returns.
#[derive(Default)]
struct RsImplFacts {
    classes: Vec<(String, String)>,
    default: Option<String>,
}

fn parse_impls(lx: &Lexed) -> BTreeMap<String, RsImplFacts> {
    let m = &lx.mask;
    let mut out: BTreeMap<String, RsImplFacts> = BTreeMap::new();
    for pos in keyword_positions(m, 0, m.len(), "impl") {
        let mut j = skip_ws(m, pos + 4);
        if m.get(j) == Some(&b'<') {
            j = skip_generics(m, j);
        }
        let Some(open) = (j..m.len()).find(|&k| m[k] == b'{' || m[k] == b';') else {
            continue;
        };
        if m[open] != b'{' {
            continue;
        }
        let Some(close) = matching(m, open) else {
            continue;
        };
        let header = String::from_utf8_lossy(&m[j..open]).into_owned();
        let header = header
            .split(" where ")
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let (trait_part, self_part) = match header.split_once(" for ") {
            Some((t, s)) => (Some(t.trim().to_string()), s.trim().to_string()),
            None => (None, header.clone()),
        };
        let self_ty = last_segment(&self_part);
        if self_ty.is_empty() {
            continue;
        }
        let entry = out.entry(self_ty).or_default();

        if let Some(t) = trait_part {
            if last_segment(&t) == "Default"
                && entry.default.is_none()
                && let Some((bo, bc)) = fn_body(m, open + 1, close, "default")
            {
                let (s, e) = trim_range(m, bo + 1, bc);
                let body = String::from_utf8_lossy(&lx.code[s..e]).into_owned();
                if body.contains("::") && !body.contains(char::is_whitespace) {
                    entry.default = pattern_variant(&body);
                }
            }
            continue;
        }

        if !entry.classes.is_empty() {
            continue;
        }
        if let Some((bo, bc)) = fn_body(m, open + 1, close, "classes") {
            entry.classes = classes_arms(lx, bo, bc);
        }
    }
    out
}

/// `Foo<T>` / `crate::a::Foo` → `Foo`.
fn last_segment(path: &str) -> String {
    let no_generics = path.split('<').next().unwrap_or("");
    no_generics
        .rsplit("::")
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('&')
        .to_string()
}

/// Inside `from..to`, the `{`..`}` body of the first `fn <name>` — its open and close index.
fn fn_body(m: &[u8], from: usize, to: usize, name: &str) -> Option<(usize, usize)> {
    for pos in keyword_positions(m, from, to, "fn") {
        let i = skip_ws(m, pos + 2);
        let Some((ident, j)) = read_ident(m, i) else {
            continue;
        };
        if ident != name {
            continue;
        }
        let paren = (j..to).find(|&k| m[k] == b'(')?;
        let paren_close = matching(m, paren)?;
        let open = (paren_close..to).find(|&k| m[k] == b'{' || m[k] == b';')?;
        if m[open] != b'{' {
            return None;
        }
        let close = matching(m, open)?;
        return Some((open, close));
    }
    None
}

/// The arms of the first `match` in a method body, as (Rust variant, class literal) pairs.
/// An arm counts only if its body — bare, or a `{ ... }` block — is exactly one string
/// literal. Or-patterns (`A | B => "..."`) give the literal to each variant; guarded arms
/// and wildcards give it to none.
fn classes_arms(lx: &Lexed, body_open: usize, body_close: usize) -> Vec<(String, String)> {
    let m = &lx.mask;
    let mut out = Vec::new();
    let Some(&mpos) = keyword_positions(m, body_open, body_close, "match").first() else {
        return out;
    };
    let Some(open) = (mpos..body_close).find(|&k| m[k] == b'{') else {
        return out;
    };
    let Some(close) = matching(m, open) else {
        return out;
    };
    let mut i = open + 1;
    loop {
        i = skip_ws(m, i);
        if i >= close {
            break;
        }
        let Some(arrow) = find_at_depth0(m, i, close, b"=>") else {
            break;
        };
        let pattern = String::from_utf8_lossy(&lx.code[i..arrow]).into_owned();
        let k = skip_ws(m, arrow + 2);
        let (body_s, body_e, next) = if m.get(k) == Some(&b'{') {
            let Some(bc) = matching(m, k) else { break };
            let mut next = skip_ws(m, bc + 1);
            if m.get(next) == Some(&b',') {
                next += 1;
            }
            (k + 1, bc, next)
        } else {
            let comma = find_at_depth0(m, k, close, b",").unwrap_or(close);
            (k, comma, comma + 1)
        };
        let (s, e) = trim_range(m, body_s, body_e);
        if let Some(lit) = lx.lits.iter().find(|l| l.start == s && l.end == e) {
            for alt in pattern.split('|') {
                if alt.contains(" if ") {
                    continue;
                }
                if let Some(v) = pattern_variant(alt) {
                    out.push((v, lit.value.clone()));
                }
            }
        }
        i = next;
    }
    out
}

/// Read every `enum` declared in a Rust file, with its `#[default]` variant and the class
/// strings of its `classes()` method, as [`EnumModel`]s.
///
/// Used on the hand-written reference, and on a Dioxus candidate — the same code both sides,
/// so the Dioxus arm is not scored by a different yardstick than the reference is read with.
///
/// Recognised shapes, and nothing else:
/// - `enum Name { A, #[default] B, C(..), D { .. } }`, with any attributes and doc comments.
/// - `impl Name { ... fn classes(...) ... { match ... { <arms> } } ... }`, where each arm is
///   `<pattern> => "<literal>"` or `<pattern> => { "<literal>" }`. Other methods (`slug()`,
///   …) and other arms (a `format!`, a `const`, a call) are not class strings and are not read.
/// - `impl Default for Name { fn default() -> Self { Self::B } }`, as an alternative to
///   `#[default]`.
pub fn parse_rust_enums(src: &str) -> Vec<EnumModel> {
    let lx = lex(src);
    let decls = parse_enum_decls(&lx);
    let impls = parse_impls(&lx);
    decls
        .into_iter()
        .map(|d| {
            let facts = impls.get(&d.rust_name);
            let mut classes = BTreeMap::new();
            if let Some(f) = facts {
                for (v, c) in &f.classes {
                    classes.entry(to_snake_case(v)).or_insert_with(|| c.clone());
                }
            }
            let heights = classes
                .iter()
                .filter_map(|(v, c)| tailwind_height(c).map(|(_, h)| (v.clone(), h)))
                .collect();
            let default = d
                .default
                .or_else(|| facts.and_then(|f| f.default.clone()))
                .map(|v| to_snake_case(&v));
            EnumModel {
                name: to_snake_case(&d.rust_name),
                variants: d.variants.iter().map(|v| to_snake_case(v)).collect(),
                default,
                classes,
                heights,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// The Mzizi extractor
// ---------------------------------------------------------------------------------------

/// Read every `enum <name> ... end` block of a `.mz` file as an [`EnumModel`]: every variant
/// row (not only height-bearing ones), its `class` and `height` columns where present, and
/// the enum's default from the first `prop <x>: <name> = <variant>` line.
pub fn parse_mzizi_enums(src: &str) -> Vec<EnumModel> {
    let mut enums: Vec<EnumModel> = Vec::new();
    let mut lines = src.lines();
    while let Some(line) = lines.next() {
        let Some(name) = enum_header(line) else {
            continue;
        };
        let mut e = EnumModel {
            name,
            ..EnumModel::default()
        };
        for body_line in lines.by_ref() {
            if body_line.trim() == "end" {
                break;
            }
            if let Some(row) = parse_variant_row(body_line) {
                if let Some(c) = row.class {
                    e.classes.insert(row.name.clone(), c);
                }
                if let Some(h) = row.height {
                    e.heights.insert(row.name.clone(), h);
                }
                e.variants.push(row.name);
            }
        }
        enums.push(e);
    }
    for line in src.lines() {
        if let Some((ty, value)) = prop_default(line)
            && let Some(e) = enums.iter_mut().find(|e| e.name == ty)
            && e.default.is_none()
        {
            e.default = Some(value);
        }
    }
    enums
}

/// `  prop size: button_size = default` → `("button_size", "default")`.
fn prop_default(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix("prop ")?;
    let (_, ty_and_value) = rest.split_once(':')?;
    let (ty, value) = ty_and_value.split_once('=')?;
    let ty = ty.trim();
    let value = value.split_whitespace().next()?;
    (is_identifier(ty) && is_identifier(value)).then(|| (ty.to_string(), value.to_string()))
}

/// `  enum button_size` → `Some("button_size")`. Anything else is `None`.
fn enum_header(line: &str) -> Option<String> {
    let name = line.trim().strip_prefix("enum ")?.trim();
    (!name.is_empty() && is_identifier(name)).then(|| name.to_string())
}

struct MzRow {
    name: String,
    class: Option<String>,
    height: Option<u32>,
}

/// A variant row: an identifier, then any columns — `class "..."` and `height <N>` are the
/// two read here. A `##` comment line is not a row.
fn parse_variant_row(line: &str) -> Option<MzRow> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("##") {
        return None;
    }
    let tokens = tokenize(line);
    let name = tokens.first()?.clone();
    if !is_identifier(&name) {
        return None;
    }
    let mut row = MzRow {
        name,
        class: None,
        height: None,
    };
    let mut i = 1;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "##" => break,
            "class" if i + 1 < tokens.len() => {
                row.class = Some(tokens[i + 1].clone());
                i += 2;
            }
            "height" if i + 1 < tokens.len() => {
                row.height = tokens[i + 1].parse::<u32>().ok();
                i += 2;
            }
            _ => i += 1,
        }
    }
    Some(row)
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

// ---------------------------------------------------------------------------------------
// `diff`: the height-only report, kept for the `diff` subcommand
// ---------------------------------------------------------------------------------------

/// One variant of a Mzizi `enum` that declares both a `class` string and a `height`.
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
    /// Its variants that declare both `class` and `height`, in source order.
    pub variants: Vec<MzVariant>,
}

/// The height-bearing enums of a `.mz` file — the input the `diff` subcommand reports on.
/// An enum with no variant carrying both `class` and `height` produces no entry.
pub fn parse_mzizi_size_enums(src: &str) -> Vec<MzSizeEnum> {
    parse_mzizi_enums(src)
        .into_iter()
        .filter_map(|e| {
            let variants: Vec<MzVariant> = e
                .variants
                .iter()
                .filter_map(|v| {
                    Some(MzVariant {
                        name: v.clone(),
                        class: e.classes.get(v)?.clone(),
                        height: *e.heights.get(v)?,
                    })
                })
                .collect();
            (!variants.is_empty()).then_some(MzSizeEnum {
                name: e.name,
                variants,
            })
        })
        .collect()
}

/// One reference variant's class string, as the `diff` report consumes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RsVariant {
    /// The variant name, snake_case.
    pub name: String,
    /// The class string from the reference's `classes()` arm, verbatim.
    pub class: String,
}

/// The `classes()` arms of the reference enum named `enum_name` (snake_case), in the
/// enum's declaration order. Empty if the reference has no such enum, or it has no
/// readable `classes()` — which [`diff_size_enum`] then reports as missing, not as passing.
pub fn reference_class_arms(reference: &[EnumModel], enum_name: &str) -> Vec<RsVariant> {
    reference
        .iter()
        .find(|e| e.name == enum_name)
        .map(|e| {
            e.variants
                .iter()
                .filter_map(|v| {
                    Some(RsVariant {
                        name: v.clone(),
                        class: e.classes.get(v)?.clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
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

/// Every comparison produced for one `.mz` size enum against one reference enum's arms.
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

/// Diff one `.mz` size enum against the class arms of the same-named reference enum
/// (from [`reference_class_arms`]).
///
/// Every variant the `.mz` enum declares gets a result — `Pass`, `Mismatch`,
/// `Unevaluable` or `MissingInReference`. A reference variant with a size-scale class that
/// the `.mz` enum never names also gets a result (`MissingInMzizi`): RFC-0006 §10.1 asks that
/// missing variants on either side be reported, not silently skipped.
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
                match tailwind_height(class) {
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
        if tailwind_height(&rv.class).is_some() {
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

// ---------------------------------------------------------------------------------------
// `score`: every fact the reference supports, as one JSON object
// ---------------------------------------------------------------------------------------

/// Which arm of the benchmark a candidate belongs to — which extractor reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arm {
    /// A `.mz` file, read by [`parse_mzizi_enums`]; its heights are its declared `height`s.
    Mzizi,
    /// A Dioxus `.rs` file, read by [`parse_rust_enums`] — the reference's own extractor;
    /// its heights are derived from its own `classes()` strings.
    Dioxus,
}

impl Arm {
    /// `"mzizi"` / `"dioxus"` → the arm.
    pub fn parse(s: &str) -> Option<Arm> {
        match s {
            "mzizi" => Some(Arm::Mzizi),
            "dioxus" => Some(Arm::Dioxus),
            _ => None,
        }
    }

    /// The arm's name as the JSON reports it.
    pub fn as_str(self) -> &'static str {
        match self {
            Arm::Mzizi => "mzizi",
            Arm::Dioxus => "dioxus",
        }
    }

    /// Read a candidate source with this arm's extractor.
    pub fn extract(self, src: &str) -> Vec<EnumModel> {
        match self {
            Arm::Mzizi => parse_mzizi_enums(src),
            Arm::Dioxus => parse_rust_enums(src),
        }
    }
}

/// The kind of fact checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactKind {
    /// The enum's variant set equals the reference's. One per reference enum.
    VariantSet,
    /// The enum's default equals the reference's. One per reference enum that declares one.
    Default,
    /// A variant's height equals the one derived from the reference's `classes()` string.
    /// One per reference variant whose class string carries an `h-N` / `size-N` token.
    Height,
    /// The candidate named the enum's variants differently from the reference, and they were
    /// paired by class string instead (see [`rename_map`]). Never a defect: it is reported
    /// so the rename is visible in the output, not silent. `actual` lists the pairing as
    /// `candidate -> reference`. One per reference enum that was paired this way.
    VariantNames,
}

impl FactKind {
    /// The fact's name as the JSON reports it.
    pub fn as_str(self) -> &'static str {
        match self {
            FactKind::VariantSet => "variant_set",
            FactKind::Default => "default",
            FactKind::Height => "height",
            FactKind::VariantNames => "variant_names",
        }
    }
}

/// One checked fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    /// The snake_case enum name.
    pub enum_name: String,
    /// The variant, for a `height` fact; `None` for `variant_set` and `default`.
    pub variant: Option<String>,
    /// What was checked.
    pub kind: FactKind,
    /// The reference's value, as text.
    pub expected: String,
    /// The candidate's value, as text.
    pub actual: String,
    /// Whether the candidate disagrees with the reference on this fact.
    pub defect: bool,
}

/// The result of scoring one candidate against one reference.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreReport {
    /// The candidate's arm.
    pub arm: Arm,
    /// Every fact checked, in reference order.
    pub facts: Vec<Fact>,
    /// See [`class_token_jaccard`].
    pub class_token_jaccard: Option<f64>,
    /// How many candidate variants were paired with a differently named reference variant
    /// by [`rename_map`], summed over every enum. `0` when every enum matched by name.
    pub renames: usize,
}

impl ScoreReport {
    /// How many facts are defects.
    pub fn defects(&self) -> usize {
        self.facts.iter().filter(|f| f.defect).count()
    }

    /// The report as the one JSON object the `score` subcommand prints:
    /// `{"arm","facts_checked","defects","renames","details":[{"enum","variant","fact",
    /// "expected","actual","defect"}],"class_token_jaccard"}`.
    pub fn to_json(&self) -> String {
        let mut s = String::new();
        s.push_str("{\"arm\":");
        push_json_str(&mut s, self.arm.as_str());
        s.push_str(&format!(
            ",\"facts_checked\":{},\"defects\":{},\"renames\":{},\"details\":[",
            self.facts.len(),
            self.defects(),
            self.renames
        ));
        for (i, f) in self.facts.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str("{\"enum\":");
            push_json_str(&mut s, &f.enum_name);
            s.push_str(",\"variant\":");
            match &f.variant {
                Some(v) => push_json_str(&mut s, v),
                None => s.push_str("null"),
            }
            s.push_str(",\"fact\":");
            push_json_str(&mut s, f.kind.as_str());
            s.push_str(",\"expected\":");
            push_json_str(&mut s, &f.expected);
            s.push_str(",\"actual\":");
            push_json_str(&mut s, &f.actual);
            s.push_str(&format!(",\"defect\":{}}}", f.defect));
        }
        s.push_str("],\"class_token_jaccard\":");
        match self.class_token_jaccard {
            Some(j) => s.push_str(&format!("{j:.4}")),
            None => s.push_str("null"),
        }
        s.push('}');
        s
    }
}

/// Append `v` as a JSON string literal: `"` and `\` escaped, control characters as
/// `\n` / `\r` / `\t` / `\u00XX`, everything else (including non-ASCII) as UTF-8.
pub fn push_json_str(out: &mut String, v: &str) {
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn set_text(vs: &[String]) -> String {
    format!("{{{}}}", vs.join(", "))
}

/// Pair a candidate enum's variants with the reference's by class string, for a candidate
/// that named them differently. Returns reference variant → candidate variant, or `None`
/// when no pairing is safe — and then the caller matches by name, exactly as it would with
/// no pairing step at all.
///
/// A pairing is accepted only when every one of these holds:
///
/// - the two variant-name sets differ (equal sets need no pairing);
/// - both sides have the same number of distinct variants, and every variant on both sides
///   has a class string;
/// - each class string, normalised to its set of whitespace-separated tokens, is unique on
///   its own side — two variants with one class cannot be told apart, so neither is paired;
/// - every reference token set equals exactly one candidate token set: a complete bijection
///   over both sides, not a best-effort partial match;
/// - a variant name present on both sides pairs with itself. A candidate whose `a` carries
///   the reference's `b` classes has swapped two behaviours, which is a disagreement, not a
///   rename.
///
/// Why this exists: a reference can drift from its own spec. The changelog task's `.tsx`
/// keys its colours by axis (`horizontal`, …) and the registry's Rust reference renamed the
/// same four class strings by mineral (`cobalt`, …); an author who followed the spec was
/// scored two defects for the reference's rename, and no jaccard at all. Pairing by an
/// identical class string — the thing the variant actually renders — scores the behaviour,
/// and the [`FactKind::VariantNames`] fact keeps the rename in the output.
pub fn rename_map(r: &EnumModel, c: &EnumModel) -> Option<BTreeMap<String, String>> {
    let rs: BTreeSet<&str> = r.variants.iter().map(String::as_str).collect();
    let cs: BTreeSet<&str> = c.variants.iter().map(String::as_str).collect();
    if rs == cs
        || rs.len() != r.variants.len()
        || cs.len() != c.variants.len()
        || rs.len() != cs.len()
    {
        return None;
    }
    fn token_sets(m: &EnumModel) -> Option<Vec<(&str, BTreeSet<&str>)>> {
        let mut out: Vec<(&str, BTreeSet<&str>)> = Vec::new();
        for v in &m.variants {
            let set: BTreeSet<&str> = m.classes.get(v)?.split_whitespace().collect();
            if out.iter().any(|(_, seen)| *seen == set) {
                return None;
            }
            out.push((v, set));
        }
        Some(out)
    }
    let rt = token_sets(r)?;
    let ct = token_sets(c)?;
    let mut map = BTreeMap::new();
    for (rv, set) in &rt {
        // Token sets are unique on each side, so at most one candidate matches, and distinct
        // reference variants find distinct candidates; with equal counts, that is a bijection.
        let (cv, _) = ct.iter().find(|(_, cset)| cset == set)?;
        if rv != cv && (cs.contains(rv) || rs.contains(cv)) {
            return None;
        }
        map.insert(rv.to_string(), cv.to_string());
    }
    Some(map)
}

/// Score a candidate's enums against the reference's.
///
/// For each reference enum, in source order:
/// - `variant_set` — a defect if the candidate's set differs; `expected`/`actual` name the
///   variants missing from, and extra in, the candidate. A candidate that lacks the enum
///   entirely fails this fact (and every other fact for that enum) rather than being
///   skipped: an absent enum is the most wrong a variant set can be (RFC-0006 FM-12).
///   When the sets differ but [`rename_map`] pairs every variant by an identical class
///   string, the set is not a defect, and a `variant_names` fact follows it listing the
///   pairing (`candidate -> reference`); every later fact for the enum is then read through
///   that pairing.
/// - `default` — checked only when the reference declares one (a reference with no default
///   has nothing for the candidate to disagree with). A defect if the candidate's differs or
///   is absent.
/// - `height` — for each reference variant whose `classes()` string has an `h-N` / `size-N`
///   token; a defect if the candidate's height for it differs or is missing.
///
/// Candidate enums the reference does not have are not facts and are ignored.
pub fn score(arm: Arm, reference: &[EnumModel], candidate: &[EnumModel]) -> ScoreReport {
    let mut facts = Vec::new();
    let mut renames = 0;
    for r in reference {
        let c = candidate.iter().find(|c| c.name == r.name);
        let paired = c.and_then(|c| rename_map(r, c));
        // The candidate's name for a reference variant: its pair, or the same name.
        let cand_name = |v: &str| -> String {
            paired
                .as_ref()
                .and_then(|m| m.get(v))
                .cloned()
                .unwrap_or_else(|| v.to_string())
        };

        if let (Some(c), Some(map)) = (c, &paired) {
            facts.push(Fact {
                enum_name: r.name.clone(),
                variant: None,
                kind: FactKind::VariantSet,
                expected: set_text(&r.variants),
                actual: format!(
                    "{}; paired with the reference by class string (see variant_names)",
                    set_text(&c.variants)
                ),
                defect: false,
            });
            let pairs: Vec<String> = r
                .variants
                .iter()
                .filter_map(|rv| {
                    let cv = &map[rv];
                    (cv != rv).then(|| format!("{cv} -> {rv}"))
                })
                .collect();
            renames += pairs.len();
            facts.push(Fact {
                enum_name: r.name.clone(),
                variant: None,
                kind: FactKind::VariantNames,
                expected: set_text(&r.variants),
                actual: format!("renamed (candidate -> reference): {}", pairs.join(", ")),
                defect: false,
            });
        } else {
            let expected_missing: Vec<String> = r
                .variants
                .iter()
                .filter(|v| !c.is_some_and(|c| c.has_variant(v)))
                .cloned()
                .collect();
            let extra: Vec<String> = c
                .map(|c| {
                    c.variants
                        .iter()
                        .filter(|v| !r.has_variant(v))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let mut expected = set_text(&r.variants);
            let mut actual = match c {
                Some(c) => set_text(&c.variants),
                None => "enum absent".to_string(),
            };
            if !expected_missing.is_empty() {
                expected.push_str(&format!(
                    "; missing from candidate: {}",
                    set_text(&expected_missing)
                ));
            }
            if !extra.is_empty() {
                actual.push_str(&format!("; extra in candidate: {}", set_text(&extra)));
            }
            facts.push(Fact {
                enum_name: r.name.clone(),
                variant: None,
                kind: FactKind::VariantSet,
                expected,
                actual,
                defect: c.is_none() || !expected_missing.is_empty() || !extra.is_empty(),
            });
        }

        if let Some(rd) = &r.default {
            let cd = c.and_then(|c| c.default.clone());
            // The candidate's default under the reference's name for it.
            let cd_ref = cd.as_ref().map(|d| {
                paired
                    .as_ref()
                    .and_then(|m| m.iter().find(|(_, cv)| *cv == d))
                    .map_or_else(|| d.clone(), |(rv, _)| rv.clone())
            });
            let actual = match (&cd, &cd_ref) {
                (Some(d), Some(dr)) if d != dr => format!("{d} -> {dr}"),
                (Some(d), _) => d.clone(),
                (None, _) => "none".to_string(),
            };
            facts.push(Fact {
                enum_name: r.name.clone(),
                variant: None,
                kind: FactKind::Default,
                expected: rd.clone(),
                actual,
                defect: cd_ref.as_deref() != Some(rd.as_str()),
            });
        }

        for v in &r.variants {
            let Some(want) = r.heights.get(v) else {
                continue;
            };
            let got = c.and_then(|c| c.heights.get(&cand_name(v))).copied();
            facts.push(Fact {
                enum_name: r.name.clone(),
                variant: Some(v.clone()),
                kind: FactKind::Height,
                expected: format!("{want}px"),
                actual: got.map_or_else(|| "missing".to_string(), |g| format!("{g}px")),
                defect: got != Some(*want),
            });
        }
    }
    ScoreReport {
        arm,
        facts,
        class_token_jaccard: class_token_jaccard(reference, candidate),
        renames,
    }
}

/// The mean, over every variant that has a class string on both sides, of the Jaccard
/// similarity `|A ∩ B| / |A ∪ B|` of the two strings' whitespace-split token sets. Variants
/// are matched by enum and variant name, or through [`rename_map`] where that pairs an enum
/// (so a paired enum contributes 1.0 per variant, by construction). `None` when no variant
/// has a class on both sides.
///
/// **Reported, never counted as a defect.** The charter's defect (CHARTER.md §6) is
/// behavioural — code that compiles but disagrees with the reference on what it does — and
/// what the reference's classes determine that this harness can check is touch height,
/// variant completeness and the default, each of which is its own fact above. Exact styling
/// is not that: the corpus's own hand-written `primitives/button.mz` drops the reference's
/// `has-data-[icon=...]` padding tweaks and `aria-expanded:` states from its class strings,
/// and a threshold on this number would call that file defective for being a faithful port
/// of the behaviour it declares. So the number is published beside the defect count, for a
/// reader to weigh, and the defect count does not move with it.
pub fn class_token_jaccard(reference: &[EnumModel], candidate: &[EnumModel]) -> Option<f64> {
    let mut sum = 0.0;
    let mut n = 0usize;
    for r in reference {
        let Some(c) = candidate.iter().find(|c| c.name == r.name) else {
            continue;
        };
        let paired = rename_map(r, c);
        for v in &r.variants {
            let cv = paired
                .as_ref()
                .and_then(|m| m.get(v))
                .map_or(v.as_str(), String::as_str);
            let (Some(rc), Some(cc)) = (r.classes.get(v), c.classes.get(cv)) else {
                continue;
            };
            let a: BTreeSet<&str> = rc.split_whitespace().collect();
            let b: BTreeSet<&str> = cc.split_whitespace().collect();
            let union = a.union(&b).count();
            sum += if union == 0 {
                1.0
            } else {
                a.intersection(&b).count() as f64 / union as f64
            };
            n += 1;
        }
    }
    (n > 0).then(|| sum / n as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTON_MZ: &str = r#"component button

  enum button_variant
    default      class "bg-primary text-primary-foreground hover:bg-primary/80"
    outline      class "border-border"
  end

  ## Sizes carry their own touch height as data.
  enum button_size
    default   class "h-14 gap-2 px-5"     height 56
    sm        class "h-12 gap-1.5 px-4"   height 48
    lg        class "h-14 gap-2 px-6"     height 56
    icon      class "size-14"             height 56
    icon_sm   class "size-12"             height 48
  end

  prop variant: button_variant = default
  prop size: button_size = default

end component button
"#;

    /// A reduced reference in the shape of the registry's `button.rs`: two enums sharing a
    /// `Default` variant, `#[default]` on each, `classes()` and `slug()` in each impl, and
    /// block-bodied arms. The real file is exercised by `tests/button_diff.rs`.
    const BUTTON_RS: &str = r#"
/// Visual treatment. { an unbalanced brace in a doc comment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Solid.
    #[default]
    Default,
    Outline,
}

#[derive(Default)]
pub enum ButtonSize {
    #[default]
    Default,
    Sm,
    Lg,
    Icon,
    IconSm,
}

const BASE: &str = "[&_svg:not([class*='size-'])]:size-4 { h-99";

impl ButtonVariant {
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Default => "bg-primary text-primary-foreground hover:bg-primary/80",
            Self::Outline => {
                "border-border"
            }
        }
    }
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Outline => "outline",
        }
    }
}

impl ButtonSize {
    /// The Tailwind classes for this size. } another stray brace
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Default => {
                "h-14 gap-2 px-5"
            }
            Self::Sm => {
                "h-12 gap-1.5 px-4"
            }
            ButtonSize::Lg => "h-14 gap-2 px-6",
            Self::Icon => "size-14",
            Self::IconSm => "size-12",
        }
    }
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
            Self::Lg => "lg",
            Self::Icon => "icon",
            Self::IconSm => "icon-sm",
        }
    }
}
"#;

    fn find<'a>(enums: &'a [EnumModel], name: &str) -> &'a EnumModel {
        enums
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("no enum {name} in {enums:#?}"))
    }

    #[test]
    fn snake_case_maps_rust_names_to_mzizi_names() {
        assert_eq!(to_snake_case("ButtonSize"), "button_size");
        assert_eq!(to_snake_case("IconSm"), "icon_sm");
        assert_eq!(to_snake_case("Default"), "default");
        assert_eq!(to_snake_case("HTTPServer"), "http_server");
        assert_eq!(to_snake_case("H2"), "h2");
        assert_eq!(to_snake_case("icon_sm"), "icon_sm");
    }

    #[test]
    fn block_bodied_arms_are_read() {
        let enums = parse_rust_enums(BUTTON_RS);
        let size = find(&enums, "button_size");
        assert_eq!(size.classes["default"], "h-14 gap-2 px-5");
        assert_eq!(size.classes["sm"], "h-12 gap-1.5 px-4");
        assert_eq!(size.heights["default"], 56);
        assert_eq!(size.heights["sm"], 48);
        assert_eq!(
            find(&enums, "button_variant").classes["outline"],
            "border-border"
        );
    }

    #[test]
    fn slug_arms_are_never_read_as_classes() {
        let enums = parse_rust_enums(BUTTON_RS);
        let size = find(&enums, "button_size");
        assert_eq!(size.classes.len(), 5);
        for (v, c) in &size.classes {
            assert!(c.contains('-'), "{v} got a slug, not a class string: {c:?}");
        }
        assert_eq!(size.classes["icon_sm"], "size-12");
        // A `slug()` that comes *before* `classes()` must not be read either.
        let slug_first = r#"
enum K { #[default] A }
impl K {
    fn slug(&self) -> &str { match self { Self::A => "a" } }
    fn classes(&self) -> &str { match self { Self::A => "h-10" } }
}"#;
        let k = parse_rust_enums(slug_first);
        assert_eq!(k[0].classes["a"], "h-10");
        assert_eq!(k[0].heights["a"], 40);
    }

    #[test]
    fn two_enums_sharing_a_variant_name_stay_separate() {
        let enums = parse_rust_enums(BUTTON_RS);
        assert_eq!(enums.len(), 2);
        let variant = find(&enums, "button_variant");
        let size = find(&enums, "button_size");
        assert_eq!(variant.variants, ["default", "outline"]);
        assert_eq!(size.variants, ["default", "sm", "lg", "icon", "icon_sm"]);
        assert_eq!(
            variant.classes["default"],
            "bg-primary text-primary-foreground hover:bg-primary/80"
        );
        assert_eq!(size.classes["default"], "h-14 gap-2 px-5");
        assert!(variant.heights.is_empty());
    }

    #[test]
    fn default_attribute_is_extracted() {
        let enums = parse_rust_enums(BUTTON_RS);
        assert_eq!(
            find(&enums, "button_size").default.as_deref(),
            Some("default")
        );
        let src = "#[derive(Default)]\npub enum Density {\n    Roomy,\n    /// doc\n    #[default]\n    #[allow(dead_code)]\n    CompactFit,\n}\n";
        let d = parse_rust_enums(src);
        assert_eq!(d[0].variants, ["roomy", "compact_fit"]);
        assert_eq!(d[0].default.as_deref(), Some("compact_fit"));
        let none = parse_rust_enums("enum E { A, B }");
        assert_eq!(none[0].default, None);
    }

    #[test]
    fn a_manual_default_impl_is_read_as_the_default() {
        let src = "enum E { A, Bee }\nimpl Default for E {\n    fn default() -> Self {\n        E::Bee\n    }\n}\n";
        assert_eq!(parse_rust_enums(src)[0].default.as_deref(), Some("bee"));
    }

    #[test]
    fn non_literal_or_guarded_arms_and_wildcards_are_not_class_strings() {
        let src = r#"
enum E { A, B, C, D, F }
impl E {
    fn classes(&self) -> String {
        match self {
            Self::A => format!("h-{}", 12),
            Self::B | Self::C => "h-10",
            x if x.is_big() => "h-20",
            _ => "h-99",
        }
    }
}"#;
        let e = &parse_rust_enums(src)[0];
        assert!(!e.classes.contains_key("a"));
        assert_eq!(e.classes["b"], "h-10");
        assert_eq!(e.classes["c"], "h-10");
        assert!(!e.classes.contains_key("d"));
        assert!(!e.classes.contains_key("f"));
    }

    #[test]
    fn mzizi_reads_every_variant_and_the_prop_default() {
        let enums = parse_mzizi_enums(BUTTON_MZ);
        assert_eq!(enums.len(), 2);
        let variant = find(&enums, "button_variant");
        assert_eq!(variant.variants, ["default", "outline"]);
        assert!(variant.heights.is_empty());
        assert_eq!(variant.default.as_deref(), Some("default"));
        let size = find(&enums, "button_size");
        assert_eq!(size.default.as_deref(), Some("default"));
        assert_eq!(size.heights["sm"], 48);
        assert_eq!(size.classes["sm"], "h-12 gap-1.5 px-4");

        let other = "enum tone\n  calm class \"a\"\n  loud\nend\nprop t: tone = loud\nprop u: tone = calm\n";
        let t = &parse_mzizi_enums(other)[0];
        assert_eq!(t.variants, ["calm", "loud"]);
        assert_eq!(
            t.default.as_deref(),
            Some("loud"),
            "the first prop default wins"
        );
        let no_default = parse_mzizi_enums("enum tone\n  calm\nend\nprop t: tone\n");
        assert_eq!(no_default[0].default, None);
    }

    #[test]
    fn the_button_shapes_score_clean() {
        let r = parse_rust_enums(BUTTON_RS);
        let report = score(Arm::Mzizi, &r, &parse_mzizi_enums(BUTTON_MZ));
        assert_eq!(report.defects(), 0, "{:#?}", report.facts);
        // 2 variant sets + 2 defaults + 5 heights.
        assert_eq!(report.facts.len(), 9);
        let j = report.class_token_jaccard.unwrap();
        assert!((j - 1.0).abs() < 1e-9, "{j}");
    }

    #[test]
    fn a_rust_candidate_scored_against_itself_is_clean() {
        let r = parse_rust_enums(BUTTON_RS);
        let report = score(Arm::Dioxus, &r, &r);
        assert_eq!(report.defects(), 0);
        assert_eq!(report.class_token_jaccard, Some(1.0));
    }

    #[test]
    fn a_missing_candidate_enum_is_a_defect_not_a_skip() {
        let r = parse_rust_enums(BUTTON_RS);
        let only_variant = "enum button_variant\n  default class \"x\"\n  outline\nend\nprop v: button_variant = default\n";
        let report = score(Arm::Mzizi, &r, &parse_mzizi_enums(only_variant));
        let size_facts: Vec<&Fact> = report
            .facts
            .iter()
            .filter(|f| f.enum_name == "button_size")
            .collect();
        // variant_set + default + 5 heights, every one a defect.
        assert_eq!(size_facts.len(), 7);
        assert!(size_facts.iter().all(|f| f.defect), "{size_facts:#?}");
        let set = size_facts[0];
        assert_eq!(set.kind, FactKind::VariantSet);
        assert_eq!(set.actual, "enum absent");
        assert!(
            set.expected
                .contains("missing from candidate: {default, sm, lg, icon, icon_sm}")
        );
        assert_eq!(report.defects(), 7);
    }

    #[test]
    fn variant_set_default_and_height_defects_are_each_reported() {
        let r = parse_rust_enums(BUTTON_RS);
        let wrong = "enum button_size\n  default class \"h-14\" height 56\n  sm class \"h-12\" height 44\n  lg height 56\n  icon height 56\n  huge height 80\nend\nprop s: button_size = sm\n";
        let report = score(Arm::Mzizi, &r, &parse_mzizi_enums(wrong));
        let get = |kind: FactKind, v: Option<&str>| {
            report
                .facts
                .iter()
                .find(|f| {
                    f.enum_name == "button_size" && f.kind == kind && f.variant.as_deref() == v
                })
                .unwrap()
        };
        let set = get(FactKind::VariantSet, None);
        assert!(set.defect);
        assert!(
            set.expected.ends_with("missing from candidate: {icon_sm}"),
            "{}",
            set.expected
        );
        assert!(
            set.actual.ends_with("extra in candidate: {huge}"),
            "{}",
            set.actual
        );
        let d = get(FactKind::Default, None);
        assert!(d.defect);
        assert_eq!((d.expected.as_str(), d.actual.as_str()), ("default", "sm"));
        assert!(!get(FactKind::Height, Some("default")).defect);
        let sm = get(FactKind::Height, Some("sm"));
        assert!(sm.defect);
        assert_eq!((sm.expected.as_str(), sm.actual.as_str()), ("48px", "44px"));
        assert!(
            !get(FactKind::Height, Some("lg")).defect,
            "a declared height needs no class"
        );
        let icon_sm = get(FactKind::Height, Some("icon_sm"));
        assert!(icon_sm.defect);
        assert_eq!(icon_sm.actual, "missing");
    }

    #[test]
    fn a_reference_without_a_default_checks_no_default_fact() {
        let r = parse_rust_enums("enum Tone { Calm, Loud }");
        let c = parse_mzizi_enums("enum tone\n  calm\n  loud\nend\nprop t: tone = loud\n");
        let report = score(Arm::Mzizi, &r, &c);
        assert_eq!(report.facts.len(), 1);
        assert_eq!(report.defects(), 0);
        assert_eq!(report.class_token_jaccard, None);
    }

    #[test]
    fn jaccard_is_the_mean_over_shared_variants_and_never_a_defect() {
        let r = parse_rust_enums(
            r#"enum E { #[default] A, B } impl E { fn classes(&self) -> &str { match self { Self::A => "x y", Self::B => "p q r s" } } }"#,
        );
        let c =
            parse_mzizi_enums("enum e\n  a class \"x y\"\n  b class \"p q\"\nend\nprop e: e = a\n");
        let report = score(Arm::Mzizi, &r, &c);
        assert_eq!(report.defects(), 0);
        // (1.0 + 2/4) / 2
        assert_eq!(report.class_token_jaccard, Some(0.75));
    }

    /// The changelog task's shape: the reference names four accents by mineral, the spec
    /// (and so the candidate) by axis, with identical class strings.
    const ACCENT_RS: &str = r#"
#[derive(Default)]
pub enum NodeAccent { #[default] Cobalt, Tanzanite, Malachite, Gold }
impl NodeAccent {
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Cobalt => "bg-[var(--color-cobalt)]/10 text-[var(--color-cobalt)]",
            Self::Tanzanite => "bg-[var(--color-tanzanite)]/10 text-[var(--color-tanzanite)]",
            Self::Malachite => "bg-[var(--color-malachite)]/10 text-[var(--color-malachite)]",
            Self::Gold => "bg-[var(--color-gold)]/10 text-[var(--color-gold)]",
        }
    }
}
"#;

    const ACCENT_MZ: &str = r#"enum node_accent
    horizontal  class "bg-[var(--color-cobalt)]/10 text-[var(--color-cobalt)]"
    vertical    class "text-[var(--color-tanzanite)]   bg-[var(--color-tanzanite)]/10"
    depth       class "bg-[var(--color-malachite)]/10 text-[var(--color-malachite)]"
    outlier     class "bg-[var(--color-gold)]/10 text-[var(--color-gold)]"
end
prop accent: node_accent = horizontal
"#;

    #[test]
    fn renamed_variants_with_identical_classes_are_paired_not_defects() {
        let r = parse_rust_enums(ACCENT_RS);
        for (arm, c) in [
            (Arm::Mzizi, parse_mzizi_enums(ACCENT_MZ)),
            (
                Arm::Dioxus,
                parse_rust_enums(
                    &ACCENT_RS
                        .replace("Cobalt", "Horizontal")
                        .replace("Tanzanite", "Vertical")
                        .replace("Malachite", "Depth")
                        // Class strings are lowercase, so they survive the renames.
                        .replace("Gold", "Outlier"),
                ),
            ),
        ] {
            let report = score(arm, &r, &c);
            assert_eq!(report.defects(), 0, "{:#?}", report.facts);
            assert_eq!(report.renames, 4);
            assert_eq!(report.facts.len(), 3, "variant_set, variant_names, default");
            let names = &report.facts[1];
            assert_eq!(names.kind, FactKind::VariantNames);
            assert!(!names.defect);
            assert_eq!(
                names.actual,
                "renamed (candidate -> reference): horizontal -> cobalt, vertical -> tanzanite, depth -> malachite, outlier -> gold"
            );
            assert_eq!(report.facts[2].actual, "horizontal -> cobalt");
            // Token order and spacing differ in `vertical` above; token sets do not.
            assert_eq!(report.class_token_jaccard, Some(1.0));
        }
    }

    #[test]
    fn a_renamed_default_that_disagrees_is_still_a_defect() {
        let r = parse_rust_enums(ACCENT_RS);
        let c = parse_mzizi_enums(&ACCENT_MZ.replace("= horizontal", "= outlier"));
        let report = score(Arm::Mzizi, &r, &c);
        assert_eq!(report.defects(), 1);
        let d = report
            .facts
            .iter()
            .find(|f| f.kind == FactKind::Default)
            .unwrap();
        assert!(d.defect);
        assert_eq!(
            (d.expected.as_str(), d.actual.as_str()),
            ("cobalt", "outlier -> gold")
        );
    }

    #[test]
    fn a_genuinely_missing_variant_is_still_a_defect() {
        let r = parse_rust_enums(ACCENT_RS);
        // Three of the four: no bijection, so no pairing, so today's name-only result.
        let three: String = ACCENT_MZ
            .lines()
            .filter(|l| !l.contains("outlier"))
            .map(|l| format!("{l}\n"))
            .collect();
        let report = score(Arm::Mzizi, &r, &parse_mzizi_enums(&three));
        assert_eq!(report.renames, 0);
        assert!(
            report
                .facts
                .iter()
                .all(|f| f.kind != FactKind::VariantNames)
        );
        assert!(report.facts[0].defect, "{:#?}", report.facts);
        assert!(
            report.facts[0]
                .expected
                .contains("missing from candidate: {cobalt, tanzanite, malachite, gold}")
        );
        // A fourth variant with a class the reference does not have: counts match, sets do not.
        let wrong = ACCENT_MZ.replace("--color-gold)]/10 text", "--color-gold)]/20 text");
        let report = score(Arm::Mzizi, &r, &parse_mzizi_enums(&wrong));
        assert_eq!(report.renames, 0);
        assert_eq!(report.defects(), 2, "{:#?}", report.facts);
        assert_eq!(report.class_token_jaccard, None);
    }

    #[test]
    fn duplicate_class_strings_are_never_paired() {
        let r = parse_rust_enums(ACCENT_RS);
        let dup = ACCENT_MZ.replace(
            "--color-gold)]/10 text-[var(--color-gold)]",
            "--color-malachite)]/10 text-[var(--color-malachite)]",
        );
        let c = parse_mzizi_enums(&dup);
        assert_eq!(rename_map(&r[0], &c[0]), None);
        let report = score(Arm::Mzizi, &r, &c);
        assert_eq!(report.renames, 0);
        assert_eq!(
            report.defects(),
            2,
            "name-only, as before: {:#?}",
            report.facts
        );
        assert_eq!(report.class_token_jaccard, None);
        // And the same on the reference's side.
        let dup_rs = ACCENT_RS.replace(
            "--color-gold)]/10 text-[var(--color-gold)]",
            "--color-malachite)]/10 text-[var(--color-malachite)]",
        );
        let rr = parse_rust_enums(&dup_rs);
        assert_eq!(rename_map(&rr[0], &parse_mzizi_enums(ACCENT_MZ)[0]), None);
    }

    #[test]
    fn swapped_classes_under_shared_names_are_not_a_rename() {
        let r = parse_rust_enums(
            r#"enum E { #[default] A, B } impl E { fn classes(&self) -> &str { match self { Self::A => "x", Self::B => "y" } } }"#,
        );
        // `a` renders like the reference's `b` and vice versa, plus one renamed extra — a
        // pairing would call the swap a rename, so none is made.
        let c = parse_mzizi_enums("enum e\n  a class \"y\"\n  b class \"x\"\nend\nprop e: e = a\n");
        assert_eq!(
            rename_map(&r[0], &c[0]),
            None,
            "equal name sets are never paired"
        );
        let r3 = parse_rust_enums(
            r#"enum E { #[default] A, B, C } impl E { fn classes(&self) -> &str { match self { Self::A => "x", Self::B => "y", Self::C => "z" } } }"#,
        );
        let c3 = parse_mzizi_enums(
            "enum e\n  a class \"y\"\n  b class \"x\"\n  d class \"z\"\nend\nprop e: e = a\n",
        );
        assert_eq!(rename_map(&r3[0], &c3[0]), None);
        // A partial rename that keeps shared names on their own classes is accepted.
        let ok = parse_mzizi_enums(
            "enum e\n  a class \"x\"\n  b class \"y\"\n  d class \"z\"\nend\nprop e: e = a\n",
        );
        let report = score(Arm::Mzizi, &r3, &ok);
        assert_eq!(report.defects(), 0, "{:#?}", report.facts);
        assert_eq!(report.renames, 1);
        assert_eq!(
            report.facts[1].actual,
            "renamed (candidate -> reference): d -> c"
        );
    }

    #[test]
    fn score_json_has_the_agreed_shape_and_escapes_strings() {
        let report = ScoreReport {
            arm: Arm::Dioxus,
            facts: vec![
                Fact {
                    enum_name: "button_size".into(),
                    variant: Some("sm".into()),
                    kind: FactKind::Height,
                    expected: "48px".into(),
                    actual: "44px".into(),
                    defect: true,
                },
                Fact {
                    enum_name: "q\"uote\\back".into(),
                    variant: None,
                    kind: FactKind::VariantSet,
                    expected: "tab\there\nnl\u{1}—".into(),
                    actual: "{a}".into(),
                    defect: false,
                },
            ],
            class_token_jaccard: Some(0.5),
            renames: 1,
        };
        assert_eq!(
            report.to_json(),
            concat!(
                r#"{"arm":"dioxus","facts_checked":2,"defects":1,"renames":1,"details":["#,
                r#"{"enum":"button_size","variant":"sm","fact":"height","expected":"48px","actual":"44px","defect":true},"#,
                r#"{"enum":"q\"uote\\back","variant":null,"fact":"variant_set","expected":"tab\there\nnl\u0001—","actual":"{a}","defect":false}"#,
                r#"],"class_token_jaccard":0.5000}"#
            )
        );
        let empty = ScoreReport {
            arm: Arm::Mzizi,
            facts: vec![],
            class_token_jaccard: None,
            renames: 0,
        };
        assert_eq!(
            empty.to_json(),
            r#"{"arm":"mzizi","facts_checked":0,"defects":0,"renames":0,"details":[],"class_token_jaccard":null}"#
        );
    }

    #[test]
    fn strings_raw_strings_and_char_literals_do_not_confuse_brace_matching() {
        let src = r####"
enum E { #[default] A, B, C }
const X: char = '{';
const Y: char = '"';
impl E {
    fn classes(&self) -> &'static str {
        match self {
            Self::A => r#"h-10 "quoted" {"#,
            Self::B => "h-\
                        12",
            Self::C => "}",
        }
    }
}"####;
        let e = &parse_rust_enums(src)[0];
        assert_eq!(e.classes["a"], "h-10 \"quoted\" {");
        assert_eq!(e.classes["b"], "h-12");
        assert_eq!(e.classes["c"], "}");
        assert_eq!(e.heights["a"], 40);
        assert_eq!(e.heights["b"], 48);
    }

    // --- `diff` (height-only report) -------------------------------------------------

    #[test]
    fn only_the_height_bearing_enum_is_a_size_enum() {
        let enums = parse_mzizi_size_enums(BUTTON_MZ);
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].name, "button_size");
        assert_eq!(enums[0].variants.len(), 5);
    }

    #[test]
    fn the_button_declarations_match_the_reduced_reference() {
        let enums = parse_mzizi_size_enums(BUTTON_MZ);
        let arms = reference_class_arms(&parse_rust_enums(BUTTON_RS), "button_size");
        assert_eq!(arms.len(), 5);
        let report = diff_size_enum(&enums[0], &arms);
        assert_eq!(report.defect_count(), 0, "{:#?}", report.results);
    }

    #[test]
    fn a_declared_height_that_disagrees_with_the_tailwind_class_is_a_mismatch() {
        let broken = BUTTON_MZ.replace(
            "sm        class \"h-12 gap-1.5 px-4\"   height 48",
            "sm        class \"h-12 gap-1.5 px-4\"   height 44",
        );
        let enums = parse_mzizi_size_enums(&broken);
        let arms = reference_class_arms(&parse_rust_enums(BUTTON_RS), "button_size");
        let report = diff_size_enum(&enums[0], &arms);
        assert_eq!(report.defect_count(), 1);
        assert!(report.results.iter().any(|r| matches!(
            r,
            VariantResult::Mismatch { name, declared: 44, derived: 48, token } if name == "sm" && token == "h-12"
        )));
    }

    #[test]
    fn a_size_enum_missing_from_the_reference_is_reported_not_skipped() {
        let enums = parse_mzizi_size_enums(BUTTON_MZ);
        let arms = reference_class_arms(&parse_rust_enums("enum Other { A }"), "button_size");
        let report = diff_size_enum(&enums[0], &arms);
        assert_eq!(report.defect_count(), 5);
        assert!(
            report
                .results
                .iter()
                .all(|r| matches!(r, VariantResult::MissingInReference { .. }))
        );
    }

    #[test]
    fn a_reference_variant_missing_from_mzizi_is_reported_not_skipped() {
        let rust_variants = vec![RsVariant {
            name: "xl".to_string(),
            class: "h-16".to_string(),
        }];
        let mz = "enum s\n  only  class \"h-14\"  height 56\nend\n";
        let report = diff_size_enum(&parse_mzizi_size_enums(mz)[0], &rust_variants);
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
        let mz = "enum s\n  only  class \"h-14\"  height 56\nend\n";
        let report = diff_size_enum(&parse_mzizi_size_enums(mz)[0], &rust_variants);
        assert_eq!(report.defect_count(), 1);
        assert!(matches!(
            &report.results[0],
            VariantResult::Unevaluable { .. }
        ));
    }
}
