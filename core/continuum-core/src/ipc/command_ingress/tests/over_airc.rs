#![cfg(test)]
//! The receiver over two real airc peers (`TwoAircLoopback`, both `own_account`): served
//! generations run as their own tasks, bounded, and are aborted when the pump stops or the
//! requester abandons them (card b3e5b582, slice 2a; card 83f774fb). Commands travel as
//! airc's addressed `Command` frames into the seat's addressed inbox.

use super::*;
use airc_core::MentionTarget;
use airc_test_fixtures::TwoAircLoopback;
use std::sync::Arc as StdArc;
use std::time::Duration;

const HOLD: &str = "hold the lane";
const WAIT: Duration = Duration::from_secs(60);

fn stamp_generation(headers: &mut airc_core::Headers) {
    headers.insert(
        continuum_airc_protocol::HEADER_GENERATE_ID.into(),
        Uuid::new_v4().to_string(),
    );
    headers.insert(
        continuum_airc_protocol::HEADER_GENERATE_VALID_UNTIL_MS.into(),
        (EpochMs::now().millis() + 60_000).to_string(),
    );
}

/// `ai/generate` whose params name [`HOLD`] signals `entered`, then waits for
/// `release` (a lane held); if its future is dropped first, `dropped` fires. Any
/// other request echoes its command and params at once.
struct Seat {
    entered: StdArc<tokio::sync::Notify>,
    release: StdArc<tokio::sync::Notify>,
    dropped: StdArc<tokio::sync::Notify>,
}
impl Seat {
    const PREFIXES: &'static [&'static str] = &["ai/generate", "test/echo"];
}
struct DropSignal(Option<StdArc<tokio::sync::Notify>>);
impl Drop for DropSignal {
    fn drop(&mut self) {
        if let Some(signal) = self.0.take() {
            signal.notify_one();
        }
    }
}
#[async_trait]
impl crate::runtime::ServiceModule for Seat {
    fn config(&self) -> crate::runtime::ModuleConfig {
        crate::runtime::ModuleConfig {
            name: "served-concurrently-seat",
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
        if command == "ai/generate" && params.to_string().contains(HOLD) {
            let mut signal = DropSignal(Some(StdArc::clone(&self.dropped)));
            self.entered.notify_one();
            if params.get("emit_progress").and_then(Value::as_bool) == Some(true) {
                let id =
                    crate::ai::stream_sinks::stream_id_of(&params).expect("registered stream id");
                let sink = crate::ai::stream_sinks::take(id).expect("registered sink");
                sink.send(crate::ai::adapter::GenerationChunk::Token(
                    "progress".into(),
                ))
                .expect("publisher owns receiver");
            }
            self.release.notified().await;
            signal.0 = None; // completed: not a drop
        }
        Ok(CommandResult::Json(
            serde_json::json!({ "command": command, "params": params }),
        ))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// An executor serving [`Seat`] with every command allowed.
fn seat_executor(
    entered: &StdArc<tokio::sync::Notify>,
    release: &StdArc<tokio::sync::Notify>,
    dropped: &StdArc<tokio::sync::Notify>,
) -> Arc<crate::runtime::CommandExecutor> {
    let registry = Arc::new(crate::runtime::ModuleRegistry::new());
    registry.register(Arc::new(Seat {
        entered: StdArc::clone(entered),
        release: StdArc::clone(release),
        dropped: StdArc::clone(dropped),
    }));
    let allow = crate::routing::ClosurePolicy::new(
        "allow-all",
        |_decision: &crate::routing::RouteDecision,
         _caller: Option<&crate::routing::CallerIdentity>| {
            crate::routing::Verdict::Allowed
        },
    );
    Arc::new(crate::runtime::CommandExecutor::new(registry).with_policy(StdArc::new(allow)))
}

struct Rig {
    loop_back: TwoAircLoopback,
    entered: StdArc<tokio::sync::Notify>,
    release: StdArc<tokio::sync::Notify>,
    dropped: StdArc<tokio::sync::Notify>,
    pump: tokio::task::JoinHandle<()>,
}

/// The seat (peer A) answers through the real handler with a generation bound of 1,
/// driven inline the way the command pump drives it.
async fn rig() -> Rig {
    let loop_back = TwoAircLoopback::new().await.expect("two-peer loopback");
    let entered = StdArc::new(tokio::sync::Notify::new());
    let release = StdArc::new(tokio::sync::Notify::new());
    let dropped = StdArc::new(tokio::sync::Notify::new());
    let executor = seat_executor(&entered, &release, &dropped);
    let seat = Arc::clone(loop_back.peer_a());
    let handler = CommandRequestHandler::with_generation_bound(
        CommandRequestHandler::new(Arc::clone(&seat), executor),
        || 1,
    );
    // The pump's loop over the seat's addressed inbox: the only way a command arrives.
    let mut inbox = seat
        .addressed_inbox(tokio::time::Instant::now() + WAIT)
        .await
        .expect("the seat opens its addressed inbox");
    let pump = tokio::spawn(async move {
        while let Ok(frame) = inbox.next().await {
            handler.on_addressed(frame).await;
        }
    });
    Rig {
        loop_back,
        entered,
        release,
        dropped,
        pump,
    }
}

/// Peer B asks the seat; `header_path` is what the header claims (None = absent),
/// `body_path` what the body executes. Returns the seat's typed answer.
async fn ask(
    loop_back: &TwoAircLoopback,
    header_path: Option<&str>,
    body_path: &str,
    params: Value,
) -> AircCommandResponse {
    let asker = loop_back.peer_b();
    let request = AircCommandRequest {
        path: body_path.into(),
        kind: "peer".into(),
        env: None,
        params,
    };
    let mut headers = airc_core::Headers::new();
    if let Some(path) = header_path {
        headers.insert(
            crate::routing::HEADER_COMMAND_PATH.to_string(),
            path.to_string(),
        );
    }
    headers.insert(
        crate::routing::HEADER_COMMAND_KIND.to_string(),
        "peer".to_string(),
    );
    let body = Body::Json(serde_json::to_value(&request).expect("serialize request"));
    if header_path == Some("ai/generate") {
        stamp_generation(&mut headers);
    }
    let pending = asker
        .request(
            MentionTarget::Peer(PeerId(loop_back.peer_a_id())),
            headers,
            body,
            WAIT,
        )
        .await
        .expect("the request leaves");
    let reply = tokio::time::timeout(WAIT, asker.await_reply(pending))
        .await
        .expect("the seat answers in time")
        .expect("a reply arrives");
    match reply.body() {
        Body::Json(value) => {
            serde_json::from_value(value.clone()).expect("a typed command response")
        }
        other => panic!("expected a JSON command response, got {other:?}"),
    }
}

// Typed on the wire (card 03660a77): the requester acts on the reason, not the text.
fn overloaded(response: &AircCommandResponse) -> bool {
    response.refusal() == Some(continuum_airc_protocol::GenerateRefusal::Overloaded)
}

// what this catches (card b3e5b582, IntelMac -> M5 2026-10-23): one served
// generation holding its lane used to stop the seat reading anything. Now, while it
// holds the only admissible slot: a control command is still answered, a second
// generation is refused AT ONCE with a typed overload (bounded custody, never an
// unbounded queue of waiting handlers), the held generation gets ITS OWN reply when
// released (correlation), and its slot is then free again.
#[tokio::test]
async fn a_held_generation_leaves_the_seat_answering_and_refuses_the_burst() {
    let rig = StdArc::new(rig().await);
    let task_rig = StdArc::clone(&rig);
    let held = tokio::spawn(async move {
        ask(
            &task_rig.loop_back,
            Some("ai/generate"),
            "ai/generate",
            serde_json::json!({ "prompt": HOLD }),
        )
        .await
    });
    tokio::time::timeout(WAIT, rig.entered.notified())
        .await
        .expect("the held generation reaches the seat");

    let control = ask(
        &rig.loop_back,
        Some("test/echo"),
        "test/echo",
        serde_json::json!({ "n": 1 }),
    )
    .await;
    assert!(
        matches!(&control, AircCommandResponse::Ok { .. }),
        "control answered: {control:?}"
    );
    let burst = ask(
        &rig.loop_back,
        Some("ai/generate"),
        "ai/generate",
        serde_json::json!({ "prompt": "second" }),
    )
    .await;
    assert!(
        overloaded(&burst),
        "the burst is refused at once: {burst:?}"
    );

    rig.release.notify_one();
    let held = held.await.expect("the held ask joins");
    match &held {
        AircCommandResponse::Ok { result } => {
            assert!(
                result.to_string().contains(HOLD),
                "the held generation got its own reply: {result}"
            )
        }
        other => panic!("the held generation must succeed once released: {other:?}"),
    }
    let after = ask(
        &rig.loop_back,
        Some("ai/generate"),
        "ai/generate",
        serde_json::json!({ "prompt": "after" }),
    )
    .await;
    assert!(
        matches!(&after, AircCommandResponse::Ok { .. }),
        "the freed slot admits again: {after:?}"
    );
    rig.pump.abort();
}

// what this catches: a stopped pump does not orphan the generation it was serving.
// Stopping the pump drops its handler, whose registry aborts the task, and the held
// generation's work is dropped rather than left running for no one.
#[tokio::test]
async fn stopping_the_pump_aborts_the_generation_it_was_serving() {
    let rig = StdArc::new(rig().await);
    let task_rig = StdArc::clone(&rig);
    let held = tokio::spawn(async move {
        ask(
            &task_rig.loop_back,
            Some("ai/generate"),
            "ai/generate",
            serde_json::json!({ "prompt": HOLD }),
        )
        .await
    });
    tokio::time::timeout(WAIT, rig.entered.notified())
        .await
        .expect("the held generation reaches the seat");
    rig.pump.abort();
    tokio::time::timeout(Duration::from_secs(10), rig.dropped.notified())
        .await
        .expect("the served generation's work is dropped when the pump stops");
    held.abort();
}

// what this catches (Codex re-review of 3cc27cbf5): the PUBLIC pump shutdown aborted
// only its supervisor, which awaited a bare JoinHandle for the inner `run` task;
// dropping that handle detached the loop, which kept reading and kept its handler and
// registry, so the served generation ran on for no one. Real PersonaCommandInboundPump
// spawn and shutdown, with the membership sender kept alive (a live runtime).
#[tokio::test]
async fn the_public_pump_shutdown_aborts_the_generation_it_was_serving() {
    let loop_back = StdArc::new(TwoAircLoopback::new().await.expect("two-peer loopback"));
    let entered = StdArc::new(tokio::sync::Notify::new());
    let release = StdArc::new(tokio::sync::Notify::new());
    let dropped = StdArc::new(tokio::sync::Notify::new());
    let executor = seat_executor(&entered, &release, &dropped);
    let home = tempfile::tempdir().expect("tempdir for the grant watermark");
    let grants = crate::persona::command_inbound_pump::build_grant_authorizer(
        loop_back.peer_a(),
        home.path(),
    )
    .await
    .expect("grant authorizer for the loopback seat");
    crate::persona::command_inbound_pump::install_command_handler(|airc, executor, grants| {
        CommandRequestHandler::with_grant_authorizer(airc, executor, grants)
    });
    let (membership_tx, membership_rx) = tokio::sync::watch::channel(0u64);
    let pump = crate::persona::command_inbound_pump::PersonaCommandInboundPump::spawn(
        loop_back.peer_a_id(),
        Arc::clone(loop_back.peer_a()),
        executor,
        grants,
        membership_rx,
    )
    .await
    .expect("the public pump spawns");

    let asker = StdArc::clone(&loop_back);
    let held = tokio::spawn(async move {
        ask(
            &asker,
            Some("ai/generate"),
            "ai/generate",
            serde_json::json!({ "prompt": HOLD }),
        )
        .await
    });
    tokio::time::timeout(WAIT, entered.notified())
        .await
        .expect("the held generation reaches the seat");
    pump.shutdown().await;
    tokio::time::timeout(Duration::from_secs(10), dropped.notified())
        .await
        .expect("shutting the public pump down drops the generation it was serving");
    held.abort();
    drop(membership_tx);
}

/// Peer B asks the seat for a held generation through the real requester transport
/// with `deadline`, and returns the transport's outcome once it gives up.
async fn abandon_a_held_generation(
    rig: &Rig,
    deadline: Duration,
) -> Result<
    crate::inference::airc_remote::protocol::RemoteInferenceResponse,
    crate::inference::airc_remote::protocol::RemoteInferenceError,
> {
    use crate::ai::types::{ChatMessage, MessageContent, TextGenerationRequest};
    use crate::inference::airc_remote::transport::{AircInferenceTransport, AircLiveTransport};
    let transport = Arc::into_inner(AircLiveTransport::new(
        Arc::clone(rig.loop_back.peer_b()),
        rig.loop_back.peer_a_id(),
    ))
    .expect("a freshly built transport has one owner")
    .with_deadline(deadline);
    let request = crate::inference::airc_remote::protocol::RemoteInferenceRequest::new(
        TextGenerationRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(HOLD.to_string()),
                name: None,
            }],
            ..Default::default()
        },
    );
    transport.send_request(request).await
}

