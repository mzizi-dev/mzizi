//! Parsing a `program` (RFC-0013): the program block, `fn`s, statements and expressions.
//!
//! A program has its own cursor rather than sharing the component parser's, because its
//! grammar shares almost nothing with a component's beyond the lexer and the `end` echo.
//! The rules are the same ones: one statement per line, `end`-delimited blocks, one
//! diagnostic per true error, and the next line is always a resynchronisation point.
//!
//! Spellings from other languages (`==`, `&&`, `->`, `def`, `console.log`, `x = 1` with no
//! binding) are repaired in the tree as they are reported, so the checker sees the program
//! the `exact` fix would produce and reports nothing more about that line (RFC-0013 §16).
//! Forms RFC-0013 designs that are not built yet (methods on text, lists, a program's
//! `contract`, …) are one `MZ0919` each, naming the form, and their block is skipped.
//! C4's control flow (§7: `else when`, `match`, `for each`, `while`, `break`, `continue`,
//! `when` and `match` as values, and the `enum`s a `match` needs) is read by [`control`],
//! this module's child.

use std::rc::Rc;

use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::expr::{BinOp, Expr, ExprKind, TextPart, Ty, UnOp, canonical, canonical_text, intern};
use crate::lex::{Tok, Token, lex_fragment};
use crate::program::{FnDecl, Param, Program, Stmt, StmtKind, TypeRef};

use super::MAX_NESTING;

mod control;
mod errors;

use errors::Known;

/// How deep a program's blocks and expressions may nest, together: the program, the `fn`,
/// each `when`, `match` and loop (an `else when` is none: the chain is flat), and in an
/// expression each `(`, call, `not` and prefix `-`. Half the cap a component or service gets ([`MAX_NESTING`]), because each of these
/// levels costs the parser and the checker several frames: measured in a debug build, a
/// level took 12 to 16 KiB of stack, and the robustness tests hold every case to the
/// 1 MiB stack `mz` gets on Windows. Real programs nest under 10. Past it, `MZ0411`.
const PROGRAM_NESTING: usize = 32;

/// Whether the token stream starts with `program`.
pub(super) fn starts_program(tokens: &[Token]) -> bool {
    tokens
        .iter()
        .find(|t| !matches!(t.kind, Tok::Newline | Tok::Doc(_)))
        .is_some_and(|t| matches!(&t.kind, Tok::Ident(w) if w == "program"))
}

