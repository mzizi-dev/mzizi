//! Errors in a program (RFC-0013 §12, tracker row C9): `result(T, E)`, `return error(e)`,
//! prefix `try`, a `match` on a result, and an enum's columns.
//!
//! The tree types for an enum live here, and so does the checker's half of §12: the codes
//! `MZ0950` (an unhandled result), `MZ0951` (a `try` that cannot propagate), `MZ0953`
//! (`error(e)` that does not fit its function, or a bare `ok` / `error` variant in a result
//! function) and `MZ0954` (`return try r` that only re-wraps `r`). `MZ0952`, the error idioms
//! of other languages, is the parser's (`parse/program/errors.rs`), because each is a
//! spelling repaired in the tree.
//!
//! A `match` on a result is C4's `match` (`program/control.rs`): its coverage keys are `ok`
//! and `error`, so `MZ0930` and `MZ0931` come from one place. What it adds is here: the
//! name each case binds ([`FnCheck::bind_case`]).

use super::{FnCheck, Kind};
use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::expr::{Arm, Expr, ExprKind, Ty, canonical};
use crate::resolve::nearest;

/// `enum <name>` … `end` in a program (RFC-0013 §7.2): variants in declaration order, each
/// with the same literal columns (RFC-0001 §1.3, §12.1), as in a component.
#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    /// The enum's name.
    pub name: String,
    /// Where the name is.
    pub name_span: Span,
    /// Its variants, in declaration order, which is also their order for `<` (§3.3).
    pub variants: Vec<Variant>,
}

/// One variant and its column values.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    /// The variant's name.
    pub name: String,
    /// Where it is.
    pub span: Span,
    /// Its columns, in the order written.
    pub columns: Vec<Column>,
    /// Whether its line was read to the end: `false` after a column without a literal
    /// (`MZ0302`), whose later columns were not read, so it is not compared with the rest.
    pub complete: bool,
}

/// `say "no digits"`: a column and its literal value.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    /// The column's name.
    pub name: String,
    /// Where the name is.
    pub span: Span,
    /// The value: an `int`, `bool` or `text` literal with no interpolation.
    pub value: Expr,
}

impl EnumDecl {
    /// Whether `name` is one of the enum's variants.
    pub fn has(&self, name: &str) -> bool {
        self.variants.iter().any(|v| v.name == name)
    }

    /// The type of column `name`, read off the first variant that has it.
    pub fn column_type(&self, name: &str) -> Option<Ty> {
        self.variants
            .iter()
            .flat_map(|v| &v.columns)
            .find(|c| c.name == name)
            .map(|c| literal_type(&c.value))
    }
}

/// The type of a column's literal.
fn literal_type(e: &Expr) -> Ty {
    match e.kind {
        ExprKind::Int(_) => Ty::Int,
        ExprKind::Bool(_) => Ty::Bool,
        ExprKind::Text(_) => Ty::Text,
        _ => Ty::Error,
    }
}

/// Whether `t` is `result(none, E)`, which returns success by a bare `return` or by
/// reaching `end fn` (§12.1, §6.2's one exception).
pub fn returns_none_result(t: Ty) -> bool {
    t.as_result().is_some_and(|(ok, _)| ok == Ty::Nothing)
}

/// Every variant has every column, once, and a column has one type (RFC-0001 §1.3). The
/// columns are those any variant has, so a variant missing one is the one reported, not
/// every variant that has it; a variant whose line was cut short (`MZ0302`) is left out.
pub(super) fn check_columns(e: &EnumDecl, file: &str, diags: &mut Vec<Diagnostic>) {
    let read: Vec<&Variant> = e.variants.iter().filter(|v| v.complete).collect();
    // Every column some variant has, in first-written order, with the first variant that
    // has it.
    let mut all: Vec<(&str, &str)> = Vec::new();
    for v in &read {
        for c in &v.columns {
            if !all.iter().any(|(n, _)| *n == c.name) {
                all.push((&c.name, &v.name));
            }
        }
    }
    for v in &read {
        let mut seen: Vec<&str> = Vec::new();
        for c in &v.columns {
            if seen.contains(&c.name.as_str()) {
                diags.push(Diagnostic::error(
                    "MZ0704",
                    file,
                    c.span,
                    format!("variant `{}` has column `{}` twice", v.name, c.name),
                ));
            }
            seen.push(&c.name);
            if let Some(want) = e.column_type(&c.name) {
                let have = literal_type(&c.value);
                if have != want && have != Ty::Error && want != Ty::Error {
                    diags.push(Diagnostic::error(
                        "MZ0711",
                        file,
                        c.value.span,
                        format!(
                            "column `{}` of `{}` is {}, and on `{}` it is {}",
                            c.name,
                            e.name,
                            want.name(),
                            v.name,
                            have.name()
                        ),
                    ));
                }
            }
        }
        for (want, owner) in &all {
            if !v.columns.iter().any(|c| c.name == *want) {
                diags.push(Diagnostic::error(
                    "MZ0303",
                    file,
                    v.span,
                    format!(
                        "variant `{}` has no `{want}` column, but `{owner}` does — every variant needs every column",
                        v.name
                    ),
                ));
            }
        }
    }
}

