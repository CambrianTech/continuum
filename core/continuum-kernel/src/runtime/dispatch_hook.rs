//! What other layers attach around every dispatch, as a kernel contract.
//!
//! The dispatcher used to name two of them directly: a caller's hands admission
//! (`identity::hands_gate::admit`, held for the command's whole life) and the cognition pulse's
//! per-card work accounting. That held the kernel in the core's dependency cycle (it
//! imported Hands and the persona layer) and made each new around-dispatch concern another
//! edit to the dispatcher. Now the kernel owns the contract and its owners install
//! themselves once at boot; every executor in the process consults the same list, so no
//! executor can be built without them (Hands admission is a safety boundary).
//!
//! Order is install order. `enter` runs before the gate and may refuse the command; a
//! refusal ends the dispatch with nothing committed, as the hands' own refusal always did.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::routing::CallerIdentity;
use crate::runtime::command_executor::DispatchFailure;
use crate::runtime::CommandResult;

/// What one dispatch produces.
pub type Dispatched = Result<CommandResult, DispatchFailure>;

/// A boxed command future, as the dispatcher runs it.
pub type DispatchFuture<'a> = Pin<Box<dyn Future<Output = Dispatched> + Send + 'a>>;

/// One concern that attaches around every dispatch.
pub trait DispatchHook: Send + Sync + 'static {
    /// Its description, for probes and logs. Identity is the hook's type, never this name.
    fn name(&self) -> &'static str;

    /// Before the gate: `Ok(None)` when this call is not its concern, `Err` to refuse it.
    fn enter(&self, caller: Option<&CallerIdentity>) -> Result<Option<Arc<dyn DispatchScope>>, String>;
}

/// What a hook holds for one command it entered.
pub trait DispatchScope: Send + Sync {
    /// The gate allowed the command and it is about to run.
    fn allowed(&self) {}

    /// The command completed successfully.
    fn succeeded(&self) {}

    /// Whether this scope admitted the caller's hands context. A call bound to hands that no
    /// entered scope admits is refused (fail closed, card 5e7498d4): hands admission is a
    /// safety boundary, and an empty hook list must never let such a call through unadmitted.
    fn admits_hands(&self) -> bool {
        false
    }

    /// Run the command inside this scope (a task-local, a lease the command's children
    /// inherit). The default runs it as it is.
    fn wrap<'a>(self: Arc<Self>, work: DispatchFuture<'a>) -> DispatchFuture<'a> {
        work
    }
}

// parking_lot: no poisoning, so no recovery path to justify; the list only grows by whole entries.
static HOOKS: parking_lot::RwLock<Vec<(std::any::TypeId, Arc<dyn DispatchHook>)>> = parking_lot::RwLock::new(Vec::new());

/// Install `hook` for every dispatch in this process. A hook is identified by its type: a
/// second install of the same type keeps the first (boot may run more than once in tests, and
/// a hook must never run twice per call). No name or string decides identity.
pub fn install<H: DispatchHook>(hook: Arc<H>) {
    let id = std::any::TypeId::of::<H>();
    let mut hooks = HOOKS.write();
    if hooks.iter().all(|(installed, _)| *installed != id) {
        hooks.push((id, hook));
    }
}

/// Enter every installed hook for one dispatch, in install order. The first refusal ends it.
pub(crate) fn enter_all(caller: Option<&CallerIdentity>) -> Result<Vec<Arc<dyn DispatchScope>>, String> {
    let hooks: Vec<Arc<dyn DispatchHook>> = HOOKS.read().iter().map(|(_, hook)| Arc::clone(hook)).collect();
    let mut scopes = Vec::new();
    for hook in hooks {
        if let Some(scope) = hook.enter(caller)? {
            scopes.push(scope);
        }
    }
    let bound = caller.filter(|c| !c.releases_hands).and_then(|c| c.hands);
    if let Some(context) = bound {
        if !scopes.iter().any(|scope| scope.admits_hands()) {
            return Err(format!(
                "hands context {} has no admission installed in this process: refused, never run unadmitted",
                context.id()
            ));
        }
    }
    Ok(scopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Cormac on #5278, card 5e7498d4): a call bound to a caller's hands
    // dispatched with no hands admission because nothing installed one (an empty hook list),
    // fail-open on a safety boundary. It is refused; a call with no hands still passes.
    #[test]
    fn a_hands_bound_call_with_no_admission_installed_is_refused() {
        let peer = crate::identity::PeerId::from_uuid(uuid::Uuid::from_u128(7));
        let bound = CallerIdentity::airc(peer).in_hands(crate::identity::hands_context::HandsContext::fresh());
        let refused = enter_all(Some(&bound)).err().unwrap_or_default();
        assert!(refused.contains("no admission installed"), "{refused}");
        assert!(enter_all(Some(&CallerIdentity::airc(peer))).is_ok(), "a call with no hands is not this gate's concern");
        assert!(enter_all(None).is_ok());
    }
}

/// Run `work` inside every scope, the first installed outermost.
pub(crate) fn wrap_all<'a>(scopes: &[Arc<dyn DispatchScope>], work: DispatchFuture<'a>) -> DispatchFuture<'a> {
    scopes.iter().rev().fold(work, |inner, scope| Arc::clone(scope).wrap(inner))
}