/// Parse a program. The lexer's diagnostics come in through `lex_diags`, so the ones a
/// skipped block or a repaired idiom makes redundant can be dropped.
pub(super) fn parse(
    tokens: Vec<Token>,
    lex_diags: &mut Vec<Diagnostic>,
    file: &str,
) -> Option<Program> {
    // A keyword written as a function's name (`fn nothing`) is reported once, at the `fn`;
    // its calls, which may come first, are read as an error value and say nothing more.
    let mut keyword_fns: Vec<String> = (0..tokens.len().saturating_sub(1))
        .filter(|&i| i == 0 || matches!(tokens[i - 1].kind, Tok::Newline))
        .filter_map(|i| match (&tokens[i].kind, &tokens[i + 1].kind) {
            (Tok::Keyword("fn"), Tok::Keyword(k)) => Some(k.to_string()),
            _ => None,
        })
        .collect();
    keyword_fns.sort();
    keyword_fns.dedup();
    // Every `fn` and `enum` name, read ahead: a type may name an enum declared later, and
    // `e.v` is a variant only when `e` is an enum.
    let declared = |kw: &str| -> Vec<String> {
        let mut names: Vec<String> = (0..tokens.len().saturating_sub(1))
            .filter(|&i| i == 0 || matches!(tokens[i - 1].kind, Tok::Newline))
            .filter_map(|i| match (&tokens[i].kind, &tokens[i + 1].kind) {
                (Tok::Keyword(k), Tok::Ident(n)) if *k == kw => Some(n.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    };
    let fn_names: Rc<[String]> = declared("fn").into();
    let enum_names: Rc<[String]> = declared("enum").into();
    // Characters the lexer dropped (`MZ0104`) or respelt (`MZ0105`), so a line read
    // without them can tell.
    let dropped: Vec<Span> = lex_diags
        .iter()
        .filter(|d| d.code == "MZ0104" || d.code == "MZ0105")
        .map(|d| d.span)
        .collect();
    let mut p = P {
        dropped,
        fn_names,
        enum_names,
        match_open: 0,
        block_value: None,
        pending_skip: false,
        keyword_names: keyword_fns.clone(),
        keyword_fns: keyword_fns.len(),
        known: std::rc::Rc::new(Known::scan(&tokens)),
        ret: None,
        toks: tokens,
        pos: 0,
        file: file.to_string(),
        diags: Vec::new(),
        skipped: Vec::new(),
        suppress: Vec::new(),
        failed: false,
        skipped_stray: false,
        nest: 0,
        too_deep: false,
    };
    let program = p.program();
    lex_diags.retain(|d| {
        !p.skipped
            .iter()
            .any(|&(from, to)| (from..=to).contains(&d.span.start_line))
            && !p.suppress.contains(&d.span)
    });
    lex_diags.append(&mut p.diags);
    program
}

/// Words that open a block a program does not hold yet, or a function body cannot: their
/// block is skipped to its `end`.
const OPENERS: &[&str] = &[
    "fn",
    "when",
    "if",
    "while",
    "for",
    "match",
    "loop",
    "contract",
    "record",
    "enum",
    "view",
    "route",
    "fallback",
    "test",
    "component",
    "service",
    "switch",
    "do",
];

/// How a statement list ended.
enum Stop {
    /// At `end` (consumed) closing this block.
    End(Span),
    /// At `else` (consumed); its span.
    Else(Span),
    /// At Python's `elif` (consumed); its span.
    Elif(Span),
    /// At a `case` line (or `default`, or `_ =>`) of the open `match`, not consumed.
    Case(Span),
    /// At `end fn`, not consumed: the `when` it arrived in was never closed.
    EndFn(Span),
    /// At a line that cannot be in a function (`fn`, `end program`), not consumed.
    Abrupt(Span),
    /// At end of file.
    Eof(Span),
}

struct P {
    toks: Vec<Token>,
    pos: usize,
    file: String,
    diags: Vec<Diagnostic>,
    /// Line ranges skipped whole as one diagnostic; the lexer's diagnostics in them go.
    skipped: Vec<(u32, u32)>,
    /// Lexer diagnostics a repair here supersedes (`fmt.Println`'s `MZ0101`).
    suppress: Vec<Span>,
    /// Set when the current line already has a diagnostic, so its leftovers are not a
    /// second one.
    failed: bool,
    /// Whether any statement stood outside every `fn`.
    skipped_stray: bool,
    /// How deep the parser is, in [`PROGRAM_NESTING`]'s levels: the open blocks (the
    /// program, the `fn`, each `when`), then each expression, `(`, call argument list,
    /// `not` and prefix `-` being read. Past the cap a `when` is skipped whole and the
    /// rest of an expression's line is not read, both without recursion (`MZ0411`).
    nest: usize,
    /// Whether `MZ0411` has been reported: once per file, as for components and services.
    too_deep: bool,
    /// Keywords written as a name, already reported (`MZ0903`): the first
    /// [`P::keyword_fns`] are functions' names, file-wide; the rest are the current
    /// function's parameters. A use of one reads as an error value, with no second error.
    keyword_names: Vec<String>,
    /// How many of [`P::keyword_names`] are functions' names.
    keyword_fns: usize,
    /// Every `fn` name in the file, so `switch(x)` calls a `fn switch` when there is one.
    fn_names: Rc<[String]>,
    /// Every `enum` name in the file: a type may name one declared later.
    enum_names: Rc<[String]>,
    /// How many `match` blocks are open, so a `case` line ends the block it is in only
    /// when a `match` is there to take it.
    match_open: usize,
    /// Set by a `when` or `match` read as a value (RFC-0013 §7.4), which reads lines through
    /// its `end`: the last token of its first line, and the line of its `end`. The
    /// statement that holds it takes it, and does not finish a line already finished.
    block_value: Option<(Span, u32)>,
    /// Set when a `when` or `match` stood where a value cannot be a block (`MZ0932`): once
    /// the line is finished, the lines of its block are skipped through its `end`.
    pending_skip: bool,
    /// Where the lexer dropped a character it does not use (`MZ0104`), such as a list's
    /// `[`, or respelt a type written with symbols (`MZ0105`, `[n]` as `list(n)`): what the
    /// parser reads there is not what was written.
    dropped: Vec<Span>,
    /// The file's enums and functions, read before parsing (RFC-0013 §12); shared with
    /// each interpolation's parser.
    known: std::rc::Rc<Known>,
    /// The return type of the function being read, for `MZ0952`'s `throw` fix.
    ret: Option<Ty>,
}

fn join(a: Span, b: Span) -> Span {
    Span {
        start_line: a.start_line,
        start_col: a.start_col,
        end_line: b.end_line,
        end_col: b.end_col,
    }
}

fn word(tok: &Tok) -> Option<&str> {
    match tok {
        Tok::Ident(w) => Some(w.as_str()),
        Tok::Keyword(k) => Some(k),
        _ => None,
    }
}

fn describe(tok: &Tok) -> String {
    super::describe(tok)
}

impl P {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)].kind
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].kind
    }

    fn span(&self) -> Span {
        self.toks[self.pos.min(self.toks.len() - 1)].span
    }

    fn span_at(&self, n: usize) -> Span {
        self.toks[(self.pos + n).min(self.toks.len() - 1)].span
    }

    /// The span of the last token consumed.
    fn prev(&self) -> Span {
        self.toks[self.pos.saturating_sub(1)].span
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos.min(self.toks.len() - 1)].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn is_word(&self, w: &str) -> bool {
        word(self.peek()) == Some(w)
    }

    fn at_line_end(&self) -> bool {
        matches!(self.peek(), Tok::Newline | Tok::Eof)
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
    }

    fn recover_line(&mut self) {
        while !self.at_line_end() {
            self.bump();
        }
        self.skip_newlines();
    }

    fn err(&mut self, code: &'static str, span: Span, say: impl Into<String>) {
        self.failed = true;
        self.diags
            .push(Diagnostic::error(code, &self.file, span, say.into()));
    }

    fn err_fix(
        &mut self,
        code: &'static str,
        span: Span,
        say: impl Into<String>,
        fix: Span,
        replace: impl Into<String>,
        c: Confidence,
    ) {
        self.failed = true;
        self.diags
            .push(Diagnostic::error(code, &self.file, span, say.into()).with_fix(fix, replace, c));
    }

    /// Report leftovers on the line unless the line already failed, then move past it.
    fn finish_line(&mut self, what: &str) {
        if !self.at_line_end() && !self.failed {
            let at = self.span();
            self.err(
                "MZ0917",
                at,
                format!(
                    "{} is left over after {what} — one statement per line",
                    describe(self.peek())
                ),
            );
        }
        self.recover_line();
        if self.pending_skip {
            self.pending_skip = false;
            self.skip_lines(1);
        }
    }

    /// Skip the block that opens on the current line, through its `end`, as one error
    /// already reported. Returns the last line skipped.
    fn skip_block(&mut self) -> u32 {
        self.skip_lines(0)
    }

    /// Skip whole lines from the current one, counting the blocks they open against the
    /// `end`s that close them, from `depth` open, until none is. Returns the last line.
    fn skip_lines(&mut self, depth: usize) -> u32 {
        let first = self.span().start_line;
        let mut depth = depth;
        loop {
            match self.peek() {
                Tok::Eof => break,
                Tok::Keyword("end") => {
                    depth = depth.saturating_sub(1);
                }
                _ if self.line_opens_block() => depth += 1,
                _ => {}
            }
            let line = self.span().start_line;
            self.recover_line();
            if depth == 0 {
                self.skipped.push((first, line));
                return line;
            }
        }
        let last = self.span().start_line;
        self.skipped.push((first, last));
        last
    }

    /// `MZ0919`: a form RFC-0013 designs that this slice does not build. One diagnostic,
    /// and the block it opens (or its line) is skipped.
    fn not_built(&mut self, what: &str, block: bool) {
        let at = self.span();
        let line_end = self.line_end_span();
        self.err(
            "MZ0919",
            join(at, line_end),
            format!(
                "{what} is designed (RFC-0013) but not built yet — a program has `fn`, `let`, `var`, `when`, `return`, `print`, and int, float, bool and text values"
            ),
        );
        if block {
            self.skip_block();
        } else {
            let line = at.start_line;
            self.skipped.push((line, line));
            self.recover_line();
        }
    }

    /// The span of the last token on the current line.
    fn line_end_span(&self) -> Span {
        let mut k = self.pos;
        while k + 1 < self.toks.len() && !matches!(self.toks[k + 1].kind, Tok::Newline | Tok::Eof) {
            if matches!(self.toks[k].kind, Tok::Newline | Tok::Eof) {
                break;
            }
            k += 1;
        }
        self.toks[k].span
    }

    // ---------------------------------------------------------------- program and fns

    fn program(&mut self) -> Option<Program> {
        self.skip_newlines();
        let mut docs = Vec::new();
        while let Tok::Doc(text) = self.peek().clone() {
            docs.push(text);
            self.bump();
            self.skip_newlines();
        }
        let program_at = self.bump().span; // `program`
        let (name, name_span) = match self.peek().clone() {
            Tok::Ident(n) => {
                let s = self.span();
                self.bump();
                (n, s)
            }
            other => {
                self.err(
                    "MZ0901",
                    program_at,
                    format!(
                        "`program` needs a snake_case name, e.g. `program hello`, found {}",
                        describe(&other)
                    ),
                );
                return None;
            }
        };
        if !self.at_line_end() {
            let at = self.span();
            self.err(
                "MZ0310",
                at,
                format!(
                    "{} is left over after `program {name}` — one declaration per line",
                    describe(self.peek())
                ),
            );
        }
        self.recover_line();
        let mut program = Program {
            name: name.clone(),
            name_span,
            docs,
            fns: Vec::new(),
            enums: Vec::new(),
            stray_statements: false,
        };
        let mut stray: Option<(Span, u32)> = None;
        loop {
            self.failed = false;
            self.skip_newlines();
            let at = self.span();
            let tok = self.peek().clone();
            let is_stray = !matches!(tok, Tok::Eof | Tok::Doc(_))
                && !matches!(word(&tok), Some(w) if is_program_item(w, self.peek_at(1)));
            if !is_stray && let Some((from, last)) = stray.take() {
                self.stray_lines(from, last);
            }
            match &tok {
                Tok::Eof => {
                    let closer = format!("end program {name}");
                    let eol = if at.start_col > 1 { "\n" } else { "" };
                    self.err_fix(
                        "MZ0204",
                        at,
                        format!(
                            "`program {name}` opened on line {} is never closed — add `{closer}`",
                            name_span.start_line
                        ),
                        Span::single(at.start_line, at.start_col, 0),
                        format!("{eol}{closer}\n"),
                        Confidence::Exact,
                    );
                    break;
                }
                Tok::Doc(text) => {
                    program.docs.push(text.clone());
                    self.recover_line();
                }
                Tok::Keyword("end") => {
                    if self.program_end(&name, name_span) {
                        break;
                    }
                }
                Tok::Keyword("fn") => {
                    if let Some(f) = self.function() {
                        program.fns.push(f);
                    }
                }
                Tok::Ident(w)
                    if matches!(w.as_str(), "def" | "function" | "func")
                        && matches!(self.peek_at(1), Tok::Ident(_)) =>
                {
                    let w = w.clone();
                    self.err_fix(
                        "MZ0903",
                        at,
                        format!("`{w}` is not how Mzizi declares a function — write `fn`"),
                        at,
                        "fn",
                        Confidence::Exact,
                    );
                    if let Some(f) = self.function() {
                        program.fns.push(f);
                    }
                }
                Tok::Keyword("use") => self.not_built("a `use` line in a program", false),
                Tok::Keyword("enum") => {
                    if let Some(e) = self.enum_decl() {
                        program.enums.push(e);
                    }
                }
                Tok::Keyword("contract") => {
                    self.not_built("a `contract` block in a program (RFC-0013 §15.1)", true)
                }
                Tok::Ident(w) if w == "record" => {
                    self.not_built("a `record` in a program (RFC-0013 §11)", true)
                }
                Tok::Ident(w) if w == "test" && matches!(self.peek_at(1), Tok::Str(_)) => {
                    self.not_built("a `test` block (RFC-0013 §15.2)", true)
                }
                Tok::Keyword(w @ ("view" | "prop" | "emit")) => {
                    let w = *w;
                    self.cannot_hold(w, at, w == "view");
                }
                Tok::Ident(w) if matches!(w.as_str(), "route" | "fallback" | "header") => {
                    let w = w.clone();
                    self.cannot_hold(&w, at, w != "header");
                }
                _ => {
                    // A statement at the top level: Python's script shape. Collected into
                    // one diagnostic per run of lines.
                    let line = at.start_line;
                    stray = Some(match stray {
                        Some((from, _)) => (from, line),
                        None => (at, line),
                    });
                    self.recover_line();
                }
            }
        }
        if let Some((from, last)) = stray.take() {
            self.stray_lines(from, last);
        }
        program.stray_statements = self.skipped_stray;
        Some(program)
    }

    fn stray_lines(&mut self, from: Span, last: u32) {
        self.skipped_stray = true;
        self.skipped.push((from.start_line, last));
        let lines = if last > from.start_line {
            format!("lines {}–{last}", from.start_line)
        } else {
            format!("line {}", from.start_line)
        };
        self.err(
            "MZ0901",
            from,
            format!(
                "a program holds `fn`s, not statements ({lines}) — statements live inside a `fn`; the entry point is `fn main` … `end fn main`"
            ),
        );
    }

    fn cannot_hold(&mut self, w: &str, at: Span, block: bool) {
        self.err(
            "MZ0901",
            at,
            format!("a program cannot hold `{w}` — it belongs in a component or a service"),
        );
        if block {
            self.skip_block();
        } else {
            self.skipped.push((at.start_line, at.start_line));
            self.recover_line();
        }
    }

    /// An `end` at program level. Returns true when it closed the program.
    fn program_end(&mut self, name: &str, name_span: Span) -> bool {
        let end_at = self.bump().span;
        let canonical = format!("end program {name}");
        let echoed = word(self.peek()).map(str::to_string);
        if echoed.as_deref() == Some("program") {
            self.bump();
            match self.peek().clone() {
                Tok::Ident(n) if n == name => {
                    self.bump();
                }
                Tok::Ident(n) => {
                    let s = self.span();
                    self.bump();
                    self.err_fix(
                        "MZ0207",
                        s,
                        format!(
                            "`end program {n}` does not match `program {name}` opened on line {}",
                            name_span.start_line
                        ),
                        s,
                        name,
                        Confidence::Exact,
                    );
                }
                _ => {
                    let closer = join(end_at, self.prev());
                    self.err_fix(
                        "MZ0208",
                        end_at,
                        format!("top-level blocks close with their name: write `{canonical}`"),
                        closer,
                        canonical.clone(),
                        Confidence::Exact,
                    );
                }
            }
        } else if echoed.is_none() {
            self.err_fix(
                "MZ0208",
                end_at,
                format!("top-level blocks close with their name: write `{canonical}`"),
                end_at,
                canonical.clone(),
                Confidence::Exact,
            );
        } else {
            // `end fn x` or `end when` with nothing open: delete it.
            self.recover_line_from(end_at);
            return false;
        }
        if !self.at_line_end() && !self.failed {
            let at = self.span();
            self.err(
                "MZ0310",
                at,
                format!("{} is left over after `{canonical}`", describe(self.peek())),
            );
        }
        self.recover_line();
        self.skip_newlines();
        if !matches!(self.peek(), Tok::Eof) {
            let at = self.span();
            self.err(
                "MZ0901",
                at,
                format!("nothing follows `{canonical}` — a file holds one program"),
            );
            let from = at.start_line;
            while !matches!(self.peek(), Tok::Eof) {
                self.bump();
            }
            self.skipped.push((from, self.span().start_line));
        }
        true
    }

    /// `end …` with nothing open for it to close: `MZ0205`, whose fix deletes the line.
    fn recover_line_from(&mut self, end_at: Span) {
        let line = end_at.start_line;
        self.err_fix(
            "MZ0205",
            end_at,
            "this `end` has nothing open to close — delete it",
            Span {
                start_line: line,
                start_col: 1,
                end_line: line + 1,
                end_col: 1,
            },
            "",
            Confidence::Exact,
        );
        self.recover_line();
    }

    /// `fn name(a: int): int` … `end fn name`. The cursor is on `fn` (or the `def` it
    /// stands for).
    fn function(&mut self) -> Option<FnDecl> {
        let fn_at = self.bump().span;
        self.keyword_names.truncate(self.keyword_fns);
        let (name, name_span) = match self.peek().clone() {
            Tok::Ident(n) => {
                let s = self.span();
                self.bump();
                (n, s)
            }
            Tok::Keyword(k) => {
                // Read on with the keyword as the name, so the body is checked and a call
                // to it is not a second error.
                let s = self.span();
                self.err(
                    "MZ0903",
                    s,
                    format!("`{k}` is a keyword, so it cannot name a function — rename it"),
                );
                self.bump();
                (k.to_string(), s)
            }
            other => {
                self.err(
                    "MZ0903",
                    fn_at,
                    format!("`fn` needs a snake_case name, found {}", describe(&other)),
                );
                // Skip the body, which cannot be attached to anything.
                self.skip_fn_body();
                return None;
            }
        };
        let params = self.params(&name);
        let ret = self.return_type(&name);
        self.ret = ret.map(|r| r.ty);
        self.trailing_block_punctuation();
        self.finish_line(&format!("the signature of `fn {name}`"));
        // The program and this `fn` are the first two open blocks.
        self.nest = 2;
        let skipped_before = self.skipped.len();
        let (body, stop) = self.stmts(false, &name);
        let skipped = self.skipped.len() > skipped_before;
        let end_span = match stop {
            Stop::End(closer) => closer,
            Stop::Abrupt(at)
            | Stop::Eof(at)
            | Stop::EndFn(at)
            | Stop::Else(at)
            | Stop::Elif(at)
            | Stop::Case(at) => {
                let closer = format!("end fn {name}");
                let eof = matches!(stop, Stop::Eof(_));
                let (fix_at, text) = if eof && at.start_col > 1 {
                    (at, format!("\n{closer}\n"))
                } else {
                    (Span::single(at.start_line, 1, 0), format!("{closer}\n"))
                };
                self.err_fix(
                    "MZ0204",
                    at,
                    format!(
                        "`fn {name}` opened on line {} is never closed — add `{closer}` before this line",
                        name_span.start_line
                    ),
                    fix_at,
                    text,
                    Confidence::Guess,
                );
                at
            }
        };
        Some(FnDecl {
            name,
            name_span,
            params,
            ret,
            body,
            end_span,
            skipped,
        })
    }

    fn skip_fn_body(&mut self) {
        let first = self.span().start_line;
        self.recover_line();
        while !matches!(self.peek(), Tok::Eof) {
            if self.is_word("end") && matches!(word(self.peek_at(1)), Some("fn" | "program")) {
                if word(self.peek_at(1)) == Some("fn") {
                    self.recover_line();
                }
                break;
            }
            self.recover_line();
        }
        self.skipped.push((first, self.span().start_line));
    }

    fn params(&mut self, fn_name: &str) -> Vec<Param> {
        let mut params = Vec::new();
        if !matches!(self.peek(), Tok::LParen) {
            return params;
        }
        let open = self.bump().span;
        if matches!(self.peek(), Tok::RParen) {
            let close = self.bump().span;
            self.err_fix(
                "MZ0903",
                join(open, close),
                format!("a function with no parameters has no parentheses — write `fn {fn_name}`"),
                join(open, close),
                "",
                Confidence::Exact,
            );
            return params;
        }
        loop {
            let (name, span) = match self.peek().clone() {
                Tok::Ident(n) => {
                    let s = self.span();
                    self.bump();
                    (n, s)
                }
                Tok::Keyword(k) if matches!(self.peek_at(1), Tok::Colon) => {
                    // `match: int`: one error here. The parameter still counts, so a call
                    // is not also short an argument, and its uses in the body are silent.
                    let s = self.span();
                    self.err(
                        "MZ0903",
                        s,
                        format!(
                            "`{k}` is a keyword, so it cannot name a parameter of `fn {fn_name}` — rename it"
                        ),
                    );
                    self.keyword_names.push(k.to_string());
                    self.bump();
                    (k.to_string(), s)
                }
                other => {
                    let at = self.span();
                    self.err(
                        "MZ0903",
                        at,
                        format!(
                            "expected a parameter, `name: type`, in `fn {fn_name}`, found {}",
                            describe(&other)
                        ),
                    );
                    self.skip_to_paren_end();
                    return params;
                }
            };
            let ty = if matches!(self.peek(), Tok::Colon) {
                self.bump();
                self.type_ref("a parameter's `:`")
            } else {
                self.err(
                    "MZ0903",
                    span,
                    format!(
                        "parameter `{name}` of `fn {fn_name}` has no type — every parameter is written `{name}: <type>`"
                    ),
                );
                TypeRef {
                    ty: Ty::Error,
                    span,
                }
            };
            if matches!(self.peek(), Tok::Equals) {
                let at = self.span();
                self.err(
                    "MZ0903",
                    at,
                    format!(
                        "parameter `{name}` has a default value, and Mzizi has none — a call gives every argument"
                    ),
                );
                while !matches!(
                    self.peek(),
                    Tok::Comma | Tok::RParen | Tok::Newline | Tok::Eof
                ) {
                    self.bump();
                }
            }
            params.push(Param { name, span, ty });
            match self.peek() {
                Tok::Comma => {
                    self.bump();
                }
                Tok::RParen => {
                    self.bump();
                    return params;
                }
                other => {
                    let at = self.span();
                    let other = describe(other);
                    if !self.failed {
                        self.err(
                            "MZ0903",
                            at,
                            format!(
                                "expected `,` or `)` in the parameters of `fn {fn_name}`, found {other}"
                            ),
                        );
                    }
                    self.skip_to_paren_end();
                    return params;
                }
            }
        }
    }

    fn skip_to_paren_end(&mut self) {
        while !matches!(self.peek(), Tok::RParen | Tok::Newline | Tok::Eof) {
            self.bump();
        }
        if matches!(self.peek(), Tok::RParen) {
            self.bump();
        }
    }

    fn return_type(&mut self, fn_name: &str) -> Option<TypeRef> {
        let before = self.prev();
        let arrow = matches!(self.peek(), Tok::Op("->"));
        if !arrow && !matches!(self.peek(), Tok::Colon) {
            return None;
        }
        // `def f(x):` — a trailing colon is block punctuation, not a return type.
        if !arrow && matches!(self.peek_at(1), Tok::Newline | Tok::Eof) {
            return None;
        }
        let sep = self.bump().span;
        let type_at = self.span();
        if let Some(w @ ("none" | "void" | "unit")) = word(self.peek()) {
            let w = w.to_string();
            self.bump();
            self.err_fix(
                "MZ0903",
                join(sep, type_at),
                format!(
                    "a function that returns nothing has no return type — delete `: {w}` from `fn {fn_name}`"
                ),
                Span {
                    start_line: before.end_line,
                    start_col: before.end_col,
                    end_line: type_at.end_line,
                    end_col: type_at.end_col,
                },
                "",
                Confidence::Exact,
            );
            return None;
        }
        if arrow {
            self.err_fix(
                "MZ0903",
                sep,
                format!("a return type follows `:`, not `->` — `fn {fn_name}(…): <type>`"),
                Span {
                    start_line: before.end_line,
                    start_col: before.end_col,
                    end_line: type_at.start_line,
                    end_col: type_at.start_col,
                },
                ": ",
                Confidence::Exact,
            );
        }
        Some(self.type_ref("`:`"))
    }

    /// A type: `int`, `float`, `bool`, `text`, one of the program's enums, or
    /// `result(T, E)` (RFC-0013 §12.1).
    fn type_ref(&mut self, after: &str) -> TypeRef {
        let at = self.span();
        let Some(name) = word(self.peek()).map(str::to_string) else {
            let found = describe(self.peek());
            self.err(
                "MZ0306",
                at,
                format!("expected a type after {after}, found {found}"),
            );
            return TypeRef {
                ty: Ty::Error,
                span: at,
            };
        };
        self.bump();
        if name == "result" && matches!(self.peek(), Tok::LParen) {
            return self.result_type(at);
        }
        let mut span = at;
        if matches!(self.peek(), Tok::LParen) {
            // `list(int)`, `option(text)`: later waves'.
            let mut depth = 0;
            loop {
                match self.peek() {
                    Tok::LParen => depth += 1,
                    Tok::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            span = join(at, self.span());
                            self.bump();
                            break;
                        }
                    }
                    Tok::Newline | Tok::Eof => break,
                    _ => {}
                }
                self.bump();
            }
        }
        // The type names are the surface types' own (`Ty::from_name`), so a type the
        // language harness lists is exactly a type this parser accepts.
        let surface = Ty::from_name(&name).filter(|_| span == at);
        let ty = match name.as_str() {
            _ if surface.is_some() => surface.unwrap_or(Ty::Error),
            n if span == at && self.enum_names.iter().any(|e| e == n) => Ty::Enum(intern(n)),
            "result" => {
                self.err(
                    "MZ0306",
                    span,
                    "a result names its success and its error type: `result(<type>, <error type>)`, or `result(none, <error type>)`",
                );
                Ty::Error
            }
            "list" | "option" | "map" | "set" => {
                self.err(
                    "MZ0919",
                    span,
                    format!(
                        "`{name}` is designed (RFC-0013 §2) but not built yet — a program has int, float, bool, text, its enums and `result(T, E)`"
                    ),
                );
                Ty::Error
            }
            other => {
                let alias = match other {
                    "i64" | "i32" | "i16" | "i8" | "u64" | "u32" | "usize" | "isize"
                    | "integer" | "number" => Some("int"),
                    "boolean" => Some("bool"),
                    "f64" | "f32" | "double" | "decimal" | "real" => Some("float"),
                    "string" | "str" | "char" => Some("text"),
                    _ => None,
                };
                let say = format!(
                    "`{other}` is not a type here — the types are int, float, bool, text, the program's enums and `result(T, E)`"
                );
                match alias {
                    Some(a) => {
                        // `String` reached here as the lexer's `string`, with its own MZ0101.
                        self.suppress.push(at);
                        self.err_fix("MZ0701", span, say, span, a, Confidence::Guess);
                    }
                    None => self.err("MZ0701", span, say),
                }
                Ty::Error
            }
        };
        self.question_after_type();
        TypeRef { ty, span }
    }

    /// `MZ0937`: a trailing `:` on a block line (Python). The fix deletes it.
    fn trailing_block_punctuation(&mut self) {
        if matches!(self.peek(), Tok::Colon) && matches!(self.peek_at(1), Tok::Newline | Tok::Eof) {
            let at = self.bump().span;
            self.err_fix(
                "MZ0937",
                at,
                "a block line has no trailing `:` — the block runs to its `end`",
                at,
                "",
                Confidence::Exact,
            );
            // The line is repaired; what follows on it is not a second error.
            self.failed = false;
        }
    }

    // ---------------------------------------------------------------- statements

    /// Statements up to the `end` (or `else`) of the block they are in.
    fn stmts(&mut self, in_when: bool, fn_name: &str) -> (Vec<Stmt>, Stop) {
        let mut out = Vec::new();
        loop {
            self.failed = false;
            self.skip_newlines();
            let at = self.span();
            match self.peek().clone() {
                Tok::Eof => return (out, Stop::Eof(at)),
                Tok::Keyword("end") => {
                    let next = word(self.peek_at(1)).map(str::to_string);
                    match next.as_deref() {
                        Some("program") => return (out, Stop::Abrupt(at)),
                        Some("fn") if in_when => return (out, Stop::EndFn(at)),
                        _ => {
                            let closer = self.block_end(in_when, fn_name);
                            return (out, Stop::End(closer));
                        }
                    }
                }
                Tok::Keyword("else") if in_when => {
                    self.bump();
                    return (out, Stop::Else(at));
                }
                Tok::Ident(w) if w == "elif" && in_when => {
                    self.bump();
                    return (out, Stop::Elif(at));
                }
                _ if self.at_case_line() => {
                    if in_when && self.match_open > 0 {
                        return (out, Stop::Case(at));
                    }
                    self.err(
                        "MZ0917",
                        at,
                        "a `case` line with no `match` open — `case` lines belong to a `match`",
                    );
                    self.recover_line();
                }
                Tok::Keyword("else") => {
                    self.err(
                        "MZ0917",
                        at,
                        "`else` with no `when` open — `else` continues a `when` block",
                    );
                    self.recover_line();
                }
                Tok::Keyword("fn") => return (out, Stop::Abrupt(at)),
                Tok::Doc(_) => self.recover_line(),
                _ => {
                    if let Some(s) = self.statement(fn_name) {
                        out.push(s);
                    }
                }
            }
        }
    }

    /// The `end` that closes a function body (`end fn <name>`) or a `when` (`end`).
    fn block_end(&mut self, in_when: bool, fn_name: &str) -> Span {
        let end_at = self.bump().span;
        let canonical = if in_when {
            "end".to_string()
        } else {
            format!("end fn {fn_name}")
        };
        let mut echo = Vec::new();
        while !self.at_line_end() {
            echo.push(self.bump());
        }
        let closer = echo.last().map_or(end_at, |t| join(end_at, t.span));
        match (in_when, echo.as_slice()) {
            (true, []) => {}
            (false, [kw, n]) if word(&kw.kind) == Some("fn") => {
                if let Tok::Ident(n_text) = &n.kind
                    && n_text != fn_name
                {
                    self.err_fix(
                        "MZ0207",
                        n.span,
                        format!("`end fn {n_text}` does not match `fn {fn_name}`"),
                        n.span,
                        fn_name,
                        Confidence::Exact,
                    );
                }
            }
            (false, []) | (false, [_])
                if echo.first().is_none_or(|t| word(&t.kind) == Some("fn")) =>
            {
                self.err_fix(
                    "MZ0208",
                    end_at,
                    format!("a `fn` closes with its name: write `{canonical}`"),
                    closer,
                    canonical.clone(),
                    Confidence::Exact,
                );
            }
            _ => {
                let say = if in_when {
                    "a block inside a `fn` (`when`, `match`, a loop) closes with a bare `end`"
                        .to_string()
                } else {
                    format!("a `fn` closes with its name: write `{canonical}`")
                };
                self.err_fix(
                    "MZ0206",
                    end_at,
                    say,
                    closer,
                    canonical.clone(),
                    Confidence::Exact,
                );
            }
        }
        self.recover_line();
        closer
    }

    /// One statement line (and, for `when`, its block).
    fn statement(&mut self, fn_name: &str) -> Option<Stmt> {
        let at = self.span();
        let tok = self.peek().clone();
        let w = word(&tok).map(str::to_string);
        // `match(1)` where `fn match` was declared (and reported): a call, read as a value,
        // not the block the keyword would open. The words every statement starts with keep
        // their meaning.
        let keyword_call = matches!(&tok, Tok::Keyword(k)
            if self.keyword_name(k)
                && matches!(self.peek_at(1), Tok::LParen)
                && !matches!(*k, "let" | "var" | "return" | "when" | "else" | "end" | "fn"));
        let kind = match w.as_deref() {
            _ if keyword_call => {
                let value = self.expr();
                self.finish_line("a call");
                StmtKind::Expr(value)
            }
            Some("let" | "var") => self.binding(w.as_deref() == Some("var"), None)?,
            Some("const" | "val" | "auto") if matches!(self.peek_at(1), Tok::Ident(_)) => {
                let w = w.clone().unwrap_or_default();
                self.err_fix(
                    "MZ0925",
                    at,
                    format!("`{w}` is not a Mzizi binding — a name that never changes is a `let`"),
                    at,
                    "let",
                    Confidence::Exact,
                );
                self.binding(false, Some(()))?
            }
            Some("mut") if matches!(self.peek_at(1), Tok::Ident(_)) => {
                self.err_fix(
                    "MZ0925",
                    at,
                    "`mut` is Rust's — a name that changes is a `var`",
                    at,
                    "var",
                    Confidence::Exact,
                );
                self.binding(true, Some(()))?
            }
            Some("return") => {
                self.bump();
                let value = if self.at_line_end() {
                    None
                } else {
                    Some(self.value())
                };
                self.finish_value_line("`return`");
                StmtKind::Return(value)
            }
            Some("when") => return self.when(at, fn_name),
            Some("try") if self.at_try_block() => {
                self.try_block(at);
                return None;
            }
            _ if self.at_throw() => self.throw_stmt(at),
            Some("if") => {
                self.err_fix(
                    "MZ0407",
                    at,
                    "Mzizi's conditional is `when` — `if` is not a word of the language",
                    at,
                    "when",
                    Confidence::Exact,
                );
                self.failed = false;
                return self.when(at, fn_name);
            }
            Some(w) if self.control_statement(w) => return self.control(at, fn_name),
            Some("contract") => {
                self.not_built("a `contract` block on a function (RFC-0013 §15.1)", true);
                return None;
            }
            _ => self.simple_statement(&tok, at)?,
        };
        let (line_end, last_line) = match self.block_value.take() {
            Some((header_end, last)) => (header_end, last),
            None => (self.prev(), at.start_line),
        };
        Some(Stmt {
            kind,
            span: join(at, line_end),
            last_line,
        })
    }

    /// An assignment, a Go `:=`, an operator-assignment, a print idiom, or an expression.
    fn simple_statement(&mut self, tok: &Tok, at: Span) -> Option<StmtKind> {
        if let Tok::Ident(name) = tok {
            let name = name.clone();
            match (self.peek_at(1).clone(), self.peek_at(2).clone()) {
                (Tok::Equals, _) => {
                    self.bump();
                    self.bump();
                    let value = self.value();
                    self.finish_value_line("an assignment");
                    return Some(StmtKind::Assign {
                        name,
                        name_span: at,
                        ty: None,
                        value,
                    });
                }
                (Tok::Colon, Tok::Equals) => {
                    // Go's `x := e`.
                    let eq = self.span_at(2);
                    self.err_fix(
                        "MZ0925",
                        join(at, eq),
                        format!("`{name} :=` is Go's — a binding is `let {name} = …`"),
                        join(at, eq),
                        format!("let {name} ="),
                        Confidence::Exact,
                    );
                    self.bump();
                    self.bump();
                    self.bump();
                    let value = self.value();
                    self.finish_value_line("a binding");
                    return Some(StmtKind::Bind {
                        mutable: false,
                        kw_span: at,
                        name,
                        name_span: at,
                        ty: None,
                        value,
                    });
                }
                (Tok::Colon, _) => {
                    // Python's annotated first assignment, `x: int = 1`.
                    self.bump();
                    self.bump();
                    let ty = self.type_ref("`:`");
                    if !matches!(self.peek(), Tok::Equals) {
                        if !self.failed {
                            let s = self.span();
                            self.err(
                                "MZ0926",
                                s,
                                format!("`{name}` has a type and no value — a binding always has a value"),
                            );
                        }
                        self.recover_line();
                        return None;
                    }
                    self.bump();
                    let value = self.value();
                    self.finish_value_line("an assignment");
                    return Some(StmtKind::Assign {
                        name,
                        name_span: at,
                        ty: Some(ty),
                        value,
                    });
                }
                (Tok::Op(op @ ("+=" | "-=" | "*=" | "/=")), _) => {
                    let op_at = self.span_at(1);
                    self.bump();
                    self.bump();
                    let rhs = self.expr();
                    let bin = match op {
                        "+=" => BinOp::Add,
                        "-=" => BinOp::Sub,
                        "*=" => BinOp::Mul,
                        _ => BinOp::Div,
                    };
                    let value = Expr {
                        span: join(at, rhs.span),
                        kind: ExprKind::Binary {
                            op: bin,
                            op_span: op_at,
                            lhs: Box::new(Expr {
                                kind: ExprKind::Name(name.clone()),
                                span: at,
                            }),
                            rhs: Box::new(rhs),
                        },
                    };
                    if !self.failed {
                        let fixed = format!("{name} = {}", canonical(&value));
                        self.err_fix(
                            "MZ0918",
                            op_at,
                            format!("`{op}` is not a Mzizi operator — write `{fixed}`"),
                            join(at, value.span),
                            fixed,
                            Confidence::Exact,
                        );
                    }
                    self.finish_line("an assignment");
                    return Some(StmtKind::Assign {
                        name,
                        name_span: at,
                        ty: None,
                        value,
                    });
                }
                (Tok::Op(op @ ("++" | "--")), _) => {
                    let op_at = self.span_at(1);
                    self.bump();
                    self.bump();
                    let bin = if op == "++" { BinOp::Add } else { BinOp::Sub };
                    let value = Expr {
                        span: join(at, op_at),
                        kind: ExprKind::Binary {
                            op: bin,
                            op_span: op_at,
                            lhs: Box::new(Expr {
                                kind: ExprKind::Name(name.clone()),
                                span: at,
                            }),
                            rhs: Box::new(Expr {
                                kind: ExprKind::Int(1),
                                span: op_at,
                            }),
                        },
                    };
                    let fixed = format!("{name} = {}", canonical(&value));
                    self.err_fix(
                        "MZ0918",
                        op_at,
                        format!("`{op}` is not a Mzizi operator — write `{fixed}`"),
                        join(at, op_at),
                        fixed,
                        Confidence::Exact,
                    );
                    self.finish_line("an assignment");
                    return Some(StmtKind::Assign {
                        name,
                        name_span: at,
                        ty: None,
                        value,
                    });
                }
                _ => {}
            }
            // `print x` and `puts x`: a print with no parentheses.
            if matches!(name.as_str(), "print" | "puts")
                && !matches!(self.peek_at(1), Tok::LParen | Tok::Newline | Tok::Eof)
            {
                self.bump();
                let value = self.expr();
                let call =
                    self.print_call(at, &format!("{name} …"), vec![value], join(at, self.prev()));
                self.finish_line("`print`");
                return Some(StmtKind::Expr(call));
            }
        }
        let e = self.expr();
        self.finish_line("an expression");
        Some(StmtKind::Expr(e))
    }

    /// `let name[: type] = value`. `prefixed` is set when the cursor is on a word
    /// (`const`, `mut`) already reported and standing for `let` or `var`.
    fn binding(&mut self, mutable: bool, prefixed: Option<()>) -> Option<StmtKind> {
        let kw_at = self.bump().span;
        let mut mutable = mutable;
        let mut kw_span = kw_at;
        // Rust's `let mut`.
        if prefixed.is_none()
            && !mutable
            && self.is_word("mut")
            && matches!(self.peek_at(1), Tok::Ident(_))
        {
            let mut_at = self.bump().span;
            self.err_fix(
                "MZ0925",
                join(kw_at, mut_at),
                "`let mut` is Rust's — a name that changes is a `var`",
                join(kw_at, mut_at),
                "var",
                Confidence::Exact,
            );
            self.failed = false;
            mutable = true;
            kw_span = join(kw_at, mut_at);
        }
        if prefixed.is_some() {
            self.failed = false;
        }
        let (name, name_span) = match self.peek().clone() {
            Tok::Ident(n) => {
                let s = self.span();
                self.bump();
                (n, s)
            }
            other => {
                let at = self.span();
                self.err(
                    "MZ0917",
                    at,
                    format!(
                        "`{}` needs a snake_case name, found {}",
                        if mutable { "var" } else { "let" },
                        describe(&other)
                    ),
                );
                self.recover_line();
                return None;
            }
        };
        let ty = if matches!(self.peek(), Tok::Colon) {
            self.bump();
            Some(self.type_ref("`:`"))
        } else {
            None
        };
        if !matches!(self.peek(), Tok::Equals) {
            if !self.failed {
                self.err(
                    "MZ0926",
                    join(kw_at, self.prev()),
                    format!(
                        "`{name}` is bound with no value — a binding always has one: `{} {name} = …`{}",
                        if mutable { "var" } else { "let" },
                        if self.at_line_end() {
                            String::new()
                        } else {
                            format!(", found {}", describe(&self.toks[self.pos].kind))
                        }
                    ),
                );
            }
            self.recover_line();
            return None;
        }
        self.bump();
        let value = self.value();
        self.finish_value_line("a binding");
        Some(StmtKind::Bind {
            mutable,
            kw_span,
            name,
            name_span,
            ty,
            value,
        })
    }

    /// `when cond` … [`else` …] `end`. The cursor is on `when` (or the `if` it stands for).
    fn when(&mut self, at: Span, fn_name: &str) -> Option<Stmt> {
        if self.nest >= PROGRAM_NESTING {
            // Past the cap the `when` is skipped whole by counting block openers against
            // `end`s, without recursion, so input of any depth costs no stack.
            let line_end = self.line_end_span();
            self.report_too_deep(join(at, line_end), "this block");
            self.failed = true;
            self.skip_block();
            return None;
        }
        self.nest += 1;
        let stmt = self.when_block(at, fn_name);
        self.nest -= 1;
        stmt
    }

    /// [`P::when`] under the nesting cap.
    fn when_block(&mut self, at: Span, fn_name: &str) -> Option<Stmt> {
        self.bump();
        let cond = self.expr();
        self.trailing_block_punctuation();
        self.finish_line("a `when` condition");
        let (then, mut stop) = self.stmts(true, fn_name);
        let mut else_whens = Vec::new();
        let mut otherwise = None;
        let mut last_line = stop_line(&stop);
        // `else when` continues the same block and shares its `end` (RFC-0013 §7.1). The
        // chain is read in a loop and kept flat, so its length costs no nesting.
        loop {
            match stop {
                Stop::End(_) | Stop::Abrupt(_) | Stop::Eof(_) => break,
                Stop::EndFn(end_at) | Stop::Case(end_at) => {
                    self.unclosed_when(at, end_at);
                    break;
                }
                Stop::Elif(elif_at) => {
                    self.failed = false;
                    self.else_when_idiom(elif_at, "elif");
                    let (link, s) = self.else_when(elif_at, fn_name);
                    else_whens.push(link);
                    last_line = stop_line(&s);
                    stop = s;
                }
                Stop::Else(else_at) if self.is_word("when") || self.is_word("if") => {
                    self.failed = false;
                    if self.is_word("if") {
                        let if_at = self.span();
                        self.else_when_idiom(if_at, "if");
                    }
                    self.bump();
                    let (link, s) = self.else_when(else_at, fn_name);
                    else_whens.push(link);
                    last_line = stop_line(&s);
                    stop = s;
                }
                Stop::Else(_) => {
                    self.failed = false;
                    self.trailing_block_punctuation();
                    self.finish_line("`else`");
                    let (o, s) = self.stmts(true, fn_name);
                    last_line = stop_line(&s);
                    match s {
                        Stop::End(_) | Stop::Abrupt(_) | Stop::Eof(_) => {}
                        Stop::Else(second) | Stop::Elif(second) => {
                            self.err(
                                "MZ0917",
                                second,
                                "a second `else` in one `when` — a `when` has one `else`, last",
                            );
                            self.recover_line();
                            let _ = self.stmts(true, fn_name);
                        }
                        Stop::EndFn(end_at) | Stop::Case(end_at) => self.unclosed_when(at, end_at),
                    }
                    otherwise = Some(o);
                    break;
                }
            }
        }
        Some(Stmt {
            span: join(at, cond.span),
            kind: StmtKind::When {
                cond,
                then,
                else_whens,
                otherwise,
            },
            last_line,
        })
    }

    fn unclosed_when(&mut self, when_at: Span, end_at: Span) {
        let indent = " ".repeat(when_at.start_col.saturating_sub(1) as usize);
        self.err_fix(
            "MZ0204",
            end_at,
            format!(
                "the `when` opened on line {} is still open when this line arrives — add `end` before this line",
                when_at.start_line
            ),
            Span::single(end_at.start_line, 1, 0),
            format!("{indent}end\n"),
            Confidence::Guess,
        );
    }

    // ---------------------------------------------------------------- expressions

    fn expr(&mut self) -> Expr {
        if self.nest >= PROGRAM_NESTING {
            let at = self.span();
            return self.too_deep_expr(at);
        }
        self.nest += 1;
        let e = self.or_expr();
        self.nest -= 1;
        e
    }

    /// `MZ0411`, once per file, as for components and services.
    fn report_too_deep(&mut self, span: Span, what: &str) {
        if self.too_deep {
            return;
        }
        self.too_deep = true;
        self.diags.push(Diagnostic::error(
            "MZ0411",
            &self.file,
            span,
            format!(
                "blocks and expressions nest more than {PROGRAM_NESTING} deep here, or an expression's operators nest more than {MAX_NESTING} deep (the program, the `fn` and each `when`, `match` and loop are a level, and so is each expression read inside another: a statement's value, a `(`, a call's arguments, an interpolation, a `not`, a prefix `-`); {what}, and everything else past that depth in this file, is not read"
            ),
        ));
    }

    /// `MZ0411` for an expression at `at`. The rest of the line is passed over without
    /// recursion and is not read, so input of any depth costs no stack, and every walk of
    /// the tree after the parser (the checker, the lowering, the canonical text) stays
    /// within a bounded depth.
    fn too_deep_expr(&mut self, at: Span) -> Expr {
        let span = join(at, self.line_end_span());
        self.report_too_deep(span, "the rest of this line");
        self.failed = true;
        while !self.at_line_end() {
            self.bump();
        }
        self.skipped.push((at.start_line, at.start_line));
        self.error_expr(at)
    }

    /// One more link in a binary chain: `lhs` is the chain so far, `height` its height (0
    /// until first measured, so an expression with no operator is never walked), and `rhs`
    /// the new right operand. True, with `MZ0411` reported and the line passed over, once
    /// the tree would be more than [`MAX_NESTING`] deep: `1 + 1 + …` builds a tree as deep
    /// as it is long, and the walks after the parser recurse once per level.
    fn chain_too_deep(&mut self, height: &mut usize, lhs: &Expr, rhs: &Expr, op_at: Span) -> bool {
        if *height == 0 {
            *height = depth(lhs);
        }
        *height = (*height).max(depth(rhs)) + 1;
        if *height <= MAX_NESTING {
            return false;
        }
        self.too_deep_expr(op_at);
        true
    }

    fn or_expr(&mut self) -> Expr {
        let first_diag = self.diags.len();
        let mut lhs = self.and_expr();
        let mut mixed = is_bare_and(&lhs);
        let mut height = 0;
        loop {
            let at = self.span();
            if self.is_word("or") {
                self.bump();
            } else if matches!(self.peek(), Tok::Op("||")) {
                self.bump();
                self.idiom_op("||", "or", at);
            } else {
                break;
            }
            let rhs = self.and_expr();
            if self.chain_too_deep(&mut height, &lhs, &rhs, at) {
                return self.error_expr(join(lhs.span, rhs.span));
            }
            mixed |= is_bare_and(&rhs);
            lhs = binary(BinOp::Or, at, lhs, rhs);
        }
        if mixed && matches!(lhs.kind, ExprKind::Binary { op: BinOp::Or, .. }) {
            self.and_meets_or(&lhs, first_diag);
        }
        lhs
    }

    /// `MZ0913`: `and` mixed with `or` without parentheses (RFC-0013 §3.4). The tree already
    /// has the reading the precedence table implies; the `exact` fix writes its parentheses
    /// (§17). An idiom repaired inside it (`&&`, `==`) is folded into this one fix, so two
    /// `exact` fixes never overlap; a `guess` inside it makes this fix a `guess` too.
    fn and_meets_or(&mut self, e: &Expr, first_diag: usize) {
        let confidence = self.fold_inner_fixes(e.span, first_diag);
        let fixed = canonical(e);
        let d = Diagnostic::error(
            "MZ0913",
            &self.file,
            e.span,
            format!("`and` and `or` are mixed without parentheses — write `{fixed}`"),
        );
        self.diags.push(if e.has_error() {
            d
        } else {
            d.with_fix(e.span, fixed, confidence)
        });
    }

    /// Fold the `exact` repairs reported since `first_diag` inside `span` (`MZ0910`'s idioms,
    /// and an inner `MZ0913`) into the one fix about to cover all of `span`, so two fixes
    /// never overlap. The covering fix is `exact` unless a `guess`, or a fix of any other
    /// code, sits inside it.
    fn fold_inner_fixes(&mut self, span: Span, first_diag: usize) -> Confidence {
        let within = |d: &Diagnostic| {
            d.fix.as_ref().is_some_and(|f| {
                (f.span.start_line, f.span.start_col) >= (span.start_line, span.start_col)
                    && (f.span.end_line, f.span.end_col) <= (span.end_line, span.end_col)
            })
        };
        let mut confidence = Confidence::Exact;
        let mut k = first_diag;
        while k < self.diags.len() {
            let d = &self.diags[k];
            let exact = d
                .fix
                .as_ref()
                .is_some_and(|f| f.confidence == Confidence::Exact);
            if within(d) && exact && matches!(d.code, "MZ0910" | "MZ0913") {
                self.diags.remove(k);
                continue;
            }
            if within(d) {
                confidence = Confidence::Guess;
            }
            k += 1;
        }
        confidence
    }

    fn and_expr(&mut self) -> Expr {
        let mut lhs = self.not_expr();
        let mut height = 0;
        loop {
            let at = self.span();
            if self.is_word("and") {
                self.bump();
            } else if matches!(self.peek(), Tok::Op("&&")) {
                self.bump();
                self.idiom_op("&&", "and", at);
            } else {
                return lhs;
            }
            let rhs = self.not_expr();
            if self.chain_too_deep(&mut height, &lhs, &rhs, at) {
                return self.error_expr(join(lhs.span, rhs.span));
            }
            lhs = binary(BinOp::And, at, lhs, rhs);
        }
    }

    /// `MZ0910`: an operator spelt from another language, repaired in place.
    fn idiom_op(&mut self, written: &str, mzizi: &str, at: Span) {
        self.diags.push(
            Diagnostic::error(
                "MZ0910",
                &self.file,
                at,
                format!("`{written}` is not a Mzizi operator — write `{mzizi}`"),
            )
            .with_fix(at, mzizi, Confidence::Exact),
        );
    }

    fn not_expr(&mut self) -> Expr {
        let at = self.span();
        let bang = matches!(self.peek(), Tok::Op("!"));
        if self.is_word("not") || bang {
            self.bump();
            if bang {
                // `!x` becomes `not x`: the space is part of the repair.
                let next = self.span();
                let spaced = next.start_line == at.start_line && next.start_col > at.end_col;
                self.diags.push(
                    Diagnostic::error(
                        "MZ0910",
                        &self.file,
                        at,
                        "`!` is not a Mzizi operator — write `not`",
                    )
                    .with_fix(
                        at,
                        if spaced { "not" } else { "not " },
                        Confidence::Exact,
                    ),
                );
            }
            if self.nest >= PROGRAM_NESTING {
                return self.too_deep_expr(at);
            }
            self.nest += 1;
            let first_diag = self.diags.len();
            let operand = self.not_expr();
            self.nest -= 1;
            let e = Expr {
                span: join(at, operand.span),
                kind: ExprKind::Unary {
                    op: UnOp::Not,
                    operand: Box::new(operand),
                },
            };
            return if bang { e } else { self.not_is(e, first_diag) };
        }
        self.cmp_expr()
    }

    /// `not a is b`, which reads `not (a is b)`: `MZ0910`, with the `exact` fix `a is not b`
    /// (RFC-0013 §3.3, one spelling for inequality), and the tree holds the repair. Written
    /// with parentheses, `not (a is b)` is left alone. `!a == b` is not this: in the languages
    /// that write it, `!` binds to `a`.
    fn not_is(&mut self, e: Expr, first_diag: usize) -> Expr {
        let ExprKind::Unary { operand, .. } = &e.kind else {
            return e;
        };
        let ExprKind::Binary {
            op: op @ (BinOp::Is | BinOp::IsNot),
            op_span,
            lhs,
            rhs,
        } = &operand.kind
        else {
            return e;
        };
        if operand.is_parenthesised() {
            return e;
        }
        let (flip, written) = if *op == BinOp::Is {
            (BinOp::IsNot, "not a is b")
        } else {
            (BinOp::Is, "not a is not b")
        };
        let flipped = Expr {
            span: e.span,
            kind: ExprKind::Binary {
                op: flip,
                op_span: *op_span,
                lhs: lhs.clone(),
                rhs: rhs.clone(),
            },
        };
        let confidence = self.fold_inner_fixes(e.span, first_diag);
        let fixed = canonical(&flipped);
        let d = Diagnostic::error(
            "MZ0910",
            &self.file,
            e.span,
            format!("`{written}` is not how Mzizi asks for inequality — write `{fixed}`"),
        );
        self.diags.push(if flipped.has_error() {
            d
        } else {
            d.with_fix(e.span, fixed, confidence)
        });
        flipped
    }

    /// A comparison operator at the cursor, consumed, with any idiom reported.
    fn cmp_op(&mut self) -> Option<(BinOp, Span)> {
        let at = self.span();
        let op = match self.peek().clone() {
            Tok::Keyword("is") => {
                self.bump();
                if self.is_word("not") {
                    let not_at = self.bump().span;
                    return Some((BinOp::IsNot, join(at, not_at)));
                }
                BinOp::Is
            }
            Tok::Op(o @ ("<" | "<=" | ">" | ">=")) => {
                self.bump();
                match o {
                    "<" => BinOp::Lt,
                    "<=" => BinOp::Le,
                    ">" => BinOp::Gt,
                    _ => BinOp::Ge,
                }
            }
            Tok::Op(o @ ("==" | "===")) => {
                self.bump();
                self.idiom_op(o, "is", at);
                BinOp::Is
            }
            Tok::Op(o @ ("!=" | "!==")) => {
                self.bump();
                self.idiom_op(o, "is not", at);
                BinOp::IsNot
            }
            Tok::Ident(w) if w == "at_least" || w == "at_most" => {
                self.bump();
                let (op, sym) = if w == "at_least" {
                    (BinOp::Ge, ">=")
                } else {
                    (BinOp::Le, "<=")
                };
                self.diags.push(
                    Diagnostic::error(
                        "MZ0910",
                        &self.file,
                        at,
                        format!("`{w}` is a contract predicate — in an expression, write `{sym}`"),
                    )
                    .with_fix(at, sym, Confidence::Exact),
                );
                op
            }
            _ => return None,
        };
        Some((op, at))
    }

    fn cmp_expr(&mut self) -> Expr {
        let lhs = self.add_expr();
        let Some((op, op_at)) = self.cmp_op() else {
            return lhs;
        };
        let rhs = self.add_expr();
        let mut height = 0;
        if self.chain_too_deep(&mut height, &lhs, &rhs, op_at) {
            return self.error_expr(join(lhs.span, rhs.span));
        }
        let first = binary(op, op_at, lhs, rhs);
        let mut links = vec![first];
        while let Some((op, op_at)) = self.cmp_op() {
            let middle = match &links.last().expect("one link").kind {
                ExprKind::Binary { rhs, .. } => (**rhs).clone(),
                _ => unreachable!("links are binary"),
            };
            let rhs = self.add_expr();
            // `a < b < c < …` is repaired to `a < b and b < c and …`, a tree one level
            // deeper per link, over the deepest link.
            let link = binary(op, op_at, middle, rhs);
            if self.chain_too_deep(&mut height, &links[0], &link, op_at) {
                return self.error_expr(join(links[0].span, link.span));
            }
            links.push(link);
        }
        if links.len() == 1 {
            return links.pop().expect("one link");
        }
        // `a < b < c`: comparisons do not chain (RFC-0013 §3.3). The fix is a guess,
        // because `b` is evaluated once in one and twice in the other.
        let mut chain = links.remove(0);
        for link in links {
            let at = link.span;
            chain = binary(BinOp::And, at, chain, link);
        }
        let fixed = canonical(&chain);
        self.diags.push(
            Diagnostic::error(
                "MZ0913",
                &self.file,
                chain.span,
                format!("comparisons do not chain — write `{fixed}`"),
            )
            .with_fix(chain.span, fixed, Confidence::Guess),
        );
        self.failed = true;
        chain
    }

    fn add_expr(&mut self) -> Expr {
        let mut lhs = self.mul_expr();
        let mut height = 0;
        loop {
            let op = match self.peek() {
                Tok::Op("+") => BinOp::Add,
                Tok::Op("-") => BinOp::Sub,
                _ => return lhs,
            };
            let at = self.bump().span;
            let rhs = self.mul_expr();
            if self.chain_too_deep(&mut height, &lhs, &rhs, at) {
                return self.error_expr(join(lhs.span, rhs.span));
            }
            lhs = binary(op, at, lhs, rhs);
        }
    }

    fn mul_expr(&mut self) -> Expr {
        let mut lhs = self.unary_expr();
        let mut height = 0;
        loop {
            let op = match self.peek() {
                Tok::Op("*") => BinOp::Mul,
                Tok::Op("/") => BinOp::Div,
                Tok::Op("%") => BinOp::Rem,
                _ => return lhs,
            };
            let at = self.bump().span;
            let rhs = self.unary_expr();
            if self.chain_too_deep(&mut height, &lhs, &rhs, at) {
                return self.error_expr(join(lhs.span, rhs.span));
            }
            lhs = binary(op, at, lhs, rhs);
        }
    }

    fn unary_expr(&mut self) -> Expr {
        let at = self.span();
        if matches!(self.peek(), Tok::Op("-")) {
            self.bump();
            if self.nest >= PROGRAM_NESTING {
                return self.too_deep_expr(at);
            }
            self.nest += 1;
            let operand = self.unary_expr();
            self.nest -= 1;
            return Expr {
                span: join(at, operand.span),
                kind: ExprKind::Unary {
                    op: UnOp::Neg,
                    operand: Box::new(operand),
                },
            };
        }
        if self.is_word("not") || matches!(self.peek(), Tok::Op("!")) {
            return self.not_expr();
        }
        if self.is_word("try") && !matches!(self.peek_at(1), Tok::Newline | Tok::Eof) {
            return self.try_prefix();
        }
        self.primary()
    }

    /// Whether the keyword `k` at the cursor is used as a name already reported: a
    /// parameter of this function, or a function's name, called or used as a value.
    /// `true`, `false` and `none` keep their meaning as values whatever was declared.
    fn keyword_name(&self, k: &str) -> bool {
        !matches!(k, "true" | "false" | "none") && self.keyword_names.iter().any(|n| n == k)
    }

    fn error_expr(&self, at: Span) -> Expr {
        Expr {
            kind: ExprKind::Error,
            span: at,
        }
    }

    fn primary(&mut self) -> Expr {
        let at = self.span();
        let tok = self.peek().clone();
        let e = match tok {
            Tok::Int(v) => {
                self.bump();
                Expr {
                    kind: ExprKind::Int(v),
                    span: at,
                }
            }
            Tok::Float(v) => {
                self.bump();
                Expr {
                    kind: ExprKind::Float(v),
                    span: at,
                }
            }
            Tok::Keyword(k) if self.keyword_name(k) => {
                // A keyword used as a name, already reported where it was declared.
                self.bump();
                if matches!(self.peek(), Tok::LParen) {
                    let (_, close) = self.args();
                    return self.error_expr(join(at, close));
                }
                self.error_expr(at)
            }
            Tok::BadInt => {
                // Already `MZ0103`: an error value, and nothing more on this line.
                self.bump();
                self.failed = true;
                self.error_expr(at)
            }
            Tok::Keyword("true") | Tok::Keyword("false") => {
                self.bump();
                Expr {
                    kind: ExprKind::Bool(tok == Tok::Keyword("true")),
                    span: at,
                }
            }
            Tok::Str(raw) => {
                self.bump();
                let parts = self.text(&raw, at);
                Expr {
                    kind: ExprKind::Text(parts),
                    span: at,
                }
            }
            Tok::LParen => {
                self.bump();
                let inner = self.expr();
                if matches!(self.peek(), Tok::RParen) {
                    let close = self.bump().span;
                    Expr {
                        kind: inner.kind,
                        span: join(at, close),
                    }
                } else {
                    if !self.failed {
                        let s = self.span();
                        let found = describe(self.peek());
                        self.err(
                            "MZ0917",
                            s,
                            format!(
                                "expected `)` to close the `(` on column {}, found {found}",
                                at.start_col
                            ),
                        );
                    }
                    self.error_expr(at)
                }
            }
            Tok::Ident(name) => self.name_or_call(name, at),
            Tok::Keyword(k @ ("when" | "match")) => return self.misplaced_block_value(k, at),
            Tok::Keyword("none") => {
                self.bump();
                self.err(
                    "MZ0919",
                    at,
                    "`none` is an option's absence, and options in a function body are designed (RFC-0013 §8) but not built yet",
                );
                self.error_expr(at)
            }
            other => {
                if !self.failed {
                    self.err(
                        "MZ0917",
                        at,
                        format!("expected a value, found {}", describe(&other)),
                    );
                }
                self.error_expr(at)
            }
        };
        if matches!(e.kind, ExprKind::Error) {
            return e;
        }
        let e = self.postfix(e);
        if matches!(self.peek(), Tok::Op("**")) {
            return self.power(e);
        }
        e
    }

    /// `a ** b`, Python's power operator. Mzizi has none (RFC-0013 §3.2): `MZ0910`, and the
    /// tree reads `a.pow(b)`. The fix is a `guess`, because `pow` takes an `int` exponent
    /// and `**` takes any number. `**` binds tighter than prefix `-` and to the right, as in
    /// Python, which is how a postfix `pow` reads too.
    fn power(&mut self, base: Expr) -> Expr {
        let at = self.bump().span;
        if self.nest >= PROGRAM_NESTING {
            return self.too_deep_expr(at);
        }
        self.nest += 1;
        let first_diag = self.diags.len();
        let exponent = self.unary_expr();
        self.nest -= 1;
        // `a ** b ** c`: the inner `**` is folded into this one diagnostic and its fix.
        let mut k = first_diag;
        while k < self.diags.len() {
            if self.diags[k].code == "MZ0910" && self.diags[k].say.starts_with("`**`") {
                self.diags.remove(k);
            } else {
                k += 1;
            }
        }
        let span = join(base.span, exponent.span);
        let e = Expr {
            span,
            kind: ExprKind::Method {
                recv: Box::new(base),
                name: "pow".to_string(),
                name_span: at,
                args: vec![exponent],
                called: true,
            },
        };
        let d = Diagnostic::error(
            "MZ0910",
            &self.file,
            at,
            "`**` is not a Mzizi operator — a power is the method `x.pow(n)`, with an int `n`",
        );
        self.diags.push(if e.has_error() {
            d
        } else {
            d.with_fix(span, canonical(&e), Confidence::Guess)
        });
        e
    }

    fn name_or_call(&mut self, name: String, at: Span) -> Expr {
        self.bump();
        // `connection_state.offline`: a variant named with its enum (RFC-0013 §3.1).
        if matches!(self.peek(), Tok::Dot)
            && self.enum_names.contains(&name)
            && let Tok::Ident(variant) = self.peek_at(1).clone()
        {
            self.bump();
            let name_span = self.bump().span;
            return Expr {
                kind: ExprKind::Variant {
                    enum_name: name,
                    name: variant,
                    name_span,
                },
                span: join(at, name_span),
            };
        }
        // `console.log(…)`, `fmt.Println(…)`, `System.out.println(…)`: print idioms.
        if matches!(self.peek(), Tok::Dot) {
            let mut path = vec![name.clone()];
            let mut k = 0;
            while matches!(self.peek_at(k), Tok::Dot)
                && let Tok::Ident(seg) = self.peek_at(k + 1)
            {
                path.push(seg.clone());
                k += 2;
            }
            let joined = path.join(".");
            if matches!(
                joined.as_str(),
                "console.log" | "fmt.println" | "system.out.println"
            ) && matches!(self.peek_at(k), Tok::LParen)
            {
                for j in 0..k {
                    self.suppress.push(self.span_at(j));
                }
                self.suppress.push(at);
                for _ in 0..k {
                    self.bump();
                }
                let (args, close) = self.args();
                let span = join(at, close);
                return self.print_call(at, &joined, args, span);
            }
            return Expr {
                kind: ExprKind::Name(name),
                span: at,
            };
        }
        // `println!(…)`: Rust's macro.
        if matches!(self.peek(), Tok::Op("!")) && matches!(self.peek_at(1), Tok::LParen) {
            let bang = self.bump().span;
            let (args, close) = self.args();
            let span = join(at, close);
            if matches!(name.as_str(), "println" | "print") {
                return self.print_call(join(at, bang), &format!("{name}!"), args, span);
            }
            self.err(
                "MZ0917",
                join(at, bang),
                format!("`{name}!` is a Rust macro, and Mzizi has none"),
            );
            return self.error_expr(span);
        }
        if !matches!(self.peek(), Tok::LParen) {
            return Expr {
                kind: ExprKind::Name(name),
                span: at,
            };
        }
        if let Some(e) = self.constructor_idiom(&name, at) {
            return e;
        }
        // `range(a, to = b)` (RFC-0013 §6.5, §7.3): the one call whose label this slice
        // reads. Labels on other calls wait for §6.5 to be built.
        let (args, close) = if name == "range" {
            self.args_labelled(Some((1, "to")))
        } else {
            self.args()
        };
        let span = join(at, close);
        if name == "print" || name == "puts" {
            return self.print_call(at, &name, args, span);
        }
        Expr {
            kind: ExprKind::Call {
                name,
                name_span: at,
                args,
            },
            span,
        }
    }

    /// `(a, b)`: the cursor is on `(`. Returns the arguments and the `)`'s span.
    fn args(&mut self) -> (Vec<Expr>, Span) {
        self.args_labelled(None)
    }

    /// [`P::args`], where `label` is an argument's position and the one label it may carry.
    fn args_labelled(&mut self, label: Option<(usize, &str)>) -> (Vec<Expr>, Span) {
        let open = self.bump().span;
        let mut args = Vec::new();
        if matches!(self.peek(), Tok::RParen) {
            return (args, self.bump().span);
        }
        loop {
            if let (Tok::Ident(n), Tok::Equals) = (self.peek(), self.peek_at(1))
                && label == Some((args.len(), n.as_str()))
            {
                self.bump();
                self.bump();
            } else if let (Tok::Ident(n), Tok::Colon) = (self.peek(), self.peek_at(1))
                && label == Some((args.len(), n.as_str()))
            {
                // `to: b`, as Swift and Kotlin label an argument (RFC-0013 §16, `MZ0927`):
                // Mzizi labels with `=`. The label is read, so the call is one diagnostic.
                let n = n.clone();
                let at = self.span();
                let written = join(at, self.span_at(1));
                self.err_fix(
                    "MZ0927",
                    written,
                    format!("`{n}:` is another language's label — Mzizi labels an argument with `=`: `{n} = …`"),
                    written,
                    format!("{n} ="),
                    Confidence::Exact,
                );
                self.failed = false;
                self.bump();
                self.bump();
            } else if let (Tok::Ident(n), Tok::Equals) =
                (self.peek().clone(), self.peek_at(1).clone())
            {
                let at = self.span();
                self.err(
                    "MZ0905",
                    join(at, self.span_at(1)),
                    format!(
                        "`{n} = …` is a named argument, and a call gives its arguments by position, in the order of the signature"
                    ),
                );
                self.bump();
                self.bump();
            }
            args.push(self.expr());
            match self.peek() {
                Tok::Comma => {
                    self.bump();
                }
                Tok::RParen => return (args, self.bump().span),
                other => {
                    if !self.failed {
                        let s = self.span();
                        let found = describe(other);
                        self.err(
                            "MZ0917",
                            s,
                            format!(
                                "expected `,` or `)` in the call opened on column {}, found {found}",
                                open.start_col
                            ),
                        );
                    }
                    let end = self.prev();
                    while !self.at_line_end() {
                        self.bump();
                    }
                    return (args, end);
                }
            }
        }
    }

    /// A call to `print`, however it was spelt. `print` takes exactly one value; any
    /// other spelling or arity is one `MZ0980` whose fix rewrites the whole call.
    fn print_call(&mut self, name_at: Span, written: &str, args: Vec<Expr>, span: Span) -> Expr {
        let ok_spelling = written == "print";
        if ok_spelling && args.len() == 1 {
            return Expr {
                kind: ExprKind::Call {
                    name: "print".to_string(),
                    name_span: name_at,
                    args,
                },
                span,
            };
        }
        let fixable = args.len() < 2
            || args
                .iter()
                .all(|a| matches!(a.kind, ExprKind::Text(_)) || !a.has_text_literal());
        let arg = match args.len() {
            0 => Expr {
                kind: ExprKind::Text(Vec::new()),
                span,
            },
            1 => args.into_iter().next().expect("one argument"),
            _ => {
                // Python's `print(a, b)` separates its arguments with a space.
                let mut parts = Vec::new();
                for (i, a) in args.into_iter().enumerate() {
                    if i > 0 {
                        parts.push(TextPart::Lit(" ".to_string()));
                    }
                    match a.kind {
                        ExprKind::Text(p) => parts.extend(p),
                        _ => parts.push(TextPart::Expr(a)),
                    }
                }
                let mut merged: Vec<TextPart> = Vec::new();
                for p in parts {
                    match (merged.last_mut(), p) {
                        (Some(TextPart::Lit(a)), TextPart::Lit(b)) => a.push_str(&b),
                        (_, p) => merged.push(p),
                    }
                }
                Expr {
                    kind: ExprKind::Text(merged),
                    span,
                }
            }
        };
        let fixed = match &arg.kind {
            ExprKind::Text(parts) => format!("print({})", canonical_text(parts)),
            _ => format!("print({})", canonical(&arg)),
        };
        let shown: String = if written.ends_with('!') || written.contains('.') {
            format!("`{written}(…)`")
        } else {
            format!("`{written}` with these arguments")
        };
        let say = format!("{shown} is not how Mzizi prints — `print` takes one value: `{fixed}`");
        if fixable && !self.failed {
            self.err_fix("MZ0980", span, say, span, fixed, Confidence::Exact);
        } else if !self.failed {
            self.err("MZ0980", span, say);
        }
        Expr {
            kind: ExprKind::Call {
                name: "print".to_string(),
                name_span: name_at,
                args: vec![arg],
            },
            span,
        }
    }

    /// A text literal's pieces: escapes decoded, `{…}` parsed as an expression
    /// (RFC-0013 §3.6). `at` is the token's span; the raw text starts one column after.
    fn text(&mut self, raw: &str, at: Span) -> Vec<TextPart> {
        let chars: Vec<char> = raw.chars().collect();
        let col0 = at.start_col + 1;
        let line = at.start_line;
        let mut parts = Vec::new();
        let mut lit = String::new();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            let here = Span::single(line, col0 + k as u32, 1);
            match c {
                '\\' => {
                    let decoded = match chars.get(k + 1) {
                        Some('n') => Some('\n'),
                        Some('t') => Some('\t'),
                        Some('"') => Some('"'),
                        Some('\\') => Some('\\'),
                        Some('{') => Some('{'),
                        Some('}') => Some('}'),
                        _ => None,
                    };
                    match decoded {
                        Some(d) => lit.push(d),
                        None => {
                            let shown: String = chars[k..(k + 2).min(chars.len())].iter().collect();
                            self.err(
                                "MZ0917",
                                Span::single(line, col0 + k as u32, shown.chars().count() as u32),
                                format!(
                                    "`{shown}` is not an escape — the escapes are `\\n`, `\\t`, `\\\"`, `\\\\`, `\\{{` and `\\}}`"
                                ),
                            );
                        }
                    }
                    k += 2;
                }
                '{' => {
                    let close = chars[k + 1..].iter().position(|c| *c == '}' || *c == '{');
                    match close.map(|o| k + 1 + o) {
                        Some(end) if chars[end] == '}' => {
                            let inner: String = chars[k + 1..end].iter().collect();
                            let span = Span::single(line, col0 + k as u32, (end - k + 1) as u32);
                            if inner.trim().is_empty() {
                                self.err(
                                    "MZ0714",
                                    span,
                                    "`{}` interpolates nothing — put a value inside, or write `\\{\\}` for the braces",
                                );
                            } else if let Some(e) =
                                self.interpolation(&inner, line, col0 + k as u32 + 1, span)
                            {
                                if !lit.is_empty() {
                                    parts.push(TextPart::Lit(std::mem::take(&mut lit)));
                                }
                                parts.push(TextPart::Expr(e));
                            }
                            k = end + 1;
                        }
                        _ => {
                            self.err(
                                "MZ0714",
                                here,
                                "this `{` is not closed in its text — an interpolation holds no string literal and no braces; bind the value with `let` first, or write `\\{` for a brace",
                            );
                            return parts;
                        }
                    }
                }
                '}' => {
                    self.err_fix(
                        "MZ0714",
                        here,
                        "a `}` with no `{` — write `\\}` for a brace in text",
                        here,
                        "\\}",
                        Confidence::Exact,
                    );
                    k += 1;
                }
                c => {
                    lit.push(c);
                    k += 1;
                }
            }
        }
        if !lit.is_empty() {
            parts.push(TextPart::Lit(lit));
        }
        parts
    }

    /// The inside of `{…}`, as an expression with spans in the file's coordinates.
    fn interpolation(&mut self, inner: &str, line: u32, col: u32, braces: Span) -> Option<Expr> {
        let (mut toks, mut lex_diags) = lex_fragment(inner, &self.file);
        let shift = |s: &mut Span| {
            s.start_line = line;
            s.end_line = line;
            s.start_col += col - 1;
            s.end_col += col - 1;
        };
        for t in &mut toks {
            shift(&mut t.span);
        }
        for d in &mut lex_diags {
            shift(&mut d.span);
            if let Some(f) = d.fix.as_mut() {
                shift(&mut f.span);
            }
        }
        let mut sub = P {
            toks,
            pos: 0,
            file: self.file.clone(),
            diags: Vec::new(),
            skipped: Vec::new(),
            suppress: Vec::new(),
            failed: false,
            skipped_stray: false,
            nest: self.nest,
            too_deep: self.too_deep,
            keyword_names: self.keyword_names.clone(),
            keyword_fns: self.keyword_fns,
            fn_names: Rc::clone(&self.fn_names),
            enum_names: Rc::clone(&self.enum_names),
            match_open: 0,
            block_value: None,
            pending_skip: false,
            dropped: Vec::new(),
            known: std::rc::Rc::clone(&self.known),
            ret: self.ret,
        };
        let e = sub.expr();
        if !sub.at_line_end() && !sub.failed {
            let at = sub.span();
            let found = describe(sub.peek());
            sub.err(
                "MZ0714",
                at,
                format!(
                    "{found} is left over inside `{{{inner}}}` — an interpolation holds one value"
                ),
            );
        }
        let failed = sub.failed;
        self.too_deep |= sub.too_deep;
        // Past the cap (`MZ0411`) the fragment is not read, so its lexer's diagnostics go.
        if sub.skipped.is_empty() {
            self.diags.append(&mut lex_diags);
        }
        self.skipped.append(&mut sub.skipped);
        self.diags.append(&mut sub.diags);
        self.suppress.append(&mut sub.suppress);
        if failed {
            self.failed = true;
            return Some(Expr {
                kind: ExprKind::Error,
                span: braces,
            });
        }
        Some(e)
    }
}

