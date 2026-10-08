//! Lowering RFC-0013 §12 (§14.2's rows for it): `result(T, E)` as Rust's `Result<T, E>`,
//! `return error(e)` as `return Err(e);`, prefix `try` as postfix `?`, the `case ok` and
//! `case error` of a `match` on a result as `Ok(v)` and `Err(e)` patterns, an enum's
//! columns as accessor methods, and a `main` that returns `result(none, E)`, whose error is
//! one line on standard error and exit status 1 (§12.5). Nothing here emits `unwrap`,
//! `expect`, `panic!` or `unsafe` (§14.3).
//!
//! The enum itself and the `match` around these arms are C4's (`run/control.rs`).

use std::fmt::Write as _;

use super::{Lower, ident};
use crate::expr::{Arm, Expr, ExprKind, TextPart, Ty};
use crate::program::{EnumDecl, FnDecl, terminates};

/// Whether `f` is `fn main` returning `result(none, E)`, lowered as `mz_main_result` beside
/// a generated `mz_main` that reports its error.
pub(super) fn is_result_main(f: &FnDecl) -> bool {
    f.name == "main" && f.ret.is_some_and(|r| r.ty.as_result().is_some())
}

/// The generated `mz_main` for a `main` that returns a result: on an error, the line of
/// §12.5 on standard error and exit status 1. Writing to standard error can fail; nothing
/// is left to report it to, so its result is dropped rather than unwrapped.
pub(super) const RESULT_MAIN: &str = r#"
/// RFC-0013 §12.5: `main` returned an error. One line on standard error, exit status 1.
fn mz_main() {
    if let Err(e) = mz_main_result() {
        let mut err = ::std::io::stderr().lock();
        let _ = writeln!(
            err,
            "mz: error MZ0992: main returned an error: {}",
            MzText::mz_text(&e)
        );
        ::std::process::exit(1)
    }
}
"#;

/// The tail of a function returning `result(none, E)` whose end can be reached: `Ok(())`.
pub(super) fn none_result_tail(f: &FnDecl) -> Option<&'static str> {
    let ret = f.ret?.ty;
    let (ok, _) = ret.as_result()?;
    (ok == Ty::Nothing && !terminates(&f.body)).then_some("    Ok(())\n")
}

/// A column's literal value as Rust: an `int`, a `bool` or a `text` with no interpolation.
fn column_value(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(v) => format!("{v}i64"),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Text(parts) => {
            let lit: String = parts
                .iter()
                .map(|p| match p {
                    TextPart::Lit(s) => s.as_str(),
                    TextPart::Expr(_) => "",
                })
                .collect();
            format!("String::from({lit:?})")
        }
        _ => "()".to_string(),
    }
}

