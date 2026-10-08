//! Records in a program (RFC-0013 §11): the `record` block with its `field`s, its `fn`
//! methods and its `contract` of `always` clauses; the record literal `point(x = 1.0, …)`;
//! `with`, which copies a record; and the assignment to a field, `p.x = 3.0`.
//!
//! The checker reads these trees in `program/records.rs`. Forms the design names that are
//! not built here (a method that changes `self`) are one `MZ0919` each, and their block is
//! skipped, as the other unbuilt forms are.

use super::*;
use crate::expr::FieldInit;
use crate::program::{Invariant, RecordDecl, RecordField};

impl P {
    /// `record name` … `end`: the cursor is on `record`. Returns the declaration, or `None`
    /// when the name is missing (the block is skipped).
    pub(super) fn record_decl(&mut self) -> Option<RecordDecl> {
        let record_at = self.bump().span;
        let (name, name_span) = match self.peek().clone() {
            Tok::Ident(n) => {
                let s = self.span();
                self.bump();
                (n, s)
            }
            other => {
                self.err(
                    "MZ0301",
                    record_at,
                    format!(
                        "`record` needs a snake_case name, e.g. `record point`, found {}",
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
                    "{} is left over after `record {name}` — one declaration per line",
                    describe(self.peek())
                ),
            );
        }
        self.recover_line();
        let mut rec = RecordDecl {
            name,
            name_span,
            fields: Vec::new(),
            methods: Vec::new(),
            always: Vec::new(),
        };
        loop {
            self.failed = false;
            self.skip_newlines();
            let at = self.span();
            match self.peek().clone() {
                Tok::Eof => {
                    self.unclosed("record", record_at, at);
                    break;
                }
                Tok::Doc(_) => self.recover_line(),
                Tok::Keyword("end") if !matches!(word(self.peek_at(1)), Some("program" | "fn")) => {
                    let end_at = self.bump().span;
                    if !self.at_line_end() {
                        let mut last = end_at;
                        while !self.at_line_end() {
                            last = self.bump().span;
                        }
                        self.err_fix(
                            "MZ0206",
                            end_at,
                            "a `record` closes with a bare `end`",
                            join(end_at, last),
                            "end",
                            Confidence::Exact,
                        );
                    }
                    self.recover_line();
                    break;
                }
                // `end fn` or `end program` closes nothing here: the record is never closed.
                Tok::Keyword("end") => {
                    self.unclosed("record", record_at, at);
                    break;
                }
                // The program's entry point is never a method: a record still open before it
                // is unclosed.
                Tok::Keyword("fn") if word(self.peek_at(1)) == Some("main") => {
                    self.unclosed("record", record_at, at);
                    break;
                }
                Tok::Keyword("fn") => {
                    if let Some(m) = self.method() {
                        rec.methods.push(m);
                    }
                }
                Tok::Keyword("contract") => self.record_contract(&mut rec),
                Tok::Ident(w) if w == "field" => {
                    if let Some(f) = self.record_field() {
                        rec.fields.push(f);
                    }
                }
                other => {
                    self.err(
                        "MZ0917",
                        at,
                        format!(
                            "a record holds `field`s, `fn`s and a `contract`, not {}",
                            describe(&other)
                        ),
                    );
                    self.recover_line();
                }
            }
        }
        Some(rec)
    }

    /// `field name: type`: the cursor is on `field`.
    fn record_field(&mut self) -> Option<RecordField> {
        let field_at = self.bump().span;
        let (name, span) = match self.peek().clone() {
            Tok::Ident(n) => {
                let s = self.span();
                self.bump();
                (n, s)
            }
            other => {
                self.err(
                    "MZ0917",
                    field_at,
                    format!(
                        "`field` needs a name, e.g. `field x: float`, found {}",
                        describe(&other)
                    ),
                );
                self.recover_line();
                return None;
            }
        };
        if !matches!(self.peek(), Tok::Colon) {
            self.err(
                "MZ0917",
                span,
                format!(
                    "field `{name}` has no type — every field is written `field {name}: <type>`"
                ),
            );
            self.recover_line();
            return None;
        }
        self.bump();
        let ty = self.type_ref("a field's `:`");
        self.finish_line(&format!("field `{name}`"));
        Some(RecordField { name, span, ty })
    }

    /// `contract` … `always <bool>` … `end`: the cursor is on `contract`. Each `always` line
    /// is one invariant (RFC-0013 §11.4).
    fn record_contract(&mut self, rec: &mut RecordDecl) {
        let contract_at = self.bump().span;
        if !self.at_line_end() {
            let at = self.span();
            self.err(
                "MZ0917",
                at,
                format!(
                    "{} is left over after `contract` — its clauses are on their own lines",
                    describe(self.peek())
                ),
            );
        }
        self.recover_line();
        loop {
            self.failed = false;
            self.skip_newlines();
            let at = self.span();
            match self.peek().clone() {
                Tok::Eof => {
                    self.unclosed("contract", contract_at, at);
                    return;
                }
                Tok::Doc(_) => self.recover_line(),
                Tok::Keyword("end") if !matches!(word(self.peek_at(1)), Some("program" | "fn")) => {
                    self.recover_line();
                    return;
                }
                // An `end fn`, an `end program` or a `fn` is the next declaration's: this
                // contract is open, and the line is left for the record or program to read.
                Tok::Keyword("end" | "fn") => {
                    self.unclosed("contract", contract_at, at);
                    return;
                }
                Tok::Ident(w) if w == "always" => {
                    self.bump();
                    let expr = self.expr();
                    let span = join(at, expr.span);
                    self.finish_line("an `always` clause");
                    rec.always.push(Invariant { expr, span });
                }
                other => {
                    self.err(
                        "MZ0917",
                        at,
                        format!(
                            "a `contract` in a record holds `always` clauses, not {}",
                            describe(&other)
                        ),
                    );
                    self.recover_line();
                }
            }
        }
    }

    /// Whether the cursor is on `with (`: the postfix form that copies a record.
    pub(super) fn with_follows(&self) -> bool {
        matches!(self.peek(), Tok::Ident(w) if w == "with")
            && matches!(self.peek_at(1), Tok::LParen)
    }

    /// `( name = value, … )`: the cursor is on `(`. Used by a record literal and by `with`.
    /// A value with no name is positional (its name is empty, and the checker reports it).
    /// Returns the fields and the span of the parentheses.
    pub(super) fn field_inits(&mut self) -> (Vec<FieldInit>, Span) {
        let open = self.bump().span;
        let mut fields = Vec::new();
        if matches!(self.peek(), Tok::RParen) {
            let close = self.bump().span;
            return (fields, join(open, close));
        }
        if self.nest >= PROGRAM_NESTING {
            let at = self.span();
            self.report_too_deep(at, "this record literal");
            self.failed = true;
            self.skip_to_paren_end();
            return (fields, join(open, at));
        }
        self.nest += 1;
        let close = loop {
            let at = self.span();
            let (name, value) = match (self.peek().clone(), self.peek_at(1).clone()) {
                (Tok::Ident(n), Tok::Equals) => {
                    self.bump();
                    self.bump();
                    (n, self.expr())
                }
                _ => (String::new(), self.expr()),
            };
            fields.push(FieldInit {
                name,
                span: at,
                value,
            });
            match self.peek() {
                Tok::Comma => {
                    self.bump();
                }
                Tok::RParen => break self.bump().span,
                other => {
                    let at = self.span();
                    let other = describe(other);
                    if !self.failed {
                        self.err(
                            "MZ0917",
                            at,
                            format!("expected `,` or `)` in a record's fields, found {other}"),
                        );
                    }
                    self.skip_to_paren_end();
                    break at;
                }
            }
        };
        self.nest -= 1;
        (fields, join(open, close))
    }

    /// `point(x = 1.0, y = 2.0)`: the cursor is on `(` after a record's name.
    pub(super) fn record_literal(&mut self, name: String, name_span: Span) -> Expr {
        let (fields, parens) = self.field_inits();
        Expr {
            kind: ExprKind::Record {
                name,
                name_span,
                fields,
            },
            span: join(name_span, parens),
        }
    }

    /// `p.x = value`: the cursor is on the `=` after `p.x`, which the caller has checked for.
    /// A field is assigned on a `var` (RFC-0013 §11.1), and the checker reports where it is
    /// not.
    pub(super) fn field_assignment(
        &mut self,
        name: String,
        name_span: Span,
        field: String,
        field_span: Span,
    ) -> StmtKind {
        self.bump(); // `=`
        let value = self.value();
        self.finish_value_line("a field assignment");
        StmtKind::FieldAssign {
            name,
            name_span,
            field,
            field_span,
            value,
        }
    }
}

/// The names of the file's records, read ahead like the `fn` and `enum` names: a type may
/// name a record declared later.
pub(super) fn declared_records(tokens: &[Token]) -> Vec<String> {
    let mut names: Vec<String> = (0..tokens.len().saturating_sub(1))
        .filter(|&i| i == 0 || matches!(tokens[i - 1].kind, Tok::Newline))
        .filter_map(|i| match (&tokens[i].kind, &tokens[i + 1].kind) {
            (Tok::Ident(k), Tok::Ident(n)) if k == "record" => Some(n.clone()),
            _ => None,
        })
        .collect();
    names.sort();
    names.dedup();
    names
}
