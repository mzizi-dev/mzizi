//! Lowering RFC-0013 §9 (§14.2's rows for it): `list(T)` as `Vec<T>`, `map(K, V)` and
//! `set(K)` as the standard library's `BTreeMap` and `BTreeSet` (key order, §9.2), an option
//! as `Option<T>`, indexing through a helper that returns an `Option` and never indexes
//! with `[…]`, `xs[i] = v` through one that traps out of range (§4.3), `otherwise` as a
//! `match` whose default runs only on `None` (§8.1), and each method and fold through a
//! helper in [`COLLECTIONS_RUNTIME`]. Nothing here emits `unwrap`, `expect`, `panic!`, Rust
//! indexing or `unsafe` (§14.3).

use super::{Lower, fn_ident, ident};
use crate::expr::{BinOp, Expr, ExprKind, Ty};

/// The helpers every lowered program carries, after the runtime, verbatim. Each takes its
/// collection by reference and its function by value (a `fn` item of the program), and
/// clones what it hands on, so the caller's value is never changed (§14.1).
pub(super) const COLLECTIONS_RUNTIME: &str = r#"
// ---- collections (RFC-0013 §9) ----

/// A size as an `int`. No collection holds more than `i64::MAX` elements.
fn mz_len(n: usize) -> i64 {
    match i64::try_from(n) {
        Ok(v) => v,
        Err(_) => i64::MAX,
    }
}

/// `xs[i]` (RFC-0013 §3.7): the element, or `None` outside the list.
fn mz_index<T: Clone>(xs: &[T], i: i64) -> Option<T> {
    usize::try_from(i).ok().and_then(|u| xs.get(u)).cloned()
}

/// `xs[i] = v` (RFC-0013 §9.2): replaces the element, and traps outside the list (§4.3).
fn mz_set_index<T>(xs: &mut Vec<T>, i: i64, v: T, at: &MzAt) {
    match usize::try_from(i).ok().and_then(|u| xs.get_mut(u)) {
        Some(slot) => *slot = v,
        None => mz_trap(at, "index out of range"),
    }
}

/// `xs.slice(a, to = b)`: `None` when either end is outside the list or `a` is past `b`.
fn mz_slice<T: Clone>(xs: &[T], a: i64, b: i64) -> Option<Vec<T>> {
    let a = usize::try_from(a).ok()?;
    let b = usize::try_from(b).ok()?;
    if a > b {
        return None;
    }
    xs.get(a..b).map(|s| s.to_vec())
}

/// `range(a, to = b)` as a list.
fn mz_range(a: i64, b: i64) -> Vec<i64> {
    (a..b).collect()
}

fn mz_map<T: Clone, U>(xs: &[T], f: impl Fn(T) -> U) -> Vec<U> {
    xs.iter().cloned().map(f).collect()
}

fn mz_filter<T: Clone>(xs: &[T], f: impl Fn(T) -> bool) -> Vec<T> {
    xs.iter().filter(|x| f((*x).clone())).cloned().collect()
}

fn mz_count<T: Clone>(xs: &[T], f: impl Fn(T) -> bool) -> i64 {
    mz_len(xs.iter().filter(|x| f((*x).clone())).count())
}

fn mz_any<T: Clone>(xs: &[T], f: impl Fn(T) -> bool) -> bool {
    xs.iter().any(|x| f(x.clone()))
}

fn mz_all<T: Clone>(xs: &[T], f: impl Fn(T) -> bool) -> bool {
    xs.iter().all(|x| f(x.clone()))
}

fn mz_first<T: Clone>(xs: &[T], f: impl Fn(T) -> bool) -> Option<T> {
    xs.iter().find(|x| f((*x).clone())).cloned()
}

fn mz_fold<T: Clone, A>(xs: &[T], init: A, f: impl Fn(A, T) -> A) -> A {
    xs.iter().cloned().fold(init, f)
}

/// A stable sort: elements with equal keys keep their order (RFC-0013 §9.4).
fn mz_sort_by<T: Clone, K: Ord>(xs: &[T], f: impl Fn(T) -> K) -> Vec<T> {
    let mut v = xs.to_vec();
    v.sort_by_key(|x| f(x.clone()));
    v
}

