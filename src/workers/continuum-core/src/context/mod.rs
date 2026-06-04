//! `Context` — the substrate's universal actor handle, Android-Context style.
//!
//! Per Joel 2026-06-04: "This is like Android context and must be fixed."
//! + [[airc-is-the-session-not-a-feature]].
//!
//! ## Mental model
//!
//! Android's `Context` is the ubiquitous handle every Android API takes.
//! You cannot `startActivity`, `openFileInput`, `getString(R.string.X)`,
//! `sendBroadcast`, or read a SharedPreference without one. Context
//! carries app identity (package, signature), resources, and services.
//!
//! The substrate's `Context` is the same shape. It carries:
//! - The actor's **`Identity`** (peer_id, kind, agent_name, home, room)
//! - The actor's **airc citizen handle** — the substrate-wide
//!   `Arc<dyn AircCitizen>` through which `say()`, `subscribe()`,
//!   `peer_id()` are reached
//! - (future slices) ORM scope, log scope, capture sinks
//!
//! Every substrate API that has substrate effect takes `&dyn Context`.
//! Functions that don't take a `Context` are either (a) pure or (b)
//! wrong and need fixing. There is no global airc handle, no implicit
//! "process-wide" actor — without a `Context`, you cannot reach the
//! substrate.
//!
//! ## Why a trait, not a struct
//!
//! Per the polymorphism doctrine in CLAUDE.md and the BaseUser
//! hierarchy plan named in `persona/supervisor.rs:108-118`:
//! - `PersonaContext` = `Context` + (role, profile, adapter, cognition)
//! - `ClaudeContext` = `Context` + (tool-use harness, model tier)
//! - `JtagContext` = `Context` + (CLI invocation args, stdio streams)
//! - `HumanContext` = `Context` + (UI session, auth scope)
//!
//! Each variant extends the substrate handle with kind-specific
//! state. The trait is the common contract; concrete types add what
//! their kind needs.
//!
//! ## What this slice IS
//!
//! - `Context` trait with `identity()` + `airc()` accessors — the
//!   minimum substrate-wide contract.
//! - `PersonaContext` impls `Context` (already had the fields; just
//!   adds the trait impl). Outlier 1 in the validation pattern.
//! - `StubContext` for tests — maximally different from
//!   `PersonaContext` (no cognition, no adapter). Outlier 2.
//! - **One new substrate utility** (`log_actor_action`) that takes
//!   `&dyn Context` — proves the trait is load-bearing, not theoretical,
//!   per the CLAUDE.md outlier-validation pattern.
//!
//! ## What this slice is NOT
//!
//! - `ClaudeContext` / `JtagContext` / `HumanContext` concrete types
//!   (Slice 3, with bootstrap paths)
//! - `&dyn Context` ubiquitous across substrate APIs (Slice 4)
//! - PersonaInstanceInfo → Identity migration in the persona module
//!   (Slice 1B; until then `PersonaContext::identity()` SYNTHESIZES
//!   an `Identity` from the underlying `PersonaInstanceInfo` on each
//!   call — acceptable per-turn cost given the substrate-overhead
//!   measurement [[substrate-overhead-is-1to3ms-LLM-dominates-latency]])
//! - ORM scope / log scope / capture sink services on Context (added
//!   when consumers appear, per the outlier-validation discipline)

use std::sync::Arc;

use crate::identity::Identity;
use crate::persona::airc_citizen::AircCitizen;

/// The substrate's universal actor handle.
///
/// Every substrate API that produces substrate-visible effect takes
/// `&dyn Context`. Implementors are: `PersonaContext` (today),
/// `ClaudeContext` / `JtagContext` / `HumanContext` (Slice 3),
/// `StubContext` (tests).
///
/// ### Why methods return what they return
///
/// - `identity()` returns by value, NOT by reference, because some
///   implementations (notably the transitional `PersonaContext` impl)
///   synthesize the `Identity` from underlying state on each call.
///   When the persona module migrates to store `Identity` directly
///   (Slice 1B), this can become `&Identity` cheaply; for now,
///   by-value is the honest signature.
/// - `airc()` returns `&Arc<dyn AircCitizen>` — every implementor
///   already holds the citizen as `Arc<dyn AircCitizen>` (per
///   `[[personas-are-citizens-airc-is-identity-provider]]`), so
///   borrowing is free.
pub trait Context: Send + Sync {
    /// The actor's identity — peer_id (== `id`), kind, name, home,
    /// default room, source. By value so transitional implementors
    /// can synthesize without holding a stored `Identity` field.
    fn identity(&self) -> Identity;

