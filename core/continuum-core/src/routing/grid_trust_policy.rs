//! `GridTrustAuthPolicy` — the production [`AuthPolicy`] that enforces
//! the grid command ACL on **airc-sourced (cross-grid) callers**.
//!
//! ### Why this exists (slice 3 of the airc-native rebuild)
//!
//! The substrate ships [`AllowAllPolicy`](super::AllowAllPolicy) by
//! default so call sites don't break before a real policy is installed.
//! But a persona is **command-callable over airc**: its inbound pump
//! ([`PersonaCommandInboundPump`](crate::persona::command_inbound_pump))
//! forwards any `command-request` envelope from a room peer to the
//! shared [`CommandExecutor`], which threads
//! `CallerIdentity::airc(verified_sender)` into the gate. Under
//! `AllowAllPolicy` that means an UNTRUSTED outsider agent in the room
//! could address a privileged command (`data/delete`, `grid/trust`) to
//! the persona and have it execute. This policy closes that — the hard
//! gate Joel chose over soft prompt guidance.
//!
//! ### The policy
//!
//! - **Local / substrate callers** (`caller == None` or
//!   `CallerSource::Local`) → `Allowed`. Local dispatch is the
//!   substrate's own code; it is not the cross-grid attack surface.
//! - **Airc callers** → enforce the grid ACL via
//!   [`is_command_authorized`]. We cannot yet resolve a given airc
//!   peer's exact grid `TrustLevel` (the airc-enrollment ↔ grid
//!   `NodeRegistry` trust bridge is a separate slice — the registries
//!   are decoupled today), so we **cap at `Provisional`** — the level
//!   the cross-grid-inference rule (`ai/generate`, continuum#1649) is
//!   admitted at. Net effect: a remote room peer may request generation
//!   (`ai/generate`) and **nothing above it** — every `Owner`-gated
//!   command (`data/delete`, `grid/pair`, `grid/trust`, and the `""`
//!   wildcard that covers everything else) is denied regardless of who
//!   sent it. Privileged ops must arrive through the owner's own trusted
//!   local path, never an arbitrary room peer's command envelope.
//!
//! When the airc↔grid trust bridge lands, swap the hardcoded
//! `Provisional` ceiling for the peer's resolved `TrustLevel` so a
//! same-account Owner peer on another machine regains full access. Until
//! then the conservative cap is the safe floor — consistent with the
//! grid's "default to Blocked, elevate deliberately" posture.
//!
//! ### Scope: command surface only (for now)
//!
//! This policy gates the **command** surface. The airc **event-subscribe**
//! surface (`routing/airc_event_adapters.rs`) runs its own
//! `AuthPolicy::gate` but still defaults to `AllowAllPolicy` — a cross-grid
//! peer's *subscriptions* are not yet gated by this policy (adversarial
//! review of PR #1653, finding 3). That's intentionally out of scope here
//! (subscribe is read-ish, and routing it through this gate would deny it
//! — `events/*/subscribe` falls to the `""`=Owner wildcard). A follow-up
//! installs a subscribe-appropriate policy on that surface.

use super::auth_policy::{AuthPolicy, CallerIdentity, CallerSource};
use super::{ForbiddenReason, RouteDecision, Verdict};
use crate::modules::grid::acl::is_command_authorized;
use crate::modules::grid::node::TrustLevel;

/// Production auth policy: ACL-gates cross-grid (airc) callers, passes
/// local/substrate callers. See module docs.
#[derive(Debug, Default, Clone, Copy)]
pub struct GridTrustAuthPolicy;

impl GridTrustAuthPolicy {
    pub fn new() -> Self {
        Self
    }
}

/// The trust ceiling applied to airc-sourced callers until the
/// airc↔grid trust bridge can resolve a peer's real `TrustLevel`.
/// `Provisional` admits `ai/generate` (continuum#1649) and denies every
/// `Trusted`/`Owner` command.
const AIRC_CALLER_CEILING: TrustLevel = TrustLevel::Provisional;

