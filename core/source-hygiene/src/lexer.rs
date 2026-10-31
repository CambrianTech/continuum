//! The lexer every rule shares: which lines of a file are production code, where a line's
//! code ends and its comment begins, and how a line moves brace depth. One owner, so two
//! rules never disagree about the same file.

/// The PRODUCTION half of a source file (see [`crate::SourceFile::production`]): every line
/// not gated by a `#[cfg(test)]` that is CODE, not prose. Matching the raw string
/// would truncate at the first doc comment that merely MENTIONS test-gating, and module
/// headers in this crate do exactly that, including this one. Found by the guard's own
/// positive control: the naive form excluded nearly all of `source_hygiene/mod.rs` from
/// its own scan, silently. Match by nature, never by name.
///
/// A file that IS a test module, moved out of its parent to shrink it, says so with the
/// inner `#![cfg(test)]` on its first code line: the whole file is test code. Without
/// this, every `mod tests;` split read as all-production and charged its unwraps and
/// spawns to the ratchets (card 0af5d5ec, 2026-10-18).
///
/// A COLUMN-ZERO `#[cfg(test)]` gates one file-scope item: that item is test code and
/// nothing else is. An indented `#[cfg(test)]` (a test-only helper inside an `impl`) is
/// scanned as production, erring toward noise: cut there, `cognition/llm_deliberation_faculty.rs`
/// hid ~9,000 production lines from every ratchet (found 2026-10-20).
///
/// Only the gated ITEM is test code, never the rest of the file. The first cut stopped
/// at the first column-zero `#[cfg(test)]`, which is the end of production only when the
/// test mod is the file's last item. A split that declares `#[cfg(test)] mod tests;` with
/// the other modules near the top (llamacpp_adapter.rs, line 93 of 934), or a gated test
/// const above the code (generate_response.rs, line 75 of 644), hid everything below it:
/// a split read as 10 unwraps removed when none were (found on #5225, 2026-10-20).
/// Lines of a gated item are blanked, not dropped, so line numbers stay true.
/// The attributes that gate an item OUT of a production build: unit tests, and the fixtures a
/// crate exports to its dependents' tests (`test-fixtures` is never enabled in a release
/// build). Code under either is test code to these scans; the kernel cut moved its fixtures
/// from the first spelling to the second so core's tests can reach them across the crate.
const TEST_GATES: &[&str] = &["#[cfg(test)]", "#[cfg(any(test, feature = \"test-fixtures\"))]"];

