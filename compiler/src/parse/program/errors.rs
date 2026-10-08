//! Parsing RFC-0013 §12 in a program: an enum variant's columns, `result(T, E)` types,
//! prefix `try`, an enum value's column (`problem.say`), and `MZ0952`: the error idioms
//! other languages bring, each repaired in the tree as it is reported, so the checker sees
//! the program the fix would produce.
//!
//! The `enum` declaration and the `match` are C4's (`parse/program/control.rs`): a `match`
//! on a result is a `match` whose `case ok <name>` and `case error <name>` lines carry the
//! name each binds ([`crate::expr::Arm::binding`]).

use super::{P, PROGRAM_NESTING, join, word};
use crate::diagnostic::{Confidence, Span};
use crate::expr::{Expr, ExprKind, TextPart, Ty, UnOp, canonical};
use crate::lex::{Tok, Token};
use crate::program::{Column, StmtKind, TypeRef};

/// What the parser knows about the whole file before it reads it: the enums with their
/// variants, and the functions' names. A type names an enum declared later in the file, and
/// an idiom's fix is `exact` only when the parser can tell what the words mean.
#[derive(Clone, Debug, Default)]
pub(super) struct Known {
    /// Each `enum`'s name and its variants' names.
    pub(super) enums: Vec<(String, Vec<String>)>,
    /// Each `fn`'s name.
    pub(super) fns: Vec<String>,
}

