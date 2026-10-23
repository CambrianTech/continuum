//! Whose hands a code call acts with: the caller, and the context the caller is acting in.
//!
//! A citizen can act in more than one context at once: her live session, an eval of her,
//! a solve on a fork. Each has its own workspace and shell. Before this, every hands map
//! (`CodeState.file_engines` / `shell_sessions`, the acting-root record)
//! was keyed by the caller alone, so an eval and her live session were the same hands: an
//! eval's root moved her live tools, and its restore moved them home
//! (docs/architecture/HER-HANDS.md, "What acts on a citizen's hands today").
//!
//! The gate that admits work to one set of hands (and the lease a command holds) is the
//! kernel's (`identity::hands_gate`): the dispatcher and the engine's process owner hold it
//! too. This file keeps the hands KEY and the dispatch hook that admits a bound caller.
//!
//! [`HandsContext`] is bound by the kernel side of a connection
//! ([`CallerIdentity::in_hands`](crate::routing::CallerIdentity::in_hands)), never read from
//! params, so a caller can't name another context's hands. [`HandsKey`] is the one key
//! every hands map uses. The caller stays the authorization identity; the key only says
//! WHICH of her hands.


use crate::identity::hands_context::{ContextKind, HandsContext};
use crate::identity::hands_gate::{admit, admitted, HandsLease};

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
            Some(context) => format!("{}#{}:{}", self.caller, context.kind().tag(), context.id()),
        }
    }

    /// Whether the session keyed `key` is one of the hands a handle lookup from THESE hands
    /// may reach: an isolated context (an eval, a fork) reaches only itself; her default hands
    /// and her card hands reach each other (a handle from claim A stays hers after her focus
    /// moves to B) but never into an isolated context (Astra on #5026).
    pub fn reaches(&self, key: &str) -> bool {
        match self.context {
            Some(context) if context.kind() == ContextKind::Isolated => key == self.map_key(),
            _ => {
                key == self.caller
                    || key.strip_prefix(&self.caller).and_then(|rest| rest.strip_prefix('#')).is_some_and(|rest| {
                        rest.starts_with(ContextKind::Claim.tag()) || rest.starts_with(ContextKind::Unbound.tag())
                    })
            }
        }
    }
}

/// Hands admission as a dispatch hook: a caller bound to a context (a card claim's turn, an
/// eval, a fork) is ADMITTED to its hands for the command's whole life, foreground or
/// background, and the command runs as that admitted work, so a child process it starts keeps
/// the admission until its tree has ended. The release itself is the one call not admitted: it
/// closes the hands to every other (card fc9e9fd7; Astra on #5026). Installed at boot.
pub struct HandsAdmission;

struct Admitted(std::sync::Arc<HandsLease>);

impl crate::runtime::dispatch_hook::DispatchScope for Admitted {
    fn admits_hands(&self) -> bool {
        true
    }

    fn wrap<'a>(
        self: std::sync::Arc<Self>,
        work: crate::runtime::dispatch_hook::DispatchFuture<'a>,
    ) -> crate::runtime::dispatch_hook::DispatchFuture<'a> {
        Box::pin(admitted(Some(std::sync::Arc::clone(&self.0)), work))
    }
}

impl crate::runtime::dispatch_hook::DispatchHook for HandsAdmission {
    fn name(&self) -> &'static str {
        "hands_admission"
    }

    fn enter(
        &self,
        caller: Option<&crate::routing::CallerIdentity>,
    ) -> Result<Option<std::sync::Arc<dyn crate::runtime::dispatch_hook::DispatchScope>>, String> {
        let Some(context) = caller.filter(|c| !c.releases_hands).and_then(|c| c.hands) else {
            return Ok(None);
        };
        Ok(Some(std::sync::Arc::new(Admitted(std::sync::Arc::new(admit(context)?)))))
    }
}

/// What `code/release-hands` ended, and what it could confirm. The hands' own receipt, so a
/// mind ending an acting context reads it here.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
pub struct ReleaseHandsResult {
    /// The acting context that ended.
    pub context: String,
    /// Pids of executions that hadn't been proven ended: killed now, or killed earlier and
    /// not yet reaped.
    pub killed: Vec<u32>,
    /// Of those, the pids NOT confirmed gone: their exit couldn't be awaited, or the kernel
    /// still shows them or can't be read. Reported by name, never assumed dead.
    pub unconfirmed: Vec<u32>,
    /// Why an exit couldn't be awaited, one line per execution.
    pub errors: Vec<String>,
    /// True only when every execution was confirmed gone and the context's engine and shells
    /// were dropped. False keeps them, so the caller can retry and the acting place stays known.
    pub released: bool,
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
