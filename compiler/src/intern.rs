//! Interned types and type pairs, so that [`crate::expr::Ty`] stays `Copy` while it holds
//! a `result`'s or a `map`'s two types (RFC-0013 §12, §9), or a list's, an option's or a
//! set's element type. An enum's name is interned by
//! [`crate::expr::intern`].
//!
//! Each distinct pair is allocated once and kept for the life of the process: the memory is
//! bounded by the number of distinct result types in the files checked, which is small, and
//! in exchange every `Ty` is a plain value that compares, copies and prints without a table
//! at hand.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::expr::Ty;

static PAIRS: Mutex<Vec<&'static (Ty, Ty)>> = Mutex::new(Vec::new());
static ONES: Mutex<Vec<&'static Ty>> = Mutex::new(Vec::new());

/// A lock that a panic elsewhere cannot poison for good: the list only ever grows, so what
/// it holds is valid whatever a panicking holder was doing.
fn lock<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `(a, b)`, as a pair that lives as long as the process. Equal pairs share one allocation.
pub fn pair(a: Ty, b: Ty) -> &'static (Ty, Ty) {
    let mut pairs = lock(&PAIRS);
    if let Some(&have) = pairs.iter().find(|p| ***p == (a, b)) {
        return have;
    }
    let leaked: &'static (Ty, Ty) = Box::leak(Box::new((a, b)));
    pairs.push(leaked);
    leaked
}

/// `t`, as a value that lives as long as the process: a list's, an option's or a set's
/// element type. Equal types share one allocation.
pub fn one(t: Ty) -> &'static Ty {
    let mut ones = lock(&ONES);
    if let Some(&have) = ones.iter().find(|p| ***p == t) {
        return have;
    }
    let leaked: &'static Ty = Box::leak(Box::new(t));
    ones.push(leaked);
    leaked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::intern;

    #[test]
    fn equal_values_are_interned_once() {
        assert!(std::ptr::eq(
            intern("parse_problem"),
            intern("parse_problem")
        ));
        let a = pair(Ty::Int, Ty::Enum(intern("e")));
        let b = pair(Ty::Int, Ty::Enum(intern("e")));
        assert!(std::ptr::eq(a, b));
        assert_eq!(
            Ty::result(Ty::Nothing, Ty::Text).name(),
            "result(none, text)"
        );
    }
}
