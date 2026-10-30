//! Inbound daemon attach stream for Continuum's event bus.
//!
//! This is the runtime half of AIRC realtime integration: the daemon owns
//! transport, trust, replay, and live delivery; Continuum subscribes through
//! typed IPC and republishes valid EventBridge envelopes into MessageBus.

use continuum_base::time::EpochMs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use airc_core::{RoomId, TranscriptKind};
use airc_ipc::{codec::read_frame, AttachRequest, AttachStart, DaemonClient, IpcCursor, Response};
use airc_lib::decode_wire_event;
use tracing::warn;

use crate::routing::airc_realtime::AircRealtimeEnvelope;
use crate::cognition::conversation::room_event::chat_transcript_message;
use crate::routing::airc_realtime_wire::{bus_event_from_envelope, envelope_from_event};
use crate::ipc::positron_kanban_source::KANBAN_CHANGED;
use crate::ipc::positron_source::CHAT_POSTED;
use crate::ipc::positron_wall_source::WALL_CHANGED;
use crate::runtime::MessageBus;

/// One attach stream per ROOM, spawned at most once — the registry the room
/// ADOPTION path (positron_presence's refresher, #2606) calls so every room
/// the node serves also bridges its transcript onto the projection bus.
/// Before this, exactly ONE channel (the bootstrap room) was attached: acts
/// radiated into solve rooms (`room.work_receipt.published` ×67 on the night
/// this was found) and no bridge ever carried them — every activity
/// transcript rendered empty while the work streamed (Joel, 2026-08-31:
/// "we should see their chats as a team, acts in the center transcript").
pub struct AttachRegistry {
    socket: PathBuf,
    bus: Arc<MessageBus>,
    rt: tokio::runtime::Handle,
    attached: std::sync::Mutex<std::collections::HashSet<RoomId>>,
}

static ATTACHES: std::sync::OnceLock<Arc<AttachRegistry>> = std::sync::OnceLock::new();

impl AttachRegistry {
    /// Install the process-wide registry (AircModule::initialize owns this).
    /// Idempotent; the first install wins.
    pub fn install(socket: PathBuf, bus: Arc<MessageBus>, rt: tokio::runtime::Handle) -> Arc<Self> {
        // A mind's perception feed attaches the rooms her region holds through this registry.
        crate::persona::perception_feed::install_room_attacher(|room| {
            if let Some(registry) = AttachRegistry::try_global() {
                registry.ensure_attached(room);
            }
        });
        ATTACHES
            .get_or_init(|| {
                Arc::new(Self {
                    socket,
                    bus,
                    rt,
                    attached: std::sync::Mutex::new(std::collections::HashSet::new()),
                })
            })
            .clone()
    }

    pub fn try_global() -> Option<Arc<Self>> {
        ATTACHES.get().cloned()
    }

    /// Bridge `room`'s transcript onto the bus — at most one stream per room.
    pub fn ensure_attached(&self, room: RoomId) {
        let fresh = self
            .attached
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(room);
        if fresh {
            continuum_base::probe!(
                class = "airc.attach.room_bridged",
                room = %room.as_uuid(),
                "daemon attach spawned for an adopted room — its transcript now reaches the projection bus"
            );
            spawn_daemon_attach(self.socket.clone(), room, self.bus.clone(), &self.rt);
        }
    }
}