/// The seat's beacon as the requester hears it, advertising `capabilities`.
fn hear_seat(rig: &Rig, capabilities: Value) {
    let offer: crate::capacity::gossip::CapacityOffer = serde_json::from_value(serde_json::json!({
        "gpuTotalBytes": 1u64, "gpuFreeBytesLive": 0u64, "systemRamFreeBytes": 0u64,
        "atMs": continuum_base::time::EpochMs::now().millis(),
        "capabilities": capabilities,
    }))
    .expect("a seat beacon");
    crate::capacity::gossip::global_ledger().hear(
        rig.loop_back.peer_a_id(),
        offer,
        continuum_base::time::EpochMs::now().millis(),
    );
}

// what this catches (card 83f774fb, IntelMac Iris 2026-10-23: the M5 finished
// 2,520 tokens ~55 s after she gave up): a requester that abandons a generation at its
// deadline tells a capable seat to stop, and the seat drops the work instead of
// finishing it for no one. Real requester transport, real seat handler.
#[tokio::test]
async fn an_abandoned_generation_is_cancelled_at_a_capable_seat() {
    let rig = StdArc::new(rig().await);
    hear_seat(&rig, serde_json::json!(["remote_generate_cancel"]));
    // Establish the addressed session before measuring the generation deadline.
    let ready = ask(&rig.loop_back, Some("test/echo"), "test/echo", json!({})).await;
    assert!(matches!(ready, AircCommandResponse::Ok { .. }));
    let task_rig = StdArc::clone(&rig);
    let requester =
        tokio::spawn(
            async move { abandon_a_held_generation(&task_rig, Duration::from_secs(10)).await },
        );
    tokio::time::timeout(WAIT, rig.entered.notified())
        .await
        .expect("deadline test must first observe a running generation");
    let outcome = requester.await.expect("deadline requester joins");
    assert!(
        outcome.is_err(),
        "the requester gave up at its deadline: {outcome:?}"
    );
    tokio::time::timeout(Duration::from_secs(15), rig.dropped.notified())
        .await
        .expect("the seat aborted the abandoned generation");
    rig.pump.abort();
}