/// The effective grid [`TrustLevel`] of a caller — the ONE place the caller→trust
/// rule lives, so the [`gate`](GridTrustAuthPolicy::gate) and every trust-aware
/// consumer (e.g. `commands/list` filtering "what can THIS caller call") share it
/// and can't drift. Local / substrate callers are the owner on their own box;
/// airc-sourced callers are capped at [`AIRC_CALLER_CEILING`] until the airc↔grid
/// per-peer trust bridge resolves a peer's real level.
pub fn caller_trust(caller: Option<&CallerIdentity>) -> TrustLevel {
    match caller {
        None => TrustLevel::Owner,
        Some(c) => match c.source {
            CallerSource::Local => TrustLevel::Owner,
            // TODO(airc-trust-bridge): EVERY airc caller maps to the Provisional
            // ceiling regardless of the peer's real grid trust — so a `Blocked`
            // peer is NOT distinguished here and gets Provisional's AiSafe surface.
            // This preserves prior gate behavior; closing it needs the airc↔grid
            // per-peer trust resolution (so Blocked/Trusted/Owner peers map to their
            // real level). Until then, upstream airc enrollment must keep blocked
            // peers from reaching the gate. Flagged by adversarial review 2026-06-21.
            CallerSource::Airc => AIRC_CALLER_CEILING,
        },
    }
}

impl AuthPolicy for GridTrustAuthPolicy {
    fn gate(&self, decision: &RouteDecision, caller: Option<&CallerIdentity>) -> Verdict {
        let trust = caller_trust(caller);
        // Owner (local / substrate) — full access; not the cross-grid surface this
        // gate constrains. Otherwise enforce the grid ACL at the caller's trust.
        if trust >= TrustLevel::Owner {
            return Verdict::Allowed;
        }
        let path = decision.path();
        if is_command_authorized(path, trust) {
            Verdict::Allowed
        } else {
            Verdict::Forbidden {
                reason: ForbiddenReason::NoPermissionForUri(path.to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{route, CommandUri};
    use uuid::Uuid;

    fn decision(path: &str) -> RouteDecision {
        route(&CommandUri::local(path))
    }

    // what this catches: the hard ACL gate's reason for existing — a
    // cross-grid (airc) caller can invoke ai/generate (the Provisional
    // cross-grid-inference rule) but is DENIED every privileged command.
    // Without this an untrusted room peer could data/delete via the
    // persona's command-inbound pump.
    #[test]
    fn airc_caller_may_generate_but_not_privileged() {
        let policy = GridTrustAuthPolicy::new();
        let airc = CallerIdentity::airc(Uuid::new_v4());

        assert_eq!(
            policy.gate(&decision("ai/generate"), Some(&airc)),
            Verdict::Allowed,
            "cross-grid inference must stay admitted (continuum#1649)"
        );

        for privileged in ["data/delete", "grid/trust", "grid/pair", "gpu/stats"] {
            match policy.gate(&decision(privileged), Some(&airc)) {
                Verdict::Forbidden {
                    reason: ForbiddenReason::NoPermissionForUri(uri),
                } => assert_eq!(uri, privileged),
                other => panic!("expected Forbidden for {privileged}, got {other:?}"),
            }
        }
    }

    // what this catches: local + substrate callers are NOT gated — the
    // owner's own local path keeps full access; only the cross-grid
    // surface is constrained. A regression that gated local callers
    // would lock the operator out of their own substrate.
    #[test]
    fn local_and_substrate_callers_pass() {
        let policy = GridTrustAuthPolicy::new();
        // None = substrate's own code.
        assert_eq!(policy.gate(&decision("data/delete"), None), Verdict::Allowed);
        // Local caller.
        let local = CallerIdentity::local(Uuid::new_v4());
        assert_eq!(
            policy.gate(&decision("data/delete"), Some(&local)),
            Verdict::Allowed
        );
    }

    // what this catches: the grid ACL matches by PREFIX, so the
    // `ai/generate` Provisional rule is a NAMESPACE grant — any future
    // `ai/generate*` path (e.g. `ai/generate/stream`) is also airc-
    // callable at Provisional. Intentional (the cross-grid inference
    // family); this test PINS it so the grant stays a conscious, reviewed
    // decision rather than a silent surprise the day someone adds an
    // `ai/generate-*` command (adversarial review of PR #1653, finding 1).
    // If a future `ai/generate*` command should NOT be cross-grid-callable,
    // tighten the acl.rs rule to exact-match — this test is the tripwire.
    #[test]
    fn ai_generate_is_a_provisional_namespace_grant() {
        let policy = GridTrustAuthPolicy::new();
        let airc = CallerIdentity::airc(Uuid::new_v4());
        for granted in ["ai/generate", "ai/generate/stream"] {
            assert_eq!(
                policy.gate(&decision(granted), Some(&airc)),
                Verdict::Allowed,
                "{granted} is within the intentional ai/generate namespace grant"
            );
        }
        // A sibling ai/* command OUTSIDE the namespace is still denied
        // (falls to the ""=Owner wildcard) — the grant is scoped.
        assert!(matches!(
            policy.gate(&decision("ai/embedding"), Some(&airc)),
            Verdict::Forbidden { .. }
        ));
    }
}
