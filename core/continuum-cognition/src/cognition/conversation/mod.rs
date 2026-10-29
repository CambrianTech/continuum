//! The conversation contract: what a mind perceives arriving in a room and how she answers
//! into one, owned by cognition so the act/observe drive, the settle and the experience
//! stream name no persona module. Persona supplies the live transport (airc) behind
//! [`PersonaConversation`]; cognition only drives it.
//!
//! The leaf group a room line renders through lives here with it: the room-event decoders
//! (`room_event`), the channel queue items and types, the inbox types, salience, and the
//! persona identity a recipient is named by.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

pub mod channel_items;
pub mod channel_types;
pub mod persona_identity;
pub mod room_event;
pub mod salience;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod scripted_conversation;
pub mod seen_ids;
pub mod transcript_reader;
pub mod types;
pub mod workspace_turns;

/// In-process inbox depth between the pump and the loop. A turn in progress
/// can leave the loop away for minutes; 8k events is hours of a busy room.
pub const INBOX_CAPACITY: usize = 8_192;

/// Events paged per room per catch-up tick.
pub const CATCH_UP_PAGE: usize = 32;

/// A substrate-friendly slice of one airc event: just what the
/// service loop needs to decide whether to respond. Strips away the
/// full `TranscriptEvent` surface so the conversation abstraction
/// stays compact and the trait remains stubbable without dragging
/// every airc type into the test.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IncomingMessage {
    /// The transcript event this turn came from — the row a `chat:heard`
    /// receipt points back at. Nil for scripted/synthetic turns.
    pub event_id: Uuid,
    /// Monotonic lamport clock — used for pre-attach high-water-mark
    /// filtering.
    pub lamport: u64,
    /// Cryptographic source identity per
    /// [[persona-identity-derives-from-source-id]]. The loop compares
    /// against the hosting persona's own peer_id to skip self-loop
    /// echoes.
    pub peer_id: Uuid,
    /// Caption/text of the room message.
    pub text: String,
    /// Durable references only; decoding a message never fetches attachment bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<channel_items::MediaItemRequest>,
    /// The room the event ARRIVED in — the transport's `TranscriptEvent.room_id`,
    /// which is authoritative for the turn's context (A.6, the missing context
    /// axis). Nil means the source predates room stamping (scripted/test
    /// conversations) and the loop falls back to `identity.default_room`; a real
    /// wire event always carries its room. Without this, an operator's CLI
    /// publish woke the persona into a nil-room turn where every room-scoped
    /// RAG source (roster, doctrine, board, kanban) abstained — she heard the
    /// words but stood in no room (glass-boxed 2026-07-23, Anwen ACK test).
    pub room_id: Uuid,
    /// A typed work fact (a verdict, a card move, a submission, a claim) decoded
    /// from the room by `cognition::conversation::room_event::room_work_from_event`. `Some` means this
    /// turn is a BOARD FACT she perceives, rendered as such, never as the actor's
    /// speech; `peer_id` is the actor (the reviewer, the mover). Phase 1 wiring of
    /// EVENT-MIND.md: measured 2026-10-04, a PASS on her own card reached her node
    /// and was dropped as `non_chat_schema`; nothing on her board could wake her.
    pub work: Option<room_event::RoomWork>,
}

impl IncomingMessage {
    pub fn render_content(&self) -> String {
        if let Some(work) = &self.work {
            return room_event::render_room_work(work, self.peer_id);
        }
        room_event::render_room_content(&self.text, &self.media)
    }

    /// The same source-labelled input at inference, replay and teaching boundaries.
    /// This is rendering, not a replacement for the typed event/room identity.
    pub fn render_room_update(&self) -> String {
        format!(
            "[Room message received during this turn; room {}; peer {}; event {}]\n{}",
            self.room_id, self.peer_id, self.event_id, self.render_content()
        )
    }
}

