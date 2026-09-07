//! Deliverable for work card b2f0818b — [bench coder-write-eval] reverse_words.
//!
//! `reverse_words` takes a string slice and returns a new `String` with the
//! words in reversed order. A word is any maximal run of non-whitespace, so
//! arbitrary runs of whitespace (spaces, tabs, newlines) separate them; the
//! result is joined by single spaces.

fn reverse_words(s: &str) -> String {
    s.split_whitespace()
        .rev()
        .collect::<Vec<&str>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_words_swap_order() {
        assert_eq!(reverse_words("hello world"), "world hello");
    }

    #[test]
    fn more_than_two_words() {
        assert_eq!(
            reverse_words("the quick brown fox"),
            "fox brown quick the"
        );
    }

    #[test]
    fn single_word_is_unchanged() {
        assert_eq!(reverse_words("bench"), "bench");
    }

    #[test]
    fn empty_string_stays_empty() {
        assert_eq!(reverse_words(""), "");
    }

    #[test]
    fn whitespace_only_becomes_empty() {
        assert_eq!(reverse_words("   \t\n  "), "");
    }

    #[test]
    fn collapses_repeated_whitespace_and_trims_edges() {
        assert_eq!(reverse_words("  a   b\tc "), "c b a");
    }

    #[test]
    fn unicode_words_round_trip() {
        assert_eq!(reverse_words("grüße wörter"), "wörter grüße");
    }
}
