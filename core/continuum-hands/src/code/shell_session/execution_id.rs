//! A shell execution's id as a typed handle: the id she types back reads through airc's `Shown`
//! (ONE-ACT stage 6), so a full id is exact, a prefix keeps every digit, and a UUID one digit off
//! reads by its short form, as for every other id she is shown.

use airc_core::shown_id::{Shown, ShownIdError, ShownKind};
use uuid::Uuid;

/// A shell execution's handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExecutionId(Uuid);

impl ExecutionId {
    /// A stored execution key, read typed. A key that is not an execution id is a corrupt store,
    /// said as one, never skipped.
    pub fn parse_stored(key: &str) -> Result<Self, CorruptExecutionKey> {
        Uuid::parse_str(key).map(Self).map_err(|source| CorruptExecutionKey { key: key.to_owned(), source })
    }

    /// A new execution's id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Its short form, as every surface shows an id (airc's `shown_form`, the one producer).
    pub fn shown(self) -> String {
        airc_core::shown_id::shown_form(self.0)
    }

    /// The key the session stores this execution under.
    pub fn key(self) -> String {
        self.0.to_string()
    }
}

impl ShownKind for ExecutionId {
    const LABEL: &'static str = "shell execution";
    fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
    fn to_uuid(self) -> Uuid {
        self.0
    }
}

/// A stored execution key that is not an execution id: a corrupt session store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorruptExecutionKey {
    pub key: String,
    pub source: uuid::Error,
}

impl std::fmt::Display for CorruptExecutionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid stored execution handle '{}': {}", self.key, self.source)
    }
}

impl std::error::Error for CorruptExecutionKey {}

/// The execution `raw` names among `candidates`, refused as `Shown`'s typed error (NoMatch and
/// Ambiguous carry what was found) when it names none or several.
pub fn execution_in(raw: &str, candidates: Vec<ExecutionId>) -> Result<ExecutionId, ShownIdError> {
    Shown::<ExecutionId>::try_from(raw)?.resolve(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (ONE-ACT stage 6): she awaits or kills an execution by the short handle
    // the shell showed her, and it resolves through the one reading; a stored key that is not an
    // execution id is a corrupt store, said as one (modules/code.rs used to skip it, so the
    // lookup read "no such execution").
    #[test]
    fn a_shown_execution_handle_resolves_and_a_corrupt_stored_key_is_reported() {
        let ran = ExecutionId::new();
        let other = ExecutionId::new();
        assert_eq!(execution_in(&ran.shown(), vec![ran, other]), Ok(ran));
        assert_eq!(execution_in(&ran.key(), vec![]), Ok(ran), "a full id is exact");
        assert!(matches!(execution_in("deadbeef", vec![ran, other]), Err(ShownIdError::NoMatch { .. })), "a miss is refused, typed");
        assert!(matches!(ExecutionId::parse_stored("not-an-id"), Err(CorruptExecutionKey { ref key, .. }) if key == "not-an-id"));
        assert_eq!(ExecutionId::parse_stored(&ran.key()), Ok(ran));
    }
}
