//! Numbers in a program (RFC-0013 §4): the numeric methods of §4.4 and their types, the
//! constant folding of the `int` ones, and a `float`'s text form (§3.8).
//!
//! The text form lives in `numbers/float_text.rs`, which is also emitted verbatim into
//! every lowered program, so what `mz` prints in a diagnostic and what the program prints
//! come from one implementation.

use crate::expr::{Fault, Ty};

mod float_text;

pub use float_text::{mz_float_layout, mz_float_text};

/// The source of `float_text.rs`, for the lowering to emit whole (RFC-0013 §14.2).
pub const FLOAT_TEXT_RUNTIME: &str = include_str!("numbers/float_text.rs");

/// Every numeric method (RFC-0013 §4.4), for the nearest-name fix.
pub const METHODS: &[&str] = &[
    "abs", "ceil", "floor", "is_nan", "max", "min", "pow", "round", "sqrt", "to_float", "to_int",
];

/// A numeric method's signature on a receiver of type `recv`: its parameters' types and
/// what it returns, or `None` when `recv` has no method `name`.
pub fn method(recv: Ty, name: &str) -> Option<(Vec<Ty>, Ty)> {
    match (recv, name) {
        (Ty::Int, "to_float") => Some((vec![], Ty::Float)),
        (Ty::Float, "to_int") => Some((vec![], Ty::Int)),
        (Ty::Float, "round" | "floor" | "ceil" | "sqrt") => Some((vec![], Ty::Float)),
        (Ty::Float, "is_nan") => Some((vec![], Ty::Bool)),
        (Ty::Int | Ty::Float, "abs") => Some((vec![], recv)),
        (Ty::Int | Ty::Float, "min" | "max") => Some((vec![recv], recv)),
        (Ty::Int | Ty::Float, "pow") => Some((vec![Ty::Int], recv)),
        _ => None,
    }
}

/// The methods a numeric type has, in alphabetical order.
pub fn methods_of(recv: Ty) -> Vec<&'static str> {
    METHODS
        .iter()
        .copied()
        .filter(|m| method(recv, m).is_some())
        .collect()
}

/// `x.pow(n)` on `int` with RFC-0013 §4.1's rule: a negative `n` and overflow are faults.
/// The lowering's `mz_pow` computes the same thing (§14.2): `checked_pow` takes a `u32`,
/// and above `u32::MAX` only `0`, `1` and `-1` have an answer that fits.
pub fn int_pow(x: i64, n: i64) -> Result<i64, Fault> {
    if n < 0 {
        return Err(Fault::NegativeExponent);
    }
    match u32::try_from(n) {
        Ok(n) => x.checked_pow(n).ok_or(Fault::Overflow),
        Err(_) => match x {
            0 | 1 => Ok(x),
            -1 => Ok(if n % 2 == 0 { 1 } else { -1 }),
            _ => Err(Fault::Overflow),
        },
    }
}

/// A numeric method on `int`s known at check time: `Some` for the methods that take and
/// return `int`, so `2.pow(64)` and `x.pow(-1)` are faults the checker sees (`MZ0915`).
pub fn fold_int_method(name: &str, x: i64, args: &[i64]) -> Option<Result<i64, Fault>> {
    match (name, args) {
        ("abs", []) => Some(x.checked_abs().ok_or(Fault::Overflow)),
        ("min", [y]) => Some(Ok(x.min(*y))),
        ("max", [y]) => Some(Ok(x.max(*y))),
        ("pow", [n]) => Some(int_pow(x, *n)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_float_text_form_follows_rfc_0013_section_3_8() {
        let cases: &[(f64, &str)] = &[
            (1.0, "1.0"),
            (0.1, "0.1"),
            (0.1 + 0.2, "0.30000000000000004"),
            (123456.789, "123456.789"),
            (0.000001, "0.000001"),
            (1e-7, "1.0e-7"),
            (1.5e-7, "1.5e-7"),
            (1e20, "100000000000000000000.0"),
            (1e21, "1.0e21"),
            (-2.5, "-2.5"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (f64::NAN, "nan"),
            (5e-324, "5.0e-324"),
            (f64::MAX, "1.7976931348623157e308"),
            (9007199254740993.0, "9007199254740992.0"),
        ];
        for (x, want) in cases {
            assert_eq!(mz_float_text(*x), *want, "{x:e}");
        }
        // A literal's canonical text never has an exponent, and reads back exactly.
        assert_eq!(mz_float_layout(1e21, true), "1000000000000000000000.0");
        assert_eq!(mz_float_layout(1.5e-7, true), "0.00000015");
        for x in [0.1, 1e-7, 123.456, 1e300, 5e-324] {
            assert_eq!(mz_float_layout(x, true).parse::<f64>(), Ok(x));
        }
    }

    #[test]
    fn int_pow_traps_on_a_negative_exponent_and_overflow() {
        assert_eq!(int_pow(2, 10), Ok(1024));
        assert_eq!(int_pow(0, 0), Ok(1));
        assert_eq!(int_pow(2, -1), Err(Fault::NegativeExponent));
        assert_eq!(int_pow(2, 63), Err(Fault::Overflow));
        assert_eq!(int_pow(-2, 63), Ok(i64::MIN));
        assert_eq!(int_pow(-1, 5_000_000_001), Ok(-1));
        assert_eq!(int_pow(1, i64::MAX), Ok(1));
        assert_eq!(int_pow(3, 5_000_000_000), Err(Fault::Overflow));
        assert_eq!(
            fold_int_method("abs", i64::MIN, &[]),
            Some(Err(Fault::Overflow))
        );
    }

    #[test]
    fn the_method_table_is_rfc_0013_section_4_4() {
        assert_eq!(method(Ty::Int, "to_float"), Some((vec![], Ty::Float)));
        assert_eq!(method(Ty::Int, "sqrt"), None);
        assert_eq!(method(Ty::Float, "pow"), Some((vec![Ty::Int], Ty::Float)));
        assert_eq!(
            methods_of(Ty::Int),
            ["abs", "max", "min", "pow", "to_float"]
        );
    }
}
