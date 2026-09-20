//! UTF-8-safe string truncation helpers.
//!
//! `&str` indexing in Rust slices by BYTE offsets — `s[..N]` panics with
//! "byte index N is not a char boundary" when N lands inside a multi-byte
//! UTF-8 sequence. The idiom `&s[..s.len().min(N)]` is therefore unsafe
//! for any text that might contain non-ASCII characters (emoji, accented
//! letters, CJK, etc.) — and chat content / decoded LLM tokens routinely
//! contain those.
//!
//! Concretely: this codebase had 8 sites doing `&s[..s.len().min(N)]` for
//! diagnostic / debug logging across persona cognition, inference backends,
//! and grid handlers. Each one was a latent panic that fired when a chat
//! message or decoded token happened to have a multi-byte char near the
//! truncation boundary. Production today tends to miss these because
//! tracing's compile-time level filter strips most `debug!` invocations,
//! but as soon as someone runs RUST_LOG=debug on real chat traffic the
//! crash surface opens.
//!
//! This module centralizes the safe-truncate primitive so every consumer
//! gets the same behavior and the lesson lands once rather than 8 times.
//! Per Joel 2026-05-30 "every error is an opportunity to battle harden" —
//! the fix isn't just the call sites, it's making the safe primitive the
//! easy thing to reach for.

/// Return the longest prefix of `s` whose byte length is at most
/// `max_bytes`, rounding DOWN to the nearest char boundary. Never
/// panics on UTF-8 multi-byte sequences.
///
/// `&s[..s.len().min(N)]` is the panic-prone idiom this replaces:
/// when byte index N lands inside a multi-byte UTF-8 sequence the
/// slice panics with "byte index N is not a char boundary." Real-world
/// trigger: a chat message with an emoji at byte 28-31 hits a 30-byte
/// truncation and crashes the persona cognition path.
///
/// Cost: O(min(4, max_bytes - actual_boundary)) — at most 3 backtracks
/// because UTF-8 chars are bounded to 4 bytes. Effectively free for the
/// log-truncate use case.
///
/// # Examples
///
/// ```ignore
/// # use continuum_core::utils::str_truncate::truncate_at_char_boundary;
/// assert_eq!(truncate_at_char_boundary("hello", 3), "hel");
/// assert_eq!(truncate_at_char_boundary("hello", 100), "hello");
/// assert_eq!(truncate_at_char_boundary("\u{1F44B} hi", 2), ""); // 👋 is 4 bytes
/// assert_eq!(truncate_at_char_boundary("\u{1F44B} hi", 4), "\u{1F44B}");
/// assert_eq!(truncate_at_char_boundary("héllo", 2), "h");      // é = 0xc3 0xa9
/// ```
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    // UTF-8 char length is bounded to 4 bytes, so this loop runs at
    // most 3 iterations before landing on a char boundary or 0.
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_truncates_to_exact_byte_count() {
        assert_eq!(truncate_at_char_boundary("hello world", 5), "hello");
        assert_eq!(truncate_at_char_boundary("hello world", 11), "hello world");
        assert_eq!(truncate_at_char_boundary("hello", 100), "hello");
    }

    #[test]
    fn max_bytes_zero_returns_empty() {
        assert_eq!(truncate_at_char_boundary("anything", 0), "");
        assert_eq!(truncate_at_char_boundary("", 0), "");
    }

    #[test]
    fn empty_input_always_returns_empty() {
        assert_eq!(truncate_at_char_boundary("", 5), "");
        assert_eq!(truncate_at_char_boundary("", 100), "");
    }

    #[test]
    fn multibyte_codepoint_backed_off_to_previous_boundary() {
        // 👋 (U+1F44B WAVING HAND SIGN) is 4 bytes in UTF-8: F0 9F 91 8B.
        // Truncating at byte 2 of "👋 hi" lands inside the emoji and must
        // back off to byte 0 (returning "") rather than panicking.
        let s = "\u{1F44B} hi";
        assert_eq!(s.len(), 7); // 4 bytes emoji + 1 space + 2 ascii
        assert_eq!(truncate_at_char_boundary(s, 0), "");
        assert_eq!(truncate_at_char_boundary(s, 2), "");
        assert_eq!(truncate_at_char_boundary(s, 3), "");
        assert_eq!(truncate_at_char_boundary(s, 4), "\u{1F44B}");
        assert_eq!(truncate_at_char_boundary(s, 5), "\u{1F44B} ");
        assert_eq!(truncate_at_char_boundary(s, 7), "\u{1F44B} hi");
    }

    #[test]
    fn two_byte_codepoint_handled() {
        // é (U+00E9 LATIN SMALL LETTER E WITH ACUTE) is 2 bytes: C3 A9.
        // "héllo" = h(1) + é(2) + l(1) + l(1) + o(1) = 6 bytes.
        let s = "héllo";
        assert_eq!(s.len(), 6);
        assert_eq!(truncate_at_char_boundary(s, 1), "h");
        assert_eq!(truncate_at_char_boundary(s, 2), "h"); // mid-é → back to 1
        assert_eq!(truncate_at_char_boundary(s, 3), "hé");
        assert_eq!(truncate_at_char_boundary(s, 4), "hél");
    }

    #[test]
    fn matches_pre_fix_idiom_for_ascii_only_inputs() {
        // The fix preserves the exact behavior of `&s[..s.len().min(N)]`
        // for ASCII-only inputs (no panics either way). This pins the
        // back-compat so future readers can confirm the swap is safe.
        let ascii = "the quick brown fox jumps over";
        for n in [0_usize, 1, 5, 10, 30, 31, 100].iter().copied() {
            let safe = truncate_at_char_boundary(ascii, n);
            let unsafe_idiom = &ascii[..ascii.len().min(n)];
            assert_eq!(
                safe, unsafe_idiom,
                "ASCII truncation diverged at n={n}: safe={safe:?} unsafe={unsafe_idiom:?}"
            );
        }
    }

    #[test]
    fn never_panics_on_arbitrary_unicode_boundaries() {
        // Brute-force: for every possible byte boundary 0..s.len(),
        // truncate_at_char_boundary must NOT panic. Pins the
        // contract that this primitive is total over all (s, n).
        let samples = [
            "\u{1F44B} hello \u{1F30D}", // emoji + ascii + emoji
            "café résumé naïve",         // accented latin
            "日本語のテスト",            // CJK
            "mixed 한국어 with English and emoji 🚀",
        ];
        for s in samples.iter() {
            for n in 0..=s.len() + 5 {
                // Just call it — no panic = pass.
                let _ = truncate_at_char_boundary(s, n);
            }
        }
    }
}