/// How many levels deep `e` is, counted without recursion.
fn depth(e: &Expr) -> usize {
    let mut deepest = 0;
    let mut todo = vec![(e, 1usize)];
    while let Some((e, d)) = todo.pop() {
        deepest = deepest.max(d);
        match &e.kind {
            ExprKind::Binary { lhs, rhs, .. } => {
                todo.push((lhs, d + 1));
                todo.push((rhs, d + 1));
            }
            ExprKind::Unary { operand, .. } => todo.push((operand, d + 1)),
            ExprKind::Field { base, .. } => todo.push((base, d + 1)),
            ExprKind::Call { args, .. } => todo.extend(args.iter().map(|a| (a, d + 1))),
            ExprKind::Method { recv, args, .. } => {
                todo.push((recv, d + 1));
                todo.extend(args.iter().map(|a| (a, d + 1)));
            }
            ExprKind::Text(parts) => todo.extend(parts.iter().filter_map(|p| match p {
                TextPart::Expr(x) => Some((x, d + 1)),
                TextPart::Lit(_) => None,
            })),
            ExprKind::When { arms, otherwise } => {
                for (c, v) in arms {
                    todo.push((c, d + 1));
                    todo.push((v, d + 1));
                }
                todo.extend(otherwise.iter().map(|o| (&**o, d + 1)));
            }
            ExprKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                todo.push((scrutinee, d + 1));
                for a in arms {
                    todo.extend(a.values.iter().map(|v| (v, d + 1)));
                    todo.push((&a.body, d + 1));
                }
                todo.extend(otherwise.iter().map(|o| (&o.body, d + 1)));
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Name(_)
            | ExprKind::Variant { .. }
            | ExprKind::Error => {}
        }
    }
    deepest
}

/// An `and` written without parentheses, as an operand of `or` (`MZ0913`).
fn is_bare_and(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::Binary { op: BinOp::And, .. }) && !e.is_parenthesised()
}

fn binary(op: BinOp, op_span: Span, lhs: Expr, rhs: Expr) -> Expr {
    Expr {
        span: join(lhs.span, rhs.span),
        kind: ExprKind::Binary {
            op,
            op_span,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
    }
}

/// Whether a line starting with `w` is something the program level reads itself, rather
/// than a stray statement.
fn is_program_item(w: &str, next: &Tok) -> bool {
    match w {
        "fn" | "end" | "use" | "enum" | "contract" | "record" | "view" | "prop" | "emit"
        | "route" | "fallback" | "header" => true,
        "def" | "function" | "func" => matches!(next, Tok::Ident(_)),
        "test" => matches!(next, Tok::Str(_)),
        _ => false,
    }
}

/// The line a block's statement list stopped on.
fn stop_line(stop: &Stop) -> u32 {
    match stop {
        Stop::End(s)
        | Stop::Else(s)
        | Stop::Elif(s)
        | Stop::Case(s)
        | Stop::EndFn(s)
        | Stop::Abrupt(s)
        | Stop::Eof(s) => s.start_line,
    }
}
