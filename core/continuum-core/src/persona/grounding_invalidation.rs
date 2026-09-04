//! Grounding invalidation — the EVENT WIRES that make [`CachedRagSource`]
//! honest (#398 slice 2).
//!
//! A cache with no invalidation wire is a staleness bug waiting to be
//! measured (#346's stale board is the cautionary tale). The rule this
//! module enforces by construction: **no wrap without a wire** — the owner
//! that wraps a source spawns the matching invalidator here, or doesn't
//! wrap at all.
//!
//! First wire (outlier A of the #398 pair): the `[workspace-map]` grounding.
//! Its substrate is the filesystem the persona's HANDS mutate, and every
//! hand action already emits `command:completed` on the kernel bus
//! ([`COMMAND_COMPLETED_TOPIC`], real-time — the coalescer passes
//! `command:*` through). So the invalidator is a pure REUSE of the
//! async-dispatch listener's shape (`cognition/dispatch_listener.rs`):
//! subscribe, filter, mark. No polling, no new pump — the event that
//! changes the substrate IS the dirty signal
//! ([[the-whole-system-is-event-based-not-polling]]).
//!
//! Deliberately COARSE: the completion event carries no persona
//! attribution, so any citizen's write dirties every wrapped map. Today all
//! personas share one workspace root (#49 pending), so that is CORRECT, and
//! after #49 it degrades to over-invalidation — an extra refetch, never a
//! stale projection. Staleness is the failure mode we refuse; a wasted
//! refetch is the price we accept.

use std::sync::Arc;

use crate::persona::cached_source::WeakDirtyHandle;
use crate::runtime::command_events::{CommandCompletedEvent, COMMAND_COMPLETED_TOPIC};
use crate::runtime::message_bus::MessageBus;

/// Does completing this command potentially change the filesystem a
/// `[workspace-map]` describes?
///
/// DENY-list of known read-only verbs, then prefix-match the families whose
/// verbs mutate (files, shell, git, cargo — a build materializes `target/`).
/// Unknown verbs in those families default to "mutates": over-invalidation
/// is safe, a missed mutation is a stale map.
pub fn mutates_workspace(command: &str) -> bool {
    const READ_ONLY: &[&str] = &[
        "code/read",
        "code/list",
        "code/tree",
        "code/search",
        "code/glob",
        "code/diff",
        "git/status",
        "git/log",
        "git/diff",
        "tool/output",
    ];
    if READ_ONLY.contains(&command) {
        return false;
    }
    command.starts_with("code/") || command.starts_with("git/") || command.starts_with("cargo/")
}

/// Spawn the workspace-map invalidator: marks `dirty` whenever a
/// workspace-mutating command completes on the bus.
///
/// Holds only a [`WeakDirtyHandle`] — when the wrapped source is gone (an
/// ephemeral eval fork's cycle dropped), the next mark fails and the task
/// exits instead of parking forever. Success is NOT required to mark: a
/// failed `code/shell` may have mutated before it failed (mkdir-then-die),
/// so failures dirty too.
/// The workspace map's dirty handle per persona, so a ROOT change (her hands
/// moved to a card's checkout, or back home) re-renders the map even though no
/// command mutated the workspace. Without it the map kept rendering her home
/// while her shell stood inside the checkout (2026-09-05).
static WORKSPACE_MAP_DIRTY: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, WeakDirtyHandle>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

pub fn register_workspace_map_dirty(persona_id: uuid::Uuid, dirty: WeakDirtyHandle) {
    WORKSPACE_MAP_DIRTY.lock().unwrap_or_else(|e| e.into_inner()).insert(persona_id, dirty);  // poisoned lock = read the last state, same policy as every lock in this crate
}

pub fn mark_workspace_map_dirty(persona_id: uuid::Uuid) -> bool {
    WORKSPACE_MAP_DIRTY
        .lock()
        .unwrap_or_else(|e| e.into_inner())  // poisoned lock = read the last state, same policy as every lock in this crate
        .get(&persona_id)
        .is_some_and(|d| d.mark())
}

