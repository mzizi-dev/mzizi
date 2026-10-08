//! Checking RFC-0013 §9 in a function body: bracket literals (`MZ0961`), indexing that
//! returns an option (§3.7), `in`, `otherwise` (§8.1, as far as indexing needs), a
//! collection's emptiness (`c is none`), the methods of §9.2 and the named folds of §9.4,
//! each function argument's signature (`MZ0909`), mutation of a `var` only (`MZ0960`), key
//! types with an order (`MZ0964`), the spellings other languages use for these (`MZ0962`),
//! and an option used where its value is meant (`MZ0710`).

use super::{Confidence, FnCheck, FnDecl, Kind, Span, receiver_text};
use crate::collections::{self as c, Recv};
use crate::expr::{BinOp, Expr, ExprKind, Ty, UnOp, canonical};
use crate::resolve::nearest;

/// Whether `e` is a name or a path through names, which a fix may rewrite exactly
/// (RFC-0013 §16, `MZ0962`: "exact on names and paths").
fn is_path(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Name(_) | ExprKind::Variant { .. } => true,
        ExprKind::Field { base, .. } => is_path(base),
        _ => false,
    }
}

/// An operand's text where a level-7 operator (`in`, `is`) stands beside it.
fn operand_text(e: &Expr) -> String {
    let t = canonical(e);
    if e.level() >= 7 { format!("({t})") } else { t }
}

/// The confidence of an `MZ0962` fix that rewrites around `recv`.
fn path_confidence(recv: &Expr) -> Confidence {
    if is_path(recv) {
        Confidence::Exact
    } else {
        Confidence::Guess
    }
}

/// What a function argument of a fold must be: each parameter's type (`None`: any), and its
/// return type (`None`: any).
struct FnShape {
    params: Vec<Option<Ty>>,
    ret: Option<Ty>,
    /// How the shape reads in a diagnostic: `a fn from int to bool`.
    says: String,
}

impl<'a> FnCheck<'a> {
    /// `MZ0710`: an option used where its value is meant (RFC-0013 §8). There is no fix: the
    /// default is the author's (§8). True when it reported.
    pub(super) fn unnarrowed(&mut self, e: &Expr, t: Ty, how: &str) -> bool {
        let Ty::Option(inner) = t else {
            return false;
        };
        self.err(
            "MZ0710",
            e.span,
            format!(
                "`{}` is an {}, which is `none` when nothing is there, and {how} — give it a default: `{} otherwise <{}>`",
                canonical(e),
                t.name(),
                operand_text(e),
                inner.name()
            ),
        );
        true
    }

    /// A bracket literal (RFC-0013 §9.1), read against `want`, the type expected where it
    /// stands (`Ty::Nothing` when nothing is expected, `Ty::Error` when what was expected was
    /// already reported): a list, or a set where a set is expected, or a map for `k: v`
    /// entries. With nothing to say otherwise, it is a list.
    pub(super) fn bracket(&mut self, e: &Expr, want: Ty) -> Ty {
        match &e.kind {
            ExprKind::List(items) => match want {
                Ty::List(t) | Ty::Set(t) => {
                    for item in items {
                        let it = self.expr_want(item, *t);
                        self.element_fits(item, it, *t, want);
                    }
                    want
                }
                Ty::Map(_) if items.is_empty() => want,
                _ if items.is_empty() => {
                    // `Ty::Nothing` is nothing expected; `Ty::Error`, an expected type
                    // already reported; any other type, one the caller reports a mismatch
                    // with.
                    if want != Ty::Nothing && !want.has_error() {
                        self.err(
                            "MZ0711",
                            e.span,
                            format!(
                                "`[]` is an empty collection, and {} is expected here",
                                want.name()
                            ),
                        );
                        return Ty::Error;
                    }
                    if want == Ty::Nothing {
                        self.err(
                            "MZ0961",
                            e.span,
                            "`[]` with nothing to say what it holds — write its type where it is bound: `let xs: list(int) = []`, `map(text, int)` or `set(int)`",
                        );
                    }
                    Ty::Error
                }
                _ => {
                    // The first element that is not itself `[]` fixes the type of the rest.
                    let anchor = items.iter().position(|i| !is_empty_bracket(i)).unwrap_or(0);
                    let t0 = self.expr(&items[anchor]);
                    let mut mixed = None;
                    for (k, item) in items.iter().enumerate() {
                        if k == anchor {
                            continue;
                        }
                        let it = self.expr_want(item, t0);
                        if it != t0 && !it.has_error() && !t0.has_error() && mixed.is_none() {
                            mixed = Some((item, it));
                        }
                    }
                    if self.element_kind(&items[anchor], t0) {
                        return Ty::Error;
                    }
                    if let Some((item, it)) = mixed {
                        self.err(
                            "MZ0961",
                            e.span,
                            format!(
                                "this list's elements have different types: `{}` is {}, and `{}` is {} — a list holds one type",
                                canonical(&items[anchor]),
                                t0.name(),
                                canonical(item),
                                it.name()
                            ),
                        );
                        return Ty::Error;
                    }
                    Ty::list(t0)
                }
            },
            ExprKind::MapLit(entries) => {
                let (kt, vt) = match want {
                    Ty::Map(&(k, v)) => {
                        for (key, value) in entries {
                            let t = self.expr_want(key, k);
                            self.element_fits(key, t, k, want);
                            let t = self.expr_want(value, v);
                            self.element_fits(value, t, v, want);
                        }
                        (k, v)
                    }
                    _ => {
                        // The first entry with no `[]` in it fixes the types of the rest,
                        // as a list's first element that is not `[]` does.
                        let anchor = entries
                            .iter()
                            .position(|(k, v)| !is_empty_bracket(k) && !is_empty_bracket(v))
                            .unwrap_or(0);
                        let k0 = self.expr(&entries[anchor].0);
                        let v0 = self.expr(&entries[anchor].1);
                        let mut mixed = None;
                        for (n, (key, value)) in entries.iter().enumerate() {
                            if n == anchor {
                                continue;
                            }
                            let k = self.expr_want(key, k0);
                            let v = self.expr_want(value, v0);
                            if mixed.is_none()
                                && ((k != k0 && !k.has_error() && !k0.has_error())
                                    || (v != v0 && !v.has_error() && !v0.has_error()))
                            {
                                mixed = Some(key);
                            }
                        }
                        if self.element_kind(&entries[anchor].0, k0)
                            | self.element_kind(&entries[anchor].1, v0)
                        {
                            return Ty::Error;
                        }
                        if let Some(key) = mixed {
                            self.err(
                                "MZ0961",
                                e.span,
                                format!(
                                    "this map's entries have different types: the first is {}: {}, and the one at `{}` differs — a map has one key type and one value type",
                                    k0.name(),
                                    v0.name(),
                                    canonical(key)
                                ),
                            );
                            return Ty::Error;
                        }
                        if !k0.is_key() {
                            self.err(
                                "MZ0964",
                                entries[anchor].0.span,
                                format!(
                                    "a map's keys are kept in key order, and {} has no order to keep — a key is an int, a text, a bool or an enum (RFC-0013 §2)",
                                    k0.name()
                                ),
                            );
                            return Ty::Error;
                        }
                        (k0, v0)
                    }
                };
                // A key written twice: one of the two values is a mistake (§9.1).
                let mut seen: std::collections::BTreeSet<String> = Default::default();
                for (key, _) in entries {
                    if !is_literal(key) {
                        continue;
                    }
                    let text = canonical(key);
                    if !seen.insert(text.clone()) {
                        self.err(
                            "MZ0961",
                            key.span,
                            format!(
                                "the key `{text}` is written twice in this map literal — one of its two values is a mistake; delete one entry"
                            ),
                        );
                        return Ty::Error;
                    }
                }
                if kt.has_error() || vt.has_error() {
                    return Ty::Error;
                }
                // A map literal where a list or set is expected: the caller reports it.
                Ty::map(kt, vt)
            }
            _ => Ty::Error,
        }
    }