fn mz_group_by<T: Clone, K: Ord>(xs: &[T], f: impl Fn(T) -> K) -> ::std::collections::BTreeMap<K, Vec<T>> {
    let mut m = ::std::collections::BTreeMap::new();
    for x in xs {
        m.entry(f(x.clone())).or_insert_with(Vec::new).push(x.clone());
    }
    m
}

/// An `int` sum traps on overflow, as `+` does.
fn mz_sum_int(xs: &[i64], at: &MzAt) -> i64 {
    let mut total = 0i64;
    for x in xs {
        total = mz_add(total, *x, at);
    }
    total
}

/// A `float` sum adds from `0.0`, so an empty list sums to `0.0`, not `iter().sum()`'s `-0.0`.
fn mz_sum_float(xs: &[f64]) -> f64 {
    let mut total = 0.0f64;
    for x in xs {
        total += *x;
    }
    total
}

fn mz_join(xs: &[String], sep: &str) -> String {
    xs.join(sep)
}

/// Text inside a collection is quoted, with the escapes it was written with (§3.8).
fn mz_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl<T: MzText> MzText for Vec<T> {
    fn mz_text(&self) -> String {
        let items: Vec<String> = self.iter().map(MzText::mz_text_in).collect();
        format!("[{}]", items.join(", "))
    }
}

impl<T: MzText> MzText for ::std::collections::BTreeSet<T> {
    fn mz_text(&self) -> String {
        let items: Vec<String> = self.iter().map(MzText::mz_text_in).collect();
        format!("[{}]", items.join(", "))
    }
}

impl<K: MzText, V: MzText> MzText for ::std::collections::BTreeMap<K, V> {
    fn mz_text(&self) -> String {
        let items: Vec<String> = self
            .iter()
            .map(|(k, v)| format!("{}: {}", k.mz_text_in(), v.mz_text_in()))
            .collect();
        format!("[{}]", items.join(", "))
    }
}

/// An option prints only inside a collection (§3.8): its value, or `none`.
impl<T: MzText> MzText for Option<T> {
    fn mz_text(&self) -> String {
        match self {
            Some(v) => v.mz_text_in(),
            None => String::from("none"),
        }
    }
}
"#;

