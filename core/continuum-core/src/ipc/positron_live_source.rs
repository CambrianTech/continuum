//! Live-call emitter — folds the TRANSPORT's calls and the ORCHESTRATOR's
//! registered sessions into one [`LiveCallViewState`], on a fixed cadence, for
//! every renderer at once (#58).
//!
//! ### Why this folds two sources instead of one
//!
//! Every sibling emitter here projects a single truth. This one deliberately
//! reads both `CallManager::live_calls()` and `VoiceService::registered_sessions()`
//! because their DISAGREEMENT is the thing worth rendering.
//!
//! Measured 2026-08-07: session registration is CLIENT-driven. `CallServer`
//! knows a call started (`join_call`) and holds the call id — which IS the airc
//! room id — and never tells the orchestrator. Every path into
//! `register_session` is a client calling in, so that orchestration only ever
//! existed in 162 lines of Node in `legacy/`. iOS, Android and TUI citizens are
//! therefore structurally voiceless, not merely buggy.
//!
//! The legacy code names the symptom itself: *"Without this, `isInCall()`
//! returns false and AI responses are silently dropped."* That is the reported
//! "personas are static — not animating, talking, hearing, seeing".
//!
//! So this emitter does not wait for the wire to be fixed to be useful. Until a
//! call registers itself, it renders live-and-unregistered — which makes the
//! defect a visible fact on web, iOS, Android and TUI simultaneously instead of
//! a mystery each client rediscovers. Absence over fabrication, exactly as
//! `positron_serving_source` does.

use std::sync::Arc;
use std::time::Duration;

use continuum_positron::live::{LiveCallView, LiveCallViewState, LiveParticipantView};
use continuum_positron::state::StateBuilder;
use continuum_positron::substrate::Substrate;

use crate::live::session::voice_service::VoiceService;
use crate::live::transport::call_server::CallManager;

/// Cadence. A call's membership changes on human timescales (someone joins,
/// someone drops), so a 1s tick is already far finer than the thing it watches —
/// fast enough that "she is not registered" surfaces while the call is still
/// happening, cheap enough that two lock snapshots per second are noise.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

pub fn spawn_live_call_emitter(
    rt: &tokio::runtime::Handle,
    substrate: Substrate,
    calls: Arc<CallManager>,
    voice: Arc<VoiceService>,
) {
    rt.spawn(async move {
        // Sole writer of the "live-call" kind → its own standalone Revisions well.
        let builder = StateBuilder::standalone();
        let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
        loop {
            ticker.tick().await;

            let live = calls.live_calls().await;
            let registered = voice.registered_sessions();

            let calls_view: Vec<LiveCallView> = live
                .iter()
                .map(|(call_id, transport_n)| {
                    // session_id == room_id == call_id — one identity, enforced
                    // fail-loud in call_server.rs (#193 slice B). So a call is
                    // "registered" exactly when the orchestrator holds a session
                    // under the same id; no translation table, no second key.
                    let session = registered
                        .iter()
                        .find(|(sid, _)| sid.to_string() == *call_id);
                    let participants: Vec<LiveParticipantView> = session
                        .map(|(_, ps)| {
                            ps.iter()
                                .map(|p| LiveParticipantView {
                                    user_id: p.user_id.to_string(),
                                    display_name: p.display_name.clone(),
                                    kind: format!("{:?}", p.participant_type).to_lowercase(),
                                    audio_native: p.is_audio_native,
                                    // Registered by construction here: this branch
                                    // only runs when the orchestrator holds them.
                                    audio_registered: true,
                                })
                                .collect()
                        })
                        // NO SESSION = the #58 state. The transport has people in
                        // this call and the core cannot route to any of them. We
                        // deliberately do NOT invent participant rows we cannot
                        // name — the empty list plus `registered: false` plus the
                        // transport count is the honest statement: "N people are in
                        // here and the core knows none of them."
                        .unwrap_or_default();
                    LiveCallView {
                        call_id: call_id.clone(),
                        participants,
                        transport_participants: *transport_n as u32,
                        registered: session.is_some(),
                    }
                })
                .collect();

            // The mirror-image leak: a session the orchestrator still holds for a
            // call that has ended. Surfaced rather than swept, because it keeps a
            // persona "in" something that is over — the same class of stale-state
            // attractor as a work claim whose lease lapsed.
            let orphaned_sessions: Vec<String> = registered
                .iter()
                .map(|(sid, _)| sid.to_string())
                .filter(|sid| !live.iter().any(|(cid, _)| cid == sid))
                .collect();

            let view = LiveCallViewState {
                calls: calls_view,
                orphaned_sessions,
                sample_interval_ms: SAMPLE_INTERVAL.as_millis() as u64,
            };
            substrate.store(builder.session(view));
        }
    });
}