pub fn spawn_daemon_attach(
    socket_path: PathBuf,
    channel: RoomId,
    bus: Arc<MessageBus>,
    runtime: &tokio::runtime::Handle,
) {
    // RE-ATTACH loop. This is a RAW `DaemonClient::attach` realtime-transcript
    // stream (positron chat/wall/kanban + the persona-turn projection), NOT
    // airc-lib's reconnecting `subscribe()` — so airc-lib's transport recovery
    // does NOT cover it ([[persona-airc-resilience]] is about the subscribe path).
    // Before this loop it ran EXACTLY ONCE: on any daemon restart / stream drop
    // the task ended (Err → warn, clean EOF → SILENT) and the persona went
    // permanently DEAF to inbound airc events until continuum-core rebooted
    // (reliability audit #5). Re-attach with capped backoff so a daemon restart
    // self-heals. There is no deliberately-loud terminal signal to mask here —
    // malformed events are already skipped (handle_attach_response), and a raw
    // attach drop is a transport event, not a wire-schema fault.
    runtime.spawn(async move {
        const MIN_BACKOFF: Duration = Duration::from_secs(1);
        const MAX_BACKOFF: Duration = Duration::from_secs(30);
        // An attach that streamed for at least this long before dropping was
        // healthy → reset backoff so a brief blip doesn't ratchet the delay.
        const HEALTHY_ATTACH: Duration = Duration::from_secs(30);
        let mut backoff = MIN_BACKOFF;
        loop {
            let started = Instant::now();
            match run_daemon_attach(socket_path.clone(), channel, bus.clone()).await {
                Ok(()) => warn!(
                    "AIRC daemon attach ended (daemon EOF) after {:?} — re-attaching in {:?}",
                    started.elapsed(),
                    backoff
                ),
                Err(error) => warn!(
                    "AIRC daemon attach stopped: {error} (after {:?}) — re-attaching in {:?}",
                    started.elapsed(),
                    backoff
                ),
            }
            // Reset backoff if the prior attach was long-lived (a healthy session
            // that dropped), else escalate up to the cap.
            if started.elapsed() >= HEALTHY_ATTACH {
                backoff = MIN_BACKOFF;
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    });
}

/// Where the per-channel attach watermark persists — the CONSUMER CURSOR
/// (task #242). One tiny JSON file per channel under the user's continuum
/// state dir: the [`IpcCursor`] this process has caught up to, threaded back
/// as [`AttachStart::After`] on the next attach so a core reboot resumes at
/// the gap instead of replaying the room's entire history into the UI and
/// every persona's perception (glass-boxed three times on 2026-07-30 — each
/// reboot re-fed days of transcript as fresh inbox to every mind).
///
/// Under the canonical continuum home, never a cwd-relative `.`: on the 5090
/// (2026-10-06) the Windows service had no `HOME`, the old `unwrap_or(".")`
/// resolved `.\.continuum\state` against an unwritable working directory, and
/// 592 persists failed "Access is denied", so every core restart re-fed the
/// room into the desktop, the human views and every persona as if it were new.
fn cursor_path(channel: &RoomId) -> Result<PathBuf, String> {
    Ok(crate::paths::continuum_home()?
        .join("state")
        .join(format!("airc-attach-cursor-{}.json", channel.as_uuid())))
}

fn load_cursor(channel: &RoomId) -> Option<IpcCursor> {
    let path = match cursor_path(channel) {
        Ok(path) => path,
        Err(error) => {
            warn!("airc attach cursor has no home ({error}): this attach starts as a first attach");
            return None;
        }
    };
    let raw = std::fs::read_to_string(path).ok()?;
    // A corrupt watermark degrades to first-attach semantics (one full seed) —
    // annoying, never wrong. It is presentation-adjacent state, not truth: the
    // durable transcript is the storage of record either way.
    serde_json::from_str(&raw).ok()
}

fn persist_cursor(channel: &RoomId, cursor: &IpcCursor) {
    let path = match cursor_path(channel) {
        Ok(path) => path,
        Err(error) => {
            warn!("failed to persist airc attach cursor: no continuum home ({error})");
            return;
        }
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string(cursor) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&path, json) {
                warn!(
                    "failed to persist airc attach cursor to {}: {error}",
                    path.display()
                );
            }
        }
        Err(error) => warn!("failed to serialize airc attach cursor: {error}"),
    }
}

/// One page of recent room history on a first-ever attach (Joel, 2026-08-01:
/// "When you join a Discord channel do you read the whole history from 10
/// years back? No — one page"). The daemon streams the newest
/// `FIRST_ATTACH_PAGE` backlog events at the catch-up seam and coalesces
/// everything older into the watermark summary (airc PR #1312). Ten, not a
/// screenful (Joel, 2026-10-06: "no bookmark should also default to like last N
/// being like 10"); with the bookmark persisting, a first attach is rare.
const FIRST_ATTACH_PAGE: u32 = 10;

