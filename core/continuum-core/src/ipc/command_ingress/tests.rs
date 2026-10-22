#![cfg(test)]
    use super::*;
    use airc_core::{ClientId, EventId, MentionTarget, RoomId, TranscriptKind};
    use serde_json::Value;
    use uuid::Uuid;

    /// Build a minimal TranscriptEvent with the headers + body shape
    /// the handler expects. Used by parse + process tests.
    fn make_envelope(
        sender: PeerId,
        reply_to: PeerId,
        correlation: Uuid,
        request: &AircCommandRequest,
    ) -> TranscriptEvent {
        let body_value = serde_json::to_value(request).expect("serialize request");
        let mut headers = airc_core::Headers::new();
        headers.insert(HEADER_AIRC_REPLY_TO.to_string(), reply_to.0.to_string());
        headers.insert(
            HEADER_AIRC_CORRELATION_ID.to_string(),
            correlation.to_string(),
        );
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: sender,
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_700_000_000,
            lamport: 1,
            target: MentionTarget::Peer(reply_to),
            headers,
            body: Some(Body::Json(body_value)),
            attachment: None,
            receipt: None,
            metadata: Value::Null,
        }
    }

    fn sample_request() -> AircCommandRequest {
        AircCommandRequest {
            path: "code/exists".into(),
            kind: "peer".into(),
            env: None,
            params: serde_json::json!({"path": "foo.rs"}),
        }
    }

    // what this catches: the loopback — a peer-addressed request answered by whoever
    // hears it (the sender included). Only the named peer answers; a broadcast target
    // is nobody's to answer.
    #[test]
    fn only_the_addressed_peer_answers_a_command_request() {
        let me = PeerId::new();
        let other = PeerId::new();
        assert!(addressed_to(&MentionTarget::Peer(me), me));
        assert!(!addressed_to(&MentionTarget::Peer(other), me), "another peer's request is not ours");
        assert!(!addressed_to(&MentionTarget::All, me), "a broadcast is not a peer RPC");
        assert!(
            !addressed_to(&MentionTarget::Room(airc_core::RoomId::from_uuid(Uuid::new_v4())), me),
            "a room target is not a peer RPC"
        );
    }

    #[test]
    fn parse_envelope_extracts_caller_reply_correlation_and_request() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let envelope = make_envelope(sender, reply_to, correlation, &request);

        let parsed = CommandRequestHandler::parse_envelope(&envelope).expect("parse succeeds");

        assert_eq!(parsed.caller_peer_id, sender);
        assert_eq!(parsed.reply_to, reply_to);
        assert_eq!(parsed.correlation_id, correlation);
        assert_eq!(parsed.request, request);
        // No capability-grant header → no presented grant (the tier-gated default).
        assert!(parsed.presented_grant.is_none());
    }

    /// base64(JSON) of an owner-signed grant — the wire shape a caller presents in
    /// [`HEADER_AIRC_CAPABILITY_GRANT`].
    fn signed_grant_header(
        owner: &airc_protocol::PeerKeypair,
        grantee: PeerId,
        grantee_pubkey: Vec<u8>,
        caps: &[&str],
    ) -> String {
        use airc_lib::grid_auth::{CapabilityGrant, SignedCapabilityGrant};
        use airc_lib::subscriptions::MeshIdentity;
        let grant = CapabilityGrant {
            grantee,
            grantee_pubkey,
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
            granted_in: MeshIdentity::new("test-mesh"),
            issued_at_ms: 1,
            expires_at_ms: None,
            epoch: 1,
        };
        let signed = SignedCapabilityGrant::sign(owner, grant).expect("sign grant");
        STANDARD.encode(serde_json::to_vec(&signed).expect("serialize grant"))
    }

    // what this catches: parse_envelope DECODES a presented capability grant from
    // the base64 HEADER_AIRC_CAPABILITY_GRANT into the typed SignedCapabilityGrant
    // (the wire shape the verifier later checks). This is the one place the grant
    // wire format is read.
    #[test]
    fn parse_envelope_extracts_presented_grant() {
        let owner = airc_protocol::PeerKeypair::generate();
        let grantee = PeerId::new();
        let request = sample_request();
        let mut envelope = make_envelope(grantee, PeerId::new(), Uuid::new_v4(), &request);
        envelope.headers.insert(
            HEADER_AIRC_CAPABILITY_GRANT.to_string(),
            signed_grant_header(&owner, grantee, vec![1, 2, 3], &["ai/generate"]),
        );

        let parsed = CommandRequestHandler::parse_envelope(&envelope).expect("parse succeeds");
        let grant = parsed.presented_grant.expect("grant decoded");
        assert_eq!(grant.grant.capabilities, vec!["ai/generate".to_string()]);
        assert_eq!(grant.grant.grantee, grantee);
    }

    // what this catches: a PRESENT but undecodable grant header is surfaced LOUDLY
    // (per [[no-fallbacks-ever]]) rather than silently dropped — the caller intended
    // to present a grant; we don't pretend they didn't.
    #[test]
    fn parse_envelope_rejects_malformed_grant_header() {
        let mut envelope = make_envelope(
            PeerId::new(),
            PeerId::new(),
            Uuid::new_v4(),
            &sample_request(),
        );
        envelope.headers.insert(
            HEADER_AIRC_CAPABILITY_GRANT.to_string(),
            "!!!not-base64!!!".to_string(),
        );

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("malformed grant header should fail");
        match err {
            AdapterError::Consumer(msg) => assert!(
                msg.contains(HEADER_AIRC_CAPABILITY_GRANT),
                "error must name the grant header, got: {msg}"
            ),
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_missing_reply_to() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.headers.remove(HEADER_AIRC_REPLY_TO);

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("missing reply_to should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(
                    msg.contains(HEADER_AIRC_REPLY_TO),
                    "error must name the missing header, got: {msg}"
                );
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_missing_correlation_id() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.headers.remove(HEADER_AIRC_CORRELATION_ID);

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("missing correlation_id should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains(HEADER_AIRC_CORRELATION_ID));
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_missing_body() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.body = None;

        let err =
            CommandRequestHandler::parse_envelope(&envelope).expect_err("missing body should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(
                    msg.contains("no body"),
                    "error must name missing body: {msg}"
                );
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_binary_body() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.body = Some(Body::Binary(vec![1, 2, 3]));

        let err =
            CommandRequestHandler::parse_envelope(&envelope).expect_err("binary body should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains("Binary"));
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_malformed_body() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.body = Some(Body::Json(serde_json::json!({"unexpected": "shape"})));

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("malformed body should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains("decode"));
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn parse_envelope_rejects_invalid_correlation_uuid() {
        let sender = PeerId::new();
        let reply_to = PeerId::new();
        let correlation = Uuid::new_v4();
        let request = sample_request();
        let mut envelope = make_envelope(sender, reply_to, correlation, &request);
        envelope.headers.insert(
            HEADER_AIRC_CORRELATION_ID.to_string(),
            "not-a-uuid".to_string(),
        );

        let err = CommandRequestHandler::parse_envelope(&envelope)
            .expect_err("invalid correlation_id UUID should fail");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(msg.contains("not a valid UUID"));
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn handler_name_and_body_hint_match_protocol_constants() {
        // These are part of the wire contract — pin them so a silent
        // refactor changing one breaks the test loudly. The
        // caller-side AircTransport and the peer-side handler MUST
        // agree on the body_hint value.
        assert_eq!(HANDLER_NAME, "continuum.command.handler");
        // body_hint() must be the same constant the protocol module
        // exports — that's how the caller-side AircTransport knows
        // who to address.
        struct Fake; // build a fake without needing real Arc<Airc>
        impl Fake {
            fn body_hint(&self) -> &'static str {
                COMMAND_REQUEST_BODY_HINT
            }
        }
        assert_eq!(Fake.body_hint(), "continuum.command.request.v1");
    }

    // ─── PR #1529 reviewer fix tests ──────────────────────────────────

    /// Build a request envelope shape suitable for direct
    /// `process_request` testing. Lets us hit the kind/env rejection
    /// paths without going through the full envelope parser.
    fn make_parsed(request: AircCommandRequest) -> ParsedEnvelope {
        ParsedEnvelope {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request,
            request_channel: airc_core::RoomId::from_uuid(Uuid::new_v4()),
            request_channel_name: None,
            presented_grant: None,
        }
    }

    /// Reviewer 1 + 2 BLOCK: prove `process_request_via` rejects
    /// `kind="room"` rather than silently substituting Local routing.
    /// Uses the Airc-free entry point so the executor-side logic is
    /// exercisable without standing up airc plumbing.
    #[tokio::test]
    async fn process_request_rejects_room_kind() {
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);

        let parsed = make_parsed(AircCommandRequest {
            path: "chat/post".into(),
            kind: "room".into(),
            env: None,
            params: serde_json::Value::Null,
        });

        let response = CommandRequestHandler::process_request_via(&executor, &parsed).await;
        match response {
            AircCommandResponse::Error { message } => {
                assert!(
                    message.contains("kind=\"room\""),
                    "error must name the rejected kind, got: {message}"
                );
                assert!(
                    message.contains("not yet implemented"),
                    "error must signal not-yet-implemented, got: {message}"
                );
            }
            AircCommandResponse::Ok { .. } => {
                panic!("kind=room must be rejected, got Ok");
            }
        }
    }

    /// Reviewer 1 + 2 BLOCK: same shape, prove `kind="broadcast"`
    /// also rejected.
    #[tokio::test]
    async fn process_request_rejects_broadcast_kind() {
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);

        let parsed = make_parsed(AircCommandRequest {
            path: "notification/send".into(),
            kind: "broadcast".into(),
            env: None,
            params: serde_json::Value::Null,
        });

        let response = CommandRequestHandler::process_request_via(&executor, &parsed).await;
        assert!(matches!(response, AircCommandResponse::Error { .. }));
    }

    /// Reviewer 2 BLOCK: prove env-targeted dispatch is rejected
    /// (until env-aware local routing lands) instead of being silently
    /// dropped.
    #[tokio::test]
    async fn process_request_rejects_env_targeted_dispatch() {
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);

        let parsed = make_parsed(AircCommandRequest {
            path: "widget/show".into(),
            kind: "peer".into(),
            env: Some("vr".into()),
            params: serde_json::Value::Null,
        });

        let response = CommandRequestHandler::process_request_via(&executor, &parsed).await;
        match response {
            AircCommandResponse::Error { message } => {
                assert!(
                    message.contains("env=\"vr\""),
                    "error must name the rejected env, got: {message}"
                );
                assert!(
                    message.contains("env-aware local routing"),
                    "error must explain what's missing, got: {message}"
                );
            }
            AircCommandResponse::Ok { .. } => {
                panic!("env-targeted dispatch must be rejected");
            }
        }
    }

    /// Reviewer 3 BLOCK: prove `process_request_via` actually threads
    /// the verified caller_peer_id into the AuthPolicy gate. Closes
    /// the headline-of-the-PR coverage gap where the
    /// `execute_with_caller` branch was added but no test asserted
    /// the caller actually reached the gate.
    ///
    /// Builds a ClosureAuthPolicy that captures the caller it
    /// receives, dispatches via process_request_via, then asserts the
    /// captured caller matches what we packed into the envelope.
    #[tokio::test]
    async fn process_request_via_threads_caller_into_gate() {
        use crate::routing::{ClosurePolicy, RouteDecision};
        use std::sync::{Arc as StdArc, Mutex};

        let captured: StdArc<Mutex<Option<crate::routing::CallerIdentity>>> =
            StdArc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        // Policy that records the caller it receives, then allows.
        let policy = ClosurePolicy::new(
            "record-caller",
            move |_decision: &RouteDecision, caller: Option<&crate::routing::CallerIdentity>| {
                *captured_clone.lock().unwrap() = caller.cloned();
                crate::routing::Verdict::Allowed
            },
        );

        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor =
            crate::runtime::CommandExecutor::new(registry).with_policy(StdArc::new(policy));

        // Build an envelope with a known sender peer_id; assert
        // process_request_via threads it into the gate.
        let sender_peer_id = PeerId::new();
        let parsed = ParsedEnvelope {
            caller_peer_id: sender_peer_id,
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircCommandRequest {
                path: "anything/no-such-command".into(),
                kind: "peer".into(),
                env: None,
                params: serde_json::Value::Null,
            },
            request_channel: airc_core::RoomId::from_uuid(Uuid::new_v4()),
            request_channel_name: None,
            presented_grant: None,
        };

        // The path doesn't resolve to a module, so execute_with_caller
        // returns a typed CommandNotFound error per [[no-fallbacks-ever]]
        // (task #219 removed the silent TS fallthrough). We don't care
        // about that error here — the gate ran FIRST, recorded the
        // caller, and that's the property under test.
        let _ = CommandRequestHandler::process_request_via(&executor, &parsed).await;

        let observed = captured.lock().unwrap().clone();
        let observed = observed.expect(
            "AuthPolicy::gate must have been invoked with Some(caller) — \
             process_request_via failed to thread the caller through",
        );
        assert_eq!(
            observed.peer_id, sender_peer_id,
            "caller's peer_id must match the envelope sender — \
             closes the silent-privilege-escalation seam reviewer 2 flagged"
        );
        assert!(
            matches!(observed.source, crate::routing::CallerSource::Airc),
            "caller source must be Airc (cross-grid), not Local: {observed:?}"
        );
    }

    // what this catches (card c84d885a, 2026-09-18): a generate that arrives over the wire
    // performing inference on this seat WITHOUT the seat counting it. The 5090 served three
    // nodes' coders on one slot because nothing counted inbound generates as demand. The
    // gauge must be held for the whole dispatch — the gate runs inside it, so a policy
    // that reads the gauge at gate time sees the call — and ONLY for `ai/generate`: a
    // peer's `work/list` is not inference and must not inflate a seat's slot count.
    // Read relative to a baseline taken before, never as an absolute (the #1960 flake class).
    #[tokio::test]
    async fn an_inbound_generate_is_counted_as_leased_in_demand_for_the_whole_dispatch() {
        use crate::inference::resource_admission::leased_in_calls;
        use crate::routing::{ClosurePolicy, RouteDecision};
        use std::sync::{Arc as StdArc, Mutex};
        let seen: StdArc<Mutex<Vec<(String, usize)>>> = StdArc::new(Mutex::new(Vec::new()));
        let seen_clone = seen.clone();
        let policy = ClosurePolicy::new(
            "record-leased-in-at-gate",
            move |decision: &RouteDecision, _caller: Option<&crate::routing::CallerIdentity>| {
                seen_clone
                    .lock()
                    .unwrap()
                    .push((format!("{decision:?}"), leased_in_calls()));
                crate::routing::Verdict::Allowed
            },
        );
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        let executor =
            crate::runtime::CommandExecutor::new(registry).with_policy(StdArc::new(policy));
        let envelope = |path: &str| ParsedEnvelope {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircCommandRequest {
                path: path.into(),
                kind: "peer".into(),
                env: None,
                params: serde_json::Value::Null,
            },
            request_channel: airc_core::RoomId::from_uuid(Uuid::new_v4()),
            request_channel_name: None,
            presented_grant: None,
        };
        let baseline = leased_in_calls();

        // a generate: counted for the whole dispatch — the gate, which runs inside, sees it
        let _ = CommandRequestHandler::process_request_via(&executor, &envelope("ai/generate")).await;
        let at_gate = seen.lock().unwrap().last().map(|(_, n)| *n).expect("the gate ran");
        assert!(
            at_gate >= baseline + 1,
            "an inbound generate must be counted while it runs: gate saw {at_gate}, baseline {baseline}"
        );
        assert_eq!(
            leased_in_calls(),
            baseline,
            "…and released on exit — a finished generate is not standing demand"
        );

        // not a generate: a peer's work/list is not inference and must not count
        let _ = CommandRequestHandler::process_request_via(&executor, &envelope("work/list")).await;
        let at_gate = seen.lock().unwrap().last().map(|(_, n)| *n).expect("the gate ran");
        assert_eq!(
            at_gate, baseline,
            "a non-generate peer command must not inflate the seat's slot count"
        );
    }
    /// Reviewer 1 nit: prove the handler refuses non-Json
    /// CommandResult shapes (Handle / Stream / Lambda) cleanly
    /// rather than silently coercing or panicking. Locks the
    /// behavior so a future refactor can't start sending HandleRef
    /// over the wire without a deliberate decision.
    #[tokio::test]
    async fn process_request_refuses_non_json_command_result() {
        use crate::runtime::{CommandResult, HandleRef};
        use async_trait::async_trait;
        use std::any::Any;

        // Module that returns a Handle result. The handler must
        // refuse to wire it across.
        struct HandleReturningModule;
        impl HandleReturningModule {
            const PREFIXES: &'static [&'static str] = &["handle-test/"];
        }
        #[async_trait]
        impl crate::runtime::ServiceModule for HandleReturningModule {
            fn config(&self) -> crate::runtime::ModuleConfig {
                crate::runtime::ModuleConfig {
                    name: "handle-test",
                    priority: crate::runtime::ModulePriority::Normal,
                    command_prefixes: Self::PREFIXES,
                    event_subscriptions: &[],
                    needs_dedicated_thread: false,
                    max_concurrency: 0,
                    tick_interval: None,
                }
            }
            async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
                Ok(())
            }
            async fn handle_command(
                &self,
                _command: &str,
                _params: serde_json::Value,
            ) -> Result<CommandResult, String> {
                Ok(CommandResult::Handle(HandleRef::with_id(
                    "handle-test",
                    Uuid::new_v4(),
                    "handle-test::TestHandle",
                )))
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        registry.register(Arc::new(HandleReturningModule));
        let executor = CommandExecutor::new(registry);

        let parsed = make_parsed(AircCommandRequest {
            path: "handle-test/mint".into(),
            kind: "peer".into(),
            env: None,
            params: serde_json::Value::Null,
        });

        let response = CommandRequestHandler::process_request_via(&executor, &parsed).await;
        match response {
            AircCommandResponse::Error { message } => {
                assert!(
                    message.contains("non-Json"),
                    "error must name the non-Json variant, got: {message}"
                );
                assert!(
                    message.contains("Handle"),
                    "error should mention the cell shape, got: {message}"
                );
            }
            AircCommandResponse::Ok { .. } => {
                panic!("Handle result must not silently wire across as Ok");
            }
        }
    }