impl FnCheck<'_> {
    /// The success and error types of the function being checked, when it returns a result.
    pub(super) fn ret_result(&self) -> Option<(Ty, Ty)> {
        self.f.ret.and_then(|r| r.ty.as_result())
    }

    /// Whether the function's return type was written and already reported as wrong, so
    /// what depends on it says nothing more.
    fn ret_unknown(&self) -> bool {
        self.f.ret.is_some_and(|r| r.ty == Ty::Error)
    }

    /// The `guess` fix `MZ0950` carries: `try ` inserted before `e`, when the enclosing
    /// function returns a result with `t`'s error type (§12.3). Never `exact`: propagating
    /// changes what the function does.
    fn try_fix(&self, e: Span, t: Ty) -> Option<Span> {
        let (_, err) = t.as_result()?;
        let (_, fn_err) = self.ret_result()?;
        (err == fn_err).then(|| Span::single(e.start_line, e.start_col, 0))
    }

    /// `MZ0950` when `t` is a result used where it cannot be: `how` says where, and is only
    /// built when there is something to report. True when it reported, so the caller says
    /// nothing more about the value.
    pub(super) fn unhandled(&mut self, e: &Expr, t: Ty, how: impl FnOnce() -> String) -> bool {
        self.unhandled_wanting(e, t, None, how)
    }

    /// [`Self::unhandled`], where the value is wanted as `want`: the `guess` `try` is
    /// offered only when the success value is of that type, since otherwise it leads to
    /// the next type error.
    pub(super) fn unhandled_wanting(
        &mut self,
        e: &Expr,
        t: Ty,
        want: Option<Ty>,
        how: impl FnOnce() -> String,
    ) -> bool {
        // An option, used where its value is meant, is `MZ0710` (RFC-0013 §8).
        if let Ty::Option(_) = t {
            let _ = how;
            return self.unnarrowed(e, t, "it is used here as its value");
        }
        let Some((ok, _)) = t.as_result() else {
            return false;
        };
        let say = format!(
            "`{}` is a {}, and {} — a result is matched (`match`, with `case ok` and `case error`) or propagated (`try`)",
            canonical(e),
            t.name(),
            how()
        );
        let fix = self
            .try_fix(e.span, t)
            .filter(|_| want.is_none_or(|w| w == ok));
        match fix {
            Some(at) => self.err_fix("MZ0950", e.span, say, at, "try ", Confidence::Guess),
            None => self.err("MZ0950", e.span, say),
        }
        true
    }

    /// [`Self::unhandled`] for a result only: where an option is what is wanted (the left
    /// of `otherwise`, an element of a collection).
    pub(super) fn unhandled_result(&mut self, e: &Expr, t: Ty, how: &str) -> bool {
        if matches!(t, Ty::Option(_)) {
            return false;
        }
        self.unhandled(e, t, || how.to_string())
    }

    /// `MZ0953`: a bare variant `ok` or `error` (resolved to `enum en`) in a function that
    /// returns a result reads as success or failure, so it is written with its enum, the
    /// `exact` fix (§1).
    pub(super) fn bare_ok_error(&mut self, name: &str, at: Span, t: Ty) {
        let Ty::Enum(en) = t else {
            return;
        };
        if !matches!(name, "ok" | "error") || self.ret_result().is_none() {
            return;
        }
        let qualified = format!("{en}.{name}");
        let what = if name == "ok" { "success" } else { "failure" };
        self.err_fix(
            "MZ0953",
            at,
            format!(
                "a bare `{name}` in `{}`, which returns a result, reads as a {what} — the variant is written `{qualified}`",
                self.f.signature()
            ),
            at,
            qualified,
            Confidence::Exact,
        );
    }

    /// `base.name` with no parentheses: an enum value's column (§12.1). A variant named
    /// through its enum is C4's `ExprKind::Variant`.
    pub(super) fn field(&mut self, e: &Expr, base: &Expr, name: &str, name_span: Span) -> Ty {
        let t = self.expr(base);
        self.field_of(e, base, t, name, name_span)
    }

    /// `.name` on a value of type `t`: an enum's column. On anything else it is a method
    /// written without its parentheses, which [`FnCheck::method_on`] reports. `e` is the
    /// whole `base.name`.
    fn field_of(&mut self, e: &Expr, base: &Expr, t: Ty, name: &str, name_span: Span) -> Ty {
        let whole = Span {
            end_line: name_span.end_line,
            end_col: name_span.end_col,
            ..base.span
        };
        match t {
            Ty::Error => Ty::Error,
            Ty::Enum(en) => {
                let Some(e) = self.enums.iter().find(|d| d.name == en) else {
                    return Ty::Error;
                };
                if let Some(ct) = e.column_type(name) {
                    return ct;
                }
                let mut cols: Vec<&str> = e
                    .variants
                    .first()
                    .map(|v| v.columns.iter().map(|c| c.name.as_str()).collect())
                    .unwrap_or_default();
                cols.sort();
                let say = format!(
                    "`{}` is {en}, which has no column `{name}`",
                    canonical(base)
                );
                match nearest(name, cols) {
                    Some((near, _)) => self.err_fix(
                        "MZ0708",
                        name_span,
                        format!("{say} — did you mean `{near}`?"),
                        name_span,
                        near,
                        Confidence::Guess,
                    ),
                    None => self.err("MZ0708", name_span, say),
                }
                Ty::Error
            }
            t if t.as_result().is_some() => {
                // `try` binds looser than a dot, so the `guess` that propagates is
                // `(try base).name`, not `try base.name`.
                let say = format!(
                    "`{}.{name}` reads `.{name}` off a {}, which may be an error — a result is matched (`match`, with `case ok` and `case error`) or propagated (`try`)",
                    canonical(base),
                    t.name()
                );
                match self.try_fix(base.span, t) {
                    Some(_) => self.err_fix(
                        "MZ0950",
                        whole,
                        say,
                        whole,
                        format!("(try {}).{name}", canonical(base)),
                        Confidence::Guess,
                    ),
                    None => self.err("MZ0950", whole, say),
                }
                Ty::Error
            }
            t => self.method_on(e, base, t, name, name_span, &[], false),
        }
    }

    /// `r.name(…)` with `r` a result: `MZ0950`, with the `guess` `(try r).name(…)` where the
    /// function propagates `r`'s error type. `try` binds looser than a dot, so the
    /// parentheses are needed.
    pub(super) fn method_on_result(
        &mut self,
        e: &Expr,
        recv: &Expr,
        t: Ty,
        name: &str,
        args: &[Expr],
        called: bool,
    ) -> Ty {
        let say = format!(
            "`{}.{name}` calls a method on a {}, which may be an error — a result is matched (`match`, with `case ok` and `case error`) or propagated (`try`)",
            canonical(recv),
            t.name()
        );
        match self.try_fix(recv.span, t) {
            Some(_) if called && !e.has_error() => {
                let args: Vec<String> = args.iter().map(canonical).collect();
                let fixed = format!("(try {}).{name}({})", canonical(recv), args.join(", "));
                self.err_fix("MZ0950", e.span, say, e.span, fixed, Confidence::Guess);
            }
            _ => self.err("MZ0950", e.span, say),
        }
        Ty::Error
    }

    /// `try operand` (§12.2).
    pub(super) fn try_expr(&mut self, e: &Expr, operand: &Expr) -> Ty {
        // `try f(x).y` is `try (f(x).y)`, since a dot binds tighter (§3.5). When `f(x)` is
        // the result, the author meant `(try f(x)).y`, which is the `exact` fix.
        if let ExprKind::Field {
            base,
            name,
            name_span,
        } = &operand.kind
        {
            let bt = self.expr(base);
            if bt.as_result().is_some() && base.has_error() {
                // The base was reported, and its text holds a placeholder.
                return Ty::Error;
            }
            if bt.as_result().is_some() {
                let fixed = format!("(try {}).{name}", canonical(base));
                self.err_fix(
                    "MZ0950",
                    e.span,
                    format!(
                        "`{}` reads `.{name}` off a {} before `try` unwraps it — `try` binds looser than a dot: write `{fixed}`",
                        canonical(e),
                        bt.name()
                    ),
                    e.span,
                    fixed,
                    Confidence::Exact,
                );
                return Ty::Error;
            }
            let t = self.field_of(operand, base, bt, name, *name_span);
            return self.try_of(e, operand, t);
        }
        let t = self.expr(operand);
        self.try_of(e, operand, t)
    }

    /// The type of `try operand`, where `operand` has type `t`; `MZ0951` when it cannot
    /// propagate. No fix: the repair is a signature the author chooses.
    pub(super) fn try_of(&mut self, e: &Expr, operand: &Expr, t: Ty) -> Ty {
        if t == Ty::Error {
            return Ty::Error;
        }
        let Some((ok, err)) = t.as_result() else {
            self.err(
                "MZ0951",
                e.span,
                format!(
                    "`try` takes a result, and `{}` is {} — there is nothing to propagate",
                    canonical(operand),
                    t.name()
                ),
            );
            return t;
        };
        let sig = self.f.signature();
        match self.ret_result() {
            None if self.ret_unknown() => {}
            None => {
                self.err(
                    "MZ0951",
                    e.span,
                    format!(
                        "`try` returns `{}`'s error from `{sig}`, which does not return a result — declare it `: result(…, {})`, or handle the error with `match`",
                        canonical(operand),
                        err.name()
                    ),
                );
                // Reported: what the `try` feeds says nothing more (no `MZ0908` too).
                return Ty::Error;
            }
            Some((_, fn_err)) if fn_err != err => {
                self.err(
                    "MZ0951",
                    e.span,
                    format!(
                        "`{}` fails with {}, and `{sig}` fails with {} — `try` cannot convert one error type to another",
                        canonical(operand),
                        err.name(),
                        fn_err.name()
                    ),
                );
                return Ty::Error;
            }
            Some(_) => {}
        }
        ok
    }

    /// `error(e)`, the failure value (§12.1). Its type is the function's own result type.
    pub(super) fn fail_value(&mut self, args: &[Expr], types: &[Ty], at: Span) -> Ty {
        let sig = self.f.signature();
        let Some((_, err)) = self.ret_result() else {
            if self.ret_unknown() {
                return Ty::Error;
            }
            self.err(
                "MZ0953",
                at,
                format!(
                    "`error(…)` is a function's failure, and `{sig}` does not return a result — declare `: result(<type>, <error type>)`, or do not fail here"
                ),
            );
            return Ty::Error;
        };
        let ret = self.f.ret.map_or(Ty::Error, |r| r.ty);
        match (args, types) {
            ([a], [t]) => {
                if *t != Ty::Error && *t != err {
                    self.err(
                        "MZ0953",
                        a.span,
                        format!(
                            "`{sig}` fails with {}, and `{}` is {}",
                            err.name(),
                            canonical(a),
                            t.name()
                        ),
                    );
                }
            }
            _ => self.err(
                "MZ0953",
                at,
                format!(
                    "`error` takes the one value `{sig}` fails with, of type {}: `error(e)`",
                    err.name()
                ),
            ),
        }
        ret
    }

    /// `return v` in a function that returns `ret`, a result: `v` of the success type is
    /// wrapped, `v` of `ret` itself is passed through, and `return try r` with `r` of `ret`
    /// is `MZ0954` (§12.1).
    pub(super) fn return_result(&mut self, v: &Expr, ret: Ty) {
        let Some((ok, _)) = ret.as_result() else {
            return;
        };
        let t = match &v.kind {
            ExprKind::Unary {
                op: crate::expr::UnOp::Try,
                operand,
            } if !matches!(operand.kind, ExprKind::Field { .. }) => {
                let ot = self.expr(operand);
                if ot == ret {
                    let delete = Span {
                        start_line: v.span.start_line,
                        start_col: v.span.start_col,
                        end_line: operand.span.start_line,
                        end_col: operand.span.start_col,
                    };
                    self.err_fix(
                        "MZ0954",
                        v.span,
                        format!(
                            "`return {}` unwraps `{}` only to wrap it again — a result of the function's own type is returned as it is: `return {}`",
                            canonical(v),
                            canonical(operand),
                            canonical(operand)
                        ),
                        delete,
                        "",
                        Confidence::Exact,
                    );
                    return;
                }
                self.try_of(v, operand, ot)
            }
            _ => self.expr_want(v, ok),
        };
        if t == Ty::Error || t == ret || (t == ok && ok != Ty::Nothing) {
            return;
        }
        let sig = self.f.signature();
        if t.as_result().is_some() {
            self.unhandled(v, t, || format!("`{sig}` returns {}", ret.name()));
            return;
        }
        self.err(
            "MZ0908",
            v.span,
            format!(
                "`{sig}` returns {}, so `return` takes {} or `error(…)`, and `{}` is {}",
                ret.name(),
                if ok == Ty::Nothing {
                    "no value".to_string()
                } else {
                    format!("a value of {}", ok.name())
                },
                canonical(v),
                t.name()
            ),
        );
    }

    /// The name a result's `case` binds, in the arm's scope (pushed by the caller), typed
    /// as its success or error value; `t` is the type matched. `case ok` with no name is
    /// `MZ0917` when the success is a value.
    pub(super) fn bind_case<B>(&mut self, a: &Arm<B>, t: Ty) {
        let Some((ok, err)) = t.as_result() else {
            // The name is still bound, so its uses say nothing more: the value matched
            // was already reported (`t` unknown), or this is not a result's `match`.
            if let Some((name, _)) = &a.binding {
                if t != Ty::Error {
                    self.err(
                        "MZ0917",
                        a.span,
                        format!(
                            "`case {} {name}` binds a name, which only a `match` on a result does, and this `match` is over {} — list values only",
                            a.values.first().map_or(String::new(), canonical),
                            t.name()
                        ),
                    );
                }
                self.bind(name, Kind::Poison, Ty::Error, None);
            }
            return;
        };
        let is_ok =
            matches!(a.values.first().map(|v| &v.kind), Some(ExprKind::Name(n)) if n == "ok");
        // A malformed case line (`MZ0917`, reported once): every name it lists is bound,
        // unknown, so its uses in the arm say nothing more.
        let well_formed = matches!(a.values.as_slice(), [v] if matches!(&v.kind, ExprKind::Name(n) if n == "ok" || n == "error"));
        if !well_formed {
            for v in a.values.iter().skip(1) {
                if let ExprKind::Name(n) = &v.kind {
                    self.bind(n, Kind::Poison, Ty::Error, None);
                }
            }
            if let Some((name, _)) = &a.binding {
                self.bind(name, Kind::Poison, Ty::Error, None);
            }
            return;
        }
        let bound = if is_ok { ok } else { err };
        match &a.binding {
            Some((name, span)) => {
                let kind = if self.may_bind(name, *span) {
                    Kind::Let
                } else {
                    Kind::Poison
                };
                self.bind(name, kind, bound, None);
            }
            None if is_ok && ok != Ty::Nothing && ok != Ty::Error => {
                self.err(
                    "MZ0917",
                    a.span,
                    format!(
                        "`case ok` names the success value, which is {}: write `case ok <name>`",
                        ok.name()
                    ),
                );
            }
            None => {}
        }
    }

    /// Pop the innermost scope, and report each `let` in it that holds a result nothing
    /// matched or propagated (`MZ0950`, §12.3).
    pub(super) fn end_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            self.unmatched_results(&scope);
        }
    }

    /// `MZ0950` for each binding in `scope` that holds a result and was never read.
    pub(super) fn unmatched_results(&mut self, scope: &[usize]) {
        // Lines the parser skipped unread may read the binding, as they may assign a `var`
        // (`MZ0924` waits the same way): the verdict waits until they are rewritten.
        if self.f.skipped {
            return;
        }
        let unread: Vec<(String, Span, Span, Ty)> = self
            .results
            .iter()
            .filter(|(i, ..)| scope.contains(i) && !self.bindings[*i].read)
            .map(|&(i, name, value)| {
                (
                    self.bindings[i].name.clone(),
                    name,
                    value,
                    self.bindings[i].ty,
                )
            })
            .collect();
        for (name, at, value, t) in unread {
            let say = format!(
                "`{name}` holds a {}, and nothing matches or propagates it before its block ends — `match {name}` with `case ok` and `case error`, or bind `try` of the value",
                t.name()
            );
            match self.try_fix(value, t) {
                Some(fix) => self.err_fix("MZ0950", at, say, fix, "try ", Confidence::Guess),
                None => self.err("MZ0950", at, say),
            }
        }
    }
}