pub fn production_half(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut gated = false;
    let mut i = 0;
    while i < lines.len() {
        let code = split_code_and_comment(lines[i]).0;
        if code.trim_start().starts_with("#![cfg(test)]") {
            gated = true;
            break; // the whole file from here is a test module moved out of its owner
        }
        let Some(gate) = TEST_GATES.iter().find(|gate| code.starts_with(**gate)) else {
            out.push(lines[i]);
            i += 1;
            continue;
        };
        // The gated item can begin on the attribute's own line. Keep the slice's
        // line count intact so blanking still preserves source line numbers.
        let mut item_lines = lines[i..].to_vec();
        item_lines[0] = &lines[i][gate.len()..];
        let end = i + gated_item_end(&item_lines, 0);
        out.extend(std::iter::repeat_n("", end + 1 - i));
        gated = true;
        i = end + 1;
    }
    if !gated {
        return src.to_string();
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Byte offset after a block comment's closing delimiter, given its body (after `/*`).
/// Rust block comments nest; punctuation inside every nesting level is trivia.
fn block_comment_end(bytes: &[u8]) -> Option<usize> {
    let (mut depth, mut at) = (1usize, 0usize);
    while at < bytes.len() {
        if bytes[at..].starts_with(b"/*") {
            depth += 1;
            at += 2;
        } else if bytes[at..].starts_with(b"*/") {
            depth -= 1;
            at += 2;
            if depth == 0 { return Some(at); }
        } else {
            at += 1;
        }
    }
    None
}

/// Skip Rust whitespace and comments. Comments are trivia even
/// between `const` and `fn`; nested block comments are consumed as one comment.
fn skip_item_trivia(mut src: &str) -> &str {
    loop {
        src = src.trim_start();
        if let Some(comment) = src.strip_prefix("//") {
            match comment.find('\n') {
                Some(end) => src = &comment[end + 1..],
                None => return "",
            }
        } else if let Some(comment) = src.strip_prefix("/*") {
            let Some(end) = block_comment_end(comment.as_bytes()) else { return "" };
            src = &comment[end..];
        } else {
            break;
        }
    }
    src
}

fn significant_word(src: &str) -> (Option<&str>, &str) {
    let src = skip_item_trivia(src);
    let end = src.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
    if end == 0 { (None, src) } else { (Some(&src[..end]), &src[end..]) }
}

/// Index of the last line of the item starting at or after `from` (attributes, blank
/// lines and comments before it belong to it). The item ends on the line where its
/// braces close, or on its `;` at depth zero when it opens none (`mod tests;`, a
/// multi-line `static ... = ...;`). Read as one character stream, not line by line: a
/// test module's multi-line string fixtures (JSON, source snippets, raw strings) carry
/// braces that a per-line count read as the module's end, leaking the rest of the tests
/// into production. An item that never closes runs to the end of the file.
pub(crate) fn gated_item_end(lines: &[&str], from: usize) -> usize {
    let last = lines.len().saturating_sub(1);
    let Some(head) = (from..lines.len()).find(|&k| {
        let code = split_code_and_comment(lines[k]).0.trim();
        !code.is_empty() && !code.starts_with("#[")
    }) else {
        return last;
    };
    let text = lines[head..].join("\n");
    // Initializer blocks can be followed by operators, method calls, or more blocks.
    // A static/const ends at its outer semicolon, never at an initializer's `}`.
    let (mut keyword, mut rest) = significant_word(&text);
    if keyword == Some("pub") {
        rest = skip_item_trivia(rest);
        if let Some(restricted) = rest.strip_prefix('(') {
            rest = restricted;
            loop {
                rest = skip_item_trivia(rest);
                match rest.chars().next() {
                    Some(')') => {
                        rest = &rest[1..];
                        break;
                    }
                    Some(ch) => rest = &rest[ch.len_utf8()..],
                    None => return last,
                }
            }
        }
        (keyword, rest) = significant_word(rest);
    }
    let semicolon_terminated = match keyword {
        Some("static") => true,
        Some("const") => !matches!(significant_word(rest).0, Some("fn" | "unsafe" | "extern")),
        _ => false,
    };
    let b = text.as_bytes();
    let ident = |at: usize| b[at].is_ascii_alphanumeric() || b[at] == b'_';
    let (mut depth, mut line, mut i, mut opened) = (0usize, head, 0usize, false);
    let (mut parentheses, mut brackets) = (0usize, 0usize);
    while i < b.len() {
        match b[i] {
            b'\n' => line += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i + 1 < b.len() && b[i + 1] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let start = i + 2;
                let Some(length) = block_comment_end(&b[start..]) else { return last };
                line += b[start..start + length].iter().filter(|&&c| c == b'\n').count();
                i = start + length - 1;
            }
            // A raw string, r"…" / r#"…"# / br#"…"#: no escapes, ends at `"` + the same hashes.
            b'r' if (i == 0 || !ident(i - 1)) || (b[i - 1] == b'b' && (i < 2 || !ident(i - 2))) => {
                let hashes = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
                if b.get(i + 1 + hashes) == Some(&b'"') {
                    i += 2 + hashes;
                    while i < b.len() && !(b[i] == b'"' && b[i + 1..].iter().take(hashes).all(|&c| c == b'#') && b.len() - i > hashes) {
                        line += usize::from(b[i] == b'\n');
                        i += 1;
                    }
                    i += hashes;
                }
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        // An escape, including the `\` + newline continuation: its newline still counts.
                        line += usize::from(b.get(i + 1) == Some(&b'\n'));
                        i += 2;
                        continue;
                    }
                    line += usize::from(b[i] == b'\n');
                    i += 1;
                }
            }
            // A char literal ('{', '\'', '\u{7d}') is skipped whole; a lifetime ('a, 'static)
            // has no closing quote right after its first character and is left alone.
            b'\'' => {
                if b.get(i + 1) == Some(&b'\\') {
                    i += 2;
                    while i < b.len() && b[i] != b'\'' {
                        i += 1;
                    }
                } else if b.get(i + 2) == Some(&b'\'') {
                    i += 2;
                }
            }
            b'{' => {
                depth += 1;
                opened = true;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && parentheses == 0 && brackets == 0
                    && opened && !semicolon_terminated
                {
                    return line;
                }
            }
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b';' if depth == 0 && parentheses == 0 && brackets == 0 => return line,
            _ => {}
        }
        i += 1;
    }
    last
}

