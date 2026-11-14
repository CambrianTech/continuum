//! The scanning primitives this crate's matchers are written in.
//!
//! The matchers were `regex` patterns compiled on first use with `.unwrap()`: a fixed
//! pattern cannot fail at runtime, but the type said it could, and each call site claimed
//! otherwise by panicking. They are hand-written now, so no failure exists to represent,
//! and they skip the regex engine on the per-message path. Each primitive is exactly the
//! regex construct it replaced, and the tests pin that against `regex` itself (a
//! dev-dependency), over every `char` for the character classes:
//!
//! - [`is_space`] is `\s` (Unicode `White_Space`).
//! - [`is_digit`] is `\d` (Unicode `Nd`, the table `regex-syntax` 0.8 generates from UCD 16).
//! - [`fold_eq`] is `(?i)` for an ASCII pattern character: ASCII case, plus the two
//!   non-ASCII characters whose simple case folding lands on ASCII (KELVIN SIGN to `k`,
//!   LATIN SMALL LETTER LONG S to `s`).

/// `\s`: Unicode `White_Space`, which is exactly what `char::is_whitespace` tests.
pub(crate) fn is_space(c: char) -> bool {
    c.is_whitespace()
}

/// `\d`: Unicode general category `Nd` (Decimal_Number), as inclusive code point ranges.
/// Copied from `regex-syntax` 0.8.10 `unicode_tables::perl_decimal` (UCD 16.0.0), the table
/// the replaced patterns matched against; `digit_is_regex_digit_for_every_char` pins it.
const DECIMAL_NUMBER: [(u32, u32); 71] = [
    (0x30, 0x39), (0x660, 0x669), (0x6F0, 0x6F9), (0x7C0, 0x7C9), (0x966, 0x96F),
    (0x9E6, 0x9EF), (0xA66, 0xA6F), (0xAE6, 0xAEF), (0xB66, 0xB6F), (0xBE6, 0xBEF),
    (0xC66, 0xC6F), (0xCE6, 0xCEF), (0xD66, 0xD6F), (0xDE6, 0xDEF), (0xE50, 0xE59),
    (0xED0, 0xED9), (0xF20, 0xF29), (0x1040, 0x1049), (0x1090, 0x1099), (0x17E0, 0x17E9),
    (0x1810, 0x1819), (0x1946, 0x194F), (0x19D0, 0x19D9), (0x1A80, 0x1A89), (0x1A90, 0x1A99),
    (0x1B50, 0x1B59), (0x1BB0, 0x1BB9), (0x1C40, 0x1C49), (0x1C50, 0x1C59), (0xA620, 0xA629),
    (0xA8D0, 0xA8D9), (0xA900, 0xA909), (0xA9D0, 0xA9D9), (0xA9F0, 0xA9F9), (0xAA50, 0xAA59),
    (0xABF0, 0xABF9), (0xFF10, 0xFF19), (0x104A0, 0x104A9), (0x10D30, 0x10D39),
    (0x10D40, 0x10D49), (0x11066, 0x1106F), (0x110F0, 0x110F9), (0x11136, 0x1113F),
    (0x111D0, 0x111D9), (0x112F0, 0x112F9), (0x11450, 0x11459), (0x114D0, 0x114D9),
    (0x11650, 0x11659), (0x116C0, 0x116C9), (0x116D0, 0x116E3), (0x11730, 0x11739),
    (0x118E0, 0x118E9), (0x11950, 0x11959), (0x11BF0, 0x11BF9), (0x11C50, 0x11C59),
    (0x11D50, 0x11D59), (0x11DA0, 0x11DA9), (0x11F50, 0x11F59), (0x16130, 0x16139),
    (0x16A60, 0x16A69), (0x16AC0, 0x16AC9), (0x16B50, 0x16B59), (0x16D70, 0x16D79),
    (0x1CCF0, 0x1CCF9), (0x1D7CE, 0x1D7FF), (0x1E140, 0x1E149), (0x1E2F0, 0x1E2F9),
    (0x1E4F0, 0x1E4F9), (0x1E5F1, 0x1E5FA), (0x1E950, 0x1E959), (0x1FBF0, 0x1FBF9),
];

