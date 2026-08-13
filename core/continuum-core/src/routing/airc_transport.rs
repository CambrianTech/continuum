//! `AircTransport` — real cross-grid `Transport` impl over airc-lib's
//! request/reply command bus.
//!
//! When the operator installs this transport via
//! `CommandExecutor::with_remote_transport(Arc::new(AircTransport::new(airc)))`,
//! every non-Local [`RouteDecision`] flows through here:
//!
//! 1. [`command_request_from_route_decision`] packages the typed
//!    decision into the wire envelope
//! 2. The envelope serializes as a [`Body::Json`]
//! 3. Headers are stamped (path, kind, env, body_hint) so middleware
//!    can filter without parsing the body
//! 4. `Airc::request(target, headers, body, deadline)` sends the
//!    frame; airc-lib auto-stamps correlation_id + reply_to + deadline
//! 5. `Airc::await_reply(pending)` waits for the matching reply
//! 6. The reply's body decodes as [`AircCommandResponse`] which
//!    collapses to `Result<Value, String>` via `into_result()`
//!
//! ## What this commit ships
//!
//! - [`AircTransport`] struct holding `Arc<airc_lib::Airc>` + default
//!   deadline
//! - `Transport` impl handling:
//!   - [`RouteDecision::Peer`] with [`PeerRef::Uuid`] — full
//!     send-and-await, the canonical cross-grid path
//!   - [`RouteDecision::Peer`] with [`PeerRef::Name`] — typed error
//!     pointing at the airc-side whois resolver (separate slice)
//!   - [`RouteDecision::Broadcast`] — wires to `MentionTarget::All`,
//!     first-reply-wins semantics
//!   - [`RouteDecision::Room`] — typed not-yet-implemented (room
//!     broadcast needs distinct semantics: all-replies-collect vs
//!     fire-and-forget; that decision belongs in its own slice)
//!   - [`RouteDecision::Local`] — loud BUG error (dispatcher routes
//!     Local inline; remote transport should never see it)
//!
//! ## What lands in the follow-up commits
//!
//! - **Peer-side command handler** — subscribes to events whose
//!   body_hint matches [`COMMAND_REQUEST_BODY_HINT`], decodes the
//!   envelope, dispatches via local `CommandExecutor` (threading
//!   `CallerIdentity::airc(verified_sender)` into the policy gate),
//!   replies via `Airc::reply()` with [`AircCommandResponse`].
//!
//! - **End-to-end integration test** — two `Airc` instances in
//!   process talking over a LAN socket. Persona A dispatches a
//!   command at `airc://<peer-b>/code/exists`; Persona B's handler
//!   runs it locally; the typed result flows back.
//!
//! - **Fake-local-grid integration test** — substrate-internal
//!   simulation: N `CommandExecutor` instances in process, a
//!   `LocalGridTransport` shuttling decisions between them. Same
//!   typed wire shape, no airc daemon, no LAN socket. Proves the
//!   dispatcher logic for multi-peer scenarios at unit-test speed.
//!
//! ## Why typed errors per variant
//!
//! Each non-implemented variant returns an error that names the
//! specific missing piece. An operator dispatching `airc://maya/...`
//! against a name (no UUID) sees `"peer-name resolution not yet
//! wired — use a peer UUID, or wait for the whois slice"`. They
//! don't see a generic "not implemented" — they see exactly what
//! to do or wait for. Same compression as the rest of Slice P:
//! errors carry their actionable cause.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use airc_core::{Body, MentionTarget, PeerId};
use airc_lib::Airc;
use airc_protocol::headers_keys::HEADER_AIRC_CAPABILITY_GRANT;

use super::airc_command_protocol::command_request_from_route_decision;
use super::presented_grant_store::PresentedGrantStore;
use super::{
    AircCommandRequest, AircCommandResponse, PeerRef, RouteDecision, Transport,
    COMMAND_REQUEST_BODY_HINT, HEADER_COMMAND_ENV, HEADER_COMMAND_KIND, HEADER_COMMAND_PATH,
    HEADER_CONTINUUM_BODY_HINT,
};
use crate::runtime::CommandResult;

