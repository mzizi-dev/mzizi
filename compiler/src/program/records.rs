//! Records in a program (RFC-0013 §11, tracker row C8): their declarations and the checks
//! on them, construction by field name, `with`, reading and assigning fields, methods, and
//! the `always` invariants a record's values hold.
//!
//! What a record is, and what a value of one has (§11.1, §11.2, §11.4):
//!
//! - **A declaration** is `record name` … `end`, holding `field name: type` lines, `fn`
//!   methods, and a `contract` of `always` clauses. [`check_records`] checks the names, the
//!   fields, a record that holds itself (`MZ0919`, as a box is not built), and then every
//!   method body and every clause.
//! - **A literal** `point(x = 1.0, y = 2.0)` gives every field by name, once, in declaration
//!   order, with a value of its type: every way it does not is `MZ0808`.
//! - **`with`** copies a record with some fields replaced, and is `MZ0974` on the same kinds
//!   of mistake, and on a value that is not a record.
//! - **A method** is a `fn` whose receiver is `self`, written and never declared. A method
//!   cannot change its receiver: `self` is bound as a parameter (so `self = …` is `MZ0922`),
//!   and a field of it assigned is `MZ0971`. `changes self` is not built (`MZ0919`, from the
//!   parser).
//! - **An `always` clause** holds for every value. It is checked after each construction and
//!   each `with`, and after each field assignment, which the lowering does; the checker
//!   folds a clause that a literal breaks (`MZ0975`). A clause reads the fields, and may not
//!   call a `fn`, a method, `try`, or build a record (`MZ0975`).

use std::collections::BTreeMap;

use super::{EnumDecl, FnCheck, FnDecl, Kind, Param, TypeRef, reserved_name};
use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::expr::{BinOp, Expr, ExprKind, FieldInit, Ty, UnOp, canonical, intern};
use crate::resolve::nearest;

/// `record name` … `end`: its fields in declaration order, its methods, and its `always`
/// clauses (RFC-0013 §11).
#[derive(Clone, Debug, PartialEq)]
pub struct RecordDecl {
    /// The record's name.
    pub name: String,
    /// Where the name is.
    pub name_span: Span,
    /// Its fields, in declaration order.
    pub fields: Vec<RecordField>,
    /// Its methods, in declaration order. Each has `self` as its receiver, never a parameter.
    pub methods: Vec<FnDecl>,
    /// Its `always` clauses, in declaration order (§11.4).
    pub always: Vec<Invariant>,
}

/// `field name: type` (RFC-0013 §11.1).
#[derive(Clone, Debug, PartialEq)]
pub struct RecordField {
    /// The field's name.
    pub name: String,
    /// Where the name is.
    pub span: Span,
    /// Its type.
    pub ty: TypeRef,
}

/// One `always` clause (RFC-0013 §11.4): a `bool` over the record's fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Invariant {
    /// The condition.
    pub expr: Expr,
    /// The clause's line.
    pub span: Span,
}

impl RecordDecl {
    /// The field called `name`, if the record has one.
    pub fn field(&self, name: &str) -> Option<&RecordField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// The method called `name`, if the record has one.
    pub fn method(&self, name: &str) -> Option<&FnDecl> {
        self.methods.iter().find(|m| m.name == name)
    }

    /// The record's type.
    pub fn ty(&self) -> Ty {
        Ty::Record(intern(&self.name))
    }
}

/// The record called `name`.
fn decl<'a>(records: &'a [RecordDecl], name: &str) -> Option<&'a RecordDecl> {
    records.iter().find(|r| r.name == name)
}

