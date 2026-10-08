//! Checking RFC-0013 §7's control flow in a function body: `else when` chains over one
//! enum (`MZ0936`), `match` with exhaustiveness (`MZ0930`) and unreachable cases
//! (`MZ0931`), `when` and `match` used as values (`MZ0932`), `for each`, `while`, `break`
//! and `continue` (`MZ0935`), and the enums a `match` is over, whose bare variants resolve
//! as RFC-0008 §5 resolves them.
//!
//! Also here, because they walk the new statements: which statements end a path
//! ([`exits`]), a `while true` no `break` leaves ([`forever`]), and the canonical text of
//! statements (RFC-0013 §17), which `MZ0936`'s fix writes.

use std::collections::BTreeMap;

use super::{
    BUILT_IN_NAMES, Confidence, Diagnostic, EnumDecl, FnCheck, FnDecl, Kind, Span, Stmt, StmtKind,
    reserved_name,
};
use crate::expr::{Arm, ElseArm, Expr, ExprKind, TextPart, Ty, UnOp, canonical, fold, intern};
use crate::program::ElseWhen;
use crate::resolve::nearest;

/// Contextual words RFC-0013 §1 lets a variant take.
const VARIANT_WORDS: &[&str] = &["ok", "error", "float", "map", "set", "result"];

/// Which enum a bare variant name belongs to: `want` when it has the variant, else the one
/// enum that does. `Err` holds every enum that has it, so an empty list means it is no
/// variant at all, and two or more mean the name needs its enum (`a.x`).
pub fn variant_owner<'e>(
    enums: &'e [EnumDecl],
    name: &str,
    want: Option<&str>,
) -> Result<&'e str, Vec<&'e str>> {
    if let Some(w) = want
        && let Some(e) = enums.iter().find(|e| e.name == w && e.has(name))
    {
        return Ok(&e.name);
    }
    let owners: Vec<&str> = enums
        .iter()
        .filter(|e| e.has(name))
        .map(|e| e.name.as_str())
        .collect();
    match owners.as_slice() {
        [one] => Ok(one),
        _ => Err(owners),
    }
}

/// The names each enum declares: reserved words (`MZ0921`), an enum or variant declared
/// twice (`MZ0704`), and an enum that shares a `fn`'s name (`MZ0904`).
pub(super) fn check_enums(
    enums: &[EnumDecl],
    fns: &BTreeMap<&str, &FnDecl>,
    file: &str,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for e in enums {
        let n = e.name.as_str();
        if let Some(why) = reserved_name(n) {
            diags.push(Diagnostic::error(
                "MZ0921",
                file,
                e.name_span,
                format!("`enum {n}` cannot be named so: {why}"),
            ));
        } else if BUILT_IN_NAMES.contains(&n) {
            diags.push(Diagnostic::error(
                "MZ0921",
                file,
                e.name_span,
                format!(
                    "`enum {n}` reuses the name of a built-in type or function — choose another"
                ),
            ));
        } else if seen.contains(&n) {
            diags.push(Diagnostic::error(
                "MZ0704",
                file,
                e.name_span,
                format!("`enum {n}` is declared twice — rename one, or merge their variants"),
            ));
        } else if fns.contains_key(n) {
            diags.push(Diagnostic::error(
                "MZ0904",
                file,
                e.name_span,
                format!("`enum {n}` and `fn {n}` share a name — rename one"),
            ));
        }
        seen.push(n);
        let mut variants: Vec<&str> = Vec::new();
        for variant in &e.variants {
            let (v, at) = (variant.name.as_str(), &variant.span);
            let why = reserved_name(v).filter(|_| !VARIANT_WORDS.contains(&v));
            if let Some(why) = why {
                diags.push(Diagnostic::error(
                    "MZ0921",
                    file,
                    *at,
                    format!("variant `{v}` of `enum {n}` cannot be named so: {why}"),
                ));
            } else if BUILT_IN_NAMES.contains(&v) {
                diags.push(Diagnostic::error(
                    "MZ0921",
                    file,
                    *at,
                    format!("variant `{v}` of `enum {n}` reuses the name of a built-in type or function — choose another"),
                ));
            } else if variants.contains(&v) {
                diags.push(
                    Diagnostic::error(
                        "MZ0704",
                        file,
                        *at,
                        format!("variant `{v}` is listed twice in `enum {n}` — delete this line"),
                    )
                    .with_fix(
                        lines(at.start_line, at.start_line),
                        "",
                        Confidence::Exact,
                    ),
                );
            }
            variants.push(v);
        }
    }
    // A `fn` named like a variant (RFC-0013 §16, `MZ0921`): one diagnostic at the `fn`,
    // naming the first enum that has the variant. Uses of the name still read as the
    // variant wherever a variant fits, so the clash is reported here only.
    for (name, f) in fns {
        if let Some(e) = enums.iter().find(|e| e.has(name)) {
            diags.push(Diagnostic::error(
                "MZ0921",
                file,
                f.name_span,
                format!(
                    "`fn {name}` cannot be named so: `{name}` is a variant of `enum {}` — rename the function",
                    e.name
                ),
            ));
        }
    }
    diags
}

/// What a branch of a block used as a value is read against: `want`, or, when nothing is
/// expected, the type of the branches before it for a bracket literal, so that
/// `when c` / `[1]` / `else` / `[]` gives its `[]` a type (RFC-0013 §9.1).
fn branch_want(want: Ty, so_far: Option<Ty>, branch: &Expr) -> Ty {
    match (want, so_far) {
        (Ty::Nothing, Some(t))
            if matches!(branch.kind, ExprKind::List(_) | ExprKind::MapLit(_)) =>
        {
            t
        }
        _ => want,
    }
}

/// Whole lines `first` to `last`, with the last one's line ending, for a fix that deletes
/// them.
fn lines(first: u32, last: u32) -> Span {
    Span {
        start_line: first,
        start_col: 1,
        end_line: last + 1,
        end_col: 1,
    }
}

