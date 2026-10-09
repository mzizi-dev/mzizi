//! Text methods in a function body (RFC-0013 §10, tracker row C6): the checker's half.
//!
//! The method table is [`crate::text`]'s. Here: a call's arity and types (`MZ0905`), a
//! method text does not have (`MZ0708`, with the nearest name), a constant fault a call can
//! see (a negative `repeat` count and an empty literal `split` separator, `MZ0915`), and
//! other languages' spellings (`MZ0962`): `len(s)`, `s.len()`, `s.length`, `s.strip()`,
//! `s.toUpperCase()`, `s.replaceAll(a, b)`, `s.substring(a, b)`, `int(s)`, `parseInt(s)`,
//! and emptiness asked as `s.is_empty()` or `s.length() is 0`, which Mzizi asks as `s is ""`
//! (§3.3).

use super::{FnCheck, receiver_text};
use crate::diagnostic::{Confidence, Span};
use crate::expr::{BinOp, Expr, ExprKind, TextPart, Ty, UnOp, canonical, fold};
use crate::resolve::nearest;
use crate::text;

/// `x.is_empty()`, or `x.is_empty` with no parentheses (which the parser reads as a
/// dotted path): its receiver.
fn is_empty_call(e: &Expr) -> Option<&Expr> {
    match &e.kind {
        ExprKind::Method {
            recv, name, args, ..
        } if name == "is_empty" && args.is_empty() => Some(recv),
        ExprKind::Field { base, name, .. } if name == "is_empty" => Some(base),
        _ => None,
    }
}

