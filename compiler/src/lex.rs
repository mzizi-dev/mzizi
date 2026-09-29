//! The lexer. One statement per line, `end`-delimited blocks, no significant whitespace.
//!
//! Two RFC decisions land here rather than in the parser:
//!
//! - **Newlines are tokens** (RFC-0001 §2). Statements are newline-terminated, which is what
//!   lets the parser resynchronize at every line and report one diagnostic per real error
//!   instead of a cascade (FM-5).
//! - **`snake_case` is grammar, not lint** (RFC-0001 §2). A `camelCase` identifier is a lex
//!   error carrying an `exact` fix, so `mz fix` repairs it without a model in the loop.

use crate::diagnostic::{Confidence, Diagnostic, Span};

/// A lexical token kind.
#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// A keyword from the closed vocabulary (RFC-0002 §1: small, common English words).
    Keyword(&'static str),
    /// A `snake_case` identifier.
    Ident(String),
    /// A string literal's raw inner text, interpolation braces intact.
    Str(String),
    /// An integer literal.
    Int(i64),
    /// A doc comment's text, `##` stripped.
    Doc(String),
    /// `:`
    Colon,
    /// `=`
    Equals,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// End of a logical line.
    Newline,
    /// End of input.
    Eof,
}

/// A token with its source position.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    /// What it is.
    pub kind: Tok,
    /// Where it is.
    pub span: Span,
}

/// The closed keyword vocabulary. Every entry is a common English word that tokenizes
/// cheaply — RFC-0002 §1's small-vocabulary rule. Adding to this list is a language change.
///
/// Type names (`bool`, `int`, `text`) are deliberately **not** here. Reserving them bought
/// nothing and cost a collision: `text` is also a view attribute, so lexing it as a keyword
/// made a valid view line unparseable. Types resolve by position, not by reservation — which
/// is also the smaller vocabulary RFC-0002 §1 asks for.
pub const KEYWORDS: &[&str] = &[
    "component",
    "end",
    "use",
    "enum",
    "prop",
    "view",
    "fn",
    "contract",
    "when",
    "else",
    "not",
    "is",
    "match",
    "case",
    "for",
    "each",
    "in",
    "emit",
    "nothing",
    "none",
    "event",
    "true",
    "false",
];