/// The statement lists nested directly inside `s`.
pub(super) fn blocks(s: &Stmt) -> Vec<&[Stmt]> {
    match &s.kind {
        StmtKind::When {
            then,
            else_whens,
            otherwise,
            ..
        } => {
            let mut out = vec![then.as_slice()];
            out.extend(else_whens.iter().map(|w| w.body.as_slice()));
            out.extend(otherwise.iter().map(|o| o.as_slice()));
            out
        }
        StmtKind::Match {
            arms, otherwise, ..
        } => {
            let mut out: Vec<&[Stmt]> = arms.iter().map(|a| a.body.as_slice()).collect();
            out.extend(otherwise.iter().map(|o| o.body.as_slice()));
            out
        }
        StmtKind::For { body, .. } | StmtKind::While { body, .. } => vec![body.as_slice()],
        _ => Vec::new(),
    }
}

/// Whether control never reaches the statement after `stmts`: every path ends in
/// `return`, `break` or `continue`, or in a `while true` that no `break` leaves. `partial`
/// holds where each `match` without `else` that may miss a case starts (one reported as
/// `MZ0930`, or whose cases could not be read): one of those can fall through, so the line
/// after it is not also `MZ0907`. `in_loop` says whether a loop holds `stmts`: outside
/// every loop, `break` and `continue` are `MZ0935` and end nothing.
pub(super) fn exits(stmts: &[Stmt], partial: &[(u32, u32)], in_loop: bool) -> bool {
    ends(stmts, partial, in_loop)
}

/// The one walk behind [`exits`] and [`super::terminates`]: whether every path through
/// `stmts` ends in `return`, or in a `while true` no `break` leaves, or, when `jumps` is
/// set, in `break` or `continue`. A `match` without `else` ends a path when its cases all do,
/// unless `partial` holds where it starts.
pub(super) fn ends(stmts: &[Stmt], partial: &[(u32, u32)], jumps: bool) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Return(_) => true,
        StmtKind::Break | StmtKind::Continue => jumps,
        StmtKind::When {
            then,
            else_whens,
            otherwise: Some(o),
            ..
        } => {
            ends(then, partial, jumps)
                && else_whens.iter().all(|w| ends(&w.body, partial, jumps))
                && ends(o, partial, jumps)
        }
        StmtKind::Match {
            scrutinee,
            arms,
            otherwise,
        } => {
            let at = (scrutinee.span.start_line, scrutinee.span.start_col);
            (otherwise.is_some() || !partial.contains(&at))
                && arms
                    .iter()
                    .all(|a| a.after_else || ends(&a.body, partial, jumps))
                && otherwise
                    .as_ref()
                    .is_none_or(|o| ends(&o.body, partial, jumps))
        }
        StmtKind::While { cond, body } => forever(cond, body),
        _ => false,
    })
}

/// Whether a `while` never ends of itself: `while true` with no `break` that leaves it.
pub fn forever(cond: &Expr, body: &[Stmt]) -> bool {
    matches!(cond.kind, ExprKind::Bool(true)) && !breaks(body)
}

/// Whether a `break` in `stmts` leaves the loop that holds them (not one nested inside).
fn breaks(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Break => true,
        StmtKind::When { .. } | StmtKind::Match { .. } => blocks(s).into_iter().any(breaks),
        _ => false,
    })
}

/// Statements as canonical Mzizi (RFC-0013 §17), each line indented by `indent` spaces.
/// Doc comments inside them are not kept: the tree does not hold them.
pub fn canonical_stmts(stmts: &[Stmt], indent: usize) -> String {
    let mut out = String::new();
    for s in stmts {
        stmt_text(s, indent, &mut out);
    }
    out
}

fn stmt_text(s: &Stmt, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    let line = |out: &mut String, text: String| {
        out.push_str(&pad);
        out.push_str(&text);
        out.push('\n');
    };
    match &s.kind {
        StmtKind::Bind {
            mutable,
            name,
            ty,
            value,
            ..
        } => {
            let word = if *mutable { "var" } else { "let" };
            let ty = ty.map(|t| format!(": {}", t.ty.name())).unwrap_or_default();
            line(
                out,
                format!("{word} {name}{ty} = {}", value_text(value, indent)),
            );
        }
        StmtKind::Assign { name, value, .. } => {
            line(out, format!("{name} = {}", value_text(value, indent)));
        }
        StmtKind::FieldAssign {
            name, field, value, ..
        } => {
            line(
                out,
                format!("{name}.{field} = {}", value_text(value, indent)),
            );
        }
        StmtKind::IndexAssign {
            name, index, value, ..
        } => {
            line(
                out,
                format!(
                    "{name}[{}] = {}",
                    canonical(index),
                    value_text(value, indent)
                ),
            );
        }
        StmtKind::Return(None) => line(out, "return".to_string()),
        StmtKind::Return(Some(v)) => line(out, format!("return {}", value_text(v, indent))),
        StmtKind::Expr(e) => line(out, canonical(e)),
        StmtKind::Break => line(out, "break".to_string()),
        StmtKind::Continue => line(out, "continue".to_string()),
        StmtKind::When {
            cond,
            then,
            else_whens,
            otherwise,
        } => {
            line(out, format!("when {}", canonical(cond)));
            out.push_str(&canonical_stmts(then, indent + 2));
            for ElseWhen { cond, body, .. } in else_whens {
                line(out, format!("else when {}", canonical(cond)));
                out.push_str(&canonical_stmts(body, indent + 2));
            }
            if let Some(o) = otherwise {
                line(out, "else".to_string());
                out.push_str(&canonical_stmts(o, indent + 2));
            }
            line(out, "end".to_string());
        }
        StmtKind::Match {
            scrutinee,
            arms,
            otherwise,
        } => {
            line(out, format!("match {}", canonical(scrutinee)));
            // A case written after the `else` stays after it, where it still never runs
            // (`MZ0931`): the canonical text never changes which case runs.
            let (before, after): (Vec<_>, Vec<_>) = arms.iter().partition(|a| !a.after_else);
            for a in before {
                line(out, format!("  case {}", arm_values(a)));
                out.push_str(&canonical_stmts(&a.body, indent + 4));
            }
            if let Some(o) = otherwise {
                line(out, "  else".to_string());
                out.push_str(&canonical_stmts(&o.body, indent + 4));
            }
            for a in after {
                line(out, format!("  case {}", arm_values(a)));
                out.push_str(&canonical_stmts(&a.body, indent + 4));
            }
            line(out, "end".to_string());
        }
        StmtKind::For {
            name, source, body, ..
        } => {
            line(out, format!("for each {name} in {}", canonical(source)));
            out.push_str(&canonical_stmts(body, indent + 2));
            line(out, "end".to_string());
        }
        StmtKind::While { cond, body } => {
            line(out, format!("while {}", canonical(cond)));
            out.push_str(&canonical_stmts(body, indent + 2));
            line(out, "end".to_string());
        }
    }
}

