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
    /// A float literal (RFC-0013 §3.1), lexed only in a `program`: digits, `.`, digits.
    Float(f64),
    /// A number already reported: an integer literal too large for `int` (`MZ0103`), or in
    /// a program a malformed one with no exact repair (`MZ0914`: `0x10`, `1e3`). Lexed only
    /// in a `program`, whose parser reads it as an error value rather than a missing one, so
    /// the line gets no second diagnostic. A component or a service drops the literal, as
    /// before.
    BadInt,
    /// An integer literal too large for `int` in a program, with its digits as written and
    /// underscores dropped. The lexer reports `MZ0103` for it. The parser reads the one that
    /// is `int`'s minimum, under a unary `-` (RFC-0013 §3.1), and cancels that report.
    TooBig(String),
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
    /// `[`, lexed only in a `program` (RFC-0013 §9.1): a bracket literal or an index. In a
    /// component or a service the lexer still rewrites `[entry]` to `list(entry)`
    /// (`MZ0105`); in a program the type parser does (RFC-0013 §9.1).
    LBracket,
    /// `]`, lexed only in a `program`.
    RBracket,
    /// An operator, lexed only in a `program` file (RFC-0013 §3): `+ - * / % < <= > >=`, and
    /// the spellings other languages use that a program's parser repairs (`==`, `!=`, `&&`,
    /// `||`, `!`, `->`, `+=`, Rust's postfix `?`, …). In a component or a service these
    /// characters are still `MZ0104`, with today's text.
    Op(&'static str),
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

/// The operators a `program` lexes, longest first, so `<=` is one token and not `<` `=`.
pub const OPERATORS: &[&str] = &[
    "===", "!==", "==", "!=", "<=", ">=", "->", "=>", "&&", "||", "+=", "-=", "*=", "/=", "++",
    "--", "**", "+", "-", "*", "/", "%", "<", ">", "!", "?",
];

/// A number in a program (RFC-0013 §3.1), starting at `bytes[i]`: a digit, or a `.` before a
/// digit. Returns the token and how many characters it covers, reporting `MZ0914` for a
/// malformed number: `1.` and `.5` (`exact`: append or prepend `0`), `1_000` (`exact`:
/// `1000`), `0x10` and `1e3` (no fix, read as an error value). Digits, `.` and a letter or
/// `_` are an `int` and a method call (`2.pow(10)`), so the `.` is left for the parser.
fn program_number(
    bytes: &[char],
    i: usize,
    line_no: u32,
    file: &str,
    diags: &mut Vec<Diagnostic>,
) -> (Tok, usize) {
    let next = |k: usize| bytes.get(k).copied();
    let digit_run = |mut j: usize| {
        while next(j).is_some_and(|c| {
            c.is_ascii_digit() || (c == '_' && next(j + 1).is_some_and(|d| d.is_ascii_digit()))
        }) {
            j += 1;
        }
        j
    };
    let leading_point = bytes[i] == '.';
    let mut j = if leading_point { i } else { digit_run(i) };
    let mut float = false;
    let mut no_fix = None;
    if !leading_point
        && bytes[i] == '0'
        && j == i + 1
        && matches!(next(j), Some('x' | 'X' | 'b' | 'B' | 'o' | 'O'))
        && next(j + 1).is_some_and(|c| c.is_ascii_alphanumeric())
    {
        // `0x10`, `0b1`, `0o7`: not forms in M1.
        j += 1;
        while next(j).is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            j += 1;
        }
        no_fix = Some(
            "hexadecimal, octal and binary literals are not forms — write the number in decimal",
        );
    } else if next(j) == Some('.')
        && !next(j + 1).is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '.')
    {
        // `0..10` is another language's range, not `0.` and `.10`: the `..` is left for
        // the parser to report as what it is.
        float = true;
        j = digit_run(j + 1);
    }
    if no_fix.is_none()
        && matches!(next(j), Some('e' | 'E'))
        && (next(j + 1).is_some_and(|c| c.is_ascii_digit())
            || (matches!(next(j + 1), Some('+' | '-'))
                && next(j + 2).is_some_and(|c| c.is_ascii_digit())))
    {
        // `1e3`, `1.5e-7`: no exponent literals in M1.
        j += 2;
        while next(j).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
        }
        no_fix = Some("exponent literals are not forms — write the number in plain decimal");
    }
    let written: String = bytes[i..j].iter().collect();
    let span = Span::single(line_no, (i + 1) as u32, (j - i) as u32);
    if let Some(why) = no_fix {
        diags.push(Diagnostic::error(
            "MZ0914",
            file,
            span,
            format!("`{written}` is not a Mzizi number: {why}"),
        ));
        return (Tok::BadInt, j - i);
    }
    // The one repair that reads as the number meant: no `_`, and a digit on both sides of
    // the point.
    let mut fixed: String = written.chars().filter(|c| *c != '_').collect();
    if fixed.starts_with('.') {
        fixed.insert(0, '0');
    }
    if fixed.ends_with('.') {
        fixed.push('0');
    }
    if fixed != written {
        let why = if written.contains('_') {
            "`_` does not group digits in Mzizi"
        } else {
            "a float has a digit on both sides of its point"
        };
        diags.push(
            Diagnostic::error(
                "MZ0914",
                file,
                span,
                format!("`{written}` is not a Mzizi number: {why} — write `{fixed}`"),
            )
            .with_fix(span, fixed.clone(), Confidence::Exact),
        );
    }
    if float || leading_point {
        match fixed.parse::<f64>() {
            Ok(v) if v.is_finite() => (Tok::Float(v), j - i),
            _ => {
                diags.push(Diagnostic::error(
                    "MZ0914",
                    file,
                    span,
                    format!("`{}` is too large for a float", short_literal(&written)),
                ));
                (Tok::BadInt, j - i)
            }
        }
    } else {
        match fixed.parse::<i64>() {
            Ok(v) => (Tok::Int(v), j - i),
            Err(_) => {
                diags.push(Diagnostic::error(
                    "MZ0103",
                    file,
                    span,
                    format!("`{written}` does not fit in an int"),
                ));
                (Tok::TooBig(fixed), j - i)
            }
        }
    }
}

