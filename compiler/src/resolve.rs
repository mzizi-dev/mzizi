//! Name and type resolution: the pass that lets `mz check` say no (RFC-0008 §5).
//!
//! Before this module, `prop x: flarp`, `text = lable` and `class = "{x.nope}"` all
//! compiled with zero errors (RFC-0008 TY-1). A misspelling that costs a Rust author one
//! compile cycle cost a Mzizi author nothing until it surfaced as a behavioural defect —
//! which flatters the iterations-to-clean-compile metric and hides the cost in the defect
//! metric (TY-2).
//!
//! Three rules shape every diagnostic here:
//!
//! - **One diagnostic per real error** (RFC-0001 §4.1). A name that failed to resolve
//!   becomes [`Ty::Unknown`], which every later check accepts silently, so an unknown type
//!   is reported at its declaration and never again at its uses.
//! - **Resolution order is fixed** (RFC-0008 §5): loop binding, prop, `fn`, then a
//!   variant of exactly one local enum. Adding a declaration can create a diagnostic but
//!   can never silently change what an existing line means.
//! - **Nearest-name fixes are drawn only from candidates of the right kind**, and are
//!   `exact` only when one candidate is unambiguously nearest (RFC-0008 §5.2).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ast::{Attr, Component, Element, EnumDecl, PropDecl, TypeExpr, TypeKind};
use crate::diagnostic::{Confidence, Diagnostic, Severity, Span};

/// A resolved type (RFC-0008 §1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    /// `bool`.
    Bool,
    /// `int`.
    Int,
    /// `text`.
    Text,
    /// A declared enum.
    Enum(String),
    /// A declared record.
    Record(String),
    /// `list(T)`.
    List(Box<Ty>),
    /// `option(T)`.
    Option(Box<Ty>),
    /// `event(T)`, or `event(none)` when the payload is `None`.
    Event(Option<Box<Ty>>),
    /// A `fn` of this component, as an event-wiring target.
    Fn,
    /// Something that already produced a diagnostic. Accepted everywhere, so an error is
    /// reported once rather than at every use.
    Unknown,
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Bool => f.write_str("bool"),
            Ty::Int => f.write_str("int"),
            Ty::Text => f.write_str("text"),
            Ty::Enum(name) | Ty::Record(name) => f.write_str(name),
            Ty::List(t) => write!(f, "list({t})"),
            Ty::Option(t) => write!(f, "option({t})"),
            Ty::Event(None) => f.write_str("event(none)"),
            Ty::Event(Some(t)) => write!(f, "event({t})"),
            Ty::Fn => f.write_str("a fn"),
            Ty::Unknown => f.write_str("unknown"),
        }
    }
}

impl Ty {
    /// A value that can be rendered as text, compared, or used as a key.
    fn is_scalar(&self) -> bool {
        matches!(
            self,
            Ty::Bool | Ty::Int | Ty::Text | Ty::Enum(_) | Ty::Unknown
        )
    }
}

const BUILTINS: &[&str] = &["bool", "int", "text"];
const CONSTRUCTORS: &[&str] = &["list", "option", "event"];

/// Type names other languages use for Mzizi's built-ins. Each maps to exactly one Mzizi
/// type, so the fix is `exact`: there is nothing else it could have meant.
const ALIASES: &[(&str, &str)] = &[
    ("string", "text"),
    ("str", "text"),
    ("boolean", "bool"),
    ("number", "int"),
    ("integer", "int"),
    ("i8", "int"),
    ("i16", "int"),
    ("i32", "int"),
    ("i64", "int"),
    ("u8", "int"),
    ("u16", "int"),
    ("u32", "int"),
    ("u64", "int"),
    ("usize", "int"),
    ("isize", "int"),
];

/// Where a type is written, which decides what it may be.
#[derive(Clone, Copy, PartialEq)]
enum TyCtx {
    Prop,
    Field,
    Payload,
    Element,
    OptionArg,
}

/// What a value is for, which decides which types it may have.
#[derive(Clone, Copy, PartialEq)]
enum Use {
    /// An ordinary attribute: a scalar, an event prop, or a `fn`.
    Attr,
    /// `tap = …` / `change = …`: an event prop or a `fn`.
    Wire,
    /// Inside `{...}`: a scalar.
    Interp,
    /// A `for each` key: a scalar.
    Key,
}

#[derive(Clone, Default)]
struct Scope {
    /// Loop bindings, innermost last.
    bindings: Vec<(String, Ty)>,
    /// Paths narrowed from `option(T)` to `T` by an enclosing `when … is none … else`.
    narrowed: Vec<String>,
}

/// Resolve every name and type in `component`, returning one diagnostic per problem.
pub fn resolve(component: &Component, file: &str) -> Vec<Diagnostic> {
    let mut r = Resolver {
        c: component,
        file: file.to_string(),
        diags: Vec::new(),
        records: BTreeMap::new(),
        columns: BTreeMap::new(),
        props: BTreeMap::new(),
        all_bindings: BTreeSet::new(),
    };
    fn collect(els: &[Element], out: &mut BTreeSet<String>) {
        for el in els {
            if el.tag == "for"
                && let Some(each) = el.attrs.first().filter(|a| a.name == "each")
            {
                out.insert(each.value.clone());
            }
            collect(&el.children, out);
            if let Some(e) = &el.else_children {
                collect(e, out);
            }
        }
    }
    if let Some(view) = &component.view {
        collect(view, &mut r.all_bindings);
    }
    r.declarations();
    r.record_fields();
    r.record_cycles();
    r.enum_columns();
    r.prop_types();
    if let Some(view) = &component.view {
        let scope = Scope::default();
        for el in view {
            r.element(el, &scope);
        }
    }
    r.emits();
    r.diags
}

struct Resolver<'a> {
    c: &'a Component,
    file: String,
    diags: Vec<Diagnostic>,
    /// Record name → fields, in declaration order.
    records: BTreeMap<String, Vec<(String, Ty)>>,
    /// `(enum, column)` → the column's type.
    columns: BTreeMap<(String, String), Ty>,
    /// Prop name → type.
    props: BTreeMap<String, Ty>,
    /// Every `for each` binding anywhere in the view, so a binding used outside its block
    /// is reported as that, not mistaken for another component's enum.
    all_bindings: BTreeSet<String>,
}

impl<'a> Resolver<'a> {
    fn err(&mut self, code: &'static str, span: Span, say: String) {
        self.diags
            .push(Diagnostic::error(code, &self.file, span, say));
    }

    fn err_fix(
        &mut self,
        code: &'static str,
        span: Span,
        say: String,
        fix: (Span, String, Confidence),
    ) {
        self.diags
            .push(Diagnostic::error(code, &self.file, span, say).with_fix(fix.0, fix.1, fix.2));
    }