fn case_values(values: &[Expr]) -> String {
    values.iter().map(canonical).collect::<Vec<_>>().join(" ")
}

/// The key of a result's case line, `case ok` or `case error` (its name is the arm's
/// binding); `None` for any other line.
fn result_key(values: &[Expr]) -> Option<Key> {
    match values {
        [v] => match &v.kind {
            ExprKind::Name(n) if n == "ok" => Some(Key::Ok),
            ExprKind::Name(n) if n == "error" => Some(Key::Error),
            _ => None,
        },
        _ => None,
    }
}

/// A `case` line's words: its values, and the name a result's case binds (`ok v`).
fn arm_values<B>(a: &Arm<B>) -> String {
    let values = case_values(&a.values);
    match &a.binding {
        Some((name, _)) => format!("{values} {name}"),
        None => values,
    }
}

/// A value as it stands after `=` or `return`: a block used as a value spans lines, each
/// indented from `indent`; anything else is its one-line canonical text.
fn value_text(e: &Expr, indent: usize) -> String {
    let pad = " ".repeat(indent);
    match &e.kind {
        ExprKind::When { arms, otherwise } => {
            let mut t = String::new();
            for (k, (c, v)) in arms.iter().enumerate() {
                if k == 0 {
                    t.push_str(&format!("when {}\n", canonical(c)));
                } else {
                    t.push_str(&format!("{pad}else when {}\n", canonical(c)));
                }
                t.push_str(&format!("{pad}  {}\n", canonical(v)));
            }
            if let Some(o) = otherwise {
                t.push_str(&format!("{pad}else\n{pad}  {}\n", canonical(o)));
            }
            t.push_str(&format!("{pad}end"));
            t
        }
        ExprKind::Match {
            scrutinee,
            arms,
            otherwise,
        } => {
            let mut t = format!("match {}\n", canonical(scrutinee));
            let case = |a: &Arm<Expr>| {
                format!(
                    "{pad}  case {}\n{pad}    {}\n",
                    arm_values(a),
                    canonical(&a.body)
                )
            };
            for a in arms.iter().filter(|a| !a.after_else) {
                t.push_str(&case(a));
            }
            if let Some(o) = otherwise {
                t.push_str(&format!("{pad}  else\n{pad}    {}\n", canonical(&o.body)));
            }
            for a in arms.iter().filter(|a| a.after_else) {
                t.push_str(&case(a));
            }
            t.push_str(&format!("{pad}end"));
            t
        }
        _ => canonical(e),
    }
}

/// A value a `case` lists, as the checker compares them.
///
/// The one place coverage (`MZ0930`) and reachability (`MZ0931`) are decided, for every
/// `match`: over an enum, an `int`, a `text`, a `bool`, or a result (RFC-0013 §12.2), whose
/// cases are `ok` and `error` and whose universe is those two.
#[derive(Clone, Debug, PartialEq)]
enum Key {
    Variant(String),
    Int(i64),
    Text(String),
    Bool(bool),
    /// `case ok <name>`.
    Ok,
    /// `case error <name>`.
    Error,
}