/// Default deadline used when the caller didn't set one. Re-export of
/// the shared `continuum_airc_protocol::DEFAULT_COMMAND_DEADLINE` so
/// substrate and client agree by import, not by literal duplication
/// (per the wire-drift-prevention purpose of the protocol crate).
pub use continuum_airc_protocol::DEFAULT_COMMAND_DEADLINE as DEFAULT_DEADLINE;

/// The substrate's cross-grid `Transport`.
///
/// Holds an `Arc<airc_lib::Airc>` — typically the same handle the
/// substrate's `PersonaAircRuntime` uses for everything else. Clone
/// is cheap (single Arc clone); the transport is normally stored
/// behind another `Arc<dyn Transport>` on `CommandExecutor`.
pub struct AircTransport {
    airc: Arc<Airc>,
    deadline: Duration,
    /// Capability grants this node holds to present on outbound requests (the
    /// grantee side of the contracted grid). `None` → present nothing (tier-gated
    /// access only). `Some` → on a peer-targeted dispatch, stamp the held grant for
    /// that peer onto `HEADER_AIRC_CAPABILITY_GRANT` so the receiver can authorize
    /// the command against it.
    grant_store: Option<Arc<dyn PresentedGrantStore>>,
}

impl std::fmt::Debug for AircTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AircTransport")
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

impl AircTransport {
    /// Build a transport against an existing airc handle with the
    /// default deadline ([`DEFAULT_DEADLINE`]).
    pub fn new(airc: Arc<Airc>) -> Self {
        Self {
            airc,
            deadline: DEFAULT_DEADLINE,
            grant_store: None,
        }
    }

    /// Replace the default deadline. Builder-style.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Present held capability grants on outbound peer requests. Builder-style.
    /// When set, a dispatch to a peer this node holds a grant for stamps that grant
    /// on the request so the peer can authorize the command against it.
    pub fn with_grant_store(mut self, grant_store: Arc<dyn PresentedGrantStore>) -> Self {
        self.grant_store = Some(grant_store);
        self
    }

    /// Build the airc envelope headers for a given request. Exposed
    /// for tests + future composability (e.g. a middleware that
    /// adds capability tokens before send).
    pub fn build_headers(request: &AircCommandRequest) -> airc_core::Headers {
        let mut headers = airc_core::Headers::new();
        headers.insert(HEADER_COMMAND_PATH.to_string(), request.path.clone());
        headers.insert(HEADER_COMMAND_KIND.to_string(), request.kind.clone());
        if let Some(env) = &request.env {
            headers.insert(HEADER_COMMAND_ENV.to_string(), env.clone());
        }
        headers.insert(
            HEADER_CONTINUUM_BODY_HINT.to_string(),
            COMMAND_REQUEST_BODY_HINT.to_string(),
        );
        headers
    }

    /// Map a [`PeerRef`] to airc's [`MentionTarget`] for outbound
    /// dispatch. Name resolution requires airc-side whois (separate
    /// slice); a name-only peer reference returns a typed error.
    fn peer_ref_to_target(peer: &PeerRef) -> Result<MentionTarget, String> {
        match peer {
            PeerRef::Uuid(u) => Ok(MentionTarget::Peer(PeerId(*u))),
            PeerRef::Name(name) => Err(format!(
                "AircTransport: peer-name resolution not yet wired (name={name:?}) — \
                 use a peer UUID, or wait for the whois resolver slice. \
                 The airc-lib identity store has the mapping; a future slice \
                 plumbs it through to the transport."
            )),
        }
    }
}

