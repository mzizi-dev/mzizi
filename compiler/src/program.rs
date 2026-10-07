//! A `program` (RFC-0013 §1): the tree, and its checker — types, scope and paths.
//!
//! This is RFC-0013's Wave 0 foundation slice (§18.1): `program` with `fn main`, `fn`s with
//! typed parameters and a return type, calls and recursion, `let` / `var` and assignment,
//! `when` / `else` as statements, `return`, `print`, and `int`, `bool` and `text` values with
//! the integer operators. Its parser is [`crate::parse`]'s (`parse/program.rs`); its
//! lowering to Rust and `mz run` are [`crate::run`]'s.
//!
//! The checker keeps RFC-0013 §16's three rules: a sub-expression that fails is
//! [`Ty::Error`] and silent from then on; a spelling fix repairs the tree in place, so the
//! line reports nothing else; and no two `exact` fixes overlap.

use std::collections::BTreeMap;

use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::expr::{
    BinOp, Expr, ExprKind, Fault, TextPart, Ty, UnOp, binary_type, canonical, canonical_text, fold,
    has_text_form,
};
use crate::resolve::nearest;

/// `program <name>` … `end program <name>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// The program's name.
    pub name: String,
    /// Where the name is.
    pub name_span: Span,
    /// Doc comment lines before or inside the program.
    pub docs: Vec<String>,
    /// Every `fn`, in source order.
    pub fns: Vec<FnDecl>,
    /// Whether statements stood outside every `fn` (reported as `MZ0901`, whose message
    /// names `fn main`, so a missing `fn main` is not reported a second time).
    pub stray_statements: bool,
}

/// A type as written in a signature or an annotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeRef {
    /// The type. [`Ty::Error`] when the name is unknown (already reported).
    pub ty: Ty,
    /// Where it is written.
    pub span: Span,
}

/// One parameter: `name: type`.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    /// Its name.
    pub name: String,
    /// Where the name is.
    pub span: Span,
    /// Its type.
    pub ty: TypeRef,
}

/// `fn name(a: int): int` … `end fn name`.
#[derive(Clone, Debug, PartialEq)]
pub struct FnDecl {
    /// The function's name.
    pub name: String,
    /// Where the name is.
    pub name_span: Span,
    /// Its parameters, in order.
    pub params: Vec<Param>,
    /// Its return type; `None` when it returns nothing.
    pub ret: Option<TypeRef>,
    /// Its body.
    pub body: Vec<Stmt>,
    /// The `end fn <name>` closer, or where it was missing.
    pub end_span: Span,
}

impl FnDecl {
    /// The signature as Mzizi writes it, for a diagnostic to quote.
    pub fn signature(&self) -> String {
        let mut s = format!("fn {}", self.name);
        if !self.params.is_empty() {
            let params: Vec<String> = self
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.ty.ty.name()))
                .collect();
            s.push_str(&format!("({})", params.join(", ")));
        }
        if let Some(r) = self.ret {
            s.push_str(&format!(": {}", r.ty.name()));
        }
        s
    }
}

/// A statement in a function body.
#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    /// What it is.
    pub kind: StmtKind,
    /// Its first line, from its first token to the end of that line.
    pub span: Span,
    /// The last line it covers: its own line, or a `when`'s `end`.
    pub last_line: u32,
}

/// What a statement is.
#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    /// `let name = value` or `var name = value`, with an optional `: type`.
    Bind {
        /// `var` rather than `let`.
        mutable: bool,
        /// Where `let` or `var` is written.
        kw_span: Span,
        /// The name bound.
        name: String,
        /// Where the name is.
        name_span: Span,
        /// The annotation, if any.
        ty: Option<TypeRef>,
        /// The value.
        value: Expr,
    },
    /// `name = value`.
    Assign {
        /// The name assigned.
        name: String,
        /// Where the name is.
        name_span: Span,
        /// A Python-style annotation (`x: int = 1`), if written.
        ty: Option<TypeRef>,
        /// The value.
        value: Expr,
    },
    /// `return` or `return value`.
    Return(Option<Expr>),
    /// `when cond` … [`else` …] `end`.
    When {
        /// The condition.
        cond: Expr,
        /// The branch taken when it holds.
        then: Vec<Stmt>,
        /// The `else` branch, if written.
        otherwise: Option<Vec<Stmt>>,
    },
    /// An expression on its own line: a call.
    Expr(Expr),
}

/// The words RFC-0013 §1 makes contextual: none of them names anything in a program.
pub const CONTEXTUAL_WORDS: &[&str] = &[
    "program", "let", "var", "return", "while", "break", "continue", "and", "or", "try", "self",
    "error", "ok", "float", "map", "set", "result",
];

/// The built-in type and function names, which no binding or `fn` reuses.
pub const BUILT_IN_NAMES: &[&str] = &["int", "bool", "text", "list", "option", "print", "range"];

