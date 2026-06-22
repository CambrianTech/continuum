//! `AuthPolicy` — the substrate's typed auth gate.
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md` §"Auth gate":
//! every URI passes through ONE chokepoint where a typed policy
//! evaluates whether the caller may execute. Today's variants
//! cover the local dispatch path; cross-grid callers carry their
//! peer identity via the airc envelope once the transport layer
//! lands.
//!
//! ## The shape
//!
//! ```ignore
//! trait AuthPolicy: Send + Sync {
//!     fn gate(&self, decision: &RouteDecision, caller: Option<&CallerIdentity>) -> Verdict;
//! }
//! ```
//!
//! The dispatcher calls `gate()` between `route()` and the variant
//! match. The returned [`Verdict`] (Allowed / Forbidden / Deferred)
//! determines whether dispatch proceeds, returns a typed refusal,
//! or queues a consent prompt.
//!
//! ## Why a trait
//!
//! Three near-term consumers:
//!
//! 1. **`AllowAllPolicy`** (default) — substrate ships with no
//!    policy curation; every dispatch is allowed. Existing callers
//!    and tests don't break, and operators opt in to real
//!    policy by replacing the trait impl at boot.
//!
//! 2. **`ClosurePolicy`** — test/fixture impl that delegates to a
//!    closure. Substrate-side test cases lease this instead of
//!    spinning up a full policy database.
//!
//! 3. **ORM-backed policy** (follow-up) — looks up `(caller_peer_id,
//!    uri_pattern)` rows in the substrate's policy table and
//!    returns the typed verdict. Swaps in via the same trait.
//!
//! Adding the cross-grid auth flow (peer signature verification,
//! capability tokens, sentinel quorum) lands as a fourth impl when
//! its slice arrives; the trait signature doesn't move.
//!
//! ## Caller identity
//!
//! Today's [`CallerIdentity`] is intentionally minimal:
//!
//! ```ignore
//! pub struct CallerIdentity {
//!     pub peer_id: Uuid,
//!     pub source: CallerSource,
//! }
//! ```
//!
//! Local dispatches today pass `None` for the caller — this substrate's
//! own code invoking commands on itself, which the default
//! `AllowAllPolicy` lets through. Cross-grid dispatches will populate
//! `Some(CallerIdentity { peer_id: airc_sender, source: CallerSource::Airc })`
//! when the transport layer extracts the sender from the envelope.
//!
//! Future fields (capability claims, signed tokens, session context)
//! land additively — `#[non_exhaustive]` keeps the struct extensible
//! without breaking external impls.

use std::sync::Arc;

use uuid::Uuid;

use super::{DeferredReason, ForbiddenReason, RouteDecision, Verdict};

/// Where a caller's request originated. The substrate's policy
/// impls often differentiate based on transport (a local in-process
/// caller is implicitly trusted; an airc-arriving caller needs
/// signature verification + policy lookup).
///
/// `#[non_exhaustive]` so adding (e.g.) `Grid` or `Bridge` later
/// is non-breaking for external policies.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallerSource {
    /// The caller is this substrate's own code invoking commands
    /// internally (e.g. a persona's service_loop dispatching via
    /// `Commands.execute`). Implicitly trusted by default policies.
    Local,
    /// The caller arrived via an airc envelope — typically a remote
    /// peer or sentinel. The transport layer extracted the sender's
    /// peer_id from the signed envelope before constructing this.
    Airc,
    /// The caller arrived over the core's TCP IPC listener — an
    /// UNauthenticated remote socket (e.g. a host-side client reaching a
    /// containerized core). Unlike [`Airc`](CallerSource::Airc) there is no
    /// signed envelope, so it must NOT be treated as local/owner: it is gated
    /// at the remote (non-owner) trust ceiling like any cross-grid caller.
    Tcp,
}

