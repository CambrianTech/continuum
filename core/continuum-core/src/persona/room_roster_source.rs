//! RoomRosterSource — reads real airc room presence (`active_agents`)
//! and packages "who else is in this room right now" as RagItems for
//! the budget allocator.
//!
//! ### Why this source exists
//!
//! The persona cognition loop binds `engram + airc` sources and grounds
//! the turn in recalled memory + recent transcript. But it had **no
//! grounding in who the other participants are** — the airc transcript
//! delivers messages tagged with other citizens' names, while the
//! system prompt only says "you are <name>, never narrate others"
//! without ever naming who "others" are. Under that gap a small model
//! sees `BigMama:`, `Joel:`, `IntelMac:` in the history and role-plays
//! the whole room (the Ivar confabulation bug).
//!
//! This source closes the gap by reading the **truthful** presence list
//! from airc — not a static seed, not a guess — and delivering one item
//! per present citizen. The projection layer routes these into
//! **system-prompt grounding** (a `[Present in this room]` block), NOT
//! the conversation history (which would re-create the very "is this a
//! message?" confusion). See
//! [[docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md]] §5 slice 1.
//!
//! ### Architecture
//!
//! Mirrors [`AircRagSource`] exactly: abstracts an `AircRosterReader`
//! trait so unit tests don't need a live airc daemon. The real impl
//! rides on `airc_lib::Airc::active_agents` + `peer_alias` +
//! `peer_id` (self, to exclude the persona from its own roster). This
//! is the adapter-first rail: ship the trait + the real impl + a stub.
//!
//! ### Security grounding (per the airc-native doc §4)
//!
//! Each `AgentLiveness` carries a `runtime` label
//! (`"claude"`/`"codex"`/`"persona"`/`"interactive"`) — outsider agents
//! self-identify. The source carries that origin into item metadata so
//! downstream slices can mark grid-local citizens vs outsider agents and
//! let a persona weigh instructions by origin. Slice 1 only surfaces it;
//! enforcement is slice 3.
//!
//! ### Doctrine alignment
//!
//! - [[substrate-is-a-good-citizen-on-the-host]]: a failed presence read
//!   returns an empty delivery + `tracing::warn` — cognition stays up
//!   even when the airc subsystem is degraded.
//! - Persona-scoped at construction: a cross-persona ctx returns empty
//!   (defense in depth, same shape as `AircRagSource`).
//! - Roster is small + always-current; no pagination (atomic unit = one
//!   present citizen, like the `ToolSource` precedent in the trait doc).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use airc_core::identity::IdentityEvent;
use airc_core::{Body, PeerId, TranscriptEvent, TranscriptKind};
use airc_lib::{AgentLiveness, AircError};
use async_trait::async_trait;

use crate::persona::rag_budget::{
    ContinuationCursor, RagContext, RagDelivery, RagItem, RagSource, ResolutionPreference,
};

/// Source identifier — used by budget presets, telemetry, and the
/// service-loop projection that routes this delivery into system-prompt
/// grounding rather than conversation history.
const SOURCE_ID: &str = "room-roster";

/// How far back a heartbeat counts as "present". Matches the airc
/// agent-liveness convention of a short recency window — a peer that
/// hasn't beaten within this window is treated as gone.
const PRESENCE_WINDOW: Duration = Duration::from_secs(120);

/// How many recent transcript events to scan for heartbeats. The
/// presence reduction keeps one entry per peer, so this only needs to
/// be large enough to span the heartbeat cadence across all present
/// peers — not the full scrollback.
const HEARTBEAT_SCAN: usize = 200;

/// How many recent transcript events to scan for `IdentityPublished`
/// when building the peer→name map. Matches airc's own `peer_alias`
/// window (200).
pub(crate) const IDENTITY_SCAN: usize = 200;

/// Rough chars/token estimate — same heuristic `AircRagSource` /
/// `EngramSource` use. Real tokenizer integration lands in slice 12+.
fn estimate_tokens(content: &str) -> u32 {
    ((content.chars().count() / 4) as u32).saturating_add(1)
}

