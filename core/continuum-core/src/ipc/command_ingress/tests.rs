#![cfg(test)]
//! The command receiver over the core's own seams: a fake addressed inbox, a fake port
//! and a fixed tier source, so the ingress contract and dispatch run without a daemon.
//! The tests that need two real airc peers live in `tests/over_airc.rs`.

use super::*;
use crate::routing::addressed_inbox::fixtures::{FakeFrame, FakeInbox, FakePort, Sent};
use crate::routing::addressed_inbox::{AddressedInbox, AddressedKind};
use airc_lib::TrustTier;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc::UnboundedReceiver;

mod over_airc;
mod required_metadata;

/// Tiers as this node's trust store would report them.
struct Tiers(HashMap<PeerId, TrustTier>);

#[async_trait]
impl PeerTierSource for Tiers {
    async fn tier_of(&self, peer: PeerId) -> Result<Option<TrustTier>, String> {
        Ok(self.0.get(&peer).copied())
    }
}

/// Echoes the command it ran and counts runs, so a test can say whether anything executed.
struct Echo {
    runs: Arc<AtomicUsize>,
}

impl Echo {
    const PREFIXES: &'static [&'static str] = &[
        "persona/placement/reserve",
        "ai/generate",
        "test/echo",
        "handle-test/",
    ];
}

