//! Parsing RFC-0013 §9 in a program: the collection types `list(T)`, `map(K, V)` and
//! `set(K)` (§2), the bracket spellings of a list type (`[T]`, `T[]`, `MZ0105`, whose repair
//! moves here from the lexer, §9.1), bracket literals (`[1, 2]`, `["a": 1]`, `[]`), indexing
//! (`xs[i]`, §3.7), `xs[i] = v`, and three idioms a model brings: a tuple (`MZ0963`), a
//! lambda (`MZ0909`) and Python's `x not in xs` (`MZ0910`).
//!
//! `option(T)` is read so that a type holding one recovers, and is `MZ0919`: options are
//! what indexing returns, and writing one as a type is RFC-0013 §18.2's "C4 options"
//! follow-up.

use super::{P, PROGRAM_NESTING, join};
use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::expr::{BinOp, Expr, ExprKind, Ty, UnOp, canonical};
use crate::lex::Tok;
use crate::program::{StmtKind, TypeRef};

impl P {
    /// The one `MZ0105` for a type spelt with brackets: the ones its parts reported since
    /// `first` are folded into one for the whole type `t`, whose `exact` fix writes it in
    /// Mzizi's spelling, so no two `exact` fixes overlap.
    pub(super) fn symbolic_type(&mut self, t: TypeRef, first: usize) -> TypeRef {
        let within = |d: &Diagnostic| {
            d.code == "MZ0105"
                && (d.span.start_line, d.span.start_col) >= (t.span.start_line, t.span.start_col)
                && (d.span.end_line, d.span.end_col) <= (t.span.end_line, t.span.end_col)
        };
        let count = self.diags[first..].iter().filter(|d| within(d)).count();
        if count <= 1 || t.ty.has_error() {
            return t;
        }
        let mut k = first;
        while k < self.diags.len() {
            if within(&self.diags[k]) {
                self.diags.remove(k);
            } else {
                k += 1;
            }
        }
        self.bracket_list_type(t.span, t.ty);
        t
    }

    /// `MZ0105` for a list type written with brackets at `span`, whose `exact` fix writes `ty`.
    fn bracket_list_type(&mut self, span: Span, ty: Ty) {
        let fixed = ty.name();
        let d = Diagnostic::error(
            "MZ0105",
            &self.file,
            span,
            format!("this is not how Mzizi spells a list type — write `{fixed}`"),
        );
        self.diags.push(if ty.has_error() {
            d
        } else {
            d.with_fix(span, fixed, Confidence::Exact)
        });
    }

    /// `[T]`, the cursor on `[`: `list(T)`, with `MZ0105`.
    pub(super) fn bracket_type(&mut self, at: Span, after: &str) -> TypeRef {
        self.bump();
        let inner = self.type_ref(after);
        if !matches!(self.peek(), Tok::RBracket) {
            if !self.failed {
                let s = self.span();
                let found = super::describe(self.peek());
                self.err(
                    "MZ0306",
                    s,
                    format!(
                        "expected `]` to close the type opened on column {}, found {found}",
                        at.start_col
                    ),
                );
            }
            return TypeRef {
                ty: Ty::Error,
                span: join(at, self.prev()),
            };
        }
        let close = self.bump().span;
        let span = join(at, close);
        let ty = if inner.ty == Ty::Error {
            Ty::Error
        } else {
            Ty::list(inner.ty)
        };
        self.bracket_list_type(span, ty);
        TypeRef { ty, span }
    }

    /// `T[]` after a type: `list(T)`, with `MZ0105`, as many times as written.
    pub(super) fn list_suffix(&mut self, mut t: TypeRef) -> TypeRef {
        while matches!(self.peek(), Tok::LBracket) && matches!(self.peek_at(1), Tok::RBracket) {
            self.bump();
            let close = self.bump().span;
            let span = join(t.span, close);
            let ty = if t.ty == Ty::Error {
                Ty::Error
            } else {
                Ty::list(t.ty)
            };
            self.bracket_list_type(span, ty);
            t = TypeRef { ty, span };
        }
        t
    }

