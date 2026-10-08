//! Lowering RFC-0013 §7's control flow to Rust (§14.2): enums, `match`, `for each` over
//! `range(a, to = b)`, `while`, `break`, `continue`, and `when` and `match` used as values.
//!
//! - An enum is a fieldless Rust `enum` deriving `Copy`, `Eq` and `Ord` (declaration
//!   order), with an `MzText` implementation that writes the variant's name (§3.8).
//! - A `match` on an enum or a `bool` has no `_` arm unless it has an `else`, so `rustc`
//!   checks exhaustiveness a second time. On `int` it matches `i64` literals; on `text`, the
//!   `&str` of the value.
//! - `for each i in range(a, to = b)` is `for i in a..b`: no list is built. `while true` is
//!   `loop`, which `rustc` knows does not end of itself.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{Lower, ident};
use crate::expr::{BinOp, Expr, ExprKind, TextPart, Ty, UnOp, fold, intern};
use crate::program::{EnumDecl, Stmt, StmtKind, variant_owner};

/// Names a Rust prelude or derive already takes, which an enum is not emitted as
/// (RFC-0013 §14.2).
const PRELUDE: &[&str] = &[
    "AsMut",
    "AsRef",
    "Box",
    "Clone",
    "Copy",
    "Debug",
    "Default",
    "DoubleEndedIterator",
    "Drop",
    "Eq",
    "ExactSizeIterator",
    "Extend",
    "Fn",
    "FnMut",
    "FnOnce",
    "From",
    "FromIterator",
    "Hash",
    "Into",
    "IntoIterator",
    "Iterator",
    "Option",
    "Ord",
    "PartialEq",
    "PartialOrd",
    "Result",
    // A keyword, not a prelude name, and the one PascalCase can make: `self_` is `Self`.
    "Self",
    "Send",
    "Sized",
    "String",
    "Sync",
    "ToOwned",
    "ToString",
    "TryFrom",
    "TryInto",
    "Unpin",
    "Vec",
];

