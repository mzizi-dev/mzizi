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
    Arm, BinOp, ElseArm, Expr, ExprKind, Fault, TextPart, Ty, UnOp, binary_type, canonical,
    canonical_text, fold, has_text_form, method_signature,
};
use crate::numbers;
use crate::resolve::nearest;

mod collections;
mod control;
mod errors;
mod text;

/// How `MZ0962`'s `say` opens for text emptiness asked another way (`s.length() is 0`), so
/// the front end can fold an operator idiom inside it into its one fix.
pub const EMPTINESS: &str = "text is not a collection";

pub use control::{canonical_stmts, variant_owner};
pub use errors::{Column, EnumDecl, Variant};

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
    /// Every `enum`, in source order (RFC-0013 §1, §7.2, §12.1).
    pub enums: Vec<EnumDecl>,
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
    /// Whether the parser skipped any of its lines unread, as one diagnostic already
    /// reported (a `do … while`, a C-style `for`, a form not built yet, a block past the
    /// nesting cap): what those lines do, such as assign a `var`, is not known.
    pub skipped: bool,
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
    /// `when cond` … [`else when cond` …]… [`else` …] `end`.
    When {
        /// The condition.
        cond: Expr,
        /// The branch taken when it holds.
        then: Vec<Stmt>,
        /// Each `else when`, in order (RFC-0013 §7.1). A flat list, so a long chain
        /// costs no nesting.
        else_whens: Vec<ElseWhen>,
        /// The `else` branch, if written.
        otherwise: Option<Vec<Stmt>>,
    },
    /// `match <expr>` … `case …` … [`else` …] `end` (RFC-0013 §7.2), and over a result
    /// (§12.2), whose `case ok <name>` and `case error <name>` carry [`Arm::binding`] and
    /// take no `else`.
    Match {
        /// The value matched.
        scrutinee: Expr,
        /// Each `case`, with its statements.
        arms: Vec<Arm<Vec<Stmt>>>,
        /// The `else`, if written.
        otherwise: Option<ElseArm<Vec<Stmt>>>,
    },
    /// `for each <name> in <source>` … `end` (RFC-0013 §7.3).
    For {
        /// The loop binding.
        name: String,
        /// Where the binding's name is.
        name_span: Span,
        /// What is iterated: `range(a, to = b)` in this slice.
        source: Expr,
        /// The loop's body.
        body: Vec<Stmt>,
    },
    /// `while <cond>` … `end` (RFC-0013 §7.3).
    While {
        /// The condition, tested before each pass.
        cond: Expr,
        /// The loop's body.
        body: Vec<Stmt>,
    },
    /// `name[index] = value`: replaces a list's element, or inserts or replaces a map's
    /// value (RFC-0013 §9.2). On a list it traps out of range (§4.3).
    IndexAssign {
        /// The `var` changed.
        name: String,
        /// Where the name is.
        name_span: Span,
        /// The position or key.
        index: Expr,
        /// The new value.
        value: Expr,
    },
    /// `break`: leaves the innermost loop.
    Break,
    /// `continue`: starts the innermost loop's next pass.
    Continue,
    /// An expression on its own line: a call.
    Expr(Expr),
}

/// `else when <cond>` and its branch (RFC-0013 §7.1).
#[derive(Clone, Debug, PartialEq)]
pub struct ElseWhen {
    /// The condition.
    pub cond: Expr,
    /// The branch taken when it holds and no earlier condition did.
    pub body: Vec<Stmt>,
    /// The `else when` line, from `else` to the end of the condition.
    pub span: Span,
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
            if let Some(ret) = first.ret
                && ret.ty != Ty::Error
                && !errors::returns_none_result(ret.ty)
            {
                diags.push(Diagnostic::error(
                    "MZ0902",
                    file,
                    ret.span,
                    format!(
                        "`fn main` returns nothing or `result(none, E)`, and this one declares `: {}` — a program's output is what it prints",
                        ret.ty.name()
                    ),
                ));
            }
        }
    }
    diags.extend(control::check_enums(&p.enums, &fns, file));
    for e in &p.enums {
        errors::check_columns(e, file, &mut diags);
    }
    for f in &p.fns {
        let mut cx = FnCheck::new(f, &fns, &p.enums, file);
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
    /// A `for each` binding, which takes each value in turn (RFC-0013 §7.3).
    Loop,
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
    /// Whether it was ever read: a `let` holding a result must be (`MZ0950`, §12.3).
    read: bool,
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
    /// A `for each` binding, which no `var` declared before its loop can stand in for.
    loop_binding: bool,
}

struct FnCheck<'a> {
    f: &'a FnDecl,
    fns: &'a BTreeMap<&'a str, &'a FnDecl>,
    /// The program's enums, whose variants a bare name may be (RFC-0008 §5).
    enums: &'a [EnumDecl],
    /// How many loops enclose the statement being checked (`MZ0935` at none).
    loops: usize,
    /// Where each statement `match` reported as missing a case (`MZ0930`) starts, by its
    /// value's line and column: it can fall through, which [`control::exits`] reads.
    partial: Vec<(u32, u32)>,
    file: &'a str,
    diags: Vec<Diagnostic>,
    bindings: Vec<Binding>,
    scopes: Vec<Vec<usize>>,
    sites: Vec<Site>,
    whens: Vec<(u32, u32)>,
    /// Lines of every assignment in the function, by name, for `MZ0923`'s `let` or `var`.
    assigns: BTreeMap<String, Vec<u32>>,
    /// Each `let` that holds a result: its binding, its name and its value, so one never
    /// read before its block ends is `MZ0950` (§12.3).
    results: Vec<(usize, Span, Span)>,
    /// Each `x.is_empty()` written as the operand of a comparison or of `not`, whose fix to
    /// `x is ""` needs parentheses there (`program/text.rs`).
    tight: Vec<Span>,
    /// How many `{…}` interpolations enclose the expression being checked: a fix that
    /// writes a string literal cannot go inside one (RFC-0013 §3.6).
    interp: u32,
}

