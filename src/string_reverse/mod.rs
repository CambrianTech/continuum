//! A module for string reversal operations.
//!
//! Provides functions to reverse strings in various ways.

/// Re reverses a string slice by converting it to a char vector,
/// reversing that, and collecting back into a String.
///
/// # Arguments
///
/// * `input` - The input string to be reversed.
///
/// # Returns
///
/// A new String containing the characters of the input in reverse order.
pub fn reverse(input: &str) -> String {
    input.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse() {
        assert_eq!(reverse("hello"), "olleh");
        assert_eq!(reverse(""), "");
        assert_eq!(reverse("a"), "a");
    }
}