    /// An element of a literal whose type `t` cannot be one: a call that returns nothing, or
    /// a result. True when it reported.
    fn element_kind(&mut self, item: &Expr, t: Ty) -> bool {
        if t == Ty::Nothing {
            self.err(
                "MZ0711",
                item.span,
                format!(
                    "`{}` returns nothing, so it cannot be an element",
                    canonical(item)
                ),
            );
            return true;
        }
        self.unhandled_result(item, t, "a collection holds values, not results")
    }

    /// An element read against its collection's element type `want`.
    fn element_fits(&mut self, item: &Expr, t: Ty, want: Ty, of: Ty) {
        if t == want || t.has_error() || want.has_error() {
            return;
        }
        if self.element_kind(item, t) {
            return;
        }
        self.err(
            "MZ0711",
            item.span,
            format!(
                "`{}` is {}, and a {} holds {}",
                canonical(item),
                t.name(),
                of.name(),
                want.name()
            ),
        );
    }

    /// `base[index]` (RFC-0013 §3.7): an `option` of a list's element or a map's value.
    pub(super) fn index(&mut self, e: &Expr, base: &Expr, index: &Expr) -> Ty {
        let bt = self.expr(base);
        if self.unnarrowed(base, bt, "indexing needs its value") {
            self.expr(index);
            return Ty::Error;
        }
        match bt {
            Ty::List(t) => {
                let it = self.expr_want(index, Ty::Int);
                if it != Ty::Int
                    && !it.has_error()
                    && !self.unnarrowed(index, it, "an index is an int")
                {
                    self.err(
                        "MZ0711",
                        index.span,
                        format!(
                            "`{}` is {}, and a list's index is an int, counted from 0",
                            canonical(index),
                            it.name()
                        ),
                    );
                    return Ty::Error;
                }
                Ty::option(*t)
            }
            Ty::Map(&(k, v)) => {
                let it = self.expr_want(index, k);
                if it != k && !it.has_error() && !self.unnarrowed(index, it, "a key is a value") {
                    self.err(
                        "MZ0711",
                        index.span,
                        format!(
                            "`{}` is {}, and the keys of `{}` are {}",
                            canonical(index),
                            it.name(),
                            canonical(base),
                            k.name()
                        ),
                    );
                    return Ty::Error;
                }
                Ty::option(v)
            }
            Ty::Error => {
                self.expr(index);
                Ty::Error
            }
            Ty::Text => {
                self.expr(index);
                self.err(
                    "MZ0919",
                    e.span,
                    "indexing text is designed (RFC-0013 §3.7, §10) but not built yet: text operations are C6's",
                );
                Ty::Error
            }
            Ty::Set(_) => {
                self.expr(index);
                self.err(
                    "MZ0711",
                    e.span,
                    format!(
                        "`{}` is a {}, which has no positions — ask `x in {}`, or index `{}.to_list()`",
                        canonical(base),
                        bt.name(),
                        canonical(base),
                        receiver_text(base)
                    ),
                );
                Ty::Error
            }
            _ => {
                self.expr(index);
                self.err(
                    "MZ0711",
                    e.span,
                    format!(
                        "`{}` is {}, and only a list or a map is indexed",
                        canonical(base),
                        bt.name()
                    ),
                );
                Ty::Error
            }
        }
    }