/// Build a `PeerId → display-name` map from ONE transcript page, reading
/// `IdentityPublished` cards. This is the batched form of airc's
/// per-peer `peer_alias` (which scans a full page EACH call): resolving
/// N present peers one-at-a-time is N+1 page scans — N+1 IPC round-trips
/// under the cognition lock for an N-peer room. One scan keeps a roster
/// delivery O(1) in room size. Uses only public `airc_core` types and
/// mirrors `airc_lib::Airc::peer_alias` parsing; later cards win
/// (page is chronological, so a re-published name overrides an old one).
pub(crate) fn parse_identity_names(events: Vec<TranscriptEvent>) -> HashMap<PeerId, String> {
    let mut names = HashMap::new();
    for event in events {
        if event.kind != TranscriptKind::IdentityPublished {
            continue;
        }
        let Some(Body::Json(value)) = event.body else {
            continue;
        };
        let Ok(IdentityEvent::PeerIdentityCard(card)) =
            serde_json::from_value::<IdentityEvent>(value)
        else {
            continue;
        };
        if !card.identity.name.is_empty() {
            names.insert(event.peer_id, card.identity.name);
        }
    }
    names
}

/// Abstract reader over airc room presence + identity. Production impl
/// rides on `airc_lib::Airc`; tests use a stub that returns canned
/// liveness without needing a daemon.
#[async_trait]
pub trait AircRosterReader: Send + Sync {
    /// This persona's own airc peer id — used to exclude self from the
    /// roster so the grounding block reads "who is NOT me".
    fn self_peer_id(&self) -> PeerId;

    /// Currently-alive agents in this persona's room, within `within`,
    /// scanning the most recent `window` transcript events. Newest-wins
    /// per peer; peers that signalled `Leaving` are excluded by airc.
    async fn active_agents(
        &self,
        within: Duration,
        window: usize,
    ) -> Result<Vec<AgentLiveness>, AircError>;

    /// All known peer display names in the room, as one `PeerId → name`
    /// map built from a SINGLE transcript scan. Batched on purpose:
    /// resolving names per-peer (airc's `peer_alias`) is one full page
    /// scan EACH, i.e. N+1 IPC round-trips under the cognition lock for
    /// an N-peer room. One map keeps a roster delivery O(1) in room size.
    async fn peer_alias_map(&self) -> Result<HashMap<PeerId, String>, AircError>;
}

/// `airc_lib::Airc` satisfies the reader contract directly. Orphan rule
/// OK — the trait is ours (defined in this crate).
#[async_trait]
impl AircRosterReader for airc_lib::Airc {
    fn self_peer_id(&self) -> PeerId {
        airc_lib::Airc::peer_id(self)
    }

    async fn active_agents(
        &self,
        within: Duration,
        window: usize,
    ) -> Result<Vec<AgentLiveness>, AircError> {
        airc_lib::Airc::active_agents(self, within, window).await
    }

    async fn peer_alias_map(&self) -> Result<HashMap<PeerId, String>, AircError> {
        let events = airc_lib::Airc::page_recent(self, IDENTITY_SCAN).await?;
        Ok(parse_identity_names(events))
    }
}

/// RoomRosterSource — persona-bound, reads presence from any
/// `AircRosterReader`.
pub struct RoomRosterSource {
    persona_id: uuid::Uuid,
    reader: Arc<dyn AircRosterReader>,
}

impl RoomRosterSource {
    pub fn new(persona_id: uuid::Uuid, reader: Arc<dyn AircRosterReader>) -> Self {
        Self { persona_id, reader }
    }

    /// Short, stable fallback label when a peer has no alias — the first
    /// 8 hex chars of its uuid. Never empty, never panics.
    fn short_peer_label(peer: PeerId) -> String {
        let s = peer.as_uuid().to_string();
        s.chars().take(8).collect()
    }

    /// Format one present citizen as a roster line. The line is
    /// human-readable on its own; the structured parts also ride in
    /// metadata so prompt-assembly and sentinel verifiers can render /
    /// trace without re-parsing the string.
    ///
    /// Shape: `<name> [<runtime>]` plus ` — <availability>` when the
    /// peer reported one. Examples: `Aria [persona]`,
    /// `win-claude [claude] — Busy`.
    fn format_line(name: &str, runtime: &str, availability: Option<String>) -> String {
        match availability {
            Some(avail) => format!("{name} [{runtime}] — {avail}"),
            None => format!("{name} [{runtime}]"),
        }
    }

