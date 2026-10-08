//! Parsing RFC-0013 §7's control flow in a function body: `else when` (read by
//! [`P::when`](super::P), with its idioms here), `match`, `for each`, `while`, `break`,
//! `continue`, `when` and `match` used as values (§7.4), and the `enum`s a `match` is over.
//!
//! The rules are the parent module's: one statement per line, `end`-delimited blocks, one
//! diagnostic per true error, and spellings from other languages (`elif`, `else if`,
//! `switch`, `default:`, `case _`, `_ =>`, `for x in xs`, `loop`, `range(n)`) repaired in
//! the tree as they are reported (`MZ0933`, `MZ0934`), so the checker sees what the `exact`
//! fix would produce. Each block is one level of the shared nesting budget.

use super::{
    Confidence, Diagnostic, Expr, ExprKind, OPENERS, P, PROGRAM_NESTING, Span, Stmt, StmtKind,
    Stop, Tok, canonical, describe, join, word,
};
use crate::expr::{Arm, ElseArm};
use crate::program::{ElseWhen, EnumDecl};

/// Words that start a statement, which a branch of a `when` or `match` used as a value
/// cannot hold (`MZ0932`): its branch is one line, an expression.
const STATEMENT_WORDS: &[&str] = &[
    "let", "var", "return", "when", "if", "match", "for", "while", "break", "continue", "loop",
    "switch",
];

/// What a `case` line turned out to be.
enum Header {
    /// `case v1 v2 …`: the values and the line's span.
    Values(Vec<Expr>, Span),
    /// An `else` spelt from another language (`default:`, `case _`, `_ =>`), repaired.
    Else(Span),
}

impl P {
    /// Whether a statement line starting with `w` is one this module reads.
    pub(super) fn control_statement(&self, w: &str) -> bool {
        let line_end = |t: &Tok| matches!(t, Tok::Newline | Tok::Eof);
        match w {
            "while" | "for" | "match" | "break" | "continue" => true,
            // Not a word of the language: the idiom only where no `fn` of that name is
            // being called.
            "switch" => {
                !(self.fn_names.iter().any(|n| n == "switch")
                    && matches!(self.peek_at(1), Tok::LParen))
                    && !matches!(
                        self.peek_at(1),
                        Tok::Equals | Tok::Dot | Tok::Op("+=" | "-=" | "*=" | "/=" | "++" | "--")
                    )
            }
            "loop" => {
                line_end(self.peek_at(1))
                    || (matches!(self.peek_at(1), Tok::Colon) && line_end(self.peek_at(2)))
            }
            "do" => line_end(self.peek_at(1)),
            _ => false,
        }
    }

    /// One control-flow statement and its block. The cursor is on its first word.
    pub(super) fn control(&mut self, at: Span, fn_name: &str) -> Option<Stmt> {
        let w = word(self.peek()).unwrap_or_default().to_string();
        match w.as_str() {
            "while" => self.nested(at, |p| p.while_block(at, fn_name, false)),
            "loop" => self.nested(at, |p| p.while_block(at, fn_name, true)),
            "for" => self.nested(at, |p| p.for_block(at, fn_name)),
            "match" | "switch" => self.nested(at, |p| p.match_block(at, fn_name)),
            "do" => {
                self.err(
                    "MZ0934",
                    at,
                    "`do … while` is not a Mzizi loop — write `while <condition>` … `end`, or `while true` with a `break`",
                );
                self.skipped.push((at.start_line, at.start_line));
                self.recover_line();
                self.skip_bounded(true);
                None
            }
            _ => {
                self.bump();
                self.finish_line(&format!("`{w}`"));
                let kind = if w == "break" {
                    StmtKind::Break
                } else {
                    StmtKind::Continue
                };
                Some(Stmt {
                    kind,
                    span: at,
                    last_line: at.start_line,
                })
            }
        }
    }

    /// Read a block one nesting level deeper, or report `MZ0411` and skip it whole.
    fn nested(&mut self, at: Span, f: impl FnOnce(&mut P) -> Option<Stmt>) -> Option<Stmt> {
        if self.nest >= PROGRAM_NESTING {
            let line_end = self.line_end_span();
            self.report_too_deep(join(at, line_end), "this block");
            self.failed = true;
            self.skip_block();
            return None;
        }
        self.nest += 1;
        let stmt = f(self);
        self.nest -= 1;
        stmt
    }

    /// The rest of a line, and the block it opened, skipped as one error already reported.
    fn skip_open_block(&mut self) {
        let line = self.span().start_line;
        self.skipped.push((line, line));
        self.recover_line();
        self.skip_bounded(false);
    }