/// Convert a `camelCase` or `PascalCase` identifier to `snake_case`.
///
/// Used to build the `exact` fix on a naming diagnostic, and to map corpus component
/// identity (`mzizi-connectivity-bar`) onto source names by the fixed rule in RFC-0001 §2.
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.char_indices() {
        if ch == '-' {
            out.push('_');
        } else if ch.is_ascii_uppercase() {
            if i > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// The constructor a symbolic spelling stands for: `list<T>` and `Vec<T>` are lists,
/// `option<T>` is an option, `event<T>` an event.
fn constructor_for(word: &str) -> Option<&'static str> {
    match word {
        "list" | "vec" | "array" => Some("list"),
        "option" => Some("option"),
        "event" => Some("event"),
        _ => None,
    }
}

/// Handle `<` or `[` at `i` when it spells a type constructor. Returns how many characters
/// to advance past, or `None` to let the ordinary bad-character diagnostic stand.
///
/// The whole type expression — from the identifier before `i` (if adjacent), else from `i`
/// — is read by [`symbolic_type`] and rewritten to its parenthesised, snake_case form in one
/// diagnostic with one `exact` fix: `list<entry>`, `entry[]`, `[entry]`, and any nesting of
/// them (`list<entry[]>` is `list(list(entry))`, `Option<Entry>` is `option(entry)`). The
/// repaired tokens are emitted directly, so nothing inside the span is reported again and
/// the parser sees a well-formed type (FM-5).
fn repair_symbolic_type(
    bytes: &[char],
    i: usize,
    line_no: u32,
    file: &str,
    tokens: &mut Vec<Token>,
    diags: &mut Vec<Diagnostic>,
) -> Option<usize> {
    let col = (i + 1) as u32;
    let adjacent = tokens.last().and_then(|t| match &t.kind {
        Tok::Ident(_) | Tok::Keyword("event")
            if t.span.start_line == line_no && t.span.end_col == col =>
        {
            Some(t.span)
        }
        _ => None,
    });
    let start = match adjacent {
        Some(span) => (span.start_col - 1) as usize,
        None if bytes[i] == '[' => i,
        None => return None,
    };
    let mut out = Vec::new();
    let (fixed, end) = symbolic_type(bytes, start, line_no, &mut out)?;
    if end <= i {
        return None;
    }
    let span = Span {
        start_line: line_no,
        start_col: (start + 1) as u32,
        end_line: line_no,
        end_col: (end + 1) as u32,
    };
    if let Some(word_span) = adjacent {
        tokens.pop();
        // A camelCase word (`Option<text>`) already reported MZ0101 at this token. Fold it
        // into this one diagnostic, so `mz fix` never sees two overlapping repairs.
        if diags
            .last()
            .is_some_and(|d| d.code == "MZ0101" && d.span == word_span)
        {
            diags.pop();
        }
    }
    let written: String = bytes[start..end].iter().collect();
    let say = if written.contains('<') {
        format!(
            "`{written}` is not how Mzizi spells a type — type constructors take parentheses: `{fixed}`"
        )
    } else {
        format!("`{written}` is not how Mzizi spells a list — write `{fixed}`")
    };
    diags.push(Diagnostic::error("MZ0105", file, span, say).with_fix(
        span,
        fixed,
        Confidence::Exact,
    ));
    tokens.extend(out);
    Some(end - i)
}

/// Read one type expression at `at`, in any mix of Mzizi and symbolic spellings:
/// `word`, `word(T)`, `word<T>`, `[T]`, each optionally followed by `[]`s. Returns the
/// canonical spelling and the index just past it, and pushes its tokens onto `out`.
/// `None` when the text is not a type expression (e.g. `list<>`, an unclosed `<`).
fn symbolic_type(
    bytes: &[char],
    at: usize,
    line_no: u32,
    out: &mut Vec<Token>,
) -> Option<(String, usize)> {
    let tok = |kind: Tok, at: usize, len: usize| Token {
        kind,
        span: Span::single(line_no, (at + 1) as u32, len as u32),
    };
    let skip = |mut k: usize| {
        while bytes.get(k) == Some(&' ') {
            k += 1;
        }
        k
    };
    let first = out.len();
    let k = skip(at);
    let (mut fixed, mut k) = if bytes.get(k) == Some(&'[') {
        out.push(tok(Tok::Ident("list".to_string()), k, 1));
        out.push(tok(Tok::LParen, k, 1));
        let (inner, after) = symbolic_type(bytes, k + 1, line_no, out)?;
        let after = skip(after);
        if bytes.get(after) != Some(&']') {
            return None;
        }
        out.push(tok(Tok::RParen, after, 1));
        (format!("list({inner})"), after + 1)
    } else {
        let mut j = k;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == '_') {
            j += 1;
        }
        if j == k || bytes[k].is_ascii_digit() {
            return None;
        }
        let word = to_snake_case(&bytes[k..j].iter().collect::<String>());
        let open = bytes.get(j).copied();
        if open == Some('<') || open == Some('(') {
            let ctor = if open == Some('<') {
                constructor_for(&word)?.to_string()
            } else {
                word
            };
            out.push(tok(ident_or_keyword(ctor.clone()), k, j - k));
            out.push(tok(Tok::LParen, j, 1));
            let (inner, after) = symbolic_type(bytes, j + 1, line_no, out)?;
            let after = skip(after);
            let close = if open == Some('<') { '>' } else { ')' };
            if bytes.get(after) != Some(&close) {
                return None;
            }
            out.push(tok(Tok::RParen, after, 1));
            (format!("{ctor}({inner})"), after + 1)
        } else {
            out.push(tok(ident_or_keyword(word.clone()), k, j - k));
            (word, j)
        }
    };
    // Postfix `[]`, any number of times: `entry[][]` is `list(list(entry))`.
    while bytes.get(k) == Some(&'[') && bytes.get(k + 1) == Some(&']') {
        out.insert(first, tok(Tok::LParen, k, 2));
        out.insert(first, tok(Tok::Ident("list".to_string()), k, 2));
        out.push(tok(Tok::RParen, k, 2));
        fixed = format!("list({fixed})");
        k += 2;
    }
    Some((fixed, k))
}