/// `MZ0911` for a comment after code on a line of a program (`x = 1 // note`, `# note`,
/// `/* note */`). Mzizi's comment is `##` on a line of its own, so the fix moves the text to
/// a line above. It is a `guess`: after code, `//` may also be Python's floor division.
fn trailing_comment(
    bytes: &[char],
    i: usize,
    line_no: u32,
    file: &str,
    diags: &mut Vec<Diagnostic>,
) {
    let marker: String = bytes[i..]
        .iter()
        .take_while(|c| matches!(c, '/' | '#' | '*' | '!'))
        .collect();
    let rest: String = bytes[i + marker.chars().count()..].iter().collect();
    let text = rest.trim().trim_end_matches("*/").trim().to_string();
    let indent: String = bytes.iter().take_while(|c| c.is_whitespace()).collect();
    let code: String = bytes[..i].iter().collect();
    let code = code.trim().to_string();
    let at = Span::single(line_no, (i + 1) as u32, (bytes.len() - i) as u32);
    let whole = Span::single(line_no, 1, bytes.len() as u32);
    let floor = if marker == "//" {
        " (if it was Python's floor division, `/` on two ints truncates toward zero)"
    } else {
        ""
    };
    let comment = format!("## {text}");
    diags.push(
        Diagnostic::error(
            "MZ0911",
            file,
            at,
            format!(
                "`{marker}` does not start a comment in Mzizi — a comment is `##` on a line of its own{floor}"
            ),
        )
        .with_fix(
            whole,
            format!("{indent}{}\n{indent}{code}", comment.trim_end()),
            Confidence::Guess,
        ),
    );
}