    /// Skip the body of a block whose first line is passed, through its `end`, but never
    /// past a line no block in a function holds (`end fn`, `end program`, `fn`): a loop
    /// written with braces has no `end`, and would otherwise swallow the function's. With
    /// `until_while`, a `while` line at the block's own depth ends it too, as it ends
    /// `do … while`.
    fn skip_bounded(&mut self, until_while: bool) {
        let mut depth = 1usize;
        let mut range: Option<(u32, u32)> = None;
        while depth > 0 {
            let line = self.span().start_line;
            match self.peek() {
                Tok::Eof | Tok::Keyword("fn") => break,
                Tok::Keyword("end") if matches!(word(self.peek_at(1)), Some("fn" | "program")) => {
                    break;
                }
                Tok::Keyword("end") => depth -= 1,
                Tok::Ident(w) if until_while && depth == 1 && w == "while" => depth = 0,
                _ if self.line_opens_block() => depth += 1,
                _ => {}
            }
            range = Some((range.map_or(line, |(first, _)| first), line));
            self.recover_line();
        }
        if let Some(r) = range {
            self.skipped.push(r);
        }
    }

    /// `MZ0204`: a block still open when a line that cannot be inside it arrives.
    fn unclosed(&mut self, what: &str, open_at: Span, end_at: Span) {
        let indent = " ".repeat(open_at.start_col.saturating_sub(1) as usize);
        self.err_fix(
            "MZ0204",
            end_at,
            format!(
                "the `{what}` opened on line {} is still open — add `end` before this line",
                open_at.start_line
            ),
            Span::single(end_at.start_line, 1, 0),
            format!("{indent}end\n"),
            Confidence::Guess,
        );
    }

    // ------------------------------------------------------------ else when

    /// `MZ0933`: `elif` (Python) or the `if` of `else if` (TypeScript, Rust), repaired to
    /// `else when` in place. `at` is the word to replace.
    pub(super) fn else_when_idiom(&mut self, at: Span, written: &str) {
        let (say, replace) = if written == "elif" {
            ("`elif` is Python's — Mzizi writes `else when`", "else when")
        } else {
            ("`else if` is not Mzizi's — write `else when`", "when")
        };
        self.diags
            .push(Diagnostic::error("MZ0933", &self.file, at, say).with_fix(
                at,
                replace,
                Confidence::Exact,
            ));
    }

    /// The condition and branch of an `else when`, the cursor past `when` (or `elif`).
    pub(super) fn else_when(&mut self, else_at: Span, fn_name: &str) -> (ElseWhen, Stop) {
        let cond = self.expr();
        self.trailing_block_punctuation();
        self.finish_line("an `else when` condition");
        let (body, stop) = self.stmts(true, fn_name);
        let span = join(else_at, cond.span);
        (ElseWhen { cond, body, span }, stop)
    }

    // ------------------------------------------------------------ loops

    /// `while <cond>` … `end`, or Rust's `loop`, repaired to `while true` (`MZ0934`).
    fn while_block(&mut self, at: Span, fn_name: &str, is_loop: bool) -> Option<Stmt> {
        let kw = self.bump().span;
        let cond = if is_loop {
            self.err_fix(
                "MZ0934",
                kw,
                "`loop` is Rust's — the Mzizi loop that runs until a `break` is `while true`",
                kw,
                "while true",
                Confidence::Exact,
            );
            self.failed = false;
            Expr {
                kind: ExprKind::Bool(true),
                span: kw,
            }
        } else {
            self.expr()
        };
        self.trailing_block_punctuation();
        self.finish_line("a `while` condition");
        let (body, last_line) = self.loop_body(at, "while", fn_name);
        Some(Stmt {
            span: join(at, cond.span),
            kind: StmtKind::While { cond, body },
            last_line,
        })
    }

