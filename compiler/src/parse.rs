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

use crate::ast::{Component, Element, EnumDecl, PropDecl, Variant};
use crate::contract::{Clause, Contract, PREDICATE_WORDS, Predicate, Subject};
use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::lex::{Tok, Token, lex};

/// Parse a source file. Returns the component when the shape was recoverable, plus every
/// diagnostic found — parsing never stops at the first error.
pub fn parse(src: &str, file: &str) -> (Option<Component>, Vec<Diagnostic>) {
    let (tokens, mut diags) = lex(src, file);
    let mut p = Parser {
        tokens,
        pos: 0,
        file: file.to_string(),
        diags: Vec::new(),
    };
    let component = p.parse_file();
    diags.append(&mut p.diags);
    (component, diags)
}

/// What kind of block is open, for the `end` cross-check.
#[derive(Clone, Copy, PartialEq, Debug)]
enum BlockKind {
    Component,
    Enum,
    View,
    Fn,
    Contract,
    Element,
}

impl BlockKind {
    fn word(self) -> &'static str {
        match self {
            BlockKind::Component => "component",
            BlockKind::Enum => "enum",
            BlockKind::View => "view",
            BlockKind::Fn => "fn",
            BlockKind::Contract => "contract",
            BlockKind::Element => "element",
        }
    }

    /// Whether `end` for this block must echo `<kind> <name>` (top-level declarations do).
    fn requires_echo(self) -> bool {
        matches!(self, BlockKind::Component)
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
        // Dotted continuation: `state.color`
        while matches!(self.peek(), Tok::Dot) {
            self.bump();
            match self.ident() {
                Some((seg, _)) => {
                    out.push('.');
                    out.push_str(&seg);
                }
                None => break,
            }
        }
        Some(out)
    }

    fn parse_file(&mut self) -> Option<Component> {
        self.skip_newlines();

        let mut docs = Vec::new();
        while let Tok::Doc(text) = self.peek().clone() {
            docs.push(text);
            self.bump();
            self.skip_newlines();
        }

        if !self.eat_keyword("component") {
            let span = self.peek_span();
            self.diags.push(Diagnostic::error(
                "MZ0201",
                &self.file,
                span,
                format!(
                    "a .mz file starts with `component <name>`, found {}",
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
            props: Vec::new(),
            view: None,
            fns: Vec::new(),
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
                Tok::Keyword("prop") => {
                    if let Some(pr) = self.parse_prop() {
                        component.props.push(pr);
                    }
                }
                Tok::Keyword("view") => {
                    self.bump();
                    stack.push(Open {
                        kind: BlockKind::View,
                        name: String::new(),
                        line: self.peek_span().start_line,
                    });
                    self.recover_line();
                    let children = self.parse_elements(&mut stack);
                    component.view = Some(children);
                }
                Tok::Keyword("fn") => {
                    self.bump();
                    if let Some((fname, _)) = self.ident() {
                        component.fns.push(fname.clone());
                        stack.push(Open {
                            kind: BlockKind::Fn,
                            name: fname,
                            line: self.peek_span().start_line,
                        });
                    }
                    self.recover_line();
                    self.skip_block_body(&mut stack);
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
                            "{} cannot start a line inside `component {}` — expected one of use, enum, prop, view, fn, contract, end",
                            describe(&other), component.name
                        ),
                    ));
                    self.recover_line();
                }
            }
        }

        // Anything still open at EOF is unclosed. One diagnostic each, innermost first,
        // every one carrying the exact text that would close it.
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
                    format!("{closer}\n"),
                    Confidence::Exact,
                ),
            );
        }

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

    /// `end`, with the cross-check that makes a mismatch one diagnostic instead of a cascade.
    fn parse_end(&mut self, stack: &mut Vec<Open>) {
        let end_span = self.peek_span();
        self.bump();

        let echoed_kind = match self.peek().clone() {
            Tok::Keyword(k) if BLOCK_WORDS.contains(&k) => {
                self.bump();
                Some(k)
            }
            _ => None,
        };
        let echoed_name = self.ident().map(|(n, _)| n);

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
                    .with_fix(end_span, "", Confidence::Exact),
                );
                self.recover_line();
                return;
            }
        };

        if let Some(kind_word) = echoed_kind
            && kind_word != open.kind.word()
        {
            self.diags.push(
                Diagnostic::error(
                    "MZ0206",
                    &self.file,
                    end_span,
                    format!(
                        "`end {}` closes the `{}` opened on line {} — write `end {}`",
                        kind_word,
                        block_label(&open),
                        open.line,
                        open.kind.word()
                    ),
                )
                .with_fix(
                    end_span,
                    format!("end {}", open.kind.word()),
                    Confidence::Exact,
                ),
            );
        }

        if let Some(name) = echoed_name {
            if !open.name.is_empty() && name != open.name {
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
                    format!(
                        "top-level blocks close with their name: write `end {} {}`",
                        open.kind.word(),
                        open.name
                    ),
                )
                .with_fix(
                    end_span,
                    format!("end {} {}", open.kind.word(), open.name),
                    Confidence::Exact,
                ),
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
            while let Some((col, _)) = self.ident() {
                match self.value() {
                    Some(v) => columns.push((col, v)),
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

    fn parse_prop(&mut self) -> Option<PropDecl> {
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
                return None;
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
            return None;
        }
        self.bump();

        let ty = match self.peek().clone() {
            Tok::Keyword(k) => {
                self.bump();
                if k == "event" && matches!(self.peek(), Tok::LParen) {
                    self.bump();
                    let inner = self.value().unwrap_or_default();
                    if matches!(self.peek(), Tok::RParen) {
                        self.bump();
                    }
                    format!("event({inner})")
                } else {
                    k.to_string()
                }
            }
            Tok::Ident(name) => {
                self.bump();
                name
            }
            _ => {
                let span = self.peek_span();
                self.diags.push(Diagnostic::error(
                    "MZ0306",
                    &self.file,
                    span,
                    format!(
                        "expected a type after `prop {name}:`, found {}",
                        describe(self.peek())
                    ),
                ));
                self.recover_line();
                return None;
            }
        };

        let mut has_default = false;
        let mut default = None;
        if matches!(self.peek(), Tok::Equals) {
            self.bump();
            default = self.value();
            has_default = default.is_some();
        }

        self.recover_line();
        Some(PropDecl {
            name,
            ty,
            has_default,
            default,
        })
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
            if self.at_eof() {
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                break;
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

        // `nothing` — the only leaf, no block.
        if matches!(self.peek(), Tok::Keyword("nothing")) {
            self.bump();
            self.recover_line();
            return Some(Element {
                tag: "nothing".to_string(),
                span,
                attrs: Vec::new(),
                children: Vec::new(),
            });
        }

        let tag = match self.peek().clone() {
            Tok::Keyword(k) if matches!(k, "when" | "match" | "for") => {
                self.bump();
                k.to_string()
            }
            Tok::Ident(name) => {
                self.bump();
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
                Tok::Keyword(k) => {
                    self.bump();
                    let rhs = self.value().unwrap_or_default();
                    attrs.push((k.to_string(), rhs));
                }
                _ => match self.value() {
                    Some(v) => attrs.push((String::new(), v)),
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

        let mut children = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            if matches!(self.peek(), Tok::Keyword("end")) {
                self.parse_end(stack);
                break;
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
                match self.value() {
                    Some(v) => attrs.push((key, v)),
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

        Some(Element {
            tag,
            span,
            attrs,
            children,
        })
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

const BLOCK_WORDS: &[&str] = &["component", "enum", "view", "fn", "contract"];

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
        Tok::Doc(_) => "a doc comment".to_string(),
        Tok::Colon => "`:`".to_string(),
        Tok::Equals => "`=`".to_string(),
        Tok::LParen => "`(`".to_string(),
        Tok::RParen => "`)`".to_string(),
        Tok::Comma => "`,`".to_string(),
        Tok::Dot => "`.`".to_string(),
        Tok::Newline => "end of line".to_string(),
        Tok::Eof => "end of file".to_string(),
    }
}
