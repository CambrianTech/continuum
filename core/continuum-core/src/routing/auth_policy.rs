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
//!     pub peer_id: PeerId,
//!     pub source: CallerSource,
//! }
//! ```
//!
//! `peer_id` is the canonical [`crate::identity::PeerId`] (airc's universal
//! actor id), NOT a bare `Uuid` — the same type the airc envelope carries and
//! the same type every other identity surface uses. The boundary to the
//! airc-address-keyed trust authority (`trust_of(Uuid)`) converts via
//! [`PeerId::as_uuid`] at the call, marking that one remaining address-space seam.
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

use crate::identity::PeerId;

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
    /// The caller is a LOCAL persona running in-process in THIS core —
    /// the owner's own autonomous agent (e.g. Asha) acting through its
    /// `CommandToolExecutor` over `InProcessTransport`. It is a local
    /// citizen of this machine, NOT a cross-grid peer: it resolves to
    /// `Trusted` (close-to-full access — file/shell/git), capped BELOW
    /// `Owner` so the most destructive ops (`data/delete`, `grid/trust`)
    /// stay the human operator's alone. A remote peer can NEVER present
    /// this source — only the local spawn path
    /// ([`CallerIdentity::local_persona`]) mints it, so it is unforgeable
    /// over the wire (the inbound pump stamps [`Airc`](CallerSource::Airc)).
    LocalPersona,
    /// The caller arrived over the core's TCP IPC listener — an
    /// UNauthenticated remote socket (e.g. a host-side client reaching a
    /// containerized core). Unlike [`Airc`](CallerSource::Airc) there is no
    /// signed envelope, so it must NOT be treated as local/owner: it is gated
    /// at the remote (non-owner) trust ceiling like any cross-grid caller.
    Tcp,
    /// The caller arrived over the core's WebSocket ingress — a thin client
    /// (browser/desktop/mobile) reaching the core over WS. Like [`Tcp`](CallerSource::Tcp)
    /// there is no signed envelope yet, so it is gated at the SAME remote
    /// (non-owner) trust ceiling: unauthenticated WS callers reach only the
    /// AiSafe surface. A GitHub-identity handshake (task #29) will later
    /// authenticate the socket and raise the ceiling per authenticated user;
    /// until then it is honestly labeled `Ws` (distinct from `Tcp` for
    /// telemetry) but shares Tcp's Provisional ceiling.
    Ws,
    /// The caller is an AI **observer** acting through a positron session — an
    /// agent that PERCEIVES a human's UI (the same typed `ViewState`s the human
    /// sees) and issues a command back through a `positron` `CommandEnvelope`
    /// whose `source` was `Observer { observer_id }`. `observer_id` names WHICH
    /// observer acted (provenance/audit — a confused deputy is only defensible
    /// if you can name it).
    ///
    /// ## The confused-deputy clamp (why this is its own source)
    ///
    /// An observer rides the SAME unauthenticated socket as the human whose UI
    /// it perceives — today a nil-peer [`Ws`](CallerSource::Ws) connection. If
    /// it inherited that transport's identity, then the moment a GH-auth
    /// handshake (task #29) elevated the human's socket, the AI observing that
    /// human's screen would silently inherit the human's elevated authority —
    /// the textbook confused-deputy escalation. So an observer is stamped with
    /// its OWN source that resolves to a FIXED [`TrustLevel::Provisional`](crate::modules::grid::node::TrustLevel::Provisional)
    /// ceiling which NEVER consults the trust bridge (see
    /// [`grid_trust_policy`](crate::routing::grid_trust_policy)). The human's
    /// authority can rise; the AI watching the human cannot ride it up.
    ///
    /// Today `Ws` and `PositronObserver` both resolve to `Provisional`, so the
    /// ceilings are equal — but the divergence is STRUCTURAL: it activates as
    /// soon as socket authentication elevates the human's `Ws` ceiling above
    /// Provisional.
    ///
    /// ## Precondition: the source declaration must be authenticated (task #29)
    ///
    /// This `CallerSource` is locally minted — the ONLY place it is stamped is
    /// the local positron dispatch path
    /// ([`CallerIdentity::positron_observer`], via `caller_for_source`); the
    /// inbound airc/ws pumps stamp [`Airc`](CallerSource::Airc)/[`Ws`](CallerSource::Ws).
    /// But WHICH identity that path mints is selected by the positron
    /// `CommandEnvelope`'s `source` discriminant, which is a client-declared,
    /// wire-deserialized field. So the clamp presumes an HONEST source: a
    /// compromised observer that self-labels `source: Human` would be minted
    /// `Ws`, not `PositronObserver`. This is harmless while `Ws` == `Provisional`
    /// (both floors), but the clamp is only COMPLETE once that source
    /// declaration is authenticated — the same task #29 handshake that raises
    /// the `Ws` ceiling must also bind the positron principal so `Human` can't be
    /// forged. Until then this is a tracked precondition, NOT an "unforgeable
    /// over the wire" guarantee.
    PositronObserver {
        /// Which positron observer issued the command (audit/provenance). Does
        /// NOT affect the trust ceiling — every observer is clamped identically.
        observer_id: String,
    },
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
    pub peer_id: PeerId,
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
    pub fn airc(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            source: CallerSource::Airc,
            granted_capabilities: Vec::new(),
        }
    }

    /// Construct a local-sourced caller identity. Used by tests
    /// that want to exercise non-trivial policies against a
    /// known peer_id.
    pub fn local(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            source: CallerSource::Local,
            granted_capabilities: Vec::new(),
        }
    }

    /// Construct a LOCAL-PERSONA caller identity — the owner's in-process
    /// autonomous agent (e.g. Asha) acting through its `CommandToolExecutor`.
    /// Resolves to `Trusted` at the gate (file/shell/git), capped below
    /// `Owner`. Only the local spawn path calls this; a remote peer can't
    /// present it (see [`CallerSource::LocalPersona`]).
    pub fn local_persona(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            source: CallerSource::LocalPersona,
            granted_capabilities: Vec::new(),
        }
    }

    /// Construct a TCP-sourced (unauthenticated remote socket) caller identity.
    /// The IPC server stamps this on connections from the TCP listener so they
    /// are gated as remote (non-owner), never as local/owner.
    pub fn tcp(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            source: CallerSource::Tcp,
            granted_capabilities: Vec::new(),
        }
    }

    /// Construct a WS-sourced (unauthenticated thin-client socket) caller
    /// identity. The WS ingress stamps this on every connection until a
    /// GitHub-identity handshake authenticates the socket. Gated as remote
    /// (non-owner) at the same Provisional ceiling as [`tcp`](Self::tcp) —
    /// AiSafe surface only — never as local/owner.
    pub fn ws(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            source: CallerSource::Ws,
            granted_capabilities: Vec::new(),
        }
    }

    /// An UNAUTHENTICATED socket: a `Ws`/`Tcp` caller the transport stamped
    /// with the nil peer id because nobody signed the connection. It is not
    /// an identity — it is the absence of one — so anything resolving "who is
    /// this" must fall through to the session's claimed actor (agent) or the
    /// operator, never render the nil uuid as a person. (The web desktop
    /// called the human "00000000-0000-…" for a day because `identity/whoami`
    /// read this sentinel as a persona — Joel, 2026-09-05.)
    pub fn is_anonymous_socket(&self) -> bool {
        self.peer_id.as_uuid().is_nil()
            && matches!(self.source, CallerSource::Ws | CallerSource::Tcp)
    }

    /// Construct a POSITRON-OBSERVER caller identity — an AI observer acting
    /// through a positron session over an unauthenticated socket. `peer_id` is
    /// the socket's peer (nil today, same as the human's `ws` connection);
    /// `observer_id` names which observer acted (audit). Resolves to a FIXED
    /// [`Provisional`](crate::modules::grid::node::TrustLevel::Provisional)
    /// ceiling that NEVER rises with socket authentication — the confused-deputy
    /// defense (see [`CallerSource::PositronObserver`]). Only the local positron
    /// dispatch path calls this; a remote peer can't present it.
    pub fn positron_observer(peer_id: PeerId, observer_id: String) -> Self {
        Self {
            peer_id,
            source: CallerSource::PositronObserver { observer_id },
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
    fn gate(&self, decision: &RouteDecision, caller: Option<&CallerIdentity>) -> Verdict;
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
        f.debug_struct("ClosurePolicy")
            .field("name", &self.name)
            .finish()
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
    // Room ids in synthetic URIs are still plain Uuids (not actor identity); PeerId
    // arrives via `use super::*`.
    use uuid::Uuid;

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
                &CommandUri::parse(&format!("airc://room:{}/chat/post", Uuid::new_v4()))
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
            policy.gate(&decision, Some(&CallerIdentity::airc(PeerId::new()))),
            Verdict::Allowed
        );
        assert_eq!(
            policy.gate(&decision, Some(&CallerIdentity::local(PeerId::new()))),
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
            policy.gate(&decision, Some(&CallerIdentity::local(PeerId::new()))),
            Verdict::Allowed
        );
        assert!(matches!(
            policy.gate(&decision, Some(&CallerIdentity::airc(PeerId::new()))),
            Verdict::Forbidden { .. }
        ));
    }

    #[test]
    fn caller_identity_constructors_set_source_correctly() {
        let id = PeerId::new();
        let airc = CallerIdentity::airc(id);
        assert_eq!(airc.peer_id, id);
        assert!(matches!(airc.source, CallerSource::Airc));

        let local = CallerIdentity::local(id);
        assert_eq!(local.peer_id, id);
        assert!(matches!(local.source, CallerSource::Local));
    }

    #[test]
    fn positron_observer_carries_its_observer_id_and_no_grants() {
        // what this catches: regression where the positron-observer constructor
        // (a) loses the observer_id that names WHICH AI acted (audit/provenance
        // for confused-deputy accountability), or (b) starts populating
        // granted_capabilities — which would be an unverified escalation, since
        // an observer presents no owner-signed grant. The trust CLAMP itself is
        // pinned in grid_trust_policy; here we pin the identity's shape.
        let id = PeerId::new();
        let obs = CallerIdentity::positron_observer(id, "asha-brain".to_string());
        assert_eq!(obs.peer_id, id);
        assert!(
            matches!(&obs.source, CallerSource::PositronObserver { observer_id } if observer_id == "asha-brain"),
            "observer_id must survive into the source for audit"
        );
        assert!(
            obs.granted_capabilities.is_empty(),
            "an observer presents no verified grant — capabilities stay empty"
        );
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