/// `\d`: is `c` a Unicode decimal digit (`Nd`)?
pub(crate) fn is_digit(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    let cp = u32::from(c);
    DECIMAL_NUMBER
        .binary_search_by(|&(lo, hi)| {
            if hi < cp {
                std::cmp::Ordering::Less
            } else if lo > cp {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// `(?i)` equality of a haystack char against an ASCII pattern char.
pub(crate) fn fold_eq(hay: char, pattern: char) -> bool {
    hay.eq_ignore_ascii_case(&pattern)
        || (hay == '\u{212A}' && pattern.eq_ignore_ascii_case(&'k'))
        || (hay == '\u{17F}' && pattern.eq_ignore_ascii_case(&'s'))
}

/// If `hay` starts with the ASCII `pattern` under `(?i)`, the byte length it spans in `hay`
/// (longer than `pattern` when a folded non-ASCII char stood in for a letter).
pub(crate) fn fold_prefix(hay: &str, pattern: &str) -> Option<usize> {
    let mut hay_chars = hay.char_indices();
    for p in pattern.chars() {
        match hay_chars.next() {
            Some((_, h)) if fold_eq(h, p) => {}
            _ => return None,
        }
    }
    Some(hay_chars.next().map_or(hay.len(), |(at, _)| at))
}

/// Every occurrence of the ASCII `pattern` in `hay` under `(?i)`, as `(start, end)` byte
/// offsets, at every start (occurrences may overlap).
pub(crate) fn fold_occurrences<'h>(hay: &'h str, pattern: &'h str) -> impl Iterator<Item = (usize, usize)> + 'h {
    hay.char_indices()
        .filter_map(move |(at, _)| fold_prefix(&hay[at..], pattern).map(|len| (at, at + len)))
}

/// The longest prefix of `s` whose chars all satisfy `pred`: its length in chars, and the
/// rest of `s` after it.
pub(crate) fn take_while(s: &str, pred: impl Fn(char) -> bool) -> (usize, &str) {
    let mut count = 0;
    for (at, c) in s.char_indices() {
        if !pred(c) {
            return (count, &s[at..]);
        }
        count += 1;
    }
    (count, "")
}

/// `s` without its first char when that char satisfies `pred`.
pub(crate) fn take_one(s: &str, pred: impl Fn(char) -> bool) -> Option<&str> {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if pred(c) => Some(chars.as_str()),
        _ => None,
    }
}

/// `[A-Z][a-zA-Z]+`: one capitalized ASCII word, returning the rest after it.
pub(crate) fn capitalized_word(s: &str) -> Option<&str> {
    let rest = take_one(s, |c| c.is_ascii_uppercase())?;
    let (letters, rest) = take_while(rest, |c| c.is_ascii_alphabetic());
    (letters >= 1).then_some(rest)
}

/// `(?m)^`-anchored `find_iter`: the non-overlapping, leftmost matches of `at` that start at
/// a line start (offset 0, or just after a `\n`), as byte ranges. `at` returns the byte
/// length of the match beginning at the start of its argument. A search resumes at the end
/// of the previous match, exactly as `Regex::find_iter` does.
pub(crate) fn line_start_matches(text: &str, at: fn(&str) -> Option<usize>) -> Vec<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut pos = 0;
    while pos <= bytes.len() {
        let start = if pos == 0 || bytes[pos - 1] == b'\n' {
            pos
        } else {
            match bytes[pos..].iter().position(|&b| b == b'\n') {
                Some(newline) => pos + newline + 1,
                None => break,
            }
        };
        match at(&text[start..]) {
            Some(len) if len > 0 => {
                found.push(start..start + len);
                pos = start + len;
            }
            _ => pos = start + 1,
        }
    }
    found
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use regex::Regex;

    /// A deterministic corpus for comparing a hand matcher against the regex it replaced:
    /// every string of the given tokens up to two long, then `count` pseudo-random strings
    /// of up to `max_tokens` tokens (a fixed-seed LCG, so a failure reproduces).
    pub(crate) fn corpus(tokens: &[&str], count: usize, max_tokens: usize) -> Vec<String> {
        let mut out = vec![String::new()];
        for a in tokens {
            out.push((*a).to_string());
            for b in tokens {
                out.push(format!("{a}{b}"));
            }
        }
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as usize
        };
        for _ in 0..count {
            let len = next() % (max_tokens + 1);
            let mut s = String::new();
            for _ in 0..len {
                s.push_str(tokens[next() % tokens.len()]);
            }
            out.push(s);
        }
        out
    }

    /// Every `char`, once each, in code point order.
    fn every_char() -> String {
        (0u32..=0x10FFFF).filter_map(char::from_u32).collect()
    }

    /// The chars of `all` that the single-char regex `re` matches, in order.
    fn regex_class(re: &str, all: &str) -> Vec<char> {
        Regex::new(re).unwrap().find_iter(all).flat_map(|m| m.as_str().chars()).collect()
    }

    // what this catches: `is_digit` drifting from regex's `\d` (Unicode Nd) on any char —
    // a stale or mistyped range in the copied table.
    #[test]
    fn digit_is_regex_digit_for_every_char() {
        let all = every_char();
        let ours: Vec<char> = all.chars().filter(|&c| is_digit(c)).collect();
        assert_eq!(ours, regex_class(r"\d", &all));
    }

    // what this catches: `is_space` drifting from regex's `\s` on any char.
    #[test]
    fn space_is_regex_space_for_every_char() {
        let all = every_char();
        let ours: Vec<char> = all.chars().filter(|&c| is_space(c)).collect();
        assert_eq!(ours, regex_class(r"\s", &all));
    }

    // what this catches: `fold_eq` missing a char that regex's `(?i)` folds onto an ASCII
    // letter (or accepting one it does not), for every pattern letter and every char.
    #[test]
    fn fold_eq_is_regex_case_insensitivity_for_every_ascii_letter() {
        let all = every_char();
        for p in ('a'..='z').chain('A'..='Z') {
            let ours: Vec<char> = all.chars().filter(|&c| fold_eq(c, p)).collect();
            assert_eq!(ours, regex_class(&format!("(?i){p}"), &all), "pattern letter {p}");
        }
    }

    // what this catches: the line-start iterator resuming in the wrong place (a match
    // consumed into the next line, a multibyte char right after a non-match) against
    // `(?m)^` find_iter.
    #[test]
    fn line_start_matches_is_multiline_find_iter() {
        fn two_word_colon(s: &str) -> Option<usize> {
            let rest = capitalized_word(s)?;
            let (spaces, rest) = take_while(rest, is_space);
            if spaces == 0 {
                return None;
            }
            let rest = capitalized_word(rest)?;
            let rest = take_one(rest, |c| c == ':')?;
            let rest = take_one(rest, is_space)?;
            Some(s.len() - rest.len())
        }
        let re = Regex::new(r"(?m)^[A-Z][a-zA-Z]+\s+[A-Z][a-zA-Z]+:\s").unwrap();
        let tokens = ["Ab", "Cd", " ", "\n", ":", "é", "x", "\n\n", "Ab Cd: "];
        for text in corpus(&tokens, 20_000, 10) {
            let want: Vec<_> = re.find_iter(&text).map(|m| m.range()).collect();
            assert_eq!(line_start_matches(&text, two_word_colon), want, "{text:?}");
        }
    }
}
