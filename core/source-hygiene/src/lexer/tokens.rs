//! A file as a stream of tokens, for the rules that read an EXPRESSION rather than a line.
//!
//! The line readers beside this (`split_code_and_comment`, `brace_delta`) answer "what is on
//! this line"; a call chain is not on one line. `Entity::find()` on one line, `.filter(..)` on
//! the next, `.all(db)` three lines down is one read, and a rule that asks whether it carries a
//! `.limit(` must see all of it. Regex over lines cannot, so this tokenizes the whole text once:
//! identifiers, punctuation and literals with their line, comments set aside with theirs (a rule
//! reads a justification comment by line, never as code). Strings, raw strings, char literals,
//! lifetimes and nested block comments are skipped as the compiler skips them, so a `.all(` inside
//! a string or a comment is never a call.

/// What a token is. Literals keep no text a rule needs, only their place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ident,
    Punct,
    Literal,
}

/// One token of code, with the 1-indexed line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'a> {
    pub kind: Kind,
    pub text: &'a str,
    pub line: usize,
}

impl Token<'_> {
    /// Is this the punctuation character `c`?
    pub fn is(&self, c: char) -> bool {
        self.kind == Kind::Punct && self.text.len() == c.len_utf8() && self.text.starts_with(c)
    }

    /// Is this the identifier `name`?
    pub fn is_ident(&self, name: &str) -> bool {
        self.kind == Kind::Ident && self.text == name
    }
}

/// A `//` comment: its text after the slashes, and its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment<'a> {
    pub text: &'a str,
    pub line: usize,
}

/// A file's code tokens and its line comments, both in source order.
pub struct Tokens<'a> {
    pub code: Vec<Token<'a>>,
    pub comments: Vec<Comment<'a>>,
}

/// Tokenize `src`. Unterminated literals and comments run to the end of the text, as the
/// compiler would refuse them anyway; the scan never panics on a file it cannot compile.
pub fn tokenize(src: &str) -> Tokens<'_> {
    let bytes = src.as_bytes();
    let mut out = Tokens { code: Vec::new(), comments: Vec::new() };
    let mut line = 1;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        let start_line = line;
        if c == b'\n' {
            line += 1;
            i += 1;
        } else if c.is_ascii_whitespace() {
            i += 1;
        } else if bytes[i..].starts_with(b"//") {
            let end = bytes[i..].iter().position(|&b| b == b'\n').map_or(bytes.len(), |n| i + n); // map_or: a comment on the last line runs to the end
            out.comments.push(Comment { text: &src[i + 2..end], line: start_line });
            i = end;
        } else if bytes[i..].starts_with(b"/*") {
            i = block_comment_end(bytes, i + 2);
            line += count_newlines(&bytes[start..i]);
        } else if let Some(len) = raw_or_byte_string_len(bytes, i) {
            i += len;
            line += count_newlines(&bytes[start..i]);
            out.code.push(Token { kind: Kind::Literal, text: &src[start..i], line: start_line });
        } else if c == b'"' {
            i = quoted_end(bytes, i + 1, b'"');
            line += count_newlines(&bytes[start..i]);
            out.code.push(Token { kind: Kind::Literal, text: &src[start..i], line: start_line });
        } else if c == b'\'' {
            match char_literal_end(bytes, i) {
                Some(end) => {
                    i = end;
                    out.code.push(Token { kind: Kind::Literal, text: &src[start..i], line: start_line });
                }
                // A lifetime: the quote is punctuation and the name an identifier after it.
                None => {
                    i += 1;
                    out.code.push(Token { kind: Kind::Punct, text: &src[start..i], line: start_line });
                }
            }
        } else if c.is_ascii_digit() {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.code.push(Token { kind: Kind::Literal, text: &src[start..i], line: start_line });
        } else if is_ident_byte(c) {
            if bytes[i..].starts_with(b"r#") {
                i += 2; // a raw identifier, `r#type`
            }
            while i < bytes.len() && is_ident_byte(bytes[i]) {
                i += 1;
            }
            out.code.push(Token { kind: Kind::Ident, text: &src[start..i], line: start_line });
        } else {
            let len = src[i..].chars().next().map_or(1, char::len_utf8); // map_or: `i` is always on a char boundary inside `src`
            i += len;
            out.code.push(Token { kind: Kind::Punct, text: &src[start..i], line: start_line });
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

fn count_newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == b'\n').count()
}

/// The offset after a block comment whose body starts at `i`; block comments nest.
fn block_comment_end(bytes: &[u8], mut i: usize) -> usize {
    let mut depth = 1;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"/*") {
            depth += 1;
            i += 2;
        } else if bytes[i..].starts_with(b"*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    bytes.len()
}

