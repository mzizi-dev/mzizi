//! Recursive-descent parser with per-line recovery.
//!
//! Two guarantees this module exists to deliver, both from RFC-0001:
//!
//! - **§1.1, the `end <kind> <name>` cross-check.** A block stack records what was opened
//!   and where. A wrong or missing closer produces one precisely-located diagnostic naming
//!   the opener's line — never a cascade. RFC-0002 §1 gives the deeper reason: the echo is
//!   an error-correcting code for a reader (or model) that cannot reliably track nesting.
//! - **§4.1, at most one diagnostic per true author error.** Recovery is line-oriented:
//!   on anything unexpected, report once and skip to the next newline. Because statements
//!   are newline-terminated, the next line is always a valid resynchronization point.

use crate::ast::{
    Attr, Component, Element, Emit, EnumDecl, FieldDecl, PropDecl, RecordDecl, TypeExpr, TypeKind,
    Variant,
};
use crate::contract::{Clause, Contract, PREDICATE_WORDS, Predicate, Subject};
use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::lex::{Tok, Token, lex};
use crate::service::Service;

mod program;
mod service;

/// A parsed file: its one top-level declaration.
#[derive(Debug, PartialEq)]
pub enum Program {
    /// `component <name>` … `end component <name>`.
    Component(Component),
    /// `service <name>` … `end service <name>` (RFC-0011).
    Service(Service),
    /// `program <name>` … `end program <name>` (RFC-0013).
    Program(crate::program::Program),
}

/// Parse a source file. Returns the component when the shape was recoverable, plus every
/// diagnostic found — parsing never stops at the first error.
pub fn parse(src: &str, file: &str) -> (Option<Component>, Vec<Diagnostic>) {
    let (program, diags) = parse_program(src, file);
    match program {
        Some(Program::Component(c)) => (Some(c), diags),
        _ => (None, diags),
    }
}

/// Parse a source file holding any kind of top-level declaration.
pub fn parse_program(src: &str, file: &str) -> (Option<Program>, Vec<Diagnostic>) {
    let (tokens, mut diags) = lex(src, file);
    if program::starts_program(&tokens) {
        let parsed = program::parse(tokens, &mut diags, file);
        return (parsed.map(Program::Program), diags);
    }
    let mut p = Parser {
        tokens,
        pos: 0,
        file: file.to_string(),
        diags: Vec::new(),
        unwinding: false,
        service: None,
        skipped: Vec::new(),
        too_deep: false,
    };
    let component = p.parse_file();
    let program = match p.service.take() {
        Some(service) => Some(Program::Service(service)),
        None => component.map(Program::Component),
    };
    // `GET` as a method is one mistake, which the parser reports with the method's own
    // fix; the lexer's snake_case repair (`g_e_t`) on the same token would be a second.
    diags.retain(|d| {
        d.code != "MZ0101"
            || !p
                .diags
                .iter()
                .any(|m| matches!(m.code, "MZ0801" | "MZ0601") && m.span == d.span)
    });
    diags.retain(|d| {
        !p.skipped
            .iter()
            .any(|&(from, to)| (from..=to).contains(&d.span.start_line))
    });
    diags.append(&mut p.diags);
    (program, diags)
}

/// What kind of block is open, for the `end` cross-check.
#[derive(Clone, Copy, PartialEq, Debug)]
enum BlockKind {
    Component,
    Enum,
    Record,
    View,
    Fn,
    Contract,
    Element,
    Service,
    Route,
    Fallback,
    When,
}

impl BlockKind {
    fn word(self) -> &'static str {
        match self {
            BlockKind::Component => "component",
            BlockKind::Enum => "enum",
            BlockKind::Record => "record",
            BlockKind::View => "view",
            BlockKind::Fn => "fn",
            BlockKind::Contract => "contract",
            BlockKind::Element => "element",
            BlockKind::Service => "service",
            BlockKind::Route => "route",
            BlockKind::Fallback => "fallback",
            BlockKind::When => "when",
        }
    }

    /// Whether `end` for this block must echo `<kind> <name>` (top-level declarations do).
    fn requires_echo(self) -> bool {
        matches!(self, BlockKind::Component | BlockKind::Service)
    }
}

