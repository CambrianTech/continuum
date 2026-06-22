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

use std::sync::Arc;

use uuid::Uuid;

use super::auth_policy::{AuthPolicy, CallerIdentity, CallerSource};
use super::{ForbiddenReason, RouteDecision, Verdict};
use crate::modules::grid::acl::is_command_authorized;
use crate::modules::grid::node::TrustLevel;

/// Resolves a signed peer (by airc `peer_id`) to its grid [`TrustLevel`] — the
/// airc↔grid trust bridge, behind an abstraction so the policy depends on the
/// SEAM, not a concrete store (tests use a trivial mock). `None` = "this peer is
/// unknown" (→ the remote default ceiling), distinct from `Some(Blocked)`
/// (explicitly denied).
///
/// IMPORTANT: the implementor MUST be keyed by the airc `peer_id`. The grid
/// `NodeRegistry` is NOT a valid source — it keys by transport address, a
/// different identity space (see modules/grid/registry.rs). A correct source is
/// airc-side enrollment/trust; wiring it is the airc↔grid identity unification
/// (task #38). Until that exists, [`GridTrustAuthPolicy::new`] (flat ceiling) is
/// used and this seam stays mock-tested but un-wired in production.
pub trait PeerTrustSource: Send + Sync {
    fn trust_of(&self, peer_id: Uuid) -> Option<TrustLevel>;
}

/// The trust ceiling for a REMOTE (airc/tcp) caller, even one the grid trusts
/// highly: a remote peer can be graduated up to [`Trusted`](TrustLevel::Trusted)
/// but NEVER to `Owner` — Owner-gated commands (`data/delete`, `grid/trust`, …)
/// stay local-only. The owner is the operator on the box, not a peer on the wire.
const REMOTE_TRUST_CEILING: TrustLevel = TrustLevel::Trusted;

/// Production auth policy: gates cross-grid (airc) + TCP callers by their resolved
/// grid trust, passes local/substrate callers. See module docs.
#[derive(Clone, Default)]
pub struct GridTrustAuthPolicy {
    /// The airc↔grid trust bridge. `None` → every remote caller resolves to the
    /// flat Provisional ceiling (the pre-bridge behavior). `Some` → a remote
    /// caller's REAL registered trust is honored (capped at
    /// [`REMOTE_TRUST_CEILING`]); a peer set to `Blocked` is actually denied.
    trust_source: Option<Arc<dyn PeerTrustSource>>,
}

impl std::fmt::Debug for GridTrustAuthPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridTrustAuthPolicy")
            .field("trust_bridge", &self.trust_source.is_some())
            .finish()
    }
}

impl GridTrustAuthPolicy {
    /// No trust bridge — every remote caller is capped at the Provisional ceiling
    /// (pre-bridge behavior; preserved for tests + un-wired callers).
    pub fn new() -> Self {
        Self { trust_source: None }
    }

    /// Wire the airc↔grid trust bridge with a peer_id-keyed [`PeerTrustSource`]
    /// (airc-side enrollment/trust — NOT the address-keyed grid `NodeRegistry`).
    /// A known peer's real trust is then honored (capped at Trusted); a `Blocked`
    /// peer is denied; an unknown peer keeps the Provisional ceiling. Not yet wired
    /// in production (no peer_id-keyed source exists — task #38); the seam is
    /// mock-tested.
    pub fn with_trust_source(trust_source: Arc<dyn PeerTrustSource>) -> Self {
        Self {
            trust_source: Some(trust_source),
        }
    }

    /// Resolve a caller's EFFECTIVE trust:
    /// - `None` / `Local` → `Owner` (the operator on the box).
    /// - `Airc` / `Tcp` (remote): the peer's registered trust if the bridge knows
    ///   it, **capped at [`REMOTE_TRUST_CEILING`]** (Owner→Trusted; `Blocked`
    ///   stays `Blocked` → denied); an unknown peer (or no bridge) → the
    ///   Provisional ceiling (the cross-grid default that admits `ai/generate`).
    ///
    /// This fixes the pre-bridge hole where EVERY airc caller — including one
    /// explicitly `Blocked` via `grid/trust` — resolved to Provisional.
    fn resolve_trust(&self, caller: Option<&CallerIdentity>) -> TrustLevel {
        match caller {
            None => TrustLevel::Owner,
            Some(c) => match c.source {
                CallerSource::Local => TrustLevel::Owner,
                CallerSource::Airc | CallerSource::Tcp => {
                    match self.trust_source.as_ref().and_then(|s| s.trust_of(c.peer_id)) {
                        Some(registered) => registered.min(REMOTE_TRUST_CEILING),
                        None => TrustLevel::Provisional,
                    }
                }
            },
        }
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
            // A TCP IPC caller is an unauthenticated remote socket — never owner.
            // Capped at the same remote ceiling as airc (Provisional): it can run
            // the AiSafe surface but NOT Owner-gated commands (data/delete,
            // grid/trust, …). Closes the "TCP == local owner" hole (security review
            // 2026-06-21). Stricter-than-airc (it has no verified peer) is a future
            // refinement; non-owner is the load-bearing guarantee.
            CallerSource::Tcp => AIRC_CALLER_CEILING,
        },
    }
}