    /// `none`, outside `c is none`: an option's absence as a value is the "C4 options"
    /// follow-up's (RFC-0013 §8, §18.2).
    pub(super) fn none_value(&mut self, e: &Expr) -> Ty {
        self.err(
            "MZ0919",
            e.span,
            "`none` as a value, and narrowing an option with `when x is none`, are designed (RFC-0013 §8) but not built yet — this slice reads `c is none` (a collection is empty) and `x otherwise d`",
        );
        Ty::Error
    }

    /// `a is none` and `a is not none` (RFC-0013 §3.3): a collection's emptiness. `side` is
    /// the operand that is not `none`.
    pub(super) fn is_none(&mut self, e: &Expr, side: &Expr) -> Ty {
        let t = self.expr(side);
        match t {
            _ if t.is_collection() || t.has_error() => Ty::Bool,
            Ty::Option(_) => {
                self.err(
                    "MZ0919",
                    e.span,
                    "narrowing an option with `is none` is designed (RFC-0013 §8) but not built yet — give it a default with `otherwise`",
                );
                Ty::Bool
            }
            _ => {
                self.err(
                    "MZ0912",
                    e.span,
                    format!(
                        "`{}` is {}, which is never `none` — `is none` asks whether a list, a map or a set is empty",
                        canonical(side),
                        t.name()
                    ),
                );
                Ty::Bool
            }
        }
    }

    /// `c is []` and `c.length() is 0`, with their negations: `MZ0962`, whose fix writes
    /// `c is none` (RFC-0013 §3.3). On a value that is not a collection, the comparison is
    /// checked here, its operand typed once. `None` when the comparison is not one of these.
    pub(super) fn emptiness_idiom(
        &mut self,
        e: &Expr,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
    ) -> Option<Ty> {
        if !matches!(op, BinOp::Is | BinOp::IsNot) {
            return None;
        }
        let word = if op == BinOp::Is { "is" } else { "is not" };
        match (&lhs.kind, &rhs.kind) {
            (_, ExprKind::List(items)) if items.is_empty() => {
                let t = self.expr(lhs);
                if !t.is_collection() {
                    if !self.unnarrowed(lhs, t, "`is` compares values") {
                        // `[]` read against `t`: one mismatch (`MZ0711`), or silence when
                        // `t` was already reported.
                        let r = self.bracket(rhs, if t.has_error() { Ty::Error } else { t });
                        self.binary(e, op, lhs, rhs, t, r);
                    }
                    return Some(Ty::Bool);
                }
                let fixed = format!("{} {word} none", operand_text(lhs));
                self.err_fix(
                    "MZ0962",
                    e.span,
                    format!("{}: `{fixed}`", super::COLLECTION_EMPTINESS),
                    rhs.span,
                    "none",
                    Confidence::Exact,
                );
                Some(Ty::Bool)
            }
            (
                ExprKind::Method {
                    recv,
                    name,
                    name_span,
                    args,
                    called,
                    ..
                },
                ExprKind::Int(0),
            ) if name == "length" && args.is_empty() => {
                let t = self.expr(recv);
                if !t.is_collection() {
                    let lt = self.method_on(lhs, recv, t, name, *name_span, args, *called);
                    let r = self.expr(rhs);
                    self.binary(e, op, lhs, rhs, lt, r);
                    return Some(Ty::Bool);
                }
                let parens = self.tight.contains(&e.span);
                self.collection_empty_fix(recv, op == BinOp::IsNot, e.span, parens);
                Some(Ty::Bool)
            }
            _ => None,
        }
    }

    /// One `MZ0962` at `whole`, a collection's length compared with 0, whose fix replaces it
    /// with `c is none` (or `is not none`), in parentheses where it stands inside another
    /// comparison.
    pub(super) fn collection_empty_fix(
        &mut self,
        recv: &Expr,
        negated: bool,
        whole: Span,
        parens: bool,
    ) {
        let word = if negated { "is not" } else { "is" };
        let mut fixed = format!("{} {word} none", operand_text(recv));
        if parens {
            fixed = format!("({fixed})");
        }
        let say = format!("{}: `{fixed}`", super::COLLECTION_EMPTINESS);
        if recv.has_error() {
            self.err("MZ0962", whole, say);
        } else {
            self.err_fix("MZ0962", whole, say, whole, fixed, path_confidence(recv));
        }
    }

    /// `not c.is_empty()`: one `MZ0962`, whose fix writes `c is not none`. On a value that
    /// is not a collection, the `not` and its method are checked here, the receiver typed
    /// once.
    pub(super) fn not_is_empty(&mut self, e: &Expr, operand: &Expr) -> Ty {
        let ExprKind::Method {
            recv,
            name,
            name_span,
            args,
            called,
            ..
        } = &operand.kind
        else {
            return self.expr(operand);
        };
        let t = self.expr(recv);
        if !t.is_collection() {
            let mt = self.method_on(operand, recv, t, name, *name_span, args, *called);
            if mt != Ty::Bool && !mt.has_error() {
                self.err(
                    "MZ0912",
                    e.span,
                    format!(
                        "`not` takes a bool, and `{}` is {}",
                        canonical(operand),
                        mt.name()
                    ),
                );
            }
            return Ty::Bool;
        }
        let fixed = format!("{} is not none", operand_text(recv));
        self.err_fix(
            "MZ0962",
            e.span,
            format!("{}: `{fixed}`", super::COLLECTION_EMPTINESS),
            e.span,
            fixed,
            path_confidence(recv),
        );
        Ty::Bool
    }