    fn find_enum(&self, name: &str) -> Option<&'a EnumDecl> {
        self.c.enums.iter().find(|e| e.name == name)
    }

    fn is_type_name(&self, name: &str) -> bool {
        BUILTINS.contains(&name)
            || self.find_enum(name).is_some()
            || self.c.records.iter().any(|r| r.name == name)
    }

    // -----------------------------------------------------------------------------------
    // Declarations
    // -----------------------------------------------------------------------------------

    /// Duplicate enums, records and props, and types named like a built-in.
    fn declarations(&mut self) {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let decls: Vec<(String, Span, &str)> = self
            .c
            .enums
            .iter()
            .map(|e| {
                let span = e
                    .variants
                    .first()
                    .map(|v| v.span)
                    .unwrap_or(self.c.name_span);
                (e.name.clone(), span, "enum")
            })
            .chain(
                self.c
                    .records
                    .iter()
                    .map(|r| (r.name.clone(), r.span, "record")),
            )
            .collect();
        for (name, span, kind) in decls {
            if BUILTINS.contains(&name.as_str()) || CONSTRUCTORS.contains(&name.as_str()) {
                self.err(
                    "MZ0704",
                    span,
                    format!("`{kind} {name}` reuses a built-in type's name — pick another"),
                );
            } else if !seen.insert(name.clone()) {
                self.err(
                    "MZ0704",
                    span,
                    format!("a type named `{name}` is already declared in this file"),
                );
            }
        }
        let mut props = BTreeSet::new();
        for p in &self.c.props {
            if !props.insert(p.name.clone()) {
                self.err(
                    "MZ0704",
                    p.span,
                    format!("`prop {}` is declared twice — delete one", p.name),
                );
            }
        }
    }

    fn record_fields(&mut self) {
        for r in &self.c.records {
            let mut fields = Vec::new();
            let mut names = BTreeSet::new();
            for f in &r.fields {
                if !names.insert(f.name.clone()) {
                    self.err(
                        "MZ0704",
                        f.span,
                        format!("`record {}` already has a field `{}`", r.name, f.name),
                    );
                    continue;
                }
                let ty = self.ty_of(&f.ty, TyCtx::Field);
                fields.push((f.name.clone(), ty));
            }
            for name in &r.broken {
                if names.insert(name.clone()) {
                    fields.push((name.clone(), Ty::Unknown));
                }
            }
            self.records.entry(r.name.clone()).or_insert(fields);
        }
    }

    /// A record that contains itself has no finite serialised form (RFC-0008 §1).
    fn record_cycles(&mut self) {
        fn mentions(ty: &Ty, out: &mut Vec<String>) {
            match ty {
                Ty::Record(name) => out.push(name.clone()),
                Ty::List(t) | Ty::Option(t) => mentions(t, out),
                _ => {}
            }
        }
        let graph: BTreeMap<String, Vec<String>> = self
            .records
            .iter()
            .map(|(name, fields)| {
                let mut out = Vec::new();
                for (_, ty) in fields {
                    mentions(ty, &mut out);
                }
                (name.clone(), out)
            })
            .collect();
        for r in &self.c.records {
            // Is `r` reachable from itself?
            let mut stack: Vec<String> = graph.get(&r.name).cloned().unwrap_or_default();
            let mut seen = BTreeSet::new();
            let mut cyclic = false;
            while let Some(next) = stack.pop() {
                if next == r.name {
                    cyclic = true;
                    break;
                }
                if seen.insert(next.clone())
                    && let Some(more) = graph.get(&next)
                {
                    stack.extend(more.iter().cloned());
                }
            }
            if cyclic {
                self.err(
                    "MZ0705",
                    r.span,
                    format!(
                        "`record {}` contains itself, so it has no finite serialised form — recursive records are not supported yet",
                        r.name
                    ),
                );
            }
        }
    }

    /// A column's type is its first variant's value's type; every other variant must agree.
    fn enum_columns(&mut self) {
        for e in &self.c.enums {
            let Some(first) = e.variants.first() else {
                continue;
            };
            for (col, value) in &first.columns {
                let ty = self.column_value_ty(e, &first.name, col, value, first.span);
                for v in e.variants.iter().skip(1) {
                    let Some((_, other)) = v.columns.iter().find(|(c, _)| c == col) else {
                        continue; // MZ0303 already reported it.
                    };
                    let other_ty = self.column_value_ty(e, &v.name, col, other, v.span);
                    if ty != Ty::Unknown && other_ty != Ty::Unknown && other_ty != ty {
                        self.err(
                            "MZ0716",
                            v.span,
                            format!(
                                "`{}.{col}` is {other_ty} but `{}.{col}` is {ty} — a column has one type",
                                v.name, first.name
                            ),
                        );
                    }
                }
                self.columns.insert((e.name.clone(), col.clone()), ty);
            }
        }
    }

    fn column_value_ty(
        &mut self,
        e: &EnumDecl,
        variant: &str,
        col: &str,
        value: &str,
        span: Span,
    ) -> Ty {
        if value.starts_with('"') {
            return Ty::Text;
        }
        if value.parse::<i64>().is_ok() {
            return Ty::Int;
        }
        if value == "true" || value == "false" {
            return Ty::Bool;
        }
        // A bare word is a variant of another local enum: `accent tanzanite`. That is
        // what lets `{n.accent.class}` resolve with columns staying static data.
        let owners: Vec<&EnumDecl> = self
            .c
            .enums
            .iter()
            .filter(|o| o.variants.iter().any(|v| v.name == value))
            .collect();
        match owners.as_slice() {
            [one] => Ty::Enum(one.name.clone()),
            [] => {
                self.err(
                    "MZ0716",
                    span,
                    format!(
                        "`{variant} {col} {value}` — `{value}` is not a string, a number, or a variant of any enum in this file"
                    ),
                );
                let _ = e;
                Ty::Unknown
            }
            many => {
                let names: Vec<&str> = many.iter().map(|o| o.name.as_str()).collect();
                self.err(
                    "MZ0716",
                    span,
                    format!(
                        "`{variant} {col} {value}` — `{value}` is a variant of {}, so the column's type is ambiguous",
                        names.join(" and ")
                    ),
                );
                Ty::Unknown
            }
        }
    }

    // -----------------------------------------------------------------------------------
    // Types
    // -----------------------------------------------------------------------------------

    fn ty_of(&mut self, t: &TypeExpr, ctx: TyCtx) -> Ty {
        match &t.kind {
            TypeKind::Name(name) => self.named_ty(name, t.span, ctx),
            TypeKind::Nothing => {
                self.err(
                    "MZ0702",
                    t.span,
                    "`none` is a type only as an event's payload, `event(none)`; absence of a value is `option(<type>)`".to_string(),
                );
                Ty::Unknown
            }
            TypeKind::Apply(ctor, arg) => match ctor.as_str() {
                "list" => {
                    let inner = self.ty_of(arg, TyCtx::Element);
                    Ty::List(Box::new(inner))
                }
                "option" => self.option_ty(t, arg),
                "event" => {
                    if ctx != TyCtx::Prop {
                        self.event_is_not_data(t.span, ctx);
                        return Ty::Unknown;
                    }
                    if arg.kind == TypeKind::Nothing {
                        Ty::Event(None)
                    } else {
                        Ty::Event(Some(Box::new(self.ty_of(arg, TyCtx::Payload))))
                    }
                }
                other if self.is_type_name(other) => {
                    self.err_fix(
                        "MZ0702",
                        t.span,
                        format!("`{other}` takes no `(...)` — only list, option and event do"),
                        (t.span, other.to_string(), Confidence::Guess),
                    );
                    Ty::Unknown
                }
                other => {
                    let fix = nearest(other, CONSTRUCTORS.iter().copied());
                    self.unknown(
                        "MZ0701",
                        t.span,
                        format!(
                            "`{other}(...)` is not a type constructor — Mzizi has list(T), option(T) and event(T)"
                        ),
                        fix.map(|(name, c)| (t.span, format!("{name}({arg})"), c)),
                    );
                    Ty::Unknown
                }
            },
        }
    }

    fn named_ty(&mut self, name: &str, span: Span, ctx: TyCtx) -> Ty {
        match name {
            "bool" => return Ty::Bool,
            "int" => return Ty::Int,
            "text" => return Ty::Text,
            "list" | "option" => {
                self.err(
                    "MZ0702",
                    span,
                    format!("`{name}` needs a type inside it: write `{name}(<type>)`"),
                );
                return Ty::Unknown;
            }
            "event" => {
                if ctx != TyCtx::Prop {
                    self.event_is_not_data(span, ctx);
                } else {
                    self.err_fix(
                        "MZ0702",
                        span,
                        "`event` needs its payload: `event(none)` for a bare signal, `event(<type>)` otherwise".to_string(),
                        (span, "event(none)".to_string(), Confidence::Exact),
                    );
                }
                return Ty::Unknown;
            }
            _ => {}
        }
        if self.find_enum(name).is_some() {
            return Ty::Enum(name.to_string());
        }
        if self.c.records.iter().any(|r| r.name == name) {
            return Ty::Record(name.to_string());
        }
        if let Some((_, to)) = ALIASES.iter().find(|(from, _)| *from == name) {
            self.err_fix(
                "MZ0701",
                span,
                format!("`{name}` is not a Mzizi type — write `{to}`"),
                (span, to.to_string(), Confidence::Exact),
            );
            return Ty::Unknown;
        }
        let candidates: Vec<String> = BUILTINS
            .iter()
            .map(|s| s.to_string())
            .chain(self.c.enums.iter().map(|e| e.name.clone()))
            .chain(self.c.records.iter().map(|r| r.name.clone()))
            .collect();
        let fix = nearest(name, candidates.iter().map(String::as_str));
        let say = match &fix {
            Some((to, _)) => format!("`{name}` is not a type in this file; nearest is `{to}`"),
            None => format!(
                "`{name}` is not a type — types are bool, int, text, list(T), option(T), and this file's enums and records"
            ),
        };
        self.unknown("MZ0701", span, say, fix.map(|(to, c)| (span, to, c)));
        Ty::Unknown
    }

    fn option_ty(&mut self, t: &TypeExpr, arg: &TypeExpr) -> Ty {
        if let TypeKind::Apply(inner_ctor, _) = &arg.kind {
            let why = match inner_ctor.as_str() {
                "list" => Some("a list is never optional: the empty list is its absence"),
                "option" => Some("an option is never nested: there is one `none`"),
                "event" => Some("an event the caller does not bind already does nothing"),
                _ => None,
            };
            if let Some(why) = why {
                self.err_fix(
                    "MZ0703",
                    t.span,
                    format!("`{t}` — {why}; write `{arg}`"),
                    (t.span, arg.to_string(), Confidence::Exact),
                );
                return Ty::Unknown;
            }
        }
        if arg.kind == TypeKind::Nothing {
            self.err(
                "MZ0702",
                t.span,
                "`option(none)` holds nothing — write `option(<type>)`".to_string(),
            );
            return Ty::Unknown;
        }
        Ty::Option(Box::new(self.ty_of(arg, TyCtx::OptionArg)))
    }

    fn event_is_not_data(&mut self, span: Span, ctx: TyCtx) {
        let place = match ctx {
            TyCtx::Field => "a record field",
            TyCtx::Payload => "an event's payload",
            TyCtx::Element => "a list's element",
            TyCtx::OptionArg | TyCtx::Prop => "an option",
        };
        self.err(
            "MZ0702",
            span,
            format!("an event is not data, so it cannot be {place} — only a prop can be an event"),
        );
    }

    /// Report an unknown name, with a fix when [`nearest`] found one.
    fn unknown(
        &mut self,
        code: &'static str,
        span: Span,
        say: String,
        fix: Option<(Span, String, Confidence)>,
    ) {
        match fix {
            Some(fix) => self.err_fix(code, span, say, fix),
            None => self.err(code, span, say),
        }
    }

    // -----------------------------------------------------------------------------------
    // Props and defaults
    // -----------------------------------------------------------------------------------

    fn prop_types(&mut self) {
        for p in &self.c.props {
            let ty = self.ty_of(&p.ty, TyCtx::Prop);
            self.default(p, &ty);
            self.props.entry(p.name.clone()).or_insert(ty);
        }
        for name in &self.c.broken {
            self.props.entry(name.clone()).or_insert(Ty::Unknown);
        }
    }

    /// One default form per type (RFC-0008 §1.2).
    fn default(&mut self, p: &PropDecl, ty: &Ty) {
        let (Some(value), Some(span)) = (&p.default, p.default_span) else {
            return;
        };
        let value_span = Span {
            start_line: span.end_line,
            start_col: span.end_col.saturating_sub(value.chars().count() as u32),
            end_line: span.end_line,
            end_col: span.end_col,
        };
        let name = &p.name;
        let wrong =
            |what: &str| format!("`prop {name}: {ty} = {value}` — the default must be {what}");
        match ty {
            Ty::Unknown => {}
            Ty::Bool if value == "true" || value == "false" => {}
            Ty::Bool => {
                let say = wrong("`true` or `false`");
                match value.as_str() {
                    "\"true\"" | "\"false\"" => self.err_fix(
                        "MZ0706",
                        value_span,
                        say,
                        (value_span, value.trim_matches('"').to_string(), Confidence::Exact),
                    ),
                    _ => self.err("MZ0706", value_span, say),
                }
            }
            Ty::Int if value.parse::<i64>().is_ok() => {}
            Ty::Int => self.err("MZ0706", value_span, wrong("an integer")),
            Ty::Text if value.starts_with('"') => {}
            Ty::Text => self.err_fix(
                "MZ0706",
                value_span,
                wrong("a string"),
                (value_span, format!("\"{value}\""), Confidence::Guess),
            ),
            Ty::Enum(e) => {
                let Some(decl) = self.find_enum(e) else {
                    return;
                };
                if decl.variants.iter().any(|v| &v.name == value) {
                    return;
                }
                let fix = nearest(value, decl.variants.iter().map(|v| v.name.as_str()));
                let say = match &fix {
                    Some((to, _)) => format!(
                        "`prop {name}: {e} = {value}` — `{value}` is not a variant of `{e}`; nearest is `{to}`"
                    ),
                    None => format!(
                        "`prop {name}: {e} = {value}` — `{value}` is not a variant of `{e}`: {}",
                        list_names(decl.variants.iter().map(|v| v.name.as_str()))
                    ),
                };
                self.unknown(
                    "MZ0708",
                    value_span,
                    say,
                    fix.map(|(to, c)| (value_span, to, c)),
                );
            }
            Ty::List(_) => self.err_fix(
                "MZ0706",
                span,
                format!(
                    "`prop {name}: {ty}` is empty when the caller omits it — a list takes no default"
                ),
                (span, String::new(), Confidence::Exact),
            ),
            Ty::Option(inner) if value == "none" => {
                let _ = inner;
                self.err_fix(
                    "MZ0706",
                    span,
                    format!("`prop {name}: {ty}` is already `none` when omitted — delete `= none`"),
                    (span, String::new(), Confidence::Exact),
                );
            }
            Ty::Option(inner) => {
                let whole = Span {
                    start_line: p.ty.span.start_line,
                    start_col: p.ty.span.start_col,
                    end_line: span.end_line,
                    end_col: span.end_col,
                };
                let inner_written = match &p.ty.kind {
                    TypeKind::Apply(_, arg) => arg.to_string(),
                    _ => inner.to_string(),
                };
                self.err_fix(
                    "MZ0706",
                    whole,
                    format!(
                        "`prop {name}: {ty} = {value}` has a value when omitted, so it is not optional — write `{inner_written} = {value}`"
                    ),
                    (whole, format!("{inner_written} = {value}"), Confidence::Exact),
                );
            }
            Ty::Record(_) | Ty::Event(_) | Ty::Fn => self.err_fix(
                "MZ0706",
                span,
                format!("`prop {name}: {ty}` takes no default"),
                (span, String::new(), Confidence::Exact),
            ),
        }
    }

    // -----------------------------------------------------------------------------------
    // Names and paths
    // -----------------------------------------------------------------------------------

    /// Every name a bare word could mean here, for nearest-name suggestions.
    fn names_in_scope(&self, scope: &Scope) -> Vec<String> {
        let mut out: Vec<String> = scope.bindings.iter().map(|(n, _)| n.clone()).collect();
        out.extend(self.props.keys().cloned());
        out.extend(self.c.fns.iter().cloned());
        for e in &self.c.enums {
            out.extend(e.variants.iter().map(|v| v.name.clone()));
            out.push(e.name.clone());
        }
        out.sort();
        out.dedup();
        out
    }

    /// Resolve a dotted path to its type, reporting the first segment that does not
    /// resolve. An option is narrowed where `scope` says so; an un-narrowed option at the
    /// end of the path is returned as an option, for the caller to judge.
    fn path(&mut self, value: &str, segments: &[Span], whole: Span, scope: &Scope) -> Ty {
        let parts: Vec<&str> = value.split('.').collect();
        let seg = |i: usize| segments.get(i).copied().unwrap_or(whole);
        let upto = |i: usize| Span {
            start_line: whole.start_line,
            start_col: whole.start_col,
            end_line: seg(i).end_line,
            end_col: seg(i).end_col,
        };
        let head = parts[0];

        let (mut ty, mut next) =
            if let Some((_, ty)) = scope.bindings.iter().rev().find(|(n, _)| n == head) {
                (ty.clone(), 1)
            } else if let Some(ty) = self.props.get(head) {
                (ty.clone(), 1)
            } else if self.c.fns.iter().any(|f| f == head) {
                (Ty::Fn, 1)
            } else if let Some(decl) = self.find_enum(head).filter(|_| parts.len() > 1) {
                // `<enum>.<variant>`, then that variant's columns.
                let member = parts[1];
                if !decl.variants.iter().any(|v| v.name == member) {
                    let fix = nearest(member, decl.variants.iter().map(|v| v.name.as_str()));
                    let say = format!(
                        "`{}` — `{head}` has no variant `{member}`: {}",
                        clip(value),
                        list_names(decl.variants.iter().map(|v| v.name.as_str()))
                    );
                    self.unknown("MZ0708", seg(1), say, fix.map(|(to, c)| (seg(1), to, c)));
                    return Ty::Unknown;
                }
                (Ty::Enum(head.to_string()), 2)
            } else {
                let owners: Vec<&EnumDecl> = self
                    .c
                    .enums
                    .iter()
                    .filter(|e| e.variants.iter().any(|v| v.name == head))
                    .collect();
                match owners.as_slice() {
                    [one] => (Ty::Enum(one.name.clone()), 1),
                    [] => return self.unresolved(value, parts.len(), seg(0), scope),
                    many => {
                        let names: Vec<&str> = many.iter().map(|e| e.name.as_str()).collect();
                        self.err(
                            "MZ0707",
                            seg(0),
                            format!(
                                "`{head}` is a variant of {} — qualify it: `{}.{head}`",
                                names.join(" and "),
                                names[0]
                            ),
                        );
                        return Ty::Unknown;
                    }
                }
            };

        while next < parts.len() {
            let prefix = parts[..next].join(".");
            ty = narrow(ty, &prefix, scope);
            let member = parts[next];
            ty = match ty {
                Ty::Unknown => return Ty::Unknown,
                Ty::Option(_) => {
                    self.unnarrowed(&prefix, &ty, upto(next - 1));
                    return Ty::Unknown;
                }
                Ty::Enum(ref e) => match self.columns.get(&(e.clone(), member.to_string())) {
                    Some(col) => col.clone(),
                    None => {
                        let cols: Vec<String> = self
                            .columns
                            .keys()
                            .filter(|(owner, _)| owner == e)
                            .map(|(_, c)| c.clone())
                            .collect();
                        let fix = nearest(member, cols.iter().map(String::as_str));
                        let say = format!(
                            "`{}` — `{prefix}` is `{e}`, which has no column `{member}`: {}",
                            clip(value),
                            list_names(cols.iter().map(String::as_str))
                        );
                        self.unknown(
                            "MZ0708",
                            seg(next),
                            say,
                            fix.map(|(to, c)| (seg(next), to, c)),
                        );
                        return Ty::Unknown;
                    }
                },
                Ty::Record(ref r) => {
                    let fields = self.records.get(r).cloned().unwrap_or_default();
                    match fields.iter().find(|(f, _)| f == member) {
                        Some((_, fty)) => fty.clone(),
                        None => {
                            let fix = nearest(member, fields.iter().map(|(f, _)| f.as_str()));
                            let say = format!(
                                "`{}` — `{prefix}` is `{r}`, which has no field `{member}`: {}",
                                clip(value),
                                list_names(fields.iter().map(|(f, _)| f.as_str()))
                            );
                            self.unknown(
                                "MZ0708",
                                seg(next),
                                say,
                                fix.map(|(to, c)| (seg(next), to, c)),
                            );
                            return Ty::Unknown;
                        }
                    }
                }
                Ty::List(_) => {
                    self.err(
                        "MZ0709",
                        seg(next),
                        format!(
                            "`{}` — `{prefix}` is {ty}, and a list has no fields: iterate it with `for each`",
                            clip(value)
                        ),
                    );
                    return Ty::Unknown;
                }
                other => {
                    self.err(
                        "MZ0709",
                        seg(next),
                        format!(
                            "`{}` — `{prefix}` is {other}, which has no `.{member}`",
                            clip(value)
                        ),
                    );
                    return Ty::Unknown;
                }
            };
            next += 1;
        }
        narrow(ty, value, scope)
    }

    /// A path whose head names nothing here.
    fn unresolved(&mut self, value: &str, parts: usize, head_span: Span, scope: &Scope) -> Ty {
        let head = value.split('.').next().unwrap_or(value);
        if self.all_bindings.contains(head) {
            self.err(
                "MZ0707",
                head_span,
                format!("`{head}` is a `for each` binding, and this line is outside its block"),
            );
            return Ty::Unknown;
        }
        if self.c.records.iter().any(|r| r.name == head) {
            self.err(
                "MZ0707",
                head_span,
                format!(
                    "`{head}` is a record type, not a value — reach a field through a prop or a `for each` binding"
                ),
            );
            return Ty::Unknown;
        }
        let names = self.names_in_scope(scope);
        let fix = nearest(head, names.iter().map(String::as_str));
        if parts > 1 {
            // Another component's enum, as `<enum>.<variant>`. It cannot be checked until
            // modules exist (RFC-0007 G2.3), and saying so beats pretending it was. A local
            // name that is merely close (`tones` for `tone`) does not make it an error: the
            // reference is still valid if the enum lives elsewhere, so the local name is
            // offered as a `guess` only.
            let mut d = Diagnostic::warning(
                "MZ0502",
                &self.file,
                head_span,
                match &fix {
                    Some((to, _)) => format!(
                        "`{}` names no enum in this file, so it is read as another component's — unchecked until modules land (RFC-0007 G2.3); if you meant the local `{to}`, that is a guess",
                        clip(value)
                    ),
                    None => format!(
                        "`{}` names no enum in this file, so it is read as another component's — unchecked until modules land (RFC-0007 G2.3); nothing to fix",
                        clip(value)
                    ),
                },
            );
            if let Some((to, _)) = fix {
                d = d.with_fix(head_span, to, Confidence::Guess);
            }
            self.diags.push(d);
            return Ty::Unknown;
        }
        let say = match &fix {
            Some((to, _)) => {
                format!(
                    "`{head}` is not a prop, loop binding, fn or variant here; nearest is `{to}`"
                )
            }
            None => format!(
                "`{head}` is not a prop, loop binding, fn or variant in this file; another component's variant is written `<enum>.<variant>`"
            ),
        };
        self.unknown(
            "MZ0707",
            head_span,
            say,
            fix.map(|(to, c)| (head_span, to, c)),
        );
        Ty::Unknown
    }

    fn unnarrowed(&mut self, path: &str, ty: &Ty, span: Span) {
        self.err(
            "MZ0710",
            span,
            format!(
                "`{}` is {ty} and may be absent — use it inside the `else` of `when {} is none … else … end`",
                clip(path),
                clip(path)
            ),
        );
    }

    /// Check a resolved value against what its position needs.
    fn fits(&mut self, ty: &Ty, use_: Use, path: &str, span: Span) {
        let shown = clip(path);
        match (ty, use_) {
            (Ty::Unknown, _) => {}
            (Ty::Option(_), _) => self.unnarrowed(path, ty, span),
            (Ty::Event(_) | Ty::Fn, Use::Attr | Use::Wire) => {}
            (_, Use::Wire) => self.err(
                "MZ0711",
                span,
                format!("`{shown}` is {ty} — an event attribute takes an event prop or a fn"),
            ),
            (Ty::List(_), _) => self.err(
                "MZ0711",
                span,
                format!(
                    "`{shown}` is {ty} — a list is rendered with `for each`, it is not a value"
                ),
            ),
            (Ty::Record(r), _) => {
                let fields = self.records.get(r).cloned().unwrap_or_default();
                self.err(
                    "MZ0711",
                    span,
                    format!(
                        "`{shown}` is a `{r}` record — name one of its fields: {}",
                        list_names(fields.iter().map(|(f, _)| f.as_str()))
                    ),
                );
            }
            (Ty::Event(_) | Ty::Fn, _) => self.err(
                "MZ0711",
                span,
                format!("`{shown}` is {ty} — it can be wired to `tap`, not rendered"),
            ),
            _ => {}
        }
    }

    // -----------------------------------------------------------------------------------
    // The view
    // -----------------------------------------------------------------------------------

    fn element(&mut self, el: &Element, scope: &Scope) {
        match el.tag.as_str() {
            "nothing" => {}
            "when" => self.when(el, scope),
            "for" => self.for_each(el, scope),
            _ => {
                for attr in &el.attrs {
                    // An unnamed attr is a word left on the element line itself; the view
                    // grammar has never given those a meaning, and they stay unchecked.
                    if attr.name.is_empty() || !is_ident(&attr.name) {
                        continue;
                    }
                    let use_ = match attr.name.as_str() {
                        "tap" | "change" => Use::Wire,
                        _ => Use::Attr,
                    };
                    self.value(attr, use_, scope);
                }
                for child in &el.children {
                    self.element(child, scope);
                }
            }
        }
    }

    /// An attribute's value: a literal, an interpolated string, or a path.
    fn value(&mut self, attr: &Attr, use_: Use, scope: &Scope) {
        let v = attr.value.as_str();
        if v.starts_with('"') {
            if use_ == Use::Wire {
                self.err(
                    "MZ0711",
                    attr.span,
                    format!(
                        "`{} = {}` — an event attribute takes an event prop or a fn, not a string",
                        attr.name,
                        clip(v)
                    ),
                );
                return;
            }
            self.interpolations(attr, scope);
            return;
        }
        if v.is_empty()
            || v.parse::<i64>().is_ok()
            || matches!(v, "true" | "false" | "none" | "nothing")
        {
            if use_ == Use::Wire {
                self.err(
                    "MZ0711",
                    attr.span,
                    format!(
                        "`{} = {v}` — an event attribute takes an event prop or a fn",
                        attr.name
                    ),
                );
            }
            return;
        }
        let ty = self.path(v, &attr.segments, attr.span, scope);
        self.fits(&ty, use_, v, attr.span);
    }

    /// Every `{...}` inside a string attribute (RFC-0008 §5).
    fn interpolations(&mut self, attr: &Attr, scope: &Scope) {
        let inner: Vec<char> = attr
            .value
            .trim_start_matches('"')
            .trim_end_matches('"')
            .chars()
            .collect();
        // Column of inner[k]: one past the opening quote.
        let col_of = |k: usize| attr.span.start_col + 1 + k as u32;
        let line = attr.span.start_line;
        let mut k = 0;
        while k < inner.len() {
            if inner[k] != '{' {
                k += 1;
                continue;
            }
            let open = k;
            let Some(close) = inner[open..]
                .iter()
                .position(|c| *c == '}')
                .map(|p| p + open)
            else {
                self.err(
                    "MZ0714",
                    Span::single(line, col_of(open), 1),
                    format!("`{}` has a `{{` that is never closed", clip(&attr.value)),
                );
                return;
            };
            let body: String = inner[open + 1..close].iter().collect();
            let span = Span::single(line, col_of(open), (close - open + 1) as u32);
            k = close + 1;

            let valid = !body.is_empty()
                && body.split('.').all(|seg| {
                    !seg.is_empty()
                        && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        && !seg.starts_with(|c: char| c.is_ascii_digit())
                });
            if !valid {
                self.err(
                    "MZ0714",
                    span,
                    format!(
                        "`{{{body}}}` — interpolation holds one name or dotted path, e.g. `{{state.label}}`; there are no operators"
                    ),
                );
                continue;
            }
            // Segment spans, from offsets within the string.
            let mut segments = Vec::new();
            let mut offset = open + 1;
            for seg in body.split('.') {
                let len = seg.chars().count();
                segments.push(Span::single(line, col_of(offset), len as u32));
                offset += len + 1;
            }
            let whole = Span::single(line, col_of(open + 1), body.chars().count() as u32);
            if !body.chars().any(|c| c.is_ascii_uppercase()) {
                let ty = self.path(&body, &segments, whole, scope);
                self.fits(&ty, Use::Interp, &body, whole);
                continue;
            }
            // Snake-case each segment on its own: `item.Version` is `item.version`, not the
            // whole string's `item._version`.
            let snake = body
                .split('.')
                .map(crate::lex::to_snake_case)
                .collect::<Vec<_>>()
                .join(".");
            // One mistake, one diagnostic: resolve the corrected path quietly. If it
            // resolves, the case is the only mistake and its fix is exact. If it does not,
            // the fix is only a guess, and the path's own error is not reported on top.
            let before = self.diags.len();
            let ty = self.path(&snake, &segments, whole, scope);
            let resolves = self.diags[before..]
                .iter()
                .all(|d| d.severity != Severity::Error);
            let (confidence, tail) = if resolves {
                (Confidence::Exact, String::new())
            } else {
                self.diags.truncate(before);
                (
                    Confidence::Guess,
                    format!(", though `{}` does not resolve either", clip(&snake)),
                )
            };
            self.err_fix(
                "MZ0101",
                whole,
                format!("`{body}` is not snake_case — Mzizi names are snake_case, so write `{snake}`{tail}"),
                (whole, snake.clone(), confidence),
            );
            if resolves {
                self.fits(&ty, Use::Interp, &snake, whole);
            }
        }
    }

    /// `when` — its condition, its branches, and option narrowing (RFC-0008 §4).
    fn when(&mut self, el: &Element, scope: &Scope) {
        for attr in &el.attrs {
            if !attr.name.is_empty() && !matches!(attr.name.as_str(), "not" | "is") {
                self.err(
                    "MZ0712",
                    attr.span,
                    format!(
                        "`{} = {}` is inside a `when`, which holds elements only — move it onto an element",
                        attr.name,
                        clip(&attr.value)
                    ),
                );
            }
        }
        let cond: Vec<&Attr> = el
            .attrs
            .iter()
            .filter(|a| a.name.is_empty() || matches!(a.name.as_str(), "not" | "is"))
            .collect();
        let mut narrowed_else: Option<String> = None;
        // After `is some`'s rewrite the branch is the narrowed one; checking it as
        // un-narrowed would report the same mistake a second time.
        let mut narrowed_then: Option<String> = None;
        let shape: Vec<&str> = cond.iter().map(|a| a.name.as_str()).collect();
        match shape.as_slice() {
            [""] | ["not"] => {
                let p = cond[0];
                if is_literal(&p.value) {
                    self.bad_condition(el, p.span);
                } else {
                    let ty = self.path(&p.value, &p.segments, p.span, scope);
                    self.truth_test(&ty, p, shape[0] == "not");
                }
            }
            ["", "is"] => {
                let (p, v) = (cond[0], cond[1]);
                if is_literal(&p.value) {
                    self.bad_condition(el, p.span);
                } else {
                    let ty = self.path(&p.value, &p.segments, p.span, scope);
                    if self.is_test(el, &ty, p, v) {
                        narrowed_else = Some(p.value.clone());
                    } else if v.value == "some" {
                        narrowed_then = Some(p.value.clone());
                    }
                }
            }
            ["not", "is"] => {
                let p = cond[0];
                self.err(
                    "MZ0712",
                    p.span,
                    format!(
                        "`when not {} is …` has no meaning — write `when {} is none … else … end` and use the `else`",
                        p.value, p.value
                    ),
                );
            }
            _ => self.bad_condition(el, el.span),
        }

        let mut then_scope = scope.clone();
        if let Some(path) = narrowed_then {
            then_scope.narrowed.push(path);
        }
        for child in &el.children {
            self.element(child, &then_scope);
        }
        if let Some(otherwise) = &el.else_children {
            let mut inner = scope.clone();
            if let Some(path) = narrowed_else {
                inner.narrowed.push(path);
            }
            for child in otherwise {
                self.element(child, &inner);
            }
        }
    }

    fn bad_condition(&mut self, el: &Element, span: Span) {
        let _ = el;
        self.err(
            "MZ0712",
            span,
            "a `when` condition is `<name>`, `not <name>`, or `<name> is <variant or none>`"
                .to_string(),
        );
    }

    /// `when p` / `when not p`: only a bool has a truth value.
    fn truth_test(&mut self, ty: &Ty, p: &Attr, negated: bool) {
        let written = if negated {
            format!("when not {}", p.value)
        } else {
            format!("when {}", p.value)
        };
        match ty {
            Ty::Bool | Ty::Unknown => {}
            Ty::Option(_) => self.err(
                "MZ0712",
                p.span,
                format!(
                    "`{written}` — `{}` is {ty}, not a bool: test presence with `when {} is none … else … end`",
                    p.value, p.value
                ),
            ),
            Ty::List(_) => self.err(
                "MZ0712",
                p.span,
                format!(
                    "`{written}` — `{}` is {ty}, not a bool: test emptiness with `when {} is none … else … end`",
                    p.value, p.value
                ),
            ),
            other => self.err(
                "MZ0712",
                p.span,
                format!("`{written}` needs a bool, but `{}` is {other}", p.value),
            ),
        }
    }

    /// `when p is v`. Returns whether this is an option's `is none`, which narrows `p` in
    /// the `else` branch.
    fn is_test(&mut self, el: &Element, ty: &Ty, p: &Attr, v: &Attr) -> bool {
        let whole = Span {
            start_line: p.span.start_line,
            start_col: p.span.start_col,
            end_line: v.span.end_line,
            end_col: v.span.end_col,
        };
        match ty {
            Ty::Unknown => false,
            Ty::Enum(e) => {
                if v.value == "none" {
                    self.err(
                        "MZ0712",
                        v.span,
                        format!("`{}` is a `{e}`, which is never none", p.value),
                    );
                    return false;
                }
                let Some(decl) = self.find_enum(e) else {
                    return false;
                };
                if !decl.variants.iter().any(|x| x.name == v.value) {
                    let fix = nearest(&v.value, decl.variants.iter().map(|x| x.name.as_str()));
                    let say = format!(
                        "`when {} is {}` — `{}` is not a variant of `{e}`: {}",
                        p.value,
                        clip(&v.value),
                        clip(&v.value),
                        list_names(decl.variants.iter().map(|x| x.name.as_str()))
                    );
                    self.unknown("MZ0708", v.span, say, fix.map(|(to, c)| (v.span, to, c)));
                }
                false
            }
            Ty::Option(_) | Ty::List(_) => {
                if v.value == "none" {
                    return matches!(ty, Ty::Option(_));
                }
                if v.value == "some" && matches!(ty, Ty::Option(_)) && el.else_children.is_some() {
                    // With an `else` already there, the repair swaps the two branches: not
                    // one span, so no fix — only what to write.
                    self.err(
                        "MZ0712",
                        whole,
                        format!(
                            "`when {} is some` — Mzizi has no `some`: write `when {} is none`, and swap this branch with the `else`",
                            p.value, p.value
                        ),
                    );
                    return false;
                }
                if v.value == "some" && matches!(ty, Ty::Option(_)) {
                    // What `avatar.mz` wrote. The repair is mechanical: the presence branch
                    // becomes the `else` of an absence test with an empty first branch.
                    let indent = " ".repeat(el.span.start_col.saturating_sub(1) as usize);
                    let from = Span {
                        start_line: el.span.start_line,
                        start_col: el.span.start_col,
                        end_line: whole.end_line,
                        end_col: whole.end_col,
                    };
                    self.err_fix(
                        "MZ0712",
                        whole,
                        format!(
                            "`when {} is some` — Mzizi has no `some`: write `when {} is none`, `nothing`, `else`, then this branch",
                            p.value, p.value
                        ),
                        (
                            from,
                            format!("when {} is none\n{indent}  nothing\n{indent}else", p.value),
                            Confidence::Exact,
                        ),
                    );
                    return false;
                }
                self.err(
                    "MZ0712",
                    v.span,
                    format!(
                        "`{}` is {ty}, so the only test is `when {} is none`",
                        p.value, p.value
                    ),
                );
                false
            }
            Ty::Bool if v.value == "true" || v.value == "false" => {
                let fixed = if v.value == "true" {
                    p.value.clone()
                } else {
                    format!("not {}", p.value)
                };
                self.err_fix(
                    "MZ0712",
                    whole,
                    format!(
                        "`when {} is {}` — a bool is tested one way: `when {fixed}`",
                        p.value, v.value
                    ),
                    (whole, fixed, Confidence::Exact),
                );
                false
            }
            other if v.value == "none" || v.value == "some" => {
                self.err(
                    "MZ0712",
                    v.span,
                    format!(
                        "`when {} is {}` — `{}` is {other}, which is never absent; declare it `option({other})` if it can be",
                        p.value, v.value, p.value
                    ),
                );
                false
            }
            other => {
                self.err(
                    "MZ0712",
                    v.span,
                    format!(
                        "`when {} is {}` — `is` tests an enum variant or `none`, and `{}` is {other}",
                        p.value,
                        clip(&v.value),
                        p.value
                    ),
                );
                false
            }
        }
    }

    /// `for each <name> in <list>` with its `key` (RFC-0008 §3).
    fn for_each(&mut self, el: &Element, scope: &Scope) {
        let (Some(each), Some(source)) = (
            el.attrs.first().filter(|a| a.name == "each"),
            el.attrs.get(1).filter(|a| a.name == "in"),
        ) else {
            self.err(
                "MZ0713",
                el.span,
                "a `for` line is `for each <name> in <list>`".to_string(),
            );
            return;
        };
        let binding = each.value.clone();
        let mut ok = true;
        if each.segments.len() != 1 || binding.contains('.') {
            self.err(
                "MZ0713",
                each.span,
                format!(
                    "`for each {}` binds one plain name, e.g. `for each entry in entries`",
                    clip(&binding)
                ),
            );
            ok = false;
        } else if scope.bindings.iter().any(|(n, _)| *n == binding)
            || self.props.contains_key(&binding)
            || self.c.fns.contains(&binding)
        {
            self.err(
                "MZ0713",
                each.span,
                format!("`for each {binding}` reuses a name already in scope — pick another, so `{binding}` means one thing"),
            );
            // Still bind it, so its uses inside the block are not a second diagnostic.
        }

        let source_ty = if is_literal(&source.value) {
            Ty::Text
        } else {
            self.path(&source.value, &source.segments, source.span, scope)
        };
        let elem = match &source_ty {
            Ty::List(t) => (**t).clone(),
            Ty::Unknown => Ty::Unknown,
            Ty::Option(_) => {
                self.unnarrowed(&source.value, &source_ty, source.span);
                Ty::Unknown
            }
            other => {
                self.err(
                    "MZ0711",
                    source.span,
                    format!(
                        "`for each {binding} in {}` — `{}` is {other}, and `for each` iterates a list",
                        clip(&source.value),
                        clip(&source.value)
                    ),
                );
                Ty::Unknown
            }
        };

        let mut inner = scope.clone();
        if ok {
            inner.bindings.push((binding.clone(), elem.clone()));
        }

        let mut keys = Vec::new();
        for attr in el.attrs.iter().skip(2) {
            match attr.name.as_str() {
                "key" => keys.push(attr),
                _ => self.err(
                    "MZ0713",
                    attr.span,
                    format!(
                        "`{} = {}` is inside a `for each`, which holds its `key` and elements only",
                        attr.name,
                        clip(&attr.value)
                    ),
                ),
            }
        }
        match keys.as_slice() {
            [] => {
                let say = format!(
                    "`for each {binding}` needs `key = {binding}…` on its first line, so each item keeps its identity"
                );
                if elem.is_scalar() && elem != Ty::Unknown {
                    let indent = " ".repeat(el.span.start_col.saturating_sub(1) as usize + 2);
                    self.err_fix(
                        "MZ0713",
                        el.span,
                        say,
                        (
                            Span::single(el.span.start_line + 1, 1, 0),
                            format!("{indent}key = {binding}\n"),
                            Confidence::Guess,
                        ),
                    );
                } else {
                    self.err("MZ0713", el.span, say);
                }
            }
            [key, rest @ ..] => {
                for extra in rest {
                    self.err(
                        "MZ0713",
                        extra.span,
                        format!(
                            "`for each {binding}` already has a key — one `key` per `for each`"
                        ),
                    );
                }
                self.key(key, &binding, &inner);
            }
        }

        for child in &el.children {
            self.element(child, &inner);
        }
    }

    fn key(&mut self, key: &Attr, binding: &str, scope: &Scope) {
        let v = key.value.as_str();
        if let Some(inner) = v
            .strip_prefix("\"{")
            .and_then(|s| s.strip_suffix("}\""))
            .filter(|s| !s.contains('{') && !s.contains('}'))
        {
            self.err_fix(
                "MZ0713",
                key.span,
                format!("`key = {v}` — a key is written as a path, not a string: `key = {inner}`"),
                (key.span, inner.to_string(), Confidence::Exact),
            );
            return;
        }
        if is_literal(v) {
            self.err(
                "MZ0713",
                key.span,
                format!(
                    "`key = {}` is the same for every item — use a path from `{binding}`",
                    clip(v)
                ),
            );
            return;
        }
        if v.split('.').next() != Some(binding) {
            self.err(
                "MZ0713",
                key.span,
                format!(
                    "`key = {}` does not use `{binding}`, so every item gets the same key",
                    clip(v)
                ),
            );
            return;
        }
        let ty = self.path(v, &key.segments, key.span, scope);
        self.fits(&ty, Use::Key, v, key.span);
    }

    // -----------------------------------------------------------------------------------
    // `emit`
    // -----------------------------------------------------------------------------------

    fn emits(&mut self) {
        for e in &self.c.emits {
            let written = match &e.arg {
                Some((arg, _)) => format!("emit {}({arg})", e.target),
                None => format!("emit {}", e.target),
            };
            let payload = match self.props.get(&e.target) {
                Some(Ty::Event(payload)) => payload.clone(),
                Some(Ty::Unknown) => continue,
                Some(other) => {
                    let other = other.clone();
                    self.err(
                        "MZ0715",
                        e.target_span,
                        format!("`{written}` — `{}` is {other}, not an event", e.target),
                    );
                    continue;
                }
                None => {
                    let events: Vec<String> = self
                        .props
                        .iter()
                        .filter(|(_, t)| matches!(t, Ty::Event(_)))
                        .map(|(n, _)| n.clone())
                        .collect();
                    let fix = nearest(&e.target, events.iter().map(String::as_str));
                    let say = match &fix {
                        Some((to, _)) => format!(
                            "`{written}` — `{}` is not an event prop; nearest is `{to}`",
                            e.target
                        ),
                        None => format!(
                            "`{written}` — `{}` is not an event prop of this component",
                            e.target
                        ),
                    };
                    self.unknown(
                        "MZ0715",
                        e.target_span,
                        say,
                        fix.map(|(to, c)| (e.target_span, to, c)),
                    );
                    continue;
                }
            };
            match (payload, &e.arg) {
                (None, None) => {}
                (None, Some((_, span))) => {
                    let whole = Span {
                        start_line: e.target_span.end_line,
                        start_col: e.target_span.end_col,
                        end_line: span.end_line,
                        end_col: span.end_col + 1,
                    };
                    self.err_fix(
                        "MZ0715",
                        *span,
                        format!(
                            "`{written}` — `{}` is event(none) and carries no value",
                            e.target
                        ),
                        (whole, String::new(), Confidence::Exact),
                    );
                }
                (Some(t), None) => self.err(
                    "MZ0715",
                    e.target_span,
                    format!(
                        "`{written}` — `{}` is event({t}), so it needs a value: `emit {}(<{t}>)`",
                        e.target, e.target
                    ),
                ),
                (Some(t), Some((arg, span))) => self.payload(&written, &t, &t, arg, *span),
            }
        }
    }

    /// Check an `emit` argument against the event's payload type `want`; `carries` is the
    /// payload type as declared, for the message. An `option(T)` payload takes `none` or
    /// any value of type `T`, wrapped implicitly (RFC-0008 §4.2).
    fn payload(&mut self, written: &str, carries: &Ty, want: &Ty, arg: &str, span: Span) {
        if let Ty::Option(inner) = want {
            if arg == "none" {
                return;
            }
            // An `option(T)` value as is — a prop or a dotted path. Resolved quietly: if it
            // is not an option, the check against `T` below reports what is wrong.
            if !is_literal(arg) {
                let before = self.diags.len();
                let got = self.path(arg, &[span], span, &Scope::default());
                self.diags.truncate(before);
                if got == *want {
                    return;
                }
            }
            return self.payload(written, carries, inner, arg, span);
        }
        let got = if arg.starts_with('"') {
            Ty::Text
        } else if arg.parse::<i64>().is_ok() {
            Ty::Int
        } else if arg == "true" || arg == "false" {
            Ty::Bool
        } else if let Ty::Enum(e) = want
            && self
                .find_enum(e)
                .is_some_and(|d| d.variants.iter().any(|v| v.name == arg))
        {
            return;
        } else if let Some(t) = self.props.get(arg) {
            t.clone()
        } else if let Ty::Enum(e) = want {
            // RFC-0001 §4.2's own example: a misspelt variant, fixed from that enum alone.
            let Some(decl) = self.find_enum(e) else {
                return;
            };
            let fix = nearest(arg, decl.variants.iter().map(|v| v.name.as_str()));
            let say = match &fix {
                Some((to, _)) => {
                    format!("`{written}` — `{arg}` is not a variant of `{e}`; nearest is `{to}`")
                }
                None => format!(
                    "`{written}` — `{arg}` is not a variant of `{e}`: {}",
                    list_names(decl.variants.iter().map(|v| v.name.as_str()))
                ),
            };
            self.unknown("MZ0708", span, say, fix.map(|(to, c)| (span, to, c)));
            return;
        } else {
            let scope = Scope::default();
            self.path(arg, &[span], span, &scope)
        };
        if got != *want && got != Ty::Unknown && *want != Ty::Unknown {
            self.err(
                "MZ0715",
                span,
                format!(
                    "`{written}` — the event carries {carries}, but `{}` is {got}",
                    clip(arg)
                ),
            );
        }
    }
}

