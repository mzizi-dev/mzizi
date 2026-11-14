//! Integer expressions inside a view string's `{…}` ([RFC-0017](../../design/RFC-0017-render.md)
//! §6, tracker U11).
//!
//! A view attribute string such as `"transform: translateX(-{100 - value}%)"` holds one
//! expression per `{…}`. The grammar is deliberately small and closed: `int` literals, a prop
//! or enum-column path, unary `-`, `+ - * / %` with the usual precedence, parentheses, and
//! `min(a, b)` and `max(a, b)`. Everything is `int`; there are no comparisons, no text
//! operators and no calls but those two. A body that is a single path is not an expression
//! here (it is the interpolation the view has always had), see [`parse`].
//!
//! The checker ([`crate::resolve`]) and the renderer ([`crate::render`]) both read the body
//! with [`parse`], so what checks is what renders.

/// A parsed integer expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// An integer literal.
    Int(i64),
    /// A prop, or an enum column read through a prop: `size.height`.
    Path(String),
    /// Unary minus.
    Neg(Box<Expr>),
    /// A binary operation.
    Bin(char, Box<Expr>, Box<Expr>),
    /// `min(a, b)` when `true`, `max(a, b)` when `false`.
    MinMax(bool, Box<Expr>, Box<Expr>),
}

/// Why a body did not parse, and the byte offset in the body where it went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset into the body.
    pub at: usize,
    /// What was wrong, for a diagnostic.
    pub say: String,
}

/// Why an expression could not be evaluated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    /// Division or remainder by zero.
    DivideByZero,
    /// A result outside `i64`.
    Overflow,
    /// A path that did not resolve to an integer; the string is the renderer's reason.
    Path(String),
}

impl Expr {
    /// Whether this is a bare path, which a view reads as a plain interpolation.
    pub fn is_path(&self) -> bool {
        matches!(self, Expr::Path(_))
    }

    /// Every path the expression reads, in source order.
    pub fn paths(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Expr::Int(_) => {}
            Expr::Path(p) => out.push(p),
            Expr::Neg(e) => e.collect(out),
            Expr::Bin(_, a, b) | Expr::MinMax(_, a, b) => {
                a.collect(out);
                b.collect(out);
            }
        }
    }

    /// Evaluate with `get` resolving each path to an `i64`. All arithmetic is checked.
    pub fn eval(&self, get: &dyn Fn(&str) -> Result<i64, String>) -> Result<i64, EvalError> {
        Ok(match self {
            Expr::Int(n) => *n,
            Expr::Path(p) => get(p).map_err(EvalError::Path)?,
            Expr::Neg(e) => e.eval(get)?.checked_neg().ok_or(EvalError::Overflow)?,
            Expr::MinMax(is_min, a, b) => {
                let (a, b) = (a.eval(get)?, b.eval(get)?);
                if *is_min { a.min(b) } else { a.max(b) }
            }
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(get)?, b.eval(get)?);
                match op {
                    '+' => a.checked_add(b),
                    '-' => a.checked_sub(b),
                    '*' => a.checked_mul(b),
                    '/' | '%' if b == 0 => return Err(EvalError::DivideByZero),
                    '/' => a.checked_div(b),
                    _ => a.checked_rem(b),
                }
                .ok_or(EvalError::Overflow)?
            }
        })
    }
}

struct P<'a> {
    s: &'a str,
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.s[self.i..].starts_with(' ') {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.ws();
        self.s[self.i..].chars().next()
    }

    fn err<T>(&self, say: &str) -> Result<T, ParseError> {
        Err(ParseError {
            at: self.i,
            say: say.to_string(),
        })
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.term()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.i += 1;
            let right = self.term()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        while let Some(op @ ('*' | '/' | '%')) = self.peek() {
            self.i += 1;
            let right = self.unary()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.peek() == Some('-') {
            self.i += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some('(') => {
                self.i += 1;
                let e = self.expr()?;
                if self.peek() != Some(')') {
                    return self.err("a `(` is never closed");
                }
                self.i += 1;
                Ok(e)
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.i;
                while self.s[self.i..].starts_with(|c: char| c.is_ascii_digit()) {
                    self.i += 1;
                }
                self.s[start..self.i]
                    .parse()
                    .map(Expr::Int)
                    .or_else(|_| self.err("that number does not fit an int"))
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let start = self.i;
                while self.s[self.i..]
                    .starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                {
                    self.i += 1;
                }
                let word = &self.s[start..self.i];
                if self.s[self.i..].starts_with('(') {
                    let is_min = match word {
                        "min" => true,
                        "max" => false,
                        _ => {
                            self.i = start;
                            return self
                                .err("the only calls in a view are `min(a, b)` and `max(a, b)`");
                        }
                    };
                    self.i += 1;
                    let a = self.expr()?;
                    if self.peek() != Some(',') {
                        return self.err("`min` and `max` take two values: `min(a, b)`");
                    }
                    self.i += 1;
                    let b = self.expr()?;
                    if self.peek() != Some(')') {
                        return self.err("a `(` is never closed");
                    }
                    self.i += 1;
                    return Ok(Expr::MinMax(is_min, Box::new(a), Box::new(b)));
                }
                if word.split('.').any(|seg| seg.is_empty()) {
                    self.i = start;
                    return self.err("a path is names joined by single dots");
                }
                Ok(Expr::Path(word.to_string()))
            }
            Some(_) => self.err("expected a number, a name, `-` or `(`"),
            None => self.err("the expression stops where a value was expected"),
        }
    }
}

/// Parse the inside of one `{…}`. A body that is a single path parses to [`Expr::Path`];
/// callers treat that as a plain interpolation.
pub fn parse(body: &str) -> Result<Expr, ParseError> {
    let mut p = P { s: body, i: 0 };
    let e = p.expr()?;
    if p.peek().is_some() {
        return p.err("unexpected text after the expression: operators are `+ - * / %`");
    }
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(body: &str, v: i64) -> Result<i64, EvalError> {
        parse(body).unwrap().eval(&|_| Ok(v))
    }

    #[test]
    fn precedence_and_unary() {
        assert_eq!(run("1 + 2 * 3", 0), Ok(7));
        assert_eq!(run("(1 + 2) * 3", 0), Ok(9));
        assert_eq!(run("-2 * -3", 0), Ok(6));
        assert_eq!(run("100 - value", 40), Ok(60));
        assert_eq!(run("7 % 4 - 10 / 3", 0), Ok(0));
    }

    #[test]
    fn min_and_max_clamp() {
        assert_eq!(run("100 - min(max(value, 0), 100)", 150), Ok(0));
        assert_eq!(run("100 - min(max(value, 0), 100)", -5), Ok(100));
    }

    #[test]
    fn a_path_alone_is_a_path() {
        assert!(parse("state.label").unwrap().is_path());
        assert!(!parse("a + 1").unwrap().is_path());
    }

    #[test]
    fn errors_are_checked_not_wrapped() {
        assert_eq!(run("1 / (value - value)", 3), Err(EvalError::DivideByZero));
        assert_eq!(run("value * value", i64::MAX), Err(EvalError::Overflow));
    }

    #[test]
    fn malformed_bodies_are_refused() {
        for bad in [
            "",
            "a +",
            "(a",
            "a b",
            "foo(a)",
            "min(a)",
            "a..b",
            "a + * b",
            "9999999999999999999999",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should not parse");
        }
    }
}