    /// `list(T)`, `option(T)`, `map(K, V)` or `set(K)`, the cursor on `(` after `name`. A key
    /// type with no order is `MZ0964` (§2).
    pub(super) fn constructor_type(&mut self, name: &str, at: Span) -> TypeRef {
        self.bump(); // `(`
        let first = self.type_ref(&format!("`{name}(`"));
        let second = if name == "map" && matches!(self.peek(), Tok::Comma) {
            self.bump();
            Some(self.type_ref("`map(…,`"))
        } else {
            None
        };
        let closed = matches!(self.peek(), Tok::RParen);
        if closed {
            self.bump();
        }
        let span = join(at, self.prev());
        let arity_ok = (name == "map") == second.is_some();
        if !closed || !arity_ok {
            if !self.failed {
                self.err(
                    "MZ0306",
                    span,
                    match name {
                        "map" => "a map names its key and value types: `map(<key>, <value>)`",
                        "set" => "a set names its element type: `set(<key>)`",
                        "option" => "an option names its type: `option(<type>)`",
                        _ => "a list names its element type: `list(<type>)`",
                    },
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
        }
        if name == "option" {
            self.err(
                "MZ0919",
                span,
                "`option(T)` written as a type is designed (RFC-0013 §8) but not built yet — an option is what indexing, `first` and `slice` return, and is read with `otherwise`; the rest of §8 is the \"C4 options\" follow-up",
            );
            return TypeRef {
                ty: Ty::Error,
                span,
            };
        }
        if first.ty.as_result().is_some() || second.is_some_and(|s| s.ty.as_result().is_some()) {
            self.err(
                "MZ0950",
                span,
                "a result is never an element, a key or a value of a collection — match it or `try` it where it is made (RFC-0013 §2)",
            );
            return TypeRef {
                ty: Ty::Error,
                span,
            };
        }
        if matches!(name, "map" | "set") && !first.ty.is_key() {
            self.err(
                "MZ0964",
                first.span,
                format!(
                    "a {name}'s {} is kept in key order, and {} has no order to keep — a key is an int, a text, a bool or an enum (RFC-0013 §2)",
                    if name == "map" { "key" } else { "element" },
                    first.ty.name()
                ),
            );
            return TypeRef {
                ty: Ty::Error,
                span,
            };
        }
        let ty = if first.ty.has_error() || second.is_some_and(|s| s.ty.has_error()) {
            Ty::Error
        } else {
            match (name, second) {
                ("map", Some(v)) => Ty::map(first.ty, v.ty),
                ("set", _) => Ty::set(first.ty),
                _ => Ty::list(first.ty),
            }
        };
        TypeRef { ty, span }
    }

    /// A bracket literal, the cursor on `[` (RFC-0013 §9.1): `[]`, `[a, b]` or `[k: v, …]`.
    /// Entries mixed with plain elements are `MZ0961`, once per literal.
    pub(super) fn bracket_literal(&mut self, at: Span) -> Expr {
        self.bump();
        if matches!(self.peek(), Tok::RBracket) {
            let close = self.bump().span;
            return Expr {
                kind: ExprKind::List(Vec::new()),
                span: join(at, close),
            };
        }
        if self.nest >= PROGRAM_NESTING {
            return self.too_deep_expr(at);
        }
        self.nest += 1;
        let mut items = Vec::new();
        let mut entries = Vec::new();
        let close = loop {
            let first = self.expr();
            if matches!(self.peek(), Tok::Colon) {
                self.bump();
                let value = self.expr();
                entries.push((first, value));
            } else {
                items.push(first);
            }
            match self.peek() {
                Tok::Comma => {
                    self.bump();
                }
                Tok::RBracket => break Some(self.bump().span),
                other => {
                    if !self.failed {
                        let s = self.span();
                        let found = super::describe(other);
                        self.err(
                            "MZ0917",
                            s,
                            format!(
                                "expected `,` or `]` in the list opened on column {}, found {found}",
                                at.start_col
                            ),
                        );
                    }
                    while !self.at_line_end() {
                        self.bump();
                    }
                    break None;
                }
            }
        };
        let mixed = !items.is_empty() && !entries.is_empty();
        self.nest -= 1;
        let span = join(at, close.unwrap_or_else(|| self.prev()));
        if close.is_none() {
            return self.error_expr(span);
        }
        if mixed {
            self.err(
                "MZ0961",
                span,
                "this bracket literal mixes `key: value` entries with plain elements — a map's literal has only entries, `[\"a\": 1]`, and a list's or a set's only elements, `[1, 2]`",
            );
            return self.error_expr(span);
        }
        Expr {
            kind: if entries.is_empty() {
                ExprKind::List(items)
            } else {
                ExprKind::MapLit(entries)
            },
            span,
        }
    }

    /// `base[index]`, the cursor on `[` (RFC-0013 §3.7). A literal negative index, Python's
    /// count from the end, is `MZ0915` with the `guess` `xs[xs.length() - 1]`.
    pub(super) fn index_postfix(&mut self, base: Expr) -> Expr {
        let open = self.bump().span;
        if self.nest >= PROGRAM_NESTING {
            return self.too_deep_expr(open);
        }
        self.nest += 1;
        let index = self.expr();
        self.nest -= 1;
        if !matches!(self.peek(), Tok::RBracket) {
            if !self.failed {
                let s = self.span();
                let found = super::describe(self.peek());
                self.err(
                    "MZ0917",
                    s,
                    format!(
                        "expected `]` to close the index opened on column {}, found {found}",
                        open.start_col
                    ),
                );
            }
            return self.error_expr(join(base.span, index.span));
        }
        let close = self.bump().span;
        let span = join(base.span, close);
        if let ExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } = &index.kind
            && let ExprKind::Int(n) = operand.kind
            && n > 0
            && !base.has_error()
        {
            let b = canonical(&base);
            let b = if base.level() > 2 {
                format!("({b})")
            } else {
                b
            };
            let fixed = format!("{b}[{b}.length() - {n}]");
            self.err_fix(
                "MZ0915",
                span,
                format!(
                    "`{}` counts from the end, as Python does, and a Mzizi index counts from 0 and is `none` below it — the element {n} from the end is `{fixed}`",
                    canonical(&Expr {
                        kind: ExprKind::Index {
                            base: Box::new(base.clone()),
                            index: Box::new(index.clone()),
                        },
                        span,
                    })
                ),
                span,
                fixed,
                Confidence::Guess,
            );
            return self.error_expr(span);
        }
        Expr {
            kind: ExprKind::Index {
                base: Box::new(base),
                index: Box::new(index),
            },
            span,
        }
    }

    /// `name[index] = value`, the cursor on `name` with `[` after it: the statement, or, when
    /// no `=` follows the index, the expression it starts.
    pub(super) fn index_statement(&mut self, at: Span) -> Option<StmtKind> {
        let target = self.expr();
        if let Tok::Op(op @ ("+=" | "-=" | "*=" | "/=" | "++" | "--")) = self.peek().clone() {
            return self.index_operator_assignment(target, op);
        }
        if !matches!(self.peek(), Tok::Equals) {
            self.finish_line("an expression");
            return Some(StmtKind::Expr(target));
        }
        let eq = self.bump().span;
        let value = self.value();
        self.finish_value_line("an assignment");
        match target.kind {
            ExprKind::Index { base, index } if matches!(base.kind, ExprKind::Name(_)) => {
                let ExprKind::Name(name) = base.kind else {
                    return None;
                };
                Some(StmtKind::IndexAssign {
                    name,
                    name_span: base.span,
                    index: *index,
                    value,
                })
            }
            ExprKind::Error => None,
            _ => {
                if !self.failed {
                    self.err(
                        "MZ0917",
                        join(at, eq),
                        "an indexed assignment changes one element of a `var` by one index, `xs[i] = v` — bind an inner list to its own `var` first",
                    );
                }
                None
            }
        }
    }

    /// `xs[i] += v` and the rest (`MZ0918`), the cursor on the operator. `xs[i]` is an
    /// option, so the repair needs a default only the author knows: the fix is a `guess`
    /// with `0` (or `0.0` beside a float literal), and there is none beside any other value.
    /// The statement is still an indexed assignment, so the `var` counts as changed.
    fn index_operator_assignment(&mut self, target: Expr, op: &'static str) -> Option<StmtKind> {
        let op_at = self.bump().span;
        let rhs = if matches!(op, "++" | "--") {
            Expr {
                kind: ExprKind::Int(1),
                span: op_at,
            }
        } else {
            self.expr()
        };
        let line = join(
            target.span,
            if matches!(op, "++" | "--") {
                op_at
            } else {
                rhs.span
            },
        );
        self.finish_line("an assignment");
        let ExprKind::Index { base, index } = target.kind else {
            return None;
        };
        let ExprKind::Name(name) = base.kind else {
            return None;
        };
        let written = format!("{name}[{}]", canonical(&index));
        let zero = match rhs.kind {
            ExprKind::Int(_) => Some("0"),
            ExprKind::Float(_) => Some("0.0"),
            _ => None,
        };
        let sym = match op {
            "+=" | "++" => "+",
            "-=" | "--" => "-",
            "*=" => "*",
            _ => "/",
        };
        let rhs_text = {
            let t = canonical(&rhs);
            if rhs.level() >= 4 {
                format!("({t})")
            } else {
                t
            }
        };
        let say = format!(
            "`{op}` is not a Mzizi operator, and `{written}` is an option — write `{written} = ({written} otherwise <default>) {sym} {rhs_text}`"
        );
        match zero {
            Some(zero) if !index.has_error() && !rhs.has_error() => {
                let fixed = format!("{written} = ({written} otherwise {zero}) {sym} {rhs_text}");
                self.err_fix("MZ0918", op_at, say, line, fixed, Confidence::Guess);
            }
            _ => self.err("MZ0918", op_at, say),
        }
        Some(StmtKind::IndexAssign {
            name,
            name_span: base.span,
            index: *index,
            value: self.error_expr(line),
        })
    }

    /// Whether the argument at the cursor is a lambda: `x => …`, `(x, y) => …` or Python's
    /// `lambda x: …`. Mzizi has none (RFC-0013 §6.4).
    pub(super) fn at_lambda(&self) -> bool {
        match (self.peek(), self.peek_at(1)) {
            (Tok::Ident(_), Tok::Op("=>")) => true,
            (Tok::Ident(w), Tok::Ident(_) | Tok::Colon) if w == "lambda" => true,
            (Tok::LParen, _) => {
                let mut k = 1;
                loop {
                    match self.peek_at(k) {
                        Tok::RParen => return matches!(self.peek_at(k + 1), Tok::Op("=>")),
                        Tok::Ident(_) | Tok::Comma | Tok::Colon => k += 1,
                        _ => return false,
                    }
                }
            }
            _ => false,
        }
    }

    /// `MZ0909`: a lambda as an argument. The argument is passed over to the next `,` or the
    /// call's `)` and read as an error value.
    pub(super) fn lambda(&mut self) -> Expr {
        let at = self.span();
        let mut depth = 0usize;
        let mut end = at;
        while !self.at_line_end() {
            match self.peek() {
                Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RParen | Tok::RBracket if depth == 0 => break,
                Tok::RParen | Tok::RBracket => depth -= 1,
                Tok::Comma if depth == 0 => break,
                _ => {}
            }
            end = self.bump().span;
        }
        let span = join(at, end);
        if !self.failed {
            self.err(
                "MZ0909",
                span,
                "Mzizi has no lambdas — declare a `fn` (`fn double(x: int): int`) and pass its name: `xs.map(double)` (RFC-0013 §6.4)",
            );
        }
        self.error_expr(span)
    }

    /// `MZ0963`: `(a, b)`, a tuple, the cursor on the `,` after `a`. Mzizi has none: two
    /// values travel together in a record, whose field names only the author knows, so
    /// there is no fix.
    pub(super) fn tuple(&mut self, at: Span) -> Expr {
        let mut depth = 0usize;
        let mut end = self.prev();
        while !self.at_line_end() {
            match self.peek() {
                Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RParen | Tok::RBracket if depth == 0 => {
                    end = self.bump().span;
                    break;
                }
                Tok::RParen | Tok::RBracket => depth -= 1,
                _ => {}
            }
            end = self.bump().span;
        }
        let span = join(at, end);
        if !self.failed {
            self.err(
                "MZ0963",
                span,
                "Mzizi has no tuples — two values that travel together are a record with named fields (RFC-0013 §2, §11)",
            );
        }
        self.error_expr(span)
    }

    /// Python's `x not in xs`, the cursor on `not` with `in` after it: `MZ0910`, whose `exact`
    /// fix writes `not x in xs`, which reads `not (x in xs)` (§3.5). The tree holds the repair.
    pub(super) fn not_in(&mut self, lhs: Expr) -> Expr {
        let not_at = self.bump().span;
        let in_at = self.bump().span;
        let first_diag = self.diags.len();
        let rhs = self.otherwise_expr();
        let span = join(lhs.span, rhs.span);
        let inner = Expr {
            span,
            kind: ExprKind::Binary {
                op: BinOp::In,
                op_span: in_at,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        };
        let e = Expr {
            span,
            kind: ExprKind::Unary {
                op: UnOp::Not,
                operand: Box::new(inner),
            },
        };
        let confidence = self.fold_inner_fixes(span, first_diag);
        let fixed = canonical(&e);
        let d = Diagnostic::error(
            "MZ0910",
            &self.file,
            join(not_at, in_at),
            format!("`not in` is Python's — Mzizi writes `{fixed}`, which reads `not (… in …)`"),
        );
        self.diags.push(if e.has_error() {
            d
        } else {
            d.with_fix(span, fixed, confidence)
        });
        e
    }

    /// Level 6 of RFC-0013 §3.5: `a otherwise b`, right-associative, so a chain of fallbacks
    /// nests to the right. Each link is a level of nesting, capped as a prefix operator is.
    pub(super) fn otherwise_expr(&mut self) -> Expr {
        let lhs = self.add_expr();
        if !self.is_word("otherwise") {
            return lhs;
        }
        let at = self.bump().span;
        if self.nest >= PROGRAM_NESTING {
            return self.too_deep_expr(at);
        }
        self.nest += 1;
        let rhs = self.otherwise_expr();
        self.nest -= 1;
        Expr {
            span: join(lhs.span, rhs.span),
            kind: ExprKind::Binary {
                op: BinOp::Otherwise,
                op_span: at,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        }
    }

    /// The one label a call or method reads before §6.5's labels are built: `range`'s `to`,
    /// `slice`'s `to`, `fold`'s `step` and Python's `sorted(…, key = f)`.
    pub(super) fn call_label(name: &str) -> Option<(usize, &'static str)> {
        match name {
            "range" | "slice" => Some((1, "to")),
            "fold" => Some((1, "step")),
            "sorted" => Some((1, "key")),
            _ => None,
        }
    }
}
