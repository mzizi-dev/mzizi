// A `float`'s text form (RFC-0013 §3.8). This file is compiled twice: as part of the
// compiler (`crate::numbers`), where its tests run, and verbatim inside every lowered
// program's `main.rs` (`crate::run`), so the checker's canonical text and the program's
// output share one implementation. It uses only `std`, and holds none of what §14.3 keeps
// out of the generated code.

/// RFC-0013 §3.8's text form of a `float`: `1.0`, `0.1`, `1.0e21`, `1.5e-7`, `inf`,
/// `-inf`, `nan`, `-0.0`. Plain decimal when `1e-6 <= |x| < 1e21`, otherwise one digit, a
/// point, the remaining digits (or `0`), `e` and the exponent.
pub fn mz_float_text(x: f64) -> String {
    mz_float_layout(x, false)
}

/// The shortest decimal digits that read back as `x`, laid out as §3.8 says, or always in
/// plain decimal when `plain` is set (a literal's canonical text, §17, which has no
/// exponent form).
pub fn mz_float_layout(x: f64, plain: bool) -> String {
    if x.is_nan() {
        return "nan".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x == 0.0 {
        return format!("{sign}0.0");
    }
    // Rust's `{:e}` with no precision is the shortest digit string that reads back as the
    // same `f64`, with its decimal exponent: `1.5e-7`, `1e21`, `1.2345e2`.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = match sci.split_once('e') {
        Some((m, e)) => match e.parse::<i64>() {
            Ok(e) => (m, e),
            Err(_) => (m, 0),
        },
        None => (sci.as_str(), 0),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let a = x.abs();
    if plain || (1e-6..1e21).contains(&a) {
        // The digits are d0.d1d2… × 10^exp, so the point falls after `exp + 1` of them.
        let point = exp + 1;
        let len = digits.chars().count() as i64;
        if point <= 0 {
            let zeros = "0".repeat((-point) as usize);
            format!("{sign}0.{zeros}{digits}")
        } else if point >= len {
            let zeros = "0".repeat((point - len) as usize);
            format!("{sign}{digits}{zeros}.0")
        } else {
            let (whole, frac) = digits.split_at(point as usize);
            format!("{sign}{whole}.{frac}")
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        format!("{sign}{first}.{rest}e{exp}")
    }
}