    /// `for each <name> in <source>` … `end`, with Python's and TypeScript's spellings
    /// repaired (`MZ0934`).
    fn for_block(&mut self, at: Span, fn_name: &str) -> Option<Stmt> {
        let for_at = self.bump().span;
        if matches!(self.peek(), Tok::LParen) {
            let typescript = matches!(word(self.peek_at(1)), Some("const" | "let" | "var"))
                && matches!(self.peek_at(2), Tok::Ident(_))
                && matches!(word(self.peek_at(3)), Some("of" | "in"));
            if !typescript {
                let line_end = self.line_end_span();
                self.err(
                    "MZ0934",
                    join(for_at, line_end),
                    "a C-style `for (…; …; …)` is not a Mzizi loop — a counted loop is `for each i in range(a, to = b)`, and any other loop is `while`",
                );
                self.skip_open_block();
                return None;
            }
            self.bump();
            self.bump();
            let Tok::Ident(name) = self.peek().clone() else {
                return None;
            };
            let name_span = self.bump().span;
            // TypeScript's `for … of` iterates values, as `for each` does; its `for … in`
            // iterates keys or indices, so that rewrite is only a guess.
            let of = self.is_word("of");
            self.bump();
            let source = self.expr();
            if matches!(self.peek(), Tok::RParen) {
                let close = self.bump().span;
                let fixed = format!("for each {name} in {}", canonical(&source));
                let (say, c) = if of {
                    (
                        format!("`for (… of …)` is TypeScript's — write `{fixed}`"),
                        Confidence::Exact,
                    )
                } else {
                    (
                        format!(
                            "`for (… in …)` is TypeScript's, and iterates keys or indices — `for each` iterates values: `{fixed}` if that is what this loop means"
                        ),
                        Confidence::Guess,
                    )
                };
                self.err_fix(
                    "MZ0934",
                    join(for_at, close),
                    say,
                    join(for_at, close),
                    fixed,
                    c,
                );
                self.failed = false;
            } else if !self.failed {
                let s = self.span();
                let found = describe(self.peek());
                self.err(
                    "MZ0917",
                    s,
                    format!("expected `)` to close `for (`, found {found}"),
                );
            }
            return self.for_tail(at, name, name_span, source, fn_name);
        }
        if self.is_word("each") {
            self.bump();
        } else if matches!(self.peek(), Tok::Ident(_))
            && matches!(self.peek_at(1), Tok::Keyword("in"))
        {
            let name_at = self.span();
            self.err_fix(
                "MZ0934",
                join(for_at, name_at),
                "`for x in xs` is Python's — Mzizi writes `for each x in xs`",
                Span::single(name_at.start_line, name_at.start_col, 0),
                "each ",
                Confidence::Exact,
            );
            self.failed = false;
        } else {
            let s = self.span();
            let found = describe(self.peek());
            self.err(
                "MZ0917",
                s,
                format!("expected `each` after `for` — `for each <name> in <list>`, found {found}"),
            );
            self.skip_open_block();
            return None;
        }
        let (name, name_span) = match self.peek().clone() {
            Tok::Ident(n) => (n, self.bump().span),
            other => {
                let s = self.span();
                self.err(
                    "MZ0917",
                    s,
                    format!(
                        "`for each` needs a snake_case name for each value, found {}",
                        describe(&other)
                    ),
                );
                self.skip_open_block();
                return None;
            }
        };
        if !matches!(self.peek(), Tok::Keyword("in")) {
            let s = self.span();
            let found = describe(self.peek());
            self.err(
                "MZ0917",
                s,
                format!("expected `in` after `for each {name}`, found {found}"),
            );
            self.skip_open_block();
            return None;
        }
        self.bump();
        let source = self.expr();
        self.for_tail(at, name, name_span, source, fn_name)
    }

    /// The end of a `for each` line, from its source on, and its body.
    fn for_tail(
        &mut self,
        at: Span,
        name: String,
        name_span: Span,
        mut source: Expr,
        fn_name: &str,
    ) -> Option<Stmt> {
        // Python's `range(n)`: Mzizi's range names both ends.
        if let ExprKind::Call {
            name: callee, args, ..
        } = &mut source.kind
            && callee == "range"
            && args.len() == 1
            && !self.failed
        {
            let fixed = format!("range(0, to = {})", canonical(&args[0]));
            let zero = Expr {
                kind: ExprKind::Int(0),
                span: args[0].span,
            };
            args.insert(0, zero);
            self.err_fix(
                "MZ0934",
                source.span,
                format!("`range(n)` is Python's — Mzizi's range names both ends: `{fixed}`"),
                source.span,
                fixed,
                Confidence::Exact,
            );
            self.failed = false;
        }
        self.trailing_block_punctuation();
        self.finish_line("a `for each` line");
        let (body, last_line) = self.loop_body(at, "for each", fn_name);
        Some(Stmt {
            span: join(at, source.span),
            kind: StmtKind::For {
                name,
                name_span,
                source,
                body,
            },
            last_line,
        })
    }

    /// A loop's statements, through its `end`. Returns them and the last line.
    fn loop_body(&mut self, at: Span, what: &str, fn_name: &str) -> (Vec<Stmt>, u32) {
        let (mut body, mut stop) = self.stmts(true, fn_name);
        loop {
            match stop {
                Stop::End(s) | Stop::Abrupt(s) | Stop::Eof(s) => return (body, s.start_line),
                Stop::Else(s) | Stop::Elif(s) => {
                    self.err(
                        "MZ0917",
                        s,
                        format!("`else` with no `when` open — a `{what}` loop has no `else`"),
                    );
                    self.recover_line();
                    let (more, next) = self.stmts(true, fn_name);
                    body.extend(more);
                    stop = next;
                }
                Stop::EndFn(s) | Stop::Case(s) => {
                    self.unclosed(what, at, s);
                    return (body, s.start_line);
                }
            }
        }
    }