fn ident_or_keyword(word: String) -> Tok {
    match KEYWORDS.iter().find(|k| **k == word) {
        Some(kw) => Tok::Keyword(kw),
        None => Tok::Ident(word),
    }
}

/// A spread at `i` — `...props`, `..attributes` or `{...props}` — as the half-open char
/// range it covers and the name it spreads. Two or three dots, then a name, and a closing
/// `}` when it opened with one. A dotted path (`state.color`) has one dot and is not this.
fn spread_at(bytes: &[char], i: usize) -> Option<(usize, usize, String)> {
    let braced = bytes.get(i) == Some(&'{');
    let mut k = if braced { i + 1 } else { i };
    let dots = bytes[k.min(bytes.len())..]
        .iter()
        .take_while(|c| **c == '.')
        .count();
    if !(2..=3).contains(&dots) {
        return None;
    }
    k += dots;
    let name_start = k;
    while k < bytes.len() && (bytes[k].is_ascii_alphanumeric() || bytes[k] == '_') {
        k += 1;
    }
    if k == name_start || bytes[name_start].is_ascii_digit() {
        return None;
    }
    let name: String = bytes[name_start..k].iter().collect();
    if braced {
        if bytes.get(k) != Some(&'}') {
            return None;
        }
        k += 1;
    }
    Some((i, k, name))
}

