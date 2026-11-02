//! The events a mind RADIATES on the bus for clients to fold: a line she heard, an act her
//! hands ran. The emitter owns its wire shape (name + payload), so it lives with the mind;
//! the chat projection (`ipc::positron_source`) is a consumer and imports it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A citizen's inbound stream ADMITTED a posted line as a turn — the delivery
/// receipt folded onto the row as `heard_by` ("heard by 8").
pub const CHAT_HEARD: &str = "chat:heard";

/// Event name a persona's HANDS radiate one executed tool act under (#243:
/// tool actions show IN chats as collapsed receipts).
pub const PERSONA_ACT: &str = "persona:act";

/// Typed `persona:act` payload — one executed tool call, radiated from the
/// act→observe seam (`apply_act`, the ONE choke point every hand action
/// passes through: live turns, directed turns, and agent/solve alike). The
/// projection folds it into `ChatViewState::acts` so every client renders
/// the same receipt stream. Same `Serialize` agree-by-construction
/// discipline as `ipc::positron_source::PersonaVitalsUpdate`: the emitter builds
/// THIS struct, never a hand JSON. Carries the resolved `actor_name`
/// because the producer (the acting body) knows it authoritatively — the
/// projection still re-resolves against the roster when the actor is
/// present, keeping one display-identity source where possible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaActUpdate {
    pub act_id: Uuid,
    pub room_id: Uuid,
    pub actor_id: Uuid,
    pub actor_name: String,
    pub tool: String,
    pub summary: String,
    /// Legacy outcome bool, unchanged in meaning for the clients already reading
    /// it: false ONLY for a real failure.
    pub ok: bool,
    /// The act's TYPED state — "succeeded" | "failed" | "running" | "undecodable"
    /// | "unprojected". `ok` cannot express a call that was accepted but has not
    /// finished, nor a result the substrate could not decode, and rendering either
    /// as a tick is card f6c50a49. New field rather than a redefinition of `ok`, so
    /// a client that has not adopted it is not silently given a new meaning.
    #[serde(default)]
    pub state: String,
    /// Her reasoning for the act batch this receipt opens, whole; `None` on the batch's
    /// later receipts, when she gave none, and for a turn in her mind room. For the
    /// human's view only: it never goes to the room's transcript.
    #[serde(default)]
    pub thought: Option<String>,
    pub timestamp: u64,
}