    // ------------------------------------------------------------ match

    /// Whether the current line is a `case` line: `case …`, or `default` / `_ =>`.
    pub(super) fn at_case_line(&self) -> bool {
        match self.peek() {
            Tok::Keyword("case") => true,
            Tok::Ident(w) if w == "default" => matches!(
                (self.peek_at(1), self.peek_at(2)),
                (Tok::Newline | Tok::Eof, _) | (Tok::Colon, Tok::Newline | Tok::Eof)
            ),
            Tok::Ident(w) if w == "_" => matches!(self.peek_at(1), Tok::Op("=>")),
            _ => false,
        }
    }

    /// `default` alone on its line, with no `:`.
    fn at_bare_default(&self) -> bool {
        matches!(self.peek(), Tok::Ident(w) if w == "default")
            && matches!(self.peek_at(1), Tok::Newline | Tok::Eof)
    }

    /// `match <expr>` … `end` as a statement, or JavaScript's `switch`, repaired
    /// (`MZ0933`).
    fn match_block(&mut self, at: Span, fn_name: &str) -> Option<Stmt> {
        let switch = self.is_word("switch");
        let kw = self.bump().span;
        if switch {
            self.diags.push(
                Diagnostic::error(
                    "MZ0933",
                    &self.file,
                    kw,
                    "`switch` is not Mzizi's — write `match`",
                )
                .with_fix(kw, "match", Confidence::Exact),
            );
        }
        let scrutinee = self.expr();
        self.trailing_block_punctuation();
        self.finish_line("a `match` value");
        self.match_open += 1;
        let (arms, otherwise, last_line) = self.arms(at, |p| {
            let (body, stop) = p.stmts(true, fn_name);
            let last = body.last().map(|s| s.last_line);
            (body, stop, last)
        });
        self.match_open -= 1;
        Some(Stmt {
            span: join(at, scrutinee.span),
            kind: StmtKind::Match {
                scrutinee,
                arms,
                otherwise,
            },
            last_line,
        })
    }

    /// A `match`'s cases and `else`, through its `end`, with `body` reading each branch.
    /// Returns them and the last line.
    fn arms<B>(
        &mut self,
        at: Span,
        mut body: impl FnMut(&mut P) -> (B, Stop, Option<u32>),
    ) -> (Vec<Arm<B>>, Option<ElseArm<B>>, u32) {
        let mut arms = Vec::new();
        let mut otherwise: Option<ElseArm<B>> = None;
        let mut stop = self.skip_to_first_case(false);
        loop {
            match stop {
                Stop::Case(_) => match self.case_header() {
                    Header::Values(values, span) => {
                        let (b, s, last) = body(self);
                        arms.push(Arm {
                            values,
                            span,
                            body: b,
                            last_line: last.unwrap_or(span.start_line),
                            after_else: otherwise.is_some(),
                        });
                        stop = s;
                    }
                    Header::Else(span) => stop = self.else_arm(span, &mut otherwise, &mut body),
                },
                Stop::Else(span) => {
                    self.failed = false;
                    self.trailing_block_punctuation();
                    self.finish_line("`else`");
                    stop = self.else_arm(span, &mut otherwise, &mut body);
                }
                Stop::Elif(span) => {
                    self.err(
                        "MZ0917",
                        span,
                        "`elif` in a `match` — a `match` holds `case` lines and one `else`",
                    );
                    self.recover_line();
                    stop = self.skip_to_first_case(true);
                }
                Stop::End(closer) => return (arms, otherwise, closer.start_line),
                Stop::EndFn(s) | Stop::Abrupt(s) | Stop::Eof(s) => {
                    self.unclosed("match", at, s);
                    return (arms, otherwise, s.start_line);
                }
            }
        }
    }

    /// An `else` branch of a `match`; a second one is `MZ0917`.
    fn else_arm<B>(
        &mut self,
        span: Span,
        otherwise: &mut Option<ElseArm<B>>,
        body: &mut impl FnMut(&mut P) -> (B, Stop, Option<u32>),
    ) -> Stop {
        let (b, s, last) = body(self);
        if otherwise.is_some() {
            self.err(
                "MZ0917",
                span,
                "a second `else` in one `match` — a `match` has one `else`, last",
            );
        } else {
            *otherwise = Some(ElseArm {
                span,
                body: b,
                last_line: last.unwrap_or(span.start_line),
            });
        }
        s
    }