#[async_trait]
impl crate::runtime::ServiceModule for Echo {
    fn config(&self) -> crate::runtime::ModuleConfig {
        crate::runtime::ModuleConfig {
            name: "ingress-echo",
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
    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        if command.starts_with("handle-test/") {
            return Ok(CommandResult::Handle(crate::runtime::HandleRef::with_id(
                "handle-test",
                Uuid::new_v4(),
                "handle-test::TestHandle",
            )));
        }
        Ok(CommandResult::Json(
            json!({ "command": command, "params": params }),
        ))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn allow_all() -> Arc<dyn crate::routing::AuthPolicy> {
    Arc::new(crate::routing::ClosurePolicy::new(
        "allow-all",
        |_decision: &crate::routing::RouteDecision, _caller: Option<&CallerIdentity>| {
            crate::routing::Verdict::Allowed
        },
    ))
}

struct Rig {
    me: PeerId,
    handler: Arc<CommandRequestHandler>,
    sent: UnboundedReceiver<Sent>,
    runs: Arc<AtomicUsize>,
}

/// A receiver for `me` whose trust store holds `tiers`, over an executor running [`Echo`].
fn rig(tiers: &[(PeerId, TrustTier)]) -> Rig {
    let me = PeerId::new();
    let (port, sent) = FakePort::new(me);
    let runs = Arc::new(AtomicUsize::new(0));
    let registry = Arc::new(crate::runtime::ModuleRegistry::new());
    registry.register(Arc::new(Echo {
        runs: Arc::clone(&runs),
    }));
    let executor = Arc::new(CommandExecutor::new(registry).with_policy(allow_all()));
    let tiers = Arc::new(Tiers(tiers.iter().copied().collect()));
    let handler = CommandRequestHandler::with_seams(port, tiers, executor, None);
    Rig {
        me,
        handler,
        sent,
        runs,
    }
}

/// Feed `frames` through a fake inbox and drive the receiver until it drains: the
/// pump's loop, without a daemon.
async fn deliver(handler: &Arc<CommandRequestHandler>, frames: Vec<FakeFrame>) {
    let (tx, mut inbox) = FakeInbox::channel();
    for frame in frames {
        let _ = tx.send(Arc::new(frame)); // the receiver is held right here
    }
    drop(tx);
    while let Ok(frame) = inbox.next().await {
        handler.on_addressed(frame).await;
    }
}

fn response(sent: &Sent) -> AircCommandResponse {
    let Body::Json(value) = &sent.body else {
        panic!("a command reply is JSON: {sent:?}");
    };
    serde_json::from_value(value.clone()).expect("a typed AircCommandResponse")
}

fn refused_by(sent: &Sent, rule: &str) -> bool {
    matches!(response(sent), AircCommandResponse::Error { message, .. } if message.contains(rule))
}

// what this catches: the 2026-10-30 incident's caller. An `untrusted` sender's command
// addressed to this node (the M5 ran `persona/placement/reserve` for e9093c03) is refused
// by rule, answered with the rule and never the params, and nothing executes.
#[tokio::test]
async fn an_untrusted_sender_is_refused_and_nothing_runs() {
    let stranger = PeerId::new();
    let mut rig = rig(&[(stranger, TrustTier::Untrusted)]);
    let reserve = "persona/placement/reserve";
    let frame = FakeFrame::command(
        stranger,
        rig.me,
        Some(reserve),
        reserve,
        json!({ "prompt": "her whole prompt" }),
    );
    deliver(&rig.handler, vec![frame]).await;
    let sent = rig
        .sent
        .try_recv()
        .expect("a command to this node is answered");
    assert!(refused_by(&sent, "tier_below_required"), "{sent:?}");
    assert!(
        !format!("{:?}", response(&sent)).contains("her whole prompt"),
        "the reply never echoes params"
    );
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0, "nothing executed");
}

// what this catches: a command for another peer answered (or run) by whoever received
// it: the 2026-09-06 loopback, where the sender answered its own request. It is not
// ours: no reply, nothing runs. A CommandResult in the inbox is not a request either.
#[tokio::test]
async fn a_frame_addressed_elsewhere_or_not_a_command_is_ignored() {
    let peer = PeerId::new();
    let mut rig = rig(&[(peer, TrustTier::OwnAccount)]);
    let elsewhere = FakeFrame::command(
        peer,
        PeerId::new(),
        Some("test/echo"),
        "test/echo",
        json!({}),
    );
    let mut result = FakeFrame::command(peer, rig.me, Some("test/echo"), "test/echo", json!({}));
    result.kind = AddressedKind::CommandResult;
    deliver(&rig.handler, vec![elsewhere, result]).await;
    assert!(
        rig.sent.try_recv().is_err(),
        "neither frame is ours to answer"
    );
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
}

// what this catches: the contract refusing our own nodes. An `own_account` sender's
// Command addressed here executes once and is answered Ok, bound to its correlation.
#[tokio::test]
async fn an_own_account_command_addressed_here_executes() {
    let node = PeerId::new();
    let mut rig = rig(&[(node, TrustTier::OwnAccount)]);
    let frame = FakeFrame::command(
        node,
        rig.me,
        Some("test/echo"),
        "test/echo",
        json!({ "n": 1 }),
    );
    let correlation = frame.correlation;
    deliver(&rig.handler, vec![frame]).await;
    let sent = rig.sent.try_recv().expect("answered");
    assert_eq!(
        sent.correlation, correlation,
        "the reply is bound to the request"
    );
    assert!(!sent.stream);
    assert!(
        matches!(response(&sent), AircCommandResponse::Ok { .. }),
        "{sent:?}"
    );
    assert_eq!(rig.runs.load(Ordering::SeqCst), 1);
}

// what this catches: a header that lies about the payload. Authority is decided on the
// `continuum.command.path` header before decoding, so a payload naming a different
// command, a missing header, or a payload that is not a request is refused by rule and
// never executes.
#[tokio::test]
async fn the_payload_must_name_the_command_the_header_was_judged_on() {
    let node = PeerId::new();
    let mut rig = rig(&[(node, TrustTier::OwnAccount)]);
    let lies = FakeFrame::command(
        node,
        rig.me,
        Some("test/echo"),
        "persona/placement/reserve",
        json!({}),
    );
    let unnamed = FakeFrame::command(node, rig.me, None, "test/echo", json!({}));
    let mut garbled = FakeFrame::command(node, rig.me, Some("test/echo"), "test/echo", json!({}));
    garbled.body = Body::Binary(vec![0xde, 0xad]);
    deliver(&rig.handler, vec![lies, unnamed, garbled]).await;
    for rule in ["path_mismatch", "missing_command_path", "malformed_request"] {
        let sent = rig
            .sent
            .try_recv()
            .expect("each is a command to this node, answered");
        assert!(refused_by(&sent, rule), "expected {rule}: {sent:?}");
    }
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
}

// what this catches: a command path back into the room stream. A command reaches the
// receiver ONLY through the addressed inbox: the room path (`inbound_attach`) names no
// command receiver, the pump reads no room subscription, and the receiver takes only
// verified addressed frames. Before 2026-10-30 a header string on a room event made it a
// command and the same event perception.
#[test]
fn nothing_from_a_room_subscription_reaches_command_ingress() {
    fn production(source: &str) -> &str {
        source.split("#[cfg(test)]").next().unwrap_or(source) // unwrap_or: a file with no test half is all production
    }
    let room_path = production(include_str!("../../airc/inbound_attach.rs"));
    for receiver in [
        "command_ingress",
        "CommandRequestHandler",
        "on_addressed",
        "AddressedHandler",
    ] {
        assert!(
            !room_path.contains(receiver),
            "the room path names a command receiver: {receiver}"
        );
    }
    let pump = production(include_str!(
        "../../../../continuum-mind/src/persona/command_inbound_pump.rs"
    ));
    for room in ["subscribe", "TranscriptEvent", "body_hint"] {
        assert!(
            !pump.contains(room),
            "the command pump reads a room stream: {room}"
        );
    }
    let receiver = production(include_str!("../command_ingress.rs"));
    for room in ["TranscriptEvent", "body_hint", "ConsumerAdapter"] {
        assert!(
            !receiver.contains(room),
            "the command receiver takes room events: {room}"
        );
    }
}

/// An admitted command, bypassing the contract, for the dispatch-side tests.
fn admitted(path: &str, kind: &str, env: Option<&str>) -> AdmittedCommand {
    let mut frame = FakeFrame::command(PeerId::new(), PeerId::new(), Some(path), path, Value::Null);
    let request = AircCommandRequest::new(
        path.into(),
        kind.into(),
        env.map(str::to_string),
        Value::Null,
    );
    frame.body = Body::Json(serde_json::json!(request));
    AdmittedCommand {
        frame: Arc::new(frame),
        request,
        verified_capabilities: Vec::new(),
        generation: None,
    }
}

// what this catches (PR #1529 reviewers 1 + 2): a room / broadcast kind or an
// env-targeted request silently routed as Local. Each is refused by name.
#[tokio::test]
async fn room_broadcast_and_env_requests_are_refused_by_name() {
    let rig = rig(&[]);
    for (kind, env, needle) in [
        ("room", None, "kind=\"room\""),
        ("broadcast", None, "kind=\"broadcast\""),
        ("peer", Some("vr"), "env=\"vr\""),
    ] {
        let response = rig
            .handler
            .process_request(&admitted("test/echo", kind, env))
            .await;
        assert!(
            matches!(&response, AircCommandResponse::Error { message, .. } if message.contains(needle)),
            "{kind}/{env:?}: {response:?}"
        );
    }
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
}

// what this catches (PR #1529 reviewer 3): the verified sender not reaching the
// AuthPolicy gate; and (card c84d885a) an inbound generate performing inference on this
// seat WITHOUT the seat counting it as leased-in demand for the whole dispatch, or a
// non-generate inflating it. Read relative to a baseline (the #1960 flake class).
#[tokio::test]
async fn the_sender_reaches_the_gate_and_only_a_generate_counts_as_leased_in() {
    use crate::inference::resource_admission::leased_in_calls;
    use std::sync::Mutex;
    let seen: Arc<Mutex<Vec<(Option<CallerIdentity>, usize)>>> = Arc::default();
    let record = Arc::clone(&seen);
    let policy = crate::routing::ClosurePolicy::new(
        "record",
        move |_decision: &crate::routing::RouteDecision, caller: Option<&CallerIdentity>| {
            record
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push((caller.cloned(), leased_in_calls())); // a poisoned recorder still records
            crate::routing::Verdict::Allowed
        },
    );
    let (port, _sent) = FakePort::new(PeerId::new());
    let executor = Arc::new(
        CommandExecutor::new(Arc::new(crate::runtime::ModuleRegistry::new()))
            .with_policy(Arc::new(policy)),
    );
    let handler =
        CommandRequestHandler::with_seams(port, Arc::new(Tiers(HashMap::new())), executor, None);
    let baseline = leased_in_calls();

    let generate = admitted("ai/generate", "peer", None);
    let _ = handler.process_request(&generate).await;
    let (caller, at_gate) = seen
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .last()
        .cloned()
        .expect("the gate ran"); // poisoned: still read
    let caller = caller.expect("the gate saw Some(caller)");
    assert_eq!(
        caller.peer_id,
        generate.caller(),
        "the verified sender reached the gate"
    );
    assert!(matches!(caller.source, crate::routing::CallerSource::Airc));
    assert!(
        at_gate >= baseline + 1,
        "a generate counts while it runs: {at_gate} vs {baseline}"
    );
    assert_eq!(leased_in_calls(), baseline, "and is released on exit");

    let _ = handler
        .process_request(&admitted("work/list", "peer", None))
        .await;
    let (_, at_gate) = seen
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .last()
        .cloned()
        .expect("the gate ran"); // poisoned: still read
    assert_eq!(
        at_gate, baseline,
        "a non-generate does not inflate the seat's slot count"
    );
}

// what this catches (PR #1529 reviewer 1): a Handle / Stream / Lambda result silently
// coerced onto the wire. It is refused, naming the shape.
#[tokio::test]
async fn a_non_json_result_is_refused_not_coerced() {
    let rig = rig(&[]);
    let response = rig
        .handler
        .process_request(&admitted("handle-test/mint", "peer", None))
        .await;
    assert!(
        matches!(&response, AircCommandResponse::Error { message, .. } if message.contains("non-Json") && message.contains("Handle")),
        "{response:?}"
    );
}

#[tokio::test]
async fn expired_request_cannot_execute_an_immediately_ready_command() {
    let rig = rig(&[]);
    let mut command = admitted("test/echo", "peer", None);
    let mut frame = FakeFrame::command(
        PeerId::new(),
        rig.me,
        Some("test/echo"),
        "test/echo",
        json!({}),
    );
    frame.deadline = tokio::time::Instant::now();
    command.frame = Arc::new(frame);
    let response = rig.handler.process_request_streaming(&command).await;
    assert!(response.into_result().is_err());
    assert_eq!(
        rig.runs.load(Ordering::SeqCst),
        0,
        "expiry must precede executor polling"
    );
}
