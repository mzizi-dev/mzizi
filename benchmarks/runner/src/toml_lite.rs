//! The small TOML reader `arm.toml` is read with (see `README.md`, "Arms: `arm.toml`").
//!
//! It reads top-level `key = value` pairs before the first `[table]` header and nothing
//! after it. A value is a basic string (`"…"`, with `\\`, `\"`, `\n`, `\t` escapes), a
//! literal string (`'…'`), `true` / `false`, or an array of strings, which may span lines
//! and end with a trailing comma. `#` starts a comment outside a string. Anything else is an
//! error, and so is a key given twice: the file is configuration for a measurement, so a
//! mis-read must stop the run rather than change it.

use std::collections::BTreeMap;

/// One top-level value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Bool(bool),
    Array(Vec<String>),
}

/// Parse the top-level keys of `src`, in key order.
pub fn parse(src: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut p = Parser {
        s: src.as_bytes(),
        i: 0,
        line: 1,
    };
    let mut out = BTreeMap::new();
    loop {
        p.skip_blank();
        let Some(c) = p.peek() else { break };
        if c == b'[' {
            break;
        }
        let key = p.key()?;
        p.skip_inline_ws();
        if p.peek() != Some(b'=') {
            return Err(p.err(&format!("expected `=` after `{key}`")));
        }
        p.i += 1;
        p.skip_inline_ws();
        let v = p.value()?;
        p.end_of_line()?;
        if out.insert(key.clone(), v).is_some() {
            return Err(p.err(&format!("`{key}` given twice")));
        }
    }
    Ok(out)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    line: usize,
}

impl Parser<'_> {
    fn err(&self, m: &str) -> String {
        format!("line {}: {m}", self.line)
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.i += 1;
        if c == b'\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn skip_inline_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.i += 1;
        }
    }

    fn skip_comment(&mut self) {
        if self.peek() == Some(b'#') {
            while !matches!(self.peek(), None | Some(b'\n')) {
                self.i += 1;
            }
        }
    }

    /// Whitespace, newlines and comments.
    fn skip_blank(&mut self) {
        loop {
            self.skip_inline_ws();
            self.skip_comment();
            match self.peek() {
                Some(b'\n' | b'\r') => {
                    self.bump();
                }
                _ => break,
            }
        }
    }

    fn end_of_line(&mut self) -> Result<(), String> {
        self.skip_inline_ws();
        self.skip_comment();
        match self.peek() {
            None | Some(b'\n') | Some(b'\r') => Ok(()),
            Some(c) => Err(self.err(&format!("unexpected `{}` after value", c as char))),
        }
    }

    fn key(&mut self) -> Result<String, String> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            self.i += 1;
        }
        if start == self.i {
            return Err(self.err("expected a bare key"));
        }
        Ok(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(b'"' | b'\'') => Ok(Value::Str(self.string()?)),
            Some(b'[') => {
                self.bump();
                let mut items = Vec::new();
                loop {
                    self.skip_blank();
                    match self.peek() {
                        Some(b']') => {
                            self.bump();
                            return Ok(Value::Array(items));
                        }
                        Some(b'"' | b'\'') => items.push(self.string()?),
                        _ => return Err(self.err("arrays hold strings only")),
                    }
                    self.skip_blank();
                    match self.peek() {
                        Some(b',') => {
                            self.bump();
                        }
                        Some(b']') => {}
                        _ => return Err(self.err("expected `,` or `]` in array")),
                    }
                }
            }
            _ => {
                let start = self.i;
                while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric()) {
                    self.i += 1;
                }
                match &self.s[start..self.i] {
                    b"true" => Ok(Value::Bool(true)),
                    b"false" => Ok(Value::Bool(false)),
                    other => Err(self.err(&format!(
                        "unsupported value `{}` (strings, booleans and string arrays only)",
                        String::from_utf8_lossy(other)
                    ))),
                }
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        let quote = self.bump().expect("caller checked a quote");
        let mut out = Vec::new();
        loop {
            match self.bump() {
                None | Some(b'\n') => return Err(self.err("unterminated string")),
                Some(c) if c == quote => break,
                Some(b'\\') if quote == b'"' => match self.bump() {
                    Some(b'\\') => out.push(b'\\'),
                    Some(b'"') => out.push(b'"'),
                    Some(b'n') => out.push(b'\n'),
                    Some(b't') => out.push(b'\t'),
                    _ => return Err(self.err("unsupported escape in string")),
                },
                Some(c) => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| self.err("string is not UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_value_kind_and_stops_at_the_first_table() {
        let src = r#"
# a comment
id = "dioxus"   # trailing
lit = 'a\b'
on = true
check = [
  "{repo}/x.sh", # one
  '{file}',
]
empty = []
[source]
id = "ignored"
not toml at all
"#;
        let m = parse(src).unwrap();
        assert_eq!(m["id"], Value::Str("dioxus".into()));
        assert_eq!(m["lit"], Value::Str("a\\b".into()));
        assert_eq!(m["on"], Value::Bool(true));
        assert_eq!(
            m["check"],
            Value::Array(vec!["{repo}/x.sh".into(), "{file}".into()])
        );
        assert_eq!(m["empty"], Value::Array(vec![]));
        assert_eq!(m.len(), 5);
    }

    #[test]
    fn a_hash_or_bracket_inside_a_string_is_text() {
        let m = parse("a = \"x # y ] z\"\nb = [\"]\", \"#\"]\n").unwrap();
        assert_eq!(m["a"], Value::Str("x # y ] z".into()));
        assert_eq!(m["b"], Value::Array(vec!["]".into(), "#".into()]));
    }

    #[test]
    fn escapes() {
        let m = parse(r#"a = "q\"b\\n\n""#).unwrap();
        assert_eq!(m["a"], Value::Str("q\"b\\n\n".into()));
    }

    #[test]
    fn rejects_what_it_cannot_read_exactly() {
        for bad in [
            "a = 1\n",
            "a = \"x\"\na = \"y\"\n",
            "a = \"unterminated\n",
            "a = [1, 2]\n",
            "a = \"x\" \"y\"\n",
            "a \"x\"\n",
            "a = [\"x\" \"y\"]\n",
            "a = \"\\q\"\n",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should not parse");
        }
    }
}