    /// The lines between a `match` line and its first `case`, which hold nothing. With
    /// `quiet`, they are the rest of a mistake already reported (an `elif` in a `match`),
    /// and are skipped without a diagnostic each.
    fn skip_to_first_case(&mut self, quiet: bool) -> Stop {
        loop {
            self.failed = false;
            self.skip_newlines();
            let at = self.span();
            match self.peek().clone() {
                Tok::Eof => return Stop::Eof(at),
                Tok::Keyword("end") => match word(self.peek_at(1)) {
                    Some("program") => return Stop::Abrupt(at),
                    Some("fn") => return Stop::EndFn(at),
                    _ => return Stop::End(self.block_end(true, "")),
                },
                Tok::Keyword("else") => {
                    self.bump();
                    return Stop::Else(at);
                }
                Tok::Keyword("fn") => return Stop::Abrupt(at),
                Tok::Doc(_) => self.recover_line(),
                _ if self.at_case_line() => return Stop::Case(at),
                _ => {
                    if quiet {
                        self.skipped.push((at.start_line, at.start_line));
                    } else {
                        self.err(
                            "MZ0917",
                            at,
                            "a `match` holds `case` lines and one `else` — this line is outside every case",
                        );
                    }
                    if self.line_opens_block() {
                        self.skip_block();
                    } else {
                        self.recover_line();
                    }
                }
            }
        }
    }

    /// A `case` line, the cursor on `case` (or `default`, or `_`).
    fn case_header(&mut self) -> Header {
        let at = self.span();
        match self.peek().clone() {
            Tok::Ident(w) if w == "default" => {
                self.bump();
                let mut end = at;
                if matches!(self.peek(), Tok::Colon) {
                    end = self.bump().span;
                }
                self.else_idiom(
                    join(at, end),
                    "`default` is not Mzizi's — write `else`",
                    "else",
                );
                self.finish_line("`else`");
                Header::Else(at)
            }
            Tok::Ident(_) => {
                // `_ =>`, Rust's catch-all arm. What follows the arrow on its line is the
                // first line of the branch, so the fix starts it on a line of its own.
                self.bump();
                let arrow = self.bump().span;
                if self.at_line_end() {
                    self.else_idiom(
                        join(at, arrow),
                        "`_ =>` is Rust's catch-all arm — Mzizi writes `else`",
                        "else",
                    );
                    self.finish_line("`else`");
                } else {
                    let next = self.span();
                    let indent = " ".repeat(at.start_col.saturating_sub(1) as usize + 2);
                    let span = Span {
                        start_line: at.start_line,
                        start_col: at.start_col,
                        end_line: next.start_line,
                        end_col: next.start_col,
                    };
                    self.else_idiom(
                        span,
                        "`_ =>` is Rust's catch-all arm — Mzizi writes `else`, with the branch on the lines after it",
                        &format!("else\n{indent}"),
                    );
                }
                Header::Else(at)
            }
            _ => {
                let case_at = self.bump().span;
                if self.is_word("_")
                    && matches!(
                        (self.peek_at(1), self.peek_at(2)),
                        (Tok::Newline | Tok::Eof, _) | (Tok::Colon, Tok::Newline | Tok::Eof)
                    )
                {
                    let mut end = self.bump().span;
                    if matches!(self.peek(), Tok::Colon) {
                        end = self.bump().span;
                    }
                    self.else_idiom(
                        join(case_at, end),
                        "`case _` is Python's catch-all — Mzizi writes `else`",
                        "else",
                    );
                    self.finish_line("`else`");
                    return Header::Else(case_at);
                }
                let mut values = Vec::new();
                while !self.at_line_end()
                    && !(matches!(self.peek(), Tok::Colon)
                        && matches!(self.peek_at(1), Tok::Newline | Tok::Eof))
                {
                    values.push(self.unary_expr());
                    if self.failed {
                        break;
                    }
                }
                if values.is_empty() && !self.failed {
                    self.err(
                        "MZ0917",
                        case_at,
                        "`case` needs at least one value: `case <value> …`",
                    );
                }
                let span = values.last().map_or(case_at, |v| join(case_at, v.span));
                self.trailing_block_punctuation();
                self.finish_line("a `case` line");
                Header::Values(values, span)
            }
        }
    }

    /// `MZ0933` on an `else` spelt from another language, repaired in place.
    fn else_idiom(&mut self, span: Span, say: &str, replace: &str) {
        self.diags
            .push(Diagnostic::error("MZ0933", &self.file, span, say).with_fix(
                span,
                replace,
                Confidence::Exact,
            ));
        self.failed = false;
    }