/// The offset after the closing `close` of an escaped literal whose body starts at `i`.
fn quoted_end(bytes: &[u8], mut i: usize, close: u8) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == close => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// `r"…"`, `r#"…"#`, `b"…"`, `br#"…"#`, `b'x'` starting at `i`, as a length. The prefix must
/// start a token, so an identifier ending in `b` or `r` is never read as one.
fn raw_or_byte_string_len(bytes: &[u8], i: usize) -> Option<usize> {
    if i > 0 && is_ident_byte(bytes[i - 1]) {
        return None;
    }
    let mut at = i;
    let byte = bytes.get(at) == Some(&b'b');
    if byte {
        at += 1;
        match bytes.get(at) {
            Some(b'"') => return Some(quoted_end(bytes, at + 1, b'"') - i),
            Some(b'\'') => return Some(quoted_end(bytes, at + 1, b'\'') - i),
            _ => {}
        }
    }
    if bytes.get(at) != Some(&b'r') {
        return None;
    }
    at += 1;
    let hashes = bytes[at..].iter().take_while(|&&b| b == b'#').count();
    at += hashes;
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    at += 1;
    while at < bytes.len() {
        if bytes[at] == b'"' && bytes[at + 1..].iter().take(hashes).filter(|&&b| b == b'#').count() == hashes {
            return Some(at + 1 + hashes - i);
        }
        at += 1;
    }
    Some(bytes.len() - i)
}

/// The offset after a char literal starting at `i` (`'x'`, `'\n'`, `'\u{1F600}'`, `'é'`), or
/// `None` for a lifetime (`'a`), which never closes with a quote right after its first char.
fn char_literal_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i + 1) == Some(&b'\\') {
        let close = bytes[i + 2..].iter().take(12).position(|&b| b == b'\'')?;
        return Some(i + 2 + close + 1);
    }
    let width = match bytes.get(i + 1)? {
        b if *b < 0x80 => 1,
        b if *b >= 0xF0 => 4,
        b if *b >= 0xE0 => 3,
        _ => 2,
    };
    (bytes.get(i + 1 + width) == Some(&b'\'')).then_some(i + 2 + width)
}

/// The index of the token that closes the group opened at `open` (`(`, `[` or `{`), or the
/// last token when it never closes.
pub fn matching_close(code: &[Token], open: usize) -> usize {
    let (o, c) = match code[open].text {
        "(" => ('(', ')'),
        "[" => ('[', ']'),
        _ => ('{', '}'),
    };
    let mut depth = 0usize;
    for (at, token) in code.iter().enumerate().skip(open) {
        if token.is(o) {
            depth += 1;
        } else if token.is(c) {
            depth -= 1;
            if depth == 0 {
                return at;
            }
        }
    }
    code.len().saturating_sub(1)
}

/// The index of the token that opens the group closed at `close`, or `None` when nothing does.
pub fn matching_open(code: &[Token], close: usize) -> Option<usize> {
    let (o, c) = match code[close].text {
        ")" => ('(', ')'),
        "]" => ('[', ']'),
        ">" => ('<', '>'),
        _ => ('{', '}'),
    };
    let mut depth = 0usize;
    for at in (0..=close).rev() {
        if code[at].is(c) {
            depth += 1;
        } else if code[at].is(o) {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str) -> Vec<&str> {
        tokenize(src).code.iter().map(|t| t.text).collect()
    }

    // what this catches: a call inside a literal or a comment read as code, and a chain's lines
    // misnumbered after a multi-line string (a violation that points at the wrong editor line).
    #[test]
    fn literals_and_comments_are_never_code_and_lines_stay_true() {
        let src = "let s = \".all(db)\";\n/* .all(x)\n */ let r = r#\"a \" .all(\"#;\nq // .limit(\n  .all(db)";
        let tokens = tokenize(src);
        let all = tokens.code.iter().filter(|t| t.is_ident("all")).collect::<Vec<_>>();
        assert_eq!(all.len(), 1, "only the real call");
        assert_eq!(all[0].line, 5);
        assert_eq!(tokens.comments, vec![Comment { text: " .limit(", line: 4 }]);
    }

    // what this catches: a lifetime opening a char literal that swallows the rest of the line,
    // and a char literal holding a quote or brace breaking the stream.
    #[test]
    fn lifetimes_and_char_literals_are_told_apart() {
        assert_eq!(texts("fn f<'a>(x: &'a str) { '\"' }"), ["fn", "f", "<", "'", "a", ">", "(", "x", ":", "&", "'", "a", "str", ")", "{", "'\"'", "}"]);
        assert_eq!(texts("b'{' '\\n' 'é'"), ["b'{'", "'\\n'", "'é'"]);
    }

    // what this catches: group matching that ignores nesting, which would end a chain early.
    #[test]
    fn groups_match_across_nesting() {
        let tokens = tokenize("f(a(b), [c]).all(d)");
        assert_eq!(matching_close(&tokens.code, 1), 10);
        assert_eq!(matching_open(&tokens.code, 10), Some(1));
    }
}
