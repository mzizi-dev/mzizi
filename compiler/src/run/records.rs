//! Lowering records to Rust (RFC-0013 §11, §14.2).
//!
//! - A record is a Rust `struct` with its fields in declaration order, deriving `Clone`,
//!   `Debug` and `PartialEq`: `is` compares every field (§3.3), and a read of a record
//!   copies it (§14.1), as a collection's read does.
//! - A method is an `impl` method named `m_<name>` taking `&self`. The `m_` keeps a method
//!   from shadowing a derived trait's (`clone`, `eq`) or a generated `mz_` helper.
//! - A record's text form is `point(x = 1.0, y = 2.0)` (§3.8), through `MzText`, with text
//!   fields quoted as they are in a collection.
//! - Each `always` clause is checked by `mz_check`, which traps (MZ0991, exit 101) at the
//!   line that built or changed the value. A literal and a `with` check the value they
//!   build; a field assignment checks the record it changed.

use std::fmt::Write as _;

use super::{Lower, errors, ident};
use crate::expr::{Expr, FieldInit, Ty, canonical};
use crate::program::{FnDecl, RecordDecl};

/// The Rust name of a Mzizi method.
fn method_ident(name: &str) -> String {
    format!("m_{name}")
}

impl Lower<'_> {
    /// The declaration of record `r`, which the enums' names already name.
    pub(super) fn record_decl(&mut self, r: &RecordDecl, out: &mut String) {
        let Some((rust, _)) = self.rust_enums.get(&r.name) else {
            return;
        };
        let rust = rust.clone();
        let _ = writeln!(
            out,
            "\n// record {}\n#[derive(Clone, Debug, PartialEq)]\nstruct {rust} {{",
            r.name
        );
        for f in &r.fields {
            let _ = writeln!(out, "    {}: {},", ident(&f.name), self.rust_type(f.ty.ty));
        }
        out.push_str("}\n");
        let _ = writeln!(out, "\nimpl {rust} {{");
        for m in &r.methods {
            self.record_method(r, m, out);
        }
        if !r.always.is_empty() {
            self.mz_check(r, out);
        }
        out.push_str("}\n");
        let _ = writeln!(
            out,
            "\nimpl MzText for {rust} {{\n    fn mz_text(&self) -> String {{"
        );
        if r.fields.is_empty() {
            let _ = writeln!(out, "        String::from({:?})", format!("{}()", r.name));
        } else {
            let shape: Vec<String> = r
                .fields
                .iter()
                .map(|f| format!("{} = {{}}", f.name))
                .collect();
            let args: Vec<String> = r
                .fields
                .iter()
                .map(|f| format!("self.{}.mz_text_in()", ident(&f.name)))
                .collect();
            let _ = writeln!(
                out,
                "        format!({:?}, {})",
                format!("{}({})", r.name, shape.join(", ")),
                args.join(", ")
            );
        }
        out.push_str("    }\n}\n");
    }

    /// One method of record `r`, as a `&self` method of its `impl`.
    fn record_method(&mut self, r: &RecordDecl, m: &FnDecl, out: &mut String) {
        self.types.clear();
        self.types
            .insert("self".to_string(), Ty::Record(crate::expr::intern(&r.name)));
        self.ret = m.ret.map_or(Ty::Nothing, |t| t.ty);
        let mut params = Vec::new();
        for p in &m.params {
            self.types.insert(p.name.clone(), p.ty.ty);
            params.push(format!("{}: {}", ident(&p.name), self.rust_type(p.ty.ty)));
        }
        let ret = m
            .ret
            .map(|t| format!(" -> {}", self.rust_type(t.ty)))
            .unwrap_or_default();
        let comma = if params.is_empty() { "" } else { ", " };
        let _ = writeln!(
            out,
            "\n    // {}\n    fn {}(&self{comma}{}){ret} {{",
            m.signature(),
            method_ident(&m.name),
            params.join(", ")
        );
        self.block(&m.body, 2, out);
        if let Some(tail) = errors::none_result_tail(m) {
            out.push_str(tail);
        }
        out.push_str("    }\n");
    }

    /// `mz_check`: each field bound to its name, then each `always` clause, which traps when
    /// it does not hold (§11.4). Called with the trap site of the line that built the value.
    fn mz_check(&mut self, r: &RecordDecl, out: &mut String) {
        self.types.clear();
        for f in &r.fields {
            self.types.insert(f.name.clone(), f.ty.ty);
        }
        // The parameter's name is reserved (`mz_`), so no field of the record can shadow it.
        let _ = writeln!(out, "\n    fn mz_check(&self, mz_at: &MzAt) {{");
        for f in &r.fields {
            let _ = writeln!(
                out,
                "        let {} = self.{}.clone();",
                ident(&f.name),
                ident(&f.name)
            );
        }
        for inv in &r.always {
            let cond = self.expr(&inv.expr);
            let what = format!(
                "record `{}` breaks `always {}`",
                r.name,
                canonical(&inv.expr)
            );
            let _ = writeln!(out, "        if !({cond}) {{ mz_trap(mz_at, {what:?}); }}");
        }
        out.push_str("    }\n");
    }

    /// The record called `name`.
    fn record_of(&self, name: &str) -> Option<&RecordDecl> {
        self.records.iter().find(|d| d.name == name)
    }

    /// Whether the record called `name` has `always` clauses, so its values are checked.
    fn has_always(&self, name: &str) -> bool {
        self.record_of(name).is_some_and(|d| !d.always.is_empty())
    }

    /// The type a method of a record of type `rt` returns, or [`Ty::Error`].
    pub(super) fn record_method_ty(&self, rt: Ty, name: &str) -> Ty {
        let Ty::Record(r) = rt else {
            return Ty::Error;
        };
        self.record_of(r)
            .and_then(|d| d.method(name))
            .map_or(Ty::Error, |m| m.ret.map_or(Ty::Nothing, |t| t.ty))
    }

    /// `recv.name(args)` on a record: a call of its `m_` method, with each argument typed as
    /// its parameter is.
    pub(super) fn record_method_call(
        &mut self,
        recv: &Expr,
        rt: Ty,
        name: &str,
        args: &[Expr],
    ) -> String {
        let want: Vec<Ty> = match rt {
            Ty::Record(r) => self
                .record_of(r)
                .and_then(|d| d.method(name))
                .map(|m| m.params.iter().map(|p| p.ty.ty).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let mut lowered = Vec::with_capacity(args.len());
        for (k, a) in args.iter().enumerate() {
            lowered.push(self.expr_want(a, want.get(k).copied().unwrap_or(Ty::Error)));
        }
        format!(
            "{}.{}({})",
            self.atom(recv),
            method_ident(name),
            lowered.join(", ")
        )
    }

    /// The Rust type of field `name` of record `r`, or [`Ty::Error`].
    pub(super) fn field_of(&self, r: &str, name: &str) -> Ty {
        self.record_of(r)
            .and_then(|d| d.field(name))
            .map_or(Ty::Error, |f| f.ty.ty)
    }

    /// `point(x = 1.0, y = 2.0)`: the struct, built in declaration order (the checker
    /// requires it), and checked by its `always` clauses when it has any.
    pub(super) fn record_literal(&mut self, e: &Expr, name: &str, fields: &[FieldInit]) -> String {
        let rust = self
            .rust_enums
            .get(name)
            .map_or_else(|| "()".to_string(), |(n, _)| n.clone());
        let mut values = Vec::with_capacity(fields.len());
        for f in fields {
            let want = self.field_of(name, &f.name);
            let v = self.expr_want(&f.value, want);
            values.push(format!("{}: {v}", ident(&f.name)));
        }
        let build = format!("{rust} {{ {} }}", values.join(", "));
        let has_always = self.has_always(name);
        if !has_always {
            return format!("({build})");
        }
        let at = self.site(e);
        format!("({{ let mz_value = {build}; mz_value.mz_check({at}); mz_value }})")
    }

    /// `base with (x = 3.0)`: a copy of `base` with each field set, then checked by the
    /// record's `always` clauses (§11.1, §11.4). The copy is made from `base`, which is read
    /// as any other read is.
    pub(super) fn with_expr(&mut self, e: &Expr, base: &Expr, fields: &[FieldInit]) -> String {
        let Ty::Record(name) = self.ty(base) else {
            return "()".to_string();
        };
        let mut body = format!("let mut mz_value = {}; ", self.expr(base));
        for f in fields {
            let want = self.field_of(name, &f.name);
            let v = self.expr_want(&f.value, want);
            let _ = write!(body, "mz_value.{} = {v}; ", ident(&f.name));
        }
        if self.has_always(name) {
            let at = self.site(e);
            let _ = write!(body, "mz_value.mz_check({at}); ");
        }
        format!("({{ {body}mz_value }})")
    }

    /// `p.x = value` as one Rust statement, and the `always` check of the record it changed.
    pub(super) fn field_assign(
        &mut self,
        name: &str,
        name_span: crate::diagnostic::Span,
        field: &str,
        value: &Expr,
    ) -> String {
        let rt = self.types.get(name).copied().unwrap_or(Ty::Error);
        let want = match rt {
            Ty::Record(r) => self.field_of(r, field),
            _ => Ty::Error,
        };
        let v = self.expr_want(value, want);
        let mut line = format!("{}.{} = {v};", ident(name), ident(field));
        if let Ty::Record(r) = rt
            && self.has_always(r)
        {
            // The trap names the assignment, `p.x = 3.0`, at the line it is on.
            self.sites.push((
                name_span.start_line,
                name_span.start_col,
                format!("{name}.{field} = {}", canonical(value)),
            ));
            let at = format!("&MZ_AT_{}", self.sites.len());
            let _ = write!(line, " {}.mz_check({at});", ident(name));
        }
        line
    }
}