// Cancellation is a mandatory command contract, independent of beacon advertisement.
// A long retained deadline cannot hide a missing cancellation when its caller drops.
#[tokio::test]
async fn dropping_a_request_cancels_even_without_a_capability_beacon() {
    let rig = StdArc::new(rig().await);
    let task_rig = StdArc::clone(&rig);
    let requester = tokio::spawn(async move {
        abandon_a_held_generation(&task_rig, Duration::from_secs(600)).await
    });
    tokio::time::timeout(WAIT, rig.entered.notified())
        .await
        .expect("the seat is serving the request");
    requester.abort();
    let _ = requester.await;
    tokio::time::timeout(Duration::from_secs(15), rig.dropped.notified())
        .await
        .expect("mandatory cancellation drops work without a capability beacon");
    rig.pump.abort();
}

// what this catches (Codex BLOCK on 1ed901a74): the guard armed only after
// request_in returned, but airc publishes before that, so a turn dropped while its
// request was being published left the seat generating for no one. The guard is now
// armed before publication, keyed by the requester's own generation id: dropping the
// requesting turn while the seat is already serving it still cancels the work.
#[tokio::test]
async fn dropping_the_requesting_turn_cancels_at_a_capable_seat() {
    let rig = StdArc::new(rig().await);
    hear_seat(&rig, serde_json::json!(["remote_generate_cancel"]));
    let task_rig = StdArc::clone(&rig);
    let requester = tokio::spawn(async move {
        abandon_a_held_generation(&task_rig, Duration::from_secs(600)).await
    });
    tokio::time::timeout(WAIT, rig.entered.notified())
        .await
        .expect("the seat is serving the request");
    requester.abort();
    tokio::time::timeout(Duration::from_secs(15), rig.dropped.notified())
        .await
        .expect("dropping the requesting turn cancelled the served generation");
    rig.pump.abort();
}