struct Open {
    kind: BlockKind,
    name: String,
    line: u32,
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    file: String,
    diags: Vec<Diagnostic>,
    /// Set when a line that can only start a component-level declaration (`contract`,
    /// `prop`, …) turns up inside a `view`: every open view block stops where it is, and
    /// the component loop reports them once and carries on from that line.
    unwinding: bool,
    /// The service, when the file holds one rather than a component (RFC-0011).
    service: Option<Service>,
    /// Line ranges the parser skipped whole as one error (a `match` block, `MZ0410`); the
    /// lexer's diagnostics inside them are dropped.
    skipped: Vec<(u32, u32)>,
    /// Whether `MZ0411` has been reported: it is reported once per file, because one
    /// nesting attack is one error however many blocks it puts past the cap.
    too_deep: bool,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos.min(self.tokens.len() - 1)].span
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }

    fn eat_keyword(&mut self, kw: &str) -> bool {
        if matches!(self.peek(), Tok::Keyword(k) if *k == kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
    }

    /// Line-oriented recovery: consume through the end of the current line.
    fn recover_line(&mut self) {
        while !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            self.bump();
        }
        self.skip_newlines();
    }

    fn ident(&mut self) -> Option<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let span = self.peek_span();
                self.bump();
                Some((name, span))
            }
            _ => None,
        }
    }

    /// A value as written: string, int, identifier, dotted path, or keyword literal.
    fn value(&mut self) -> Option<String> {
        self.value_spanned().map(|(value, _, _)| value)
    }

    /// [`Parser::value`], with the value's span and each dotted segment's span.
    fn value_spanned(&mut self) -> Option<(String, Span, Vec<Span>)> {
        let start = self.peek_span();
        let mut segments = Vec::new();
        if matches!(self.peek(), Tok::Ident(_)) {
            segments.push(start);
        }
        let mut out = match self.peek().clone() {
            Tok::Str(s) => {
                self.bump();
                format!("\"{s}\"")
            }
            Tok::Int(v) => {
                self.bump();
                v.to_string()
            }
            Tok::Ident(name) => {
                self.bump();
                name
            }
            Tok::Keyword(k) if matches!(k, "true" | "false" | "nothing" | "none") => {
                self.bump();
                k.to_string()
            }
            _ => return None,
        };
        let mut end = start;
        // Dotted continuation: `state.color`
        while matches!(self.peek(), Tok::Dot) {
            self.bump();
            match self.ident() {
                Some((seg, span)) => {
                    out.push('.');
                    out.push_str(&seg);
                    segments.push(span);
                    end = span;
                }
                None => break,
            }
        }
        let span = Span {
            start_line: start.start_line,
            start_col: start.start_col,
            end_line: end.end_line,
            end_col: end.end_col,
        };
        Some((out, span, segments))
    }

    /// A value as an [`Attr`] with the given name.
    fn attr(&mut self, name: &str) -> Option<Attr> {
        self.value_spanned().map(|(value, span, segments)| Attr {
            name: name.to_string(),
            value,
            span,
            segments,
        })
    }

    /// A type expression: a name, `none`, or `<ctor>(<type>)` (RFC-0008 §1).
    ///
    /// The parser only builds the shape; which names exist, and which constructors take
    /// which arguments, is the resolver's business, so a misspelt type is reported once
    /// with a nearest-name fix instead of as a parse failure.
    fn parse_type(&mut self, what: &str) -> Option<TypeExpr> {
        let start = self.peek_span();
        let name = match self.peek().clone() {
            Tok::Ident(name) => name,
            Tok::Keyword("event") => "event".to_string(),
            Tok::Keyword("none") => {
                self.bump();
                return Some(TypeExpr {
                    kind: TypeKind::Nothing,
                    span: start,
                });
            }
            other => {
                self.diags.push(Diagnostic::error(
                    "MZ0306",
                    &self.file,
                    start,
                    format!("expected a type after {what}, found {}", describe(&other)),
                ));
                return None;
            }
        };
        self.bump();
        if !matches!(self.peek(), Tok::LParen) {
            return Some(TypeExpr {
                kind: TypeKind::Name(name),
                span: start,
            });
        }
        self.bump();
        if matches!(self.peek(), Tok::RParen) {
            let span = self.peek_span();
            self.diags.push(Diagnostic::error(
                "MZ0309",
                &self.file,
                span,
                format!("`{name}()` needs a type inside the parentheses, e.g. `{name}(text)`"),
            ));
            return None;
        }
        let inner = self.parse_type(&format!("`{name}(`"))?;
        if !matches!(self.peek(), Tok::RParen) {
            let span = self.peek_span();
            // Inserting `)` is only a safe repair when the line ends here. Anything else
            // (`list(text, int)`) means the `)` is further on, and inserting one would
            // leave the line broken.
            if matches!(self.peek(), Tok::Newline | Tok::Eof) {
                self.diags.push(
                    Diagnostic::error(
                        "MZ0309",
                        &self.file,
                        span,
                        format!("`{name}({inner}` is missing its `)`"),
                    )
                    .with_fix(
                        Span::single(inner.span.end_line, inner.span.end_col, 0),
                        ")",
                        Confidence::Exact,
                    ),
                );
            } else {
                self.diags.push(Diagnostic::error(
                    "MZ0309",
                    &self.file,
                    span,
                    format!(
                        "`{name}({inner}` must close with `)` here — a type constructor takes exactly one type"
                    ),
                ));
            }
            return None;
        }
        let close = self.peek_span();
        self.bump();
        Some(TypeExpr {
            kind: TypeKind::Apply(name, Box::new(inner)),
            span: Span {
                start_line: start.start_line,
                start_col: start.start_col,
                end_line: close.end_line,
                end_col: close.end_col,
            },
        })
    }

    /// Report anything left on a declaration line. It used to be dropped without a word,
    /// so `prop x: text garbage` compiled — the silent acceptance FM-12 names.
    fn expect_line_end(&mut self, what: &str) {
        if !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            let span = self.peek_span();
            self.diags.push(Diagnostic::error(
                "MZ0310",
                &self.file,
                span,
                format!(
                    "{} is left over after {what} — one declaration per line",
                    describe(self.peek())
                ),
            ));
        }
    }

    fn parse_file(&mut self) -> Option<Component> {
        self.skip_newlines();

        let mut docs = Vec::new();
        while let Tok::Doc(text) = self.peek().clone() {
            docs.push(text);
            self.bump();
            self.skip_newlines();
        }

        if matches!(self.peek(), Tok::Ident(w) if w == "service") {
            self.service = self.parse_service(docs);
            return None;
        }

        if !self.eat_keyword("component") {
            let span = self.peek_span();
            self.diags.push(Diagnostic::error(
                "MZ0201",
                &self.file,
                span,
                format!(
                    "a .mz file starts with `component <name>`, `service <name>` or `program <name>`, found {}",
                    describe(self.peek())
                ),
            ));
            return None;
        }

        let (name, name_span) = match self.ident() {
            Some(pair) => pair,
            None => {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0202",
                    &self.file,
                    span,
                    format!(
                        "`component` needs a snake_case name, found {}",
                        describe(self.peek())
                    ),
                ));
                return None;
            }
        };

        let mut stack = vec![Open {
            kind: BlockKind::Component,
            name: name.clone(),
            line: name_span.start_line,
        }];

        let mut component = Component {
            name: name.clone(),
            name_span,
            docs,
            uses: Vec::new(),
            enums: Vec::new(),
            records: Vec::new(),
            props: Vec::new(),
            view: None,
            fns: Vec::new(),
            emits: Vec::new(),
            broken: Vec::new(),
            contract: None,
        };

        self.recover_line();

        while !self.at_eof() {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }

            match self.peek().clone() {
                Tok::Doc(text) => {
                    component.docs.push(text);
                    self.bump();
                    self.recover_line();
                }
                Tok::Keyword("use") => {
                    self.bump();
                    match self.ident() {
                        Some((cap, _)) => component.uses.push(cap),
                        None => {
                            let span = self.peek_span();
                            self.diags.push(Diagnostic::error(
                                "MZ0203",
                                &self.file,
                                span,
                                "`use` needs a capability name, e.g. `use net`",
                            ));
                        }
                    }
                    self.recover_line();
                }
                Tok::Keyword("enum") => {
                    if let Some(e) = self.parse_enum(&mut stack) {
                        component.enums.push(e);
                    }
                }
                Tok::Ident(word) if word == "record" => {
                    if let Some(r) = self.parse_record(&mut stack) {
                        component.records.push(r);
                    }
                }
                Tok::Keyword("prop") => match self.parse_prop() {
                    Ok(pr) => component.props.push(pr),
                    Err(Some(name)) => component.broken.push(name),
                    Err(None) => {}
                },
                Tok::Keyword("view") => {
                    self.bump();
                    stack.push(Open {
                        kind: BlockKind::View,
                        name: String::new(),
                        line: self.peek_span().start_line,
                    });
                    self.recover_line();
                    self.unwinding = false;
                    let children = self.parse_elements(&mut stack);
                    component.view = Some(children);
                    if self.unwinding {
                        self.unwinding = false;
                        self.close_unclosed_view(&mut stack);
                    }
                }
                Tok::Keyword("fn") => {
                    self.bump();
                    let mut fname = String::new();
                    if let Some((name, _)) = self.ident() {
                        component.fns.push(name.clone());
                        fname = name.clone();
                        stack.push(Open {
                            kind: BlockKind::Fn,
                            name,
                            line: self.peek_span().start_line,
                        });
                    }
                    self.recover_line();
                    let mut emits = self.parse_fn_body(&mut stack, &fname);
                    component.emits.append(&mut emits);
                }
                Tok::Keyword("contract") => {
                    self.bump();
                    stack.push(Open {
                        kind: BlockKind::Contract,
                        name: String::new(),
                        line: self.peek_span().start_line,
                    });
                    self.recover_line();
                    // A second `contract` block would silently replace the first. Keep the
                    // first and report the second, so no assertion is lost without a word.
                    let parsed = self.parse_contract(&mut stack);
                    if component.contract.is_some() {
                        let span = self.peek_span();
                        self.diags.push(Diagnostic::error(
                            "MZ0209",
                            &self.file,
                            span,
                            format!(
                                "`component {}` already has a `contract` block — merge these {} assertion(s) into it",
                                component.name,
                                parsed.clauses.len()
                            ),
                        ));
                    } else {
                        component.contract = Some(parsed);
                    }
                }
                Tok::Keyword("end") => {
                    self.parse_end(&mut stack);
                }
                other => {
                    let span = self.peek_span();
                    self.diags.push(Diagnostic::error(
                        "MZ0401",
                        &self.file,
                        span,
                        format!(
                            "{} cannot start a line inside `component {}` — expected one of use, enum, record, prop, view, fn, contract, end",
                            describe(&other), component.name
                        ),
                    ));
                    self.recover_line();
                }
            }
        }

        self.close_at_eof(&mut stack);

        self.as_child(&component);

        if component.contract.is_none() {
            self.diags.push(Diagnostic::warning(
                "MZ0501",
                &self.file,
                name_span,
                format!(
                    "`component {}` has no `contract` block — behaviour is unverified (RFC-0001 §1.6)",
                    component.name
                ),
            ));
        }

        Some(component)
    }

    /// Anything still open at EOF is unclosed. One diagnostic each, innermost first,
    /// every one carrying the exact text that would close it.
    fn close_at_eof(&mut self, stack: &mut Vec<Open>) {
        while let Some(open) = stack.pop() {
            let eof = self.peek_span();
            let closer = if open.kind.requires_echo() {
                format!("end {} {}", open.kind.word(), open.name)
            } else {
                "end".to_string()
            };
            self.diags.push(
                Diagnostic::error(
                    "MZ0204",
                    &self.file,
                    eof,
                    format!(
                        "`{}` opened on line {} is never closed — add `{}`",
                        block_label(&open),
                        open.line,
                        closer
                    ),
                )
                .with_fix(
                    Span::single(eof.start_line, eof.start_col, 0),
                    // A last line with no newline needs one before the closer.
                    if eof.start_col > 1 {
                        format!("\n{closer}")
                    } else {
                        format!("{closer}\n")
                    },
                    Confidence::Exact,
                ),
            );
        }
    }

    /// `prop as_child` — React's `asChild`, which renders the component *as* its child by
    /// merging props into it through Radix's `Slot`. Mzizi elements are not polymorphic,
    /// so the flag promises something the port cannot do. Every shadcn/Radix spec carries
    /// it, and pilot 2's 7B model built each badge around it (`Comp = Slot.Root`).
    ///
    /// It is a warning, not an error: a declared, unused `as_child` compiles to a
    /// component that renders its default element, which is what the reference does with
    /// the flag off, and two of pilot 2's clean 7B buttons declared one. Failing them would
    /// cost an iteration for nothing. When the view never reads it, deleting the line is
    /// the whole repair, so the fix is `exact`. When a `when` tests it, the branch has to
    /// go too, which is the author's edit, so there is no fix.
    fn as_child(&mut self, component: &Component) {
        fn reads(els: &[Element]) -> Option<u32> {
            els.iter().find_map(|e| {
                let here = e.attrs.iter().any(|a| {
                    a.value == "as_child"
                        || a.value.starts_with("as_child.")
                        || a.value.contains("{as_child")
                });
                if here {
                    Some(e.span.start_line)
                } else {
                    reads(&e.children).or_else(|| e.else_children.as_deref().and_then(reads))
                }
            })
        }
        let Some(prop) = component.props.iter().find(|p| p.name == "as_child") else {
            return;
        };
        let line = prop.span.start_line;
        let used = component.view.as_deref().and_then(reads);
        let base = "`as_child` is React's `asChild` (render as the child via `Slot`), and Mzizi elements are not polymorphic";
        let d = match used {
            None => Diagnostic::warning(
                "MZ0312",
                &self.file,
                prop.span,
                format!("{base}: delete this prop"),
            )
            .with_fix(
                Span {
                    start_line: line,
                    start_col: 1,
                    end_line: line + 1,
                    end_col: 1,
                },
                "",
                Confidence::Exact,
            ),
            Some(at) => Diagnostic::warning(
                "MZ0312",
                &self.file,
                prop.span,
                format!(
                    "{base}: delete this prop, and the branch on line {at} that reads it, keeping what `as_child = false` renders"
                ),
            ),
        };
        self.diags.push(d);
    }

    /// `end`, with the cross-check that makes a mismatch one diagnostic instead of a cascade.
    ///
    /// Every fix here rewrites the whole closer — `end`, the echoed kind and the echoed
    /// name — rather than the `end` word alone. Replacing only `end` left the echo behind
    /// it, so `end view` closing a `row` became `end element view` and a nameless
    /// `end component` became `end component a component` (pilot 2, divergence 7).
    fn parse_end(&mut self, stack: &mut Vec<Open>) {
        let end_span = self.peek_span();
        self.bump();

        let echoed_kind = match self.peek().clone() {
            Tok::Keyword(k) if BLOCK_WORDS.contains(&k) => {
                self.bump();
                Some(k)
            }
            // `record` is not a keyword (RFC-0008 §1), so its echo arrives as a word.
            Tok::Ident(word)
                if matches!(word.as_str(), "record" | "service" | "route" | "fallback") =>
            {
                self.bump();
                match word.as_str() {
                    "record" => Some("record"),
                    "service" => Some("service"),
                    "route" => Some("route"),
                    _ => Some("fallback"),
                }
            }
            _ => None,
        };
        let echoed_name = self.ident().map(|(n, _)| n);
        // The closer as written, `end` through the last echoed word.
        let closer_span = {
            let last = self.tokens[self.pos.saturating_sub(1)].span;
            Span {
                start_line: end_span.start_line,
                start_col: end_span.start_col,
                end_line: last.end_line,
                end_col: last.end_col,
            }
        };

        // `end component <name>` while blocks inside the component are still open: the
        // file's last line arrived early, because an `end` is missing further up. It used
        // to close the innermost block and report MZ0206 ("write `end element`") plus an
        // MZ0204 at end of file ("add `end component`") — two fixes that contradict each
        // other. It is one mistake: report the open blocks once, and close the component.
        if matches!(echoed_kind, Some("component" | "service"))
            && stack.len() > 1
            && stack.first().is_some_and(|o| {
                o.kind.requires_echo() && o.kind.word() == echoed_kind.unwrap_or("")
            })
        {
            let mut open = Vec::new();
            while stack.len() > 1 {
                open.push(stack.pop().expect("len checked"));
            }
            let names: Vec<String> = open
                .iter()
                .map(|o| format!("`{}` opened on line {}", block_label(o), o.line))
                .collect();
            self.diags.push(
                Diagnostic::error(
                    "MZ0204",
                    &self.file,
                    end_span,
                    format!(
                        "`end {}` arrived while {} {} still open — an `end` is missing above; add {} before this line",
                        echoed_kind.unwrap_or("component"),
                        names.join(", "),
                        if open.len() == 1 { "is" } else { "are" },
                        if open.len() == 1 {
                            "`end`".to_string()
                        } else {
                            format!("{} `end`s", open.len())
                        }
                    ),
                )
                // A `guess`, for the reason `close_unclosed_view` gives: the file parses
                // after it, but which line each missing `end` belonged on is not known.
                .with_fix(
                    Span::single(end_span.start_line, 1, 0),
                    "end\n".repeat(open.len()),
                    Confidence::Guess,
                ),
            );
            // Nothing is left for the enclosing view and element loops to read.
            self.unwinding = true;
        }

        let open = match stack.pop() {
            Some(o) => o,
            None => {
                self.diags.push(
                    Diagnostic::error(
                        "MZ0205",
                        &self.file,
                        end_span,
                        "`end` with nothing open — delete it",
                    )
                    .with_fix(closer_span, "", Confidence::Exact),
                );
                self.recover_line();
                return;
            }
        };
        let canonical = if open.kind.requires_echo() {
            format!("end {} {}", open.kind.word(), open.name)
        } else {
            "end".to_string()
        };

        if let Some(kind_word) = echoed_kind
            && kind_word != open.kind.word()
        {
            let say = if open.kind.requires_echo() {
                format!(
                    "`end {kind_word}` closes the `{}` opened on line {} — write `{canonical}`",
                    block_label(&open),
                    open.line,
                )
            } else {
                format!(
                    "`end {kind_word}` closes the `{}` opened on line {} — an inner block closes with a bare `end`",
                    block_label(&open),
                    open.line,
                )
            };
            self.diags.push(
                Diagnostic::error("MZ0206", &self.file, end_span, say).with_fix(
                    closer_span,
                    canonical.clone(),
                    Confidence::Exact,
                ),
            );
            self.recover_line();
            return;
        }

        if let Some(name) = echoed_name {
            if !open.name.is_empty() && name != open.name && !open.kind.requires_echo() {
                // `end enum g` for `enum g_tone`: an inner block needs no echo at all, so
                // the repair is the bare `end`, not a second spelling of the echo.
                self.diags.push(
                    Diagnostic::error(
                        "MZ0207",
                        &self.file,
                        closer_span,
                        format!(
                            "`end {} {name}` does not match `{}` opened on line {} — an inner block closes with a bare `end`",
                            open.kind.word(),
                            block_label(&open),
                            open.line
                        ),
                    )
                    .with_fix(closer_span, canonical, Confidence::Exact),
                );
            } else if !open.name.is_empty() && name != open.name {
                let span = self.tokens[self.pos.saturating_sub(1)].span;
                self.diags.push(
                    Diagnostic::error(
                        "MZ0207",
                        &self.file,
                        span,
                        format!(
                            "`end {} {}` does not match `{} {}` opened on line {}",
                            open.kind.word(),
                            name,
                            open.kind.word(),
                            open.name,
                            open.line
                        ),
                    )
                    .with_fix(span, open.name.clone(), Confidence::Exact),
                );
            }
        } else if open.kind.requires_echo() {
            self.diags.push(
                Diagnostic::error(
                    "MZ0208",
                    &self.file,
                    end_span,
                    format!("top-level blocks close with their name: write `{canonical}`"),
                )
                .with_fix(closer_span, canonical, Confidence::Exact),
            );
        }

        self.recover_line();
    }

    fn parse_enum(&mut self, stack: &mut Vec<Open>) -> Option<EnumDecl> {
        self.bump(); // `enum`
        let (name, name_span) = self.ident()?;
        stack.push(Open {
            kind: BlockKind::Enum,
            name: name.clone(),
            line: name_span.start_line,
        });
        self.recover_line();

        let mut variants = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                break;
            }
            let Some((vname, vspan)) = self.ident() else {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0301",
                    &self.file,
                    span,
                    format!("expected a variant name, found {}", describe(self.peek())),
                ));
                self.recover_line();
                continue;
            };
            let mut columns = Vec::new();
            let mut height_at = None;
            while let Some((col, _)) = self.ident() {
                match self.value_spanned() {
                    Some((v, vspan, _)) => {
                        if col == "height" {
                            height_at = Some(vspan);
                        }
                        columns.push((col, v));
                    }
                    None => {
                        let span = self.peek_span();
                        self.diags.push(Diagnostic::error(
                            "MZ0302",
                            &self.file,
                            span,
                            format!("column `{col}` needs a value, e.g. `{col} \"...\"`"),
                        ));
                        break;
                    }
                }
            }
            if let Some(at) = height_at {
                self.height_agrees_with_class(&vname, &columns, at);
            }
            variants.push(Variant {
                name: vname,
                span: vspan,
                columns,
            });
            self.recover_line();
        }

        // The whole point of columns-on-variants (RFC-0001 §1.3) is that a missing cell is
        // a compile error rather than the silent drift the corpus's parallel maps produced.
        if let Some(first) = variants.first() {
            let expected: Vec<String> = first.columns.iter().map(|(c, _)| c.clone()).collect();
            for v in variants.iter().skip(1) {
                for col in &expected {
                    if !v.columns.iter().any(|(c, _)| c == col) {
                        self.diags.push(Diagnostic::error(
                            "MZ0303",
                            &self.file,
                            v.span,
                            format!(
                                "variant `{}` has no `{}` column, but `{}` does — every variant needs every column",
                                v.name, col, first.name
                            ),
                        ));
                    }
                }
            }
        }

        Some(EnumDecl { name, variants })
    }

    /// FM-11 in the variant table: a row that writes `height` beside a `class` whose
    /// spacing-scale token renders a different height states one fact twice, and the two
    /// copies disagree. Both of pilot 2's clean 7B buttons wrote `icon class "size-14"
    /// height 48`: the class renders 56px, the contract checked 48, and `mz contract`
    /// passed. The class is what renders, so the repair rewrites the number, and it is
    /// `exact`. A row may omit `height` altogether; the evaluator then reads it from the
    /// class (`contract::scale_height`), so the fact is written once.
    fn height_agrees_with_class(&mut self, variant: &str, columns: &[(String, String)], at: Span) {
        let get = |c: &str| {
            columns
                .iter()
                .find(|(k, _)| k == c)
                .map(|(_, v)| v.as_str())
        };
        let (Some(height), Some(class)) = (get("height"), get("class")) else {
            return;
        };
        let (Ok(declared), Some(inner)) = (
            height.parse::<i64>(),
            class.strip_prefix('"').and_then(|c| c.strip_suffix('"')),
        ) else {
            return;
        };
        let Some((token, rendered)) = crate::contract::scale_height(inner) else {
            return;
        };
        if declared != rendered {
            self.diags.push(
                Diagnostic::error(
                    "MZ0313",
                    &self.file,
                    at,
                    format!(
                        "`{variant}` declares `height {declared}` but its class `{token}` renders {rendered}px (N × 4) — write `height {rendered}`, or drop the column and let the class say it once"
                    ),
                )
                .with_fix(at, rendered.to_string(), Confidence::Exact),
            );
        }
    }

    /// `prop <name>: <type> [= <default>]`.
    ///
    /// `Err(Some(name))` means the line named a prop but did not parse; the name is kept
    /// so the resolver does not report every later use of it as unknown (RFC-0001 §4.1).
    fn parse_prop(&mut self) -> Result<PropDecl, Option<String>> {
        self.bump(); // `prop`
        let (name, name_span) = match self.ident() {
            Some(pair) => pair,
            None => {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0304",
                    &self.file,
                    span,
                    format!("`prop` needs a name, found {}", describe(self.peek())),
                ));
                self.recover_line();
                return Err(None);
            }
        };

        if !matches!(self.peek(), Tok::Colon) {
            let span = self.peek_span();
            self.diags.push(
                Diagnostic::error(
                    "MZ0305",
                    &self.file,
                    span,
                    format!("`prop {name}` needs a type: write `prop {name}: <type>`"),
                )
                .with_fix(
                    Span::single(name_span.end_line, name_span.end_col, 0),
                    ": bool",
                    Confidence::Guess,
                ),
            );
            self.recover_line();
            return Err(Some(name));
        }
        self.bump();

        let Some(ty) = self.parse_type(&format!("`prop {name}:`")) else {
            self.recover_line();
            return Err(Some(name));
        };

        let mut has_default = false;
        let mut default = None;
        let mut default_span = None;
        if matches!(self.peek(), Tok::Equals) {
            let eq = self.peek_span();
            self.bump();
            if let Some((value, span, _)) = self.value_spanned() {
                default = Some(value);
                has_default = true;
                default_span = Some(Span {
                    start_line: eq.start_line,
                    start_col: eq.start_col,
                    end_line: span.end_line,
                    end_col: span.end_col,
                });
            }
        }
        self.expect_line_end(&format!("`prop {name}: {ty}`"));

        self.recover_line();
        Ok(PropDecl {
            name,
            span: name_span,
            ty,
            has_default,
            default,
            default_span,
        })
    }

    /// `record <name>`, then `field <name>: <type>` lines, then a bare `end` (RFC-0008 §2).
    fn parse_record(&mut self, stack: &mut Vec<Open>) -> Option<RecordDecl> {
        self.bump(); // `record`
        let Some((name, span)) = self.ident() else {
            let at = self.peek_span();
            self.diags.push(Diagnostic::error(
                "MZ0307",
                &self.file,
                at,
                format!(
                    "`record` needs a snake_case name, found {}",
                    describe(self.peek())
                ),
            ));
            self.recover_line();
            // Still consume the body, so its `field` lines are not read as top-level lines.
            stack.push(Open {
                kind: BlockKind::Record,
                name: String::new(),
                line: at.start_line,
            });
            self.skip_block_body(stack);
            return None;
        };
        stack.push(Open {
            kind: BlockKind::Record,
            name: name.clone(),
            line: span.start_line,
        });
        self.recover_line();

        let mut fields = Vec::new();
        let mut broken = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                break;
            }
            if matches!(self.peek(), Tok::Doc(_)) {
                self.recover_line();
                continue;
            }
            let line_start = self.peek_span();
            let is_field = matches!(self.peek(), Tok::Ident(w) if w == "field");
            // `version: text` is what TypeScript and Rust priors write. It is the form
            // with one word missing, so the repair is exact.
            let bare = matches!(self.peek(), Tok::Ident(_))
                && matches!(
                    self.tokens.get(self.pos + 1).map(|t| &t.kind),
                    Some(Tok::Colon)
                );
            if !is_field && !bare {
                self.diags.push(Diagnostic::error(
                    "MZ0308",
                    &self.file,
                    line_start,
                    format!(
                        "a record holds `field <name>: <type>` lines, found {}",
                        describe(self.peek())
                    ),
                ));
                self.recover_line();
                continue;
            }
            if is_field {
                self.bump();
            }
            let Some((fname, fspan)) = self.ident() else {
                let at = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0308",
                    &self.file,
                    at,
                    format!("`field` needs a name, found {}", describe(self.peek())),
                ));
                self.recover_line();
                continue;
            };
            if !is_field {
                self.diags.push(
                    Diagnostic::error(
                        "MZ0308",
                        &self.file,
                        fspan,
                        format!(
                            "a record field is written `field {fname}: <type>` — the word `field` is missing"
                        ),
                    )
                    .with_fix(
                        Span::single(fspan.start_line, fspan.start_col, 0),
                        "field ",
                        Confidence::Exact,
                    ),
                );
            }
            if !matches!(self.peek(), Tok::Colon) {
                let at = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0308",
                    &self.file,
                    at,
                    format!("`field {fname}` needs a type: write `field {fname}: <type>`"),
                ));
                broken.push(fname);
                self.recover_line();
                continue;
            }
            self.bump();
            let Some(ty) = self.parse_type(&format!("`field {fname}:`")) else {
                broken.push(fname);
                self.recover_line();
                continue;
            };
            if matches!(self.peek(), Tok::Equals) {
                let at = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0310",
                    &self.file,
                    at,
                    format!(
                        "record fields take no default — `field {fname}: {ty}` is supplied whole by the caller"
                    ),
                ));
            } else {
                self.expect_line_end(&format!("`field {fname}: {ty}`"));
            }
            fields.push(FieldDecl {
                name: fname,
                span: fspan,
                ty,
            });
            self.recover_line();
        }
        Some(RecordDecl {
            name,
            span,
            fields,
            broken,
        })
    }

    /// A `fn` body. Statements are not modelled (RFC-0001 §7.1) except `emit`, which the
    /// resolver checks against the event props (RFC-0008 §5).
    fn parse_fn_body(&mut self, stack: &mut Vec<Open>, fn_name: &str) -> Vec<Emit> {
        let depth = stack.len();
        let mut emits = Vec::new();
        while !self.at_eof() && stack.len() >= depth {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                if stack.len() < depth {
                    break;
                }
                continue;
            }
            if matches!(self.peek(), Tok::Keyword("emit")) {
                let at = self.peek_span();
                self.bump();
                match self.ident() {
                    Some((target, target_span)) => {
                        let mut arg = None;
                        if matches!(self.peek(), Tok::LParen) {
                            self.bump();
                            if let Some((value, span, _)) = self.value_spanned() {
                                arg = Some((value, span));
                            }
                            if matches!(self.peek(), Tok::RParen) {
                                self.bump();
                            }
                        }
                        emits.push(Emit {
                            in_fn: fn_name.to_string(),
                            target,
                            target_span,
                            arg,
                        });
                    }
                    None => self.diags.push(Diagnostic::error(
                        "MZ0405",
                        &self.file,
                        at,
                        format!(
                            "`emit` needs an event prop's name, found {}",
                            describe(self.peek())
                        ),
                    )),
                }
            }
            self.recover_line();
        }
        emits
    }

    /// Elements inside a `view` or a nested element, until the matching `end`.
    ///
    /// The grammar here is deliberately LL(1), and the prototype forced a correction to
    /// RFC-0001's view sketch. The RFC wrote attributes and child elements identically
    /// (`class "..."` beside `text state.label`), which is ambiguous: with one token of
    /// lookahead a reader cannot tell whether a line contributes an attribute to the current
    /// element or opens a child. That is precisely the ambiguity RFC-0001 §1.2 and RFC-0002
    /// §1 forbid, so:
    ///
    /// - an **attribute** is `name = value` — the `=` makes it unmistakable;
    /// - an **element** is a bare word opening a block that `end` closes;
    /// - `nothing` is the one leaf keyword, taking neither.
    ///
    /// Every line inside a view is now classifiable from its first two tokens.
    fn parse_elements(&mut self, stack: &mut Vec<Open>) -> Vec<Element> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_eof() || self.unwinding {
                break;
            }
            if self.at_declaration_word() {
                self.unwinding = true;
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                break;
            }
            if self.view_level_attributes() {
                continue;
            }
            match self.parse_element(stack) {
                Some(el) => out.push(el),
                None => continue,
            }
        }
        out
    }

    /// One element: a tag line, then attributes and children, then `end`.
    fn parse_element(&mut self, stack: &mut Vec<Open>) -> Option<Element> {
        let span = self.peek_span();

        // Each element is one stack frame here and in every pass after the parser, so
        // nesting is capped before it can exhaust the stack (SECURITY.md, "Crashes and
        // hangs"): 5,000 nested `row`s aborted `mz` with a stack overflow.
        if stack.len() >= MAX_NESTING && self.view_line_kind() == LineKind::Opens {
            self.skip_too_deep(stack, span, false);
            return None;
        }

        // `nothing` — the only leaf, no block.
        if matches!(self.peek(), Tok::Keyword("nothing")) {
            self.bump();
            self.recover_line();
            return Some(Element {
                tag: "nothing".to_string(),
                span,
                attrs: Vec::new(),
                children: Vec::new(),
                else_children: None,
            });
        }

        let tag = match self.peek().clone() {
            // `match` is a reserved word but not a construct (#54): one diagnostic for the
            // whole block, which is left out of the tree.
            Tok::Keyword("match") => {
                self.skip_match(span);
                return None;
            }
            Tok::Keyword(k) if matches!(k, "when" | "for") => {
                self.bump();
                k.to_string()
            }
            // `if` is not a Mzizi word, and it used to be accepted silently as an element
            // named `if` with its condition as an unchecked tail (pilot 2, divergence 5).
            // The one conditional is `when`, with the same condition syntax, so the repair
            // is exact and the block is parsed as the `when` it will become.
            Tok::Ident(name)
                if name == "if"
                    && !matches!(
                        self.tokens.get(self.pos + 1).map(|t| &t.kind),
                        Some(Tok::Newline | Tok::Eof | Tok::Equals)
                    ) =>
            {
                self.diags.push(
                    Diagnostic::error(
                        "MZ0407",
                        &self.file,
                        span,
                        "Mzizi has no `if` — the one conditional is `when`, with the same condition: write `when …` and close it with `end`",
                    )
                    .with_fix(span, "when", Confidence::Exact),
                );
                self.bump();
                "when".to_string()
            }
            Tok::Ident(name) => {
                self.bump();
                // An element word stands alone on its line. Anything after it used to be
                // kept as an unnamed, unchecked attribute, so `span class="x"` compiled.
                if !matches!(self.peek(), Tok::Newline | Tok::Eof) {
                    let at = self.peek_span();
                    self.diags.push(Diagnostic::error(
                        "MZ0409",
                        &self.file,
                        at,
                        format!(
                            "an element word stands alone on its line, found {} after `{name}` — put each attribute on its own line as `name = value`",
                            describe(self.peek())
                        ),
                    ));
                    self.recover_line();
                    stack.push(Open {
                        kind: BlockKind::Element,
                        name: name.clone(),
                        line: span.start_line,
                    });
                    return self.element_body(stack, name, span, Vec::new());
                }
                name
            }
            other => {
                self.diags.push(Diagnostic::error(
                    "MZ0402",
                    &self.file,
                    span,
                    format!(
                        "{} cannot start a view line — expected an element, `name = value`, `nothing`, or `end`",
                        describe(&other)
                    ),
                ));
                self.recover_line();
                return None;
            }
        };

        let mut attrs = Vec::new();

        // A condition tail on the element line: `when state is offline`, `when not visible`.
        while !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            match self.peek().clone() {
                Tok::Keyword(k) if !matches!(k, "true" | "false" | "nothing" | "none") => {
                    let at = self.peek_span();
                    self.bump();
                    let rhs = self.attr(k).unwrap_or(Attr {
                        name: k.to_string(),
                        value: String::new(),
                        span: at,
                        segments: Vec::new(),
                    });
                    attrs.push(rhs);
                }
                _ => match self.attr("") {
                    Some(v) => attrs.push(v),
                    None => {
                        self.bump();
                    }
                },
            }
        }
        self.recover_line();

        stack.push(Open {
            kind: BlockKind::Element,
            name: tag.clone(),
            line: span.start_line,
        });
        self.element_body(stack, tag, span, attrs)
    }

    /// `match` used to be accepted silently as an element named `match` with its subject as
    /// an unchecked tail (#54). The per-variant form is one `when v is x` block per arm, so
    /// there is no single replacement to offer, `exact` or `guess`: the diagnostic names the
    /// form and carries no fix. Its arms come in whatever shape the writer knows (`case x`
    /// blocks with or without `end`, Rust's `x => …`, an `else`), so none of them is parsed:
    /// every line indented past `match` is skipped, then its `end`, and the lexer's
    /// diagnostics on those lines are dropped (`skipped`), so the block is one error.
    fn skip_match(&mut self, span: Span) {
        self.bump();
        // The subject is quoted back when it is one name, the usual `match size`.
        let subject = match (self.peek(), self.tokens.get(self.pos + 1).map(|t| &t.kind)) {
            (Tok::Ident(name), Some(Tok::Newline | Tok::Eof | Tok::Doc(_))) => name.clone(),
            _ => "v".to_string(),
        };
        let mut end = span;
        while !matches!(self.peek(), Tok::Newline | Tok::Eof | Tok::Doc(_)) {
            end = self.peek_span();
            self.bump();
        }
        self.diags.push(Diagnostic::error(
            "MZ0410",
            &self.file,
            Span {
                end_line: end.end_line,
                end_col: end.end_col,
                ..span
            },
            format!(
                "Mzizi has no `match` — write one `when {subject} is x … end` block per variant, with `else` for the rest"
            ),
        ));
        let mut last = span.start_line;
        self.recover_line();
        while !matches!(self.peek(), Tok::Eof) && self.peek_span().start_col > span.start_col {
            last = self.peek_span().start_line;
            self.recover_line();
        }
        if matches!(self.peek(), Tok::Keyword("end"))
            && self.peek_span().start_col == span.start_col
        {
            last = self.peek_span().start_line;
            self.recover_line();
        }
        self.skipped.push((span.start_line, last));
    }

    /// What a line does to the block depth, in a view, by the rules `parse_elements`,
    /// `parse_element` and `element_body` apply to it. [`Parser::skip_too_deep`] counts
    /// with this, so the skip and the parser cannot disagree about where a block ends.
    pub(crate) fn view_line_kind(&self) -> LineKind {
        let next = self.tokens.get(self.pos + 1).map(|t| &t.kind);
        match self.peek() {
            Tok::Eof => LineKind::Stop,
            _ if self.at_declaration_word() => LineKind::Stop,
            // `end component <name>` closes every open block at once (`parse_end`), so it
            // belongs to the parent, as a declaration word does.
            Tok::Keyword("end") if matches!(next, Some(Tok::Keyword("component"))) => {
                LineKind::Stop
            }
            Tok::Keyword("end") => LineKind::Closes,
            Tok::Keyword("match") => LineKind::Match,
            Tok::Keyword("when" | "for") => LineKind::Opens,
            // `name = value`, and `name value` missing its `=` (`MZ0406`), are attributes.
            Tok::Ident(_) if matches!(next, Some(Tok::Equals)) || self.at_missing_equals() => {
                LineKind::Leaf
            }
            Tok::Ident(_) => LineKind::Opens,
            // `else`, `nothing`, and anything that cannot start a view line (`MZ0402`).
            _ => LineKind::Leaf,
        }
    }

    /// The same for a line in a service handler, by the rules of `statements`.
    pub(crate) fn handler_line_kind(&self) -> LineKind {
        let next = self.tokens.get(self.pos + 1).map(|t| &t.kind);
        match self.peek() {
            Tok::Eof => LineKind::Stop,
            // `end service <name>` closes every open block at once (`parse_end`).
            Tok::Keyword("end") if matches!(next, Some(Tok::Ident(w)) if w == "service") => {
                LineKind::Stop
            }
            Tok::Keyword("end") => LineKind::Closes,
            Tok::Keyword("when") => LineKind::Opens,
            Tok::Ident(w) if w == "if" && !matches!(next, Some(Tok::Newline | Tok::Eof)) => {
                LineKind::Opens
            }
            Tok::Ident(w)
                if matches!(w.as_str(), "route" | "fallback" | "record")
                    && matches!(next, Some(Tok::Ident(_) | Tok::Newline | Tok::Eof)) =>
            {
                LineKind::Stop
            }
            Tok::Keyword("contract" | "enum" | "use") => LineKind::Stop,
            _ => LineKind::Leaf,
        }
    }

    /// A block opened past [`MAX_NESTING`]: `MZ0411` (once per file), then the whole block is skipped
    /// by counting, with [`Parser::view_line_kind`] or [`Parser::handler_line_kind`], the
    /// lines that open a block against the `end`s that close one, without recursing, so
    /// input of any depth costs no stack. The `end` that closes the parent, and a line that
    /// would stop the parent (a declaration word, the next `route`), are left to the parent.
    pub(crate) fn skip_too_deep(&mut self, stack: &mut Vec<Open>, span: Span, handler: bool) {
        if !self.too_deep {
            self.too_deep = true;
            self.diags.push(Diagnostic::error(
                "MZ0411",
                &self.file,
                span,
                format!(
                    "blocks are nested more than {MAX_NESTING} deep here; this block, and every other block past that depth in this file, is not read"
                ),
            ));
        }
        // The skipped blocks still open, innermost last: an explicit stack in place of the
        // recursion. If the skip stops before they close (a declaration word, `end
        // component`, the next `route`), they go onto the parser's own stack, so the
        // `MZ0204` that follows counts every open block and its fix inserts every `end`.
        let mut open: Vec<Open> = Vec::new();
        let mut last = span.start_line;
        loop {
            self.skip_newlines();
            let kind = if handler {
                self.handler_line_kind()
            } else {
                self.view_line_kind()
            };
            match kind {
                LineKind::Stop => break,
                LineKind::Closes if open.is_empty() => break,
                LineKind::Closes => {
                    open.pop();
                }
                LineKind::Opens => open.push(Open {
                    kind: if handler {
                        BlockKind::When
                    } else {
                        BlockKind::Element
                    },
                    name: match self.peek() {
                        // As the parser names them: a handler's `when` (and the `if` for it)
                        // has no name; in a view, `if` with a condition is a `when`.
                        _ if handler => String::new(),
                        Tok::Ident(w)
                            if w == "if"
                                && !matches!(
                                    self.tokens.get(self.pos + 1).map(|t| &t.kind),
                                    Some(Tok::Newline | Tok::Eof | Tok::Equals)
                                ) =>
                        {
                            "when".to_string()
                        }
                        Tok::Ident(w) => w.clone(),
                        Tok::Keyword(k) => k.to_string(),
                        _ => String::new(),
                    },
                    line: self.peek_span().start_line,
                }),
                LineKind::Leaf => {}
                LineKind::Match => {
                    // `match` skips its own block with the parser's own `skip_match`, so it
                    // ends exactly where it would outside the skip, odd indentation and
                    // all; its `MZ0410` is inside what `MZ0411` already says is not read.
                    let at = self.peek_span();
                    let reported = self.diags.len();
                    self.skip_match(at);
                    self.diags.truncate(reported);
                    last = self.skipped.last().map_or(last, |&(_, to)| to);
                    if open.is_empty() {
                        break;
                    }
                    continue;
                }
            }
            last = self.peek_span().start_line;
            self.recover_line();
            if open.is_empty() {
                break;
            }
        }
        stack.append(&mut open);
        self.skipped.push((span.start_line, last));
    }

    /// An element's lines after its tag line: attributes, children and `else`, to `end`.
    fn element_body(
        &mut self,
        stack: &mut Vec<Open>,
        tag: String,
        span: Span,
        mut attrs: Vec<Attr>,
    ) -> Option<Element> {
        let mut children = Vec::new();
        let mut else_children: Option<Vec<Element>> = None;
        loop {
            self.skip_newlines();
            if self.at_eof() || self.unwinding {
                break;
            }
            if self.at_declaration_word() {
                self.unwinding = true;
                break;
            }
            if let Some(attr) = self.attribute_missing_equals() {
                attrs.push(attr);
                continue;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                break;
            }
            // `else` splits a `when` into its two branches (RFC-0001 §1.2, RFC-0008 §4).
            // One `end` closes both, so the block stack is untouched.
            if matches!(self.peek(), Tok::Keyword("else")) {
                let at = self.peek_span();
                if tag != "when" {
                    self.diags.push(Diagnostic::error(
                        "MZ0404",
                        &self.file,
                        at,
                        format!(
                            "`else` belongs to a `when`, but it is inside `{tag}` opened on line {}",
                            span.start_line
                        ),
                    ));
                } else if else_children.is_some() {
                    self.diags.push(Diagnostic::error(
                        "MZ0404",
                        &self.file,
                        at,
                        format!(
                            "the `when` opened on line {} already has an `else` — a `when` has two branches",
                            span.start_line
                        ),
                    ));
                } else {
                    else_children = Some(std::mem::take(&mut children));
                }
                self.recover_line();
                continue;
            }
            // `name = value` is an attribute; anything else opens a child element.
            let is_attr = matches!(self.peek(), Tok::Ident(_))
                && matches!(
                    self.tokens.get(self.pos + 1).map(|t| &t.kind),
                    Some(Tok::Equals)
                );
            if is_attr {
                let Tok::Ident(key) = self.peek().clone() else {
                    unreachable!()
                };
                self.bump();
                self.bump(); // `=`
                match self.attr(&key) {
                    Some(v) => attrs.push(v),
                    None => {
                        let vspan = self.peek_span();
                        self.diags.push(Diagnostic::error(
                            "MZ0403",
                            &self.file,
                            vspan,
                            format!("`{key} =` needs a value"),
                        ));
                    }
                }
                self.recover_line();
            } else if let Some(child) = self.parse_element(stack) {
                children.push(child);
            }
        }

        // `else_children` collected the *then* branch when `else` was seen; swap them back
        // so `children` is always the branch the condition selects.
        let else_children = else_children.map(|then| std::mem::replace(&mut children, then));
        Some(Element {
            tag,
            span,
            attrs,
            children,
            else_children,
        })
    }

    /// Whether the line starts with a word that only begins a component-level
    /// declaration. Inside a view it means a view block was never closed: `contract` read
    /// as an element line used to leave the view open to the end of the file, so one
    /// missing `end` became an error at `contract` plus one `MZ0204` per open block.
    fn at_declaration_word(&self) -> bool {
        match self.peek() {
            Tok::Keyword(k) => matches!(*k, "contract" | "prop" | "enum" | "fn" | "use" | "view"),
            Tok::Ident(w) => {
                w == "record"
                    && matches!(
                        self.tokens.get(self.pos + 1).map(|t| &t.kind),
                        Some(Tok::Ident(_))
                    )
                    && matches!(
                        self.tokens.get(self.pos + 2).map(|t| &t.kind),
                        Some(Tok::Newline | Tok::Eof)
                    )
            }
            _ => false,
        }
    }

    /// After a declaration word stopped a `view` early: report every block still open
    /// inside the component once, at that line, and pop them so the line parses as what
    /// it is. The fix inserts the missing `end`s just before it. It is a `guess`: the
    /// file parses after it, but where each `end` belonged is the author's call — a
    /// missing `end` in the middle of the view nests the siblings after it one level too
    /// deep, and only the author knows which element was meant to close.
    fn close_unclosed_view(&mut self, stack: &mut Vec<Open>) {
        let at = self.peek_span();
        let mut open = Vec::new();
        while stack.len() > 1 {
            open.push(stack.pop().expect("len checked"));
        }
        if open.is_empty() {
            return;
        }
        let names: Vec<String> = open
            .iter()
            .map(|o| format!("`{}` opened on line {}", block_label(o), o.line))
            .collect();
        let word = describe(self.peek());
        self.diags.push(
            Diagnostic::error(
                "MZ0204",
                &self.file,
                at,
                format!(
                    "{word} cannot appear inside a view, so {} {} never closed — add {} before this line",
                    names.join(", "),
                    if open.len() == 1 { "was" } else { "were" },
                    if open.len() == 1 {
                        "`end`".to_string()
                    } else {
                        format!("{} `end`s", open.len())
                    }
                ),
            )
            .with_fix(
                Span::single(at.start_line, 1, 0),
                "end\n".repeat(open.len()),
                Confidence::Guess,
            ),
        );
    }

    /// `class "flex"` — an attribute whose `=` is missing. Without the `=` the line reads
    /// as an element opening a block, which swallowed the rest of the view: pilot 2
    /// measured four errors, none on the line that was wrong (divergence 4). The shape is
    /// a word, one value, end of line; it is parsed as the attribute it was meant to be
    /// and reported once, with the `=` as the fix. The fix is `exact` for a word that is
    /// an attribute in every corpus file (`class`, `slot`, `text`, …, `aria_*`,
    /// `data_*`); for any other word the author may have meant an element with text,
    /// so it is a `guess`.
    fn attribute_missing_equals(&mut self) -> Option<Attr> {
        if !self.at_missing_equals() {
            return None;
        }
        let Tok::Ident(name) = self.peek().clone() else {
            return None;
        };
        let name_span = self.peek_span();
        self.bump();
        let attr = self.attr(&name)?;
        let confidence = if is_attribute_word(&name) {
            Confidence::Exact
        } else {
            Confidence::Guess
        };
        self.diags.push(
            Diagnostic::error(
                "MZ0406",
                &self.file,
                name_span,
                format!(
                    "`{name} {}` is missing its `=` — an attribute is `{name} = {}`; without the `=` the line opens an element",
                    attr.value, attr.value
                ),
            )
            .with_fix(
                Span::single(name_span.end_line, name_span.end_col, 0),
                " =",
                confidence,
            ),
        );
        self.recover_line();
        Some(attr)
    }

    /// Lookahead for [`Parser::attribute_missing_equals`]: a word other than `if`, one
    /// value (a string, an integer, `true`/`false`/`none`, or a dotted path), end of line.
    fn at_missing_equals(&self) -> bool {
        let kind = |k: usize| self.tokens.get(k).map(|t| &t.kind);
        if !matches!(self.peek(), Tok::Ident(w) if w != "if") {
            return false;
        }
        let mut k = self.pos + 1;
        match kind(k) {
            Some(Tok::Str(_) | Tok::Int(_) | Tok::Keyword("true" | "false" | "none")) => k += 1,
            Some(Tok::Ident(_)) => {
                k += 1;
                while matches!(kind(k), Some(Tok::Dot))
                    && matches!(kind(k + 1), Some(Tok::Ident(_)))
                {
                    k += 2;
                }
            }
            _ => return false,
        }
        matches!(kind(k), Some(Tok::Newline | Tok::Eof))
    }

    /// Attributes written directly under `view`, before or instead of its one element.
    /// The 7B model did this in every pilot-2 badge: each `slot = …` line was read as an
    /// element named `slot` whose block swallowed the rest of the file. A run of them is
    /// one mistake, so it is one diagnostic, and the lines are skipped.
    fn view_level_attributes(&mut self) -> bool {
        let is_attr_line = |p: &Parser| {
            (matches!(p.peek(), Tok::Ident(_))
                && matches!(p.tokens.get(p.pos + 1).map(|t| &t.kind), Some(Tok::Equals)))
                || p.at_missing_equals()
        };
        if !is_attr_line(self) {
            return false;
        }
        let first = self.peek_span();
        let Tok::Ident(first_name) = self.peek().clone() else {
            return false;
        };
        let mut last_line = first.start_line;
        let mut count = 0;
        loop {
            self.skip_newlines();
            if self.at_eof() || !is_attr_line(self) {
                break;
            }
            last_line = self.peek_span().start_line;
            count += 1;
            self.recover_line();
        }
        let lines = if count == 1 {
            format!("line {}", first.start_line)
        } else {
            format!("lines {}–{last_line}", first.start_line)
        };
        self.diags.push(Diagnostic::error(
            "MZ0408",
            &self.file,
            first,
            format!(
                "`{first_name} = …` ({lines}) is an attribute directly inside `view`, which holds one element tree — put attributes inside the element they describe, e.g. `row` … `end`"
            ),
        ));
        true
    }

    /// The `contract` block body — the clause grammar from RFC-0006 §2.
    ///
    /// Recovery is the same line-oriented rule as everywhere else: a clause the grammar
    /// does not recognize produces one diagnostic and the next line is parsed normally, so
    /// a typo in the first assertion never hides the other four (FM-5).
    fn parse_contract(&mut self, stack: &mut Vec<Open>) -> Contract {
        let depth = stack.len();
        let mut clauses = Vec::new();
        while !self.at_eof() && stack.len() >= depth {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                if stack.len() < depth {
                    break;
                }
                continue;
            }
            if let Some(clause) = self.parse_clause() {
                clauses.push(clause);
            }
        }
        Contract { clauses }
    }

    /// One assertion, `<subject> <predicate>`, spanning exactly one line.
    fn parse_clause(&mut self) -> Option<Clause> {
        let start = self.peek_span();
        let parts = self.clause_parts();

        // A clause that parsed but left tokens behind means the author wrote something the
        // compiler ignored — the silent-acceptance failure a contract can least afford.
        if parts.is_some() && !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            let span = self.peek_span();
            self.diags.push(Diagnostic::error(
                "MZ0601",
                &self.file,
                span,
                format!(
                    "{} is left over at the end of a contract clause — one assertion per line",
                    describe(self.peek())
                ),
            ));
        }

        // Span the whole line, so a diagnostic points at the assertion rather than a word.
        let mut end = self.pos;
        while end < self.tokens.len() && !matches!(self.tokens[end].kind, Tok::Newline | Tok::Eof) {
            end += 1;
        }
        let end_span = self.tokens[end.min(self.tokens.len() - 1)].span;
        self.recover_line();

        let (subject, predicate) = parts?;
        Some(Clause {
            span: Span {
                start_line: start.start_line,
                start_col: start.start_col,
                end_line: end_span.start_line,
                end_col: end_span.start_col,
            },
            subject,
            predicate,
        })
    }

    fn clause_parts(&mut self) -> Option<(Subject, Predicate)> {
        // `uses <component>` — a claim about the whole component, so it carries no subject.
        if matches!(self.peek(), Tok::Ident(w) if w == "uses")
            && matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                Some(Tok::Ident(_))
            )
        {
            self.bump();
            let name = self.expect_ident("`uses`")?;
            return Some((Subject::Whole, Predicate::Composes(name)));
        }

        // `every <enum> <column> <predicate>` — the whole column of a variant table.
        if matches!(self.peek(), Tok::Ident(w) if w == "every") {
            self.bump();
            let enum_name = self.expect_ident("`every`")?;
            let column = self.expect_ident(&format!("`every {enum_name}`"))?;
            let predicate = self.parse_predicate()?;
            return Some((Subject::EveryVariant { enum_name, column }, predicate));
        }

        // `when <variant> shows <tag> "<text>"` — what a guarded branch renders.
        if matches!(self.peek(), Tok::Keyword("when")) {
            self.bump();
            let variant = self.expect_ident("`when`")?;
            if !matches!(self.peek(), Tok::Ident(w) if w == "shows") {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0601",
                    &self.file,
                    span,
                    format!(
                        "`when {variant}` asserts what that branch renders: write `when {variant} shows <element> \"<text>\"`"
                    ),
                ));
                return None;
            }
            self.bump();
            let tag = self.expect_ident("`shows`")?;
            let text = self.expect_string(&format!("`shows {tag}`"))?;
            return Some((
                Subject::WhenVariant(variant),
                Predicate::Shows { tag, text },
            ));
        }

        // Everything else opens with a name.
        let head = self.expect_ident("a contract clause")?;
        let subject = if matches!(self.peek(), Tok::Dot) {
            self.bump();
            let member = self.expect_ident(&format!("`{head}.`"))?;
            // A third bare word is the column: `button_size.default height is 56`. A
            // predicate word there means the clause named only variant and column:
            // `offline.color is "bg-terracotta"`.
            let column = match self.peek().clone() {
                Tok::Ident(word) if !PREDICATE_WORDS.contains(&word.as_str()) => {
                    self.bump();
                    Some(word)
                }
                _ => None,
            };
            Subject::Cell {
                qualifier: head,
                member,
                column,
            }
        } else if let Tok::Str(text) = self.peek().clone() {
            self.bump();
            // `control "Button"` alone (pilot 2, frontier button seed 3): there is no
            // operand, so `is` cannot be the repair, and the generic MZ0602 listed eight
            // predicates of which exactly one applies to an element. Say which one. The
            // number is the author's, so there is no fix.
            if matches!(self.peek(), Tok::Newline | Tok::Eof) {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0602",
                    &self.file,
                    span,
                    format!(
                        "`{head} \"{text}\"` names an element and asserts nothing about it — an element takes `min_height <n>`; to assert it renders, write `when <name> shows {head} \"{text}\"`"
                    ),
                ));
                return None;
            }
            Subject::Element { tag: head, text }
        } else {
            Subject::Named(head)
        };
        let predicate = self.parse_predicate()?;
        Some((subject, predicate))
    }

    fn parse_predicate(&mut self) -> Option<Predicate> {
        match self.peek().clone() {
            Tok::Keyword("is") => {
                self.bump();
                match self.value() {
                    Some(v) => Some(Predicate::Is(v)),
                    None => {
                        let span = self.peek_span();
                        self.diags.push(Diagnostic::error(
                            "MZ0601",
                            &self.file,
                            span,
                            format!("`is` needs a value, found {}", describe(self.peek())),
                        ));
                        None
                    }
                }
            }
            Tok::Keyword("in") => {
                self.bump();
                let mut set = Vec::new();
                while let Tok::Str(text) = self.peek().clone() {
                    self.bump();
                    set.push(text);
                }
                if set.is_empty() {
                    let span = self.peek_span();
                    self.diags.push(Diagnostic::error(
                        "MZ0601",
                        &self.file,
                        span,
                        "`in` needs at least one allowed value, e.g. `in \"status\" \"alert\"`",
                    ));
                    return None;
                }
                Some(Predicate::OneOf(set))
            }
            Tok::Ident(word) => match word.as_str() {
                "contains" => {
                    self.bump();
                    self.expect_string("`contains`").map(Predicate::Contains)
                }
                "not_empty" => {
                    self.bump();
                    Some(Predicate::NotEmpty)
                }
                "at_least" => {
                    self.bump();
                    self.expect_int("`at_least`").map(Predicate::AtLeast)
                }
                "uses" => {
                    self.bump();
                    self.expect_string("`uses`").map(Predicate::UsesToken)
                }
                "min_height" => {
                    self.bump();
                    self.expect_int("`min_height`").map(Predicate::MinHeight)
                }
                _ => self.missing_predicate(),
            },
            _ => self.missing_predicate(),
        }
    }

    /// No predicate where one was required.
    ///
    /// The corpus wrote `button_size.default height 56` — a bare operand where `is` was
    /// meant. RFC-0001 §1.2 allows exactly one form per intent, so the abbreviation is an
    /// error rather than a second accepted spelling, and it carries the `exact` repair that
    /// lets `mz fix` close it with no model in the loop.
    fn missing_predicate(&mut self) -> Option<Predicate> {
        let span = self.peek_span();
        let mut d = Diagnostic::error(
            "MZ0602",
            &self.file,
            span,
            format!(
                "a contract clause needs a predicate ({}), found {}",
                PREDICATE_WORDS.join(" / "),
                describe(self.peek())
            ),
        );
        if matches!(self.peek(), Tok::Str(_) | Tok::Int(_)) {
            d = d.with_fix(
                Span::single(span.start_line, span.start_col, 0),
                "is ",
                Confidence::Exact,
            );
        }
        self.diags.push(d);
        None
    }

    fn expect_ident(&mut self, what: &str) -> Option<String> {
        match self.ident() {
            Some((name, _)) => Some(name),
            None => {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0601",
                    &self.file,
                    span,
                    format!("{what} needs a name, found {}", describe(self.peek())),
                ));
                None
            }
        }
    }

    fn expect_string(&mut self, what: &str) -> Option<String> {
        match self.peek().clone() {
            Tok::Str(text) => {
                self.bump();
                Some(text)
            }
            other => {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0601",
                    &self.file,
                    span,
                    format!("{what} needs a string, found {}", describe(&other)),
                ));
                None
            }
        }
    }

    fn expect_int(&mut self, what: &str) -> Option<i64> {
        match self.peek().clone() {
            Tok::Int(value) => {
                self.bump();
                Some(value)
            }
            other => {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0601",
                    &self.file,
                    span,
                    format!("{what} needs a number, found {}", describe(&other)),
                ));
                None
            }
        }
    }

    /// Consume a `fn` body without modelling it — statements are not part of the
    /// prototype's grammar yet (RFC-0001 §7.1).
    fn skip_block_body(&mut self, stack: &mut Vec<Open>) {
        let depth = stack.len();
        while !self.at_eof() && stack.len() >= depth {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                if stack.len() < depth {
                    break;
                }
                continue;
            }
            self.recover_line();
        }
    }
}