/// The declaration checks, and the checks of every method body and `always` clause, in
/// the order the program's records are declared.
pub fn check_records(
    records: &[RecordDecl],
    enums: &[EnumDecl],
    fns: &BTreeMap<&str, &FnDecl>,
    file: &str,
    src: &str,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for r in records {
        if let Some(why) = reserved_name(&r.name) {
            diags.push(Diagnostic::error(
                "MZ0921",
                file,
                r.name_span,
                format!("`record {}` cannot be named so: {why}", r.name),
            ));
        } else if Ty::from_name(&r.name).is_some()
            || Ty::CONSTRUCTORS.contains(&r.name.as_str())
            || super::BUILT_IN_NAMES.contains(&r.name.as_str())
        {
            diags.push(Diagnostic::error(
                "MZ0921",
                file,
                r.name_span,
                format!(
                    "`record {}` reuses the name of a built-in type — choose another name",
                    r.name
                ),
            ));
        } else if seen.contains(&r.name.as_str())
            || enums.iter().any(|e| e.name == r.name)
            || fns.contains_key(r.name.as_str())
        {
            diags.push(Diagnostic::error(
                "MZ0904",
                file,
                r.name_span,
                format!(
                    "`record {}` is declared twice, or shares its name with an enum or a function — Mzizi has one name per declaration; rename one",
                    r.name
                ),
            ));
        }
        seen.push(&r.name);
        let mut field_names: Vec<&str> = Vec::new();
        for f in &r.fields {
            if let Some(why) = reserved_name(&f.name) {
                diags.push(Diagnostic::error(
                    "MZ0921",
                    file,
                    f.span,
                    format!("field `{}` cannot be named so: {why}", f.name),
                ));
            } else if field_names.contains(&f.name.as_str()) {
                diags.push(Diagnostic::error(
                    "MZ0904",
                    file,
                    f.span,
                    format!(
                        "field `{}` of `record {}` is declared twice — rename one",
                        f.name, r.name
                    ),
                ));
            }
            field_names.push(&f.name);
            if f.ty.ty.as_result().is_some() {
                diags.push(Diagnostic::error(
                    "MZ0950",
                    file,
                    f.ty.span,
                    format!(
                        "field `{}` is a {} — a result is matched or propagated where it is made, and a record does not hold one",
                        f.name,
                        f.ty.ty.name()
                    ),
                ));
            }
        }
        let mut method_names: Vec<&str> = Vec::new();
        for m in &r.methods {
            if let Some(why) = reserved_name(&m.name) {
                diags.push(Diagnostic::error(
                    "MZ0921",
                    file,
                    m.name_span,
                    format!("method `{}` cannot be named so: {why}", m.name),
                ));
            } else if method_names.contains(&m.name.as_str()) {
                diags.push(Diagnostic::error(
                    "MZ0904",
                    file,
                    m.name_span,
                    format!(
                        "method `{}` of `record {}` is declared twice — Mzizi has no overloading; rename one",
                        m.name, r.name
                    ),
                ));
            } else if field_names.contains(&m.name.as_str()) {
                diags.push(Diagnostic::error(
                    "MZ0970",
                    file,
                    m.name_span,
                    format!(
                        "`{}` is a field of `record {}`, so a method cannot have that name — `{}.{}` would be ambiguous",
                        m.name, r.name, r.name, m.name
                    ),
                ));
            }
            method_names.push(&m.name);
        }
    }
    for r in records {
        if holds_itself(records, &r.name) {
            diags.push(Diagnostic::error(
                "MZ0919",
                file,
                r.name_span,
                format!(
                    "`record {}` holds itself, directly or through an option — a value that holds itself needs a box, which is designed (RFC-0013 §11.5) but not built yet; hold it in a `list` instead",
                    r.name
                ),
            ));
        }
    }
    for r in records {
        for m in &r.methods {
            let mut cx = FnCheck::new(m, fns, enums, records, file, src);
            cx.receiver = Some(intern(&r.name));
            cx.run();
            diags.append(&mut cx.diags);
        }
        diags.extend(check_invariants(r, fns, enums, records, file, src));
    }
    diags
}

/// The record a field of type `t` holds by value, if it is one: a record itself, or an
/// option of one (an option is inline in Rust, so it is not a box).
fn holds_directly(t: Ty) -> Option<&'static str> {
    match t {
        Ty::Record(n) => Some(n),
        Ty::Option(inner) => holds_directly(*inner),
        _ => None,
    }
}