impl Lower<'_> {
    /// An enum's columns, one accessor method each (§14.2's table), in an `impl` after the
    /// enum: `problem.say` is `problem.say()`.
    pub(super) fn enum_columns(&self, e: &EnumDecl, out: &mut String) {
        let Some((rust, variants)) = self.rust_enums.get(&e.name) else {
            return;
        };
        let columns: Vec<&str> = e
            .variants
            .first()
            .map(|v| v.columns.iter().map(|c| c.name.as_str()).collect())
            .unwrap_or_default();
        if columns.is_empty() {
            return;
        }
        let _ = writeln!(out, "\nimpl {rust} {{");
        for (k, col) in columns.iter().enumerate() {
            let ty = self.rust_type(e.column_type(col).unwrap_or(Ty::Error));
            if k > 0 {
                out.push('\n');
            }
            let _ = writeln!(
                out,
                "    fn {}(&self) -> {ty} {{\n        match *self {{",
                ident(col)
            );
            for v in &e.variants {
                let value = v
                    .columns
                    .iter()
                    .find(|c| c.name == *col)
                    .map_or_else(|| "()".to_string(), |c| column_value(&c.value));
                let _ = writeln!(
                    out,
                    "            {rust}::{} => {value},",
                    variants
                        .get(&v.name)
                        .map_or(v.name.as_str(), String::as_str)
                );
            }
            out.push_str("        }\n    }\n");
        }
        out.push_str("}\n");
    }

    /// The type of `base.name`: a column of an enum value, or a field of a record.
    pub(super) fn field_ty(&self, base: &Expr, name: &str) -> Ty {
        match self.ty(base) {
            Ty::Record(r) => self
                .records
                .iter()
                .find(|d| d.name == r)
                .and_then(|d| d.field(name))
                .map_or(Ty::Error, |f| f.ty.ty),
            Ty::Enum(en) => self
                .enums
                .iter()
                .find(|e| e.name == en)
                .and_then(|e| e.column_type(name))
                .unwrap_or(Ty::Error),
            _ => Ty::Error,
        }
    }

    /// `base.name` as Rust: the column's accessor call, or a record's field, copied out when
    /// its type is not `Copy` (RFC-0013 §14.1).
    pub(super) fn field(&mut self, base: &Expr, name: &str) -> String {
        if let Ty::Record(_) = self.ty(base) {
            let t = self.field_ty(base, name);
            // A binding's field is read in place, without copying the whole record.
            let owner = match &base.kind {
                ExprKind::Name(n) => ident(n),
                _ => self.atom(base),
            };
            let read = format!("{owner}.{}", ident(name));
            return if t.is_copy() {
                read
            } else {
                format!("{read}.clone()")
            };
        }
        format!("{}.{}()", self.atom(base), ident(name))
    }

    /// `try e` as Rust: `e?`, parenthesised unless `e` is a name or a call.
    pub(super) fn try_expr(&mut self, operand: &Expr) -> String {
        let inner = self.expr(operand);
        match operand.kind {
            ExprKind::Name(_) | ExprKind::Call { .. } => format!("{inner}?"),
            _ => format!("({inner})?"),
        }
    }

    /// `return v`: in a function that returns a result, `Ok(v)` when `v` is the success
    /// value, and `v` as it is when it is already a result of the function's type (§12.1).
    pub(super) fn returned(&mut self, v: &Expr) -> String {
        let ret = self.ret;
        match ret.as_result() {
            Some((ok, _)) if self.ty(v) != ret => format!("Ok({})", self.expr_want(v, ok)),
            _ => self.expr_want(v, ret),
        }
    }

    /// `return` with no value in a function returning `result(none, E)`: `return Ok(());`.
    pub(super) fn bare_return(&self) -> &'static str {
        match self.ret.as_result() {
            Some(_) => "return Ok(());",
            None => "return;",
        }
    }

    /// Record the type of each name a `match` on a result used as the value `e` binds.
    pub(super) fn case_names(&mut self, e: &Expr) {
        let ExprKind::Match {
            scrutinee, arms, ..
        } = &e.kind
        else {
            return;
        };
        let t = self.ty(scrutinee);
        let Some((ok, err)) = t.as_result() else {
            return;
        };
        for a in arms {
            if let Some((name, _)) = &a.binding {
                let is_error = matches!(a.values.first().map(|v| &v.kind), Some(ExprKind::Name(n)) if n == "error");
                self.types
                    .insert(name.clone(), if is_error { err } else { ok });
            }
        }
    }

    /// The pattern of a `case ok <name>` or `case error <name>` arm of a `match` on a
    /// result of type `t`: `Ok(name)` or `Err(name)`, `_` when it binds none. The name's
    /// type is recorded for the arm's body.
    pub(super) fn result_pattern<B>(&mut self, a: &Arm<B>, t: Ty) -> String {
        let Some((ok, err)) = t.as_result() else {
            return "_".to_string();
        };
        let (ctor, bound) = match a.values.first().map(|v| &v.kind) {
            Some(ExprKind::Name(n)) if n == "error" => ("Err", err),
            _ => ("Ok", ok),
        };
        let name = match &a.binding {
            Some((name, _)) => {
                self.types.insert(name.clone(), bound);
                ident(name)
            }
            None => "_".to_string(),
        };
        format!("{ctor}({name})")
    }
}