    /// `x in c` (RFC-0013 §3.3): an element of a list or a set, or a key of a map. A
    /// bracket literal on the right is read against `x`'s type. On text it is `MZ0962`,
    /// whose fix is `s.contains(t)`.
    pub(super) fn in_expr(&mut self, e: &Expr, lhs: &Expr, rhs: &Expr) -> Ty {
        let l = self.expr(lhs);
        if self.unnarrowed(lhs, l, "`in` asks about a value") {
            // The collection is read on its own, not against an option's type.
            match rhs.kind {
                ExprKind::List(_) | ExprKind::MapLit(_) => self.bracket(rhs, Ty::Error),
                _ => self.expr(rhs),
            };
            return Ty::Bool;
        }
        let r = if matches!(rhs.kind, ExprKind::List(_)) && !l.has_error() {
            self.expr_want(rhs, Ty::list(l))
        } else {
            self.expr(rhs)
        };
        if r == Ty::Text && l == Ty::Text {
            let fixed = format!("{}.contains({})", receiver_text(rhs), canonical(lhs));
            let say = format!(
                "`in` asks about a collection, and on text a substring is the method `s.contains(t)`: `{fixed}`"
            );
            if e.has_error() {
                self.err("MZ0962", e.span, say);
            } else {
                self.err_fix("MZ0962", e.span, say, e.span, fixed, Confidence::Exact);
            }
            return Ty::Bool;
        }
        if self.unnarrowed(rhs, r, "`in` asks about a collection") {
            return Ty::Bool;
        }
        self.binary(e, BinOp::In, lhs, rhs, l, r)
    }

    /// `x otherwise d` (RFC-0013 §8.1): `d` is read against the option's element type.
    pub(super) fn otherwise(&mut self, e: &Expr, lhs: &Expr, rhs: &Expr) -> Ty {
        let l = self.expr(lhs);
        let r = match l {
            Ty::Option(t)
                if !matches!(
                    rhs.kind,
                    ExprKind::Binary {
                        op: BinOp::Otherwise,
                        ..
                    }
                ) =>
            {
                self.expr_want(rhs, *t)
            }
            _ => self.expr(rhs),
        };
        if self.unhandled_result(lhs, l, "`otherwise` gives an option's default")
            | self.unhandled_result(rhs, r, "a default is a value")
        {
            return Ty::Error;
        }
        self.binary(e, BinOp::Otherwise, lhs, rhs, l, r)
    }

    /// What `for each` iterates (RFC-0013 §7.3), other than `range`: a list's elements. A map
    /// or a set is `MZ0711`, whose `exact` fix iterates `m.keys()` or `s.to_list()` (key
    /// order). Returns the loop binding's type.
    pub(super) fn loop_collection(&mut self, source: &Expr, t: Ty) -> Option<Ty> {
        match t {
            Ty::List(el) => Some(*el),
            Ty::Map(_) | Ty::Set(_) => {
                let method = if matches!(t, Ty::Map(_)) {
                    "keys"
                } else {
                    "to_list"
                };
                let fixed = format!("{}.{method}()", receiver_text(source));
                let say = format!(
                    "`for each` iterates a list, and `{}` is a {} — iterate `{fixed}`, in key order",
                    canonical(source),
                    t.name()
                );
                if source.has_error() {
                    self.err("MZ0711", source.span, say);
                } else {
                    self.err_fix(
                        "MZ0711",
                        source.span,
                        say,
                        source.span,
                        fixed,
                        path_confidence(source),
                    );
                }
                Some(Ty::Error)
            }
            Ty::Option(_) => {
                self.unnarrowed(source, t, "`for each` iterates a list");
                Some(Ty::Error)
            }
            _ => None,
        }
    }

    /// The receiver of a mutation (`push`, `remove`, `insert`, `xs[i] = v`): a `var`
    /// (`MZ0960` otherwise, with the `exact` fix `var` on a `let`). The `var` counts as
    /// changed, so it is not `MZ0924`.
    pub(super) fn mutated(&mut self, recv: &Expr, what: &str) {
        let ExprKind::Name(name) = &recv.kind else {
            self.err(
                "MZ0960",
                recv.span,
                format!(
                    "{what} changes a value no `var` holds, so the change would be lost — bind it to a `var` first"
                ),
            );
            return;
        };
        let Some(i) = self.visible(name) else {
            return;
        };
        let b = &self.bindings[i];
        match b.kind {
            Kind::Var => self.bindings[i].assigned = true,
            Kind::Poison => {}
            Kind::Let => {
                let kw = b.kw_span;
                // One report per binding: the fix makes every later change legal.
                self.bindings[i].kind = Kind::Var;
                self.bindings[i].assigned = true;
                let say = format!(
                    "{what} changes `{name}`, a `let`, which never changes — declare it with `var`"
                );
                match kw {
                    Some(kw) => {
                        self.err_fix("MZ0960", recv.span, say, kw, "var", Confidence::Exact)
                    }
                    None => self.err("MZ0960", recv.span, say),
                }
            }
            Kind::Param | Kind::Loop => {
                let which = if b.kind == Kind::Param {
                    "a parameter"
                } else {
                    "a `for each` binding"
                };
                self.err(
                    "MZ0960",
                    recv.span,
                    format!(
                        "{what} changes `{name}`, {which}, which cannot change — bind a `var` with its value and change that"
                    ),
                );
            }
        }
    }

