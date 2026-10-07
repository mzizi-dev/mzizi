//! Parsing a `service` (RFC-0011): the service body, `route` and `fallback` blocks, handler
//! statements, and the service's `contract` clauses.
//!
//! A child of `parse`, so it shares the parser's token cursor, its `end` cross-check and its
//! line-oriented recovery. Every rule of the component grammar holds here too: one
//! diagnostic per real error, and the next line is always a resynchronisation point.

use super::{BlockKind, Open, Parser, describe};
use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::lex::Tok;
use crate::resolve::nearest;
use crate::service::{
    Body, Check, ClauseKind, Cond, CondTest, Facet, Handler, HeaderLine, Lit, Method, Pred,
    QueryDecl, RecordLit, Respond, Route, Service, ServiceClause, ServiceContract, Stmt, Value,
};

/// How a statement list ended.
enum Stop {
    /// At `end`; the span of the `end`.
    End(Span),
    /// At `else`, not consumed.
    Else,
    /// At end of file, or unwinding from a declaration word.
    Abrupt(Span),
}

const METHOD_WORDS: &[&str] = &["get", "head", "post", "put", "patch", "delete", "options"];

/// The repair for a word written where a method belongs. `GET` reaches the parser as the
/// lexer's snake_case `g_e_t`, so an upper-case method is recognised with its underscores
/// removed, and the lower-case word is `exact` (the lexer's MZ0101 on the same token is
/// dropped, see [`super::parse_program`]). Any other near miss is a `guess`: `fetch` is two
/// edits from `patch`, and meant neither.
fn method_fix(word: &str) -> (String, Option<(String, Confidence)>) {
    let squashed = word.replace('_', "");
    if METHOD_WORDS.contains(&squashed.as_str()) && squashed != word {
        return (
            squashed.to_ascii_uppercase(),
            Some((squashed, Confidence::Exact)),
        );
    }
    let fix = nearest(word, METHOD_WORDS.iter().copied()).map(|(m, _)| (m, Confidence::Guess));
    (word.to_string(), fix)
}