/// The deepest blocks may nest: in a view, counting the component and the view; in a
/// service handler, counting the service and the route. Real files nest under 12
/// (`examples/changelog_renderer.mz` is the deepest, at 9 levels of indent). Past this
/// the parser stops descending and reports `MZ0411`.
const MAX_NESTING: usize = 64;

/// What one line does to the block depth; see [`Parser::view_line_kind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
    /// Opens a block that an `end` closes.
    Opens,
    /// An `end`.
    Closes,
    /// Opens no block: an attribute, `else`, `nothing`, a statement, an error line.
    Leaf,
    /// `match`, which skips its own block (`MZ0410`).
    Match,
    /// Ends the enclosing block without an `end`: a declaration word, the next route, EOF.
    Stop,
}

const BLOCK_WORDS: &[&str] = &["component", "enum", "view", "fn", "contract"];

/// Words that are attributes in every corpus file and never elements, so a missing `=`
/// after one is certain (`MZ0406`'s `exact` fix). `text` is here though `text` could be an
/// element name in HTML: in Mzizi a view's text is always the `text = …` attribute.
fn is_attribute_word(w: &str) -> bool {
    matches!(
        w,
        "slot"
            | "class"
            | "role"
            | "text"
            | "tap"
            | "change"
            | "key"
            | "disabled"
            | "portal"
            | "value"
            | "placeholder"
            | "href"
            | "id"
            | "alt"
            | "src"
            | "title"
            | "announce"
    ) || w.starts_with("aria_")
        || w.starts_with("data_")
}

fn block_label(open: &Open) -> String {
    if open.name.is_empty() {
        open.kind.word().to_string()
    } else {
        format!("{} {}", open.kind.word(), open.name)
    }
}

/// Describe a token for a reader with no file open — the `say` field quotes source inline.
fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Keyword(k) => format!("keyword `{k}`"),
        Tok::Ident(name) => format!("`{name}`"),
        Tok::Str(s) => format!("string `\"{s}\"`"),
        Tok::Int(v) => format!("`{v}`"),
        Tok::BadInt => "an integer too large for `int`".to_string(),
        Tok::Doc(_) => "a doc comment".to_string(),
        Tok::Colon => "`:`".to_string(),
        Tok::Equals => "`=`".to_string(),
        Tok::LParen => "`(`".to_string(),
        Tok::RParen => "`)`".to_string(),
        Tok::Comma => "`,`".to_string(),
        Tok::Dot => "`.`".to_string(),
        Tok::Op(op) => format!("`{op}`"),
        Tok::Newline => "end of line".to_string(),
        Tok::Eof => "end of file".to_string(),
    }
}