/// Build the attach request for this consumer's cursor state (#295).
///
/// A persisted watermark resumes strictly after it, streaming the gap
/// event-by-event (those events were live while we were down — the
/// transcript writer and perception both want them). No watermark means
/// first-ever attach: start from the transcript head for cursor
/// correctness, coalesce the deep backlog into one summary frame so history
/// never floods the bus, the personas, or the CPU — but keep one page of
/// recent tail so the room isn't a void on join.
///
/// BOTH arms ASK for the daemon's cursor heartbeat. This consumer persists its
/// watermark and (per the WATERMARK DISCIPLINE below) the heartbeat is its SOLE
/// advance source — airc #1417 made that frame opt-in per attach to end the WARN
/// floods on airc-lib's subscribe streams (cards c5466b56 / e28889cc), and this
/// request never opted in. From that afternoon (2026-09-14 16:27) every cursor
/// file under `~/.continuum/state/` froze, and every core boot on every node
/// resumed each room from a week-old watermark: 7,700 events of the fleet room
/// replayed in three minutes on the IntelMac at 18:15Z on 2026-09-21, feeding the
/// bus, the chat projection and the capacity ledger (whose fold then judged three
/// nodes "behind" from beacons published days earlier). Card ac4bca50.
fn attach_request_for_cursor(channel: RoomId, cursor: Option<IpcCursor>) -> AttachRequest {
    match cursor {
        Some(cursor) => {
            AttachRequest::new(channel, AttachStart::After(cursor)).with_cursor_heartbeat()
        }
        None => AttachRequest::new(channel, AttachStart::FromTranscriptStart)
            .with_coalesced_backlog()
            .with_backlog_tail(FIRST_ATTACH_PAGE)
            .with_cursor_heartbeat(),
    }
}

pub async fn run_daemon_attach(
    socket_path: PathBuf,
    channel: RoomId,
    bus: Arc<MessageBus>,
) -> Result<(), String> {
    let client = DaemonClient::new(socket_path.clone());
    // Owner-core model (airc-daemon/src/server.rs:274): the router
    // subscribes per channel — no global fan-out table. AttachRequest
    // MUST carry `channel: Some(_)` or the daemon responds
    // `attach requires a channel in the owner-core model`. continuum
    // discovers the scope's default channel at boot via
    // `crate::airc::discover_default_channel` (parses `airc room`).
    // Multi-room scopes will spawn one daemon_attach task per channel
    // they care about — single-attach today, per-room fan-out as a
    // follow-up when continuum rooms become first-class.
    //
    // CONSUMER CURSOR (#242): consumers resume from durable cursors over
    // durable storage, never from zero. A persisted watermark resumes
    // strictly after it (gap replay only, no duplicates at the seam —
    // the AttachStart::After contract).
    //
    // BOUNDED FIRST ATTACH (#295): the FIRST-EVER attach (no watermark on
    // disk) must NOT stream the room's full history into live perception —
    // on a long-lived room that is thousands of events decoded, published,
    // and serviced as if they were happening now (BigMama's Windows node
    // pegged 24 cores for ~45min on exactly this). AttachStart's own doc
    // marks FromTranscriptStart "audit/replay tools only". Instead the
    // first attach coalesces the backlog daemon-side (airc card 7d5b6a65)
    // into ONE AttachCursorAdvanced summary frame at the live seam; the
    // handler below persists it, so every later attach resumes After(_).
    // The room starts at join for a fresh node — personas perceive the
    // room AS IT IS NOW (#131), and deep history is #249's bounded
    // durable-transcript hydration, never a perception replay.
    let request = attach_request_for_cursor(channel, load_cursor(&channel));
    let mut stream = client
        .attach(request)
        .await
        .map_err(|error| format!("failed to attach to airc daemon: {error}"))?;

    // WATERMARK DISCIPLINE (#261, both PR #2057 review findings): the attach
    // NEVER pre-persists the room tip. The old tip-probe persisted a cursor
    // for events not yet processed — a daemon Error frame or transport read
    // error mid-backlog then resumed PAST the tip, permanently SKIPPING the
    // unprocessed remainder from live perception (the one failure worse than
    // redelivery). The daemon's cursor HEARTBEAT (airc 9390c32e8) is now the
    // sole advance source: an `AttachCursorAdvanced` frame rides live
    // streaming at most once per second, always pointing at an ALREADY
    // DELIVERED event — so the persisted watermark can only ever resume
    // at-or-before what this consumer actually processed. No skip, and the
    // redelivery window shrinks from the whole session to ≤1s of events.

    loop {
        let response = read_frame::<_, Response>(&mut stream)
            .await
            .map_err(|error| format!("failed to read airc daemon event: {error}"))?;
        let Some(response) = response else {
            return Ok(());
        };
        // The daemon's cursor-advance frames (seam summary + the 1/s live
        // HEARTBEAT, airc 9390c32e8) are the authoritative watermark. The
        // probe makes every advance VISIBLE — 2026-07-30's verification found
        // a silently-stale watermark with no way to tell whether frames never
        // arrived or the persist failed quietly ([[glass-box]]: success needs
        // receipts too, not only failures).
        if let Response::AttachCursorAdvanced { advanced_to, .. } = &response {
            tracing::info!(
                room = %channel,
                epoch = advanced_to.epoch,
                counter = advanced_to.counter,
                probe_class = "airc.cursor.advanced",
                "attach watermark advanced (daemon heartbeat) — persisting"
            );
            persist_cursor(&channel, advanced_to);
        }
        handle_attach_response(response, &bus).await?;
    }
}

