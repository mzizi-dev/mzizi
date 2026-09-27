//! Extract the one fenced code block from a model reply.
//!
//! The prompt asks for exactly one block. The rule applied is the same for both arms:
//! prose around the block is tolerated, but zero blocks, more than one block, or a block
//! that never closes (typically a reply cut off at `max_tokens`) is a failed iteration and
//! is fed back as an error. Picking "the first" or "the longest" of several blocks would be
//! a silent judgement call on the model's behalf, so it is not made.
//!
//! Fences follow CommonMark: a line indented at most three spaces opening with three or
//! more backticks or tildes; a backtick fence's info string may not contain a backtick;
//! the closing fence uses the same character, is at least as long, and carries no info
//! string.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractError {
    NoBlock,
    MultipleBlocks(usize),
    Unterminated,
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractError::NoBlock => write!(f, "Your reply contained no fenced code block"),
            ExtractError::MultipleBlocks(n) => write!(
                f,
                "Your reply contained {n} fenced code blocks; exactly one is required"
            ),
            ExtractError::Unterminated => {
                write!(
                    f,
                    "Your reply contained a fenced code block that was never closed"
                )
            }
        }
    }
}

struct Open {
    ch: char,
    len: usize,
    indent: usize,
    lines: Vec<String>,
}

/// Returns the block's content (without fences), newline-terminated.
pub fn extract_code_block(reply: &str) -> Result<String, ExtractError> {
    let mut blocks: Vec<String> = Vec::new();
    let mut open: Option<Open> = None;

    for raw in reply.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        match open.as_mut() {
            None => {
                if let Some((ch, len, indent)) = opening_fence(line) {
                    open = Some(Open {
                        ch,
                        len,
                        indent,
                        lines: Vec::new(),
                    });
                }
            }
            Some(o) => {
                if is_closing_fence(line, o.ch, o.len) {
                    let mut body = o.lines.join("\n");
                    if !body.is_empty() {
                        body.push('\n');
                    }
                    blocks.push(body);
                    open = None;
                } else {
                    o.lines.push(strip_indent(line, o.indent).to_string());
                }
            }
        }
    }

    if open.is_some() {
        return Err(ExtractError::Unterminated);
    }
    match blocks.len() {
        0 => Err(ExtractError::NoBlock),
        1 => Ok(blocks.pop().unwrap_or_default()),
        n => Err(ExtractError::MultipleBlocks(n)),
    }
}

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|&c| c == ' ').count()
}

fn opening_fence(line: &str) -> Option<(char, usize, usize)> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ch = rest.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = rest.chars().take_while(|&c| c == ch).count();
    if len < 3 {
        return None;
    }
    let info = &rest[len..];
    if ch == '`' && info.contains('`') {
        return None;
    }
    Some((ch, len, indent))
}

fn is_closing_fence(line: &str, ch: char, min_len: usize) -> bool {
    let indent = leading_spaces(line);
    if indent > 3 {
        return false;
    }
    let rest = &line[indent..];
    let len = rest.chars().take_while(|&c| c == ch).count();
    len >= min_len && rest[len..].trim().is_empty()
}

fn strip_indent(line: &str, indent: usize) -> &str {
    let n = leading_spaces(line).min(indent);
    &line[n..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_block_with_language() {
        let r = "```rust\nfn main() {}\n```";
        assert_eq!(extract_code_block(r).unwrap(), "fn main() {}\n");
    }

    #[test]
    fn prose_before_and_after_is_tolerated() {
        let r = "Here is the port:\n\n```mz\ncomponent button\nend\n```\n\nHope this helps.";
        assert_eq!(extract_code_block(r).unwrap(), "component button\nend\n");
    }

    #[test]
    fn no_language_tag_and_crlf() {
        let r = "```\r\nline one\r\nline two\r\n```\r\n";
        assert_eq!(extract_code_block(r).unwrap(), "line one\nline two\n");
    }

    #[test]
    fn tilde_fence_and_longer_backtick_fence_containing_backticks() {
        assert_eq!(extract_code_block("~~~\na\n~~~").unwrap(), "a\n");
        let r = "````md\n```inner```\n```\nstill inside\n````";
        assert_eq!(
            extract_code_block(r).unwrap(),
            "```inner```\n```\nstill inside\n"
        );
    }

    #[test]
    fn info_string_line_inside_block_is_content_not_a_close() {
        let r = "```rust\nlet s = r#\"\n```rust\n\"#;\n```";
        assert_eq!(
            extract_code_block(r).unwrap(),
            "let s = r#\"\n```rust\n\"#;\n"
        );
    }

    #[test]
    fn indented_fence_strips_its_indent() {
        let r = "  ```\n  a\n    b\n  ```";
        assert_eq!(extract_code_block(r).unwrap(), "a\n  b\n");
    }

    #[test]
    fn missing_block() {
        assert_eq!(
            extract_code_block("I cannot do that."),
            Err(ExtractError::NoBlock)
        );
        assert_eq!(extract_code_block(""), Err(ExtractError::NoBlock));
        assert_eq!(
            extract_code_block("inline ``` is not a fence"),
            Err(ExtractError::NoBlock)
        );
    }

    #[test]
    fn two_blocks_is_an_error_not_a_guess() {
        let r = "```rust\nfn a() {}\n```\nand also\n```rust\nfn b() {}\n```";
        assert_eq!(extract_code_block(r), Err(ExtractError::MultipleBlocks(2)));
    }

    #[test]
    fn truncated_reply_is_unterminated() {
        let r = "```rust\nfn main() {\n    let x =";
        assert_eq!(extract_code_block(r), Err(ExtractError::Unterminated));
        let r2 = "```\nok\n```\n```\ncut off";
        assert_eq!(extract_code_block(r2), Err(ExtractError::Unterminated));
    }

    #[test]
    fn empty_block_is_an_empty_candidate() {
        assert_eq!(extract_code_block("```\n```").unwrap(), "");
    }
}