/// Whether the record `start` holds itself, through its fields' records by value.
fn holds_itself(records: &[RecordDecl], start: &str) -> bool {
    let mut stack = vec![start];
    let mut seen: Vec<&str> = Vec::new();
    while let Some(name) = stack.pop() {
        let Some(r) = decl(records, name) else {
            continue;
        };
        for f in &r.fields {
            if let Some(next) = holds_directly(f.ty.ty) {
                if next == start {
                    return true;
                }
                if !seen.contains(&next) {
                    seen.push(next);
                    stack.push(next);
                }
            }
        }
    }
    false
}

/// The `always` clauses of `r`, checked as a function over its fields, each bound as a
/// parameter: a `bool` (`MZ0712`), reading nothing else (`MZ0707`), and calling nothing
/// (`MZ0975`).
fn check_invariants(
    r: &RecordDecl,
    fns: &BTreeMap<&str, &FnDecl>,
    enums: &[EnumDecl],
    records: &[RecordDecl],
    file: &str,
    src: &str,
) -> Vec<Diagnostic> {
    if r.always.is_empty() {
        return Vec::new();
    }
    let shape = FnDecl {
        name: r.name.clone(),
        name_span: r.name_span,
        params: r
            .fields
            .iter()
            .map(|f| Param {
                name: f.name.clone(),
                span: f.span,
                ty: f.ty,
            })
            .collect(),
        ret: None,
        body: Vec::new(),
        end_span: r.name_span,
        skipped: false,
    };
    let mut cx = FnCheck::new(&shape, fns, enums, records, file, src);
    cx.in_always = true;
    for p in &shape.params {
        cx.bind(&p.name, Kind::Param, p.ty.ty, None);
    }
    for inv in &r.always {
        let t = cx.expr(&inv.expr);
        cx.condition(&inv.expr, t, "always");
    }
    cx.diags
}

/// A value a literal's field can hold, when it is written as a literal: what
/// [`const_of`] folds an `always` clause to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Const {
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// The value of `e` when it is a literal, or a name bound in `env`, and an operator over
/// those that cannot trap. `None` otherwise: an unknown value is never folded.
fn const_of(e: &Expr, env: &[(&str, Const)]) -> Option<Const> {
    match &e.kind {
        ExprKind::Int(v) => Some(Const::Int(*v)),
        ExprKind::Float(v) => Some(Const::Float(*v)),
        ExprKind::Bool(b) => Some(Const::Bool(*b)),
        ExprKind::Name(n) => env.iter().find(|(k, _)| k == n).map(|(_, v)| *v),
        ExprKind::Unary {
            op: UnOp::Not,
            operand,
        } => match const_of(operand, env)? {
            Const::Bool(b) => Some(Const::Bool(!b)),
            _ => None,
        },
        ExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } => match const_of(operand, env)? {
            Const::Int(v) => v.checked_neg().map(Const::Int),
            Const::Float(v) => Some(Const::Float(-v)),
            Const::Bool(_) => None,
        },
        ExprKind::Binary { op, lhs, rhs, .. } => {
            let a = const_of(lhs, env)?;
            let b = const_of(rhs, env)?;
            match (op, a, b) {
                (BinOp::And, Const::Bool(x), Const::Bool(y)) => Some(Const::Bool(x && y)),
                (BinOp::Or, Const::Bool(x), Const::Bool(y)) => Some(Const::Bool(x || y)),
                (BinOp::Is, x, y) if std::mem::discriminant(&x) == std::mem::discriminant(&y) => {
                    Some(Const::Bool(x == y))
                }
                (BinOp::IsNot, x, y)
                    if std::mem::discriminant(&x) == std::mem::discriminant(&y) =>
                {
                    Some(Const::Bool(x != y))
                }
                (BinOp::Lt, Const::Int(x), Const::Int(y)) => Some(Const::Bool(x < y)),
                (BinOp::Le, Const::Int(x), Const::Int(y)) => Some(Const::Bool(x <= y)),
                (BinOp::Gt, Const::Int(x), Const::Int(y)) => Some(Const::Bool(x > y)),
                (BinOp::Ge, Const::Int(x), Const::Int(y)) => Some(Const::Bool(x >= y)),
                (BinOp::Lt, Const::Float(x), Const::Float(y)) => Some(Const::Bool(x < y)),
                (BinOp::Le, Const::Float(x), Const::Float(y)) => Some(Const::Bool(x <= y)),
                (BinOp::Gt, Const::Float(x), Const::Float(y)) => Some(Const::Bool(x > y)),
                (BinOp::Ge, Const::Float(x), Const::Float(y)) => Some(Const::Bool(x >= y)),
                (BinOp::Add, Const::Int(x), Const::Int(y)) => x.checked_add(y).map(Const::Int),
                (BinOp::Sub, Const::Int(x), Const::Int(y)) => x.checked_sub(y).map(Const::Int),
                (BinOp::Mul, Const::Int(x), Const::Int(y)) => x.checked_mul(y).map(Const::Int),
                (BinOp::Add, Const::Float(x), Const::Float(y)) => Some(Const::Float(x + y)),
                (BinOp::Sub, Const::Float(x), Const::Float(y)) => Some(Const::Float(x - y)),
                (BinOp::Mul, Const::Float(x), Const::Float(y)) => Some(Const::Float(x * y)),
                _ => None,
            }
        }
        _ => None,
    }
}