    fn make_item(
        peer: PeerId,
        name: String,
        runtime: String,
        availability: Option<String>,
        last_seen_ms: u64,
    ) -> RagItem {
        let content = Self::format_line(&name, &runtime, availability.clone());
        let tokens = estimate_tokens(&content);
        RagItem {
            content,
            tokens,
            metadata: serde_json::json!({
                "peer_id": peer.as_uuid().to_string(),
                "display_name": name,
                "runtime": runtime,
                "availability": availability,
                "last_seen_ms": last_seen_ms,
            }),
        }
    }
}

#[async_trait]
impl RagSource for RoomRosterSource {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    async fn deliver(
        &self,
        ctx: &RagContext,
        budget: u32,
        resolution: ResolutionPreference,
    ) -> RagDelivery {
        // Persona-scoped: a cross-persona ctx gets nothing (defense in
        // depth, same shape as AircRagSource).
        if ctx.persona_id != self.persona_id {
            return RagDelivery {
                source_id: SOURCE_ID.to_string(),
                items: Vec::new(),
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Placeholder,
            };
        }

        let agents = match self
            .reader
            .active_agents(PRESENCE_WINDOW, HEARTBEAT_SCAN)
            .await
        {
            Ok(a) => a,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    persona_id = %self.persona_id,
                    "room_roster: active_agents failed — empty delivery, cognition stays up"
                );
                return RagDelivery {
                    source_id: SOURCE_ID.to_string(),
                    items: Vec::new(),
                    tokens_used: 0,
                    continuation: None,
                    resolution_used: ResolutionPreference::Placeholder,
                };
            }
        };

        let self_peer = self.reader.self_peer_id();

        // Resolve ALL names in ONE scan, not per-peer. A failure here is
        // non-fatal — degrade to short peer labels (still truthful) so a
        // transient identity-read outage doesn't blank the roster or the
        // turn. Logged at debug so an outage is observable.
        let names = self.reader.peer_alias_map().await.unwrap_or_else(|err| {
            tracing::debug!(
                error = %err,
                persona_id = %self.persona_id,
                "room_roster: peer_alias_map failed — falling back to short peer labels"
            );
            HashMap::new()
        });

        let mut items: Vec<RagItem> = Vec::new();
        let mut tokens_used: u32 = 0;
        for agent in agents {
            // Exclude self — the roster grounds the persona in who is
            // NOT itself.
            if agent.peer == self_peer {
                continue;
            }
            // Name from the single-scan map; fall back to a short peer
            // label so a present citizen is never invisible.
            let name = names
                .get(&agent.peer)
                .cloned()
                .unwrap_or_else(|| Self::short_peer_label(agent.peer));
            let availability = agent.coordination.availability.map(|a| format!("{a:?}"));
            let item = Self::make_item(
                agent.peer,
                name,
                agent.runtime.clone(),
                availability,
                agent.last_seen_ms,
            );
            if tokens_used.saturating_add(item.tokens) > budget {
                // Budget exhausted. Roster is unordered presence; we
                // stop rather than paginate (atomic unit = one present
                // citizen, no continuation). A truncated roster is
                // still truthful for the citizens it names.
                break;
            }
            tokens_used += item.tokens;
            items.push(item);
        }

        tracing::debug!(
            persona_id = %self.persona_id,
            budget,
            present = items.len(),
            tokens_used,
            "room_roster: deliver"
        );

        RagDelivery {
            source_id: SOURCE_ID.to_string(),
            items,
            tokens_used,
            // Presence is always-current and small; no pagination.
            continuation: None,
            resolution_used: resolution,
        }
    }

    async fn deliver_continuation(
        &self,
        _ctx: &RagContext,
        _cursor: ContinuationCursor,
        _budget: u32,
    ) -> Option<RagDelivery> {
        // Roster does not paginate — it's a small, always-current
        // snapshot. Any cursor is stale by construction.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    /// Test double — canned liveness + alias map, optional failure.
    struct StubReader {
        self_peer: PeerId,
        agents: Vec<AgentLiveness>,
        aliases: Vec<(PeerId, String)>,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn new(self_peer: PeerId, agents: Vec<AgentLiveness>) -> Self {
            Self {
                self_peer,
                agents,
                aliases: Vec::new(),
                fail: Mutex::new(false),
            }
        }
        fn with_alias(mut self, peer: PeerId, alias: &str) -> Self {
            self.aliases.push((peer, alias.to_string()));
            self
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl AircRosterReader for StubReader {
        fn self_peer_id(&self) -> PeerId {
            self.self_peer
        }
        async fn active_agents(
            &self,
            _within: Duration,
            _window: usize,
        ) -> Result<Vec<AgentLiveness>, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.agents.clone())
        }
        async fn peer_alias_map(&self) -> Result<HashMap<PeerId, String>, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.aliases.iter().cloned().collect())
        }
    }

    fn liveness(peer: PeerId, runtime: &str) -> AgentLiveness {
        AgentLiveness {
            peer,
            runtime: runtime.to_string(),
            client_id: None,
            scope: None,
            build: None,
            last_seen_ms: 1_000_000,
            coordination: Default::default(),
        }
    }

    // what this catches: the confabulation root cause — a present peer
    // must surface in the roster with a real alias + its origin runtime,
    // so the persona is grounded in who else is here.
    #[tokio::test]
    async fn present_peer_surfaces_with_alias_and_origin() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(
            StubReader::new(me, vec![liveness(other, "claude")]).with_alias(other, "win-claude"),
        );
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 1);
        assert!(delivery.items[0].content.contains("win-claude"));
        assert!(delivery.items[0].content.contains("claude"));
        assert_eq!(delivery.items[0].metadata["runtime"], "claude");
        // The service-loop projection reads metadata["display_name"] to
        // populate other_persona_names (single-party history-drop). Lock
        // that contract: the bare name must be present and match the
        // transcript sender name, not the formatted line.
        assert_eq!(delivery.items[0].metadata["display_name"], "win-claude");
        assert!(delivery.continuation.is_none());
    }

    // what this catches: the persona must NOT appear in its own roster —
    // the block is "who is NOT me". A self-entry would re-introduce the
    // self/other confusion the source exists to remove.
    #[tokio::test]
    async fn self_peer_excluded_from_roster() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(
            me,
            vec![liveness(me, "persona"), liveness(other, "persona")],
        ));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 1, "only the other peer, not self");
        assert_eq!(
            delivery.items[0].metadata["peer_id"],
            other.as_uuid().to_string()
        );
    }

    // what this catches: a peer with no alias is still surfaced (short
    // peer label), never silently dropped — an unnamed citizen is worse
    // than a uuid-labelled one for grounding.
    #[tokio::test]
    async fn peer_without_alias_falls_back_to_short_label() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(me, vec![liveness(other, "codex")]));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert_eq!(delivery.items.len(), 1);
        let expected = other
            .as_uuid()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>();
        assert!(delivery.items[0].content.contains(&expected));
    }

    // what this catches: empty room → empty delivery, no panic.
    #[tokio::test]
    async fn empty_room_delivers_nothing() {
        let me = PeerId::new();
        let reader = Arc::new(StubReader::new(me, vec![]));
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
    }

    // what this catches: airc degraded → empty delivery, cognition stays
    // up (good-citizen doctrine). A roster read failure must never take
    // down the turn.
    #[tokio::test]
    async fn reader_error_returns_empty_no_panic() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(me, vec![liveness(other, "persona")]));
        reader.set_fail(true);
        let source = RoomRosterSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx(), 1_000, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
    }

    // what this catches: cross-persona ctx gets nothing (defense in
    // depth) — a persona must never read another's room grounding.
    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let me = PeerId::new();
        let other = PeerId::new();
        let reader = Arc::new(StubReader::new(me, vec![liveness(other, "persona")]));
        let source = RoomRosterSource::new(persona(), reader);
        let alien = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(
                &RagContext::for_persona(alien, 1_000_000),
                1_000,
                ResolutionPreference::Raw,
            )
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.resolution_used, ResolutionPreference::Placeholder);
    }

    // what this catches: budget truncates the roster instead of
    // over-spending — a present citizen's line is an atomic unit, so we
    // stop cleanly rather than emit a partial line.
    #[tokio::test]
    async fn budget_truncates_without_overspend() {
        let me = PeerId::new();
        let reader = Arc::new(StubReader::new(
            me,
            vec![
                liveness(PeerId::new(), "persona"),
                liveness(PeerId::new(), "persona"),
                liveness(PeerId::new(), "persona"),
            ],
        ));
        let source = RoomRosterSource::new(persona(), reader);
        // Each line ~ a handful of tokens; a tiny budget fits at most one.
        let delivery = source.deliver(&ctx(), 4, ResolutionPreference::Raw).await;
        assert!(delivery.items.len() < 3, "budget must truncate the roster");
        assert!(delivery.tokens_used <= 4, "never overspend the budget");
    }
}
