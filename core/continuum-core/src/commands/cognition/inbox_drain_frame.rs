//! `inbox/drain-frame` — drain a batched turn-frame off a persona's inbox (typed,
//! dep-holding).
//!
//! Drains up to `max_items` messages that arrived within a `window_ms` coalescing window
//! into one [`PersonaInboxFrame`](crate::persona::PersonaInboxFrame), and fires the
//! background replay-record write so the drained frame survives into the recorder.
//! Captures the owning module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! The frame-recording helper lives in `modules/cognition.rs`
//! ([`record_drained_turn_frame`](crate::modules::cognition::record_drained_turn_frame)) —
//! it is shared with the still-legacy Lane D arms (`persona/drain-turn-frame`,
//! `persona/turn-execute`), so this command references the one canonical copy rather than
//! forking it.
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::{record_drained_turn_frame, CognitionState};
use crate::persona::PersonaInboxFrame;
use crate::sdk_codegen::CommandError;

fn default_window_ms() -> u64 {
    80
}

fn default_max_items() -> usize {
    16
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/InboxDrainFrameParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InboxDrainFrameParams {
    /// Persona whose inbox is drained.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Coalescing window in milliseconds (default 80).
    #[serde(default = "default_window_ms")]
    #[ts(type = "number")]
    pub window_ms: u64,
    /// Maximum messages to drain into one frame (default 16).
    #[serde(default = "default_max_items")]
    #[ts(type = "number")]
    pub max_items: usize,
}

/// Result of `inbox/drain-frame`: the drained frame, or `None` on an empty inbox.
///
/// A NAMED wrapper around `Option<PersonaInboxFrame>` — the command-schema
/// validator ([`crate::sdk_codegen`]) rejects a bare `Option<T>` output because an
/// inline `T | null` has no named TS type to `export_to`, and one such command
/// panics the whole `command_registry()` walk. `frame == None` preserves the
/// contract: the coalescing window was empty (no-op).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/InboxDrainFrameResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InboxDrainFrameResult {
    pub frame: Option<PersonaInboxFrame>,
}

crate::action_command! {
    /// Drain a batched turn-frame (messages within a coalescing window) off the persona's
    /// inbox and background-record it for replay. Host-invoked. Returns `null` when the
    /// inbox is empty.
    pub struct InboxDrainFrame { state: Arc<CognitionState> }
    name: "inbox/drain-frame",
    access: Internal,
    params: InboxDrainFrameParams,
    output: InboxDrainFrameResult,
    run(this, _ctx, p) => {
        let persona = this
            .state
            .personas
            .get(&p.persona_id)
            .ok_or_else(|| CommandError::NotFound(format!("No cognition for {}", p.persona_id)))?;

        let frame = persona.inbox.drain_frame(p.window_ms, p.max_items);
        record_drained_turn_frame(&frame);

        Ok(InboxDrainFrameResult { frame })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. inbox/drain-frame is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(InboxDrainFrame::NAME, "inbox/drain-frame");
        assert_eq!(InboxDrainFrame::ACCESS, AccessLevel::Internal);
    }
}