/// Caller identity passed to the auth gate. Cross-grid dispatches
/// carry the verified sender; local in-process calls pass `None`
/// (substrate's own code).
///
/// `#[non_exhaustive]` so future fields (capability claims, signed
/// tokens, session correlation) land additively.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CallerIdentity {
    pub peer_id: Uuid,
    pub source: CallerSource,
    /// Capability tags a transport boundary has CRYPTOGRAPHICALLY VERIFIED this
    /// caller may exercise for THIS dispatch — the conferred capabilities of an
    /// owner-signed `SignedCapabilityGrant` the caller presented, populated ONLY
    /// after [`GrantAuthorizer::authorize_command`](crate::routing::grid_capability::GrantAuthorizer::authorize_command)
    /// returns `Authorized` (signature + key-binding + mesh + expiry + epoch all
    /// checked against the AUTHENTICATED sender key).
    ///
    /// Default empty. A policy MAY treat a command conferred by these caps as
    /// authorized regardless of the caller's tier ceiling (the contracted-grid
    /// fast-path) — which is sound ONLY because the boundary verified them; no
    /// local/Tcp constructor ever populates this, and the field carries the
    /// SAME boundary-aware capability semantics the gate re-checks. Never set it
    /// from unverified input.
    pub granted_capabilities: Vec<String>,
}

impl CallerIdentity {
    /// Construct an airc-sourced caller identity. Used by the
    /// airc transport when it extracts the sender's peer_id
    /// from a verified envelope.
    pub fn airc(peer_id: Uuid) -> Self {
        Self {
            peer_id,
            source: CallerSource::Airc,
            granted_capabilities: Vec::new(),
        }
    }

    /// Construct a local-sourced caller identity. Used by tests
    /// that want to exercise non-trivial policies against a
    /// known peer_id.
    pub fn local(peer_id: Uuid) -> Self {
        Self {
            peer_id,
            source: CallerSource::Local,
            granted_capabilities: Vec::new(),
        }
    }

    /// Construct a TCP-sourced (unauthenticated remote socket) caller identity.
    /// The IPC server stamps this on connections from the TCP listener so they
    /// are gated as remote (non-owner), never as local/owner.
    pub fn tcp(peer_id: Uuid) -> Self {
        Self {
            peer_id,
            source: CallerSource::Tcp,
            granted_capabilities: Vec::new(),
        }
    }

    /// Attach the capability tags a transport boundary VERIFIED this caller may
    /// exercise (see [`granted_capabilities`](Self::granted_capabilities)). Called
    /// by the airc command handler after a presented `SignedCapabilityGrant`
    /// authorizes against the authenticated sender key — NEVER from unverified
    /// input. Builder-style so the boundary can layer it onto `airc(peer_id)`.
    pub fn with_granted_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.granted_capabilities = capabilities;
        self
    }
}

/// The substrate's auth-gate trait.
///
/// `Send + Sync` because policies are typically held behind
/// `Arc<dyn AuthPolicy>` and shared across the dispatcher's tasks.
///
/// The trait method is sync because typical policy lookups
/// (in-memory HashMap, ORM cached row, capability-token check)
/// complete in microseconds and the dispatcher is hot-path. An
/// ORM-backed impl that needs async I/O caches its rows behind an
/// `Arc<RwLock<...>>` warmed at boot and refreshed in the
/// background; the gate call itself stays sync.
pub trait AuthPolicy: Send + Sync + std::fmt::Debug {
    /// Evaluate the gate. Called once per dispatch, between
    /// `route()` and the dispatcher's variant match.
    ///
    /// `caller = None` means "this substrate's own code" —
    /// default policies treat it as implicitly trusted.
    fn gate(
        &self,
        decision: &RouteDecision,
        caller: Option<&CallerIdentity>,
    ) -> Verdict;
}

/// Default policy — every dispatch is allowed.
///
/// The substrate ships with this so existing call sites and tests
/// don't break when the gate is introduced. Operators wanting real
/// policy install an ORM-backed impl at boot via
/// `CommandExecutor::with_policy`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAllPolicy;