pub fn spawn_workspace_invalidator(bus: Arc<MessageBus>, dirty: WeakDirtyHandle) {
    let mut rx = bus.receiver();
    tokio::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(e) => e,
                // Lagged == we MISSED events, and one of them may have been a
                // mutation. Mark dirty conservatively — correctness over a
                // spare refetch — and keep listening.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if !dirty.mark() {
                        return;
                    }
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            };
            if event.name != COMMAND_COMPLETED_TOPIC {
                continue;
            }
            let Ok(ev) = serde_json::from_value::<CommandCompletedEvent>((*event.payload).clone()) else { // typed decode needs owned; one copy at THIS consumer, not per receiver
                continue;
            };
            if !mutates_workspace(&ev.command_name) {
                continue;
            }
            if !dirty.mark() {
                return; // cache dropped — stop listening on its behalf
            }
        }
    });
}

/// Is this transcript event a room-state PUBLISH — the event class that
/// changes what the doctrine and wall groundings project?
///
/// Covers BOTH kinds because they alias in the wild: doctrine now rides
/// `WallPostPublished` with `category="doctrine"` while legacy peers still
/// emit `DoctrinePublished` (transcript.rs documents the aliasing). One
/// predicate, both caches — a doctrine event dirtying the wall (and vice
/// versa) is over-invalidation, which is safe; splitting them on the
/// category field would risk the stale side of the aliasing instead.
pub fn is_room_state_publish(kind: &airc_core::TranscriptKind) -> bool {
    matches!(
        kind,
        airc_core::TranscriptKind::DoctrinePublished | airc_core::TranscriptKind::WallPostPublished
    )
}