pub async fn handle_attach_response(response: Response, bus: &MessageBus) -> Result<(), String> {
    match response {
        Response::Ok => Ok(()),
        // v5 owner-core schema (task #82): the daemon now streams raw
        // airc-wire envelope bytes; `airc_lib::decode_wire_event` is
        // the canonical helper that decodes + projects to a
        // TranscriptEvent. A malformed buffer is logged + skipped (the
        // live stream shouldn't die because one event failed to parse).
        Response::Event { envelope } => match decode_wire_event(envelope) {
            Ok(event) => publish_transcript_event(&Arc::new(event), bus).await,
            Err(error) => {
                warn!("Skipping malformed airc daemon event: {error}");
                Ok(())
            }
        },
        Response::Error { message } => Err(message),
        // Wildcard for non-event responses the daemon may emit on the
        // attach stream (Pong, Status, Inbox, Publish, Peers, cursor
        // advances, future variants). v5 dropped ResolveWire; future
        // variants come/go on the airc side without breaking continuum
        // — same `non_exhaustive`-style posture the airc-cli monitor
        // uses against the same enum.
        _ => Ok(()),
    }
}

pub async fn publish_transcript_event(
    event: &Arc<airc_core::TranscriptEvent>,
    bus: &MessageBus,
) -> Result<(), String> {
    // THE ONE INBOUND DECODE POINT for room events: the transport kind and headers
    // decide protocol frame or conversation, before anything reads the payload. The
    // room path has NO command handling: commands arrive only through the identity's
    // addressed inbox (`routing::addressed_inbox`), and a protocol frame that surfaces
    // here never reaches a mind, the chat projection or the bus.
    // 2026-10-30 00:28Z: an untrusted peer's `persona/placement/reserve` requests
    // published into #general were fed to two residents as room speech by this seam.
    let conversation = match continuum_airc_protocol::classify_inbound(Arc::clone(event)) {
        continuum_airc_protocol::InboundEvent::Conversation(conversation) => conversation,
        continuum_airc_protocol::InboundEvent::Protocol(envelope) => {
            continuum_base::probe!(
                class = "airc.inbound.protocol_not_perceived",
                room = %envelope.channel().as_uuid(),
                from = %envelope.sender().as_uuid(),
                kind = ?envelope.kind(),
                addressing = ?envelope.addressing(),
                "a protocol frame crossed the room seam; routed by header away from perception"
            );
            return Ok(());
        }
    };
    // ONE FEED (EVENT-MIND.md §1b, 2026-10-04): every resident persona's perception
    // region is fed from this seam, the one place each room event crosses once per
    // core, instead of from a per-persona subscription (which on the 5090 never
    // carried other peers' durable pushes while this path did). Synchronous and
    // cheap: one classification, one lock per resident, no await.
    let fed = crate::persona::perception_feed::feed(&conversation, EpochMs::now().millis());
    if fed > 0 {
        continuum_base::probe!(
            class = "mind.feed.event",
            room = %event.room_id.as_uuid(),
            from = %event.peer_id.as_uuid(),
            residents = fed,
            "a room event fed to resident minds"
        );
    }
    // The room's verdict settles staged credit in `reviewed::bridge_review`, AFTER the
    // review is verified (signed by its author, accepted in the room's projection,
    // admitted by the room's self-review policy). It used to settle here, on any
    // review-shaped event, unchecked.
    let envelope = match envelope_from_event(event) {
        Ok(Some(envelope)) => envelope,
        // Not a Continuum EventBridge envelope. Before dropping it, try
        // the thin chat projection: a plain airc chat message projects
        // to `chat:posted` for the positron renderer (task #84). Any
        // other kind (receipts, presence transitions, lifecycle) is not
        // ours here → skip. This is classification, not a fallback —
        // a non-message never fabricates a chat event.
        Ok(None) => {
            // Classify the plain airc event into a positron projection
            // signal. A message → `chat:posted`; a wall post → `wall:changed`;
            // a work-board event → `kanban:changed` (all re-read cues, not the
            // content). An event is at most one of these; anything else is not
            // ours here → skip. Classification, never a fallback — a
            // non-matching kind fabricates nothing.
            if let Some((name, payload)) = chat_posted_from_message(event) {
                // task #84: the persona-turn / plain-airc-message stream into
                // the room. This is the seam a client's live read surface
                // (positron ChatViewState) is fed from — probe it so the turn
                // stream is glass-box (did the say traverse daemon→attach→bus?).
                continuum_base::probe!(
                    class = "airc.chat.projected",
                    sender_id = %event.peer_id.as_uuid(),
                    room_id = %event.room_id.as_uuid(),
                    event_id = %event.event_id.as_uuid(),
                    "plain airc message projected to chat:posted for positron"
                );
                // #264: room-speech ring — recorded ONCE here (the single
                // seam every room message crosses), so restatement knowledge
                // never depends on any persona's workspace budget.
                crate::cognition::deliberation_budget::record_room_speech(
                    event.room_id.as_uuid(),
                    Some(event.peer_id.as_uuid()),
                    payload["content"].as_str().unwrap_or_default(),
                );
                bus.publish_async_only(name, payload);
            } else if let Some((name, payload)) = wall_changed_from_event(event) {
                bus.publish_async_only(name, payload);
            } else if let Some((name, payload)) = kanban_changed_from_event(event) {
                bus.publish_async_only(name, payload);
            }
            return Ok(());
        }
        Err(error) => {
            warn!("Ignoring malformed Continuum AIRC realtime event: {error}");
            return Ok(());
        }
    };
    // Case A — a Continuum realtime envelope. Two shapes reach the bus:
    //  - EventBridge payload → republish under its inline `eventName`.
    //  - chat_transcript (a human `chat/send`, the web client — the
    //    `Body::Json` shape) → project to the SAME thin `chat:posted` the
    //    plain-text sibling emits. Without this arm, human chat lines reach
    //    the daemon transcript and the persona (which learned to read the
    //    envelope) but never the positron `ChatViewState` — the room would
    //    show persona replies to structurally-invisible questions. Converge
    //    on ONE `chat:posted`, two wire shapes, mirroring the receive-side
    //    `perceptual_from_event`. Anything else is not ours here → skip.
    if let Some(bus_event) = bus_event_from_envelope(&envelope) {
        bus.publish_async_only(&bus_event.name, (*bus_event.payload).clone()); // crossing OUT of the bus type; one owned copy at the boundary
    } else if let Some(offer) = capacity_offer_from_envelope(&envelope) {
        // Grid capacity gossip (#56 step 4): fold the heard offer into the
        // process-global ledger, keyed on the WIRE's peer id (airc's authenticated
        // transport identity — the payload declares no identity to spoof). Our own
        // echoed offer lands here too: that IS the one-node-grid loopback proof.
        let now_ms = EpochMs::now().millis();
        let is_new =
            crate::capacity::gossip::global_ledger().hear(event.peer_id.as_uuid(), offer.clone(), now_ms);
        if is_new {
            continuum_base::probe!(
                class = "grid.capacity.heard",
                from_peer = %event.peer_id.as_uuid(),
                free_gb = (offer.gpu_free_bytes_live / 1_000_000_000),
                heard_peers = crate::capacity::gossip::global_ledger().heard_count(),
                "first capacity offer heard from a grid peer",
            );
        }
    } else if let Some((name, payload)) = chat_posted_from_envelope(&envelope, event) {
        continuum_base::probe!(
            class = "airc.chat.projected",
            sender_id = %payload["senderId"],
            room_id = %event.room_id.as_uuid(),
            event_id = %event.event_id.as_uuid(),
            "chat_transcript envelope projected to chat:posted for positron"
        );
        // #264: room-speech ring — same single-seam record as the plain arm.
        crate::cognition::deliberation_budget::record_room_speech(
            event.room_id.as_uuid(),
            Some(event.peer_id.as_uuid()),
            payload["content"].as_str().unwrap_or_default(),
        );
        bus.publish_async_only(name, payload);
    }
    Ok(())
}

