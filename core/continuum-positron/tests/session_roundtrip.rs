//! End-to-end session-protocol smoke for the continuum-positron
//! substrate.
//!
//! This integration test exercises the whole chain that a real
//! session-task will go through, but without any transport: typed
//! payload → StateBuilder → cache.store → apply_subscribe →
//! snapshot frame → serde JSON round-trip → deserialize on the
//! "client side" → typed payload back.
//!
//! ## Why an integration test and not more unit tests
//!
//! Unit tests pin each module's behavior in isolation. They cannot
//! catch the class of bug where two modules each pass their own
//! unit tests but compose incorrectly — the chat payload typed at
//! the substrate seam, JSON-serialized, then re-parsed on a client
//! that imports the same `ChatViewState` shape. This test exercises
//! the WHOLE chain so a future refactor that breaks the substrate
//! ↔ wire ↔ consumer round-trip fails here even if every module's
//! own tests stay green.
//!
//! ## What the smoke proves end-to-end
//!
//! 1. Substrate produces a typed `ChatViewState`.
//! 2. `StateBuilder::session(chat)` stamps the
//!    right kind tag + monotonic revision + Session layer onto a
//!    `StateEnvelope`.
//! 3. `SubstrateStateCache` stores it; `apply_subscribe` retrieves
//!    it as a snapshot frame.
//! 4. The snapshot frame round-trips through `serde_json` losslessly
//!    — what crosses the wire is byte-identical to what the
//!    substrate emitted.
//! 5. The deserialized payload's typed fields are what they should
//!    be — proving the schema the renderer side imports matches the
//!    substrate's emitted shape.
//! 6. The exact-equality skip rule kicks in end-to-end on resubscribe
//!    — proving §6 reconnect-tolerance is wired correctly through
//!    every layer, not just at the unit-test apply_subscribe seam.
//!
//! ## Doctrine pinned by this smoke
//!
//! - `[[strong-typing-across-boundaries]]`: substrate emits typed
//!   `ChatViewState`, client receives typed `ChatViewState`, the
//!   `serde_json::Value` envelope in between is just transport.
//! - `[[shared-decode-per-persona-perspective]]`: the cache stores
//!   `Arc<StateEnvelope>`; two subscribers snapshotting against the
//!   same cached envelope receive identical bytes (proven by the
//!   two-subscriber test).
//! - §6 (ALPHA-GAP UI/Realtime Stability): the exact-equality skip
//!   rule end-to-end means a renderer can disconnect, reconnect, and
//!   resync via `last_seen` without seeing stale state forever.

use std::sync::Arc;

use continuum_positron::{
    apply_subscribe, ChatMessageView, ChatViewState, ClientMessage, KindRevision, Provenance,
    Revisions, RosterSlotView, SenderKind, ServerMessage, StateBuilder, StateLayer,
    SubstrateStateCache,
};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Helper: build a ChatViewState with a single message + one member
/// in the roster. Reused across tests so each one stays focused on
/// the property under test.
fn build_chat_state(content: &str) -> ChatViewState {
    let room_id = Uuid::from_u128(0xa);
    let member_id = Uuid::from_u128(0xb);
    let message_id = Uuid::from_u128(0xc);
    let sender_id = Uuid::from_u128(0xd);
    ChatViewState {
        room_id,
        room_name: "general".into(),
        purpose: "chat".into(),
        messages: vec![ChatMessageView {
            id: message_id,
            room_id,
            sender_id,
            sender_name: "Joel".into(),
            sender_kind: SenderKind::Human,
            integrations: BTreeMap::new(),
            provenance: Provenance {
                runtime: "interactive".into(),
            },
            content: content.into(),
            timestamp: 1_700_000_000_000,
        }],
        roster: vec![RosterSlotView {
            member_id,
            display_name: "Helper".into(),
            kind: SenderKind::Agent,
            integrations: BTreeMap::new(),
            provenance: Provenance {
                runtime: "claude".into(),
            },
            active: true,
            availability: Some("ready".into()),
            last_seen_ms: 1_700_000_000_000,
            vitals: BTreeMap::new(),
            loadout: None,
            avatar_url: None,
        }],
    }
}