impl AircTransport {
    /// Pre-flight conversion of a non-Local `RouteDecision` into the
    /// outbound airc envelope: `(MentionTarget, AircCommandRequest)`.
    ///
    /// PR #1529 reviewer 3 fix: factored out as a `pub` free function
    /// so every error branch (Local-guard, Peer-name not-implemented,
    /// Room semantics not-implemented, Broadcast silent-fallback
    /// refusal) is testable WITHOUT an `Airc` handle. The real
    /// `dispatch` method just delegates here, sends, awaits, decodes —
    /// leaving only the airc-side IO untested at the unit level
    /// (which is exactly what the LAN-loopback integration test in
    /// #188 covers).
    ///
    /// Returns `Err(typed_string)` for every documented refusal
    /// shape. Returns `Ok((target, request))` for the happy path
    /// (UUID peer dispatch).
    pub fn resolve_outbound(
        decision: &RouteDecision,
        params: Value,
    ) -> Result<(MentionTarget, AircCommandRequest), String> {
        // Per Transport::dispatch contract, the dispatcher never
        // routes Local decisions to a remote transport. Surface the
        // invariant breach loudly rather than silently ignore.
        if let RouteDecision::Local { .. } = decision {
            return Err("BUG: AircTransport received a Local decision — \
                 CommandExecutor::dispatch handles Local inline; \
                 remote transports never see this variant."
                .to_string());
        }

        // Resolve the outbound target before doing any serialization.
        // Cheaper error path for the not-yet-supported cases.
        let target = match decision {
            RouteDecision::Peer { peer, .. } => Self::peer_ref_to_target(peer)?,
            RouteDecision::Broadcast {
                peer, node, path, ..
            } => {
                // Per [[no-fallbacks-ever]]: env-wildcard broadcast to a
                // SPECIFIC peer cannot be silently mapped to
                // `MentionTarget::All` — that would fan out to every
                // peer in the room, including peers OTHER THAN the
                // target. The peer-side handler has no peer filter, so
                // those peers would process the request as if it were
                // addressed to them. Caught in PR #1529 reviewer 1.
                //
                // Per-peer env-fanout needs its own slice to design:
                // does it require the peer's env registry on the
                // sender side, or does the receiver filter on the peer
                // identity in the envelope, or does airc-lib gain a
                // typed `MentionTarget::PeerEnvWildcard(PeerId)`? Each
                // shape has different latency + auth implications.
                // Hard error today so the question gets answered when
                // someone needs it, not silently substituted.
                return Err(format!(
                    "AircTransport: env-wildcard broadcast to a specific peer \
                     not yet implemented. MentionTarget::All would broadcast \
                     to every peer in the room (not just env replicas of the \
                     target peer), which silently changes routing semantics \
                     and violates [[no-fallbacks-ever]]. Per-peer env-fanout \
                     semantics need their own slice. \
                     Routing was: peer={peer:?}, node={node:?}, path={path}"
                ));
            }
            RouteDecision::Room { room_id, .. } => {
                // Room broadcast semantics need their own slice: should
                // every subscribed peer reply? First-reply-wins? Fan-in
                // with timeout? That's a design decision, not an impl
                // detail. Hard error today so the question gets answered
                // when someone needs it, not silently substituted.
                return Err(format!(
                    "AircTransport: room broadcast routing not yet \
                     implemented (room={room_id}). Room semantics \
                     (all-replies-collect vs first-reply-wins vs \
                     fire-and-forget) need their own slice."
                ));
            }
            RouteDecision::Local { path, .. } => {
                // Belt-and-suspenders: the early-return above handles
                // this, but if a future refactor breaks that guard, the
                // error here surfaces as a typed BUG error rather than
                // an `unreachable!()` panic.
                return Err(format!(
                    "BUG: AircTransport reached the Local match arm \
                     (path={path}) — the guard at the top of resolve_outbound \
                     should have caught Local before this point. A future \
                     refactor must have broken that invariant."
                ));
            }
        };

        let request = command_request_from_route_decision(decision, params).ok_or_else(|| {
            "BUG: command_request_from_route_decision returned None for a non-Local decision"
                .to_string()
        })?;

        Ok((target, request))
    }