/// `MZ0911` for a `/* … */` that closes on its line with code after it (`/* temp */ let x =
/// 1`, `a /* note */ + b`). The code around it is still code, so lexing goes on after the
/// `*/`: returns where. The fix, a `guess`, moves the comment to a line above and joins the
/// code. `None` when there is no such comment at `i`.
fn inline_block_comment(
    bytes: &[char],
    i: usize,
    line_no: u32,
    file: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<usize> {
    if bytes.get(i) != Some(&'/') || bytes.get(i + 1) != Some(&'*') {
        return None;
    }
    let close =
        (i + 2..bytes.len().saturating_sub(1)).find(|&k| bytes[k] == '*' && bytes[k + 1] == '/')?;
    let after: String = bytes[close + 2..].iter().collect();
    if after.trim().is_empty() {
        return None;
    }
    let inner: String = bytes[i + 2..close].iter().collect();
    let before: String = bytes[..i].iter().collect();
    let indent: String = bytes.iter().take_while(|c| c.is_whitespace()).collect();
    let code = match (before.trim(), after.trim()) {
        ("", a) => a.to_string(),
        (b, a) => format!("{b} {a}"),
    };
    let at = Span::single(line_no, (i + 1) as u32, (close + 2 - i) as u32);
    let whole = Span::single(line_no, 1, bytes.len() as u32);
    let comment = format!("## {}", inner.trim());
    diags.push(
        Diagnostic::error(
            "MZ0911",
            file,
            at,
            "`/* … */` does not make a comment in Mzizi — a comment is `##` on a line of its own",
        )
        .with_fix(
            whole,
            format!("{indent}{}\n{indent}{code}", comment.trim_end()),
            Confidence::Guess,
        ),
    );
    Some(close + 2)
}

/// Whether `src` holds a `program` (RFC-0013 §1): its first line that is neither blank nor
/// a comment starts with the word `program`. Only then does the lexer read operators and
/// string escapes, so a component or a service lexes exactly as it did before. A comment
/// here is `##`, or a `//` or `#` line, which a program reports as `MZ0911`.
pub fn is_program(src: &str) -> bool {
    src.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("//"))
        .is_some_and(|l| {
            // `Program` too, which the lexer repairs to `program` for the parser: `Program t`
            // is one `MZ0101`, not a program lexed without its operators.
            l.get(..7).is_some_and(|w| w == "program" || w == "Program")
                && l[7..].chars().next().is_none_or(char::is_whitespace)
        })
}

/// Tokenize `src`. Never fails: bad input produces diagnostics and the lexer keeps going,
/// so the parser always receives a full token stream to recover against. A `program` file
/// ([`is_program`]) also lexes operators and string escapes.
pub fn lex(src: &str, file: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let program = is_program(src);
    lex_with(src, file, program, program)
}

/// Tokenize a piece of a program on its own — the inside of a `{…}` interpolation — with
/// a program's operators. Spans are relative to the piece: line 1, column 1 is its start.
pub fn lex_fragment(text: &str, file: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    lex_with(text, file, true, false)
}