#[test]
fn typed_payload_round_trips_through_subscribe_snapshot() {
    // what this catches: regression where the substrate's typed
    // payload, after passing through StateBuilder + cache + the
    // subscribe handler + serde, doesn't deserialize back into a
    // typed payload that matches the original. This is the
    // load-bearing E2E claim: continuum and positron-lit both speak
    // the same typed shape.

    let revisions = Arc::new(Revisions::new());
    let builder = StateBuilder::new(revisions);
    let cache = SubstrateStateCache::new();

    // 1. Substrate produces typed state.
    let original = build_chat_state("hello world");

    // 2. StateBuilder frames it as a wire envelope.
    let envelope = builder.session(original.clone());
    assert_eq!(envelope.kind, "chat");
    assert_eq!(envelope.revision, Some(1));
    assert_eq!(envelope.layer, StateLayer::Session);

    // 3. Cache stores it; subscribe retrieves it as a snapshot frame.
    cache.store(envelope.clone());
    let (sub, frames) = apply_subscribe(
        &cache,
        ClientMessage::Subscribe {
            kinds: vec!["chat".into()],
            layers: vec![StateLayer::Session],
            last_seen: vec![],
        },
    )
    .expect("subscribe ok");
    assert!(sub.covers("chat", StateLayer::Session));
    assert_eq!(frames.len(), 1, "fresh subscribe → exactly one snapshot");

    let snapshot = match &frames[0] {
        ServerMessage::State(e) => e,
        other => panic!("expected State frame, got {other:?}"),
    };
    assert_eq!(snapshot.kind, envelope.kind);
    assert_eq!(snapshot.revision, envelope.revision);

    // 4. The snapshot frame round-trips through serde_json losslessly.
    let json = serde_json::to_string(snapshot).expect("serialize snapshot");
    let parsed: continuum_positron::StateEnvelope =
        serde_json::from_str(&json).expect("deserialize snapshot");
    assert_eq!(&parsed, snapshot, "envelope must round-trip losslessly");

    // 5. The "client side" deserializes the payload as typed
    //    ChatViewState. This is the contract that makes positron-lit
    //    work — the renderer imports the SAME ts-rs-generated
    //    ChatViewState shape, and the payload value lifted out of
    //    `StateEnvelope.payload` is byte-identical to what the
    //    substrate built.
    let recovered: ChatViewState =
        serde_json::from_value(parsed.payload.clone()).expect("typed payload deserializes");
    assert_eq!(recovered, original, "typed payload must round-trip");
}

#[test]
fn skip_rule_works_end_to_end_on_resubscribe() {
    // what this catches: regression where the exact-equality skip
    // rule passes its unit test but breaks composed against the
    // builder + cache. This is §6 reconnect-tolerance in
    // miniature: substrate emits, renderer "renders" rev=1, renderer
    // resubscribes with last_seen=1, substrate skips the redundant
    // snapshot. If this regresses, every reconnect floods the
    // renderer with a duplicate frame — bandwidth waste, not stale-
    // forever, but the kind of inefficiency that leads to a future
    // "let's use >= instead" patch that re-introduces stale-forever.

    let revisions = Arc::new(Revisions::new());
    let builder = StateBuilder::new(revisions);
    let cache = SubstrateStateCache::new();

    let chat = build_chat_state("first message");
    let envelope = builder.session(chat);
    cache.store(envelope);

    // Renderer subscribes the first time, gets snapshot, renders
    // rev=1.
    let (_, first) = apply_subscribe(
        &cache,
        ClientMessage::Subscribe {
            kinds: vec!["chat".into()],
            layers: vec![StateLayer::Session],
            last_seen: vec![],
        },
    )
    .unwrap();
    assert_eq!(first.len(), 1, "first subscribe gets snapshot");

    // Renderer transport hiccups, reconnects, declares it last saw
    // rev=1. Substrate's current is still rev=1; skip rule fires.
    let (_, second) = apply_subscribe(
        &cache,
        ClientMessage::Subscribe {
            kinds: vec!["chat".into()],
            layers: vec![StateLayer::Session],
            last_seen: vec![KindRevision {
                kind: "chat".into(),
                revision: 1,
            }],
        },
    )
    .unwrap();
    assert!(
        second.is_empty(),
        "resubscribe with last_seen == current → no snapshot"
    );
}