impl Parser {
    /// `service <name>` … `end service <name>`. The `service` word is next.
    pub(super) fn parse_service(&mut self, docs: Vec<String>) -> Option<Service> {
        self.bump(); // `service`
        let (name, name_span) = match self.ident() {
            Some(pair) => pair,
            None => {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0202",
                    &self.file,
                    span,
                    format!(
                        "`service` needs a snake_case name, found {}",
                        describe(self.peek())
                    ),
                ));
                return None;
            }
        };
        let mut stack = vec![Open {
            kind: BlockKind::Service,
            name: name.clone(),
            line: name_span.start_line,
        }];
        let mut service = Service {
            name: name.clone(),
            name_span,
            docs,
            uses: Vec::new(),
            headers: Vec::new(),
            enums: Vec::new(),
            records: Vec::new(),
            routes: Vec::new(),
            fallback: None,
            contract: None,
        };
        self.recover_line();

        while !self.at_eof() {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            self.unwinding = false;
            match self.peek().clone() {
                Tok::Doc(text) => {
                    service.docs.push(text);
                    self.bump();
                    self.recover_line();
                }
                Tok::Keyword("use") => {
                    self.bump();
                    match self.ident() {
                        Some((cap, _)) => service.uses.push(cap),
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
                        service.enums.push(e);
                    }
                }
                Tok::Ident(w) if w == "record" => {
                    if let Some(r) = self.parse_record(&mut stack) {
                        service.records.push(r);
                    }
                }
                Tok::Ident(w) if w == "header" => {
                    if let Some(h) = self.parse_header() {
                        service.headers.push(h);
                    }
                }
                Tok::Ident(w) if w == "route" => {
                    let records: Vec<String> =
                        service.records.iter().map(|r| r.name.clone()).collect();
                    if let Some(r) = self.parse_route(&mut stack, &records) {
                        service.routes.push(r);
                    }
                }
                Tok::Ident(w) if w == "fallback" => {
                    let at = self.peek_span();
                    let records: Vec<String> =
                        service.records.iter().map(|r| r.name.clone()).collect();
                    let handler = self.parse_fallback(&mut stack, &records);
                    if service.fallback.is_some() {
                        self.diags.push(Diagnostic::error(
                            "MZ0812",
                            &self.file,
                            at,
                            format!(
                                "`service {}` already has a `fallback` — a service has one",
                                service.name
                            ),
                        ));
                    } else {
                        service.fallback = Some(handler);
                    }
                }
                Tok::Keyword("contract") => {
                    let at = self.peek_span();
                    let parsed = self.parse_service_contract(&mut stack);
                    if service.contract.is_some() {
                        self.diags.push(Diagnostic::error(
                            "MZ0209",
                            &self.file,
                            at,
                            format!(
                                "`service {}` already has a `contract` block — merge these {} clause(s) into it",
                                service.name,
                                parsed.clauses.len()
                            ),
                        ));
                    } else {
                        service.contract = Some(parsed);
                    }
                }
                Tok::Keyword("end") => self.parse_end(&mut stack),
                Tok::Keyword(k @ ("prop" | "view" | "fn")) => {
                    let span = self.peek_span();
                    let why = match k {
                        "prop" => {
                            "a service takes no props — a route's inputs are its `{param}`s and `query` lines"
                        }
                        "view" => "a service renders no view — each route ends with `respond`",
                        _ => {
                            "a service has no `fn` yet — a route's handler holds `when`, `header` and `respond` (RFC-0011 §3)"
                        }
                    };
                    self.diags
                        .push(Diagnostic::error("MZ0811", &self.file, span, why));
                    let depth = stack.len();
                    self.recover_line();
                    // Parse the block as what it is and drop it, so its nested `end`s
                    // close its own blocks and nothing inside it is reported again.
                    if k != "prop" {
                        stack.push(Open {
                            kind: if k == "view" {
                                BlockKind::View
                            } else {
                                BlockKind::Fn
                            },
                            name: String::new(),
                            line: span.start_line,
                        });
                        let before = self.diags.len();
                        if k == "view" {
                            self.parse_elements(&mut stack);
                        } else {
                            self.parse_fn_body(&mut stack, "");
                        }
                        self.diags.truncate(before);
                        self.unwinding = false;
                        stack.truncate(depth);
                    }
                }
                other => {
                    let span = self.peek_span();
                    self.diags.push(Diagnostic::error(
                        "MZ0811",
                        &self.file,
                        span,
                        format!(
                            "{} cannot start a line inside `service {}` — expected one of use, header, enum, record, route, fallback, contract, end",
                            describe(&other),
                            service.name
                        ),
                    ));
                    self.recover_line();
                }
            }
        }
        self.close_at_eof(&mut stack);
        Some(service)
    }

    /// `header "<name>" "<value>"`, at service level or in a handler.
    fn parse_header(&mut self) -> Option<HeaderLine> {
        let at = self.peek_span();
        self.bump(); // `header`
        let name = match self.peek().clone() {
            Tok::Str(s) => {
                let span = self.peek_span();
                self.bump();
                (s, span)
            }
            other => {
                self.diags.push(Diagnostic::error(
                    "MZ0807",
                    &self.file,
                    at,
                    format!(
                        "`header` takes a quoted name and a quoted value, `header \"cache-control\" \"no-store\"`, found {}",
                        describe(&other)
                    ),
                ));
                self.recover_line();
                return None;
            }
        };
        if matches!(self.peek(), Tok::Equals | Tok::Colon) {
            let span = self.peek_span();
            self.diags.push(
                Diagnostic::error(
                    "MZ0807",
                    &self.file,
                    span,
                    format!(
                        "`header \"{}\"` takes its value with no {} between them",
                        name.0,
                        describe(self.peek())
                    ),
                )
                .with_fix(span, "", Confidence::Exact),
            );
            self.bump();
        }
        let value = match self.peek().clone() {
            Tok::Str(s) => {
                let span = self.peek_span();
                self.bump();
                (s, span)
            }
            other => {
                self.diags.push(Diagnostic::error(
                    "MZ0807",
                    &self.file,
                    self.peek_span(),
                    format!(
                        "`header \"{}\"` needs a quoted value, found {}",
                        name.0,
                        describe(&other)
                    ),
                ));
                self.recover_line();
                return None;
            }
        };
        self.expect_line_end(&format!("`header \"{}\"`", name.0));
        self.recover_line();
        Some(HeaderLine {
            name: name.0,
            name_span: name.1,
            value: value.0,
            value_span: value.1,
        })
    }

    /// `route <name>`, its method line, its `query` lines, then its handler body.
    fn parse_route(&mut self, stack: &mut Vec<Open>, records: &[String]) -> Option<Route> {
        let at = self.peek_span();
        self.bump(); // `route`
        let Some((name, name_span)) = self.ident() else {
            self.diags.push(Diagnostic::error(
                "MZ0811",
                &self.file,
                self.peek_span(),
                format!(
                    "`route` needs a snake_case name, found {}",
                    describe(self.peek())
                ),
            ));
            self.recover_line();
            stack.push(Open {
                kind: BlockKind::Route,
                name: String::new(),
                line: at.start_line,
            });
            self.skip_block_body(stack);
            return None;
        };
        self.expect_line_end(&format!("`route {name}`"));
        self.recover_line();
        stack.push(Open {
            kind: BlockKind::Route,
            name: name.clone(),
            line: name_span.start_line,
        });

        let mut method = None;
        let mut pattern = None;
        if let Some((m, p)) = self.method_line(true) {
            method = m;
            pattern = Some(p);
        }
        let mut queries = Vec::new();
        loop {
            self.skip_newlines();
            if !matches!(self.peek(), Tok::Ident(w) if w == "query") {
                break;
            }
            if let Some(q) = self.parse_query() {
                queries.push(q);
            }
        }
        let (body, end_span, closed) = self.handler_body(stack, records);
        Some(Route {
            name,
            name_span,
            method,
            pattern,
            queries,
            handler: Handler {
                span: at,
                body,
                end_span,
                closed,
            },
        })
    }

    /// `<method> "<pattern>"`. With `first`, a missing method line is reported; later, a
    /// second one is.
    #[allow(clippy::type_complexity)]
    fn method_line(&mut self, first: bool) -> Option<(Option<(Method, Span)>, (String, Span))> {
        self.skip_newlines();
        let is_line = matches!(self.peek(), Tok::Ident(_))
            && matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                Some(Tok::Str(_))
            );
        if !is_line {
            if first {
                self.diags.push(Diagnostic::error(
                    "MZ0801",
                    &self.file,
                    self.peek_span(),
                    format!(
                        "a route starts with its method and pattern, e.g. `get \"/v1/items\"`, found {}",
                        describe(self.peek())
                    ),
                ));
            }
            return None;
        }
        let Tok::Ident(word) = self.peek().clone() else {
            return None;
        };
        let word_span = self.peek_span();
        self.bump();
        let Tok::Str(path) = self.peek().clone() else {
            return None;
        };
        let path_span = self.peek_span();
        self.bump();
        let method = match Method::from_word(&word) {
            Some(m) => Some((m, word_span)),
            None => {
                let (shown, fix) = method_fix(&word);
                let say = format!(
                    "`{shown}` is not a method — a route declares one of {}",
                    METHOD_WORDS.join(", ")
                );
                let d = Diagnostic::error("MZ0801", &self.file, word_span, say);
                self.diags.push(match fix {
                    Some((m, c)) => d.with_fix(word_span, m, c),
                    None => d,
                });
                None
            }
        };
        self.expect_line_end(&format!("`{word} \"{path}\"`"));
        self.recover_line();
        Some((method, (path, path_span)))
    }

    /// `query <name>: <type>`.
    fn parse_query(&mut self) -> Option<QueryDecl> {
        self.bump(); // `query`
        let Some((name, span)) = self.ident() else {
            self.diags.push(Diagnostic::error(
                "MZ0809",
                &self.file,
                self.peek_span(),
                format!(
                    "`query` is written `query <name>: option(<type>)`, found {}",
                    describe(self.peek())
                ),
            ));
            self.recover_line();
            return None;
        };
        if !matches!(self.peek(), Tok::Colon) {
            self.diags.push(
                Diagnostic::error(
                    "MZ0809",
                    &self.file,
                    self.peek_span(),
                    format!("`query {name}` needs a type: write `query {name}: option(text)`"),
                )
                .with_fix(
                    Span::single(span.end_line, span.end_col, 0),
                    ": option(text)",
                    Confidence::Guess,
                ),
            );
            self.recover_line();
            return None;
        }
        self.bump();
        let ty = self.parse_type(&format!("`query {name}:`"));
        let Some(ty) = ty else {
            self.recover_line();
            return None;
        };
        self.expect_line_end(&format!("`query {name}: {ty}`"));
        self.recover_line();
        Some(QueryDecl { name, span, ty })
    }

    /// `fallback` … `end`.
    fn parse_fallback(&mut self, stack: &mut Vec<Open>, records: &[String]) -> Handler {
        let at = self.peek_span();
        self.bump(); // `fallback`
        self.expect_line_end("`fallback`");
        self.recover_line();
        stack.push(Open {
            kind: BlockKind::Fallback,
            name: String::new(),
            line: at.start_line,
        });
        let (body, end_span, closed) = self.handler_body(stack, records);
        Handler {
            span: at,
            body,
            end_span,
            closed,
        }
    }

    /// A route's or fallback's statements, through its `end`.
    fn handler_body(
        &mut self,
        stack: &mut Vec<Open>,
        records: &[String],
    ) -> (Vec<Stmt>, Span, bool) {
        let (body, stop) = self.statements(stack, records);
        match stop {
            Stop::End(span) => (body, span, !self.unwinding),
            Stop::Abrupt(span) => (body, span, false),
            Stop::Else => unreachable!("`else` outside a `when` is handled in `statements`"),
        }
    }

    /// Statements until the block's `end`, an `else` belonging to the enclosing `when`, or
    /// an abrupt stop.
    fn statements(&mut self, stack: &mut Vec<Open>, records: &[String]) -> (Vec<Stmt>, Stop) {
        let mut out = Vec::new();
        let in_when = stack.last().is_some_and(|o| o.kind == BlockKind::When);
        loop {
            self.skip_newlines();
            if self.at_eof() {
                return (out, Stop::Abrupt(self.peek_span()));
            }
            if self.unwinding {
                return (out, Stop::Abrupt(self.peek_span()));
            }
            let span = self.peek_span();
            // Past the nesting cap a `when` (or `if`) is skipped whole: see `skip_too_deep`.
            if stack.len() >= super::MAX_NESTING
                && self.handler_line_kind() == super::LineKind::Opens
            {
                self.skip_too_deep(stack, span, true);
                continue;
            }
            match self.peek().clone() {
                Tok::Keyword("end") => {
                    self.parse_end(stack);
                    return (out, Stop::End(span));
                }
                Tok::Keyword("else") if in_when => return (out, Stop::Else),
                Tok::Keyword("else") => {
                    self.diags.push(Diagnostic::error(
                        "MZ0404",
                        &self.file,
                        span,
                        "`else` belongs to a `when`, and there is none open here",
                    ));
                    self.recover_line();
                }
                Tok::Keyword("when") => {
                    self.bump();
                    out.push(self.parse_when(stack, records, span));
                }
                Tok::Ident(w)
                    if w == "if"
                        && !matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(Tok::Newline | Tok::Eof)
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
                    out.push(self.parse_when(stack, records, span));
                }
                Tok::Ident(w) if w == "header" => {
                    if let Some(h) = self.parse_header() {
                        out.push(Stmt::Header(h));
                    }
                }
                Tok::Ident(w) if w == "respond" => {
                    out.push(Stmt::Respond(self.parse_respond(records)));
                }
                Tok::Ident(w) if w == "query" => {
                    self.diags.push(Diagnostic::error(
                        "MZ0811",
                        &self.file,
                        span,
                        "`query` lines come right after the route's method line, before any statement",
                    ));
                    self.recover_line();
                }
                Tok::Ident(w) if w == "return" => {
                    self.diags.push(Diagnostic::error(
                        "MZ0811",
                        &self.file,
                        span,
                        "Mzizi has no `return` — a handler path ends with `respond <status>`",
                    ));
                    self.recover_line();
                }
                Tok::Ident(w)
                    if (w == "route" || w == "fallback" || w == "record")
                        && matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(Tok::Ident(_) | Tok::Newline | Tok::Eof)
                        ) =>
                {
                    self.unclosed_before(stack, &w);
                    return (out, Stop::Abrupt(span));
                }
                Tok::Keyword(k) if matches!(k, "contract" | "enum" | "use") => {
                    self.unclosed_before(stack, k);
                    return (out, Stop::Abrupt(span));
                }
                Tok::Ident(w)
                    if Method::from_word(&w).is_some()
                        && matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(Tok::Str(_))
                        ) =>
                {
                    self.diags.push(Diagnostic::error(
                        "MZ0801",
                        &self.file,
                        span,
                        format!(
                            "a route has one method line — `{w} …` here is a second method; declare it as its own route"
                        ),
                    ));
                    self.recover_line();
                }
                other => {
                    self.diags.push(Diagnostic::error(
                        "MZ0811",
                        &self.file,
                        span,
                        format!(
                            "{} cannot start a line in a handler — expected when, header, respond or end",
                            describe(&other)
                        ),
                    ));
                    self.recover_line();
                }
            }
        }
    }

    /// A declaration word inside a route: every block open inside the service is unclosed.
    /// One diagnostic, and the blocks are popped so the line parses as what it is.
    fn unclosed_before(&mut self, stack: &mut Vec<Open>, word: &str) {
        let at = self.peek_span();
        let mut open = Vec::new();
        while stack.len() > 1 {
            open.push(stack.pop().expect("len checked"));
        }
        let names: Vec<String> = open
            .iter()
            .map(|o| format!("`{}` opened on line {}", super::block_label(o), o.line))
            .collect();
        self.diags.push(
            Diagnostic::error(
                "MZ0204",
                &self.file,
                at,
                format!(
                    "`{word}` cannot appear inside a handler, so {} {} never closed — add {} before this line",
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
        self.unwinding = true;
    }

    /// The rest of a `when` line, its branches, and its `end`. The `when` word is consumed.
    fn parse_when(&mut self, stack: &mut Vec<Open>, records: &[String], span: Span) -> Stmt {
        let cond = self.parse_cond();
        self.recover_line();
        stack.push(Open {
            kind: BlockKind::When,
            name: String::new(),
            line: span.start_line,
        });
        let (then, stop) = self.statements(stack, records);
        let (els, end_span) = match stop {
            Stop::End(s) | Stop::Abrupt(s) => (None, s),
            Stop::Else => {
                self.bump(); // `else`
                self.expect_line_end("`else`");
                self.recover_line();
                let (els, stop) = self.statements(stack, records);
                let end_span = match stop {
                    Stop::End(s) | Stop::Abrupt(s) => s,
                    Stop::Else => {
                        // A second `else`: report it, and read the rest as part of this one.
                        let at = self.peek_span();
                        self.diags.push(Diagnostic::error(
                            "MZ0404",
                            &self.file,
                            at,
                            format!(
                                "the `when` opened on line {} already has an `else` — a `when` has two branches",
                                span.start_line
                            ),
                        ));
                        self.recover_line();
                        let (_, stop) = self.statements(stack, records);
                        match stop {
                            Stop::End(s) | Stop::Abrupt(s) => s,
                            Stop::Else => self.peek_span(),
                        }
                    }
                };
                (Some(els), end_span)
            }
        };
        Stmt::When {
            cond,
            then,
            els,
            span,
            end_span,
        }
    }

    /// `[not] <name> [is <lit> | is none | in <lit>… | at_least <n> | at_most <n>]`.
    fn parse_cond(&mut self) -> Option<Cond> {
        let negated = self.eat_keyword("not");
        let Some((subject, subject_span)) = self.ident() else {
            self.diags.push(Diagnostic::error(
                "MZ0811",
                &self.file,
                self.peek_span(),
                format!(
                    "`when` needs a parameter to test, found {}",
                    describe(self.peek())
                ),
            ));
            return None;
        };
        let test = match self.peek().clone() {
            Tok::Newline | Tok::Eof => CondTest::Truth,
            Tok::Keyword("is") => {
                self.bump();
                let at = self.peek_span();
                match self.peek().clone() {
                    Tok::Keyword("none") => {
                        self.bump();
                        CondTest::IsNone(at)
                    }
                    Tok::Keyword("not") => {
                        self.diags.push(Diagnostic::error(
                            "MZ0712",
                            &self.file,
                            at,
                            format!(
                                "there is no `is not` — write `when {subject} is none` … `else`, or `when not …`"
                            ),
                        ));
                        return None;
                    }
                    _ => match self.lit() {
                        Some((lit, span)) => CondTest::Is(lit, span),
                        None => {
                            self.diags.push(Diagnostic::error(
                                "MZ0811",
                                &self.file,
                                at,
                                format!(
                                    "`when {subject} is` needs a value or `none`, found {}",
                                    describe(self.peek())
                                ),
                            ));
                            return None;
                        }
                    },
                }
            }
            Tok::Keyword("in") => {
                self.bump();
                let mut set = Vec::new();
                while let Some(pair) = self.lit() {
                    set.push(pair);
                }
                if set.is_empty() {
                    self.diags.push(Diagnostic::error(
                        "MZ0811",
                        &self.file,
                        self.peek_span(),
                        format!("`when {subject} in` needs at least one value"),
                    ));
                    return None;
                }
                CondTest::In(set)
            }
            Tok::Ident(w) if w == "at_least" || w == "at_most" => {
                self.bump();
                let at = self.peek_span();
                let Tok::Int(n) = self.peek().clone() else {
                    self.diags.push(Diagnostic::error(
                        "MZ0811",
                        &self.file,
                        at,
                        format!("`{w}` needs a number, found {}", describe(self.peek())),
                    ));
                    return None;
                };
                self.bump();
                if w == "at_least" {
                    CondTest::AtLeast(n, at)
                } else {
                    CondTest::AtMost(n, at)
                }
            }
            Tok::Equals => {
                // `when name = "x"` and `when name == "x"`: the comparison word is `is`.
                let first = self.peek_span();
                let mut last = first;
                while matches!(self.peek(), Tok::Equals) {
                    last = self.peek_span();
                    self.bump();
                }
                let span = Span {
                    start_line: first.start_line,
                    start_col: first.start_col,
                    end_line: last.end_line,
                    end_col: last.end_col,
                };
                self.diags.push(
                    Diagnostic::error(
                        "MZ0811",
                        &self.file,
                        span,
                        format!("Mzizi compares with `is`, not `=` — write `when {subject} is …`"),
                    )
                    .with_fix(span, "is", Confidence::Exact),
                );
                return None;
            }
            other => {
                self.diags.push(Diagnostic::error(
                    "MZ0811",
                    &self.file,
                    self.peek_span(),
                    format!(
                        "a condition is `{subject}`, `{subject} is <value>`, `{subject} is none`, `{subject} in <values>` or `{subject} at_least <n>` — found {}",
                        describe(&other)
                    ),
                ));
                return None;
            }
        };
        if !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            self.diags.push(Diagnostic::error(
                "MZ0811",
                &self.file,
                self.peek_span(),
                format!(
                    "{} is left over after the condition — one test per `when`",
                    describe(self.peek())
                ),
            ));
        }
        Some(Cond {
            subject,
            subject_span,
            negated,
            test,
        })
    }

    /// A literal: a string, an int, `true`, `false` or `none`.
    fn lit(&mut self) -> Option<(Lit, Span)> {
        let span = self.peek_span();
        let lit = match self.peek().clone() {
            Tok::Str(s) => Lit::Text(s),
            Tok::Int(v) => Lit::Int(v),
            Tok::Keyword("true") => Lit::Bool(true),
            Tok::Keyword("false") => Lit::Bool(false),
            Tok::Keyword("none") => Lit::None,
            _ => return None,
        };
        self.bump();
        Some((lit, span))
    }

    /// `respond <status> [json <record> <field> <value>… | text "…" | file "…"]`.
    fn parse_respond(&mut self, records: &[String]) -> Respond {
        let at = self.peek_span();
        self.bump(); // `respond`
        let status = match self.peek().clone() {
            Tok::Int(v) => {
                let span = self.peek_span();
                self.bump();
                Some((v, span))
            }
            other => {
                self.diags.push(Diagnostic::error(
                    "MZ0806",
                    &self.file,
                    self.peek_span(),
                    format!(
                        "`respond` needs a status first, e.g. `respond 200`, found {}",
                        describe(&other)
                    ),
                ));
                None
            }
        };
        let body = if status.is_none() {
            Body::Empty
        } else {
            self.parse_body(records)
        };
        if status.is_some() {
            self.expect_line_end("the `respond` line");
        }
        let end = self.tokens[self.pos.saturating_sub(1)].span;
        self.recover_line();
        Respond {
            span: Span {
                start_line: at.start_line,
                start_col: at.start_col,
                end_line: end.end_line,
                end_col: end.end_col,
            },
            status,
            body,
        }
    }

    fn parse_body(&mut self, records: &[String]) -> Body {
        let at = self.peek_span();
        let word = match self.peek().clone() {
            Tok::Newline | Tok::Eof => return Body::Empty,
            Tok::Ident(w) => w,
            other => {
                self.diags.push(Diagnostic::error(
                    "MZ0806",
                    &self.file,
                    at,
                    format!(
                        "a body is `json <record> …`, `text \"…\"` or `file \"…\"`, found {}",
                        describe(&other)
                    ),
                ));
                return Body::Empty;
            }
        };
        match word.as_str() {
            "text" | "file" => {
                self.bump();
                let Tok::Str(s) = self.peek().clone() else {
                    self.diags.push(Diagnostic::error(
                        "MZ0806",
                        &self.file,
                        self.peek_span(),
                        format!(
                            "`{word}` needs a quoted string, found {}",
                            describe(self.peek())
                        ),
                    ));
                    return Body::Empty;
                };
                let span = self.peek_span();
                self.bump();
                if word == "text" {
                    Body::Text(s, span)
                } else {
                    Body::File(s, span)
                }
            }
            "json" => {
                self.bump();
                let Some((record, record_span)) = self.ident() else {
                    self.diags.push(Diagnostic::error(
                        "MZ0806",
                        &self.file,
                        self.peek_span(),
                        format!(
                            "`json` needs a record name, then its fields, found {}",
                            describe(self.peek())
                        ),
                    ));
                    return Body::Empty;
                };
                self.record_lit(at, record, record_span)
            }
            w if records.iter().any(|r| r == w) => {
                // `respond 200 item name "x"`: the record without its body word.
                self.diags.push(
                    Diagnostic::error(
                        "MZ0806",
                        &self.file,
                        at,
                        format!("`{w}` is a record — a JSON body is written `json {w} …`"),
                    )
                    .with_fix(
                        Span::single(at.start_line, at.start_col, 0),
                        "json ",
                        Confidence::Exact,
                    ),
                );
                self.bump();
                self.record_lit(at, w.to_string(), at)
            }
            other => {
                let fix = nearest(other, ["json", "text", "file"]);
                let d = Diagnostic::error(
                    "MZ0806",
                    &self.file,
                    at,
                    format!(
                        "`{other}` is not a body form — a body is `json <record> …`, `text \"…\"` or `file \"…\"`"
                    ),
                );
                self.diags.push(match fix {
                    Some((f, c)) => d.with_fix(at, f, c),
                    None => d,
                });
                self.recover_to_line_end();
                Body::Empty
            }
        }
    }

    /// `<field> <value>` pairs to the end of the line.
    fn record_lit(&mut self, at: Span, record: String, record_span: Span) -> Body {
        let mut fields = Vec::new();
        let mut last = record_span;
        while let Some((field, fspan)) = self.ident() {
            last = fspan;
            let value = if let Some((lit, span)) = self.lit() {
                Value::Lit(lit, span)
            } else if let Some((name, span)) = self.ident() {
                Value::Name(name, span)
            } else {
                self.diags.push(Diagnostic::error(
                    "MZ0808",
                    &self.file,
                    self.peek_span(),
                    format!(
                        "field `{field}` needs a value — a literal or a parameter — found {}",
                        describe(self.peek())
                    ),
                ));
                break;
            };
            last = value.span();
            fields.push((field, fspan, value));
        }
        Body::Json(RecordLit {
            record,
            record_span,
            fields,
            span: Span {
                start_line: at.start_line,
                start_col: at.start_col,
                end_line: last.end_line,
                end_col: last.end_col,
            },
        })
    }

    /// Skip to the end of the line without consuming the newline.
    fn recover_to_line_end(&mut self) {
        while !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            self.bump();
        }
    }

    /// A service's `contract` block (RFC-0011 §7).
    fn parse_service_contract(&mut self, stack: &mut Vec<Open>) -> ServiceContract {
        let at = self.peek_span();
        self.bump(); // `contract`
        stack.push(Open {
            kind: BlockKind::Contract,
            name: String::new(),
            line: at.start_line,
        });
        self.recover_line();
        let depth = stack.len();
        let mut clauses = Vec::new();
        while !self.at_eof() && stack.len() >= depth {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                continue;
            }
            let start = self.peek_span();
            let kind = self.service_clause();
            if kind.is_some() && !matches!(self.peek(), Tok::Newline | Tok::Eof) {
                self.diags.push(Diagnostic::error(
                    "MZ0601",
                    &self.file,
                    self.peek_span(),
                    format!(
                        "{} is left over at the end of a contract clause — one assertion per line",
                        describe(self.peek())
                    ),
                ));
            }
            let mut end = self.pos;
            while end < self.tokens.len()
                && !matches!(self.tokens[end].kind, Tok::Newline | Tok::Eof)
            {
                end += 1;
            }
            let end_span = self.tokens[end.min(self.tokens.len() - 1)].span;
            self.recover_line();
            if let Some(kind) = kind {
                clauses.push(ServiceClause {
                    span: Span {
                        start_line: start.start_line,
                        start_col: start.start_col,
                        end_line: end_span.start_line,
                        end_col: end_span.start_col,
                    },
                    kind,
                });
            }
        }
        ServiceContract { clauses }
    }

    fn service_clause(&mut self) -> Option<ClauseKind> {
        let at = self.peek_span();
        match self.peek().clone() {
            Tok::Ident(w) if w == "example" => {
                self.bump();
                let mspan = self.peek_span();
                let Some((word, _)) = self.ident() else {
                    self.clause_error(format!(
                        "`example` needs a method, then a quoted path, found {}",
                        describe(self.peek())
                    ));
                    return None;
                };
                let Some(method) = Method::from_word(&word) else {
                    let (shown, fix) = method_fix(&word);
                    let d = Diagnostic::error(
                        "MZ0601",
                        &self.file,
                        mspan,
                        format!(
                            "`{shown}` is not a method — one of {}",
                            METHOD_WORDS.join(", ")
                        ),
                    );
                    self.diags.push(match fix {
                        Some((m, c)) => d.with_fix(mspan, m, c),
                        None => d,
                    });
                    return None;
                };
                let Tok::Str(target) = self.peek().clone() else {
                    self.clause_error(format!(
                        "`example {word}` needs a quoted path, found {}",
                        describe(self.peek())
                    ));
                    return None;
                };
                self.bump();
                let check = self.check()?;
                Some(ClauseKind::Example {
                    method,
                    target,
                    check,
                })
            }
            Tok::Ident(w) if w == "ensure" => {
                self.bump();
                if self.eat_keyword("when") {
                    let guard = self.check()?;
                    if !matches!(self.peek(), Tok::Ident(w) if w == "then") {
                        self.clause_error(format!(
                            "`ensure when …` names its consequence after `then`, found {}",
                            describe(self.peek())
                        ));
                        return None;
                    }
                    self.bump();
                    let then = self.check()?;
                    Some(ClauseKind::Ensure {
                        when: Some(guard),
                        then,
                    })
                } else {
                    let then = self.check()?;
                    Some(ClauseKind::Ensure { when: None, then })
                }
            }
            other => {
                self.diags.push(Diagnostic::error(
                    "MZ0601",
                    &self.file,
                    at,
                    format!(
                        "a service's contract holds `example <method> \"<path>\" …` and `ensure …` lines, found {}",
                        describe(&other)
                    ),
                ));
                None
            }
        }
    }

    fn clause_error(&mut self, say: String) {
        let span = self.peek_span();
        self.diags
            .push(Diagnostic::error("MZ0601", &self.file, span, say));
    }

    /// `<facet> <predicate>`.
    fn check(&mut self) -> Option<Check> {
        let facet = match self.peek().clone() {
            Tok::Ident(w) if w == "status" => {
                self.bump();
                Facet::Status
            }
            Tok::Ident(w) if w == "header" => {
                self.bump();
                let Tok::Str(name) = self.peek().clone() else {
                    self.clause_error(format!(
                        "`header` needs a quoted name, found {}",
                        describe(self.peek())
                    ));
                    return None;
                };
                self.bump();
                Facet::Header(name)
            }
            Tok::Ident(w) if w == "body" => {
                self.bump();
                let mut path = Vec::new();
                while matches!(self.peek(), Tok::Dot) {
                    self.bump();
                    match self.ident() {
                        Some((seg, _)) => path.push(seg),
                        None => {
                            self.clause_error(format!(
                                "`body.` needs a field name, found {}",
                                describe(self.peek())
                            ));
                            return None;
                        }
                    }
                }
                if path.is_empty() {
                    Facet::Body
                } else {
                    Facet::BodyPath(path)
                }
            }
            other => {
                self.clause_error(format!(
                    "a check reads `status`, `header \"<name>\"`, `body` or `body.<field>`, found {}",
                    describe(&other)
                ));
                return None;
            }
        };
        let pred = match self.peek().clone() {
            Tok::Keyword("is") => {
                self.bump();
                match self.lit() {
                    Some((lit, _)) => Pred::Is(lit),
                    None => {
                        self.clause_error(format!(
                            "`is` needs a value, found {}",
                            describe(self.peek())
                        ));
                        return None;
                    }
                }
            }
            Tok::Keyword("in") => {
                self.bump();
                let mut set = Vec::new();
                while let Some((lit, _)) = self.lit() {
                    set.push(lit);
                }
                if set.is_empty() {
                    self.clause_error("`in` needs at least one value".to_string());
                    return None;
                }
                Pred::In(set)
            }
            Tok::Ident(w) => match w.as_str() {
                "not_empty" => {
                    self.bump();
                    Pred::NotEmpty
                }
                "contains" => {
                    self.bump();
                    let Tok::Str(s) = self.peek().clone() else {
                        self.clause_error(format!(
                            "`contains` needs a string, found {}",
                            describe(self.peek())
                        ));
                        return None;
                    };
                    self.bump();
                    Pred::Contains(s)
                }
                "at_least" | "at_most" => {
                    self.bump();
                    let Tok::Int(n) = self.peek().clone() else {
                        self.clause_error(format!(
                            "`{w}` needs a number, found {}",
                            describe(self.peek())
                        ));
                        return None;
                    };
                    self.bump();
                    if w == "at_least" {
                        Pred::AtLeast(n)
                    } else {
                        Pred::AtMost(n)
                    }
                }
                _ => return self.missing_service_predicate(),
            },
            _ => return self.missing_service_predicate(),
        };
        Some(Check { facet, pred })
    }

    /// `status 200`: the operand without its `is`, which is an `exact` repair.
    fn missing_service_predicate(&mut self) -> Option<Check> {
        let span = self.peek_span();
        let mut d = Diagnostic::error(
            "MZ0602",
            &self.file,
            span,
            format!(
                "a check needs a predicate (is / in / not_empty / contains / at_least / at_most), found {}",
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
}