// A broken private progress route must cancel a lane still generating, not wait for
// it to finish and then send success despite losing its chunks.
#[tokio::test]
async fn progress_publish_failure_drops_the_running_generation() {
    let entered = StdArc::new(tokio::sync::Notify::new());
    let release = StdArc::new(tokio::sync::Notify::new());
    let dropped = StdArc::new(tokio::sync::Notify::new());
    let executor = seat_executor(&entered, &release, &dropped);
    let (port, sent) = FakePort::new(PeerId::new());
    drop(sent);
    let tiers = Arc::new(Tiers(HashMap::new()));
    let handler = CommandRequestHandler::with_seams(port, tiers, executor, None);
    let mut command = admitted("ai/generate", "peer", None);
    command.request.params = json!({"prompt": HOLD, "emit_progress": true});
    let response = tokio::time::timeout(WAIT, handler.process_request_streaming(&command))
        .await
        .expect("failed stream terminates without releasing the held command");
    assert!(response.into_result().is_err());
    tokio::time::timeout(WAIT, dropped.notified())
        .await
        .expect("generation future dropped");
}

#[tokio::test]
async fn silent_generation_expires_and_releases_admission_without_cancel_traffic() {
    let entered = StdArc::new(tokio::sync::Notify::new());
    let release = StdArc::new(tokio::sync::Notify::new());
    let dropped = StdArc::new(tokio::sync::Notify::new());
    let executor = seat_executor(&entered, &release, &dropped);
    let me = PeerId::new();
    let caller = PeerId::new();
    let (port, _sent) = FakePort::new(me);
    let tiers = Arc::new(Tiers(HashMap::from([(caller, TrustTier::OwnAccount)])));
    let handler = CommandRequestHandler::with_generation_bound(
        CommandRequestHandler::with_seams(port, tiers, executor, None),
        || 1,
    );
    let mut frame = FakeFrame::command(
        caller,
        me,
        Some("ai/generate"),
        "ai/generate",
        json!({"prompt": HOLD}),
    );
    frame.deadline = tokio::time::Instant::now() + Duration::from_millis(250);
    stamp_generation(&mut frame.headers);
    handler.on_addressed(Arc::new(frame)).await;
    tokio::time::timeout(WAIT, entered.notified())
        .await
        .expect("generation entered");
    tokio::time::timeout(WAIT, dropped.notified())
        .await
        .expect("silent generation cancelled at expiry");
    let generations = handler.generations.as_ref().expect("bounded owner");
    tokio::time::timeout(WAIT, async {
        while generations.outstanding() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("expired generation released admission");
}