/// Split a source line into (code, same-line comment), respecting string literals so
/// a `//` inside `"http://…"` is not read as a comment.
///
/// Shared because every rule that asks "is this line justified / is this a real
/// occurrence" needs exactly this, and getting it subtly different per rule is how
/// two scanners disagree about the same file.
pub fn split_code_and_comment(line: &str) -> (&str, Option<&str>) {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if !in_str {
            if let Some(len) = char_literal_len(bytes, i) {
                i += len;
                continue;
            }
        }
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            return (&line[..i], Some(&line[i + 2..]));
        }
        i += 1;
    }
    (line, None)
}

/// The byte length of a char (or byte) literal starting at `i`, if one does: `'x'` (any
/// single byte, `'"'` and `'{'` included) or an escaped `'\x'` (`'\''`, `'\"'`). A lifetime
/// (`'a`) never has a closing `'` two bytes on, so it is never mistaken for one (Cormac,
/// review of #4678: `'"'` opened a string and hid every brace after it on the line).
fn char_literal_len(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\'') {
        return None;
    }
    if bytes.get(i + 2) == Some(&b'\'') && bytes.get(i + 1) != Some(&b'\\') {
        return Some(3);
    }
    if bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 3) == Some(&b'\'') {
        return Some(4);
    }
    None
}

/// The net brace depth change of a line of CODE (comments already split off), counting
/// only braces outside string literals and outside the `'{'` / `'}'` char literals, by
/// the same string tracking as [`split_code_and_comment`]. A brace in `"}"` is text, not
/// a block (Astra, review of #4678). Line-based like its sibling, so a string literal
/// spanning lines or a raw string with an unbalanced brace is outside its reach.
pub fn brace_delta(code: &str) -> i64 {
    outside_literals(code)
        .map(|(_, c)| match c {
            b'{' => 1,
            b'}' => -1,
            _ => 0,
        })
        .sum()
}

/// The bytes of a line of CODE that lie outside string and char literals, with their offsets:
/// the one literal-aware walk this lexer's line readers share ([`brace_delta`],
/// [`without_signature`]), so a literal is skipped the same way by every rule.
fn outside_literals(code: &str) -> impl Iterator<Item = (usize, u8)> + '_ {
    let bytes = code.as_bytes();
    let mut in_str = false;
    let mut escaped = false;
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < bytes.len() {
            let (at, c) = (i, bytes[i]);
            if in_str {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            if c == b'"' {
                in_str = true;
                i += 1;
                continue;
            }
            if let Some(len) = raw_string_len(bytes, i) {
                i += len;
                continue;
            }
            if let Some(len) = char_literal_len(bytes, i) {
                i += len;
                continue;
            }
            i += 1;
            return Some((at, c));
        }
        None
    })
}