/// Check a program. Every diagnostic it finds, in no particular order (the report sorts).
pub fn check(p: &Program, file: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut fns: BTreeMap<&str, &FnDecl> = BTreeMap::new();
    let mut mains = Vec::new();
    for f in &p.fns {
        if f.name == "main" {
            mains.push(f);
        }
        if let Some(why) = reserved_name(&f.name) {
            diags.push(Diagnostic::error(
                "MZ0921",
                file,
                f.name_span,
                format!("`fn {}` cannot be named so: {why}", f.name),
            ));
        } else if matches!(f.name.as_str(), "print" | "range") {
            diags.push(Diagnostic::error(
                "MZ0904",
                file,
                f.name_span,
                format!(
                    "`fn {}` reuses the name of a built-in function — choose another name",
                    f.name
                ),
            ));
        } else if fns.contains_key(f.name.as_str()) {
            if f.name != "main" {
                diags.push(Diagnostic::error(
                    "MZ0904",
                    file,
                    f.name_span,
                    format!(
                        "`fn {}` is declared twice — Mzizi has no overloading; rename one",
                        f.name
                    ),
                ));
            }
        } else {
            fns.insert(f.name.as_str(), f);
        }
        let mut seen: Vec<&str> = Vec::new();
        for param in &f.params {
            if seen.contains(&param.name.as_str()) {
                diags.push(Diagnostic::error(
                    "MZ0904",
                    file,
                    param.span,
                    format!(
                        "`{}` is a parameter of `fn {}` twice — rename one",
                        param.name, f.name
                    ),
                ));
            }
            seen.push(&param.name);
        }
    }
    match mains.as_slice() {
        [] if p.stray_statements => {}
        [] => diags.push(Diagnostic::error(
            "MZ0902",
            file,
            p.name_span,
            format!(
                "`program {}` has no `fn main` — the entry point is `fn main`, with no parameters",
                p.name
            ),
        )),
        [first, rest @ ..] => {
            for extra in rest {
                diags.push(Diagnostic::error(
                    "MZ0902",
                    file,
                    extra.name_span,
                    format!(
                        "a second `fn main` — a program has exactly one entry point, declared on line {}",
                        first.name_span.start_line
                    ),
                ));
            }
            if let Some(param) = first.params.first() {
                diags.push(Diagnostic::error(
                    "MZ0902",
                    file,
                    param.span,
                    "`fn main` takes no parameters — command-line arguments are not in the language yet",
                ));
            }
            if let Some(ret) = first.ret {
                diags.push(Diagnostic::error(
                    "MZ0902",
                    file,
                    ret.span,
                    format!(
                        "`fn main` returns nothing, and this one declares `: {}` — a program's output is what it prints",
                        ret.ty.name()
                    ),
                ));
            }
        }
    }
    for f in &p.fns {
        let mut cx = FnCheck::new(f, &fns, file);
        cx.run();
        diags.append(&mut cx.diags);
    }
    diags
}

/// Why `name` cannot name anything in a program, if it cannot.
fn reserved_name(name: &str) -> Option<String> {
    if CONTEXTUAL_WORDS.contains(&name) {
        return Some(format!("`{name}` is a word of the language (RFC-0013 §1)"));
    }
    if name.starts_with("mz_") {
        return Some("the prefix `mz_` belongs to the generated code (RFC-0013 §14.2)".into());
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Let,
    Var,
    Param,
    /// A name already reported as unbound; silent from then on.
    Poison,
}

struct Binding {
    name: String,
    kind: Kind,
    ty: Ty,
    /// Where `let` or `var` is written, for the `let` ↔ `var` fixes.
    kw_span: Option<Span>,
    /// Whether a `var` was ever assigned (`MZ0924` otherwise).
    assigned: bool,
}

/// A binding anywhere in the function, collected before checking, so that a use the scope
/// rules reject can say why: bound later (`MZ0920`), bound in a block that ended
/// (`MZ0920`), or bound nowhere (`MZ0707`).
struct Site {
    name: String,
    line: u32,
    ty: Ty,
    /// The `when` statements that enclose the binding, outermost first: `(line, column)`.
    whens: Vec<(u32, u32)>,
}

struct FnCheck<'a> {
    f: &'a FnDecl,
    fns: &'a BTreeMap<&'a str, &'a FnDecl>,
    file: &'a str,
    diags: Vec<Diagnostic>,
    bindings: Vec<Binding>,
    scopes: Vec<Vec<usize>>,
    sites: Vec<Site>,
    whens: Vec<(u32, u32)>,
    /// Lines of every assignment in the function, by name, for `MZ0923`'s `let` or `var`.
    assigns: BTreeMap<String, Vec<u32>>,
}