impl Known {
    /// Read the names off the token stream: `enum <name>` and the first word of each line up
    /// to its `end`, and `fn <name>`, each at the start of a line.
    pub(super) fn scan(toks: &[Token]) -> Known {
        let mut known = Known::default();
        let at_line_start = |i: usize| i == 0 || matches!(toks[i - 1].kind, Tok::Newline);
        let mut i = 0;
        while i + 1 < toks.len() {
            if !at_line_start(i) {
                i += 1;
                continue;
            }
            match (&toks[i].kind, &toks[i + 1].kind) {
                (Tok::Keyword("fn"), Tok::Ident(n)) => known.fns.push(n.clone()),
                (Tok::Keyword("enum"), Tok::Ident(n)) => {
                    let mut variants = Vec::new();
                    let mut k = i + 2;
                    while k < toks.len() {
                        if at_line_start(k) {
                            match &toks[k].kind {
                                Tok::Keyword("end" | "fn") | Tok::Eof => break,
                                Tok::Ident(v) => variants.push(v.clone()),
                                _ => {}
                            }
                        }
                        k += 1;
                    }
                    known.enums.push((n.clone(), variants));
                    i = k;
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
        known
    }

    /// Whether any enum of the program declares variant `v`.
    fn is_variant(&self, v: &str) -> bool {
        self.enums.iter().any(|(_, vs)| vs.iter().any(|x| x == v))
    }

    /// The one enum that declares variant `v`, if exactly one does.
    fn owner(&self, v: &str) -> Option<&str> {
        let mut owners = self
            .enums
            .iter()
            .filter(|(_, vs)| vs.iter().any(|x| x == v));
        let first = owners.next()?;
        owners.next().is_none().then_some(first.0.as_str())
    }
}

impl P {
    /// `say "no digits" code 3`: a variant's columns, each a name and a literal.
    /// Also whether the line was read to its end, `false` after `MZ0302`.
    pub(super) fn columns(&mut self, variant: &str) -> (Vec<Column>, bool) {
        let mut columns = Vec::new();
        while let Tok::Ident(col) = self.peek().clone() {
            let span = self.span();
            self.bump();
            let at = self.span();
            let value = match self.peek().clone() {
                Tok::Int(v) => Some(ExprKind::Int(v)),
                Tok::Keyword(b @ ("true" | "false")) => Some(ExprKind::Bool(b == "true")),
                Tok::Str(raw) if !raw.contains('{') => {
                    // `text` decodes the escapes; with no `{` there is no interpolation.
                    let parts = self.text(&raw, at);
                    Some(ExprKind::Text(parts))
                }
                _ => None,
            };
            let Some(kind) = value else {
                self.err(
                    "MZ0302",
                    at,
                    format!(
                        "column `{col}` of `{variant}` needs a literal value — a text with no `{{…}}`, an int or a bool, e.g. `{col} \"…\"`"
                    ),
                );
                return (columns, false);
            };
            self.bump();
            columns.push(Column {
                name: col,
                span,
                value: Expr { kind, span: at },
            });
        }
        (columns, true)
    }

    /// The type after `result`: `(<success>, <error>)`, the cursor on `(`. The success type
    /// may be `none`. A result inside a result is `MZ0950` (§2), with no fix.
    pub(super) fn result_type(&mut self, at: Span) -> TypeRef {
        self.bump(); // `(`
        let ok = if self.is_word("none") {
            let s = self.span();
            self.bump();
            TypeRef {
                ty: Ty::Nothing,
                span: s,
            }
        } else {
            self.type_ref("`result(`")
        };
        let err = if matches!(self.peek(), Tok::Comma) {
            self.bump();
            Some(self.type_ref("`result(…,`"))
        } else {
            None
        };
        let close = if matches!(self.peek(), Tok::RParen) {
            Some(self.bump().span)
        } else {
            None
        };
        let span = join(at, self.prev());
        let (Some(err), Some(_)) = (err, close) else {
            if !self.failed {
                self.err(
                    "MZ0306",
                    span,
                    "a result names its success and its error type: `result(<type>, <error type>)`, or `result(none, <error type>)`",
                );
            }
            while !matches!(
                self.peek(),
                Tok::RParen | Tok::Newline | Tok::Eof | Tok::Equals
            ) {
                self.bump();
            }
            if matches!(self.peek(), Tok::RParen) {
                self.bump();
            }
            return TypeRef {
                ty: Ty::Error,
                span,
            };
        };
        if ok.ty.as_result().is_some() || err.ty.as_result().is_some() {
            self.err(
                "MZ0950",
                span,
                "a result cannot hold a result — unwrap the inner one with `try` or `match` where it is made (RFC-0013 §2)",
            );
            return TypeRef {
                ty: Ty::Error,
                span,
            };
        }
        if ok.ty == Ty::Error || err.ty == Ty::Error {
            return TypeRef {
                ty: Ty::Error,
                span,
            };
        }
        TypeRef {
            ty: Ty::result(ok.ty, err.ty),
            span,
        }
    }

    /// `case ok v` / `case error v` (RFC-0013 §12.2): the name the case binds, taken off
    /// `values`. A second word is a binding only when no enum of the program has it as a
    /// variant, since no binding may take a variant's name (`MZ0921`): with
    /// `enum status { ok, error }`, `case ok error` still lists two variants.
    pub(super) fn result_case_binding(&self, values: &mut Vec<Expr>) -> Option<(String, Span)> {
        let [first, second] = values.as_slice() else {
            return None;
        };
        let (ExprKind::Name(case), ExprKind::Name(name)) = (&first.kind, &second.kind) else {
            return None;
        };
        if !matches!(case.as_str(), "ok" | "error") || self.known.is_variant(name) {
            return None;
        }
        let binding = (name.clone(), second.span);
        values.truncate(1);
        Some(binding)
    }

    /// `int?` in a program: the lexer reads `?` as an operator there, so the parser keeps
    /// the component's `MZ0104` and its text for it.
    pub(super) fn question_after_type(&mut self) {
        if matches!(self.peek(), Tok::Op("?")) {
            let at = self.bump().span;
            self.err(
                "MZ0104",
                at,
                "`?` is not a character Mzizi uses — an optional type is written `option(<type>)`",
            );
        }
    }

    /// `try e`, the cursor on `try` with a value after it (§12.2). Level 3, as prefix `-`.
    pub(super) fn try_prefix(&mut self) -> Expr {
        let at = self.bump().span;
        if self.nest >= PROGRAM_NESTING {
            return self.too_deep_expr(at);
        }
        self.nest += 1;
        let operand = self.unary_expr();
        self.nest -= 1;
        Expr {
            span: join(at, operand.span),
            kind: ExprKind::Unary {
                op: UnOp::Try,
                operand: Box::new(operand),
            },
        }
    }

    /// Whether `try` at the cursor opens a block, as in Python, TypeScript or Java: it ends
    /// its line, or a `:` does (a `{` is not a token, so it ends the line too).
    pub(super) fn at_try_block(&self) -> bool {
        self.is_word("try")
            && (matches!(self.peek_at(1), Tok::Newline | Tok::Eof)
                || (matches!(self.peek_at(1), Tok::Colon)
                    && matches!(self.peek_at(2), Tok::Newline | Tok::Eof)))
    }

    /// `MZ0952`: a `try` block. Mzizi has no exceptions, so there is no fix; the block, and
    /// the `catch`, `except` and `finally` lines after it, are skipped as one error.
    pub(super) fn try_block(&mut self, at: Span) {
        self.err(
            "MZ0952",
            join(at, self.line_end_span()),
            "Mzizi has no exceptions, so `try` opens no block — handle a result with `match r` and `case ok v` / `case error e`, or propagate it with `try f(x)` (RFC-0013 §12)",
        );
        let col = at.start_col;
        let first = at.start_line;
        let mut last = first;
        self.recover_line();
        while !matches!(self.peek(), Tok::Eof) {
            let s = self.span();
            let handler = matches!(word(self.peek()), Some("except" | "catch" | "finally"));
            if s.start_col > col || handler {
                last = s.start_line;
                self.recover_line();
            } else {
                // Lines between hold nothing but braces, which are not tokens.
                last = last.max(s.start_line.saturating_sub(1));
                break;
            }
        }
        self.skipped.push((first, last));
    }

    /// Whether `throw` or `raise` at the cursor is the statement, not a name.
    pub(super) fn at_throw(&self) -> bool {
        matches!(word(self.peek()), Some("throw" | "raise"))
            && matches!(
                self.peek_at(1),
                Tok::Ident(_) | Tok::Str(_) | Tok::Int(_) | Tok::Keyword("true" | "false")
            )
    }

    /// `MZ0952`: `throw e` or `raise e` is `return error(e)`, `exact` when the parser can see
    /// that `e` has the function's error type (a variant of its error enum, or a literal of
    /// its error type), a `guess` otherwise. A guess leaves `return` of an error value in the
    /// tree, which the checker passes over in silence.
    pub(super) fn throw_stmt(&mut self, at: Span) -> StmtKind {
        let written = word(self.peek()).unwrap_or("throw").to_string();
        self.bump();
        let value = self.expr();
        if !self.at_line_end() {
            // `throw new Error("bad")`: the thrown value is not one Mzizi expression, so the
            // line is one `MZ0952`, with no fix to guess from, and nothing more.
            let span = join(at, self.line_end_span());
            self.skipped.push((at.start_line, at.start_line));
            if !self.failed {
                self.err(
                    "MZ0952",
                    span,
                    format!(
                        "Mzizi has no exceptions — a function fails by returning its error, `return error(e)`, with `e` of its error type, in place of `{written}`"
                    ),
                );
            }
            self.failed = true;
            self.recover_line();
            return StmtKind::Return(Some(self.error_expr(span)));
        }
        self.recover_line();
        let thrown = value.span;
        // `throw Error("bad")`: the lexer reads `Error` as `error` (with an `MZ0101` this line
        // skips), so the value is already `error("bad")`; the fix does not wrap it twice.
        let value = match value.kind {
            ExprKind::Call { name, mut args, .. } if name == "error" && args.len() == 1 => {
                args.remove(0)
            }
            kind => Expr {
                kind,
                span: value.span,
            },
        };
        if value.has_error() {
            // The value was reported, and `return error(…)` would write its placeholder.
            self.skipped.push((at.start_line, at.start_line));
            self.failed = true;
            return StmtKind::Return(Some(self.error_expr(join(at, value.span))));
        }
        let exact = self.has_error_type(&value);
        let fixed = format!("return error({})", canonical(&value));
        let span = join(at, thrown);
        // The line is repaired whole, so a lexer note on it (`MZ0101` on `Negative`) would
        // be a second fix over the same text.
        self.skipped.push((at.start_line, at.start_line));
        self.diags.push(
            crate::diagnostic::Diagnostic::error(
                "MZ0952",
                &self.file,
                span,
                format!(
                    "Mzizi has no exceptions — a function fails by returning its error: `{fixed}`"
                ),
            )
            .with_fix(
                span,
                fixed,
                if exact {
                    Confidence::Exact
                } else {
                    Confidence::Guess
                },
            ),
        );
        self.failed = true;
        let returned = if exact {
            Expr {
                span,
                kind: ExprKind::Call {
                    name: "error".to_string(),
                    name_span: at,
                    args: vec![value],
                },
            }
        } else {
            self.error_expr(span)
        };
        StmtKind::Return(Some(returned))
    }

    /// Whether `e` visibly has the current function's error type.
    fn has_error_type(&self, e: &Expr) -> bool {
        let Some((_, err)) = self.ret.and_then(Ty::as_result) else {
            return false;
        };
        match (&e.kind, err) {
            (ExprKind::Name(v), Ty::Enum(en)) => self.known.owner(v) == Some(en),
            (
                ExprKind::Variant {
                    enum_name, name, ..
                },
                Ty::Enum(en),
            ) => {
                enum_name == en
                    && self
                        .known
                        .enums
                        .iter()
                        .any(|(n, vs)| n == en && vs.iter().any(|v| v == name))
            }
            (ExprKind::Text(parts), Ty::Text) => {
                parts.iter().all(|p| matches!(p, TextPart::Lit(_)))
            }
            (ExprKind::Int(_), Ty::Int) | (ExprKind::Bool(_), Ty::Bool) => true,
            _ => false,
        }
    }

    /// `ok(v)` or `Ok(v)`, and `Err(e)` or `err(e)` when no function is named `err`: Rust's
    /// result constructors, `MZ0952` with the `exact` fixes `v` and `error(e)`. The cursor is
    /// on `(`, past the name at `at`. `None` when the name is not one of these.
    pub(super) fn constructor_idiom(&mut self, name: &str, at: Span) -> Option<Expr> {
        let success = name == "ok";
        if !success && (name != "err" || self.known.fns.iter().any(|f| f == "err")) {
            return None;
        }
        let (mut args, close) = self.args();
        let span = join(at, close);
        // `Ok` and `Err` reached here as the lexer's `ok` and `err`, with an `MZ0101` each.
        self.suppress.push(at);
        if args.len() != 1 {
            if !self.failed {
                self.err(
                    "MZ0952",
                    span,
                    if success {
                        "a function returns success with `return v`, or a bare `return` when it returns `result(none, E)` — there is no `ok(…)`"
                    } else {
                        "a function fails with `return error(e)`, one value"
                    },
                );
            }
            return Some(self.error_expr(span));
        }
        let value = args.remove(0);
        if value.has_error() {
            // The argument was reported, and its canonical text is a placeholder (`…`):
            // there is no fix to build, and the line has its diagnostic.
            return Some(self.error_expr(span));
        }
        let (fixed, tree) = if success {
            (canonical(&value), value)
        } else {
            let fixed = format!("error({})", canonical(&value));
            let tree = Expr {
                span,
                kind: ExprKind::Call {
                    name: "error".to_string(),
                    name_span: at,
                    args: vec![value],
                },
            };
            (fixed, tree)
        };
        let say = if success {
            format!(
                "Mzizi wraps success itself — `return v` returns `v` as success: write `{fixed}`"
            )
        } else {
            format!("a failure is written `error(e)`: `{fixed}`")
        };
        self.diags.push(
            crate::diagnostic::Diagnostic::error("MZ0952", &self.file, span, say).with_fix(
                span,
                fixed,
                Confidence::Exact,
            ),
        );
        Some(tree)
    }

    /// What follows a value: `.name` (a variant through its enum, or a column), Rust's
    /// postfix `?` (`MZ0952`, `exact` to a prefix `try`), `.unwrap()` / `.expect(…)`
    /// (`MZ0952`, no fix), and a method call, `.name(…)` (RFC-0013 §3.7), left to right.
    pub(super) fn postfix(&mut self, mut e: Expr) -> Expr {
        // Each `.name` and `?` is a level of the tree the passes after the parser walk, so a
        // chain is capped as a chain of binary operators is (`MZ0411`).
        let mut links = 0usize;
        // A chain's `?`s are one diagnostic, whose fix rewrites the whole chain, so no two
        // `exact` fixes overlap: `x??` is `try try x`.
        let mut question: Option<usize> = None;
        loop {
            if matches!(self.peek(), Tok::Op("?") | Tok::Dot) {
                links += 1;
                if links > super::MAX_NESTING {
                    // The line is not read, so nothing on it is reported but `MZ0411`.
                    if let Some(i) = question {
                        self.diags.remove(i);
                    }
                    let at = self.span();
                    self.too_deep_expr(at);
                    return self.error_expr(e.span);
                }
            }
            match (self.peek().clone(), self.peek_at(1).clone()) {
                (Tok::Op("?"), _) => {
                    let q = self.bump().span;
                    let span = join(e.span, q);
                    let mut fixed = format!("try {}", canonical(&e));
                    // A dot binds tighter than `try` (§3.5), so `f(x)?.y` is `(try f(x)).y`:
                    // without the parentheses the fix would read `.y` off the result.
                    if matches!(self.peek(), Tok::Dot) {
                        fixed = format!("({fixed})");
                    }
                    if let Some(i) = question.take() {
                        self.diags.remove(i);
                    } else if self.failed {
                        // The line already has its diagnostic.
                        e = Expr {
                            span,
                            kind: ExprKind::Unary {
                                op: UnOp::Try,
                                operand: Box::new(e),
                            },
                        };
                        continue;
                    }
                    self.diags.push(
                        crate::diagnostic::Diagnostic::error(
                            "MZ0952",
                            &self.file,
                            q,
                            format!("Rust's postfix `?` is Mzizi's prefix `try`: write `{fixed}`"),
                        )
                        .with_fix(span, fixed, Confidence::Exact),
                    );
                    question = Some(self.diags.len() - 1);
                    e = Expr {
                        span,
                        kind: ExprKind::Unary {
                            op: UnOp::Try,
                            operand: Box::new(e),
                        },
                    };
                }
                (Tok::Dot, Tok::Ident(name)) if !matches!(self.peek_at(2), Tok::LParen) => {
                    self.bump();
                    let name_span = self.bump().span;
                    e = Expr {
                        span: join(e.span, name_span),
                        kind: ExprKind::Field {
                            base: Box::new(e),
                            name,
                            name_span,
                        },
                    };
                }
                (Tok::Dot, Tok::Ident(name)) if matches!(name.as_str(), "unwrap" | "expect") => {
                    let dot = self.bump().span;
                    self.bump();
                    let (_, close) = self.args();
                    if !self.failed {
                        self.err(
                            "MZ0952",
                            join(dot, close),
                            format!(
                                "`.{name}(…)` turns an error into a crash, and Mzizi has no way to — handle the result with `match`, or propagate it with `try`"
                            ),
                        );
                    }
                    return self.error_expr(join(e.span, close));
                }
                (Tok::Dot, next) => {
                    // `x.abs()`: a method (RFC-0013 §3.7). Which receivers have which
                    // methods is the checker's question.
                    let dot = self.bump().span;
                    let name_at = self.span();
                    let Some(name) = word(&next).map(str::to_string) else {
                        if !self.failed {
                            let found = super::describe(&next);
                            self.err(
                                "MZ0917",
                                name_at,
                                format!("expected a method's name after `.`, found {found}"),
                            );
                        }
                        return self.error_expr(join(e.span, dot));
                    };
                    self.bump();
                    let (args, called, end) = if matches!(self.peek(), Tok::LParen) {
                        if self.nest >= PROGRAM_NESTING {
                            return self.too_deep_expr(name_at);
                        }
                        self.nest += 1;
                        let (args, close) = self.args();
                        self.nest -= 1;
                        (args, true, close)
                    } else {
                        (Vec::new(), false, name_at)
                    };
                    e = Expr {
                        span: join(e.span, end),
                        kind: ExprKind::Method {
                            recv: Box::new(e),
                            name,
                            name_span: name_at,
                            args,
                            called,
                        },
                    };
                }
                _ => return e,
            }
        }
    }
}