/// Narrow `ty` if `path` is narrowed in `scope`.
fn narrow(ty: Ty, path: &str, scope: &Scope) -> Ty {
    match ty {
        Ty::Option(inner) if scope.narrowed.iter().any(|p| p == path) => *inner,
        other => other,
    }
}

fn is_ident(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_literal(v: &str) -> bool {
    v.is_empty()
        || v.starts_with('"')
        || v.parse::<i64>().is_ok()
        || matches!(v, "true" | "false" | "none" | "nothing")
}

/// Quote long source briefly, so `say` stays inside RFC-0001 §4.2's 200-character budget.
fn clip(s: &str) -> String {
    if s.chars().count() <= 48 {
        s.to_string()
    } else {
        let head: String = s.chars().take(45).collect();
        format!("{head}…")
    }
}

/// A short list of names for a `say`: at most six, then a count.
fn list_names<'b>(names: impl Iterator<Item = &'b str>) -> String {
    let all: Vec<&str> = names.collect();
    if all.is_empty() {
        return "it has none".to_string();
    }
    let shown: Vec<String> = all.iter().take(6).map(|n| format!("`{n}`")).collect();
    if all.len() > 6 {
        format!("{} and {} more", shown.join(", "), all.len() - 6)
    } else {
        shown.join(", ")
    }
}