impl<'a> FnCheck<'a> {
    fn new(f: &'a FnDecl, fns: &'a BTreeMap<&'a str, &'a FnDecl>, file: &'a str) -> Self {
        let mut cx = FnCheck {
            f,
            fns,
            file,
            diags: Vec::new(),
            bindings: Vec::new(),
            scopes: vec![Vec::new()],
            sites: Vec::new(),
            whens: Vec::new(),
            assigns: BTreeMap::new(),
        };
        cx.collect(&f.body, &mut Vec::new());
        cx
    }

    fn collect(&mut self, stmts: &[Stmt], whens: &mut Vec<(u32, u32)>) {
        for s in stmts {
            match &s.kind {
                StmtKind::Bind {
                    name,
                    name_span,
                    ty,
                    value,
                    ..
                } => self.sites.push(Site {
                    name: name.clone(),
                    line: name_span.start_line,
                    ty: ty.map(|t| t.ty).unwrap_or_else(|| shallow_type(value)),
                    whens: whens.clone(),
                }),
                StmtKind::Assign {
                    name, name_span, ..
                } => self
                    .assigns
                    .entry(name.clone())
                    .or_default()
                    .push(name_span.start_line),
                StmtKind::When {
                    then, otherwise, ..
                } => {
                    whens.push((s.span.start_line, s.span.start_col));
                    self.collect(then, whens);
                    if let Some(o) = otherwise {
                        self.collect(o, whens);
                    }
                    whens.pop();
                }
                _ => {}
            }
        }
    }

    fn err(&mut self, code: &'static str, span: Span, say: impl Into<String>) {
        self.diags
            .push(Diagnostic::error(code, self.file, span, say.into()));
    }

    fn err_fix(
        &mut self,
        code: &'static str,
        span: Span,
        say: impl Into<String>,
        fix: Span,
        replace: impl Into<String>,
        c: Confidence,
    ) {
        self.diags
            .push(Diagnostic::error(code, self.file, span, say.into()).with_fix(fix, replace, c));
    }

    fn run(&mut self) {
        let f = self.f;
        for p in &f.params {
            if let Some(why) = reserved_name(&p.name) {
                self.err(
                    "MZ0921",
                    p.span,
                    format!("parameter `{}` cannot be named so: {why}", p.name),
                );
            } else if BUILT_IN_NAMES.contains(&p.name.as_str())
                || self.fns.contains_key(p.name.as_str())
            {
                self.err(
                    "MZ0921",
                    p.span,
                    format!(
                        "parameter `{}` reuses the name of a function or built-in — choose another name",
                        p.name
                    ),
                );
            }
            self.bind(&p.name, Kind::Param, p.ty.ty, None);
        }
        // The Rust habit: a body whose last line is a value of the return type, with no
        // `return`. That is `MZ0906` with an `exact` fix, and the line is not also `MZ0916`.
        let tail_value = match (f.ret, f.body.last()) {
            (
                Some(_),
                Some(Stmt {
                    kind: StmtKind::Expr(e),
                    ..
                }),
            ) if !terminates(&f.body) => Some(e.span),
            _ => None,
        };
        self.block(&f.body, tail_value);
        self.reachability(&f.body);
        if let Some(ret) = f.ret
            && !terminates(&f.body)
        {
            let say = format!(
                "a path through `{}` reaches `end fn {}` on line {} without a `return` — every path of a function with a return type ends in `return`",
                f.signature(),
                f.name,
                f.end_span.start_line
            );
            let fix = match f.body.last() {
                Some(Stmt {
                    kind: StmtKind::Expr(e),
                    ..
                }) if self.shallow_ty(e) == ret.ty => Some(e.span),
                _ => None,
            };
            match fix {
                Some(at) => self.err_fix(
                    "MZ0906",
                    f.end_span,
                    say,
                    Span::single(at.start_line, at.start_col, 0),
                    "return ",
                    Confidence::Exact,
                ),
                None => self.err("MZ0906", f.end_span, say),
            }
        }
        // `var`s never reassigned: one intent, one form (RFC-0013 §5.1).
        let unassigned: Vec<(String, Span)> = self
            .bindings
            .iter()
            .filter(|b| b.kind == Kind::Var && !b.assigned)
            .filter_map(|b| b.kw_span.map(|s| (b.name.clone(), s)))
            .collect();
        for (name, span) in unassigned {
            self.diags.push(
                Diagnostic::warning(
                    "MZ0924",
                    self.file,
                    span,
                    format!(
                        "`var {name}` is never assigned — a name that never changes is a `let`"
                    ),
                )
                .with_fix(span, "let", Confidence::Exact),
            );
        }
    }

