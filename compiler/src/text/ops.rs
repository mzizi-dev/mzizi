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

/// `s[i]` (RFC-0013 §3.7, §10): the `i`th Unicode scalar value as a one-character text, or
/// `None` when `i` is negative or past the end. It walks the characters, so no byte index is
/// taken and no character is cut.
pub fn mz_text_index(s: &str, i: i64) -> Option<String> {
    let u = usize::try_from(i).ok()?;
    s.chars().nth(u).map(String::from)
}

/// `s.slice(a, to = b)` (RFC-0013 §10): the scalar values from `a` up to, not including,
/// `b`, or `None` when either end is negative or past the end, or `a` is past `b`. A slice
/// that starts and ends at the end of the text is empty, not `None`.
pub fn mz_text_slice(s: &str, a: i64, b: i64) -> Option<String> {
    let a = usize::try_from(a).ok()?;
    let b = usize::try_from(b).ok()?;
    let len = b.checked_sub(a)?;
    if b > s.chars().count() {
        return None;
    }
    Some(s.chars().skip(a).take(len).collect())
}

/// `s.find(t)` (RFC-0013 §10): the scalar-value index of the first occurrence of `t`, or
/// `None`. `find` gives a byte offset, which is always a character boundary; counting the
/// characters before it turns that into a scalar-value index. `""` is found at `0`.
pub fn mz_text_find(s: &str, t: &str) -> Option<i64> {
    let at = s.find(t)?;
    i64::try_from(s.char_indices().take_while(|&(i, _)| i < at).count()).ok()
}

/// `s.chars()` (RFC-0013 §10): one text per Unicode scalar value, in order.
pub fn mz_text_chars(s: &str) -> Vec<String> {
    s.chars().map(String::from).collect()
}

/// `s.split(sep)` (RFC-0013 §10), or the trap's reason: an empty `sep` (a literal one is
/// `MZ0915`, §10, and this is the same fault met at run time). Every piece is kept, empty
/// ones included, as Python's `split(sep)` keeps them.
pub fn mz_text_split(s: &str, sep: &str) -> Result<Vec<String>, &'static str> {
    if sep.is_empty() {
        return Err("empty separator");
    }
    Ok(s.split(sep).map(String::from).collect())
}

/// `s.parse_int()` (RFC-0013 §10, RFC-0011 §4.2): an optional `-`, then one or more ASCII
/// digits, and a value that fits an `int`. `"0x10"`, `"1e2"`, `" 7 "`, `"1.5"`, `"+1"`, `""`
/// and `"-"` are `None`. The same rule as a query parameter's (`serve::decode_query`), which
/// a unit test checks.
pub fn mz_text_parse_int(s: &str) -> Option<i64> {
    let digits = match s.strip_prefix('-') {
        Some(rest) => rest,
        None => s,
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i64>().ok()
}

/// `s.parse_float()` (RFC-0013 §10): an optional `-`, one or more ASCII digits, and an
/// optional `.` followed by one or more digits. No exponent, no leading `+`, no `inf` or
/// `nan`, so `"1."` and `".5"` are `None`. A number too large for a `float` is `None` too,
/// so a successful parse is always a finite `float`.
pub fn mz_text_parse_float(s: &str) -> Option<f64> {
    let body = match s.strip_prefix('-') {
        Some(rest) => rest,
        None => s,
    };
    let digits = |d: &str| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit());
    let (whole, fraction) = match body.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (body, None),
    };
    if !digits(whole) || fraction.is_some_and(|f| !digits(f)) {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}