/// The nearest candidate to `word`, and how far to trust it (RFC-0008 §5.2).
///
/// Distance is optimal-string-alignment edit distance, so a transposition (`lable`) costs
/// one. A candidate that `word` is a prefix of (`sync` → `syncing`) counts as one edit,
/// because a truncated name is the commonest shape of this mistake. Within reach means at
/// most two edits and at most half the word's length. `Exact` when one candidate is
/// strictly nearest; `Guess` on a tie.
pub fn nearest<'b>(
    word: &str,
    candidates: impl IntoIterator<Item = &'b str>,
) -> Option<(String, Confidence)> {
    let len = word.chars().count();
    let mut scored: Vec<(usize, &str)> = candidates
        .into_iter()
        .filter(|c| *c != word)
        .map(|c| {
            let d = if len >= 3 && c.starts_with(word) {
                1
            } else {
                osa_distance(word, c)
            };
            (d, c)
        })
        .filter(|(d, _)| *d <= 2 && *d * 2 <= len.max(2))
        .collect();
    scored.sort();
    scored.dedup();
    let (best, name) = *scored.first()?;
    let tied = scored.iter().filter(|(d, _)| *d == best).count();
    let confidence = if tied == 1 {
        Confidence::Exact
    } else {
        Confidence::Guess
    };
    Some((name.to_string(), confidence))
}

fn osa_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transposition_is_one_edit() {
        assert_eq!(osa_distance("lable", "label"), 1);
        assert_eq!(osa_distance("versoin", "version"), 1);
    }

    #[test]
    fn a_unique_nearest_name_is_exact_and_a_tie_is_a_guess() {
        assert_eq!(
            nearest("lable", ["label", "title"]),
            Some(("label".to_string(), Confidence::Exact))
        );
        assert_eq!(
            nearest("cat", ["bat", "hat"]).map(|(_, c)| c),
            Some(Confidence::Guess)
        );
    }

    #[test]
    fn a_truncated_name_reaches_its_full_form() {
        // RFC-0001 §4.2's worked example: `sync` → `syncing`.
        assert_eq!(
            nearest("sync", ["online", "syncing", "cached", "offline"]),
            Some(("syncing".to_string(), Confidence::Exact))
        );
    }

    #[test]
    fn nothing_close_means_no_fix() {
        assert_eq!(nearest("flarp", ["bool", "int", "text"]), None);
    }
}