    /// The type of an expression without reporting anything: for the `MZ0906` fix, which
    /// is decided after the body has been checked and its diagnostics are already out.
    fn shallow_ty(&self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::Name(n) => self
                .bindings
                .iter()
                .rev()
                .find(|b| &b.name == n)
                .map_or(Ty::Error, |b| b.ty),
            ExprKind::Call { name, .. } => self
                .fns
                .get(name.as_str())
                .map_or(Ty::Error, |f| f.ret.map_or(Ty::Nothing, |r| r.ty)),
            _ => shallow_type(e),
        }
    }

    fn bind(&mut self, name: &str, kind: Kind, ty: Ty, kw_span: Option<Span>) {
        self.bindings.push(Binding {
            name: name.to_string(),
            kind,
            ty,
            kw_span,
            assigned: false,
        });
        let i = self.bindings.len() - 1;
        self.scopes
            .last_mut()
            .expect("a function always has a scope")
            .push(i);
    }

    fn visible(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .copied()
            .find(|&i| self.bindings[i].name == name)
    }

    fn block(&mut self, stmts: &[Stmt], tail_value: Option<Span>) {
        for s in stmts {
            self.stmt(s, tail_value);
        }
    }

    fn scoped(&mut self, stmts: &[Stmt]) {
        self.scopes.push(Vec::new());
        self.block(stmts, None);
        self.scopes.pop();
    }

    /// Whether a new binding may take `name`; reports `MZ0921` when it may not.
    fn may_bind(&mut self, name: &str, span: Span) -> bool {
        if let Some(why) = reserved_name(name) {
            self.err(
                "MZ0921",
                span,
                format!("`{name}` cannot name a binding: {why}"),
            );
            return false;
        }
        if BUILT_IN_NAMES.contains(&name) || self.fns.contains_key(name) {
            let what = if self.fns.contains_key(name) {
                "a function of this program"
            } else {
                "a built-in"
            };
            self.err(
                "MZ0921",
                span,
                format!(
                    "`{name}` is already {what}, so a binding cannot take the name — choose another"
                ),
            );
            return false;
        }
        if let Some(i) = self.visible(name)
            && self.bindings[i].kind != Kind::Poison
        {
            let what = match self.bindings[i].kind {
                Kind::Param => "a parameter",
                _ => "a binding in scope",
            };
            self.err_fix(
                "MZ0921",
                span,
                format!(
                    "`{name}` is already {what} — a name is bound at most once in a function (RFC-0013 §5.3)"
                ),
                span,
                format!("{name}_2"),
                Confidence::Guess,
            );
            return false;
        }
        true
    }

    fn stmt(&mut self, s: &Stmt, tail_value: Option<Span>) {
        match &s.kind {
            StmtKind::Bind {
                mutable,
                kw_span,
                name,
                name_span,
                ty,
                value,
            } => {
                let vt = self.expr(value);
                let bound = self.binding_type(name, ty.as_ref(), vt, value.span);
                if self.may_bind(name, *name_span) {
                    let kind = if *mutable { Kind::Var } else { Kind::Let };
                    self.bind(name, kind, bound, Some(*kw_span));
                } else if let Some(i) = self.visible(name) {
                    // Reported once; later reads see the newer type, and are silent.
                    self.bindings[i].ty = bound;
                } else {
                    // A reserved name, reported once; later reads are silent.
                    self.bind(name, Kind::Poison, bound, None);
                }
            }
            StmtKind::Assign {
                name,
                name_span,
                ty,
                value,
            } => {
                let vt = self.expr(value);
                match self.visible(name) {
                    Some(i) => {
                        // Python's annotated assignment to a name already bound: the type
                        // was fixed at the binding, so the annotation goes.
                        if let Some(t) = ty {
                            let written = Span {
                                start_line: name_span.end_line,
                                start_col: name_span.end_col,
                                end_line: t.span.end_line,
                                end_col: t.span.end_col,
                            };
                            self.err_fix(
                                "MZ0925",
                                written,
                                format!(
                                    "`{name}` is already bound, and an assignment takes no type — write `{name} = …`"
                                ),
                                written,
                                "",
                                Confidence::Exact,
                            );
                        }
                        let b = &self.bindings[i];
                        match b.kind {
                            Kind::Let => {
                                let kw = b.kw_span;
                                // One report per binding: the fix makes every assignment legal.
                                self.bindings[i].kind = Kind::Var;
                                self.bindings[i].assigned = true;
                                let say = format!(
                                    "`{name}` is a `let`, which never changes — declare it with `var` to assign to it"
                                );
                                match kw {
                                    Some(kw) => self.err_fix(
                                        "MZ0922",
                                        *name_span,
                                        say,
                                        kw,
                                        "var",
                                        Confidence::Exact,
                                    ),
                                    None => self.err("MZ0922", *name_span, say),
                                }
                            }
                            Kind::Param => self.err(
                                "MZ0922",
                                *name_span,
                                format!(
                                    "`{name}` is a parameter, and a parameter cannot be assigned — bind a `var` with its value instead"
                                ),
                            ),
                            Kind::Var => {
                                self.bindings[i].assigned = true;
                                let bt = self.bindings[i].ty;
                                if bt != Ty::Error && vt != Ty::Error && bt != vt {
                                    self.err(
                                        "MZ0711",
                                        value.span,
                                        format!(
                                            "`{name}` is {}, and `{}` is {}",
                                            bt.name(),
                                            canonical(value),
                                            vt.name()
                                        ),
                                    );
                                }
                            }
                            Kind::Poison => {}
                        }
                    }
                    None => {
                        if self.unbound_elsewhere(name, *name_span) {
                            self.bind(name, Kind::Poison, Ty::Error, None);
                            return;
                        }
                        if !self.may_bind(name, *name_span) {
                            self.bind(name, Kind::Poison, vt, None);
                            return;
                        }
                        // Python's first assignment. The checker knows whether the name is
                        // assigned again, so `let` or `var` is not a guess.
                        let again = self
                            .assigns
                            .get(name)
                            .is_some_and(|lines| lines.iter().any(|&l| l > name_span.start_line));
                        let word = if again { "var" } else { "let" };
                        self.err_fix(
                            "MZ0923",
                            *name_span,
                            format!(
                                "`{name}` is assigned before it is bound — a binding starts with `{word}`: `{word} {name} = {}`",
                                canonical(value)
                            ),
                            Span::single(name_span.start_line, name_span.start_col, 0),
                            format!("{word} "),
                            Confidence::Exact,
                        );
                        let bound = self.binding_type(name, ty.as_ref(), vt, value.span);
                        let kind = if again { Kind::Var } else { Kind::Let };
                        self.bind(name, kind, bound, None);
                        if let Some(i) = self.visible(name) {
                            self.bindings[i].assigned = true;
                        }
                    }
                }
            }
            StmtKind::Return(value) => {
                let f = self.f;
                match (value, f.ret) {
                    (None, None) => {}
                    (Some(v), None) => {
                        let t = self.expr(v);
                        if t != Ty::Error {
                            self.err(
                                "MZ0908",
                                v.span,
                                format!(
                                    "`{}` returns nothing, so `return` takes no value — `{}` is {}",
                                    f.signature(),
                                    canonical(v),
                                    t.name()
                                ),
                            );
                        }
                    }
                    (None, Some(r)) => self.err(
                        "MZ0908",
                        s.span,
                        format!(
                            "`{}` returns {}, so `return` needs a value",
                            f.signature(),
                            r.ty.name()
                        ),
                    ),
                    (Some(v), Some(r)) => {
                        let t = self.expr(v);
                        if t != Ty::Error && r.ty != Ty::Error && t != r.ty {
                            self.err(
                                "MZ0908",
                                v.span,
                                format!(
                                    "`{}` returns {}, and `{}` is {}",
                                    f.signature(),
                                    r.ty.name(),
                                    canonical(v),
                                    t.name()
                                ),
                            );
                        }
                    }
                }
            }
            StmtKind::When {
                cond,
                then,
                otherwise,
            } => {
                let t = self.expr(cond);
                if t != Ty::Bool && t != Ty::Error {
                    let question = match t {
                        Ty::Int => Some(format!("{} is not 0", canonical(cond))),
                        Ty::Text => Some(format!("{} is not \"\"", canonical(cond))),
                        _ => None,
                    };
                    let say = format!(
                        "`when {}` needs a bool, and `{}` is {} — Mzizi has no truthiness, so ask the question",
                        canonical(cond),
                        canonical(cond),
                        t.name()
                    );
                    match question {
                        Some(q) => {
                            self.err_fix("MZ0712", cond.span, say, cond.span, q, Confidence::Guess)
                        }
                        None => self.err("MZ0712", cond.span, say),
                    }
                }
                self.whens.push((s.span.start_line, s.span.start_col));
                self.scoped(then);
                if let Some(o) = otherwise {
                    self.scoped(o);
                }
                self.whens.pop();
            }
            StmtKind::Expr(e) => {
                let t = self.expr(e);
                let is_tail = tail_value == Some(e.span);
                if !matches!(t, Ty::Nothing | Ty::Error) && !is_tail {
                    self.err(
                        "MZ0916",
                        e.span,
                        format!(
                            "`{}` is {} and nothing reads it — bind it with `let`, return it, or print it",
                            canonical(e),
                            t.name()
                        ),
                    );
                }
            }
        }
    }

    /// The type a binding takes: its annotation when written (checked against the value),
    /// otherwise the value's.
    fn binding_type(&mut self, name: &str, ty: Option<&TypeRef>, vt: Ty, at: Span) -> Ty {
        if vt == Ty::Nothing {
            self.err(
                "MZ0711",
                at,
                format!("`{name}` is bound to a call that returns nothing, so it has no value"),
            );
            return Ty::Error;
        }
        match ty {
            Some(t) => {
                if t.ty != Ty::Error && vt != Ty::Error && t.ty != vt {
                    self.err(
                        "MZ0711",
                        at,
                        format!(
                            "`{name}` is declared {}, and its value is {}",
                            t.ty.name(),
                            vt.name()
                        ),
                    );
                }
                t.ty
            }
            None => vt,
        }
    }

    /// For a name the scope rules do not see: is it bound elsewhere in this function? If
    /// so, report `MZ0920` and return true.
    fn unbound_elsewhere(&mut self, name: &str, at: Span) -> bool {
        let before = self
            .sites
            .iter()
            .find(|s| s.name == name && s.line < at.start_line);
        let Some(site) = before.or_else(|| self.sites.iter().find(|s| s.name == name)) else {
            return false;
        };
        let (line, ty, whens) = (site.line, site.ty, site.whens.clone());
        if line >= at.start_line {
            self.err(
                "MZ0920",
                at,
                format!(
                    "`{name}` is used before its binding on line {line} — a binding is visible from the line after it"
                ),
            );
            return true;
        }
        // Bound in a block that has ended. The repair (RFC-0013 §5.2): declare a `var`
        // before the outermost `when` that holds the binding and not this use.
        let at_when = whens
            .iter()
            .enumerate()
            .find(|(k, w)| self.whens.get(*k) != Some(*w))
            .map(|(_, w)| *w);
        let zero = match ty {
            Ty::Int => Some("0"),
            Ty::Bool => Some("false"),
            Ty::Text => Some("\"\""),
            _ => None,
        };
        let say = format!(
            "`{name}` was bound on line {line}, in a block that ended before this line — declare `var {name}` before that block and assign it inside"
        );
        match (at_when, zero) {
            (Some((wl, wc)), Some(zero)) => {
                let indent = " ".repeat(wc.saturating_sub(1) as usize);
                self.err_fix(
                    "MZ0920",
                    at,
                    say,
                    Span::single(wl, wc, 0),
                    format!("var {name} = {zero}\n{indent}"),
                    Confidence::Guess,
                );
            }
            _ => self.err("MZ0920", at, say),
        }
        true
    }

    /// Type an expression, reporting what is wrong with it once.
    fn expr(&mut self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Text(parts) => {
                for part in parts {
                    if let TextPart::Expr(inner) = part {
                        let t = self.expr(inner);
                        if !has_text_form(t) {
                            self.err(
                                "MZ0711",
                                inner.span,
                                format!(
                                    "`{{{}}}` is {}, which has no text form to interpolate",
                                    canonical(inner),
                                    t.name()
                                ),
                            );
                        }
                    }
                }
                Ty::Text
            }
            ExprKind::Name(name) => self.read(name, e.span),
            ExprKind::Call {
                name,
                name_span,
                args,
            } => self.call(name, *name_span, args, e.span),
            ExprKind::Unary { op, operand } => {
                let t = self.expr(operand);
                let want = match op {
                    UnOp::Neg => Ty::Int,
                    UnOp::Not => Ty::Bool,
                };
                if t != want && t != Ty::Error {
                    let (word, kind) = match op {
                        UnOp::Neg => ("-", "an int"),
                        UnOp::Not => ("not", "a bool"),
                    };
                    self.err(
                        "MZ0912",
                        e.span,
                        format!(
                            "`{word}` takes {kind}, and `{}` is {}",
                            canonical(operand),
                            t.name()
                        ),
                    );
                    return if *op == UnOp::Not {
                        Ty::Bool
                    } else {
                        Ty::Error
                    };
                }
                if *op == UnOp::Neg
                    && matches!(fold(e), Some(Err(Fault::Overflow)))
                    && fold(operand).is_some_and(|r| r.is_ok())
                {
                    self.err(
                        "MZ0915",
                        e.span,
                        format!(
                            "`{}` overflows an int — this line would trap every time it ran",
                            canonical(e)
                        ),
                    );
                }
                want
            }
            ExprKind::Binary { op: BinOp::Add, .. } => self.add_chain(e),
            ExprKind::Binary { op, lhs, rhs, .. } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                self.binary(e, *op, lhs, rhs, l, r)
            }
            ExprKind::Error => Ty::Error,
        }
    }

    /// The type of a binary expression whose operands are typed, with its diagnostics.
    fn binary(&mut self, e: &Expr, op: BinOp, lhs: &Expr, rhs: &Expr, l: Ty, r: Ty) -> Ty {
        match binary_type(op, l, r) {
            Ok(t) => {
                if t == Ty::Int {
                    self.constant_fault(e, op, lhs, rhs);
                }
                t
            }
            Err(why) => {
                self.err("MZ0912", e.span, format!("`{}`: {why}", canonical(e)));
                if op.is_arithmetic() {
                    Ty::Error
                } else {
                    Ty::Bool
                }
            }
        }
    }

    /// `MZ0915`: a division by a constant zero, or a constant computation that overflows,
    /// reported where it first goes wrong.
    fn constant_fault(&mut self, e: &Expr, op: BinOp, lhs: &Expr, rhs: &Expr) {
        if matches!(op, BinOp::Div | BinOp::Rem) && fold(rhs) == Some(Ok(0)) {
            self.err(
                "MZ0915",
                e.span,
                format!(
                    "`{}` divides by zero — this line would trap every time it ran",
                    canonical(e)
                ),
            );
            return;
        }
        let children_ok =
            fold(lhs).is_some_and(|r| r.is_ok()) && fold(rhs).is_some_and(|r| r.is_ok());
        if children_ok && fold(e) == Some(Err(Fault::Overflow)) {
            self.err(
                "MZ0915",
                e.span,
                format!(
                    "`{}` overflows an int — this line would trap every time it ran",
                    canonical(e)
                ),
            );
        }
    }

    /// `a + b + c`. Integer addition, unless a leaf is text: then it is the concatenation
    /// idiom, one `MZ0912` for the whole chain with the interpolated form as its fix
    /// (RFC-0013 §3.6), `exact` when every leaf is a name or a literal.
    fn add_chain(&mut self, e: &Expr) -> Ty {
        let mut leaves = Vec::new();
        flatten_add(e, &mut leaves);
        let types: Vec<Ty> = leaves.iter().map(|l| self.expr(l)).collect();
        if types.contains(&Ty::Text) && types.iter().all(|t| has_text_form(*t)) {
            let mut parts = Vec::new();
            let mut fixable = true;
            let mut simple = true;
            for leaf in &leaves {
                match &leaf.kind {
                    ExprKind::Text(p) => parts.extend(p.iter().cloned()),
                    ExprKind::Name(_) => parts.push(TextPart::Expr((*leaf).clone())),
                    _ if !leaf.has_text_literal() => {
                        simple = false;
                        parts.push(TextPart::Expr((*leaf).clone()));
                    }
                    _ => fixable = false,
                }
            }
            let say = format!(
                "`{}`: Mzizi has no `+` on text — interpolation is the one way to build text",
                canonical(e)
            );
            if fixable {
                let fixed = canonical_text(&merge_literals(parts));
                let c = if simple {
                    Confidence::Exact
                } else {
                    Confidence::Guess
                };
                self.err_fix("MZ0912", e.span, say, e.span, fixed, c);
            } else {
                self.err("MZ0912", e.span, say);
            }
            return Ty::Text;
        }
        let mut at = 0;
        self.retype_add(e, &types, &mut at)
    }

    fn retype_add(&mut self, e: &Expr, types: &[Ty], at: &mut usize) -> Ty {
        match &e.kind {
            ExprKind::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                ..
            } => {
                let l = self.retype_add(lhs, types, at);
                let r = self.retype_add(rhs, types, at);
                self.binary(e, BinOp::Add, lhs, rhs, l, r)
            }
            _ => {
                let t = types[*at];
                *at += 1;
                t
            }
        }
    }

    fn read(&mut self, name: &str, at: Span) -> Ty {
        if let Some(i) = self.visible(name) {
            return self.bindings[i].ty;
        }
        if let Some(f) = self.fns.get(name).copied() {
            let say = format!(
                "`{name}` is a function — a function is called, `{name}(…)`, and is not a value in a program yet"
            );
            if f.params.is_empty() {
                self.err_fix(
                    "MZ0909",
                    at,
                    say,
                    Span::single(at.end_line, at.end_col, 0),
                    "()",
                    Confidence::Exact,
                );
            } else {
                self.err("MZ0909", at, say);
            }
            return Ty::Error;
        }
        if !self.unbound_elsewhere(name, at) {
            let mut names: Vec<&str> = self
                .scopes
                .iter()
                .flatten()
                .map(|&i| self.bindings[i].name.as_str())
                .collect();
            names.sort();
            names.dedup();
            match nearest(name, names) {
                Some((near, _)) => self.err_fix(
                    "MZ0707",
                    at,
                    format!("`{name}` is not bound here — did you mean `{near}`?"),
                    at,
                    near,
                    Confidence::Guess,
                ),
                None => self.err(
                    "MZ0707",
                    at,
                    format!("`{name}` is not bound here — bind it with `let {name} = …` first"),
                ),
            }
        }
        // Reported once; later reads of the same name are silent.
        self.bind(name, Kind::Poison, Ty::Error, None);
        Ty::Error
    }

    fn call(&mut self, name: &str, name_span: Span, args: &[Expr], at: Span) -> Ty {
        let types: Vec<Ty> = args.iter().map(|a| self.expr(a)).collect();
        if name == "print" {
            // The parser repairs every other arity (`MZ0980`), so one argument is left.
            if let (Some(a), Some(t)) = (args.first(), types.first())
                && !has_text_form(*t)
            {
                self.err(
                    "MZ0905",
                    a.span,
                    format!(
                        "`print` takes one value with a text form, and `{}` is {}",
                        canonical(a),
                        t.name()
                    ),
                );
            }
            return Ty::Nothing;
        }
        if self.visible(name).is_some() {
            self.err(
                "MZ0711",
                name_span,
                format!("`{name}` is a binding, not a function, so it cannot be called"),
            );
            return Ty::Error;
        }
        let Some(f) = self.fns.get(name).copied() else {
            let mut names: Vec<&str> = self.fns.keys().copied().collect();
            names.push("print");
            match nearest(name, names) {
                Some((near, _)) => self.err_fix(
                    "MZ0707",
                    name_span,
                    format!("there is no function `{name}` — did you mean `{near}`?"),
                    name_span,
                    near,
                    Confidence::Guess,
                ),
                None => self.err(
                    "MZ0707",
                    name_span,
                    format!("there is no function `{name}` in this program"),
                ),
            }
            return Ty::Error;
        };
        let ret = f.ret.map_or(Ty::Nothing, |r| r.ty);
        if args.len() != f.params.len() {
            self.err(
                "MZ0905",
                at,
                format!(
                    "`{name}` takes {} argument{}, and this call gives {} — `{}`",
                    f.params.len(),
                    if f.params.len() == 1 { "" } else { "s" },
                    args.len(),
                    f.signature()
                ),
            );
            return ret;
        }
        for ((a, t), p) in args.iter().zip(&types).zip(&f.params) {
            if *t != Ty::Error && p.ty.ty != Ty::Error && *t != p.ty.ty {
                self.err(
                    "MZ0905",
                    a.span,
                    format!(
                        "`{}` is {}, and `{}` takes `{}: {}` — `{}`",
                        canonical(a),
                        t.name(),
                        name,
                        p.name,
                        p.ty.ty.name(),
                        f.signature()
                    ),
                );
            }
        }
        ret
    }

    /// `MZ0907`: a statement after a `return` on the same path can never run. One
    /// diagnostic per block, whose `exact` fix deletes every such line.
    fn reachability(&mut self, stmts: &[Stmt]) {
        let mut done = false;
        for s in stmts {
            if done {
                let last = stmts.last().map_or(s.last_line, |l| l.last_line);
                self.err_fix(
                    "MZ0907",
                    s.span,
                    "this line comes after a `return` on every path, so it can never run — delete it",
                    Span {
                        start_line: s.span.start_line,
                        start_col: 1,
                        end_line: last + 1,
                        end_col: 1,
                    },
                    "",
                    Confidence::Exact,
                );
                break;
            }
            if let StmtKind::When {
                then, otherwise, ..
            } = &s.kind
            {
                self.reachability(then);
                if let Some(o) = otherwise {
                    self.reachability(o);
                }
            }
            done = terminates(std::slice::from_ref(s));
        }
    }
}