/// Polymorphism rail for "talk to the grid as this persona". The
/// substrate's loop never touches `airc_lib::Airc` directly; the
/// real surface is behind this trait. Slice 11 ships the
/// `AircPersonaConversation` impl.
///
/// All methods are async because the production impl chains over
/// airc's IPC socket. Tests use a stub that's instant.
#[async_trait]
pub trait PersonaConversation: Send + Sync {
    /// Open whatever connection / stream / state the conversation
    /// needs to start yielding messages. Called once at boot, BEFORE
    /// the first `high_water_mark` or `next_message`.
    ///
    /// Production (`AircPersonaConversation`): opens the airc
    /// subscribe stream. Without `prime`, the stream was opened
    /// lazily on first `next_message`, which paid the daemon
    /// round-trip on the cognition hot path. With `prime`, that
    /// round-trip lands at the supervisor's boot phase — the
    /// persona is "ready to converse" the moment her service loop
    /// starts iterating.
    ///
    /// Tests (`StubConversation`): no-op. Idempotent — calling
    /// `prime` twice is safe but the second call is a no-op.
    ///
    /// Returns `Err` if priming fails (daemon unreachable, room
    /// gone). Per [[no-fallbacks-ever]] the loop refuses to start
    /// rather than entering a degraded path.
    async fn prime(&mut self) -> Result<(), String>;

    /// Highest lamport observed in transcript history before live
    /// subscription. Used to ignore messages that arrived BEFORE the
    /// persona attached — avoids replying to ancient chat just
    /// because a restart loaded them through `page_recent`.
    async fn high_water_mark(&self, limit: usize) -> Result<u64, String>;

    /// The cutoff established by successful priming. Live conversations freeze
    /// this at attach, so starting the loop does not issue another fallible RPC.
    async fn initial_water_mark(&self, limit: usize) -> Result<u64, String> {
        self.high_water_mark(limit).await
    }


    /// Yield the next inbound message, or `Ok(None)` when the
    /// stream is exhausted (daemon disconnected, peer gone). On
    /// transient errors (stream lag, transport hiccup) the impl
    /// should yield `Err` so the loop can record + continue.
    ///
    /// Assumes `prime` was called once at boot. If it wasn't, the
    /// impl MAY lazy-prime (production currently does, for
    /// backward compat) but the substrate's preferred path is
    /// eager priming so the round-trip lands off the hot path.
    async fn next_message(&mut self) -> Result<Option<IncomingMessage>, String>;

    /// Perceive currently ready room input without waiting for a new arrival.
    /// The conversation RETAINS ownership for subsequent room attention, including
    /// if the active drive is cancelled or cannot fit these inputs. Repeated
    /// snapshots may overlap: the drive uses the ordinary event-id deduper.
    async fn perceive_ready(
        &mut self,
    ) -> Result<&std::collections::VecDeque<Arc<IncomingMessage>>, String>;

    /// Reply with text INTO A NAMED ROOM — normally the room the turn
    /// being answered arrived in ([`IncomingMessage::room_id`]).
    ///
    /// Not "the persona's default room", which is what this used to
    /// be. A persona is in more than one room, so a reply that always
    /// went to her default answered the wrong audience the moment she
    /// was addressed anywhere else — visibly worse than silence,
    /// because it reads as a non-sequitur rather than a missing wire.
    /// `Uuid::nil()` keeps the documented pre-room-stamping contract
    /// (scripted / test sources) and routes to her default.
    async fn say_in(&self, room_id: Uuid, text: &str) -> Result<(), String>;

    /// Her wakes from the one feed (EVENT-MIND §1b), once her perception region has
    /// booted; `None` before then, after they were taken, or for a conversation with no
    /// region. The serve loop takes them once and consumes them beside her inbox.
    fn take_wakes(&mut self) -> Option<tokio::sync::mpsc::Receiver<Wake>> {
        None
    }
}

/// Why she is being woken. The activity rides on the wake; no room is implied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wake {
    /// An activity's delta is loud enough for her dial.
    Perceive { activity: Uuid, salience: salience::Salience },
    /// The time she expected something by has passed.
    Continuation,
    /// First wake after a restart: continue what she was doing.
    Resume,
    /// Work she sent away ended (a command past its act's window, a sentinel): its result is
    /// already in her working memory. She is woken whether or not she remembers the handle.
    TaskEnded { task: Uuid },
}