    /// `name[index] = value` (RFC-0013 §9.2): a list's element, or a map's value.
    pub(super) fn index_assign(&mut self, name: &str, name_span: Span, index: &Expr, value: &Expr) {
        let recv = Expr {
            kind: ExprKind::Name(name.to_string()),
            span: name_span,
        };
        let t = self.expr(&recv);
        let (key, val) = match t {
            Ty::List(el) => (Ty::Int, *el),
            Ty::Map(&(k, v)) => (k, v),
            Ty::Error => {
                self.expr(index);
                self.expr(value);
                return;
            }
            _ => {
                self.expr(index);
                self.expr(value);
                let say = if matches!(t, Ty::Set(_)) {
                    format!(
                        "`{name}` is a {}, which has no positions — `{name}.insert(v)` adds one",
                        t.name()
                    )
                } else {
                    format!(
                        "`{name}` is {}, and only a list's element or a map's value is assigned through `[…]`",
                        t.name()
                    )
                };
                self.err("MZ0711", name_span, say);
                return;
            }
        };
        let it = self.expr_want(index, key);
        if it != key
            && !it.has_error()
            && !self.unhandled(index, it, || "an index is a value".into())
        {
            self.err(
                "MZ0711",
                index.span,
                format!(
                    "`{}` is {}, and `{name}`'s index is {}",
                    canonical(index),
                    it.name(),
                    key.name()
                ),
            );
        }
        let vt = self.expr_want(value, val);
        if vt != val
            && !vt.has_error()
            && !self.unhandled(value, vt, || "an element is a value".into())
        {
            self.err(
                "MZ0711",
                value.span,
                format!(
                    "`{}` is {}, and `{name}` holds {}",
                    canonical(value),
                    vt.name(),
                    val.name()
                ),
            );
        }
        self.mutated(&recv, &format!("`{name}[…] = …`"));
    }