impl<'a> FnCheck<'a> {
    fn new(
        f: &'a FnDecl,
        fns: &'a BTreeMap<&'a str, &'a FnDecl>,
        enums: &'a [EnumDecl],
        file: &'a str,
    ) -> Self {
        let mut cx = FnCheck {
            f,
            fns,
            enums,
            loops: 0,
            partial: Vec::new(),
            file,
            diags: Vec::new(),
            bindings: Vec::new(),
            scopes: vec![Vec::new()],
            sites: Vec::new(),
            whens: Vec::new(),
            assigns: BTreeMap::new(),
            results: Vec::new(),
            tight: Vec::new(),
            interp: 0,
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
                    loop_binding: false,
                }),
                StmtKind::Assign {
                    name, name_span, ..
                } => self
                    .assigns
                    .entry(name.clone())
                    .or_default()
                    .push(name_span.start_line),
                StmtKind::When {
                    then,
                    else_whens,
                    otherwise,
                    ..
                } => {
                    whens.push((s.span.start_line, s.span.start_col));
                    self.collect(then, whens);
                    for w in else_whens {
                        self.collect(&w.body, whens);
                    }
                    if let Some(o) = otherwise {
                        self.collect(o, whens);
                    }
                    whens.pop();
                }
                StmtKind::Match {
                    arms, otherwise, ..
                } => {
                    whens.push((s.span.start_line, s.span.start_col));
                    for a in arms {
                        if let Some((name, span)) = &a.binding {
                            self.sites.push(Site {
                                name: name.clone(),
                                line: span.start_line,
                                ty: Ty::Error,
                                whens: whens.clone(),
                                loop_binding: false,
                            });
                        }
                        self.collect(&a.body, whens);
                    }
                    if let Some(o) = otherwise {
                        self.collect(&o.body, whens);
                    }
                    whens.pop();
                }
                StmtKind::For {
                    name,
                    name_span,
                    body,
                    ..
                } => {
                    whens.push((s.span.start_line, s.span.start_col));
                    self.sites.push(Site {
                        name: name.clone(),
                        line: name_span.start_line,
                        ty: Ty::Int,
                        whens: whens.clone(),
                        loop_binding: true,
                    });
                    self.collect(body, whens);
                    whens.pop();
                }
                StmtKind::While { body, .. } => {
                    whens.push((s.span.start_line, s.span.start_col));
                    self.collect(body, whens);
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
            } else if let Some(why) = self.program_name(&p.name) {
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
            let mut ty = p.ty.ty;
            if ty.as_result().is_some() {
                // §2: a result is a return type, never a parameter's.
                self.err(
                    "MZ0950",
                    p.ty.span,
                    format!(
                        "parameter `{}` is a {} — a result is matched or propagated where it is made, and its value passed on",
                        p.name,
                        ty.name()
                    ),
                );
                ty = Ty::Error;
            }
            self.bind(&p.name, Kind::Param, ty, None);
        }
        // `result(none, E)` returns success by reaching `end fn` (§12.1).
        let must_return = f.ret.is_some_and(|r| !errors::returns_none_result(r.ty));
        // The Rust habit: a body whose last line is a value of the return type, with no
        // `return`. That is `MZ0906` with an `exact` fix, and the line is not also `MZ0916`.
        let tail_value = match (must_return, f.body.last()) {
            (
                true,
                Some(Stmt {
                    kind: StmtKind::Expr(e),
                    ..
                }),
            ) if !terminates(&f.body) => Some(e.span),
            _ => None,
        };
        self.block(&f.body, tail_value);
        self.reachability(&f.body);
        let top = self.scopes.first().cloned().unwrap_or_default();
        self.unmatched_results(&top);
        // A return type already reported as unknown says nothing more: there is no type to
        // return, and no `return` to insert.
        if let Some(ret) = f.ret
            && must_return
            && ret.ty != Ty::Error
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
                }) if fits_return(self.shallow_ty(e), ret.ty) => Some(e.span),
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
        // `var`s never reassigned: one intent, one form (RFC-0013 §5.1). Not when lines of
        // the function were skipped unread, which may assign one: the warning and its
        // `exact` fix would then be wrong, and they return once those lines are fixed.
        if f.skipped {
            return;
        }
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
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => numeric(self.shallow_ty(operand), Ty::Int),
            ExprKind::Binary { op, lhs, rhs, .. } if op.is_arithmetic() => {
                numeric(self.shallow_ty(lhs), self.shallow_ty(rhs))
            }
            ExprKind::Method { recv, name, .. } => {
                method_signature(self.shallow_ty(recv), name).map_or(Ty::Error, |(_, ret)| ret)
            }
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
            read: false,
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
        self.end_scope();
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
        if let Some(why) = self.program_name(name) {
            self.err(
                "MZ0921",
                span,
                format!("`{name}` cannot name a binding: {why}"),
            );
            return false;
        }
        if BUILT_IN_NAMES.contains(&name) || self.fns.contains_key(name) {
            let what = if self.fns.contains_key(name) {
                "a function of this program".to_string()
            } else {
                "a built-in".to_string()
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
                Kind::Loop => "a loop binding",
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
        // A `when` used as a value is a chain too (RFC-0013 §7.1, §7.4: `MZ0936`).
        if let StmtKind::Bind { value, .. }
        | StmtKind::Assign { value, .. }
        | StmtKind::Return(Some(value)) = &s.kind
        {
            self.value_variant_chain(s, value);
        }
        match &s.kind {
            StmtKind::Bind {
                mutable,
                kw_span,
                name,
                name_span,
                ty,
                value,
            } => {
                let vt = match ty {
                    Some(t) => self.expr_want(value, t.ty),
                    None => self.expr(value),
                };
                // `let x: int = f()` with `f` a result: unhandled (§12.3), not a mismatch.
                let vt = match ty {
                    Some(t) if t.ty != vt && t.ty.as_result().is_none() => {
                        if self.unhandled(value, vt, || {
                            format!("`{name}` is declared {}", t.ty.name())
                        }) {
                            Ty::Error
                        } else {
                            vt
                        }
                    }
                    _ => vt,
                };
                let mut bound = self.binding_type(name, ty.as_ref(), vt, value.span);
                // A `var` already reported for holding a result is not also `MZ0924`: the
                // one fix, `let`, answers both.
                let mut kw = Some(*kw_span);
                if *mutable && bound.as_result().is_some() {
                    // §12.3: a `let` may hold a result until it is matched; a `var` may not.
                    self.err_fix(
                        "MZ0950",
                        *kw_span,
                        format!(
                            "`var {name}` would hold a {} — a result is held by a `let` until it is matched or propagated",
                            bound.name()
                        ),
                        *kw_span,
                        "let",
                        Confidence::Guess,
                    );
                    bound = Ty::Error;
                    kw = None;
                }
                // The checked type replaces the shape's guess, so a later `MZ0920` fix that
                // hoists this binding declares it with the right zero (`0.0` for `a * b` on
                // floats, which the shape alone cannot tell from ints).
                if bound != Ty::Error
                    && let Some(site) = self
                        .sites
                        .iter_mut()
                        .find(|s| s.name == *name && s.line == name_span.start_line)
                {
                    site.ty = bound;
                }
                if self.may_bind(name, *name_span) {
                    let kind = if *mutable { Kind::Var } else { Kind::Let };
                    self.bind(name, kind, bound, kw);
                    if bound.as_result().is_some() {
                        let i = self.bindings.len() - 1;
                        self.results.push((i, *name_span, value.span));
                    }
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
                let want = self
                    .visible(name)
                    .map_or(Ty::Error, |i| self.bindings[i].ty);
                let vt = self.expr_want(value, want);
                // An option assigned to a `var` that holds one is not used un-narrowed.
                let vt = if matches!(vt, Ty::Option(_)) && vt == want {
                    vt
                } else if self.unhandled(value, vt, || "an assignment stores it unexamined".into())
                {
                    Ty::Error
                } else {
                    vt
                };
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
                                if !bt.has_error() && !vt.has_error() && bt != vt {
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
                            Kind::Loop => self.err(
                                "MZ0922",
                                *name_span,
                                format!(
                                    "`{name}` is a `for each` binding, which takes each value in turn and cannot be assigned — bind a `var` with its value instead"
                                ),
                            ),
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
                    (None, Some(r)) if errors::returns_none_result(r.ty) => {}
                    (Some(v), Some(r)) if r.ty.as_result().is_some() => {
                        self.return_result(v, r.ty);
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
                        let t = self.expr_want(v, r.ty);
                        let how = || format!("`{}` returns {}", f.signature(), r.ty.name());
                        if self.unhandled(v, t, how) {
                            // Reported: a result returned where its type does not fit.
                        } else if !t.has_error() && !r.ty.has_error() && t != r.ty {
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
                else_whens,
                otherwise,
            } => {
                let t = self.expr(cond);
                self.condition(cond, t, "when");
                for w in else_whens {
                    let t = self.expr(&w.cond);
                    self.condition(&w.cond, t, "else when");
                }
                self.variant_chain(s, cond, else_whens, otherwise.as_deref());
                self.whens.push((s.span.start_line, s.span.start_col));
                self.scoped(then);
                for w in else_whens {
                    self.scoped(&w.body);
                }
                if let Some(o) = otherwise {
                    self.scoped(o);
                }
                self.whens.pop();
            }
            StmtKind::IndexAssign {
                name,
                name_span,
                index,
                value,
            } => self.index_assign(name, *name_span, index, value),
            StmtKind::Match { .. }
            | StmtKind::For { .. }
            | StmtKind::While { .. }
            | StmtKind::Break
            | StmtKind::Continue => self.control(s),
            StmtKind::Expr(e) => {
                let t = self.expr(e);
                let is_tail = tail_value == Some(e.span);
                // A tail value is `MZ0906`'s, whose fix inserts `return`; a result there is
                // not also `MZ0950`.
                if !is_tail && self.unhandled(e, t, || "nothing reads it".into()) {
                    // Reported: a discarded result is `MZ0950`, not `MZ0916`.
                } else if !matches!(t, Ty::Nothing | Ty::Error) && !is_tail {
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

    /// `MZ0712`: a condition that is not a `bool`. Mzizi has no truthiness, so the fix
    /// names the question to ask.
    fn condition(&mut self, cond: &Expr, t: Ty, word: &str) {
        if t == Ty::Bool || t == Ty::Error {
            return;
        }
        // A result is not a condition (§12.3): `MZ0950`, with `try` where it propagates and
        // its success is a `bool`.
        if self.unhandled_wanting(cond, t, Some(Ty::Bool), || format!("`{word}` needs a bool")) {
            return;
        }
        let question = match t {
            Ty::Int => Some(format!("{} is not 0", canonical(cond))),
            Ty::Float => Some(format!("{} is not 0.0", canonical(cond))),
            Ty::Text => Some(format!("{} is not \"\"", canonical(cond))),
            t if t.is_collection() => Some(format!("{} is not none", canonical(cond))),
            _ => None,
        };
        let say = format!(
            "`{word} {}` needs a bool, and `{}` is {} — Mzizi has no truthiness, so ask the question",
            canonical(cond),
            canonical(cond),
            t.name()
        );
        match question {
            Some(q) => self.err_fix("MZ0712", cond.span, say, cond.span, q, Confidence::Guess),
            None => self.err("MZ0712", cond.span, say),
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
                if !t.ty.has_error() && !vt.has_error() && t.ty != vt {
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
        let (line, ty, whens, loop_binding) =
            (site.line, site.ty, site.whens.clone(), site.loop_binding);
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
            Ty::Float => Some("0.0"),
            Ty::Bool => Some("false"),
            Ty::Text => Some("\"\""),
            _ => None,
        };
        let say = format!(
            "`{name}` was bound on line {line}, in a block that ended before this line — declare `var {name}` before that block and assign it inside"
        );
        if loop_binding {
            // A `var` of the same name before the loop would shadow-clash with the loop's
            // binding (`MZ0921`), so there is no fix to offer, only the repair to name.
            self.err(
                "MZ0920",
                at,
                format!(
                    "`{name}` is the `for each` binding on line {line}, visible only inside its loop — to read it after the loop, keep a `var` with another name and assign it inside"
                ),
            );
            return true;
        }
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
        if let Some(t) = self.text_emptiness(e) {
            return t;
        }
        match &e.kind {
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Float(_) => Ty::Float,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Text(parts) => {
                for part in parts {
                    if let TextPart::Expr(inner) = part {
                        self.interp += 1;
                        let t = self.expr(inner);
                        self.interp -= 1;
                        if self.unhandled(inner, t, || "a result has no text form".into()) {
                            // Reported.
                        } else if !has_text_form(t) {
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
            ExprKind::Method {
                recv,
                name,
                name_span,
                args,
                called,
                ..
            } => self.method(e, recv, name, *name_span, args, *called),
            ExprKind::Unary {
                op: UnOp::Try,
                operand,
            } => self.try_expr(e, operand),
            ExprKind::Field {
                base,
                name,
                name_span,
            } => self.field(e, base, name, *name_span),
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
            } if self.is_empty_call(operand) => match self.not_is_empty(e, operand) {
                Some(t) => t,
                None => self.method_expr(operand),
            },
            ExprKind::Unary { op, operand } => {
                if *op == UnOp::Not {
                    self.mark_tight(operand);
                }
                let t = self.expr(operand);
                // `try` is typed by `try_expr`, in the arm above; this is `-` or `not`.
                let want = if *op == UnOp::Not {
                    Ty::Bool
                } else if t == Ty::Float {
                    Ty::Float
                } else {
                    Ty::Int
                };
                let how = || format!("`{}` needs its success value", canonical(e));
                if self.unhandled(operand, t, how) {
                    return Ty::Error;
                }
                if t != want && t != Ty::Error {
                    let (word, kind) = if *op == UnOp::Not {
                        ("not", "a bool")
                    } else {
                        ("-", "an int or a float")
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
            ExprKind::Binary {
                op: BinOp::In,
                lhs,
                rhs,
                ..
            } => self.in_expr(e, lhs, rhs),
            ExprKind::Binary {
                op: BinOp::Otherwise,
                lhs,
                rhs,
                ..
            } => self.otherwise(e, lhs, rhs),
            ExprKind::List(_) | ExprKind::MapLit(_) => self.bracket(e, Ty::Nothing),
            ExprKind::Index { base, index } => self.index(e, base, index),
            ExprKind::None => self.none_value(e),
            ExprKind::Binary {
                op: op @ (BinOp::Is | BinOp::IsNot | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge),
                lhs,
                rhs,
                ..
            } => {
                self.mark_tight(lhs);
                self.mark_tight(rhs);
                self.compare(e, *op, lhs, rhs)
            }
            ExprKind::Variant { .. } | ExprKind::When { .. } | ExprKind::Match { .. } => {
                self.control_expr(e)
            }
            ExprKind::Binary { op, lhs, rhs, .. } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                self.binary(e, *op, lhs, rhs, l, r)
            }
            ExprKind::Error => Ty::Error,
        }
    }

    /// Whether `e` is `x.is_empty()`, whose negation is one idiom (`MZ0962`).
    fn is_empty_call(&self, e: &Expr) -> bool {
        matches!(&e.kind, ExprKind::Method { name, args, called: true, .. } if name == "is_empty" && args.is_empty())
    }

    /// [`Self::expr`] on a method call, named so a caller's arm reads.
    fn method_expr(&mut self, e: &Expr) -> Ty {
        let t = self.expr(e);
        if t != Ty::Bool && !t.has_error() {
            self.err(
                "MZ0912",
                e.span,
                format!("`not` takes a bool, and `{}` is {}", canonical(e), t.name()),
            );
        }
        Ty::Bool
    }

    /// The type of a binary expression whose operands are typed, with its diagnostics.
    fn binary(&mut self, e: &Expr, op: BinOp, lhs: &Expr, rhs: &Expr, l: Ty, r: Ty) -> Ty {
        let how = || format!("`{}` needs its success value", op.text());
        // `otherwise` takes an option on its left, and checked its sides itself.
        if op != BinOp::Otherwise && (self.unhandled(lhs, l, how) | self.unhandled(rhs, r, how)) {
            return if op.is_arithmetic() {
                Ty::Error
            } else {
                Ty::Bool
            };
        }
        match binary_type(op, l, r) {
            Ok(t) => {
                if t == Ty::Int {
                    self.constant_fault(e, op, lhs, rhs);
                }
                t
            }
            Err(why) if matches!(op, BinOp::In | BinOp::Otherwise) => {
                self.err("MZ0912", e.span, format!("`{}`: {why}", canonical(e)));
                if op == BinOp::In { Ty::Bool } else { Ty::Error }
            }
            Err(why) => {
                // `int` with `float`: no implicit conversion (RFC-0013 §3.2). The fix
                // converts the `int` side.
                let int_side = match (l, r) {
                    (Ty::Int, Ty::Float) => Some(lhs),
                    (Ty::Float, Ty::Int) => Some(rhs),
                    _ => None,
                };
                match int_side.and_then(|side| to_float_fix(side).map(|f| (side.span, f))) {
                    Some((at, (text, c))) => self.err_fix(
                        "MZ0912",
                        e.span,
                        format!(
                            "`{}`: {why} — Mzizi never converts between them implicitly; write `{text}`",
                            canonical(e)
                        ),
                        at,
                        text,
                        c,
                    ),
                    None => self.err("MZ0912", e.span, format!("`{}`: {why}", canonical(e))),
                }
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
        let mut types: Vec<Ty> = leaves.iter().map(|l| self.expr(l)).collect();
        for (leaf, t) in leaves.iter().zip(types.iter_mut()) {
            if self.unhandled(leaf, *t, || "`+` needs its success value".into()) {
                *t = Ty::Error;
            }
        }
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
            self.bindings[i].read = true;
            return self.bindings[i].ty;
        }
        if let Some(t) = self.bare_variant(name, at) {
            self.bare_ok_error(name, at, t);
            return t;
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
        if !self.fns.contains_key(name) && self.visible(name).is_none() {
            if name == "range" {
                return self.range_value(at, args);
            }
            if let Some(t) = self.free_collection(name, args, at) {
                return t;
            }
        }
        // `error(e)`'s one argument is read against the function's error type (§12.1), so a
        // bare variant of the error enum resolves there.
        let params: Vec<Ty> = if name == "error" {
            self.ret_result()
                .map(|(_, err)| vec![err])
                .unwrap_or_default()
        } else {
            self.fns
                .get(name)
                .map(|f| f.params.iter().map(|p| p.ty.ty).collect())
                .unwrap_or_default()
        };
        let mut types: Vec<Ty> = args
            .iter()
            .enumerate()
            .map(|(k, a)| {
                // `print` expects nothing in particular, so `print([])` has no type to take
                // (`MZ0961`); an unknown function's arguments were expected as something
                // already reported.
                let none = if name == "print" {
                    Ty::Nothing
                } else {
                    Ty::Error
                };
                self.expr_want(a, params.get(k).copied().unwrap_or(none))
            })
            .collect();
        if name == "error" {
            return self.fail_value(args, &types, at);
        }
        // A result is matched or propagated, never passed on (§12.3). A parameter declared
        // as a result was reported at the parameter, so its arguments say nothing more.
        for (k, (a, t)) in args.iter().zip(types.iter_mut()).enumerate() {
            if params.get(k).is_some_and(|p| p.as_result().is_some()) {
                *t = Ty::Error;
                continue;
            }
            if self.unhandled(a, *t, || format!("`{name}` takes the value, not a result")) {
                *t = Ty::Error;
            }
        }
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
        if !self.fns.contains_key(name)
            && self.visible(name).is_none()
            && let Some(t) = self
                .free_text(name, args, &types, at)
                .or_else(|| self.free_numeric(name, args, &types, at))
        {
            return t;
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
            // A result parameter is `MZ0950` at the parameter already.
            if !t.has_error()
                && !p.ty.ty.has_error()
                && p.ty.ty.as_result().is_none()
                && *t != p.ty.ty
            {
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

    /// Python's free numeric functions, `float(x)`, `int(x)`, `abs(x)`, `round(x)`,
    /// `min(a, b)`, `pow(a, b)` and the rest: in Mzizi each is a method (RFC-0013 §3.7,
    /// §4.4), so `MZ0962` with the `exact` method call when the types fit. `str(x)` and
    /// `String(x)` are `MZ0962` too, with the `exact` fix `"{x}"` (§3.6). `None` when
    /// `name` is not one of them, so the call is checked as any other.
    fn free_numeric(&mut self, name: &str, args: &[Expr], types: &[Ty], at: Span) -> Option<Ty> {
        if matches!(name, "str" | "string" | "String") && args.len() == 1 {
            // `str(x)`, `String(x)`: a value's text is interpolation (RFC-0013 §3.6).
            let fixed = if types[0] == Ty::Text {
                // Already text: the conversion has nothing to do.
                canonical(&args[0])
            } else {
                format!("\"{{{}}}\"", canonical(&args[0]))
            };
            let mut say =
                format!("`{name}(…)` is not a Mzizi function — a value's text is `\"{{x}}\"`");
            if args[0].has_error() {
                self.err("MZ0962", at, say);
            } else if !has_text_form(types[0]) {
                // A value with no text form cannot be interpolated either: no fix is safe.
                say.push_str(&format!(", and {} has no text form", types[0].name()));
                self.err("MZ0962", at, say);
            } else if types[0] != Ty::Text && args[0].has_text_literal() {
                say.push_str(
                    ", and an interpolation holds no string literal: bind the value with `let` first",
                );
                self.err("MZ0962", at, say);
            } else {
                say = format!("{say}: write `{fixed}`");
                self.err_fix("MZ0962", at, say, at, fixed, Confidence::Exact);
            }
            return Some(Ty::Text);
        }
        let method = match name {
            "float" => "to_float",
            "int" => "to_int",
            "abs" | "round" | "floor" | "ceil" | "sqrt" | "min" | "max" | "pow" => name,
            _ => return None,
        };
        let (recv, rest) = args.split_first()?;
        let rt = types[0];
        if rt == Ty::Error {
            return Some(Ty::Error);
        }
        let r = receiver_text(recv);
        let fits = |params: &[Ty]| {
            params.len() == rest.len()
                && params
                    .iter()
                    .zip(&types[1..])
                    .all(|(p, t)| p == t || *t == Ty::Error)
        };
        let (fixed, ty) = match numbers::method(rt, method) {
            Some((params, ret)) if fits(&params) => {
                let rest: Vec<String> = rest.iter().map(canonical).collect();
                (format!("{r}.{method}({})", rest.join(", ")), ret)
            }
            // `float(x)` on a float, `int(n)` on an int: there is nothing to convert.
            None if rest.is_empty()
                && ((method, rt) == ("to_float", Ty::Float)
                    || (method, rt) == ("to_int", Ty::Int)) =>
            {
                (canonical(recv), rt)
            }
            _ => {
                self.err(
                    "MZ0962",
                    at,
                    format!(
                        "`{name}(…)` is not a Mzizi function — an operation on a value is a method, and {} has {}",
                        rt.name(),
                        method_list(rt)
                    ),
                );
                return Some(Ty::Error);
            }
        };
        let say = format!(
            "`{name}(…)` is not a Mzizi function — an operation on a value is a method: `{fixed}`"
        );
        // A fix that changes behaviour is a `guess` (RFC-0013 §16). Python's `round` breaks
        // ties to even and JavaScript's `Math.round` breaks them upward, where `.round()`
        // rounds half away from zero; Python's `pow(2, -1)` is `0.5`, where `int.pow` with a
        // negative exponent traps. So `round` is always a guess, and `pow` is exact only for
        // an exponent that is a non-negative `int` literal.
        let preserves = match method {
            "round" => false,
            "pow" => matches!(rest.first().map(|e| &e.kind), Some(ExprKind::Int(n)) if *n >= 0),
            _ => true,
        };
        let confidence = if preserves {
            Confidence::Exact
        } else {
            Confidence::Guess
        };
        if args.iter().any(Expr::has_error) {
            self.err("MZ0962", at, say);
        } else {
            self.err_fix("MZ0962", at, say, at, fixed, confidence);
        }
        Some(ty)
    }

    /// `recv.name(args)`: the numeric methods of RFC-0013 §4.4. Methods on text, lists and
    /// records are later waves' (`MZ0919`).
    fn method(
        &mut self,
        e: &Expr,
        recv: &Expr,
        name: &str,
        name_span: Span,
        args: &[Expr],
        called: bool,
    ) -> Ty {
        let rt = self.expr(recv);
        self.method_on(e, recv, rt, name, name_span, args, called)
    }

    /// [`Self::method`], on a receiver already typed `rt`: also `recv.name` with no
    /// parentheses, which the parser reads as a dotted path (`errors::field`) and hands here
    /// when `recv` is a number.
    #[allow(clippy::too_many_arguments)]
    fn method_on(
        &mut self,
        e: &Expr,
        recv: &Expr,
        rt: Ty,
        name: &str,
        name_span: Span,
        args: &[Expr],
        called: bool,
    ) -> Ty {
        if rt.is_collection() || matches!(rt, Ty::Option(_)) {
            return self.collection_method(e, recv, rt, name, name_span, args, called);
        }
        let types: Vec<Ty> = args.iter().map(|a| self.expr(a)).collect();
        // `x.to_string()`: a value's text is interpolation (RFC-0013 §3.6), on any value
        // with a text form; on a `text` it is the value itself.
        if name == "to_string" && called && args.is_empty() && has_text_form(rt) {
            if rt == Ty::Error {
                return Ty::Error;
            }
            let fixed = if rt == Ty::Text {
                canonical(recv)
            } else {
                format!("\"{{{}}}\"", canonical(recv))
            };
            let say =
                "`.to_string()` is not a Mzizi method — a value's text is `\"{x}\"`".to_string();
            if recv.has_error() || (rt != Ty::Text && recv.has_text_literal()) {
                self.err("MZ0962", e.span, say);
            } else {
                let say = format!("{say}: write `{fixed}`");
                self.err_fix("MZ0962", e.span, say, e.span, fixed, Confidence::Exact);
            }
            return Ty::Text;
        }
        match rt {
            Ty::Error => return Ty::Error,
            Ty::Int | Ty::Float => {}
            Ty::Text => return self.text_method(e, recv, name, name_span, args, &types, called),
            Ty::Enum(_) => {
                self.err(
                    "MZ0708",
                    name_span,
                    format!(
                        "`{}` is {}, which has no method `{name}` — an enum's columns are read without parentheses, `{}.<column>`",
                        canonical(recv),
                        rt.name(),
                        canonical(recv)
                    ),
                );
                return Ty::Error;
            }
            Ty::Result(_) => return self.method_on_result(e, recv, rt, name, args, called),
            // Dispatched above, to the collections' methods.
            Ty::List(_) | Ty::Option(_) | Ty::Map(_) | Ty::Set(_) => return Ty::Error,
            Ty::Bool | Ty::Nothing => {
                self.err(
                    "MZ0708",
                    name_span,
                    format!(
                        "`{}` is {}, which has no method `{name}` — only numbers and text have methods in a program yet",
                        canonical(recv),
                        rt.name()
                    ),
                );
                return Ty::Error;
            }
        }
        let r = receiver_text(recv);
        let Some((params, ret)) = numbers::method(rt, name) else {
            return self.no_such_method(e, rt, &r, name, name_span, args, called);
        };
        if self.method_shape(e, rt, &r, name, name_span, &params, args.len(), called) {
            return ret;
        }
        for ((a, t), p) in args.iter().zip(&types).zip(&params) {
            if *t == Ty::Error || t == p {
                continue;
            }
            match (*t, *p) {
                // `x.min(1)` on a float, `n.min(1.5)` on an int: the `int` side converts.
                (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int) if name != "pow" => {
                    let int_side = if *t == Ty::Int { a } else { recv };
                    let why = format!(
                        "`{}`: `.{name}` takes two values of one type, and this is {} and {} — Mzizi never converts between them implicitly",
                        canonical(e),
                        rt.name(),
                        t.name()
                    );
                    match to_float_fix(int_side) {
                        Some((text, c)) => {
                            self.err_fix("MZ0912", e.span, why, int_side.span, text, c)
                        }
                        None => self.err("MZ0912", e.span, why),
                    }
                    return Ty::Error;
                }
                _ => {
                    let say = format!(
                        "`{}` is {}, and the exponent of `.pow` is an int",
                        canonical(a),
                        t.name()
                    );
                    let say = if name == "pow" {
                        say
                    } else {
                        format!(
                            "`{}` is {}, and `.{name}` on {} takes {}",
                            canonical(a),
                            t.name(),
                            rt.name(),
                            p.name()
                        )
                    };
                    match (name, &a.kind) {
                        ("pow", ExprKind::Float(v)) if v.fract() == 0.0 && v.abs() < 9.0e15 => {
                            let int = format!("{}", *v as i64);
                            self.err_fix("MZ0905", a.span, say, a.span, int, Confidence::Guess)
                        }
                        // Python's `x ** 0.5` is a square root. Any other fractional or
                        // computed exponent has no `int` that means the same, so no fix:
                        // `e.to_int()` would truncate `0.5` to `0` and print `1.0`.
                        ("pow", ExprKind::Float(v))
                            if *v == 0.5 && rt == Ty::Float && !recv.has_error() =>
                        {
                            let fixed = format!("{}.sqrt()", receiver_text(recv));
                            self.err_fix("MZ0905", a.span, say, e.span, fixed, Confidence::Guess)
                        }
                        _ => self.err("MZ0905", a.span, say),
                    }
                    return ret;
                }
            }
        }
        // Faults the checker can see (RFC-0013 §4.4, `MZ0915`).
        if rt == Ty::Int {
            let negative = name == "pow"
                && args
                    .first()
                    .and_then(fold)
                    .is_some_and(|k| k.is_ok_and(|k| k < 0));
            let children_ok = fold(recv).is_some_and(|v| v.is_ok())
                && args.iter().all(|a| fold(a).is_some_and(|v| v.is_ok()));
            if negative {
                self.err(
                    "MZ0915",
                    e.span,
                    format!(
                        "`{}` has a negative exponent, which traps on an int every time — for a fraction, use a float: `{r}.to_float().pow(…)`",
                        canonical(e)
                    ),
                );
            } else if children_ok && fold(e) == Some(Err(Fault::Overflow)) {
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
        ret
    }

    /// A call of a built-in method `name` on a receiver of type `rt` (written `r`), whose
    /// signature takes `params`: written without parentheses (`MZ0962`, `exact` `()` when
    /// it takes nothing), or with the wrong number of arguments (`MZ0905`). True when it
    /// reported, so the caller says nothing more. Numbers and text share it (§4.4, §10).
    #[allow(clippy::too_many_arguments)]
    fn method_shape(
        &mut self,
        e: &Expr,
        rt: Ty,
        r: &str,
        name: &str,
        name_span: Span,
        params: &[Ty],
        argc: usize,
        called: bool,
    ) -> bool {
        if !called {
            let say = format!(
                "`.{name}` is a method, and a method is always called with parentheses: `{r}.{name}(…)`"
            );
            if params.is_empty() {
                // JavaScript's `s.length` counts UTF-16 code units, Mzizi's `s.length()`
                // Unicode scalar values (§10): `"🙂".length` is 2 there and 1 here, so
                // that fix may change what the program means, and is a guess.
                let c = if rt == Ty::Text && name == "length" {
                    Confidence::Guess
                } else {
                    Confidence::Exact
                };
                self.err_fix(
                    "MZ0962",
                    name_span,
                    say,
                    Span::single(name_span.end_line, name_span.end_col, 0),
                    "()",
                    c,
                );
            } else {
                self.err("MZ0962", name_span, say);
            }
            return true;
        }
        if argc != params.len() {
            let wanted: Vec<&str> = params.iter().map(|p| p.name()).collect();
            self.err(
                "MZ0905",
                e.span,
                format!(
                    "`.{name}` on {} takes {} argument{}{}, and this call gives {argc}",
                    rt.name(),
                    params.len(),
                    if params.len() == 1 { "" } else { "s" },
                    if wanted.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", wanted.join(", "))
                    },
                ),
            );
            return true;
        }
        false
    }

    /// `MZ0708`: a method the receiver's type does not have. When the other numeric type
    /// has it, the fix converts (`n.sqrt()` on an int); otherwise the nearest name.
    #[allow(clippy::too_many_arguments)]
    fn no_such_method(
        &mut self,
        e: &Expr,
        rt: Ty,
        r: &str,
        name: &str,
        name_span: Span,
        args: &[Expr],
        called: bool,
    ) -> Ty {
        let other = if rt == Ty::Int { Ty::Float } else { Ty::Int };
        if let Some((_, other_ret)) = numbers::method(other, name) {
            let args: Vec<String> = args.iter().map(canonical).collect();
            let (say, fixed, ty) = match name {
                "to_float" | "to_int" => (
                    format!(
                        "`{r}` is already {}, so `.{name}()` has nothing to convert — delete it",
                        rt.name()
                    ),
                    r.to_string(),
                    rt,
                ),
                _ => (
                    format!(
                        "`.{name}` is a method of float, and `{r}` is an int — convert it first: `{r}.to_float().{name}(…)`"
                    ),
                    format!("{r}.to_float().{name}({})", args.join(", ")),
                    other_ret,
                ),
            };
            if called && !e.has_error() {
                self.err_fix("MZ0708", name_span, say, e.span, fixed, Confidence::Guess);
            } else {
                self.err("MZ0708", name_span, say);
            }
            return ty;
        }
        match nearest(name, numbers::methods_of(rt)) {
            Some((near, _)) => self.err_fix(
                "MZ0708",
                name_span,
                format!(
                    "{} has no method `{name}` — did you mean `{near}`?",
                    rt.name()
                ),
                name_span,
                near,
                Confidence::Guess,
            ),
            None => self.err(
                "MZ0708",
                name_span,
                format!(
                    "{} has no method `{name}` — it has {}",
                    rt.name(),
                    method_list(rt)
                ),
            ),
        }
        Ty::Error
    }

    /// `MZ0907`: a statement after a `return`, `break` or `continue` on the same path can
    /// never run. One diagnostic per block, whose `exact` fix deletes every such line.
    fn reachability(&mut self, stmts: &[Stmt]) {
        let mut done = false;
        for s in stmts {
            if done {
                let last = stmts.last().map_or(s.last_line, |l| l.last_line);
                self.err_fix(
                    "MZ0907",
                    s.span,
                    "this line comes after a `return`, `break` or `continue` on every path, so it can never run — delete it",
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
            // A loop's body is where `break` and `continue` end a path; outside every loop
            // they are `MZ0935`, and the line after one is not also `MZ0907`.
            let is_loop = matches!(s.kind, StmtKind::For { .. } | StmtKind::While { .. });
            self.loops += usize::from(is_loop);
            for block in control::blocks(s) {
                self.reachability(block);
            }
            self.loops -= usize::from(is_loop);
            done = control::exits(std::slice::from_ref(s), &self.partial, self.loops > 0);
        }
    }
}

/// Whether every path through `stmts` ends in `return`, or in a `while true` that no
/// `break` leaves (RFC-0013 §6.2). A `match` without `else` counts as exhaustive here: one
/// that is not is `MZ0930`, and is not reported a second time as a path without `return`.
pub fn terminates(stmts: &[Stmt]) -> bool {
    control::ends(stmts, &[], false)
}

/// A type read off an expression's shape alone, for the sites collected before checking.
fn shallow_type(e: &Expr) -> Ty {
    match &e.kind {
        ExprKind::Int(_) => Ty::Int,
        ExprKind::Float(_) => Ty::Float,
        ExprKind::Bool(_) => Ty::Bool,
        ExprKind::Text(_) => Ty::Text,
        // An arithmetic result is a float when an operand visibly is, else an int.
        ExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } => numeric(shallow_type(operand), Ty::Int),
        ExprKind::Unary { op: UnOp::Not, .. } => Ty::Bool,
        ExprKind::Binary { op, lhs, rhs, .. } if op.is_arithmetic() => {
            numeric(shallow_type(lhs), shallow_type(rhs))
        }
        ExprKind::Binary { .. } => Ty::Bool,
        ExprKind::Method { recv, name, .. } => {
            let rt = shallow_type(recv);
            method_signature(rt, name).map_or(Ty::Error, |(_, ret)| ret)
        }
        _ => Ty::Error,
    }
}

/// Whether `return v`, with `v` of type `t`, fits a function returning `ret`: `t` is `ret`
/// itself, or `ret` is `result(T, E)` and `t` is its success type `T` (§12.1). Used only
/// with a known `ret`.
fn fits_return(t: Ty, ret: Ty) -> bool {
    t == ret
        || ret
            .as_result()
            .is_some_and(|(ok, _)| ok != Ty::Nothing && ok == t)
}

/// The type of arithmetic on operands of shallow types `a` and `b`: a float when either
/// visibly is, an int when both are, and unknown (`Error`) otherwise, so that a fix built on
/// it is withheld rather than wrong (`a * b` on two float names is not an int).
fn numeric(a: Ty, b: Ty) -> Ty {
    if a == Ty::Float || b == Ty::Float {
        Ty::Float
    } else if a == Ty::Int && b == Ty::Int {
        Ty::Int
    } else {
        Ty::Error
    }
}

/// An expression as the receiver of a method: in parentheses unless it is a primary or
/// already postfix (RFC-0013 §3.5).
fn receiver_text(e: &Expr) -> String {
    let c = canonical(e);
    if e.level() > 2 { format!("({c})") } else { c }
}

/// `MZ0912`'s fix for an `int` where a `float` is wanted (RFC-0013 §3.2): on an `int`
/// literal the `exact` `1.0`, on anything else the `guess` `x.to_float()`. `None` when the
/// expression holds a part the parser could not read.
fn to_float_fix(e: &Expr) -> Option<(String, Confidence)> {
    if e.has_error() {
        return None;
    }
    match &e.kind {
        ExprKind::Int(v) => Some((format!("{v}.0"), Confidence::Exact)),
        ExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } if matches!(operand.kind, ExprKind::Int(_)) => {
            Some((format!("{}.0", canonical(e)), Confidence::Exact))
        }
        _ => Some((
            format!("{}.to_float()", receiver_text(e)),
            Confidence::Guess,
        )),
    }
}

/// A numeric type's methods, as a diagnostic lists them.
fn method_list(t: Ty) -> String {
    let ms: Vec<String> = numbers::methods_of(t)
        .iter()
        .map(|m| format!("`{m}`"))
        .collect();
    if ms.is_empty() {
        "no methods".to_string()
    } else {
        format!("the methods {}", ms.join(", "))
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