    // ------------------------------------------------------------ blocks as values

    /// The value of a `let`, a `var`, an assignment or a `return`: an expression, or a
    /// `when` or `match` block used as a value (RFC-0013 §7.4).
    pub(super) fn value(&mut self) -> Expr {
        let if_value = self.is_word("if") && !self.fn_names.iter().any(|n| n == "if");
        if self.is_word("when") || if_value {
            return self.block_value_nested(|p| p.value_when());
        }
        if matches!(self.peek(), Tok::Keyword("match")) && !self.keyword_name("match") {
            return self.block_value_nested(|p| p.value_match());
        }
        self.expr()
    }

    /// Finish a statement line, unless a block used as a value already finished it.
    pub(super) fn finish_value_line(&mut self, what: &str) {
        if self.block_value.is_none() {
            self.finish_line(what);
        }
    }

    fn block_value_nested(&mut self, f: impl FnOnce(&mut P) -> Expr) -> Expr {
        let at = self.span();
        if self.nest >= PROGRAM_NESTING {
            let line_end = self.line_end_span();
            self.report_too_deep(join(at, line_end), "this block");
            self.failed = true;
            let last = self.skip_block();
            self.block_value = Some((line_end, last));
            return self.error_expr(at);
        }
        self.nest += 1;
        let e = f(self);
        self.nest -= 1;
        e
    }

    /// `MZ0932`: a `when` or `match` where a value cannot be a block — inside a larger
    /// expression or a call's arguments. The rest of the line is not read, and once it is
    /// finished the block's lines are skipped through its `end`.
    pub(super) fn misplaced_block_value(&mut self, k: &str, at: Span) -> Expr {
        let line_end = self.line_end_span();
        if !self.failed {
            self.err(
                "MZ0932",
                join(at, line_end),
                format!(
                    "a `{k}` used as a value is the whole value of a `let`, a `var`, an assignment or a `return` (RFC-0013 §7.4), never part of a larger expression — bind it with `let` first and use the name"
                ),
            );
        }
        self.failed = true;
        while !self.at_line_end() {
            self.bump();
        }
        self.pending_skip = true;
        self.error_expr(at)
    }

    /// `when c` … `else when c2` … `else` … `end` used as a value. The cursor is on `when`
    /// (or the `if` it stands for, `MZ0407`).
    fn value_when(&mut self) -> Expr {
        let at = self.span();
        if self.is_word("if") {
            self.err_fix(
                "MZ0407",
                at,
                "Mzizi's conditional is `when` — `if` is not a word of the language",
                at,
                "when",
                Confidence::Exact,
            );
            self.failed = false;
        }
        self.bump();
        let cond = self.expr();
        self.trailing_block_punctuation();
        let header_end = self.prev();
        self.finish_line("a `when` condition");
        let (first, mut stop) = self.value_branch(header_end, "when");
        let mut arms = vec![(cond, first)];
        let mut otherwise = None;
        let mut closed = true;
        let end_line = loop {
            match stop {
                Stop::Else(else_at) if self.is_word("when") || self.is_word("if") => {
                    self.failed = false;
                    if self.is_word("if") {
                        let if_at = self.span();
                        self.else_when_idiom(if_at, "if");
                    }
                    self.bump();
                    stop = self.value_else_when(else_at, &mut arms);
                }
                Stop::Elif(elif_at) => {
                    self.failed = false;
                    self.else_when_idiom(elif_at, "elif");
                    stop = self.value_else_when(elif_at, &mut arms);
                }
                Stop::Else(else_at) => {
                    self.failed = false;
                    self.trailing_block_punctuation();
                    self.finish_line("`else`");
                    let (v, s) = self.value_branch(else_at, "when");
                    otherwise = Some(Box::new(v));
                    match s {
                        Stop::End(c) => break c.start_line,
                        Stop::Else(second) | Stop::Elif(second) => {
                            self.err(
                                "MZ0917",
                                second,
                                "a second `else` in one `when` — a `when` has one `else`, last",
                            );
                            self.recover_line();
                            break self.skip_lines(1);
                        }
                        Stop::EndFn(s) | Stop::Case(s) => {
                            self.unclosed_when(at, s);
                            break s.start_line;
                        }
                        Stop::Abrupt(s) | Stop::Eof(s) => break s.start_line,
                    }
                }
                Stop::End(c) => break c.start_line,
                Stop::EndFn(s) | Stop::Case(s) => {
                    closed = false;
                    self.unclosed_when(at, s);
                    break s.start_line;
                }
                Stop::Abrupt(s) | Stop::Eof(s) => {
                    closed = false;
                    break s.start_line;
                }
            }
        };
        if otherwise.is_none() && closed {
            self.err(
                "MZ0932",
                join(at, header_end),
                "a `when` used as a value needs an `else`, so that it has a value on every path (RFC-0013 §7.4)",
            );
        }
        self.block_value = Some((header_end, end_line));
        Expr {
            kind: ExprKind::When { arms, otherwise },
            span: join(at, header_end),
        }
    }