/// The byte length of a raw string literal starting at `i`, if one does: `r"..."`, `r#"..."#` with
/// any number of `#`, and the byte forms `br"..."`, `br#"..."#`. Its body may hold `"` freely, so
/// only the closing quote followed by the same number of `#` ends it (Codex on #5609:
/// `write(path, r#"let source = "fn main() {}";"#)` hid the write). The prefix must start a token,
/// so an identifier ending in `r` or `br` is not one. A raw string left open on this line runs to
/// the line's end, the limit every line reader here shares.
fn raw_string_len(bytes: &[u8], i: usize) -> Option<usize> {
    let starts_token = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
    if !starts_token {
        return None;
    }
    let mut at = i;
    if bytes.get(at) == Some(&b'b') {
        at += 1;
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

/// `code` without a function signature it declares: what follows the body's opening brace, or
/// nothing when the line is only a signature. A declaration is `fn` at a word boundary followed by
/// whitespace, outside every literal: a `fn ` inside a string is text, so
/// `std::fs::write(path, "fn main() {}")` keeps its write visible (Codex on #5609).
pub fn without_signature(code: &str) -> &str {
    let bytes = code.as_bytes();
    let starts_fn = |at: usize| {
        bytes.get(at + 1) == Some(&b'n')
            && bytes.get(at + 2).is_some_and(u8::is_ascii_whitespace)
            && (at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_'))
    };
    let mut declared = false;
    for (at, c) in outside_literals(code) {
        if !declared {
            declared = c == b'f' && starts_fn(at);
        } else if c == b'{' {
            return &code[at + 1..];
        }
    }
    if declared { "" } else { code }
}

/// The numeric types a bound can wear. A `&str`, a `bool` or a `Duration` named `..._MAX`
/// is not a quantity a bound rule can reason about.
const NUMERIC_TYPES: &[&str] = &[
    "u8", "u16", "u32", "usize", "u64", "i32", "i64", "f32", "f64",
];

/// Is `token` a SCREAMING_SNAKE constant name? At least one underscore, so a type
/// (`Duration`) or a single-letter generic is never mistaken for one.
pub fn is_constant_name(token: &str) -> bool {
    token.len() >= 3
        && token.contains('_')
        && token.starts_with(|c: char| c.is_ascii_uppercase())
        && token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// `CONST_A * CONST_B` on this line — a bound assembled from constants alone, with
/// no measurement anywhere in it. Returns the two names.
///
/// A constant times a LITERAL (`MIN_SERVE_CTX * 8`) is deliberately NOT this shape:
/// it is a derivation from a named bound, which the de-hardcode guard already blesses
/// and which keeps one owner for the number.
pub fn constant_product<'a>(code: &'a str) -> Option<(&'a str, &'a str)> {
    let bytes = code.as_bytes();
    for (i, c) in code.char_indices() {
        if c != '*' {
            continue;
        }
        // `**`, `*/`, `/*` are not multiplication; nor is `*mut` / `*const`, whose
        // keywords are lowercase and so can never be constant names.
        if bytes.get(i + 1).is_some_and(|b| *b == b'*' || *b == b'/') {
            continue;
        }
        if i > 0 && (bytes[i - 1] == b'*' || bytes[i - 1] == b'/') {
            continue;
        }
        let left = trailing_identifier(&code[..i]);
        let right = leading_identifier(&code[i + 1..]);
        if let (Some(l), Some(r)) = (left, right) {
            if is_constant_name(l) && is_constant_name(r) {
                return Some((l, r));
            }
        }
    }
    None
}

fn trailing_identifier<'a>(before: &'a str) -> Option<&'a str> {
    let t = before.trim_end();
    let start = t
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map(|(i, _)| i)?;
    Some(&t[start..])
}

fn leading_identifier<'a>(after: &'a str) -> Option<&'a str> {
    let t = after.trim_start();
    let end = t
        .char_indices()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    Some(&t[..end])
}

/// `(name, type, value)` if this line declares a numeric constant.
pub fn numeric_const<'a>(code: &'a str) -> Option<(&'a str, &'a str, &'a str)> {
    let trimmed = code.trim_start();
    // Visibility does not change whether a declaration is a numeric constant.
    // Restricted visibility includes pub(in crate::owner), not only crate/super.
    let declaration = if let Some(restricted) = trimmed.strip_prefix("pub(") {
        restricted.split_once(')')?.1.trim_start()
    } else if let Some(public) = trimmed.strip_prefix("pub ") {
        public.trim_start()
    } else {
        trimmed
    };
    let rest = declaration.strip_prefix("const ")?;
    let (name, tail) = rest.split_once(':')?;
    let (ty, value) = tail.split_once('=')?;
    let ty = ty.trim();
    if !NUMERIC_TYPES.contains(&ty) {
        return None;
    }
    Some((name.trim(), ty, value.trim().trim_end_matches(';').trim()))
}

