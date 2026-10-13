//! Whose hands a code call acts with: the caller, and the context the caller is acting in.
//!
//! A citizen can act in more than one context at once: her live session, an eval of her,
//! a solve on a fork. Each has its own workspace and shell. Before this, every hands map
//! (`CodeState.file_engines` / `shell_sessions` / `parked_shells`, the acting-root record)
//! was keyed by the caller alone, so an eval and her live session were the same hands: an
//! eval's root moved her live tools, and its restore moved them home
//! (docs/architecture/HER-HANDS.md, "What acts on a citizen's hands today").
//!
//! [`HandsContext`] is bound by the kernel side of a connection
//! ([`CallerIdentity::in_hands`](crate::routing::CallerIdentity::in_hands)), never read from
//! params, so a caller can't name another context's hands. [`HandsKey`] is the one key
//! every hands map uses. The caller stays the authorization identity; the key only says
//! WHICH of her hands.

use uuid::Uuid;

/// One acting context of a caller: a session of work with its own workspace and shell.
/// `None` on a caller identity is her default (live) context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandsContext(Uuid);

impl HandsContext {
    /// A fresh context, distinct from every other: an eval or a fork opens one.
    pub fn fresh() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn id(&self) -> Uuid {
        self.0
    }
}

/// The key of one set of hands: whose (the caller) and in which context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandsKey {
    /// The caller's identity string (`code_commands::caller_id`): who acts, and whose
    /// citizen layer and permissions apply.
    pub caller: String,
    /// The acting context, or `None` for the caller's default context.
    pub context: Option<HandsContext>,
}

impl HandsKey {
    pub fn new(caller: String, context: Option<HandsContext>) -> Self {
        Self { caller, context }
    }

    /// The string a hands map is keyed by. The default context keeps the bare caller, so
    /// a caller with no context has exactly the hands it had before contexts existed.
    pub fn map_key(&self) -> String {
        match self.context {
            None => self.caller.clone(),
            Some(context) => format!("{}#{}", self.caller, context.id()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two contexts of one caller sharing a hands map entry, which is
    // how an eval's root moved a citizen's live tools. The default context must keep the
    // pre-context key, so live hands are unchanged until something opens a context.
    #[test]
    fn each_context_of_a_caller_is_its_own_hands_and_the_default_keeps_the_caller() {
        let caller = "c0ffee00-0000-0000-0000-000000000001".to_string();
        let live = HandsKey::new(caller.clone(), None);
        let eval = HandsKey::new(caller.clone(), Some(HandsContext::fresh()));
        let solve = HandsKey::new(caller.clone(), Some(HandsContext::fresh()));
        assert_eq!(live.map_key(), caller, "the default context is the caller's existing hands");
        assert_ne!(eval.map_key(), live.map_key(), "an eval is not her live hands");
        assert_ne!(eval.map_key(), solve.map_key(), "two contexts are two sets of hands");
        assert!(eval.map_key().starts_with(&caller), "a context's hands stay attributable to their caller");
    }
}