    /// One `else when` of a `when` used as a value, the cursor on its condition.
    fn value_else_when(&mut self, else_at: Span, arms: &mut Vec<(Expr, Expr)>) -> Stop {
        let cond = self.expr();
        self.trailing_block_punctuation();
        self.finish_line("an `else when` condition");
        let (v, s) = self.value_branch(else_at, "when");
        arms.push((cond, v));
        s
    }

    /// `match s` … `end` used as a value. The cursor is on `match`.
    fn value_match(&mut self) -> Expr {
        let at = self.bump().span;
        let scrutinee = self.expr();
        self.trailing_block_punctuation();
        let header_end = self.prev();
        self.finish_line("a `match` value");
        self.match_open += 1;
        let (arms, otherwise, end_line) = self.arms(at, |p| {
            let header = p.prev();
            let (v, stop) = p.value_branch(header, "match");
            let last = Some(v.span.end_line);
            (v, stop, last)
        });
        self.match_open -= 1;
        self.block_value = Some((header_end, end_line));
        Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
                otherwise: otherwise.map(Box::new),
            },
            span: join(at, header_end),
        }
    }

    /// One branch of a block used as a value: exactly one line, an expression
    /// (`MZ0932` otherwise). Returns it and how the branch ended.
    fn value_branch(&mut self, header: Span, what: &str) -> (Expr, Stop) {
        let mut values: Vec<Expr> = Vec::new();
        let mut bad = false;
        loop {
            self.failed = false;
            self.skip_newlines();
            let at = self.span();
            let stop = match self.peek().clone() {
                Tok::Eof => Some(Stop::Eof(at)),
                Tok::Keyword("end") => Some(match word(self.peek_at(1)) {
                    Some("program") => Stop::Abrupt(at),
                    Some("fn") => Stop::EndFn(at),
                    _ => Stop::End(self.block_end(true, "")),
                }),
                Tok::Keyword("else") => {
                    self.bump();
                    Some(Stop::Else(at))
                }
                Tok::Ident(w) if w == "elif" => {
                    self.bump();
                    Some(Stop::Elif(at))
                }
                Tok::Keyword("fn") => Some(Stop::Abrupt(at)),
                // A bare `default` where the branch's value belongs is that value (a binding
                // may be named so); JavaScript's `default:` keeps its colon.
                _ if self.match_open > 0
                    && self.at_case_line()
                    && !(values.is_empty() && self.at_bare_default()) =>
                {
                    Some(Stop::Case(at))
                }
                Tok::Doc(_) => {
                    self.recover_line();
                    None
                }
                t => {
                    let starts = word(&t).filter(|w| STATEMENT_WORDS.contains(w));
                    match starts {
                        Some(w) => {
                            if !bad {
                                self.err(
                                    "MZ0932",
                                    at,
                                    format!(
                                        "a branch of a `{what}` used as a value is one line, an expression, and `{w}` starts a statement — a branch that needs statements needs a `fn`"
                                    ),
                                );
                            }
                            bad = true;
                            if self.line_opens_block() {
                                self.skip_block();
                            } else {
                                self.recover_line();
                            }
                        }
                        None => {
                            let e = self.expr();
                            self.finish_line("a branch's value");
                            values.push(e);
                        }
                    }
                    None
                }
            };
            if let Some(stop) = stop {
                let value = match values.len() {
                    1 => values.pop(),
                    0 => {
                        // A block cut off by `end fn` or the end of the file is reported as
                        // unclosed; its empty last branch is part of that one mistake.
                        let cut_off =
                            matches!(stop, Stop::EndFn(_) | Stop::Abrupt(_) | Stop::Eof(_));
                        if !bad && !cut_off {
                            self.err(
                                "MZ0932",
                                header,
                                format!(
                                    "this branch of a `{what}` used as a value has no value line — each branch is one line, an expression"
                                ),
                            );
                        }
                        None
                    }
                    _ => {
                        if !bad {
                            self.err(
                                "MZ0932",
                                values[1].span,
                                format!(
                                    "a branch of a `{what}` used as a value is exactly one line, its value — a branch that needs statements needs a `fn`"
                                ),
                            );
                        }
                        None
                    }
                };
                let value = value.unwrap_or_else(|| self.error_expr(header));
                return (value, stop);
            }
        }
    }

    /// Whether the line at the cursor opens a block that a skip must count against the
    /// `end`s: a keyword that opens one, a word that is not a keyword (`while`, `loop`, `do`,
    /// `switch`, `record`, …) where it is not used as a name, or a `when` or `match` used as
    /// a value.
    pub(super) fn line_opens_block(&self) -> bool {
        let opens = match self.peek() {
            Tok::Keyword(k) => OPENERS.contains(k),
            Tok::Ident(w) if matches!(w.as_str(), "do" | "loop" | "switch") => {
                self.control_statement(w)
            }
            // Used as a name: assigned, or called as a declared `fn`.
            Tok::Ident(w) if OPENERS.contains(&w.as_str()) => {
                !matches!(
                    self.peek_at(1),
                    Tok::Equals | Tok::Dot | Tok::Op("+=" | "-=" | "*=" | "/=" | "++" | "--")
                ) && !(self.fn_names.iter().any(|n| n == w)
                    && matches!(self.peek_at(1), Tok::LParen))
            }
            _ => false,
        };
        opens || self.line_holds_block_value()
    }

    /// Whether the line at the cursor holds a `when` or `match` used as a value (after an
    /// `=` or a `return`), which opens a block a skip must count.
    pub(super) fn line_holds_block_value(&self) -> bool {
        let mut prev: Option<&Tok> = None;
        for t in self.toks[self.pos..].iter().map(|t| &t.kind) {
            if matches!(t, Tok::Newline | Tok::Eof) {
                break;
            }
            let block = matches!(t, Tok::Keyword("when" | "match"))
                || matches!(t, Tok::Ident(w) if w == "if");
            let after = matches!(prev, Some(Tok::Equals))
                || matches!(prev, Some(Tok::Ident(w)) if w == "return");
            if block && after {
                return true;
            }
            prev = Some(t);
        }
        false
    }

    // ------------------------------------------------------------ enum

    /// `enum <name>`, one variant per line, then a bare `end`. The cursor is on `enum`.
    pub(super) fn enum_decl(&mut self) -> Option<EnumDecl> {
        let enum_at = self.bump().span;
        let (name, name_span) = match self.peek().clone() {
            Tok::Ident(n) => (n, self.bump().span),
            other => {
                self.err(
                    "MZ0301",
                    enum_at,
                    format!(
                        "`enum` needs a snake_case name, e.g. `enum shape`, found {}",
                        describe(&other)
                    ),
                );
                self.skip_open_block();
                return None;
            }
        };
        if !self.at_line_end() {
            let at = self.span();
            self.err(
                "MZ0310",
                at,
                format!(
                    "{} is left over after `enum {name}` — one declaration per line",
                    describe(self.peek())
                ),
            );
        }
        self.recover_line();
        let mut variants = Vec::new();
        loop {
            self.failed = false;
            self.skip_newlines();
            let at = self.span();
            match self.peek().clone() {
                Tok::Keyword("end") if word(self.peek_at(1)) != Some("program") => {
                    let end_at = self.bump().span;
                    if !self.at_line_end() {
                        let mut last = end_at;
                        while !self.at_line_end() {
                            last = self.bump().span;
                        }
                        self.err_fix(
                            "MZ0206",
                            end_at,
                            "an `enum` closes with a bare `end`",
                            join(end_at, last),
                            "end",
                            Confidence::Exact,
                        );
                    }
                    self.recover_line();
                    break;
                }
                Tok::Eof | Tok::Keyword("end" | "fn") => {
                    self.unclosed("enum", enum_at, at);
                    break;
                }
                Tok::Doc(_) => self.recover_line(),
                Tok::Ident(v) => {
                    self.bump();
                    variants.push((v.clone(), at));
                    if !self.at_line_end() {
                        let line_end = self.line_end_span();
                        self.err(
                            "MZ0919",
                            join(at, line_end),
                            format!(
                                "a column on a variant (`{v} label \"…\"`) is designed for a program's enums (RFC-0013 §14.2) but not built yet — a program's enum lists one variant name per line"
                            ),
                        );
                        self.skipped.push((at.start_line, at.start_line));
                    }
                    self.recover_line();
                }
                other => {
                    self.err(
                        "MZ0301",
                        at,
                        format!(
                            "expected a variant name, one per line, found {}",
                            describe(&other)
                        ),
                    );
                    self.recover_line();
                }
            }
        }
        Some(EnumDecl {
            name,
            name_span,
            variants,
        })
    }
}