/// A value the compiler reads as a fresh guess: a bare decimal or float literal. An
/// expression over another named bound is a derivation and passes.
pub fn is_bare_literal(value: &str) -> bool {
    !value.is_empty()
        && value.starts_with(|c: char| c.is_ascii_digit())
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a `fn ` inside a string literal read as a signature, so the code before
    // the literal's brace was dropped and a real write escaped the writes ratchet (Codex on #5609).
    #[test]
    fn a_signature_is_declared_only_outside_literals() {
        let raw = r##"std::fs::write(path, r#"let source = "fn main() {}";"#)?;"##;
        assert_eq!(without_signature(raw), raw, "a fn inside a raw string with embedded quotes is text");
        let byte_raw = r##"let b = br#"fn x() { "}" }"#;"##;
        assert_eq!(without_signature(byte_raw), byte_raw);
        assert_eq!(brace_delta(byte_raw), 0, "braces inside a raw byte string are not blocks");
        assert_eq!(without_signature("let bar = barrier(fn_ptr);"), "let bar = barrier(fn_ptr);", "an identifier ending in r is not a raw string");
        let write = r#"std::fs::write(path, "fn main() {}")?;"#;
        assert_eq!(without_signature(write), write, "a fn inside a string is text");
        assert_eq!(without_signature("fn cap(&self) -> usize { self.grant.take(8) }"), " self.grant.take(8) }");
        assert_eq!(without_signature("pub fn recall_tokens(&self) -> usize {"), "");
        assert_eq!(without_signature("let defn = x { y };"), "let defn = x { y };", "fn inside a word is no declaration");
        assert_eq!(without_signature(r#"fn label() -> &'static str { "{" }"#), r#" "{" }"#);
    }

    // what this catches (Astra, review of #4678): a brace inside a string or char
    // literal counted as a block, so `{ "}" }` read as closing the enclosing impl.
    #[test]
    fn braces_in_string_and_char_literals_are_not_blocks() {
        assert_eq!(brace_delta(r#"fn label() -> &str { "}" }"#), 0);
        assert_eq!(brace_delta(r#"let s = "{{";"#), 0);
        assert_eq!(brace_delta(r#"let s = "a \" }";"#), 0, "an escaped quote does not end the string");
        assert_eq!(brace_delta("if c == '{' { open += 1;"), 1);
        assert_eq!(brace_delta("impl Runner {"), 1);
        assert_eq!(brace_delta("}"), -1);
        // Cormac, review of #4678: a quote char or byte literal is not a string.
        assert_eq!(brace_delta("if c == '\"' {"), 1);
        assert_eq!(brace_delta("} else if c == b'\"' {"), 0);
        assert_eq!(brace_delta(r"} else if c == b'\'' {"), 0, "an escaped quote char");
        assert_eq!(brace_delta("fn f<'a>(x: &'a str) {"), 1, "lifetimes are not char literals");
        // The same rule keeps the comment splitter right after a quote char literal.
        assert_eq!(split_code_and_comment("if c == '\"' { // note").1, Some(" note"));
    }

    /// What this catches: a `//` inside a string literal being read as the start of
    /// a comment — which would let `let u = fetch("http://x").unwrap();` count as
    /// "justified" because the URL's slashes look like a comment. Real shape: URLs
    /// and file paths appear on plenty of unwrap lines in this crate.
    #[test]
    fn a_double_slash_inside_a_string_is_not_a_comment() {
        let (code, comment) = split_code_and_comment(r#"let u = get("http://x").unwrap();"#);
        assert_eq!(comment, None, "the URL's // must not read as a comment");
        assert!(code.contains("unwrap"));

        let (code, comment) =
            split_code_and_comment(r#"let u = get("http://x").unwrap(); // JUSTIFIED: seeded"#);
        assert_eq!(comment, Some(" JUSTIFIED: seeded"));
        assert!(code.contains("unwrap"));

        // An escaped quote must not end the string early.
        let (_, comment) = split_code_and_comment(r#"let s = "a\"// b";"#);
        assert_eq!(comment, None);
    }

    // what this catches (card 0af5d5ec): a test module moved to its own file, marked
    // `#![cfg(test)]`, read as production, so its unwraps and spawns counted against the
    // ratchets and every "split the monolith by moving its tests" looked like growth.
    #[test]
    fn a_file_marked_inner_cfg_test_has_no_production_half() {
        let moved = "//! Tests for disk_eviction.\n#![cfg(test)]\nuse super::*;\nfn t() { x.unwrap(); }\n";
        assert_eq!(production_half(moved), "//! Tests for disk_eviction.");
        let inline = "fn real() {}\n#[cfg(test)]\nmod tests { fn t() { x.unwrap(); } }\n";
        assert_eq!(production_half(inline), "fn real() {}");
        let prose = "//! Mentions #![cfg(test)] in prose only.\nfn real() {}\n";
        assert_eq!(production_half(prose), prose, "a mention in a comment is not the marker");
    }

    // what this catches: the first cut's blind spot. A `mod tests;` declared with the other
    // modules near the top, or a gated test const above the code, hid every production line
    // below it (llamacpp_adapter.rs on #5225: 10 unwraps read as removed when none were).
    #[test]
    fn only_the_gated_item_is_test_code_never_the_rest_of_the_file() {
        let declared = "mod a;\n#[cfg(test)]\nmod tests;\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(declared), "mod a;\n\n\nfn real() { x.unwrap(); }");
        let konst = "#[cfg(test)]\n#[allow(dead_code)]\nstatic LOCK: Lazy<M> =\n    Lazy::new(|| M::new());\nfn real() { y.unwrap(); }\n";
        assert!(production_half(konst).ends_with("fn real() { y.unwrap(); }"));
        assert!(!production_half(konst).contains("LOCK"));
        let mid = "#[cfg(test)]\nmod tests {\n    fn t() { let s = \"}\"; let c = '}'; }\n}\nfn after() { z.unwrap(); }\n";
        let p = production_half(mid);
        assert!(p.contains("fn after()") && !p.contains("fn t()"), "{p:?}");
        assert_eq!(p.lines().count(), 5, "line numbers stay true");
        let helper = "fn real() {}\n#[cfg(test)]\nfn helper() {\n    q.unwrap();\n}\nfn more() {}\n";
        assert_eq!(production_half(helper), "fn real() {}\n\n\n\n\nfn more() {}");
        // Regression for #5227: a same-line test item must not consume its successor.
        let inline = "#[cfg(test)] mod tests {}\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(inline), "\nfn real() { x.unwrap(); }");
        let inline_decl = "#[cfg(test)] mod tests;\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(inline_decl), "\nfn real() { x.unwrap(); }");
        // A comment's semicolon cannot make the closed test module swallow production.
        let commented = "#[cfg(test)]\nmod tests {\n} // diagnostic; not syntax\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(commented), "\n\n\nfn real() { x.unwrap(); }");
        // Real constructor closing syntax still belongs to the gated static.
        let constructor = "#[cfg(test)]\nstatic X: LazyLock<()> = LazyLock::new(|| {\n});\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(constructor), "\n\n\nfn real() { x.unwrap(); }");
        // Regression for the first proposed fix: initializer blocks do not end a static.
        for initializer in [
            "{1} + { let y = 2;\n const EXTRA_LIMIT: usize = 9;\n y + EXTRA_LIMIT\n}",
            "{1usize}.wrapping_add({ let y = 2;\n const EXTRA_LIMIT: usize = 9;\n y + EXTRA_LIMIT\n})",
            "[ { let y = 2;\n const EXTRA_LIMIT: usize = 9;\n y + EXTRA_LIMIT\n} ][0]",
        ] {
            for declaration in [
                "static X", "const X", "pub(in crate::owner) const X",
                "pub\nconst X", "pub /* comment */ (crate) const X",
                "pub // visibility\n(in crate::owner) static X",
            ] {
                let raw = format!("#[cfg(test)]\n{declaration}: usize = {initializer};\nfn real() {{ x.unwrap(); }}\n");
                let projected = production_half(&raw);
                assert!(!projected.contains("EXTRA_LIMIT"), "{projected}");
                assert_eq!(projected.lines().last(), Some("fn real() { x.unwrap(); }"));
                assert_eq!(projected.lines().count(), raw.lines().count(), "line positions");
            }
        }
        // Const functions still end with their body, even when a comment has punctuation.
        let konst_fn = "#[cfg(test)]\nconst fn helper() -> usize { 1 } // ;\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(konst_fn), "\n\nfn real() { x.unwrap(); }");
        // Regression for proposed revision 4: nested comment delimiters are never code.
        let nested = "#[cfg(test)]\nfn helper() { /* outer /* inner */ ( [ { ;\n tail ) ] } */ }\nfn production() { x.unwrap(); }\n";
        assert_eq!(production_half(nested), "\n\n\nfn production() { x.unwrap(); }");
        let open_delimiter = "#[cfg(test)]\nfn helper() { /* outer /* inner */ ( */ }\nfn production() { x.unwrap(); }\n";
        assert_eq!(production_half(open_delimiter), "\n\nfn production() { x.unwrap(); }");

        // Regression for proposed revision 2: comments are whitespace between keywords.
        for trivia in ["/* explanation */", "// explanation\n", "/* outer /* nested */ tail */"] {
            let raw = format!("#[cfg(test)]\nconst {trivia} fn helper() -> usize {{ 1 }}\nfn production() {{ x.unwrap(); }}\n");
            let projected = production_half(&raw);
            assert_eq!(projected.lines().last(), Some("fn production() { x.unwrap(); }"));
            assert!(!projected.contains("helper"));
            assert_eq!(projected.lines().count(), raw.lines().count());
        }



    }
}