impl AuthPolicy for AllowAllPolicy {
    fn gate(&self, _decision: &RouteDecision, _caller: Option<&CallerIdentity>) -> Verdict {
        Verdict::Allowed
    }
}

/// A simple closure-backed policy useful for tests and ad-hoc
/// substrate configuration. Stores a function pointer so the impl
/// itself is `Clone + Debug`-safe even though typical closures
/// are not.
pub struct ClosurePolicy {
    name: &'static str,
    f: Arc<dyn Fn(&RouteDecision, Option<&CallerIdentity>) -> Verdict + Send + Sync>,
}

impl ClosurePolicy {
    pub fn new(
        name: &'static str,
        f: impl Fn(&RouteDecision, Option<&CallerIdentity>) -> Verdict + Send + Sync + 'static,
    ) -> Self {
        Self {
            name,
            f: Arc::new(f),
        }
    }
}

impl std::fmt::Debug for ClosurePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClosurePolicy").field("name", &self.name).finish()
    }
}

impl AuthPolicy for ClosurePolicy {
    fn gate(&self, decision: &RouteDecision, caller: Option<&CallerIdentity>) -> Verdict {
        (self.f)(decision, caller)
    }
}

/// Convenience policy for tests: deny any dispatch whose URI path
/// starts with `prefix`, allow everything else. Comes packaged so
/// tests don't reinvent the closure shape each time.
///
/// Returns `Verdict::Forbidden { reason: NoPermissionForUri(<path>) }`
/// — the same shape an ORM-backed deny would produce, so tests
/// pinning error messages are immediately portable.
pub fn deny_path_prefix(prefix: &'static str) -> ClosurePolicy {
    ClosurePolicy::new(prefix, move |decision, _caller| {
        if decision.path().starts_with(prefix) {
            Verdict::Forbidden {
                reason: ForbiddenReason::NoPermissionForUri(decision.path().to_string()),
            }
        } else {
            Verdict::Allowed
        }
    })
}