impl AuthPolicy for GridTrustAuthPolicy {
    fn gate(&self, decision: &RouteDecision, caller: Option<&CallerIdentity>) -> Verdict {
        // Per-peer resolution (the trust bridge): a known remote peer gets its real
        // trust (capped at Trusted); Blocked is denied; unknown → Provisional.
        let trust = self.resolve_trust(caller);
        // Owner (local / substrate) — full access; not the cross-grid surface this
        // gate constrains. Otherwise enforce the grid ACL at the caller's trust.
        if trust >= TrustLevel::Owner {
            return Verdict::Allowed;
        }
        let path = decision.path();
        // Contracted-grid fast-path: a caller presenting an owner-signed capability
        // grant the airc command handler ALREADY verified (signature + key-binding
        // + mesh + expiry + epoch, against the authenticated sender key) carries the
        // conferred tags in `granted_capabilities`. If they confer THIS command, it
        // is authorized regardless of the tier ceiling — the explicit signed
        // contract overrides the coarse default trust. Sound because the field is
        // populated ONLY post-verification by the boundary; re-checked here through
        // grid_capability::confers (the one capability-match rule) so the gate stays
        // the authority. NOTE: a Blocked peer is denied BEFORE this (resolve_trust →
        // is_command_authorized below would deny), but a grant should not resurrect a
        // Blocked peer — so the fast-path is gated on trust > Blocked.
        if trust > TrustLevel::Blocked {
            if let Some(c) = caller {
                if !c.granted_capabilities.is_empty()
                    && crate::routing::grid_capability::confers(&c.granted_capabilities, path)
                {
                    return Verdict::Allowed;
                }
            }
        }
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

    // what this catches: THE per-peer trust bridge. With a trust source wired, a
    // remote caller's REAL grid trust is honored — but (1) a Blocked peer is denied
    // EVERYTHING (the fix: pre-bridge every airc caller got Provisional, so a
    // grid/trust-Blocked peer could still run AiSafe + ai/generate), (2) a remote
    // peer is CAPPED at Trusted so Owner-gated commands stay local-only even for an
    // Owner-registered peer, and (3) an unknown peer keeps the Provisional default.
    #[test]
    fn per_peer_trust_bridge_blocks_blocked_and_caps_remote_at_trusted() {
        use crate::modules::grid::node::TrustLevel;
        use std::collections::HashMap;

        struct MockTrust(HashMap<Uuid, TrustLevel>);
        impl PeerTrustSource for MockTrust {
            fn trust_of(&self, peer_id: Uuid) -> Option<TrustLevel> {
                self.0.get(&peer_id).copied()
            }
        }

        let blocked = Uuid::new_v4();
        let trusted = Uuid::new_v4();
        let owner_peer = Uuid::new_v4();
        let mut m = HashMap::new();
        m.insert(blocked, TrustLevel::Blocked);
        m.insert(trusted, TrustLevel::Trusted);
        m.insert(owner_peer, TrustLevel::Owner);
        let policy = GridTrustAuthPolicy::with_trust_source(Arc::new(MockTrust(m)));

        // (1) Blocked peer — denied everything, INCLUDING ai/generate and ping.
        let b = CallerIdentity::airc(blocked);
        assert!(matches!(
            policy.gate(&decision("ai/generate"), Some(&b)),
            Verdict::Forbidden { .. }
        ));
        assert!(matches!(
            policy.gate(&decision("ping"), Some(&b)),
            Verdict::Forbidden { .. }
        ));

        // (2) Trusted peer — graduated (≥ Provisional), but Owner commands are
        // local-only: still forbidden data/delete.
        let t = CallerIdentity::airc(trusted);
        assert_eq!(policy.gate(&decision("ai/generate"), Some(&t)), Verdict::Allowed);
        assert!(matches!(
            policy.gate(&decision("data/delete"), Some(&t)),
            Verdict::Forbidden { .. }
        ));

        // Owner-REGISTERED remote peer is CAPPED at Trusted → still forbidden the
        // Owner-only command. The owner is the operator on the box, never a peer.
        let o = CallerIdentity::airc(owner_peer);
        assert!(
            matches!(policy.gate(&decision("data/delete"), Some(&o)), Verdict::Forbidden { .. }),
            "a remote peer is capped at Trusted — Owner-gated commands stay local-only"
        );

        // (3) Unknown peer (not in the bridge) → Provisional default: ai/generate
        // allowed, Owner denied — the cross-grid default is preserved.
        let u = CallerIdentity::airc(Uuid::new_v4());
        assert_eq!(policy.gate(&decision("ai/generate"), Some(&u)), Verdict::Allowed);
        assert!(matches!(
            policy.gate(&decision("data/delete"), Some(&u)),
            Verdict::Forbidden { .. }
        ));
    }

    // what this catches: a TCP IPC caller (unauthenticated remote socket) is
    // remote-not-owner — capped at Provisional, so it can run the AiSafe surface
    // (ai/generate) but is FORBIDDEN every Owner-gated command. This is the policy
    // half of the "TCP == local owner" fix; the IPC server reuses this exact
    // caller_trust + is_command_authorized at its dispatch boundary.
    #[test]
    fn tcp_caller_is_remote_not_owner() {
        use crate::modules::grid::node::TrustLevel;
        let policy = GridTrustAuthPolicy::new();
        let tcp = CallerIdentity::tcp(Uuid::new_v4());

        assert_eq!(
            caller_trust(Some(&tcp)),
            TrustLevel::Provisional,
            "TCP is remote — capped at Provisional, never Owner"
        );
        assert_eq!(
            policy.gate(&decision("ai/generate"), Some(&tcp)),
            Verdict::Allowed,
            "TCP may run the cross-grid-inference surface"
        );
        for owner_only in ["data/delete", "grid/trust", "grid/pair"] {
            match policy.gate(&decision(owner_only), Some(&tcp)) {
                Verdict::Forbidden { .. } => {}
                other => panic!("TCP must be forbidden for {owner_only}, got {other:?}"),
            }
        }
    }

    // what this catches: THE contracted-grid fast-path. A remote airc caller is
    // capped at Provisional (only the ai/generate namespace is admitted), so
    // ai/embedding is normally FORBIDDEN. But when the caller carries a VERIFIED
    // granted capability for it (the airc handler having authorized an owner-signed
    // grant against the authenticated sender key), the gate honors the explicit
    // signed contract and ALLOWS it — overriding the coarse tier default. Without
    // the grant the same command is denied. This is the gate-side half of the
    // capability-grant wiring; the handler-side producer populates the field.
    #[test]
    fn verified_grant_overrides_tier_ceiling_for_conferred_command() {
        let policy = GridTrustAuthPolicy::new();

        // Baseline: a plain remote caller is denied ai/embedding (outside the
        // Provisional ai/generate namespace grant).
        let plain = CallerIdentity::airc(Uuid::new_v4());
        assert!(matches!(
            policy.gate(&decision("ai/embedding"), Some(&plain)),
            Verdict::Forbidden { .. }
        ));

        // Same caller, now carrying a verified grant conferring ai/embedding → the
        // signed contract authorizes it.
        let granted = CallerIdentity::airc(Uuid::new_v4())
            .with_granted_capabilities(vec!["ai/embedding".to_string()]);
        assert_eq!(
            policy.gate(&decision("ai/embedding"), Some(&granted)),
            Verdict::Allowed,
            "an owner-signed, handler-verified grant overrides the tier ceiling"
        );
        // Boundary-aware: the grant confers sub-commands on a / boundary…
        assert_eq!(
            policy.gate(&decision("ai/embedding/batch"), Some(&granted)),
            Verdict::Allowed
        );
        // …but NOT a command it doesn't confer.
        assert!(matches!(
            policy.gate(&decision("data/delete"), Some(&granted)),
            Verdict::Forbidden { .. }
        ));
    }

    // what this catches: a verified grant does NOT resurrect a BLOCKED peer. Even
    // carrying a conferring capability, a peer the owner explicitly Blocked is
    // denied — the fast-path is gated on trust > Blocked, so revocation-by-Block
    // can't be bypassed by also presenting an older grant.
    #[test]
    fn verified_grant_does_not_resurrect_a_blocked_peer() {
        use crate::modules::grid::node::TrustLevel;
        use std::collections::HashMap;

        struct MockTrust(HashMap<Uuid, TrustLevel>);
        impl PeerTrustSource for MockTrust {
            fn trust_of(&self, peer_id: Uuid) -> Option<TrustLevel> {
                self.0.get(&peer_id).copied()
            }
        }

        let blocked = Uuid::new_v4();
        let mut m = HashMap::new();
        m.insert(blocked, TrustLevel::Blocked);
        let policy = GridTrustAuthPolicy::with_trust_source(Arc::new(MockTrust(m)));

        let b = CallerIdentity::airc(blocked)
            .with_granted_capabilities(vec!["ai/embedding".to_string()]);
        assert!(
            matches!(
                policy.gate(&decision("ai/embedding"), Some(&b)),
                Verdict::Forbidden { .. }
            ),
            "a Blocked peer stays denied even with a conferring grant"
        );
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