impl FnCheck<'_> {
    /// `point(x = 1.0, y = 2.0)` (RFC-0013 §11.1): every field, by name, once, in declaration
    /// order, with a value of its type. Each way it is wrong is one `MZ0808`.
    pub(super) fn record_literal(&mut self, e: &Expr, name: &str, fields: &[FieldInit]) -> Ty {
        let records = self.records;
        let Some(d) = decl(records, name) else {
            for f in fields {
                self.expr(&f.value);
            }
            return Ty::Error;
        };
        if self.in_always {
            self.err(
                "MZ0975",
                e.span,
                format!(
                    "`{}` builds a record, which an `always` clause may not do — a clause reads the fields of a value and nothing else",
                    canonical(e)
                ),
            );
            return d.ty();
        }
        let mut value_types = Vec::with_capacity(fields.len());
        for f in fields {
            let want = d.field(&f.name).map_or(Ty::Error, |rf| rf.ty.ty);
            let t = self.expr_want(&f.value, want);
            let t = if self.unhandled(&f.value, t, || "a record stores it unexamined".into()) {
                Ty::Error
            } else {
                t
            };
            value_types.push(t);
        }
        let mut ok = true;
        let positional = fields.iter().find(|f| f.name.is_empty());
        if let Some(f) = positional {
            ok = false;
            self.err(
                "MZ0808",
                f.span,
                format!(
                    "`{name}` takes its fields by name, `{name}(x = …)`, and this value has no name — a record's fields are never positional"
                ),
            );
        }
        let mut given: Vec<usize> = Vec::new();
        // The fields a misspelt name is guessed to mean: each is reported once, as the
        // misspelling, not again as left out.
        let mut guessed: Vec<String> = Vec::new();
        for (f, t) in fields.iter().zip(&value_types) {
            if f.name.is_empty() {
                continue;
            }
            let Some(index) = d.fields.iter().position(|rf| rf.name == f.name) else {
                ok = false;
                let names: Vec<&str> = d.fields.iter().map(|rf| rf.name.as_str()).collect();
                let say = format!("`{name}` has no field `{}`", f.name);
                if let Some((near, _)) = nearest(&f.name, names.iter().copied()) {
                    guessed.push(near);
                }
                self.did_you_mean("MZ0808", f.span, say, &f.name, names);
                continue;
            };
            if given.contains(&index) {
                ok = false;
                self.err(
                    "MZ0808",
                    f.span,
                    format!(
                        "field `{}` of `{name}` is given twice — give it once",
                        f.name
                    ),
                );
                continue;
            }
            let want = d.fields[index].ty.ty;
            if !want.has_error() && !t.has_error() && *t != Ty::Error && *t != want {
                ok = false;
                self.err(
                    "MZ0808",
                    f.value.span,
                    format!(
                        "field `{}` of `{name}` is {}, and `{}` is {}",
                        f.name,
                        want.name(),
                        canonical(&f.value),
                        t.name()
                    ),
                );
            }
            if given.last().is_some_and(|&last| index < last) && ok {
                ok = false;
                self.err(
                    "MZ0808",
                    f.span,
                    format!(
                        "the fields of `{name}` are given in declaration order, and `{}` comes before the one written before it",
                        f.name
                    ),
                );
            }
            given.push(index);
        }
        if positional.is_none() {
            for rf in &d.fields {
                if !fields.iter().any(|f| f.name == rf.name) && !guessed.contains(&rf.name) {
                    ok = false;
                    self.err(
                        "MZ0808",
                        e.span,
                        format!(
                            "`{name}` needs field `{}`, and this literal leaves it out — every field is given by name, and there are no zero values",
                            rf.name
                        ),
                    );
                }
            }
        }
        if ok {
            self.constant_invariant(e, d, fields);
        }
        d.ty()
    }

    /// `MZ0975` at check time: a literal whose fields are all literals, and that breaks an
    /// `always` clause, traps every time it is built.
    fn constant_invariant(&mut self, e: &Expr, d: &RecordDecl, fields: &[FieldInit]) {
        if d.always.is_empty() || fields.len() != d.fields.len() {
            return;
        }
        let env: Vec<(&str, Const)> = fields
            .iter()
            .filter_map(|f| const_of(&f.value, &[]).map(|c| (f.name.as_str(), c)))
            .collect();
        if env.len() != fields.len() {
            return;
        }
        if let Some(inv) = d
            .always
            .iter()
            .find(|inv| const_of(&inv.expr, &env) == Some(Const::Bool(false)))
        {
            self.err(
                "MZ0975",
                e.span,
                format!(
                    "`{}` breaks `always {}`, so this line traps every time it runs",
                    canonical(e),
                    canonical(&inv.expr)
                ),
            );
        }
    }

    /// `base with (x = 3.0)` (RFC-0013 §11.1): a copy of a record with fields replaced. Its
    /// own mistakes are `MZ0974`.
    pub(super) fn with_expr(&mut self, e: &Expr, base: &Expr, fields: &[FieldInit]) -> Ty {
        let bt = self.expr(base);
        if self.in_always {
            self.err(
                "MZ0975",
                e.span,
                format!(
                    "`{}` copies a record, which an `always` clause may not do — a clause reads the fields of a value and nothing else",
                    canonical(e)
                ),
            );
            return bt;
        }
        let Ty::Record(name) = bt else {
            for f in fields {
                self.expr(&f.value);
            }
            if bt != Ty::Error && !bt.has_error() {
                self.err(
                    "MZ0974",
                    base.span,
                    format!(
                        "`with` copies a record, and `{}` is {}, which is not one",
                        canonical(base),
                        bt.name()
                    ),
                );
            }
            return Ty::Error;
        };
        let records = self.records;
        let Some(d) = decl(records, name) else {
            return Ty::Error;
        };
        if fields.is_empty() {
            self.err(
                "MZ0974",
                e.span,
                format!(
                    "`with ()` replaces no field — name each field it replaces, `{} with (x = …)`",
                    canonical(base)
                ),
            );
            return bt;
        }
        let mut given: Vec<usize> = Vec::new();
        for f in fields {
            let want = if f.name.is_empty() {
                Ty::Error
            } else {
                d.field(&f.name).map_or(Ty::Error, |rf| rf.ty.ty)
            };
            let t = self.expr_want(&f.value, want);
            let t = if self.unhandled(&f.value, t, || "a `with` stores it unexamined".into()) {
                Ty::Error
            } else {
                t
            };
            if f.name.is_empty() {
                self.err(
                    "MZ0974",
                    f.span,
                    format!(
                        "`with` names each field it replaces, `{} with (x = …)`, and this value has no name",
                        canonical(base)
                    ),
                );
                continue;
            }
            let Some(index) = d.fields.iter().position(|rf| rf.name == f.name) else {
                let names: Vec<&str> = d.fields.iter().map(|rf| rf.name.as_str()).collect();
                let say = format!("`{name}` has no field `{}`", f.name);
                self.did_you_mean("MZ0974", f.span, say, &f.name, names);
                continue;
            };
            if given.contains(&index) {
                self.err(
                    "MZ0974",
                    f.span,
                    format!("field `{}` is replaced twice — replace it once", f.name),
                );
                continue;
            }
            if given.last().is_some_and(|&last| index < last) {
                self.err(
                    "MZ0974",
                    f.span,
                    format!(
                        "the fields of a `with` are in declaration order, and `{}` comes before the one written before it",
                        f.name
                    ),
                );
            }
            given.push(index);
            if !want.has_error() && !t.has_error() && t != Ty::Error && t != want {
                self.err(
                    "MZ0974",
                    f.value.span,
                    format!(
                        "field `{}` of `{name}` is {}, and `{}` is {}",
                        f.name,
                        want.name(),
                        canonical(&f.value),
                        t.name()
                    ),
                );
            }
        }
        bt
    }

    /// `p.x`: a field of a record. A method named without its parentheses is read here too,
    /// and is `MZ0708`.
    pub(super) fn record_field(&mut self, base: &Expr, r: &str, name: &str, name_span: Span) -> Ty {
        let records = self.records;
        let Some(d) = decl(records, r) else {
            return Ty::Error;
        };
        if let Some(f) = d.field(name) {
            return f.ty.ty;
        }
        if d.method(name).is_some() {
            self.err(
                "MZ0708",
                name_span,
                format!(
                    "`{}.{name}` is a method, and a method is called with parentheses: `{}.{name}()`",
                    canonical(base),
                    canonical(base)
                ),
            );
            return Ty::Error;
        }
        let say = format!("`{}` is {r}, which has no field `{name}`", canonical(base));
        let names: Vec<&str> = d.fields.iter().map(|f| f.name.as_str()).collect();
        self.did_you_mean("MZ0708", name_span, say, name, names);
        Ty::Error
    }

    /// `p.norm(…)`: a method of a record (RFC-0013 §11.2), with its arguments typed as its
    /// parameters are. `types` are the arguments' types, already read.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_method(
        &mut self,
        e: &Expr,
        recv: &Expr,
        r: &str,
        name: &str,
        name_span: Span,
        args: &[Expr],
        types: &[Ty],
        called: bool,
    ) -> Ty {
        let records = self.records;
        let Some(d) = decl(records, r) else {
            return Ty::Error;
        };
        if self.in_always {
            self.err(
                "MZ0975",
                e.span,
                format!(
                    "`{}` calls a method, which an `always` clause may not do — a clause reads the fields of a value and nothing else",
                    canonical(e)
                ),
            );
            return Ty::Error;
        }
        let Some(m) = d.method(name) else {
            if d.field(name).is_some() {
                self.err(
                    "MZ0708",
                    name_span,
                    format!(
                        "`{name}` is a field of {r}, and a field is read without parentheses: `{}.{name}`",
                        canonical(recv)
                    ),
                );
            } else {
                let say = format!("`{}` is {r}, which has no method `{name}`", canonical(recv));
                let names: Vec<&str> = d.methods.iter().map(|m| m.name.as_str()).collect();
                self.did_you_mean("MZ0708", name_span, say, name, names);
            }
            return Ty::Error;
        };
        if !called {
            self.err(
                "MZ0708",
                name_span,
                format!(
                    "`{}.{name}` is a method, and a method is called with parentheses: `{}.{name}()`",
                    canonical(recv),
                    canonical(recv)
                ),
            );
            return Ty::Error;
        }
        let ret = m.ret.map_or(Ty::Nothing, |r| r.ty);
        if args.len() != m.params.len() {
            self.err(
                "MZ0905",
                e.span,
                format!(
                    "`.{name}` takes {} {}, and this call gives {}",
                    m.params.len(),
                    if m.params.len() == 1 {
                        "value"
                    } else {
                        "values"
                    },
                    args.len()
                ),
            );
            return ret;
        }
        for ((a, t), p) in args.iter().zip(types).zip(&m.params) {
            if *t == Ty::Error || t.has_error() || *t == p.ty.ty {
                continue;
            }
            self.err(
                "MZ0905",
                a.span,
                format!(
                    "`{}` is {}, and `.{name}` takes {} there",
                    canonical(a),
                    t.name(),
                    p.ty.ty.name()
                ),
            );
            return ret;
        }
        ret
    }

    /// `p.x = value` (RFC-0013 §11.1): a field of a `var` record. Assigning to `self` is
    /// `MZ0971`, which a method is told to return a changed copy instead.
    pub(super) fn field_assign(&mut self, name: &str, name_span: Span, field: &str, value: &Expr) {
        if name == "self" {
            if self.receiver.is_some() {
                self.err(
                    "MZ0971",
                    name_span,
                    "a method cannot assign to a field of `self`, which it only reads — return a changed copy with `with`; a method that changes `self` (`changes self`) is designed but not built (RFC-0013 §11.2)",
                );
            } else {
                self.err(
                    "MZ0970",
                    name_span,
                    "`self` is the receiver of a method, and only a method has one — a record's `fn` names it",
                );
            }
            self.expr(value);
            return;
        }
        let Some(i) = self.visible(name) else {
            self.read(name, name_span);
            self.expr(value);
            return;
        };
        let t = self.bindings[i].ty;
        self.mutated(
            &Expr {
                kind: ExprKind::Name(name.to_string()),
                span: name_span,
            },
            "a field assignment",
        );
        let Ty::Record(r) = t else {
            self.expr(value);
            if t != Ty::Error && !t.has_error() {
                self.err(
                    "MZ0708",
                    name_span,
                    format!("`{name}` is {}, which has no field `{field}`", t.name()),
                );
            }
            return;
        };
        let records = self.records;
        let Some(d) = decl(records, r) else {
            self.expr(value);
            return;
        };
        let Some(rf) = d.field(field) else {
            self.expr(value);
            self.err(
                "MZ0708",
                name_span,
                format!("`{name}` is {r}, which has no field `{field}`"),
            );
            return;
        };
        let want = rf.ty.ty;
        let vt = self.expr_want(value, want);
        if self.unhandled(value, vt, || "an assignment stores it unexamined".into()) {
            return;
        }
        if !want.has_error() && !vt.has_error() && vt != want {
            self.err(
                "MZ0711",
                value.span,
                format!(
                    "`{name}.{field}` is {}, and `{}` is {}",
                    want.name(),
                    canonical(value),
                    vt.name()
                ),
            );
        }
    }
}

impl FnCheck<'_> {
    /// A name the record does not have: `say`, with the nearest of `names` as a guess when one
    /// is near (`MZ0708`, `MZ0808`, `MZ0974`).
    fn did_you_mean(
        &mut self,
        code: &'static str,
        span: Span,
        say: String,
        name: &str,
        names: Vec<&str>,
    ) {
        match nearest(name, names) {
            Some((near, _)) => self.err_fix(
                code,
                span,
                format!("{say} — did you mean `{near}`?"),
                span,
                near,
                Confidence::Guess,
            ),
            None => self.err(code, span, say),
        }
    }
}
