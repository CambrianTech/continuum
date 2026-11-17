//! A list a contract requires to hold something: it cannot exist empty, so no handler ever checks for it.
//!
//! The list form of [`crate::non_empty_text::NonEmptyText`]: refused where the value is parsed, never complained
//! about by the handler that received it (Joel, 2026-11-02: "We should never have the ability to complain about a
//! parameter we require").

use serde::{Deserialize, Serialize};

/// At least one item. Serialized as the list itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    try_from = "Vec<T>",
    into = "Vec<T>",
    bound(serialize = "T: Serialize + Clone", deserialize = "T: Deserialize<'de>")
)]
pub struct NonEmptyList<T>(Vec<T>);

/// The list was empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyList;

impl std::fmt::Display for EmptyList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an empty list where the contract requires at least one item")
    }
}

impl std::error::Error for EmptyList {}

impl<T> NonEmptyList<T> {
    /// The list holding `first`: a list built from an item is never empty.
    pub fn of(first: T) -> Self {
        Self(vec![first])
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T> TryFrom<Vec<T>> for NonEmptyList<T> {
    type Error = EmptyList;
    fn try_from(items: Vec<T>) -> Result<Self, EmptyList> {
        match items.is_empty() {
            true => Err(EmptyList),
            false => Ok(Self(items)),
        }
    }
}

impl<T> From<NonEmptyList<T>> for Vec<T> {
    fn from(list: NonEmptyList<T>) -> Self {
        list.0
    }
}

impl<T> std::ops::Deref for NonEmptyList<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an empty required list reaching a handler; the parse is the one refusal.
    #[test]
    fn an_empty_list_never_parses_and_a_list_round_trips() {
        assert_eq!(NonEmptyList::<u8>::try_from(Vec::new()), Err(EmptyList));
        assert!(serde_json::from_str::<NonEmptyList<u8>>("[]").is_err(), "an empty list is refused at the parse");
        let list: NonEmptyList<u8> = serde_json::from_str("[1,2]").expect("a list parses");
        assert_eq!(serde_json::to_string(&list).expect("serializes"), "[1,2]"); // test: the wire form is the list
        assert_eq!(NonEmptyList::of(7).into_vec(), [7]);
    }
}