/// `program`: lex a program's operators and escapes. `comments`: also read a line that
/// starts with `//` or `#` as `MZ0911` (a whole program file, not an interpolation).
fn lex_with(src: &str, file: &str, program: bool, comments: bool) -> (Vec<Token>, Vec<Diagnostic>) {
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

            // In a program, a line that starts with another language's comment, `//` or
            // `#`: one `MZ0911`, whose exact fix writes `##`, and the line reads as the
            // doc comment it becomes (RFC-0013 §16).
            if comments
                && tokens.len() == line_first_token
                && ((ch == '#' && bytes.get(i + 1) != Some(&'#'))
                    || (ch == '/' && bytes.get(i + 1) == Some(&'/')))
            {
                // The whole marker: `#`, or `//` with any more `/`s and a `!` (`///`, `//!`).
                let mut marker = 1;
                if ch == '/' {
                    while bytes.get(i + marker) == Some(&'/') {
                        marker += 1;
                    }
                    if bytes.get(i + marker) == Some(&'!') {
                        marker += 1;
                    }
                }
                let written: String = bytes[i..i + marker].iter().collect();
                let at = Span::single(line_no, col, marker as u32);
                // Exact only when the marker is followed by a space or nothing. `#!` may be a
                // shebang, and `#[inline]` or `#define` is code, which `##` would turn into a
                // comment: there the fix is a guess.
                let confidence = if bytes.get(i + marker).is_none_or(|c| c.is_whitespace()) {
                    Confidence::Exact
                } else {
                    Confidence::Guess
                };
                diags.push(
                    Diagnostic::error(
                        "MZ0911",
                        file,
                        at,
                        format!("`{written}` does not start a comment in Mzizi — write `##`"),
                    )
                    .with_fix(at, "##", confidence),
                );
                let text: String = bytes[i + marker..].iter().collect();
                tokens.push(Token {
                    kind: Tok::Doc(text.trim().to_string()),
                    span: Span::single(line_no, col, (bytes.len() - i) as u32),
                });
                i = bytes.len();
                continue;
            }

            // In a program, a `/* … */` with code after it on its line: the code is read.
            if comments
                && let Some(next) = inline_block_comment(&bytes, i, line_no, file, &mut diags)
            {
                i = next;
                continue;
            }

            // In a program, a line that is a `/* … */` comment: `exact` fix `## …`. One that
            // does not close on its line has no fix, since the next lines are not comments.
            if comments
                && tokens.len() == line_first_token
                && ch == '/'
                && bytes.get(i + 1) == Some(&'*')
            {
                let rest: String = bytes[i + 2..].iter().collect();
                let at = Span::single(line_no, col, (bytes.len() - i) as u32);
                let d = Diagnostic::error(
                    "MZ0911",
                    file,
                    at,
                    "`/* … */` does not make a comment in Mzizi — write `##`",
                );
                let inner = rest.trim_end().strip_suffix("*/").map(str::trim);
                let d = match inner {
                    Some(inner) => d.with_fix(
                        at,
                        format!("## {inner}").trim_end().to_string(),
                        Confidence::Exact,
                    ),
                    None => d,
                };
                diags.push(d);
                tokens.push(Token {
                    kind: Tok::Doc(inner.unwrap_or(rest.trim()).to_string()),
                    span: at,
                });
                i = bytes.len();
                continue;
            }

            // In a program, a comment after code on the same line (`MZ0911`, with a `guess`
            // fix that moves it above). The line's code reads as if the comment were not
            // there.
            if comments
                && tokens.len() > line_first_token
                && ((ch == '#' && bytes.get(i + 1) != Some(&'#'))
                    || (ch == '/' && matches!(bytes.get(i + 1), Some('/' | '*'))))
            {
                trailing_comment(&bytes, i, line_no, file, &mut diags);
                break;
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
                    // In a program a backslash escapes the next character (RFC-0013 §3.1),
                    // so `\"` does not close the string. The text keeps the backslash: the
                    // program parser decodes escapes and interpolation together.
                    if program && bytes[j] == '\\' && j + 1 < bytes.len() {
                        text.push(bytes[j]);
                        j += 1;
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
                            // In a program, an escaped quote may be the one meant to end the string,
                            // so where the string ends is the author's call.
                            if program && text.contains("\\\"") {
                                Confidence::Guess
                            } else {
                                Confidence::Exact
                            },
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

            // A number in a program: floats, and RFC-0013 §3.1's malformed forms.
            if program
                && (ch.is_ascii_digit()
                    || (ch == '.'
                        && bytes.get(i + 1).is_some_and(char::is_ascii_digit)
                        && !(i > 0
                            && (bytes[i - 1].is_ascii_alphanumeric()
                                || matches!(bytes[i - 1], '_' | ')' | '"' | '.')))))
            {
                let (kind, len) = program_number(&bytes, i, line_no, file, &mut diags);
                tokens.push(Token {
                    kind,
                    span: Span::single(line_no, col, len as u32),
                });
                i += len;
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
                    Err(_) => {
                        diags.push(Diagnostic::error(
                            "MZ0103",
                            file,
                            Span::single(line_no, col, len),
                            format!("`{text}` does not fit in an int"),
                        ));
                        if program {
                            tokens.push(Token {
                                kind: Tok::BadInt,
                                span: Span::single(line_no, col, len),
                            });
                        }
                    }
                }
                i = j;
                continue;
            }

            // A type constructor spelt with symbols — the TypeScript and Rust priors
            // `list<entry>`, `entry[]`, `[entry]` (RFC-0008 §1). One diagnostic with the
            // Mzizi spelling as an `exact` fix, and the tokens are repaired in place so
            // the parser sees `list(entry)` and reports nothing further (FM-5).
            // In a program `[` is a list literal or an index, so the repair moves to the
            // type parser, where only a type can stand (RFC-0013 §9.1).
            if program && (ch == '[' || ch == ']') {
                tokens.push(Token {
                    kind: if ch == '[' {
                        Tok::LBracket
                    } else {
                        Tok::RBracket
                    },
                    span: Span::single(line_no, col, 1),
                });
                i += 1;
                continue;
            }
            if (ch == '<' || ch == '[')
                && let Some(consumed) =
                    repair_symbolic_type(&bytes, i, line_no, file, &mut tokens, &mut diags)
            {
                i += consumed;
                continue;
            }

            if program
                && let Some(op) = OPERATORS.iter().find(|op| {
                    op.chars()
                        .enumerate()
                        .all(|(k, c)| bytes.get(i + k) == Some(&c))
                })
            {
                let len = op.chars().count();
                tokens.push(Token {
                    kind: Tok::Op(op),
                    span: Span::single(line_no, col, len as u32),
                });
                i += len;
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

/// A number literal as a diagnostic quotes it: whole up to 24 characters, else its first 20
/// and `…`, so a `say` stays within RFC-0001's 200 characters however long the literal is.
fn short_literal(written: &str) -> String {
    if written.chars().count() <= 24 {
        written.to_string()
    } else {
        let head: String = written.chars().take(20).collect();
        format!("{head}…")
    }
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