/// Whether a line holds nothing but a spread: `...props`, `{...props}`, or the prop
/// declaration an agent writes for one, `prop ...props: <type>`. Then the whole line is
/// the mistake, and deleting it is the repair.
fn spread_is_whole_line(bytes: &[char], start: usize, end: usize) -> bool {
    let rest: String = bytes[..start].iter().chain(&bytes[end..]).collect();
    let mut r = rest.trim();
    if let Some(after) = r.strip_prefix("prop")
        && (after.is_empty() || after.starts_with(char::is_whitespace) || after.starts_with(':'))
    {
        r = after.trim_start();
    }
    // A spread as a value — `tap = ...props`, or `tap is ...props` in a contract — leaves
    // `tap =` behind if only the spread goes, which is a second error for the same
    // mistake. The line is nothing without its value, so it goes too.
    let word_then = |op: &str| {
        r.strip_suffix(op).is_some_and(|w| {
            let w = w.trim_end();
            !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    };
    r.is_empty() || r.starts_with(':') || word_then("=") || word_then(" is")
}

/// Tokenize `src`. Never fails: bad input produces diagnostics and the lexer keeps going,
/// so the parser always receives a full token stream to recover against.
pub fn lex(src: &str, file: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diags = Vec::new();

    for (line_idx, line) in src.lines().enumerate() {
        let line_no = (line_idx + 1) as u32;
        let bytes: Vec<char> = line.chars().collect();
        let mut i = 0usize;
        let line_first_token = tokens.len();
        let line_first_diag = diags.len();

        while i < bytes.len() {
            let col = (i + 1) as u32;
            let ch = bytes[i];

            // A spread is the React (`{...props}`) and Dioxus (`..attributes`) way to pass
            // on every attribute a component does not name. Mzizi has no form for it, so
            // the one repair is to delete it — pilot 2's 7B model wrote `prop ...props`
            // in every badge episode and got `MZ0304 prop needs a name, found .`, which
            // told it nothing it could act on. One diagnostic, and the parser sees
            // nothing of the spread, so it cannot cascade.
            if (ch == '.' || ch == '{')
                && let Some((start, end, name)) = spread_at(&bytes, i)
            {
                let written: String = bytes[start..end].iter().collect();
                let span = Span::single(line_no, col, (end - start) as u32);
                let whole = spread_is_whole_line(&bytes, start, end);
                if whole {
                    // The line goes, so nothing reported earlier on it still applies.
                    diags.truncate(line_first_diag);
                }
                let (fix_span, what) = if whole {
                    let fix = Span {
                        start_line: line_no,
                        start_col: 1,
                        end_line: line_no + 1,
                        end_col: 1,
                    };
                    (fix, "delete the line")
                } else {
                    (span, "delete it")
                };
                diags.push(
                    Diagnostic::error(
                        "MZ0106",
                        file,
                        span,
                        format!(
                            "`{written}` is a spread, and Mzizi has none: a component names each prop it reads and each attribute it sets. `{name}` has no equivalent — {what}"
                        ),
                    )
                    .with_fix(fix_span, "", Confidence::Exact),
                );
                if whole {
                    tokens.truncate(line_first_token);
                    break;
                }
                i = end;
                continue;
            }

            if ch.is_whitespace() {
                i += 1;
                continue;
            }

            // Doc comment: `##` to end of line.
            if ch == '#' && bytes.get(i + 1) == Some(&'#') {
                let text: String = bytes[(i + 2).min(bytes.len())..].iter().collect();
                let len = (bytes.len() - i) as u32;
                tokens.push(Token {
                    kind: Tok::Doc(text.trim().to_string()),
                    span: Span::single(line_no, col, len),
                });
                i = bytes.len();
                continue;
            }

            // String literal. An unterminated string is reported and closed at line end —
            // never allowed to swallow the rest of the file.
            if ch == '"' {
                let mut j = i + 1;
                let mut text = String::new();
                let mut closed = false;
                while j < bytes.len() {
                    if bytes[j] == '"' {
                        closed = true;
                        break;
                    }
                    text.push(bytes[j]);
                    j += 1;
                }
                let len = ((j + if closed { 1 } else { 0 }) - i) as u32;
                if !closed {
                    diags.push(
                        Diagnostic::error(
                            "MZ0102",
                            file,
                            Span::single(line_no, col, len),
                            format!(
                                "unterminated string `\"{}` — strings cannot span lines",
                                text
                            ),
                        )
                        .with_fix(
                            Span::single(line_no, (bytes.len() + 1) as u32, 0),
                            "\"",
                            Confidence::Exact,
                        ),
                    );
                }
                tokens.push(Token {
                    kind: Tok::Str(text),
                    span: Span::single(line_no, col, len),
                });
                i = if closed { j + 1 } else { bytes.len() };
                continue;
            }

            // Identifier or keyword.
            if ch.is_ascii_alphabetic() || ch == '_' {
                let mut j = i;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == '_') {
                    j += 1;
                }
                let word: String = bytes[i..j].iter().collect();
                let len = (j - i) as u32;
                let span = Span::single(line_no, col, len);

                if let Some(kw) = KEYWORDS.iter().find(|k| **k == word) {
                    tokens.push(Token {
                        kind: Tok::Keyword(kw),
                        span,
                    });
                } else {
                    if word.chars().any(|c| c.is_ascii_uppercase()) {
                        let snake = to_snake_case(&word);
                        diags.push(
                            Diagnostic::error(
                                "MZ0101",
                                file,
                                span,
                                format!(
                                    "`{word}` is not snake_case — Mzizi names are snake_case, so write `{snake}`"
                                ),
                            )
                            .with_fix(span, snake.clone(), Confidence::Exact),
                        );
                        tokens.push(Token {
                            kind: Tok::Ident(snake),
                            span,
                        });
                    } else {
                        tokens.push(Token {
                            kind: Tok::Ident(word),
                            span,
                        });
                    }
                    i = j;
                    continue;
                }
                i = j;
                continue;
            }

            // Integer.
            if ch.is_ascii_digit() {
                let mut j = i;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                let text: String = bytes[i..j].iter().collect();
                let len = (j - i) as u32;
                // A literal too large for i64 is reported, not silently wrapped.
                match text.parse::<i64>() {
                    Ok(v) => tokens.push(Token {
                        kind: Tok::Int(v),
                        span: Span::single(line_no, col, len),
                    }),
                    Err(_) => diags.push(Diagnostic::error(
                        "MZ0103",
                        file,
                        Span::single(line_no, col, len),
                        format!("`{text}` does not fit in an int"),
                    )),
                }
                i = j;
                continue;
            }

            // A type constructor spelt with symbols — the TypeScript and Rust priors
            // `list<entry>`, `entry[]`, `[entry]` (RFC-0008 §1). One diagnostic with the
            // Mzizi spelling as an `exact` fix, and the tokens are repaired in place so
            // the parser sees `list(entry)` and reports nothing further (FM-5).
            if (ch == '<' || ch == '[')
                && let Some(consumed) =
                    repair_symbolic_type(&bytes, i, line_no, file, &mut tokens, &mut diags)
            {
                i += consumed;
                continue;
            }

            let single = match ch {
                ':' => Some(Tok::Colon),
                '=' => Some(Tok::Equals),
                '(' => Some(Tok::LParen),
                ')' => Some(Tok::RParen),
                ',' => Some(Tok::Comma),
                '.' => Some(Tok::Dot),
                _ => None,
            };
            match single {
                Some(kind) => {
                    tokens.push(Token {
                        kind,
                        span: Span::single(line_no, col, 1),
                    });
                }
                None => {
                    let say = if ch == '?' {
                        "`?` is not a character Mzizi uses — an optional type is written `option(<type>)`".to_string()
                    } else {
                        format!("`{ch}` is not a character Mzizi uses")
                    };
                    diags.push(Diagnostic::error(
                        "MZ0104",
                        file,
                        Span::single(line_no, col, 1),
                        say,
                    ));
                }
            }
            i += 1;
        }

        tokens.push(Token {
            kind: Tok::Newline,
            span: Span::single(line_no, (bytes.len() + 1) as u32, 0),
        });
    }

    // End of file is the point just past the last character: the start of the line after
    // a trailing newline, or the end of an unterminated last line. It used to be column 1
    // of the last line, which put every `MZ0204` insertion *before* that line's text, so
    // the `exact` fix for an unclosed block broke the line it was meant to follow.
    let lines = src.lines().count() as u32;
    let eof = match src.lines().last() {
        Some(last) if !src.ends_with('\n') => {
            Span::single(lines, last.chars().count() as u32 + 1, 0)
        }
        _ => Span::single(lines + 1, 1, 0),
    };
    tokens.push(Token {
        kind: Tok::Eof,
        span: eof,
    });
    (tokens, diags)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Tok> {
        lex(src, "t.mz")
            .0
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| *k != Tok::Newline)
            .collect()
    }

    #[test]
    fn snake_case_conversion_handles_the_shapes_that_occur() {
        assert_eq!(to_snake_case("onStateChange"), "on_state_change");
        assert_eq!(to_snake_case("ConnectivityBar"), "connectivity_bar");
        assert_eq!(
            to_snake_case("mzizi-connectivity-bar"),
            "mzizi_connectivity_bar"
        );
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn a_camel_case_identifier_is_an_error_with_an_exact_fix() {
        let (_, diags) = lex("prop onStateChange: event", "t.mz");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "MZ0101");
        let fix = diags[0]
            .fix
            .as_ref()
            .expect("naming errors must carry a fix");
        assert_eq!(fix.replace, "on_state_change");
        assert_eq!(fix.confidence, Confidence::Exact);
    }

    #[test]
    fn a_camel_case_identifier_still_lexes_as_its_snake_form() {
        // Recovery: the parser must be able to keep going and find later errors, so the
        // token stream carries the repaired name rather than a hole.
        let toks = kinds("prop onState: bool");
        assert!(toks.contains(&Tok::Ident("on_state".to_string())));
    }

    #[test]
    fn keywords_are_distinguished_from_identifiers() {
        let toks = kinds("component connectivity_bar");
        assert_eq!(toks[0], Tok::Keyword("component"));
        assert_eq!(toks[1], Tok::Ident("connectivity_bar".to_string()));
    }

    #[test]
    fn an_unterminated_string_is_reported_and_does_not_swallow_the_file() {
        let (toks, diags) = lex("text \"hello\nprop visible: bool", "t.mz");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "MZ0102");
        // The second line still lexed, so the parser can still check it.
        assert!(toks.iter().any(|t| t.kind == Tok::Keyword("prop")));
    }

    #[test]
    fn doc_comments_are_captured_not_discarded() {
        let toks = kinds("## A status strip.");
        assert_eq!(toks[0], Tok::Doc("A status strip.".to_string()));
    }

    #[test]
    fn interpolation_braces_survive_into_the_string_token() {
        let toks = kinds(r#"class "top-0 {state.color}""#);
        assert_eq!(toks[1], Tok::Str("top-0 {state.color}".to_string()));
    }

    #[test]
    fn an_oversized_int_is_reported_rather_than_wrapping() {
        let (_, diags) = lex("min_height 99999999999999999999999", "t.mz");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "MZ0103");
    }
}