    /// The actor's live airc citizen handle. Substrate primitives
    /// that need to `say()`, `subscribe()`, get `peer_id()`, or
    /// otherwise interact with the grid reach them through this.
    fn airc(&self) -> &Arc<dyn AircCitizen>;
}

/// Substrate utility: emit a structured log line scoped to a
/// specific actor. The first proven `&dyn Context` consumer —
/// validates the trait is load-bearing, not theoretical, per the
/// CLAUDE.md outlier-validation pattern.
///
/// Why this lives in `context/`: every substrate-wide concern that
/// needs an actor's identity ("who did this") routes through
/// `&dyn Context`. Logging is the first such concern; capture
/// scoping, ORM-tenancy, and rate-limit attribution will follow in
/// the same shape (each adding a method on `Context` or a free
/// function alongside this one).
pub fn log_actor_action(ctx: &dyn Context, action: &str) {
    let id = ctx.identity();
    tracing::info!(
        actor.id = %id.id,
        actor.kind = ?id.kind,
        actor.name = %id.agent_name,
        action = %action,
        "actor action"
    );
}

// ─── Test fixture: StubContext ──────────────────────────────────────────

/// A `Context` implementor for tests — holds an `Identity` + a
/// stub airc citizen. Per `[[test-fixtures-are-system-primitives]]`
/// every test that needs a `&dyn Context` leases THIS rather than
/// inventing a bespoke variant.
///
/// Maximally different from `PersonaContext`: no role, no profile,
/// no adapter, no cognition. If the `Context` trait fits BOTH
/// (`PersonaContext` and `StubContext`), Outlier-A + Outlier-B
/// validate the interface for future variants per the CLAUDE.md
/// build-with-intent discipline.
///
/// `pub` (not `#[cfg(test)]`) so production code that wants a
/// no-substrate-effect Context for benchmarks or replay can use it
/// too. Stub adapters are first-class in the substrate per
/// `[[inference-is-an-adapter-always-in-the-loop]]`; stub Contexts
/// follow the same doctrine.
pub struct StubContext {
    identity: Identity,
    airc: Arc<dyn AircCitizen>,
}

impl StubContext {
    /// Construct with the actor's identity + a citizen handle.
    /// Tests typically pair this with `StubAircCitizen::new(peer_id)`
    /// for a no-network stub.
    pub fn new(identity: Identity, airc: Arc<dyn AircCitizen>) -> Self {
        Self { identity, airc }
    }
}

impl Context for StubContext {
    fn identity(&self) -> Identity {
        self.identity.clone()
    }

    fn airc(&self) -> &Arc<dyn AircCitizen> {
        &self.airc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IdentityKind, IdentitySource};
    use crate::persona::airc_citizen::StubAircCitizen;
    use uuid::Uuid;

    /// StubContext implements Context cleanly — identity round-trips
    /// through the trait surface, airc handle is reachable.
    #[test]
    fn stub_context_implements_context() {
        let peer_id = Uuid::new_v4();
        let identity = Identity {
            id: peer_id,
            kind: IdentityKind::Claude,
            agent_name: "Claude-Opus-4.7-test".to_string(),
            home_path: "/tmp/claude-test/airc".to_string(),
            default_room: Uuid::new_v4(),
            source: IdentitySource::FreshlyMinted,
        };
        let citizen: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(peer_id));
        let ctx: Box<dyn Context> = Box::new(StubContext::new(identity.clone(), citizen));

        let observed = ctx.identity();
        assert_eq!(observed.id, peer_id);
        assert_eq!(observed.kind, IdentityKind::Claude);
        assert_eq!(observed.agent_name, "Claude-Opus-4.7-test");
        assert_eq!(ctx.airc().peer_id(), peer_id);
    }

    /// `log_actor_action` accepts any `&dyn Context` — validates the
    /// trait works through dynamic dispatch, which is the substrate-
    /// wide shape every consumer will adopt. No assertion on the log
    /// output itself (tracing setup is outside this slice); the
    /// proof is "it compiles + runs."
    #[test]
    fn log_actor_action_takes_any_context() {
        let peer_id = Uuid::new_v4();
        let ctx = StubContext::new(
            Identity {
                id: peer_id,
                kind: IdentityKind::Jtag,
                agent_name: "jtag-test-invocation".to_string(),
                home_path: "/tmp/jtag/airc".to_string(),
                default_room: Uuid::new_v4(),
                source: IdentitySource::FreshlyMinted,
            },
            Arc::new(StubAircCitizen::new(peer_id)),
        );
        log_actor_action(&ctx, "test-action");
    }
}
