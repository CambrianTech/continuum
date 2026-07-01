//! `persona/drain-turn-frame` — drain a persona's inbox into a replay-stable turn
//! frame (typed, dep-holding).
//!
//! Lane D of the persona substrate (alpha card #1409). Drains the persona's inbox
//! into one bounded room slice, wraps it in a
//! [`PersonaTurnFrame`](crate::persona::turn_frame::PersonaTurnFrame), derives the
//! replay-stable outputs (consolidated inbox + RAG seed + captured prompt), and
//! persists the record to `~/.continuum/replay/` for prod-replay. The returned record
//! IS the input contract downstream inference / RAG / sentinel attribution replay
//! against — building it lazily at replay time would depend on the inbox→prompt mapping
//! staying bit-identical across substrate versions, a contract no one wants to maintain.
//!
//! Captures the owning [`CognitionModule`](crate::modules::cognition::CognitionModule)'s
//! shared [`CognitionState`] (same dep-holding shape as the other stateful cognition
//! commands); the per-persona inbox lives on it. Assembled by
//! [`command_objects`](super::command_objects), called from `CognitionModule::commands`.
//!
//! Fail-loud note: a persona with no cognition engine (`cognition/create-engine` never
//! ran) is a caller bug — surfaced as `CommandError::Invalid`, never a silent no-op.
//! An **empty** drain (no messages in the window) is a legitimate contract signal, not a
//! failure: it returns `Ok(None)` → JSON `null`, and the caller treats it as no-op.
//!
//! `access: Internal` — substrate cognition IPC the host persona loop drives, not a
//! remote-callable persona toolbelt verb.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::logging::TimingGuard;
use crate::modules::cognition::{record_drained_turn_frame, CognitionState};
use crate::persona::turn_frame::{PersonaTurnFrame, PersonaTurnFrameReplayRecord};
use crate::sdk_codegen::CommandError;

/// Default frame window (ms) when the caller omits `windowMs`: the bounded slice of
/// recent inbox time the drain consolidates. Transplanted verbatim from the legacy arm.
fn default_window_ms() -> u64 {
    80
}

/// Default max messages a single drain pulls when the caller omits `maxItems`. Bounds
/// the frame so one drain can't pull an unbounded backlog. Transplanted from the arm.
fn default_max_items() -> u64 {
    16
}

/// Params for `persona/drain-turn-frame`: which persona to drain and the frame bounds.
/// Missing `windowMs` / `maxItems` fall back to the substrate defaults (80ms / 16), so
/// the minimal call is just `{ personaId }` — matching the legacy `u64_or` reads.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/DrainTurnFrameParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct DrainTurnFrameParams {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[serde(default = "default_window_ms")]
    #[ts(type = "number")]
    pub window_ms: u64,
    #[serde(default = "default_max_items")]
    #[ts(type = "number")]
    pub max_items: u64,
}

crate::action_command! {
    /// Drain a persona's inbox into one replay-stable turn frame: consolidate the bounded
    /// room slice, derive the RAG seed + captured prompt, and persist the record for
    /// prod-replay. Returns the record, or `null` when the drain window was empty (no-op).
    /// Substrate cognition IPC the host persona loop drives; not a persona toolbelt verb.
    pub struct DrainTurnFrame { state: Arc<CognitionState> }
    name: "persona/drain-turn-frame",
    access: Internal,
    params: DrainTurnFrameParams,
    output: Option<PersonaTurnFrameReplayRecord>,
    run(this, _ctx, params) => {
        let _timer = TimingGuard::new("module", "persona_drain_turn_frame");

        let max_items = usize::try_from(params.max_items)
            .map_err(|_| CommandError::Invalid(format!("max_items too large: {}", params.max_items)))?;

        let persona = this
            .state
            .personas
            .get(&params.persona_id)
            .ok_or_else(|| CommandError::Invalid(format!("No cognition for {}", params.persona_id)))?;

        // Drain the inbox into a raw frame, then record it for observability.
        let raw_frame = persona.inbox.drain_frame(params.window_ms, max_items);
        record_drained_turn_frame(&raw_frame);

        // Wrap + populate derived outputs. None = empty drain; returned as JSON null.
        let record = match raw_frame {
            Some(inbox_frame) => {
                PersonaTurnFrame::from_inbox_frame(inbox_frame).replay_record()
            }
            None => None,
        };

        // Persist the record to ~/.continuum/replay/ for prod-replay ("FROM PROD not POC").
        if let Some(ref rec) = record {
            crate::persona::recorder::record_turn_frame_replay(rec);
        }

        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. drain-turn-frame is host-driven
    // substrate IPC (the persona loop drains its own inbox), so it stays Internal —
    // registered and grid-routable, never a remote-callable persona toolbelt verb. A
    // regression renaming the wire path or widening access is caught here.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(DrainTurnFrame::NAME, "persona/drain-turn-frame");
        assert_eq!(DrainTurnFrame::ACCESS, AccessLevel::Internal);
    }

    // what this catches: the window/max defaults survive an absent-field payload. The
    // legacy arm read `u64_or(.., 80)` / `u64_or(.., 16)`; a caller sending only
    // `{ personaId }` must still get the 80ms / 16-item frame bounds, not a deserialize
    // error. Guards the serde(default) wiring that replaced the u64_or reads.
    #[test]
    fn defaults_fill_absent_frame_bounds() {
        let id = Uuid::nil();
        let params: DrainTurnFrameParams =
            serde_json::from_value(serde_json::json!({ "personaId": id })).unwrap();
        assert_eq!(params.window_ms, 80);
        assert_eq!(params.max_items, 16);
    }
}