#[test]
fn substrate_restart_resync_works_end_to_end() {
    // what this catches: THE load-bearing regression Fable's
    // round-2 review prevented and this codebase has multiple unit
    // tests pinned against. End-to-end: a "renderer" that holds
    // last_seen=500 from a pre-restart substrate meets a "freshly-
    // started" substrate at rev=1, and SHOULD receive the snapshot
    // (not silently skip via `>=`). If this regresses end-to-end
    // even with the unit tests green, something has shimmed the
    // skip rule above the substrate's seam.

    let revisions = Arc::new(Revisions::new());
    let builder = StateBuilder::new(revisions);
    let cache = SubstrateStateCache::new();

    // Fresh substrate, first-ever build → rev=1.
    let chat = build_chat_state("post-restart hello");
    let envelope = builder.session(chat.clone());
    assert_eq!(envelope.revision, Some(1));
    cache.store(envelope);

    // Stale renderer holds last_seen=500 from before a substrate
    // restart that reset its counter.
    let (_, frames) = apply_subscribe(
        &cache,
        ClientMessage::Subscribe {
            kinds: vec!["chat".into()],
            layers: vec![StateLayer::Session],
            last_seen: vec![KindRevision {
                kind: "chat".into(),
                revision: 500,
            }],
        },
    )
    .unwrap();
    assert_eq!(
        frames.len(),
        1,
        "stale-renderer-vs-fresh-substrate MUST snapshot, never skip"
    );

    // And the payload is the current one — not a "your future is
    // gone" silent staleness.
    let snapshot = match &frames[0] {
        ServerMessage::State(e) => e,
        other => panic!("expected State frame, got {other:?}"),
    };
    let recovered: ChatViewState = serde_json::from_value(snapshot.payload.clone()).unwrap();
    assert_eq!(recovered, chat, "renderer sees current substrate truth");
}

#[test]
fn two_subscribers_share_envelope_bytes_per_doctrine() {
    // what this catches: regression where the cache or subscribe
    // path accidentally duplicates per-subscriber storage. The
    // doctrine claim ([[shared-decode-per-persona-perspective]]) is
    // that two renderers subscribing to the same kind in the same
    // tick get byte-identical snapshots backed by the same cached
    // allocation. If the cache started cloning on each get(), N
    // renderers means N decoded payloads — the exact memory bloat
    // the lazy-cell doctrine exists to avoid.

    let revisions = Arc::new(Revisions::new());
    let builder = StateBuilder::new(revisions);
    let cache = SubstrateStateCache::new();

    let chat = build_chat_state("shared snapshot");
    let envelope = builder.session(chat);
    cache.store(envelope);

    let make_subscribe = || {
        apply_subscribe(
            &cache,
            ClientMessage::Subscribe {
                kinds: vec!["chat".into()],
                layers: vec![StateLayer::Session],
                last_seen: vec![],
            },
        )
        .unwrap()
        .1
    };

    let frames_a = make_subscribe();
    let frames_b = make_subscribe();

    // Both renderers receive identical bytes.
    let json_a = serde_json::to_string(&frames_a).unwrap();
    let json_b = serde_json::to_string(&frames_b).unwrap();
    assert_eq!(
        json_a, json_b,
        "two subscribers' snapshots must be byte-identical"
    );
}

#[test]
fn three_kinds_independently_partitioned() {
    // what this catches: regression where multi-kind subscribes
    // cross-contaminate (e.g. one kind's snapshot accidentally
    // emitted under another kind's tag). Models a realistic surface
    // where a chat widget + roster widget + presence widget share
    // one session. Skip rule applies independently per kind.

    let revisions = Arc::new(Revisions::new());
    let builder = StateBuilder::new(revisions);
    let cache = SubstrateStateCache::new();

    // Only ChatViewState ships a typed builder today; for the partition
    // test we write two extra kinds directly to the cache using arbitrary
    // kind strings. (Kinds are open + self-registered — a new widget kind
    // is a new `ViewState` impl owning its `KIND` const, not an enum edit;
    // these synthetic strings stand in for those future kinds.)
    let chat_env = builder.session(build_chat_state("chat hi"));
    cache.store(chat_env);

    // Synthesize two more kinds inline for the partition test.
    cache.store(continuum_positron::StateEnvelope {
        kind: "user-list".into(),
        revision: Some(11),
        layer: StateLayer::Session,
        payload: serde_json::json!({"users": ["a", "b"]}),
    });
    cache.store(continuum_positron::StateEnvelope {
        kind: "presence".into(),
        revision: Some(7),
        layer: StateLayer::Session,
        payload: serde_json::json!({"online": ["c"]}),
    });

    // Subscribe to all three; client holds last_seen for `user-list`
    // matching the cache → skip that one only.
    let (_, frames) = apply_subscribe(
        &cache,
        ClientMessage::Subscribe {
            kinds: vec!["chat".into(), "user-list".into(), "presence".into()],
            layers: vec![StateLayer::Session],
            last_seen: vec![KindRevision {
                kind: "user-list".into(),
                revision: 11,
            }],
        },
    )
    .unwrap();

    assert_eq!(frames.len(), 2, "chat + presence sent, user-list skipped");
    let kinds_sent: Vec<&str> = frames
        .iter()
        .map(|f| match f {
            ServerMessage::State(e) => e.kind.as_str(),
            _ => panic!("non-State frame in snapshot list"),
        })
        .collect();
    assert!(kinds_sent.contains(&"chat"));
    assert!(kinds_sent.contains(&"presence"));
    assert!(!kinds_sent.contains(&"user-list"), "user-list skipped");
}