/// `snake_case` as `PascalCase`.
fn pascal(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// Each enum's Rust name and its variants' Rust names: PascalCase, with `MzUser` before a
/// prelude name or `Self`, and a numbered `MzUser` name where two would otherwise be one
/// (`a1` and `a_1`) or a variant would be `Self`. A Mzizi name cannot start with `mz_`, so
/// no Mzizi name becomes an `MzUser` one.
pub(super) fn rust_enum_names(
    enums: &[EnumDecl],
) -> BTreeMap<String, (String, BTreeMap<String, String>)> {
    let mut out = BTreeMap::new();
    let mut used: Vec<String> = Vec::new();
    for (k, e) in enums.iter().enumerate() {
        let mut name = pascal(&e.name);
        if PRELUDE.contains(&name.as_str()) {
            name = format!("MzUser{name}");
        }
        if name.is_empty() || used.contains(&name) {
            name = format!("MzUser{}{name}", k + 1);
        }
        used.push(name.clone());
        let mut variants = BTreeMap::new();
        let mut taken: Vec<String> = Vec::new();
        for (j, (v, _)) in e.variants.iter().enumerate() {
            let mut rust = pascal(v);
            if rust.is_empty() || rust == "Self" || taken.contains(&rust) {
                rust = format!("MzUser{}{rust}", j + 1);
            }
            taken.push(rust.clone());
            variants.insert(v.clone(), rust);
        }
        out.insert(e.name.clone(), (name, variants));
    }
    out
}

impl Lower<'_> {
    /// An enum: the type, and its text form.
    pub(super) fn enum_decl(&self, e: &EnumDecl, out: &mut String) {
        let Some((rust, variants)) = self.rust_enums.get(&e.name) else {
            return;
        };
        let _ = writeln!(
            out,
            "\n// enum {}\n#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]\nenum {rust} {{",
            e.name
        );
        for (v, _) in &e.variants {
            let _ = writeln!(
                out,
                "    {},",
                variants.get(v).map_or(v.as_str(), String::as_str)
            );
        }
        let _ = writeln!(
            out,
            "}}\n\nimpl MzText for {rust} {{\n    fn mz_text(&self) -> String {{\n        match *self {{"
        );
        for (v, _) in &e.variants {
            let _ = writeln!(
                out,
                "            {rust}::{} => String::from({v:?}),",
                variants.get(v).map_or(v.as_str(), String::as_str)
            );
        }
        out.push_str("        }\n    }\n}\n");
    }

    /// `<Enum>::<Variant>`, for variant `v` of the enum named `owner`.
    fn path(&self, owner: &str, v: &str) -> String {
        match self.rust_enums.get(owner) {
            Some((rust, variants)) => {
                format!("{rust}::{}", variants.get(v).map_or(v, String::as_str))
            }
            None => "()".to_string(),
        }
    }

    /// The type of a variant: of `enum_name` when written, else of the one enum that has
    /// it (the checker reported any other bare use).
    pub(super) fn variant_type(&self, name: &str, enum_name: Option<&str>) -> Ty {
        match enum_name {
            Some(e) => Ty::Enum(intern(e)),
            None => {
                variant_owner(self.enums, name, None).map_or(Ty::Error, |e| Ty::Enum(intern(e)))
            }
        }
    }

    /// A variant as a Rust path.
    pub(super) fn variant_path(&self, name: &str, enum_name: Option<&str>) -> String {
        match enum_name {
            Some(e) => self.path(e, name),
            None => match variant_owner(self.enums, name, None) {
                Ok(e) => self.path(e, name),
                Err(_) => "()".to_string(),
            },
        }
    }

    /// `e` where a value of type `want` is expected, a bare variant read against `want`'s
    /// enum, as the checker reads it.
    pub(super) fn expr_want(&mut self, e: &Expr, want: Ty) -> String {
        if let ExprKind::Name(n) = &e.kind
            && !self.types.contains_key(n)
            && !self.fns.contains_key(n.as_str())
            && let Ty::Enum(en) = want
            && self.enums.iter().any(|d| d.name == en && d.has(n))
        {
            return self.path(en, n);
        }
        if matches!(e.kind, ExprKind::When { .. } | ExprKind::Match { .. }) {
            return self.block_value(e, want);
        }
        self.expr(e)
    }

    /// `x is v`, `x < v` and the other comparisons with `v` a bare variant, read against
    /// `x`'s enum, as the checker reads it.
    pub(super) fn variant_comparison(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
    ) -> Option<String> {
        let bare = |x: &Expr| match &x.kind {
            ExprKind::Name(n)
                if !self.types.contains_key(n) && !self.fns.contains_key(n.as_str()) =>
            {
                Some(n.clone())
            }
            _ => None,
        };
        let rust_op = super::rust_cmp_op(op);
        match (bare(lhs), bare(rhs)) {
            (None, Some(v)) => {
                let Ty::Enum(e) = self.ty(lhs) else {
                    return None;
                };
                let l = self.atom(lhs);
                Some(format!("{l} {rust_op} {}", self.path(e, &v)))
            }
            (Some(v), None) => {
                let Ty::Enum(e) = self.ty(rhs) else {
                    return None;
                };
                let r = self.atom(rhs);
                Some(format!("{} {rust_op} {r}", self.path(e, &v)))
            }
            _ => None,
        }
    }

    /// A `case` value as a Rust pattern, against the type matched.
    fn pattern(&self, v: &Expr, scrut: Ty) -> String {
        match (&v.kind, scrut) {
            (ExprKind::Name(n), Ty::Enum(e)) => self.path(e, n),
            (
                ExprKind::Variant {
                    enum_name, name, ..
                },
                _,
            ) => self.path(enum_name, name),
            (ExprKind::Int(i), _) => format!("{i}i64"),
            (ExprKind::Unary { op: UnOp::Neg, .. }, _) => match fold(v) {
                Some(Ok(i)) => format!("{i}i64"),
                _ => "_".to_string(),
            },
            (ExprKind::Text(parts), _) => {
                let lit: String = parts
                    .iter()
                    .map(|p| match p {
                        TextPart::Lit(s) => s.as_str(),
                        TextPart::Expr(_) => "",
                    })
                    .collect();
                format!("{lit:?}")
            }
            (ExprKind::Bool(b), _) => b.to_string(),
            _ => "_".to_string(),
        }
    }

    /// The value a `match` is over, as the head of a Rust `match`.
    fn match_head(&mut self, scrutinee: &Expr) -> (String, Ty) {
        let t = self.ty(scrutinee);
        let head = self.expr(scrutinee);
        match t {
            Ty::Text => (format!("({head}).as_str()"), t),
            _ => (head, t),
        }
    }

    /// The patterns of one `case`, joined with `|`.
    fn patterns(&self, values: &[Expr], scrut: Ty) -> String {
        values
            .iter()
            .map(|v| self.pattern(v, scrut))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// One bound of a `range(a, to = b)`, parenthesised unless it is a literal, a name or a
    /// call.
    fn bound(&mut self, e: &Expr) -> String {
        let s = self.expr(e);
        if fold(e).is_some_and(|r| r.is_ok())
            || matches!(e.kind, ExprKind::Name(_) | ExprKind::Call { .. })
        {
            s
        } else {
            format!("({s})")
        }
    }

    /// `match`, `for each`, `while`, `break` and `continue`.
    pub(super) fn control(&mut self, s: &Stmt, depth: usize, out: &mut String) {
        let pad = "    ".repeat(depth);
        match &s.kind {
            StmtKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                let (head, t) = self.match_head(scrutinee);
                let _ = writeln!(out, "{pad}match {head} {{");
                for a in arms.iter().filter(|a| !a.after_else) {
                    let _ = writeln!(out, "{pad}    {} => {{", self.patterns(&a.values, t));
                    self.block(&a.body, depth + 2, out);
                    let _ = writeln!(out, "{pad}    }}");
                }
                if let Some(o) = otherwise {
                    let _ = writeln!(out, "{pad}    _ => {{");
                    self.block(&o.body, depth + 2, out);
                    let _ = writeln!(out, "{pad}    }}");
                }
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::For {
                name, source, body, ..
            } => {
                self.types.insert(name.clone(), Ty::Int);
                let range = match &source.kind {
                    ExprKind::Call { args, .. } if args.len() == 2 => {
                        let a = self.bound(&args[0]);
                        let b = self.bound(&args[1]);
                        format!("{a}..{b}")
                    }
                    _ => "0i64..0i64".to_string(),
                };
                let _ = writeln!(out, "{pad}for {} in {range} {{", ident(name));
                self.block(body, depth + 1, out);
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::While { cond, body } => {
                if matches!(cond.kind, ExprKind::Bool(true)) {
                    let _ = writeln!(out, "{pad}loop {{");
                } else {
                    let _ = writeln!(out, "{pad}while {} {{", self.expr(cond));
                }
                self.block(body, depth + 1, out);
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::Break => {
                let _ = writeln!(out, "{pad}break;");
            }
            StmtKind::Continue => {
                let _ = writeln!(out, "{pad}continue;");
            }
            _ => {}
        }
    }

    /// A `when` or `match` used as a value: a Rust `if` or `match` expression.
    /// Each branch is read against `want`, the type expected where the block stands.
    pub(super) fn block_value(&mut self, e: &Expr, want: Ty) -> String {
        match &e.kind {
            ExprKind::When { arms, otherwise } => {
                let mut t = String::new();
                for (k, (c, v)) in arms.iter().enumerate() {
                    if k > 0 {
                        t.push_str(" else ");
                    }
                    let c = self.expr(c);
                    let v = self.expr_want(v, want);
                    let _ = write!(t, "if {c} {{ {v} }}");
                }
                if let Some(o) = otherwise {
                    let o = self.expr_want(o, want);
                    let _ = write!(t, " else {{ {o} }}");
                }
                t
            }
            ExprKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                let (head, t) = self.match_head(scrutinee);
                let mut branches = Vec::new();
                for a in arms.iter().filter(|a| !a.after_else) {
                    let v = self.expr_want(&a.body, want);
                    branches.push(format!("{} => {v}", self.patterns(&a.values, t)));
                }
                if let Some(o) = otherwise {
                    let v = self.expr_want(&o.body, want);
                    branches.push(format!("_ => {v}"));
                }
                format!("match {head} {{ {} }}", branches.join(", "))
            }
            _ => "()".to_string(),
        }
    }
}