/// `x.length()`, or another language's length of `x`: `x.len()`, `x.size()`, `x.count()`,
/// `x.length` with no parentheses, or Python's `len(x)`: its receiver. A `fn len` of the
/// program's own is the caller's to rule out.
fn length_call(e: &Expr) -> Option<&Expr> {
    let lengthy = |n: &str| matches!(n, "length" | "len" | "size");
    match &e.kind {
        ExprKind::Method {
            recv,
            name,
            args,
            called,
            ..
        } if args.is_empty() && (lengthy(name) || (*called && name == "count")) => Some(recv),
        ExprKind::Field { base, name, .. } if lengthy(name) => Some(base),
        ExprKind::Call { name, args, .. } if name == "len" && args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

/// `x.length() is 0`, `is not 0` or `> 0`: the receiver, and whether it asks "is it not
/// empty".
fn length_compared(e: &Expr) -> Option<(&Expr, bool)> {
    match &e.kind {
        ExprKind::Binary { op, lhs, rhs, .. }
            if matches!(op, BinOp::Is | BinOp::IsNot | BinOp::Gt)
                && matches!(rhs.kind, ExprKind::Int(0)) =>
        {
            length_call(lhs).map(|r| (r, *op != BinOp::Is))
        }
        _ => None,
    }
}

/// The text `span` covers, as the source spells it. A camelCase name is read as its
/// snake_case form by the parser, so the message names it from the source instead.
/// `None` when the span is not on one line of `src`.
pub(super) fn spelled(src: &str, span: Span) -> Option<String> {
    if span.start_line != span.end_line {
        return None;
    }
    let line = src
        .lines()
        .nth((span.start_line as usize).checked_sub(1)?)?;
    let chars: Vec<char> = line.chars().collect();
    let from = (span.start_col as usize).checked_sub(1)?;
    let to = (span.end_col as usize).checked_sub(1)?;
    chars.get(from..to).map(|c| c.iter().collect())
}

impl FnCheck<'_> {
    /// Remember that `e` stands where `x is ""` would need parentheses: the operand of a
    /// comparison or of `not`. Only an `is_empty()` call is remembered, so the list stays
    /// as short as the calls written.
    pub(super) fn mark_tight(&mut self, e: &Expr) {
        if is_empty_call(e).is_some() || length_compared(e).is_some() {
            self.tight.push(e.span);
        }
    }

    /// Emptiness asked another language's way, on text (RFC-0013 §3.3): `s.length() is 0`,
    /// `s.length() is not 0`, `s.length() > 0`, and `not s.is_empty()`. Each is one
    /// `MZ0962` whose `exact` fix is `s is ""` or `s is not ""`. `None` when `e` is none of
    /// these on a receiver visibly typed `text`, so it is checked as any other expression
    /// (a bare `s.is_empty()` is [`Self::text_method`]'s).
    pub(super) fn text_emptiness(&mut self, e: &Expr) -> Option<Ty> {
        let (recv, negated, at, by_is_empty) = match &e.kind {
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
            } => match (is_empty_call(operand), length_compared(operand)) {
                (Some(r), _) => (r, true, e.span, true),
                (None, Some((r, not_empty))) => (r, !not_empty, e.span, false),
                _ => return None,
            },
            ExprKind::Binary { .. } => {
                let (r, not_empty) = length_compared(e)?;
                (r, not_empty, e.span, false)
            }
            _ => return None,
        };
        if self.fns.contains_key("len") {
            return None;
        }
        let shallow = self.shallow_ty(recv);
        // A collection's length compared with 0, in any of these spellings, is one
        // `MZ0962` whose fix writes `c is none` in one pass (`not c.is_empty()` is
        // [`Self::not_is_empty`]'s).
        if shallow.is_collection() && !by_is_empty {
            let t = self.expr(recv);
            if t.is_collection() {
                let parens = self.tight.contains(&e.span);
                self.collection_empty_fix(recv, negated, e.span, parens);
            }
            return Some(Ty::Bool);
        }
        if shallow != Ty::Text {
            return None;
        }
        let t = self.expr(recv);
        if t != Ty::Text {
            return Some(Ty::Bool);
        }
        // Inside another comparison, `x is ""` needs the parentheses `x.length() is 0` may
        // not have had: `(s.length() is 0) is false`.
        let parens = self.tight.contains(&e.span);
        self.empty_fix(recv, negated, at, e.span, parens);
        Some(Ty::Bool)
    }

    /// One `MZ0962` at `at` whose fix replaces `whole` with `r is ""` (or `is not ""`).
    fn empty_fix(&mut self, recv: &Expr, negated: bool, at: Span, whole: Span, parens: bool) {
        let r = receiver_text(recv);
        let op = if negated { "is not" } else { "is" };
        let mut fixed = format!("{r} {op} \"\"");
        if parens {
            fixed = format!("({fixed})");
        }
        let say = format!(
            "{}: its emptiness is asked `{fixed}`, one form for one question (RFC-0013 §3.3)",
            super::EMPTINESS
        );
        if recv.has_error() || self.interp > 0 {
            // A string literal cannot stand inside `{…}` (§3.6): bind the test with `let`.
            self.err("MZ0962", at, say);
        } else {
            self.err_fix("MZ0962", at, say, whole, fixed, Confidence::Exact);
        }
    }

    /// Python's and JavaScript's free functions on text (RFC-0013 §3.7, §10): `len(s)`, with
    /// the `exact` fix `s.length()`, and the conversions `int(s)`, `float(s)`, `parseInt(s)`
    /// and `parseFloat(s)` (`parse_int`, `parse_float` after the lexer's snake_case), which
    /// become `s.parse_int()` and `s.parse_float()` as a `guess`: the conversions raise on
    /// text that is no number, and the methods answer `none`. The conversions are reported at
    /// the name, where the lexer's `MZ0101` for `parseInt` stands, so the two are one
    /// diagnostic (`lib.rs`), and they type as an error, so the answer is not reported again
    /// as an option used as its value. `None` when it is none of these, so the call is
    /// checked as any other.
    pub(super) fn free_text(
        &mut self,
        name: &str,
        name_span: Span,
        args: &[Expr],
        types: &[Ty],
        at: Span,
    ) -> Option<Ty> {
        if args.len() != 1 || types[0] != Ty::Text {
            return None;
        }
        let r = receiver_text(&args[0]);
        // The name as the author wrote it (`parseInt`), not the snake_case form the parser
        // reads (`parse_int`), which is what `name` is.
        let written = spelled(self.src, name_span).unwrap_or_else(|| name.to_string());
        let method = match name {
            "len" => {
                let fixed = format!("{r}.length()");
                let say = format!(
                    "`{written}(…)` is not a Mzizi function — an operation on a value is a method: `{fixed}`"
                );
                if args[0].has_error() {
                    self.err("MZ0962", at, say);
                } else {
                    self.err_fix("MZ0962", at, say, at, fixed, Confidence::Exact);
                }
                return Some(Ty::Int);
            }
            "int" | "parse_int" => "parse_int",
            "float" | "parse_float" => "parse_float",
            _ => return None,
        };
        let fixed = format!("{r}.{method}()");
        let say = format!(
            "`{written}(…)` is not a Mzizi function — text is read by a method that answers `none` when it is no number: `{fixed}`"
        );
        if args[0].has_error() {
            self.err("MZ0962", name_span, say);
        } else {
            self.err_fix("MZ0962", name_span, say, at, fixed, Confidence::Guess);
        }
        Some(Ty::Error)
    }

    /// `recv.name(args)` with `recv` a `text`, its arguments already typed (RFC-0013 §10).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn text_method(
        &mut self,
        e: &Expr,
        recv: &Expr,
        name: &str,
        name_span: Span,
        args: &[Expr],
        types: &[Ty],
        called: bool,
    ) -> Ty {
        let r = receiver_text(recv);
        // `s.is_empty()`, or `s.is_empty` with no parentheses: emptiness (§3.3).
        if name == "is_empty" && args.is_empty() {
            let parens = self.tight.contains(&e.span);
            self.empty_fix(recv, false, name_span, e.span, parens);
            return Ty::Bool;
        }
        let Some((params, ret)) = text::method(Ty::Text, name) else {
            return self.text_idiom(e, &r, name, name_span, args, called);
        };
        if self.method_shape(
            e,
            Ty::Text,
            &r,
            name,
            name_span,
            &params,
            args.len(),
            called,
        ) {
            return ret;
        }
        // `s.replace(a, b)`: its second argument is labelled (RFC-0013 §6.5, §10), and the
        // `exact` fix inserts the label. The parser saw it unlabelled; only here is the
        // receiver known to be text, so a number's `replace` is `MZ0708` alone.
        if let ExprKind::Method {
            unlabelled: true, ..
        } = e.kind
            && let (Some(label), [_, second]) = (text::label(name, 1), args)
            && !e.has_error()
        {
            let at = second.span;
            self.err_fix(
                "MZ0927",
                at,
                format!(
                    "`.{name}`'s second argument is labelled — write `{label} = {}`",
                    canonical(second)
                ),
                Span::single(at.start_line, at.start_col, 0),
                format!("{label} = "),
                Confidence::Exact,
            );
            return ret;
        }
        for ((a, t), p) in args.iter().zip(types).zip(&params) {
            if *t == Ty::Error || t == p {
                continue;
            }
            if self.unhandled(a, *t, || format!("`.{name}` takes the value, not a result")) {
                return ret;
            }
            self.err(
                "MZ0905",
                a.span,
                format!(
                    "`{}` is {}, and `.{name}` on text takes {}",
                    canonical(a),
                    t.name(),
                    p.name()
                ),
            );
            return ret;
        }
        // A constant negative count traps every time (RFC-0013 §4.3, `MZ0915`).
        if name == "repeat"
            && let Some(Ok(n)) = args.first().and_then(fold)
            && n < 0
        {
            self.err(
                "MZ0915",
                e.span,
                format!(
                    "`{}` repeats a negative number of times, which traps every time it runs — the count is 0 or more",
                    canonical(e)
                ),
            );
        }
        // `s.split("")`: an empty separator traps at run time (§10, §4.3), so a literal one
        // is `MZ0915`. The guess `s.chars()` is what the other languages' `split("")` means,
        // one text per character.
        if name == "split"
            && let [sep] = args
            && let ExprKind::Text(parts) = &sep.kind
            && parts
                .iter()
                .all(|p| matches!(p, TextPart::Lit(l) if l.is_empty()))
            && !e.has_error()
        {
            let fixed = format!("{r}.chars()");
            self.err_fix(
                "MZ0915",
                e.span,
                format!(
                    "`{}` splits on an empty separator, which traps every time it runs (§10); one text per character is `{fixed}`",
                    canonical(e)
                ),
                e.span,
                fixed,
                Confidence::Guess,
            );
        }
        ret
    }

    /// A method text does not have: another language's spelling of one it does (`MZ0962`),
    /// or `MZ0708` with the nearest name.
    fn text_idiom(
        &mut self,
        e: &Expr,
        r: &str,
        name: &str,
        name_span: Span,
        args: &[Expr],
        called: bool,
    ) -> Ty {
        let Some((to, preserves)) = text::idiom(name, args.len()) else {
            match nearest(name, text::designed()) {
                Some((near, _)) => self.err_fix(
                    "MZ0708",
                    name_span,
                    format!("text has no method `{name}` — did you mean `{near}`?"),
                    name_span,
                    near,
                    Confidence::Guess,
                ),
                None => self.err(
                    "MZ0708",
                    name_span,
                    format!(
                        "text has no method `{name}` — it has {}",
                        text::METHODS
                            .iter()
                            .map(|m| format!("`{m}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ),
            }
            return Ty::Error;
        };
        let sig = text::method(Ty::Text, to);
        // An answer that is an option or a list is typed as an error here: the one `MZ0962`
        // is the diagnostic, not that and an `MZ0710` or `MZ0912` at every use of it.
        let ret = match sig.as_ref().map_or(Ty::Error, |(_, ret)| *ret) {
            Ty::Option(_) | Ty::List(_) => Ty::Error,
            ret => ret,
        };
        // A built method called with another arity (Python's `strip(chars)`) has no fix:
        // renaming it would only move the error.
        let fits = sig
            .as_ref()
            .is_none_or(|(params, _)| !called || params.len() == args.len());
        let args_text: Vec<String> = args.iter().map(canonical).collect();
        // The fix: the name alone where only the name differs, else the whole call.
        let (span, fixed) = if !called {
            // `s.len`, `s.size`: the name and its parentheses.
            (name_span, format!("{to}()"))
        } else if to == "slice" {
            // `substr(a, n)` takes a length, not an end: no fix.
            match args_text.as_slice() {
                [a, b] if name == "substring" => (e.span, format!("{r}.slice({a}, to = {b})")),
                _ => (e.span, String::new()),
            }
        } else if let ([a, b], Some(label)) = (args_text.as_slice(), text::label(to, 1)) {
            (e.span, format!("{r}.{to}({a}, {label} = {b})"))
        } else {
            (name_span, to.to_string())
        };
        let written = spelled(self.src, name_span).unwrap_or_else(|| name.to_string());
        let say = format!("`.{written}` is another language's — Mzizi's text method is `.{to}`");
        if fixed.is_empty() || e.has_error() || !fits {
            self.err("MZ0962", name_span, say);
        } else {
            let c = if preserves {
                Confidence::Exact
            } else {
                Confidence::Guess
            };
            self.err_fix("MZ0962", name_span, say, span, fixed, c);
        }
        ret
    }
}