impl FnCheck<'_> {
    /// Why `name` is the program's already, if it is: an enum's name or a variant's.
    pub(super) fn program_name(&self, name: &str) -> Option<String> {
        if self.enums.iter().any(|e| e.name == name) {
            return Some(format!("`{name}` is an enum of this program"));
        }
        self.enums
            .iter()
            .find(|e| e.has(name))
            .map(|e| format!("`{name}` is a variant of `enum {}`", e.name))
    }

    /// `match`, `for each`, `while`, `break` and `continue` as statements.
    pub(super) fn control(&mut self, s: &Stmt) {
        let block_at = (s.span.start_line, s.span.start_col);
        match &s.kind {
            StmtKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                let t = self.expr(scrutinee);
                let t = self.scrutinee_type(scrutinee, t);
                self.arms_check(t, scrutinee, arms, otherwise.as_ref(), false);
                self.whens.push(block_at);
                for a in arms {
                    self.scopes.push(Vec::new());
                    self.bind_case(a, t);
                    self.block(&a.body, None);
                    self.end_scope();
                }
                if let Some(o) = otherwise {
                    self.scoped(&o.body);
                }
                self.whens.pop();
            }
            StmtKind::For {
                name,
                name_span,
                source,
                body,
            } => {
                let el = self.loop_source(source);
                self.whens.push(block_at);
                self.scopes.push(Vec::new());
                let kind = if self.may_bind(name, *name_span) {
                    Kind::Loop
                } else {
                    Kind::Poison
                };
                self.bind(name, kind, el, None);
                self.loops += 1;
                self.block(body, None);
                self.loops -= 1;
                self.end_scope();
                self.whens.pop();
            }
            StmtKind::While { cond, body } => {
                let t = self.expr(cond);
                self.condition(cond, t, "while");
                self.whens.push(block_at);
                self.loops += 1;
                self.scoped(body);
                self.loops -= 1;
                self.whens.pop();
            }
            StmtKind::Break | StmtKind::Continue if self.loops == 0 => {
                let w = if matches!(s.kind, StmtKind::Break) {
                    "break"
                } else {
                    "continue"
                };
                self.err(
                    "MZ0935",
                    s.span,
                    format!(
                        "`{w}` with no loop open — `{w}` belongs inside a `for each` or a `while`; to leave a function, `return`"
                    ),
                );
            }
            _ => {}
        }
    }

    /// What a `for each` iterates (RFC-0013 §7.3): `range(a, to = b)` of two ints, or a
    /// list. Returns the loop binding's type: `int` for a range, the element type for a list.
    fn loop_source(&mut self, source: &Expr) -> Ty {
        if let ExprKind::Call { name, args, .. } = &source.kind
            && name == "range"
            && !self.fns.contains_key(name.as_str())
        {
            self.range_args(source.span, args);
            return Ty::Int;
        }
        let t = self.expr(source);
        if let Some(el) = self.loop_collection(source, t) {
            return el;
        }
        let say = format!(
            "`for each` iterates a list, and `{}` is {}",
            canonical(source),
            t.name()
        );
        match t {
            Ty::Error => {}
            Ty::Int => self.err_fix(
                "MZ0711",
                source.span,
                format!(
                    "{say} — a counted loop is `for each i in range(0, to = {})`",
                    canonical(source)
                ),
                source.span,
                format!("range(0, to = {})", canonical(source)),
                Confidence::Guess,
            ),
            _ => self.err("MZ0711", source.span, say),
        }
        Ty::Error
    }

    /// `range(a, to = b)`'s arguments: two ints.
    pub(super) fn range_args(&mut self, at: Span, args: &[Expr]) {
        let types: Vec<Ty> = args.iter().map(|a| self.expr(a)).collect();
        // A call the parser cut short is reported already.
        if args.iter().any(|a| matches!(a.kind, ExprKind::Error)) {
            return;
        }
        if args.len() != 2 {
            self.err(
                "MZ0905",
                at,
                format!(
                    "`range` takes two ints, `range(a, to = b)`: from `a` up to but not including `b` — this call gives {}",
                    args.len()
                ),
            );
            return;
        }
        for (a, t) in args.iter().zip(types) {
            if t != Ty::Int
                && !t.has_error()
                && !self.unhandled(a, t, || "`range` takes ints".into())
            {
                self.err(
                    "MZ0905",
                    a.span,
                    format!(
                        "`{}` is {}, and `range(a, to = b)` takes two ints",
                        canonical(a),
                        t.name()
                    ),
                );
            }
        }
    }

    /// `a is b` and `a is not b`: a bare variant on one side is read against the other
    /// side's enum (RFC-0008 §5), so a variant two enums share needs no qualifying there.
    pub(super) fn compare(
        &mut self,
        e: &Expr,
        op: crate::expr::BinOp,
        lhs: &Expr,
        rhs: &Expr,
    ) -> Ty {
        // `c is none`: a collection's emptiness (RFC-0013 §3.3).
        if matches!(op, crate::expr::BinOp::Is | crate::expr::BinOp::IsNot) {
            match (&lhs.kind, &rhs.kind) {
                (_, ExprKind::None) => return self.is_none(e, lhs),
                (ExprKind::None, _) => return self.is_none(e, rhs),
                _ => {}
            }
        }
        if let Some(t) = self.emptiness_idiom(e, op, lhs, rhs) {
            return t;
        }
        let candidate = |x: &Expr| match &x.kind {
            ExprKind::Name(n) if self.may_be_variant(n) => Some(n.clone()),
            _ => None,
        };
        let (l, r) = match (candidate(lhs), candidate(rhs)) {
            (None, Some(v)) => {
                let l = self.expr(lhs);
                let r = match self.variant_of(&v, rhs.span, l) {
                    Some(r) => r,
                    None => self.expr(rhs),
                };
                (l, r)
            }
            (Some(v), None) => {
                let r = self.expr(rhs);
                let l = match self.variant_of(&v, lhs.span, r) {
                    Some(l) => l,
                    None => self.expr(lhs),
                };
                (l, r)
            }
            // Two bare names: one that belongs to exactly one enum supplies the type the
            // other is read against (RFC-0013 §18.6), so `amber is blue` reads `blue` as
            // `light.blue` even when `color` has a `blue` too.
            (Some(a), Some(b)) => {
                let one = |n: &str| variant_owner(self.enums, n, None).is_ok();
                match (one(&a), one(&b)) {
                    (true, false) => {
                        let l = self.expr(lhs);
                        let r = match self.variant_of(&b, rhs.span, l) {
                            Some(r) => r,
                            None => self.expr(rhs),
                        };
                        (l, r)
                    }
                    (false, true) => {
                        let r = self.expr(rhs);
                        let l = match self.variant_of(&a, lhs.span, r) {
                            Some(l) => l,
                            None => self.expr(lhs),
                        };
                        (l, r)
                    }
                    // Both shared (`blue is blue`): naming the left one's enum settles the
                    // right one too, so the left is the one diagnostic.
                    (false, false)
                        if variant_owner(self.enums, &a, None).is_err_and(|o| o.len() > 1)
                            && variant_owner(self.enums, &b, None).is_err_and(|o| o.len() > 1) =>
                    {
                        (self.expr(lhs), Ty::Error)
                    }
                    _ => (self.expr(lhs), self.expr(rhs)),
                }
            }
            _ => (self.expr(lhs), self.expr(rhs)),
        };
        self.binary(e, op, lhs, rhs, l, r)
    }

    /// Whether a bare name not in scope here is read as a variant: always when some enum
    /// has it (a `fn` of that name is `MZ0921` at the `fn`), and otherwise only when no
    /// `fn` and no binding elsewhere in this function has the name, so a binding from a
    /// block that ended is `MZ0920` and a misspelt variant `MZ0708`.
    fn may_be_variant(&self, n: &str) -> bool {
        if self.visible(n).is_some() {
            return false;
        }
        self.enums.iter().any(|d| d.has(n))
            || (!self.fns.contains_key(n) && !self.sites.iter().any(|s| s.name == n))
    }

    /// `e` where a value of type `want` is expected (an annotated `let`, an assignment, a
    /// `return`, an argument): a bare variant is read against `want`'s enum, as RFC-0008 §5
    /// reads one, so a variant two enums share needs no qualifying there.
    pub(super) fn expr_want(&mut self, e: &Expr, want: Ty) -> Ty {
        if let ExprKind::Name(n) = &e.kind
            && self.may_be_variant(n)
            && let Ty::Enum(en) = want
            && self.enums.iter().any(|d| d.name == en && d.has(n))
        {
            self.bare_ok_error(n, e.span, want);
            return want;
        }
        if matches!(e.kind, ExprKind::When { .. } | ExprKind::Match { .. }) {
            return self.block_value(e, want);
        }
        if matches!(e.kind, ExprKind::List(_) | ExprKind::MapLit(_)) {
            return self.bracket(e, want);
        }
        self.expr(e)
    }

    /// A bare name compared with a value of `other`'s type: when that is an enum, the
    /// name is one of its variants (`MZ0708` with the nearest otherwise). `None` when
    /// `other` is not an enum, so the name resolves as it would anywhere.
    fn variant_of(&mut self, name: &str, at: Span, other: Ty) -> Option<Ty> {
        let Ty::Enum(en) = other else {
            return None;
        };
        let decl = self.enums.iter().find(|d| d.name == en)?;
        if decl.has(name) {
            self.bare_ok_error(name, at, other);
            return Some(other);
        }
        self.no_such_variant(decl, name, at);
        Some(Ty::Error)
    }

    /// `MZ0708`: `name` is not a variant of `decl`, with the nearest one as its fix.
    fn no_such_variant(&mut self, decl: &EnumDecl, name: &str, at: Span) {
        // Each variant once: one listed twice is `MZ0704` at its declaration already.
        let mut all: Vec<&str> = Vec::new();
        for v in &decl.variants {
            if !all.contains(&v.name.as_str()) {
                all.push(&v.name);
            }
        }
        let say = format!(
            "`{name}` is not a variant of `enum {}`, whose variants are {}",
            decl.name,
            all.iter()
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        match nearest(name, all.iter().copied()) {
            Some((near, c)) => self.err_fix(
                "MZ0708",
                at,
                format!("{say} — did you mean `{near}`?"),
                at,
                near,
                c,
            ),
            None => self.err("MZ0708", at, say),
        }
    }

    /// A bare name that is no binding: a variant of the one enum that has it. Two enums
    /// with it make it ambiguous outside a comparison or a `case` (`MZ0708`).
    pub(super) fn bare_variant(&mut self, name: &str, at: Span) -> Option<Ty> {
        match variant_owner(self.enums, name, None) {
            Ok(e) => Some(Ty::Enum(intern(e))),
            Err(owners) if owners.len() > 1 => {
                let first = owners[0].to_string();
                self.err_fix(
                    "MZ0708",
                    at,
                    format!(
                        "`{name}` is a variant of `enum {first}` and of `enum {}` — name its enum, `{first}.{name}`",
                        owners[1]
                    ),
                    at,
                    format!("{first}.{name}"),
                    Confidence::Guess,
                );
                Some(Ty::Error)
            }
            Err(_) => None,
        }
    }

    /// `<enum>.<variant>`, and `when` and `match` used as values.
    pub(super) fn control_expr(&mut self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::Variant {
                enum_name,
                name,
                name_span,
            } => {
                let Some(decl) = self.enums.iter().find(|d| &d.name == enum_name) else {
                    return Ty::Error;
                };
                if !decl.has(name) {
                    self.no_such_variant(decl, name, *name_span);
                    return Ty::Error;
                }
                Ty::Enum(intern(enum_name))
            }
            _ => self.block_value(e, Ty::Nothing),
        }
    }

    /// A `when` or `match` used as a value, each branch read against `want`, the type
    /// expected where the block stands (`Ty::Nothing` when nothing is expected, `Ty::Error`
    /// when what was expected was already reported).
    fn block_value(&mut self, e: &Expr, want: Ty) -> Ty {
        match &e.kind {
            ExprKind::When { arms, otherwise } => {
                let mut ty = None;
                for (c, v) in arms {
                    let t = self.expr(c);
                    self.condition(c, t, "when");
                    let vt = self.expr_want(v, branch_want(want, ty, v));
                    self.branch(&mut ty, v, vt, "when");
                }
                if let Some(o) = otherwise {
                    let vt = self.expr_want(o, branch_want(want, ty, o));
                    self.branch(&mut ty, o, vt, "when");
                }
                ty.unwrap_or(Ty::Error)
            }
            ExprKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                let t = self.expr(scrutinee);
                let t = self.scrutinee_type(scrutinee, t);
                self.arms_check(t, scrutinee, arms, otherwise.as_deref(), true);
                let mut ty = None;
                for a in arms {
                    self.scopes.push(Vec::new());
                    self.bind_case(a, t);
                    let vt = self.expr_want(&a.body, branch_want(want, ty, &a.body));
                    self.end_scope();
                    self.branch(&mut ty, &a.body, vt, "match");
                }
                if let Some(o) = otherwise {
                    let vt = self.expr_want(&o.body, branch_want(want, ty, &o.body));
                    self.branch(&mut ty, &o.body, vt, "match");
                }
                ty.unwrap_or(Ty::Error)
            }
            _ => Ty::Error,
        }
    }

    /// One branch of a block used as a value: every branch has the first one's type
    /// (`MZ0932`).
    fn branch(&mut self, ty: &mut Option<Ty>, v: &Expr, vt: Ty, what: &str) {
        match (vt, *ty) {
            (Ty::Error, _) => {}
            (Ty::Nothing, _) => self.err(
                "MZ0932",
                v.span,
                format!(
                    "this branch is `{}`, a call that returns nothing — each branch of a `{what}` used as a value is a value",
                    canonical(v)
                ),
            ),
            (vt, None) => *ty = Some(vt),
            (vt, Some(first)) if vt != first => self.err(
                "MZ0932",
                v.span,
                format!(
                    "the branches of a `{what}` used as a value have one type: the first is {}, and `{}` is {}",
                    first.name(),
                    canonical(v),
                    vt.name()
                ),
            ),
            _ => {}
        }
    }

    /// The type a `match` is over; a call that returns nothing has nothing to match.
    fn scrutinee_type(&mut self, s: &Expr, t: Ty) -> Ty {
        if t == Ty::Nothing {
            self.err(
                "MZ0711",
                s.span,
                format!(
                    "`{}` returns nothing, so a `match` has nothing to match",
                    canonical(s)
                ),
            );
            return Ty::Error;
        }
        if t == Ty::Float {
            // RFC-0013 §7.2 matches an enum, an int, a text or a bool: equality on a float
            // is rarely the test meant (`0.1 + 0.2`), so it has no cases.
            self.err(
                "MZ0711",
                s.span,
                format!(
                    "`match {}` is over a float, and a `match` takes an enum, an int, a text or a bool — compare a float with `when`",
                    canonical(s)
                ),
            );
            return Ty::Error;
        }
        t
    }

    /// A `match`'s cases against the type it is over: each value a literal or variant of
    /// that type, none listed twice or after the `else` (`MZ0931`), and every value
    /// covered (`MZ0930`, or `MZ0932` for a `match` used as a value).
    fn arms_check<B>(
        &mut self,
        scrut: Ty,
        s: &Expr,
        arms: &[Arm<B>],
        otherwise: Option<&ElseArm<B>>,
        as_value: bool,
    ) {
        let mut seen: Vec<Key> = Vec::new();
        // A case value already reported (a misspelt variant, a value of the wrong type)
        // covers nothing anyone can name, so the coverage verdict would be a second
        // diagnostic for the same mistake: it waits for the next check.
        let mut unresolved = false;
        let result = scrut.as_result().is_some();
        if result && let Some(o) = otherwise {
            // §12: a `match` on a result has no `else`; `case ok` and `case error` are its
            // cases, so a case written after the `else` still counts.
            self.err_fix(
                "MZ0931",
                o.span,
                "a `match` on a result takes no `else` — its cases are `case ok <name>` and `case error <name>`; delete it",
                lines(o.span.start_line, o.last_line),
                "",
                Confidence::Exact,
            );
        }
        for a in arms.iter().filter(|_| result) {
            // A result's case line is `case ok <name>` or `case error <name>`: anything
            // else is one `MZ0917` for the line (`bind_case` keeps the names it lists quiet).
            let Some(key) = result_key(&a.values) else {
                if !a.values.is_empty() && !a.values.iter().any(Expr::has_error) {
                    self.err(
                        "MZ0917",
                        a.span,
                        format!(
                            "a `match` on a result has `case ok <name>` and `case error <name>`, and this case reads `case {}`",
                            arm_values(a)
                        ),
                    );
                }
                unresolved = true;
                continue;
            };
            if seen.contains(&key) {
                self.err_fix(
                    "MZ0931",
                    a.span,
                    format!(
                        "a second `case {}` can never run — delete it",
                        if key == Key::Ok { "ok" } else { "error" }
                    ),
                    lines(a.span.start_line, a.last_line),
                    "",
                    Confidence::Exact,
                );
            } else {
                seen.push(key);
            }
        }
        for a in arms.iter().filter(|_| !result) {
            // A case that binds a name in a `match` that is not on a result is `MZ0917`
            // (`bind_case`), once: its values say nothing more, and coverage waits.
            if a.binding.is_some() {
                unresolved = true;
                continue;
            }
            if a.after_else {
                self.err_fix(
                    "MZ0931",
                    a.span,
                    "this `case` comes after the `else`, which takes every value left, so it can never run — delete it",
                    lines(a.span.start_line, a.last_line),
                    "",
                    Confidence::Exact,
                );
                continue;
            }
            let keys: Vec<Option<Key>> = a.values.iter().map(|v| self.case_key(v, scrut)).collect();
            // A `case` with no values was already reported by the parser (`MZ0917`).
            unresolved |= keys.is_empty() || keys.iter().any(Option::is_none);
            let mut dup = vec![false; keys.len()];
            let mut here: Vec<Key> = Vec::new();
            for (k, key) in keys.iter().enumerate() {
                if let Some(key) = key {
                    dup[k] = seen.contains(key) || here.contains(key);
                    here.push(key.clone());
                }
            }
            if !keys.is_empty() && keys.iter().all(Option::is_some) && dup.iter().all(|d| *d) {
                self.err_fix(
                    "MZ0931",
                    a.span,
                    format!(
                        "`case {}` lists only values an earlier case already takes, so it can never run — delete it",
                        case_values(&a.values)
                    ),
                    lines(a.span.start_line, a.last_line),
                    "",
                    Confidence::Exact,
                );
            } else {
                for (k, v) in a.values.iter().enumerate() {
                    if !dup[k] {
                        continue;
                    }
                    // The value and the space before it, or after it when it is first.
                    let span = if k > 0 {
                        let before = a.values[k - 1].span;
                        Span {
                            start_line: before.end_line,
                            start_col: before.end_col,
                            end_line: v.span.end_line,
                            end_col: v.span.end_col,
                        }
                    } else {
                        let after = a.values[1].span;
                        Span {
                            start_line: v.span.start_line,
                            start_col: v.span.start_col,
                            end_line: after.start_line,
                            end_col: after.start_col,
                        }
                    };
                    self.err_fix(
                        "MZ0931",
                        v.span,
                        format!(
                            "`{}` is already a case of this `match`, so listing it again can never matter — delete it",
                            canonical(v)
                        ),
                        span,
                        "",
                        Confidence::Exact,
                    );
                }
            }
            for key in here {
                if !seen.contains(&key) {
                    seen.push(key);
                }
            }
        }
        let at = (s.span.start_line, s.span.start_col);
        // Coverage that cannot be judged (a case already reported, a value of an unknown
        // type) may miss a case: without `else`, the line after the `match` is reachable.
        if unresolved || !(matches!(scrut, Ty::Enum(_) | Ty::Bool | Ty::Int | Ty::Text) || result) {
            if otherwise.is_none() {
                self.partial.push(at);
            }
            return;
        }
        let code = if as_value { "MZ0932" } else { "MZ0930" };
        let all: Vec<Key> = match scrut {
            Ty::Enum(e) => self
                .enums
                .iter()
                .find(|d| d.name == e)
                .map(|d| {
                    d.variants
                        .iter()
                        .map(|v| Key::Variant(v.name.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            Ty::Bool => vec![Key::Bool(true), Key::Bool(false)],
            Ty::Result(_) => vec![Key::Ok, Key::Error],
            Ty::Int | Ty::Text => {
                if otherwise.is_none() {
                    self.partial.push(at);
                    self.err(
                        code,
                        s.span,
                        format!(
                            "`match {}` is over {}, and no list of cases covers every {} — end it with `else`",
                            canonical(s),
                            if scrut == Ty::Int { "an int" } else { "a text" },
                            scrut.name()
                        ),
                    );
                }
                return;
            }
            _ => return,
        };
        // A variant listed twice (`MZ0704`) is named once.
        let mut all_once: Vec<Key> = Vec::new();
        for k in all {
            if !all_once.contains(&k) {
                all_once.push(k);
            }
        }
        let missing: Vec<String> = all_once
            .iter()
            .filter(|k| !seen.contains(k))
            .map(|k| match k {
                Key::Variant(v) => format!("`{v}`"),
                Key::Bool(b) => format!("`{b}`"),
                Key::Ok => "`case ok`".to_string(),
                Key::Error => "`case error`".to_string(),
                Key::Int(_) | Key::Text(_) => String::new(),
            })
            .collect();
        if result {
            if !missing.is_empty() {
                self.partial.push(at);
                self.err(
                    code,
                    s.span,
                    format!(
                        "`match {}` misses {} — a `match` on a result handles its success and its error",
                        canonical(s),
                        missing.join(" and ")
                    ),
                );
            }
            return;
        }
        match otherwise {
            None if !missing.is_empty() => {
                let more = if as_value {
                    " — a `match` used as a value covers every value (RFC-0013 §7.4)"
                } else {
                    ""
                };
                self.partial.push(at);
                self.err(
                    code,
                    s.span,
                    format!(
                        "`match {}` misses {} — add a `case` for each, or end it with `else`{more}",
                        canonical(s),
                        missing.join(", ")
                    ),
                );
            }
            Some(o) if missing.is_empty() => self.err_fix(
                "MZ0931",
                o.span,
                format!(
                    "every value of `{}` already has a `case`, so this `else` can never run — delete it",
                    scrut.name()
                ),
                lines(o.span.start_line, o.last_line),
                "",
                Confidence::Exact,
            ),
            _ => {}
        }
    }

    /// The value a `case` lists, checked against the type matched.
    fn case_key(&mut self, v: &Expr, scrut: Ty) -> Option<Key> {
        if scrut == Ty::Error {
            return None;
        }
        match (&v.kind, scrut) {
            (ExprKind::Error, _) => None,

            (ExprKind::Name(n), Ty::Enum(e)) if self.may_be_variant(n) => {
                let decl = self.enums.iter().find(|d| d.name == e)?;
                if decl.has(n) {
                    return Some(Key::Variant(n.clone()));
                }
                self.no_such_variant(decl, n, v.span);
                None
            }
            (
                ExprKind::Variant {
                    enum_name,
                    name,
                    name_span,
                },
                Ty::Enum(e),
            ) if enum_name == e => {
                let decl = self.enums.iter().find(|d| d.name == e)?;
                if decl.has(name) {
                    return Some(Key::Variant(name.clone()));
                }
                self.no_such_variant(decl, name, *name_span);
                None
            }
            (ExprKind::Int(i), Ty::Int) => Some(Key::Int(*i)),
            (
                ExprKind::Unary {
                    op: UnOp::Neg,
                    operand,
                },
                Ty::Int,
            ) if matches!(operand.kind, ExprKind::Int(_)) => match fold(v) {
                Some(Ok(i)) => Some(Key::Int(i)),
                _ => None,
            },
            (ExprKind::Text(parts), Ty::Text)
                if parts.iter().all(|p| matches!(p, TextPart::Lit(_))) =>
            {
                Some(Key::Text(
                    parts
                        .iter()
                        .map(|p| match p {
                            TextPart::Lit(s) => s.as_str(),
                            TextPart::Expr(_) => "",
                        })
                        .collect(),
                ))
            }
            (ExprKind::Bool(b), Ty::Bool) => Some(Key::Bool(*b)),
            _ => {
                match self.literal_type(v) {
                    Some(t) => self.err(
                        "MZ0912",
                        v.span,
                        format!(
                            "`case {}` is {}, and this `match` is over {}",
                            canonical(v),
                            t.name(),
                            scrut.name()
                        ),
                    ),
                    None => self.err(
                        "MZ0917",
                        v.span,
                        format!(
                            "a `case` lists literals or variants, and `{}` is neither — a computed test is a `when`",
                            canonical(v)
                        ),
                    ),
                }
                None
            }
        }
    }

    /// The type of a literal or variant, or `None` for anything computed.
    fn literal_type(&self, v: &Expr) -> Option<Ty> {
        match &v.kind {
            ExprKind::Int(_) => Some(Ty::Int),
            ExprKind::Float(_) => Some(Ty::Float),
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } if matches!(operand.kind, ExprKind::Int(_)) => Some(Ty::Int),
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } if matches!(operand.kind, ExprKind::Float(_)) => Some(Ty::Float),
            ExprKind::Bool(_) => Some(Ty::Bool),
            ExprKind::Text(parts) if parts.iter().all(|p| matches!(p, TextPart::Lit(_))) => {
                Some(Ty::Text)
            }
            ExprKind::Variant { enum_name, .. } => Some(Ty::Enum(intern(enum_name))),
            ExprKind::Name(n) if self.may_be_variant(n) => variant_owner(self.enums, n, None)
                .ok()
                .map(|e| Ty::Enum(intern(e))),
            _ => None,
        }
    }

    /// The subject and case texts of a chain whose every condition is `<e> is <variant>`
    /// on one enum-typed name `<e>` (RFC-0013 §7.1): `(e, its enum, one case per
    /// condition)`. `None` for one condition, or for any other chain.
    fn chain_subject(&self, conds: &[&Expr]) -> Option<(String, &'static str, Vec<String>)> {
        if conds.len() < 2 {
            return None;
        }
        let subject = |c: &Expr| -> Option<(String, &'static str, String)> {
            let ExprKind::Binary {
                op: crate::expr::BinOp::Is,
                lhs,
                rhs,
                ..
            } = &c.kind
            else {
                return None;
            };
            let ExprKind::Name(e) = &lhs.kind else {
                return None;
            };
            let Ty::Enum(en) = self.bindings[self.visible(e)?].ty else {
                return None;
            };
            let decl = self.enums.iter().find(|d| d.name == en)?;
            let is_variant = match &rhs.kind {
                ExprKind::Name(v) => self.visible(v).is_none() && decl.has(v),
                ExprKind::Variant {
                    enum_name, name, ..
                } => enum_name == en && decl.has(name),
                _ => false,
            };
            is_variant.then(|| (e.clone(), en, canonical(rhs)))
        };
        let (name, en, first) = subject(conds[0])?;
        let mut cases = vec![first];
        for c in &conds[1..] {
            match subject(c) {
                Some((n, _, v)) if n == name => cases.push(v),
                _ => return None,
            }
        }
        Some((name, en, cases))
    }

    /// `MZ0936` at `s`, whose fix replaces `span` with `text`.
    fn chain_is_a_match(&mut self, at: Span, name: &str, en: &str, fix: Span, text: String) {
        self.err_fix(
            "MZ0936",
            at,
            format!(
                "this chain tests `{name}` against the variants of `enum {en}` one by one — that is a `match`, which checks that every variant has a case (RFC-0013 §7.1); the fix rewrites the chain as one, in canonical form, and keeps no `##` comment from inside it"
            ),
            fix,
            text,
            Confidence::Guess,
        );
    }

    /// `MZ0936`: a `when` with `else when`s whose every condition is `<e> is <variant>` on
    /// one enum-typed name is a `match` written as a chain, which loses the check that
    /// every variant is covered (RFC-0013 §7.1, CL-8). The fix is a `guess`: a chain
    /// without `else` that misses variants becomes a `match` that is `MZ0930`.
    pub(super) fn variant_chain(
        &mut self,
        s: &Stmt,
        cond: &Expr,
        else_whens: &[ElseWhen],
        otherwise: Option<&[Stmt]>,
    ) {
        let conds: Vec<&Expr> = std::iter::once(cond)
            .chain(else_whens.iter().map(|w| &w.cond))
            .collect();
        let Some((name, en, cases)) = self.chain_subject(&conds) else {
            return;
        };
        let indent = s.span.start_col.saturating_sub(1) as usize;
        let pad = " ".repeat(indent);
        let StmtKind::When { then, .. } = &s.kind else {
            return;
        };
        let bodies =
            std::iter::once(then.as_slice()).chain(else_whens.iter().map(|w| w.body.as_slice()));
        let mut text = format!("match {name}\n");
        for (v, body) in cases.iter().zip(bodies) {
            text.push_str(&format!("{pad}  case {v}\n"));
            text.push_str(&canonical_stmts(body, indent + 4));
        }
        if let Some(o) = otherwise {
            text.push_str(&format!("{pad}  else\n"));
            text.push_str(&canonical_stmts(o, indent + 4));
        }
        text.push_str(&format!("{pad}end\n"));
        let fix = Span {
            start_line: s.span.start_line,
            start_col: s.span.start_col,
            end_line: s.last_line + 1,
            end_col: 1,
        };
        self.chain_is_a_match(s.span, &name, en, fix, text);
    }

    /// `MZ0936` for a `when` used as a value (RFC-0013 §7.4) whose chain is over one enum's
    /// variants: the `guess` fix rewrites it as a `match` used as a value, from `when`
    /// through the `end` on `s`'s last line. `s` is the `let`, `var`, assignment or
    /// `return` that holds it.
    pub(super) fn value_variant_chain(&mut self, s: &Stmt, value: &Expr) {
        let ExprKind::When { arms, otherwise } = &value.kind else {
            return;
        };
        let conds: Vec<&Expr> = arms.iter().map(|(c, _)| c).collect();
        let Some((name, en, cases)) = self.chain_subject(&conds) else {
            return;
        };
        let pad = " ".repeat(s.span.start_col.saturating_sub(1) as usize);
        let mut text = format!("match {name}\n");
        for (v, (_, branch)) in cases.iter().zip(arms) {
            text.push_str(&format!(
                "{pad}  case {v}\n{pad}    {}\n",
                canonical(branch)
            ));
        }
        if let Some(o) = otherwise {
            text.push_str(&format!("{pad}  else\n{pad}    {}\n", canonical(o)));
        }
        text.push_str(&format!("{pad}end\n"));
        let fix = Span {
            start_line: value.span.start_line,
            start_col: value.span.start_col,
            end_line: s.last_line + 1,
            end_col: 1,
        };
        self.chain_is_a_match(value.span, &name, en, fix, text);
    }
}