    /// Decode an airc reply's body as an `AircCommandResponse` and
    /// collapse to the canonical `Result<Value, String>` shape.
    ///
    /// PR #1529 reviewer 3 fix: factored as a `pub` free function so
    /// every error branch (no body, Binary body, malformed JSON, Error
    /// variant) is testable without a real `Airc` reply. The real
    /// `dispatch` method delegates here after `await_reply`.
    pub fn decode_reply(reply_body: Option<Body>) -> Result<Value, String> {
        let reply_body = reply_body.ok_or_else(|| {
            "AircTransport: reply has no body (peer-side handler must \
             attach Body::Json(AircCommandResponse))"
                .to_string()
        })?;

        let response_value = match reply_body {
            Body::Json(v) => v,
            Body::Binary(_) => {
                return Err("AircTransport: reply body was Binary; expected Json \
                            (AircCommandResponse is a JSON envelope)"
                    .to_string());
            }
        };

        let response: AircCommandResponse =
            serde_json::from_value(response_value).map_err(|e| {
                format!("AircTransport: deserialize reply body as AircCommandResponse: {e}")
            })?;

        response.into_result()
    }
}

#[async_trait]
impl Transport for AircTransport {
    async fn dispatch(
        &self,
        decision: RouteDecision,
        params: Value,
    ) -> Result<CommandResult, String> {
        // Pre-flight conversion (testable as a free function — see
        // routing::airc_transport::tests for every error branch).
        let (target, request) = Self::resolve_outbound(&decision, params)?;

        let body_value = serde_json::to_value(&request).map_err(|e| {
            format!("AircTransport: serialize AircCommandRequest to JSON value: {e}")
        })?;
        let body = Body::Json(body_value);

        let mut headers = Self::build_headers(&request);

        // Present a held capability grant (the grantee side of the contracted grid):
        // if this node holds a grant for the target PEER, stamp it so the receiver
        // can authorize the command against the owner's signature instead of the
        // bare tier ceiling. Only peer targets carry a single owner to present to;
        // room / wildcard targets have no single verifier, so nothing is stamped.
        if let (Some(store), MentionTarget::Peer(peer_id)) = (&self.grant_store, &target) {
            if let Some(grant_b64) = store.grant_for(*peer_id) {
                headers.insert(HEADER_AIRC_CAPABILITY_GRANT.to_string(), grant_b64);
            }
        }

        // Send + await. airc-lib stamps correlation_id, reply_to,
        // deadline automatically; the reply stream is armed BEFORE
        // the frame is sent (per airc_lib::Airc::request contract,
        // a fast same-process responder cannot win the race).
        let pending = self
            .airc
            .request(target, headers, body, self.deadline)
            .await
            .map_err(|e| format!("AircTransport: airc request failed: {e}"))?;

        let reply = self
            .airc
            .await_reply(pending)
            .await
            .map_err(|e| format!("AircTransport: await_reply failed: {e}"))?;

        // Decode via the testable free function. Every error path
        // here is covered by unit tests against `decode_reply`.
        let value = Self::decode_reply(reply.body)?;
        Ok(CommandResult::Json(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{route, CommandUri};
    use uuid::Uuid;

    /// `build_headers` produces the stable header set the peer-side
    /// handler subscribes against. Pin every header that lands so a
    /// silent refactor renaming one breaks the wire-test loudly.
    #[test]
    fn build_headers_stamps_path_kind_and_body_hint() {
        let request = AircCommandRequest {
            path: "code/exists".into(),
            kind: "peer".into(),
            env: None,
            params: serde_json::json!({"path": "foo"}),
        };
        let headers = AircTransport::build_headers(&request);
        assert_eq!(
            headers.get(HEADER_COMMAND_PATH).map(String::as_str),
            Some("code/exists")
        );
        assert_eq!(
            headers.get(HEADER_COMMAND_KIND).map(String::as_str),
            Some("peer")
        );
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
            Some(COMMAND_REQUEST_BODY_HINT)
        );
        // No env on this request → no env header
        assert!(headers.get(HEADER_COMMAND_ENV).is_none());
    }

    #[test]
    fn build_headers_includes_env_when_set() {
        let request = AircCommandRequest {
            path: "screenshot".into(),
            kind: "peer".into(),
            env: Some("vr".into()),
            params: Value::Null,
        };
        let headers = AircTransport::build_headers(&request);
        assert_eq!(
            headers.get(HEADER_COMMAND_ENV).map(String::as_str),
            Some("vr")
        );
    }

    #[test]
    fn peer_ref_uuid_maps_to_mention_target_peer() {
        let id = Uuid::new_v4();
        let target =
            AircTransport::peer_ref_to_target(&PeerRef::Uuid(id)).expect("uuid peer should map");
        match target {
            MentionTarget::Peer(peer_id) => assert_eq!(peer_id.0, id),
            other => panic!("expected MentionTarget::Peer, got {other:?}"),
        }
    }

    #[test]
    fn peer_ref_name_returns_typed_error_pointing_at_whois() {
        let err = AircTransport::peer_ref_to_target(&PeerRef::Name("maya".to_string()))
            .expect_err("name peer should error until whois lands");
        assert!(
            err.contains("whois"),
            "error must name the missing piece: {err}"
        );
        assert!(
            err.contains("maya"),
            "error must include the peer name: {err}"
        );
        assert!(
            err.contains("UUID"),
            "error must suggest the working alternative (use a UUID): {err}"
        );
    }

    // ─── PR #1529 reviewer 3 fix: real coverage of dispatch's logic ─

    /// Reviewer 3 BLOCK: extracted `resolve_outbound` lets us exercise
    /// the Local-guard, peer name rejection, broadcast refusal, and
    /// room not-implemented errors WITHOUT a real `Airc` instance.
    /// Replaces the prior stub tests.

    #[test]
    fn resolve_outbound_local_is_a_bug() {
        let local = route(&CommandUri::local("anything"));
        let err = AircTransport::resolve_outbound(&local, Value::Null)
            .expect_err("Local must be rejected at the transport boundary");
        assert!(
            err.contains("BUG"),
            "Local refusal must signal a BUG (the dispatcher routes Local inline; \
             reaching here means a refactor broke the invariant): {err}"
        );
    }

    #[test]
    fn resolve_outbound_room_returns_typed_not_implemented_error() {
        let room_id = Uuid::new_v4();
        let decision =
            route(&CommandUri::parse(&format!("airc://room:{room_id}/chat/post")).expect("parse"));
        let err = AircTransport::resolve_outbound(&decision, Value::Null)
            .expect_err("Room must not silently dispatch");
        assert!(
            err.contains("room broadcast routing not yet implemented"),
            "Room error must name the missing semantics: {err}"
        );
        assert!(
            err.contains(&room_id.to_string()),
            "Room error must echo the room_id so the operator can correlate: {err}"
        );
    }

    #[test]
    fn resolve_outbound_broadcast_refuses_silent_fallback() {
        let decision = route(&CommandUri::parse("airc://maya:*/notification/send").expect("parse"));
        let err = AircTransport::resolve_outbound(&decision, Value::Null)
            .expect_err("Broadcast must not silently map to MentionTarget::All");
        // PR #1529 reviewer 1 + 2 found the original silent-fallback;
        // pin the typed-refusal error so a future refactor can't
        // regress it.
        assert!(
            err.contains("env-wildcard broadcast"),
            "Broadcast error must name the dispatch class: {err}"
        );
        assert!(
            err.contains("[[no-fallbacks-ever]]"),
            "Broadcast error must cite the doctrine being upheld: {err}"
        );
    }

    #[test]
    fn resolve_outbound_peer_name_pending_whois_resolver() {
        let decision =
            route(&CommandUri::parse("airc://maya/inference/llm/generate").expect("parse"));
        let err = AircTransport::resolve_outbound(&decision, Value::Null)
            .expect_err("Name-only peers cannot resolve until whois slice lands");
        assert!(
            err.contains("whois"),
            "error must name the missing slice: {err}"
        );
    }

    #[test]
    fn resolve_outbound_peer_uuid_produces_target_and_request() {
        let id = Uuid::new_v4();
        let decision =
            route(&CommandUri::parse(&format!("airc://{id}/code/exists")).expect("parse"));
        let (target, request) =
            AircTransport::resolve_outbound(&decision, serde_json::json!({"path": "foo"}))
                .expect("UUID peer happy-path");
        match target {
            MentionTarget::Peer(peer_id) => assert_eq!(peer_id.0, id),
            other => panic!("expected Peer target, got {other:?}"),
        }
        assert_eq!(request.path, "code/exists");
        assert_eq!(request.kind, "peer");
        assert_eq!(request.params, serde_json::json!({"path": "foo"}));
    }

    /// Reviewer 3 BLOCK: extracted `decode_reply` makes every reply-
    /// path error testable without a real `Airc` reply.

    #[test]
    fn decode_reply_none_body_errors_with_actionable_message() {
        let err = AircTransport::decode_reply(None).expect_err("None body must error");
        assert!(
            err.contains("no body"),
            "error must name the missing body: {err}"
        );
        assert!(
            err.contains("peer-side handler"),
            "error must point at where to look (handler omitted body): {err}"
        );
    }

    #[test]
    fn decode_reply_binary_body_errors_with_shape_mismatch() {
        let err = AircTransport::decode_reply(Some(Body::Binary(vec![1, 2, 3])))
            .expect_err("Binary body must error");
        assert!(
            err.contains("Binary"),
            "error must name the surprising shape: {err}"
        );
        assert!(
            err.contains("Json"),
            "error must name the expected shape: {err}"
        );
    }

    #[test]
    fn decode_reply_malformed_json_errors_with_decode_context() {
        // Valid JSON but not the AircCommandResponse shape — should
        // surface the deserialize error in a way the operator can
        // correlate.
        let body = Body::Json(serde_json::json!({"unexpected": "shape"}));
        let err = AircTransport::decode_reply(Some(body))
            .expect_err("non-AircCommandResponse JSON must error");
        assert!(
            err.contains("deserialize"),
            "error must name the deserialize failure: {err}"
        );
    }

    #[test]
    fn decode_reply_ok_response_returns_value() {
        let response = AircCommandResponse::ok(serde_json::json!({"hello": "world"}));
        let body = Body::Json(serde_json::to_value(&response).unwrap());
        let value = AircTransport::decode_reply(Some(body)).expect("Ok response decodes");
        assert_eq!(value, serde_json::json!({"hello": "world"}));
    }

    #[test]
    fn decode_reply_error_response_returns_error_propagating_message() {
        let response = AircCommandResponse::error("forbidden: NoPermissionForUri(\"x/y\")");
        let body = Body::Json(serde_json::to_value(&response).unwrap());
        let err = AircTransport::decode_reply(Some(body))
            .expect_err("Error response must propagate as Err");
        // The remote peer's error message arrives exactly — uniform
        // shape across local + remote per the protocol design.
        assert_eq!(err, "forbidden: NoPermissionForUri(\"x/y\")");
    }

    /// command_request_from_route_decision is the typed bridge
    /// from RouteDecision to wire envelope; pin that the transport
    /// uses it correctly for the variants we DO implement.
    #[test]
    fn peer_uuid_decision_produces_request_with_kind_peer() {
        let id = Uuid::new_v4();
        let decision = route(
            &CommandUri::parse(&format!("airc://{id}/inference/llm/generate")).expect("parse"),
        );
        let request =
            command_request_from_route_decision(&decision, serde_json::json!({"prompt": "hi"}))
                .expect("Peer decision packages");
        assert_eq!(request.kind, "peer");
        assert_eq!(request.path, "inference/llm/generate");
        assert_eq!(request.params, serde_json::json!({"prompt": "hi"}));
    }

    #[test]
    fn broadcast_decision_produces_request_with_kind_broadcast() {
        let decision = route(&CommandUri::parse("airc://maya:*/notification/send").expect("parse"));
        let request = command_request_from_route_decision(&decision, Value::Null)
            .expect("Broadcast decision packages");
        assert_eq!(request.kind, "broadcast");
        assert_eq!(request.path, "notification/send");
    }
}