impl Lower<'_> {
    /// A value read in place, by reference: a binding's name with no `.clone()`, or any
    /// other expression in parentheses. For a method that only reads (§14.1).
    pub(super) fn place(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Name(n) if self.types.contains_key(n) => ident(n),
            _ => format!("({})", self.expr(e)),
        }
    }

    /// A bracket literal (RFC-0013 §9.1) as the collection `want` names: a `Vec`, a
    /// `BTreeSet` where a set is expected, or a `BTreeMap`.
    pub(super) fn bracket(&mut self, e: &Expr, want: Ty) -> String {
        let want = if want.is_collection() {
            want
        } else {
            self.ty(e)
        };
        match (&e.kind, want) {
            (ExprKind::List(items), Ty::Set(t)) => {
                if items.is_empty() {
                    return format!(
                        "::std::collections::BTreeSet::<{}>::new()",
                        self.rust_type(*t)
                    );
                }
                let items: Vec<String> = items.iter().map(|i| self.expr_want(i, *t)).collect();
                format!("::std::collections::BTreeSet::from([{}])", items.join(", "))
            }
            (ExprKind::List(items), Ty::Map(&(k, v))) if items.is_empty() => format!(
                "::std::collections::BTreeMap::<{}, {}>::new()",
                self.rust_type(k),
                self.rust_type(v)
            ),
            (ExprKind::List(items), Ty::List(t)) => {
                if items.is_empty() {
                    return format!("Vec::<{}>::new()", self.rust_type(*t));
                }
                let items: Vec<String> = items.iter().map(|i| self.expr_want(i, *t)).collect();
                format!("vec![{}]", items.join(", "))
            }
            (ExprKind::MapLit(entries), Ty::Map(&(k, v))) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| {
                        let key = self.expr_want(key, k);
                        let value = self.expr_want(value, v);
                        format!("({key}, {value})")
                    })
                    .collect();
                format!(
                    "::std::collections::BTreeMap::from([{}])",
                    entries.join(", ")
                )
            }
            _ => "()".to_string(),
        }
    }

    /// The type of a bracket literal with nothing expected: a list of its first element's
    /// type that is not itself `[]`, or a map of its first entry's.
    pub(super) fn bracket_ty(&self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::List(items) => {
                let anchor = items
                    .iter()
                    .find(|i| !matches!(&i.kind, ExprKind::List(v) if v.is_empty()))
                    .or(items.first());
                anchor.map_or(Ty::list(Ty::Error), |a| Ty::list(self.ty(a)))
            }
            ExprKind::MapLit(entries) => entries
                .first()
                .map_or(Ty::Error, |(k, v)| Ty::map(self.ty(k), self.ty(v))),
            _ => Ty::Error,
        }
    }

    /// `base[index]`: a list's element through `mz_index`, a map's value through `get`.
    pub(super) fn index(&mut self, base: &Expr, index: &Expr) -> String {
        match self.ty(base) {
            Ty::Map(&(k, _)) => {
                let b = self.place(base);
                let key = self.expr_want(index, k);
                format!("{b}.get(&{key}).cloned()")
            }
            _ => {
                let b = self.place(base);
                let i = self.expr(index);
                format!("mz_index(&{b}, {i})")
            }
        }
    }

    /// `name[index] = value`: the index and value first, then the change, so either may
    /// read the collection.
    pub(super) fn index_assign(
        &mut self,
        s: &Expr,
        name: &str,
        index: &Expr,
        value: &Expr,
    ) -> String {
        let t = self.types.get(name).copied().unwrap_or(Ty::Error);
        match t {
            Ty::Map(&(k, v)) => {
                let key = self.expr_want(index, k);
                let value = self.expr_want(value, v);
                format!(
                    "{{ let mz_k = {key}; let mz_v = {value}; {}.insert(mz_k, mz_v); }}",
                    ident(name)
                )
            }
            Ty::List(el) => {
                let i = self.expr(index);
                let value = self.expr_want(value, *el);
                let at = self.site(s);
                format!(
                    "{{ let mz_i = {i}; let mz_v = {value}; mz_set_index(&mut {}, mz_i, mz_v, {at}); }}",
                    ident(name)
                )
            }
            _ => "()".to_string(),
        }
    }

    /// `x in c` and `c is none`, `c is not none`, as Rust.
    pub(super) fn collection_compare(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
    ) -> Option<String> {
        match (op, &lhs.kind, &rhs.kind) {
            (BinOp::In, _, _) => {
                let el = self.ty(rhs).element().unwrap_or(Ty::Error);
                let x = self.expr_want(lhs, el);
                let wanted = Ty::list(self.ty(lhs));
                let c = match rhs.kind {
                    ExprKind::List(_) => format!("({})", self.bracket(rhs, wanted)),
                    _ => self.place(rhs),
                };
                let method = if matches!(self.ty(rhs), Ty::Map(_)) {
                    "contains_key"
                } else {
                    "contains"
                };
                Some(format!("{c}.{method}(&{x})"))
            }
            (BinOp::Is | BinOp::IsNot, _, ExprKind::None)
            | (BinOp::Is | BinOp::IsNot, ExprKind::None, _) => {
                let side = if matches!(rhs.kind, ExprKind::None) {
                    lhs
                } else {
                    rhs
                };
                let c = self.place(side);
                let not = if op == BinOp::IsNot { "!" } else { "" };
                Some(format!("{not}{c}.is_empty()"))
            }
            _ => None,
        }
    }

    /// `x otherwise d` (§8.1): `d` runs only when `x` is `None`.
    pub(super) fn otherwise(&mut self, lhs: &Expr, rhs: &Expr) -> String {
        let inner = match self.ty(lhs) {
            Ty::Option(t) => *t,
            _ => Ty::Error,
        };
        let x = self.expr(lhs);
        let rt = self.ty(rhs);
        let (some, d) = if matches!(rt, Ty::Option(_)) {
            ("Some(mz_v)", self.expr(rhs))
        } else {
            ("mz_v", self.expr_want(rhs, inner))
        };
        format!("match {x} {{ Some(mz_v) => {some}, None => {d} }}")
    }

    /// The type of `x otherwise d`.
    pub(super) fn otherwise_ty(&self, lhs: &Expr, rhs: &Expr) -> Ty {
        match self.ty(rhs) {
            t @ Ty::Option(_) => t,
            _ => match self.ty(lhs) {
                Ty::Option(t) => *t,
                _ => Ty::Error,
            },
        }
    }

    /// The `fn` an argument names.
    fn named_fn(&self, e: &Expr) -> Option<&crate::program::FnDecl> {
        match &e.kind {
            ExprKind::Name(n) if !self.types.contains_key(n) => self.fns.get(n.as_str()).copied(),
            _ => None,
        }
    }

    /// The type a collection method returns.
    pub(super) fn method_ty(&self, rt: Ty, name: &str, args: &[Expr]) -> Ty {
        let el = rt.element().unwrap_or(Ty::Error);
        let fn_ret = |k: usize| {
            args.get(k)
                .and_then(|a| self.named_fn(a))
                .and_then(|f| f.ret)
                .map_or(Ty::Error, |r| r.ty)
        };
        match name {
            "length" | "count" => Ty::Int,
            "slice" => Ty::option(rt),
            "keys" | "to_list" => Ty::list(el),
            "values" => match rt {
                Ty::Map(&(_, v)) => Ty::list(v),
                _ => Ty::Error,
            },
            "map" => Ty::list(fn_ret(0)),
            "filter" | "sort_by" => rt,
            "join" => Ty::Text,
            "sum" => el,
            "any" | "all" => Ty::Bool,
            "first" => Ty::option(el),
            "fold" => fn_ret(1),
            "group_by" => Ty::map(fn_ret(0), rt),
            _ => Ty::Nothing,
        }
    }

    /// A collection method (§9.2, §9.4) as Rust.
    pub(super) fn method_call(
        &mut self,
        e: &Expr,
        recv: &Expr,
        name: &str,
        args: &[Expr],
    ) -> String {
        let rt = self.ty(recv);
        let el = rt.element().unwrap_or(Ty::Error);
        let r = self.place(recv);
        let f = |k: usize| match args.get(k).map(|a| &a.kind) {
            Some(ExprKind::Name(n)) => fn_ident(n),
            _ => "()".to_string(),
        };
        match name {
            "length" => format!("mz_len({r}.len())"),
            "keys" => format!("{r}.keys().cloned().collect::<Vec<_>>()"),
            "values" => format!("{r}.values().cloned().collect::<Vec<_>>()"),
            "to_list" => format!("{r}.iter().cloned().collect::<Vec<_>>()"),
            "slice" => {
                let a = self.expr(&args[0]);
                let b = self.expr(&args[1]);
                format!("mz_slice(&{r}, {a}, {b})")
            }
            "join" => {
                let sep = self.expr(&args[0]);
                format!("mz_join(&{r}, &{sep})")
            }
            "sum" if el == Ty::Float => format!("mz_sum_float(&{r})"),
            "sum" => {
                let at = self.site(e);
                format!("mz_sum_int(&{r}, {at})")
            }
            "push" => {
                let v = self.expr_want(&args[0], el);
                format!(
                    "{{ let mz_v = {v}; {}.push(mz_v); }}",
                    self.receiver_name(recv)
                )
            }
            "insert" => {
                let v = self.expr_want(&args[0], el);
                format!(
                    "{{ let mz_v = {v}; {}.insert(mz_v); }}",
                    self.receiver_name(recv)
                )
            }
            "remove" => {
                let v = self.expr_want(&args[0], el);
                format!(
                    "{{ let mz_v = {v}; {}.remove(&mz_v); }}",
                    self.receiver_name(recv)
                )
            }
            "fold" => {
                let acc = self
                    .named_fn(&args[1])
                    .and_then(|d| d.params.first())
                    .map_or(Ty::Error, |p| p.ty.ty);
                let init = self.expr_want(&args[0], acc);
                format!("mz_fold(&{r}, {init}, {})", f(1))
            }
            "map" | "filter" | "count" | "any" | "all" | "first" | "sort_by" | "group_by" => {
                format!("mz_{name}(&{r}, {})", f(0))
            }
            _ => "()".to_string(),
        }
    }

    /// The `var` a mutation changes: its name, never a clone of it.
    fn receiver_name(&self, recv: &Expr) -> String {
        match &recv.kind {
            ExprKind::Name(n) => ident(n),
            _ => "()".to_string(),
        }
    }
}
