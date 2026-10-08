// Text operations (RFC-0013 §10) that need more than one call to `std`. This file is
// compiled twice: as part of the compiler (`crate::text`), where its tests run, and
// verbatim inside every lowered program's `main.rs` (`crate::run`), so what the tests pin
// is what a program runs. It uses only `std`, and holds none of what §14.3 keeps out of
// the generated code.

/// `s.length()`: the number of Unicode scalar values, not bytes (RFC-0013 §10), so
/// `"héllo".length()` is `5`. A `String` holds at most `isize::MAX` bytes, so the count
/// always fits in an `i64` and the cast is exact.
pub fn mz_text_length(s: &str) -> i64 {
    s.chars().count() as i64
}

/// `s.repeat(n)`, or the trap's reason (RFC-0013 §4.3): a negative `n`, or a result longer
/// than a `String` can hold. Checking the length first keeps `str::repeat` from panicking
/// on a capacity overflow. A result that fits but cannot be allocated still stops the
/// process the way any allocation failure does; that is not a trap.
pub fn mz_text_repeat(s: &str, n: i64) -> Result<String, &'static str> {
    if n < 0 {
        return Err("negative repeat count");
    }
    // Above `usize::MAX` (a 32-bit target) only the empty text has a result that fits.
    let Ok(times) = usize::try_from(n) else {
        return if s.is_empty() {
            Ok(String::new())
        } else {
            Err("text too long")
        };
    };
    match s.len().checked_mul(times) {
        Some(total) if total <= isize::MAX as usize => Ok(s.repeat(times)),
        _ => Err("text too long"),
    }
}