/// Spawn the doctrine/wall invalidator: ONE subscribe stream per persona
/// marking every handed-in cache when a room-state publish lands. The
/// caller subscribes BEFORE calling (failure surfaces at the call site,
/// same contract as the command pump) and hands the stream in.
///
/// These events are RARE (a doctrine or wall edit), which is exactly why
/// the sources cache so well — and why the doctrine fetch, the one
/// SYNCHRONOUS airc round-trip on every live compose (ColdStartCritical),
/// was pure waste per tick. Exits when every handle is dead (all wrapped
/// sources dropped) or the stream ends terminally (airc-lib owns transient
/// reconnection; a terminal end is a real fault the pump already logs loud
/// — this listener just stops, the caches go permanently dirty-capable-less
/// but the personas' pumps have bigger problems at that point).
pub fn spawn_publish_invalidator(
    mut stream: airc_lib::FilteredEventStream,
    handles: Vec<WeakDirtyHandle>,
) {
    use futures::stream::StreamExt;
    tokio::spawn(async move {
        let mut handles = handles;
        loop {
            let event = match stream.next().await {
                None => return,
                // Lag: we missed events — one may have been a publish.
                // Mark everything conservatively, prune the dead.
                Some(Err(_lag)) => {
                    handles.retain(|h| h.mark());
                    if handles.is_empty() {
                        return;
                    }
                    continue;
                }
                Some(Ok(e)) => e,
            };
            if !is_room_state_publish(&event.kind) {
                continue;
            }
            handles.retain(|h| h.mark());
            if handles.is_empty() {
                return; // every wrapped source dropped — stop listening
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::cached_source::{CachedRagSource, DirtyHandle};
    use crate::persona::rag_budget::{
        ContinuationCursor, RagContext, RagDelivery, RagSource, ResolutionPreference,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct CountingSource {
        fetches: AtomicU32,
    }

    #[async_trait]
    impl RagSource for CountingSource {
        fn source_id(&self) -> &'static str {
            "counting"
        }
        fn expand_command(&self) -> Option<&'static str> {
            None
        }
        fn floor_tokens(&self) -> u32 {
            1
        }
        async fn deliver(
            &self,
            _ctx: &RagContext,
            _budget: u32,
            _resolution: ResolutionPreference,
        ) -> RagDelivery {
            let n = self.fetches.fetch_add(1, Ordering::SeqCst) + 1;
            RagDelivery {
                source_id: "counting".to_string(),
                items: vec![crate::persona::rag_budget::RagItem {
                    content: format!("fetch #{n}"),
                    tokens: 4,
                    metadata: serde_json::json!({}),
                }],
                tokens_used: 4,
                continuation: None,
                resolution_used: ResolutionPreference::Raw,
            }
        }
        async fn deliver_continuation(
            &self,
            _ctx: &RagContext,
            _cursor: ContinuationCursor,
            _budget: u32,
        ) -> Option<RagDelivery> {
            None
        }
    }

    fn completed(command: &str) -> serde_json::Value {
        serde_json::to_value(CommandCompletedEvent {
            command_name: command.to_string(),
            duration_ms: 1,
            success: true,
            error: None,
            handle: None,
            result: None,
        })
        .expect("serialize")
    }

    // what this catches: the deny-list vs prefix-family split. A read-only verb
    // marking dirty defeats the whole cache (refetch per act again); a mutating
    // verb NOT marking is a stale map — the #346 staleness class.
    #[test]
    fn mutation_predicate_splits_read_from_write() {
        for read_only in [
            "code/read",
            "code/list",
            "code/tree",
            "code/search",
            "git/status",
        ] {
            assert!(
                !mutates_workspace(read_only),
                "{read_only} must not dirty the map"
            );
        }
        for mutating in [
            "code/write",
            "code/edit",
            "code/shell",
            "code/run",
            "code/create-workspace",
            "git/checkout",
            "cargo/build",
        ] {
            assert!(mutates_workspace(mutating), "{mutating} must dirty the map");
        }
        // Foreign namespaces never dirty (a chat/send is not a file mutation).
        assert!(!mutates_workspace("chat/send"));
        assert!(!mutates_workspace("work/claim"));
    }

    // what this catches: the end-to-end wire — a `command:completed` bus event
    // for a mutating command reaches the cache and forces exactly the refetch
    // the compose path needs, while read-only completions leave the cached
    // projection served. This is the "no wrap without a wire" contract working.
    #[tokio::test]
    async fn bus_mutation_events_dirty_the_cache_read_only_do_not() {
        let bus = Arc::new(MessageBus::new());
        let inner = Arc::new(CountingSource {
            fetches: AtomicU32::new(0),
        });
        let (cached, dirty) = CachedRagSource::new(inner.clone());
        spawn_workspace_invalidator(bus.clone(), dirty.downgrade());
        drop(dirty); // wiring done — cache liveness now keys the listener

        let ctx = RagContext::for_persona(uuid::Uuid::new_v4(), 0);
        let d = cached.deliver(&ctx, 100, ResolutionPreference::Raw).await;
        assert_eq!(d.items[0].content, "fetch #1");

        // Read-only completion → cache keeps serving.
        bus.publish_async_only(COMMAND_COMPLETED_TOPIC, completed("code/read"));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let d = cached.deliver(&ctx, 100, ResolutionPreference::Raw).await;
        assert_eq!(
            d.items[0].content, "fetch #1",
            "read-only completion must not dirty"
        );

        // Mutating completion → next deliver refetches.
        bus.publish_async_only(COMMAND_COMPLETED_TOPIC, completed("code/write"));
        let mut refetched = false;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let d = cached.deliver(&ctx, 100, ResolutionPreference::Raw).await;
            if d.items[0].content == "fetch #2" {
                refetched = true;
                break;
            }
        }
        assert!(
            refetched,
            "a code/write completion must dirty the wrapped map"
        );
        assert_eq!(
            inner.fetches.load(Ordering::SeqCst),
            2,
            "exactly one refetch"
        );
    }

    // what this catches: the publish predicate — BOTH kinds must dirty (they
    // alias: doctrine rides WallPostPublished with category="doctrine" while
    // legacy peers emit DoctrinePublished), and chat traffic must NOT, or the
    // doctrine cache refetches per message and the whole win evaporates.
    #[test]
    fn room_state_publish_predicate_covers_the_aliasing_pair_only() {
        use airc_core::TranscriptKind as K;
        assert!(is_room_state_publish(&K::DoctrinePublished));
        assert!(is_room_state_publish(&K::WallPostPublished));
        for benign in [
            K::Message,
            K::Attachment,
            K::Receipt,
            K::Presence,
            K::System,
        ] {
            assert!(
                !is_room_state_publish(&benign),
                "{benign:?} must not dirty doctrine/wall"
            );
        }
    }

    // what this catches: the weak-handle lifecycle — a dead cache must stop
    // being markable, which is what lets per-fork invalidator tasks exit
    // instead of leaking one parked task per ephemeral eval fork.
    #[test]
    fn weak_handle_dies_with_the_cache() {
        let inner = Arc::new(CountingSource {
            fetches: AtomicU32::new(0),
        });
        let (cached, dirty): (Arc<CachedRagSource>, DirtyHandle) = CachedRagSource::new(inner);
        let weak = dirty.downgrade();
        assert!(weak.mark(), "alive while the cache lives");
        drop(dirty);
        assert!(weak.mark(), "the cache alone keeps the flag alive");
        drop(cached);
        assert!(!weak.mark(), "all owners gone → mark reports dead");
    }
}