/// Project a `chat_transcript` envelope (a human `chat/send`, the web
/// client — the `Body::Json` room-message shape) into the SAME thin
/// `chat:posted` payload the plain-text sibling
/// [`chat_posted_from_message`] emits. Returns `None` for any other
/// envelope (EventBridge, presence, media-control) — it is not a chat line.
///
/// Identity-free like its sibling: `senderId` is the envelope's logical
/// sender (recovered by [`chat_transcript_message`], falling back to the
/// relaying transport peer), name/kind resolved downstream from the
/// roster. `roomId`/`timestamp` are airc's transcript facts.
///
/// `messageId` prefers the envelope's inline `messageId` — the id
/// `chat/send` already persisted the row under — falling back to airc's
/// `event_id` when absent. Both are stable across replay + peers; using
/// the inline id means the projection, the durable chat store, and the
/// original `chat/send` all agree on ONE identity per human message, so
/// the store-side projector (#140) dedups instead of double-writing.
fn chat_posted_from_envelope(
    envelope: &AircRealtimeEnvelope,
    event: &airc_core::TranscriptEvent,
) -> Option<(&'static str, serde_json::Value)> {
    let (sender_id, content) = chat_transcript_message(envelope, event.peer_id.as_uuid())?;
    let message_id = match &envelope.payload {
        crate::routing::airc_realtime::AircRealtimePayload::ExistingSchema { payload } => payload
            .inline
            .as_ref()
            .and_then(|i| i.get("messageId"))
            .and_then(serde_json::Value::as_str)
            .and_then(|s| uuid::Uuid::parse_str(s).ok()),
        _ => None,
    }
    .unwrap_or_else(|| event.event_id.as_uuid());
    let payload = serde_json::json!({
        "messageId": message_id,
        "roomId": event.room_id.as_uuid(),
        "senderId": sender_id,
        "content": content,
        "timestamp": event.occurred_at_ms,
    });
    Some((CHAT_POSTED, payload))
}