/// Convenience policy for tests: defer any dispatch whose URI path
/// starts with `prefix` to consent on the named env. Mirrors the
/// shape an ORM-backed "ask the user before evicting LoRA" policy
/// would produce.
pub fn defer_path_prefix(
    prefix: &'static str,
    prompt_target_env: super::EnvironmentId,
) -> ClosurePolicy {
    ClosurePolicy::new(prefix, move |decision, _caller| {
        if decision.path().starts_with(prefix) {
            Verdict::Deferred {
                reason: DeferredReason::AskTargetEnv,
                prompt_target_env: prompt_target_env.clone(),
            }
        } else {
            Verdict::Allowed
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{route, CommandUri, EnvironmentId};

    fn local_decision(path: &str) -> RouteDecision {
        route(&CommandUri::local(path))
    }

    #[test]
    fn allow_all_policy_allows_every_decision() {
        let policy = AllowAllPolicy;
        let cases = vec![
            local_decision("inference/llm/generate"),
            route(&CommandUri::parse("airc://maya/inference/llm/generate").expect("peer")),
            route(
                &CommandUri::parse(&format!(
                    "airc://room:{}/chat/post",
                    Uuid::new_v4()
                ))
                .expect("room"),
            ),
            route(&CommandUri::parse("airc://maya:*/notification/send").expect("broadcast")),
        ];
        for decision in cases {
            assert_eq!(policy.gate(&decision, None), Verdict::Allowed);
        }
    }

    #[test]
    fn allow_all_policy_is_insensitive_to_caller() {
        let policy = AllowAllPolicy;
        let decision = local_decision("x/y");
        assert_eq!(policy.gate(&decision, None), Verdict::Allowed);
        assert_eq!(
            policy.gate(&decision, Some(&CallerIdentity::airc(Uuid::new_v4()))),
            Verdict::Allowed
        );
        assert_eq!(
            policy.gate(&decision, Some(&CallerIdentity::local(Uuid::new_v4()))),
            Verdict::Allowed
        );
    }

    #[test]
    fn deny_path_prefix_blocks_matching_uri() {
        let policy = deny_path_prefix("cognition/genome/");
        let denied = local_decision("cognition/genome/lora-evict");
        match policy.gate(&denied, None) {
            Verdict::Forbidden {
                reason: ForbiddenReason::NoPermissionForUri(uri),
            } => {
                assert_eq!(uri, "cognition/genome/lora-evict");
            }
            other => panic!("expected Forbidden(NoPermissionForUri), got {other:?}"),
        }
    }

    #[test]
    fn deny_path_prefix_allows_non_matching_uri() {
        let policy = deny_path_prefix("cognition/genome/");
        let allowed = local_decision("data/list");
        assert_eq!(policy.gate(&allowed, None), Verdict::Allowed);
    }

    #[test]
    fn deny_path_prefix_matches_across_route_kinds() {
        // Same path prefix applied to a Peer URI: still denied. Auth
        // shouldn't depend on transport.
        let policy = deny_path_prefix("cognition/genome/");
        let peer_decision =
            route(&CommandUri::parse("airc://maya/cognition/genome/lora-evict").expect("peer"));
        assert!(matches!(
            policy.gate(&peer_decision, None),
            Verdict::Forbidden { .. }
        ));
    }

    #[test]
    fn defer_path_prefix_produces_consent_verdict() {
        let policy = defer_path_prefix("persona/state/", EnvironmentId::Named("web".into()));
        let decision = local_decision("persona/state/mutate");
        match policy.gate(&decision, None) {
            Verdict::Deferred {
                reason: DeferredReason::AskTargetEnv,
                prompt_target_env,
            } => {
                assert_eq!(prompt_target_env, EnvironmentId::Named("web".into()));
            }
            other => panic!("expected Deferred(AskTargetEnv), got {other:?}"),
        }
    }

    #[test]
    fn closure_policy_receives_caller_for_inspection() {
        // Policy that allows local callers, denies airc callers.
        let policy = ClosurePolicy::new("trust-local", |_d, caller| match caller {
            Some(c) if matches!(c.source, CallerSource::Airc) => Verdict::Forbidden {
                reason: ForbiddenReason::UnknownPeer,
            },
            _ => Verdict::Allowed,
        });

        let decision = local_decision("anything");
        assert_eq!(policy.gate(&decision, None), Verdict::Allowed);
        assert_eq!(
            policy.gate(&decision, Some(&CallerIdentity::local(Uuid::new_v4()))),
            Verdict::Allowed
        );
        assert!(matches!(
            policy.gate(&decision, Some(&CallerIdentity::airc(Uuid::new_v4()))),
            Verdict::Forbidden { .. }
        ));
    }

    #[test]
    fn caller_identity_constructors_set_source_correctly() {
        let id = Uuid::new_v4();
        let airc = CallerIdentity::airc(id);
        assert_eq!(airc.peer_id, id);
        assert!(matches!(airc.source, CallerSource::Airc));

        let local = CallerIdentity::local(id);
        assert_eq!(local.peer_id, id);
        assert!(matches!(local.source, CallerSource::Local));
    }

    #[test]
    fn policy_trait_object_dispatches_correctly() {
        // Proves the trait is object-safe + Arc-able the way the
        // dispatcher will hold it.
        let policy: Arc<dyn AuthPolicy> = Arc::new(deny_path_prefix("forbidden/"));
        let decision = local_decision("forbidden/x");
        assert!(matches!(
            policy.gate(&decision, None),
            Verdict::Forbidden { .. }
        ));

        let decision2 = local_decision("ok/y");
        assert_eq!(policy.gate(&decision2, None), Verdict::Allowed);
    }
}