    /// A function argument of `map`, `filter` or a fold (RFC-0013 §6.4, §9.3): the name of a
    /// `fn` of the program whose signature fits `shape` (`MZ0909` otherwise, quoting both).
    fn fn_arg(&mut self, call: &str, arg: &Expr, shape: &FnShape) -> Option<&'a FnDecl> {
        let named = match &arg.kind {
            ExprKind::Name(n) if self.visible(n).is_none() => self.fns.get(n.as_str()).copied(),
            ExprKind::Error => return None,
            _ => None,
        };
        let Some(f) = named else {
            if !matches!(arg.kind, ExprKind::Name(_)) || self.visible(&canonical(arg)).is_some() {
                self.expr(arg);
            }
            self.err(
                "MZ0909",
                arg.span,
                format!(
                    "`{call}` takes the name of a `fn` of the program, {} — `{}` is not one",
                    shape.says,
                    canonical(arg)
                ),
            );
            return None;
        };
        let fits = f.params.len() == shape.params.len()
            && f.params.iter().zip(&shape.params).all(|(p, want)| {
                want.is_none_or(|w| w == p.ty.ty || w.has_error() || p.ty.ty.has_error())
            })
            && match (shape.ret, f.ret) {
                (_, None) => false,
                (None, Some(_)) => true,
                (Some(w), Some(r)) => w == r.ty || w.has_error() || r.ty.has_error(),
            };
        if !fits {
            self.err(
                "MZ0909",
                arg.span,
                format!(
                    "`{call}` takes {}, and `{}` is `{}`",
                    shape.says,
                    f.name,
                    f.signature()
                ),
            );
            return None;
        }
        Some(f)
    }

    /// A method on a list, a map or a set (RFC-0013 §9.2, §9.4), or on an option, which has
    /// none (`MZ0710`). Its argument functions are not read as values.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn collection_method(
        &mut self,
        e: &Expr,
        recv: &Expr,
        rt: Ty,
        name: &str,
        name_span: Span,
        args: &[Expr],
        called: bool,
    ) -> Ty {
        if self.unnarrowed(recv, rt, &format!("`.{name}` needs its value")) {
            for a in args {
                self.expr(a);
            }
            return Ty::Error;
        }
        let kind = match rt {
            Ty::List(_) => Recv::List,
            Ty::Map(_) => Recv::Map,
            _ => Recv::Set,
        };
        let r = receiver_text(recv);
        // Spellings from other languages, each with its fix (§9.2, §9.4, §16).
        if let Some(t) = self.collection_idiom(e, recv, rt, &r, name, name_span, args, called) {
            return t;
        }
        let Some(m) = c::method(kind, name) else {
            return self.no_collection_method(rt, kind, name, name_span, args);
        };
        if !called {
            let say = format!(
                "`.{name}` is a method, and a method is always called with parentheses: `{r}.{name}(…)`"
            );
            if m.arity == 0 {
                self.err_fix(
                    "MZ0962",
                    name_span,
                    say,
                    Span::single(name_span.end_line, name_span.end_col, 0),
                    "()",
                    Confidence::Exact,
                );
            } else {
                self.err("MZ0962", name_span, say);
            }
            return Ty::Error;
        }
        if args.len() != m.arity {
            for a in args {
                if !matches!(a.kind, ExprKind::Name(_)) {
                    self.expr(a);
                }
            }
            self.err(
                "MZ0905",
                e.span,
                format!(
                    "`.{name}` on a {} takes {} argument{} — `{}`, and this call gives {}",
                    rt.name(),
                    m.arity,
                    if m.arity == 1 { "" } else { "s" },
                    m.grammar,
                    args.len()
                ),
            );
            return Ty::Error;
        }
        let el = rt.element().unwrap_or(Ty::Error);
        let call = format!("{r}.{name}");
        match name {
            "length" => Ty::Int,
            "keys" => Ty::list(el),
            "values" => match rt {
                Ty::Map(&(_, v)) => Ty::list(v),
                _ => Ty::Error,
            },
            "to_list" => Ty::list(el),
            "slice" => {
                for a in args {
                    let t = self.expr(a);
                    if t != Ty::Int
                        && !t.has_error()
                        && !self.unhandled(a, t, || "a position is an int".into())
                    {
                        self.err(
                            "MZ0905",
                            a.span,
                            format!(
                                "`{}` is {}, and `slice(a, to = b)` takes two ints",
                                canonical(a),
                                t.name()
                            ),
                        );
                    }
                }
                Ty::option(rt)
            }
            "join" => {
                let t = self.expr(&args[0]);
                if el != Ty::Text && !el.has_error() {
                    self.err(
                        "MZ0711",
                        e.span,
                        format!(
                            "`.join` joins a list of text, and `{}` is a {}",
                            canonical(recv),
                            rt.name()
                        ),
                    );
                    return Ty::Error;
                }
                if t != Ty::Text
                    && !t.has_error()
                    && !self.unhandled(&args[0], t, || "a separator is text".into())
                {
                    self.err(
                        "MZ0905",
                        args[0].span,
                        format!(
                            "`{}` is {}, and `.join` takes a text separator",
                            canonical(&args[0]),
                            t.name()
                        ),
                    );
                }
                Ty::Text
            }
            "sum" => match el {
                Ty::Int | Ty::Float => el,
                t if t.has_error() => Ty::Error,
                _ => {
                    self.err(
                        "MZ0711",
                        e.span,
                        format!(
                            "`.sum()` adds a list of ints or of floats, and `{}` is a {}",
                            canonical(recv),
                            rt.name()
                        ),
                    );
                    Ty::Error
                }
            },
            "push" | "insert" => {
                let t = self.expr_want(&args[0], el);
                self.element_fits(&args[0], t, el, rt);
                self.mutated(recv, &format!("`.{name}`"));
                Ty::Nothing
            }
            "remove" => {
                let t = self.expr_want(&args[0], el);
                if t != el
                    && !t.has_error()
                    && !el.has_error()
                    && !self.unhandled(&args[0], t, || "a key is a value".into())
                {
                    self.err(
                        "MZ0905",
                        args[0].span,
                        format!(
                            "`{}` is {}, and `{}`'s {} are {}",
                            canonical(&args[0]),
                            t.name(),
                            canonical(recv),
                            if kind == Recv::Map {
                                "keys"
                            } else {
                                "elements"
                            },
                            el.name()
                        ),
                    );
                }
                self.mutated(recv, "`.remove`");
                Ty::Nothing
            }
            "map" => {
                let shape = FnShape {
                    params: vec![Some(el)],
                    ret: None,
                    says: format!("a `fn` of one {} that returns a value", el.name()),
                };
                match self.fn_arg(&call, &args[0], &shape) {
                    Some(f) => match f.ret.map(|r| r.ty) {
                        Some(u) if u.as_result().is_some() => {
                            self.err("MZ0950", args[0].span, format!("`{}` returns a {}, and a list holds values, not results — map with a `fn` that handles it", f.name, u.name()));
                            Ty::Error
                        }
                        Some(u) => Ty::list(u),
                        None => Ty::Error,
                    },
                    None => Ty::Error,
                }
            }
            "filter" | "count" | "any" | "all" | "first" => {
                let shape = FnShape {
                    params: vec![Some(el)],
                    ret: Some(Ty::Bool),
                    says: format!("a `fn` from {} to bool", el.name()),
                };
                self.fn_arg(&call, &args[0], &shape);
                match name {
                    "filter" => rt,
                    "count" => Ty::Int,
                    "first" => Ty::option(el),
                    _ => Ty::Bool,
                }
            }
            "sort_by" | "group_by" => {
                let shape = FnShape {
                    params: vec![Some(el)],
                    ret: None,
                    says: format!(
                        "a `fn` from {} to a key: an int, a text, a bool or an enum",
                        el.name()
                    ),
                };
                let Some(f) = self.fn_arg(&call, &args[0], &shape) else {
                    return Ty::Error;
                };
                let k = f.ret.map_or(Ty::Error, |r| r.ty);
                if !k.is_key() {
                    self.err(
                        "MZ0964",
                        args[0].span,
                        format!(
                            "`{call}` orders by what `{}` returns, and {} has no order to keep — a key is an int, a text, a bool or an enum (RFC-0013 §2, §9.4)",
                            f.name,
                            k.name()
                        ),
                    );
                    return Ty::Error;
                }
                if name == "sort_by" {
                    rt
                } else {
                    Ty::map(k, rt)
                }
            }
            "fold" => {
                // The step fixes the accumulator's type, so `init` is read against it.
                let step = &args[1];
                let named = match &step.kind {
                    ExprKind::Name(n) if self.visible(n).is_none() => {
                        self.fns.get(n.as_str()).copied()
                    }
                    _ => None,
                };
                let acc = named
                    .and_then(|f| f.params.first())
                    .map_or(Ty::Error, |p| p.ty.ty);
                let it = if acc == Ty::Error {
                    self.expr(&args[0])
                } else {
                    self.expr_want(&args[0], acc)
                };
                if self.unhandled(&args[0], it, || "an accumulator is a value".into()) {
                    return Ty::Error;
                }
                let shape = FnShape {
                    params: vec![Some(it), Some(el)],
                    ret: Some(it),
                    says: format!(
                        "a `fn` from the accumulator, {}, and an element, {}, to the next accumulator, {}",
                        it.name(),
                        el.name(),
                        it.name()
                    ),
                };
                match self.fn_arg(&call, step, &shape) {
                    Some(_) => it,
                    None => Ty::Error,
                }
            }
            _ => Ty::Error,
        }
    }

    /// `MZ0708`: a method a collection does not have, with the nearest of its own.
    fn no_collection_method(
        &mut self,
        rt: Ty,
        kind: Recv,
        name: &str,
        name_span: Span,
        args: &[Expr],
    ) -> Ty {
        for a in args {
            if !matches!(a.kind, ExprKind::Name(_)) {
                self.expr(a);
            }
        }
        let own = c::methods_of(kind);
        match nearest(name, own.iter().copied()) {
            Some((near, _)) => self.err_fix(
                "MZ0708",
                name_span,
                format!(
                    "a {} has no method `{name}` — did you mean `{near}`?",
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
                    "a {} has no method `{name}` — it has {}",
                    rt.name(),
                    own.iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
        }
        Ty::Error
    }

    /// The spellings other languages bring for a collection's operations (`MZ0962`): each
    /// reported once with its fix, and the expression typed as the fix would type it.
    #[allow(clippy::too_many_arguments)]
    fn collection_idiom(
        &mut self,
        e: &Expr,
        recv: &Expr,
        rt: Ty,
        r: &str,
        name: &str,
        name_span: Span,
        args: &[Expr],
        called: bool,
    ) -> Option<Ty> {
        let list = matches!(rt, Ty::List(_));
        let el = rt.element().unwrap_or(Ty::Error);
        let say_fix = |s: &mut Self, say: String, span: Span, fixed: String, conf: Confidence| {
            if e.has_error() {
                s.err("MZ0962", e.span, say);
            } else {
                s.err_fix("MZ0962", e.span, say, span, fixed, conf);
            }
        };
        match name {
            // `.len()`, `.size()`, `.count()` with no argument, and `.length` with no `()`.
            "len" | "size" | "count" if called && args.is_empty() => {
                let fixed = format!("{r}.length()");
                say_fix(
                    self,
                    format!("`.{name}()` is another language's — a collection's size is `{fixed}`"),
                    e.span,
                    fixed,
                    path_confidence(recv),
                );
                Some(Ty::Int)
            }
            "to_string" if called && args.is_empty() => {
                let fixed = format!("\"{{{}}}\"", canonical(recv));
                let say = "`.to_string()` is not a Mzizi method — a value's text is `\"{x}\"`"
                    .to_string();
                if recv.has_error() || recv.has_text_literal() || !crate::expr::has_text_form(rt) {
                    self.err("MZ0962", e.span, say);
                } else {
                    self.err_fix(
                        "MZ0962",
                        e.span,
                        format!("{say}: write `{fixed}`"),
                        e.span,
                        fixed,
                        Confidence::Exact,
                    );
                }
                Some(Ty::Text)
            }
            "append" if list && called && args.len() == 1 => {
                for a in args {
                    self.expr(a);
                }
                // A change, as `push` is: the `var` is not `MZ0924`, and a `let` is `MZ0960`.
                self.mutated(recv, "`.append`");
                say_fix(
                    self,
                    format!("`.append` is Python's — a list grows with `{r}.push(…)`"),
                    name_span,
                    "push".to_string(),
                    Confidence::Exact,
                );
                Some(Ty::Nothing)
            }
            "contains" | "includes" | "has" if called && args.len() == 1 => {
                let at = self.expr_want(&args[0], el);
                let _ = at;
                let fixed = format!("{} in {}", operand_text(&args[0]), operand_text(recv));
                say_fix(
                    self,
                    format!("`.{name}(…)` on a collection is `in`: `{fixed}`"),
                    e.span,
                    fixed,
                    path_confidence(recv),
                );
                Some(Ty::Bool)
            }
            "get" if called && args.len() == 1 && matches!(rt, Ty::List(_) | Ty::Map(_)) => {
                let want = if list { Ty::Int } else { el };
                self.expr_want(&args[0], want);
                let fixed = format!("{r}[{}]", canonical(&args[0]));
                say_fix(
                    self,
                    format!("there is no `.get` — indexing already gives an option: `{fixed}`"),
                    e.span,
                    fixed,
                    Confidence::Exact,
                );
                Some(match rt {
                    Ty::List(t) => Ty::option(*t),
                    Ty::Map(&(_, v)) => Ty::option(v),
                    _ => Ty::Error,
                })
            }
            "is_empty" if called && args.is_empty() => {
                let mut fixed = format!("{} is none", operand_text(recv));
                // Beside another comparison, `c is none` needs parentheses the call did not:
                // `(xs is none) is true`, not the chain `xs is none is true`.
                if self.tight.contains(&e.span) {
                    fixed = format!("({fixed})");
                }
                say_fix(
                    self,
                    format!("{}: `{fixed}`", super::COLLECTION_EMPTINESS),
                    e.span,
                    fixed,
                    path_confidence(recv),
                );
                Some(Ty::Bool)
            }
            // The folds under another language's name, with a direct equivalent (§9.4).
            "find" | "some" | "every" | "sorted_by" if list && called => {
                let to = match name {
                    "find" => "first",
                    "some" => "any",
                    "every" => "all",
                    _ => "sort_by",
                };
                // Typed as the fold it names, so its function argument is checked too.
                let t = self.collection_method(e, recv, rt, to, name_span, args, called);
                // At the name, where the lexer's `MZ0101` for `sortedBy` stands: the two are
                // one mistake, and `mz check` keeps this one (`lib.rs`).
                // `.first` returns an option where JavaScript's `.find` returns the element
                // or `undefined`, so the program around it may still need a default: a guess.
                let (say, conf) = if name == "find" {
                    (
                        "`.find` is another language's — Mzizi's is `.first`, which returns an option read with `otherwise`".to_string(),
                        Confidence::Guess,
                    )
                } else {
                    (
                        format!("`.{name}` is another language's — Mzizi's is `.{to}`"),
                        Confidence::Exact,
                    )
                };
                self.err_fix("MZ0962", name_span, say, name_span, to, conf);
                Some(t)
            }
            "reduce" if list && called && args.len() == 2 => {
                let fixed = format!(
                    "{r}.fold({}, step = {})",
                    canonical(&args[1]),
                    canonical(&args[0])
                );
                say_fix(
                    self,
                    format!("`.reduce(f, init)` is JavaScript's — Mzizi's is `{fixed}`"),
                    e.span,
                    fixed,
                    Confidence::Guess,
                );
                Some(Ty::Error)
            }
            _ => None,
        }
    }

    /// Python's `len(x)` and `sorted(xs, key = f)`, free functions where Mzizi has methods
    /// (`MZ0962`). `None` when `name` is not one of them.
    pub(super) fn free_collection(
        &mut self,
        name: &str,
        args: &[Expr],
        labelled: bool,
        at: Span,
    ) -> Option<Ty> {
        match (name, args) {
            ("len", [x]) => {
                let t = self.expr(x);
                if !(t.is_collection() || t == Ty::Text) {
                    if !t.has_error() {
                        self.err(
                            "MZ0707",
                            at,
                            format!(
                                "there is no function `len`, and `{}` is {}, which has no size — a list's, map's or set's is `x.length()`",
                                canonical(x),
                                t.name()
                            ),
                        );
                    }
                    return Some(Ty::Error);
                }
                let fixed = format!("{}.length()", receiver_text(x));
                let say = format!("`len(…)` is Python's — a size is the method `{fixed}`");
                if x.has_error() {
                    self.err("MZ0962", at, say);
                } else {
                    // Python's `len` counts a text's scalar values, as `length()` does (C6), on
                    // any text; a collection's rewrite around its receiver is exact on a path.
                    let conf = if t == Ty::Text {
                        Confidence::Exact
                    } else {
                        path_confidence(x)
                    };
                    self.err_fix("MZ0962", at, say, at, fixed, conf);
                }
                Some(Ty::Int)
            }
            ("sorted", [xs, ..]) => {
                let t = self.expr(xs);
                if !matches!(t, Ty::List(_)) {
                    if !t.has_error() {
                        self.err(
                            "MZ0707",
                            at,
                            format!(
                                "there is no function `sorted`, and `{}` is {} — a list sorts with `xs.sort_by(f)`",
                                canonical(xs),
                                t.name()
                            ),
                        );
                    }
                    return Some(Ty::Error);
                }
                match args.get(1) {
                    // Only `key = f` is Python's sort key: another label (`reverse = true`)
                    // has been reported as a named argument (`MZ0905`), and has no fix here.
                    Some(key) if args.len() == 2 && labelled && !xs.has_error() => {
                        let fixed = format!("{}.sort_by({})", receiver_text(xs), canonical(key));
                        // Exact when the key is a function of the program; anything else
                        // `sort_by` would refuse (`MZ0909`), so the rewrite is a guess.
                        let conf = match &key.kind {
                            ExprKind::Name(n) if self.fns.contains_key(n.as_str()) => {
                                Confidence::Exact
                            }
                            _ => Confidence::Guess,
                        };
                        self.err_fix(
                            "MZ0962",
                            at,
                            format!("`sorted(xs, key = f)` is Python's — Mzizi's is `{fixed}`"),
                            at,
                            fixed,
                            conf,
                        );
                    }
                    _ => self.err(
                        "MZ0962",
                        at,
                        "`sorted(…)` is Python's — Mzizi sorts a list by a key function, `xs.sort_by(f)`, and a sort with no key is not in M1 (RFC-0013 §9.4, §20 Q26)",
                    ),
                }
                Some(t)
            }
            _ => None,
        }
    }

    /// `range(a, to = b)` as a value, anywhere: the list of `int`s from `a` up to `b`.
    pub(super) fn range_value(&mut self, at: Span, args: &[Expr]) -> Ty {
        self.range_args(at, args);
        Ty::list(Ty::Int)
    }
}

/// A literal key (`1`, `"a"`, `true`, a variant), whose repetition in a map literal the
/// checker can see.
fn is_literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Variant { .. } => true,
        ExprKind::Text(parts) => parts
            .iter()
            .all(|p| matches!(p, crate::expr::TextPart::Lit(_))),
        ExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } => matches!(operand.kind, ExprKind::Int(_)),
        _ => false,
    }
}

/// `[]`, whose type comes from where it stands.
fn is_empty_bracket(e: &Expr) -> bool {
    matches!(&e.kind, ExprKind::List(v) if v.is_empty())
}