/// Decode a `grid_capacity` envelope's inline payload into a [`CapacityOffer`].
/// Returns `None` for any other envelope — a non-offer fabricates nothing. A
/// present-but-malformed inline is also `None` (logged upstream as a decode
/// skip by the schema match being the honest gate here).
fn capacity_offer_from_envelope(
    envelope: &AircRealtimeEnvelope,
) -> Option<crate::capacity::gossip::CapacityOffer> {
    let crate::routing::airc_realtime::AircRealtimePayload::ExistingSchema { payload } = &envelope.payload
    else {
        return None;
    };
    if payload.schema != crate::routing::airc_realtime::AircRealtimeSchema::GridCapacity {
        return None;
    }
    // Borrow-decode: this runs per inbound event, and `.clone()?` copied the
    // whole inline payload just to hand `from_value` an owned Value.
    serde::Deserialize::deserialize(payload.inline.as_ref()?).ok()
}

/// Project a plain airc chat message into the THIN `chat:posted` bus
/// payload the positron chat projection consumes (`AircChatPosted` in
/// `ipc/positron_source.rs`). Returns `None` for any non-message event
/// or a message with no text body — those are not chat rows.
///
/// The payload is deliberately identity-free: `sender_id` is airc's
/// authoritative `peer_id`, and the projection resolves name / kind /
/// integrations downstream from the roster (presence cards). Identity
/// is a presence fact, never a message fact — see
/// `airc-to-positron-chat-projection`. airc's `event_id` IS the stable
/// `message_id` (same event → same id across replay + peers).
fn chat_posted_from_message(
    event: &airc_core::TranscriptEvent,
) -> Option<(&'static str, serde_json::Value)> {
    if event.kind != TranscriptKind::Message {
        return None;
    }
    let content = event.body.as_ref()?.as_text()?;
    let payload = serde_json::json!({
        "messageId": event.event_id.as_uuid(),
        "roomId": event.room_id.as_uuid(),
        "senderId": event.peer_id.as_uuid(),
        "content": content,
        "timestamp": event.occurred_at_ms,
    });
    Some((CHAT_POSTED, payload))
}

