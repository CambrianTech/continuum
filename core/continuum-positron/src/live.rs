//! Typed live-call glass-box payload — `LiveCallViewState`, the substrate-shaped
//! view of who is actually in a call and whether the core knows it.
//!
//! ### Why this projects BOTH sides (#58)
//!
//! Every other view here folds one source. This one deliberately folds two —
//! `CallManager`'s live calls AND `VoiceOrchestrator`'s registered sessions —
//! because the DIVERGENCE between them is the defect it exists to show.
//!
//! Measured 2026-08-07: session registration is CLIENT-driven. `CallServer`
//! knows a call started (`join_call`) and holds the call id, which IS the airc
//! room id, but never tells the orchestrator. Every path into
//! `register_session` is a client calling in, so the orchestration logic only
//! ever lived in 162 lines of Node in `legacy/` — presentation tier, now
//! retired. iOS, Android and TUI citizens are therefore not "buggy on voice",
//! they are structurally voiceless.
//!
//! The user-visible symptom is documented by the legacy code that prevented it:
//! *"Without this, `isInCall()` returns false and AI responses are silently
//! dropped."* A persona sits in the room, present, and says nothing — with
//! nothing anywhere stating why.
//!
//! So `audio_registered` is a FIELD, not an inference. A call that is live with
//! no matching registration renders as exactly that, in web + iOS + Android +
//! TUI simultaneously, instead of being a mystery each client rediscovers.
//! Honest-absence discipline, same as `ServingViewState`: absence over
//! fabrication, and an unknown is never dressed up as a ready.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One participant in a live call, and whether the CORE can actually reach her.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/LiveParticipantView.ts")]
pub struct LiveParticipantView {
    /// Stable id — a persona's citizen id, or a human's user id.
    pub user_id: String,
    /// Published display name. Falls back to the short id at the source; never
    /// "someone" — an unnamed participant must still be addressable.
    pub display_name: String,
    /// `"persona"` | `"human"`, as the orchestrator classified her.
    pub kind: String,
    /// Hears raw audio through the mixer rather than transcriptions. Affects
    /// whether she should receive text at all, so a renderer showing "listening"
    /// needs it to be truthful about HOW.
    pub audio_native: bool,
    /// THE FIELD THIS VIEW EXISTS FOR. True when the orchestrator holds a
    /// registration for this participant's session. False means she is in the
    /// call and the core cannot route audio to her — the silent-drop state,
    /// now a rendered fact instead of an invisible one.
    pub audio_registered: bool,
}

/// One call the transport is actually running.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/LiveCallView.ts")]
pub struct LiveCallView {
    /// The call id, which IS the airc room id — `session_id == room_id`, no
    /// parallel identity (#193 slice B, enforced fail-loud in `call_server.rs`).
    pub call_id: String,
    /// Participants the TRANSPORT has, whether or not the core registered them.
    pub participants: Vec<LiveParticipantView>,
    /// Transport participant count. Kept beside `participants.len()` on purpose:
    /// if they ever disagree, the projection is lying and a renderer can say so.
    #[ts(type = "number")]
    pub transport_participants: u32,
    /// True when the orchestrator holds a session for this call. FALSE is the
    /// #58 state — a live call the core does not know about, which is why a
    /// persona in it is silent.
    pub registered: bool,
}

/// The live-call glass box — what a call panel draws, on every interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/positron/LiveCallViewState.ts")]
pub struct LiveCallViewState {
    /// Calls the transport is running. EMPTY means no call is live — an honest
    /// nothing, never a fabricated idle state.
    pub calls: Vec<LiveCallView>,
    /// Sessions the orchestrator holds that have NO live call. The mirror-image
    /// leak of an unregistered call: a registration that outlived its call keeps
    /// a persona "in" something that ended. Surfaced rather than swept.
    pub orphaned_sessions: Vec<String>,
    /// Emitter cadence in ms, so a renderer labels its window from data instead
    /// of hardcoding a number that drifts from the source.
    #[ts(type = "number")]
    pub sample_interval_ms: u64,
}

impl LiveCallViewState {
    /// The on-wire `kind` this view publishes under. Open self-registration —
    /// deliberately a const per view, never a central enum every new panel has
    /// to be added to.
    pub const KIND: &'static str = "live-call";

    /// Calls that are live but unregistered — the #58 condition, named once so
    /// every renderer and every test asks the same question.
    pub fn unregistered(&self) -> impl Iterator<Item = &LiveCallView> {
        self.calls.iter().filter(|c| !c.registered)
    }
}

impl positron_core::ViewState for LiveCallViewState {
    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, registered: bool) -> LiveCallView {
        LiveCallView {
            call_id: id.to_string(),
            participants: vec![LiveParticipantView {
                user_id: "p1".into(),
                display_name: "Anwen".into(),
                kind: "persona".into(),
                audio_native: false,
                audio_registered: registered,
            }],
            transport_participants: 1,
            registered,
        }
    }

    // what this catches: THE reason this view folds two sources. A live call with
    // no registration must be visible AS THAT — it is the state where a persona is
    // present, isInCall() is false, and her responses are silently dropped. If this
    // ever renders as "no call" or as a healthy call, the projection has re-hidden
    // the defect it was built to expose.
    #[test]
    fn a_live_but_unregistered_call_is_visible_as_exactly_that() {
        let view = LiveCallViewState {
            calls: vec![call("room-a", true), call("room-b", false)],
            orphaned_sessions: vec![],
            sample_interval_ms: 1000,
        };
        let stuck: Vec<&str> = view.unregistered().map(|c| c.call_id.as_str()).collect();
        assert_eq!(stuck, vec!["room-b"], "the unregistered call is named");
        assert!(
            !view.calls.is_empty(),
            "a live call is never rendered as 'no call' just because the core missed it"
        );
        assert!(
            !view.calls[1].participants[0].audio_registered,
            "the participant carries the unreachable fact too — a renderer can mark HER, \
             not just the call"
        );
    }

    // what this catches: honest absence. No calls is an EMPTY list, which renders as
    // "nothing live" — never a fabricated idle call, and never confused with the
    // unregistered state above. Same discipline as ServingViewState's None header.
    #[test]
    fn no_calls_is_empty_not_invented() {
        let view = LiveCallViewState {
            calls: vec![],
            orphaned_sessions: vec![],
            sample_interval_ms: 1000,
        };
        assert_eq!(view.unregistered().count(), 0);
        assert!(view.calls.is_empty());
    }
}
