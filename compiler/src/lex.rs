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
/// identity (`nyuchi-connectivity-bar`) onto source names by the fixed rule in RFC-0001 §2.
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
/// Three shapes, each rewritten to the parenthesised form:
///
/// - `list<entry>` (and `vec<…>`, `option<…>`, `event<…>`): the angle brackets become
///   parentheses in place, so lexing continues over the repaired line.
/// - `entry[]`: the preceding identifier token is replaced by `list ( entry )`.
/// - `[entry]`: a `list` token is emitted and the brackets become parentheses.
fn repair_symbolic_type(
    bytes: &mut [char],
    i: usize,
    line_no: u32,
    file: &str,
    tokens: &mut Vec<Token>,
    diags: &mut Vec<Diagnostic>,
) -> Option<usize> {
    let col = (i + 1) as u32;
    let adjacent_ident = match tokens.last() {
        Some(Token {
            kind: Tok::Ident(word),
            span,
        }) if span.start_line == line_no && span.end_col == col => Some((word.clone(), *span)),
        Some(Token {
            kind: Tok::Keyword("event"),
            span,
        }) if span.start_line == line_no && span.end_col == col => {
            Some(("event".to_string(), *span))
        }
        _ => None,
    };
    let close_of = |open: char, close: char| -> Option<usize> {
        let mut depth = 0usize;
        for (j, c) in bytes.iter().enumerate().skip(i) {
            if *c == open {
                depth += 1;
            } else if *c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            } else if !(c.is_ascii_alphanumeric()
                || matches!(c, '_' | '(' | ')' | ' ' | '<' | '>' | '[' | ']'))
            {
                return None;
            }
        }
        None
    };
    let text = |from: usize, to: usize, bytes: &[char]| -> String {
        bytes[from..to]
            .iter()
            .map(|c| match c {
                '<' | '[' => '(',
                '>' | ']' => ')',
                c => *c,
            })
            .collect::<String>()
    };
    // A camelCase constructor (`Option<text>`) already reported MZ0101 at the same token.
    // Fold that into this one diagnostic, so `mz fix` never sees two overlapping repairs.
    let fold_naming = |diags: &mut Vec<Diagnostic>, span: Span| {
        if diags
            .last()
            .is_some_and(|d| d.code == "MZ0101" && d.span == span)
        {
            diags.pop();
        }
    };

    if bytes[i] == '<' {
        let (word, word_span) = adjacent_ident?;
        let ctor = constructor_for(&word)?;
        let close = close_of('<', '>')?;
        let inner = text(i + 1, close, bytes).trim().to_string();
        if inner.is_empty() {
            return None;
        }
        let span = Span {
            start_line: line_no,
            start_col: word_span.start_col,
            end_line: line_no,
            end_col: (close + 2) as u32,
        };
        fold_naming(diags, word_span);
        let fixed = format!("{ctor}({inner})");
        diags.push(
            Diagnostic::error(
                "MZ0105",
                file,
                span,
                format!(
                    "`{}` is not how Mzizi spells a type — type constructors take parentheses: `{fixed}`",
                    bytes[(word_span.start_col - 1) as usize..=close]
                        .iter()
                        .collect::<String>()
                ),
            )
            .with_fix(span, fixed, Confidence::Exact),
        );
        if let Some(last) = tokens.last_mut() {
            last.kind = Tok::Ident(ctor.to_string());
        }
        for c in bytes[i..=close].iter_mut() {
            *c = match *c {
                '<' | '[' => '(',
                '>' | ']' => ')',
                other => other,
            };
        }
        return Some(0);
    }

    // `entry[]`
    if bytes.get(i + 1) == Some(&']') {
        let (word, word_span) = adjacent_ident?;
        let span = Span {
            start_line: line_no,
            start_col: word_span.start_col,
            end_line: line_no,
            end_col: col + 2,
        };
        fold_naming(diags, word_span);
        let fixed = format!("list({word})");
        diags.push(
            Diagnostic::error(
                "MZ0105",
                file,
                span,
                format!("`{word}[]` is not how Mzizi spells a list — write `{fixed}`"),
            )
            .with_fix(span, fixed, Confidence::Exact),
        );
        tokens.pop();
        for kind in [
            Tok::Ident("list".to_string()),
            Tok::LParen,
            Tok::Ident(word),
            Tok::RParen,
        ] {
            tokens.push(Token { kind, span });
        }
        return Some(2);
    }

    // `[entry]`
    let close = close_of('[', ']')?;
    let inner = text(i + 1, close, bytes).trim().to_string();
    if inner.is_empty() {
        return None;
    }
    let span = Span::single(line_no, col, (close - i + 1) as u32);
    let fixed = format!("list({inner})");
    diags.push(
        Diagnostic::error(
            "MZ0105",
            file,
            span,
            format!(
                "`{}` is not how Mzizi spells a list — write `{fixed}`",
                bytes[i..=close].iter().collect::<String>()
            ),
        )
        .with_fix(span, fixed, Confidence::Exact),
    );
    tokens.push(Token {
        kind: Tok::Ident("list".to_string()),
        span: Span::single(line_no, col, 1),
    });
    for c in bytes[i..=close].iter_mut() {
        *c = match *c {
            '<' | '[' => '(',
            '>' | ']' => ')',
            other => other,
        };
    }
    Some(0)
}

/// Tokenize `src`. Never fails: bad input produces diagnostics and the lexer keeps going,
/// so the parser always receives a full token stream to recover against.
pub fn lex(src: &str, file: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diags = Vec::new();

    for (line_idx, line) in src.lines().enumerate() {
        let line_no = (line_idx + 1) as u32;
        let mut bytes: Vec<char> = line.chars().collect();
        let mut i = 0usize;

        while i < bytes.len() {
            let col = (i + 1) as u32;
            let ch = bytes[i];

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
                    repair_symbolic_type(&mut bytes, i, line_no, file, &mut tokens, &mut diags)
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

    let last = src.lines().count().max(1) as u32;
    tokens.push(Token {
        kind: Tok::Eof,
        span: Span::single(last, 1, 0),
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
            to_snake_case("nyuchi-connectivity-bar"),
            "nyuchi_connectivity_bar"
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