/// Project a `WallPostPublished` transcript event into the `wall:changed`
/// bus signal the positron wall projection consumes (`AircWallChanged` in
/// `ipc/positron_wall_source.rs`). Returns `None` for any other kind.
///
/// The signal carries ONLY the `room_id`: the pinned board is airc's
/// supersede projection (`Airc::wall_posts`), which cannot be
/// reconstructed from a single delta, so the consumer RE-READS the
/// authoritative board rather than trusting this event's body. This is
/// why we emit a change *cue*, not the post content — the exact
/// re-read-not-fold discipline the wall projector documents.
fn wall_changed_from_event(
    event: &airc_core::TranscriptEvent,
) -> Option<(&'static str, serde_json::Value)> {
    if event.kind != TranscriptKind::WallPostPublished {
        return None;
    }
    let payload = serde_json::json!({
        "roomId": event.room_id.as_uuid(),
    });
    Some((WALL_CHANGED, payload))
}

/// Project an airc work-board event into the `kanban:changed` bus signal
/// the positron kanban projection consumes (`AircKanbanChanged` in
/// `ipc/positron_kanban_source.rs`). Returns `None` for any non-work event.
///
/// Work events are not a `TranscriptKind` variant — they ride the transcript
/// stream discriminated by a body-hint header, so we ask airc-work's
/// authoritative `transcript_is_work_event` rather than matching a kind (the
/// header set is airc-work's contract to own, not ours to re-derive).
///
/// Like the wall cue, the signal carries ONLY the `room_id`: the board fold
/// (`Airc::work_board_complete`) cannot be reconstructed from a single work
/// delta, so the consumer RE-READS the authoritative board rather than
/// trusting this event's body. We emit a change *cue*, not the delta — the
/// same re-read-not-fold discipline the kanban projector documents.
fn kanban_changed_from_event(
    event: &airc_core::TranscriptEvent,
) -> Option<(&'static str, serde_json::Value)> {
    if !airc_work::transcript_is_work_event(event) {
        return None;
    }
    let payload = serde_json::json!({
        "roomId": event.room_id.as_uuid(),
    });
    Some((KANBAN_CHANGED, payload))
}

#[cfg(test)]
mod tests;
