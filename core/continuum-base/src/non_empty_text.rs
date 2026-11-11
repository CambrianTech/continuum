//! Text a contract requires: it cannot exist empty or blank, so no handler ever checks for it.
//!
//! A required string parameter (a PR title, a message) is this type, refused where the value is
//! parsed, never complained about by the handler that received it (Joel, 2026-11-02: "We should
//! never have the ability to complain about a parameter we require").

use serde::{Deserialize, Serialize};

/// Non-blank text: at least one character that is not whitespace. Serialized as the text itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NonEmptyText(String);

/// The text was empty or only whitespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlankText;

impl std::fmt::Display for BlankText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("blank text where the contract requires text")
    }
}

impl std::error::Error for BlankText {}

impl NonEmptyText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for NonEmptyText {
    type Error = BlankText;
    fn try_from(text: String) -> Result<Self, BlankText> {
        match text.trim().is_empty() {
            true => Err(BlankText),
            false => Ok(Self(text)),
        }
    }
}

impl From<NonEmptyText> for String {
    fn from(text: NonEmptyText) -> Self {
        text.0
    }
}

impl std::ops::Deref for NonEmptyText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a blank required string reaching a handler; the parse is the one refusal.
    #[test]
    fn blank_text_never_parses_and_text_round_trips() {
        assert_eq!(NonEmptyText::try_from("  \n".to_string()), Err(BlankText));
        let title = NonEmptyText::try_from("her fix".to_string()).expect("text parses");
        assert_eq!(String::from(title), "her fix");
    }
}
