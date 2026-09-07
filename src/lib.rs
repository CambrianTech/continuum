//! Reverses the order of words in a string.

/// Reverses the word order of `s`.
///
/// A word is any maximal run of non-whitespace characters, so leading,
/// trailing, and repeated internal whitespace is normalized away: the result
/// joins the reversed words with single spaces. An empty (or all-whitespace)
/// input yields an empty string.
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
    fn reverses_word_order() {
        assert_eq!(reverse_words("the sky is blue"), "blue is sky the");
    }

    #[test]
    fn single_word_round_trips() {
        assert_eq!(reverse_words("hello"), "hello");
    }

    #[test]
    fn empty_string_stays_empty() {
        assert_eq!(reverse_words(""), "");
    }

    #[test]
    fn whitespace_is_normalized() {
        assert_eq!(
            reverse_words("  hello   world\tfoo\nbar "),
            "bar foo world hello"
        );
    }
}