/// Whether every path through `stmts` ends in `return`.
pub fn terminates(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::When {
            then,
            otherwise: Some(o),
            ..
        } => terminates(then) && terminates(o),
        _ => false,
    })
}

/// A type read off an expression's shape alone, for the sites collected before checking.
fn shallow_type(e: &Expr) -> Ty {
    match &e.kind {
        ExprKind::Int(_) => Ty::Int,
        ExprKind::Bool(_) => Ty::Bool,
        ExprKind::Text(_) => Ty::Text,
        ExprKind::Unary { op: UnOp::Neg, .. } => Ty::Int,
        ExprKind::Unary { op: UnOp::Not, .. } => Ty::Bool,
        ExprKind::Binary { op, .. } if op.is_arithmetic() => Ty::Int,
        ExprKind::Binary { .. } => Ty::Bool,
        _ => Ty::Error,
    }
}

fn flatten_add<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    match &e.kind {
        ExprKind::Binary {
            op: BinOp::Add,
            lhs,
            rhs,
            ..
        } => {
            flatten_add(lhs, out);
            flatten_add(rhs, out);
        }
        _ => out.push(e),
    }
}

/// Adjacent literal pieces as one, so `"a" + "b"` becomes `"ab"`, not `"a" "b"` pieces.
fn merge_literals(parts: Vec<TextPart>) -> Vec<TextPart> {
    let mut out: Vec<TextPart> = Vec::new();
    for p in parts {
        match (out.last_mut(), p) {
            (Some(TextPart::Lit(a)), TextPart::Lit(b)) => a.push_str(&b),
            (_, p) => out.push(p),
        }
    }
    out
}
