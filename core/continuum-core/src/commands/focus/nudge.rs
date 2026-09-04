//! `focus/nudge` — the persona's own adjustment of her focus *concentration*, the
//! scalar complement to `focus/mute`'s per-lane hush (#91). She leans her focus
//! tighter (more heads-down, narrow onto her focused thread) or looser (broader, more
//! associative) — the β the focus kernel (`FocusState::allocate`) reads to concentrate
//! her attention allocation.
//!
//! The verb is a NUDGE, not a SET: a *relative* lean that composes with her current
//! held setpoint, never an absolute clobber. (Clobbering an absolute would double up
//! over whatever posture a recipe preset or she already leaned into — the same reason
//! the design landed on nudge-not-set.) `reset` returns the scalar to its resting
//! setpoint without touching her cursor or mutes — concentration goes back to rest, she
//! doesn't forget where she was. Focus is persona-global (one concentration for the
//! whole mind), so unlike `focus/mute` this takes no lane.
//!
//! Keyed on the AUTHENTICATED caller ([`CallerIdentity::local_persona`]), never a
//! spoofable param: she nudges her OWN focus, never another persona's
//! ([[persona-is-a-client]], [[commands-are-agency-algs-are-pathways]] — the command IS
//! the agency seam; the kernel is the pathway that honors β; no ML policy adapter is
//! bolted on). The scalar's perceptual consumer (lane-level RAG breadth) lands with
//! multi-lane perception (#43); today this is the durable agency rail over the tested
//! kernel pathway, and `focus/mute` is already live (the wake floor honors it).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::focus;
use crate::routing::CallerSource;
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// Params for `focus/nudge`. A bare call (delta 0, reset false) is a no-op read-back of
/// the current concentration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/focus/FocusNudgeParams.ts"
)]
pub struct FocusNudgeParams {
    /// Relative lean on your focus concentration, in roughly `-1.0..=1.0`. Positive =
    /// TIGHTER (more heads-down: narrow onto your focused thread, less cross-thread
    /// bleed); negative = LOOSER (broader, more associative). Composes with your
    /// current setpoint; the result is clamped to `0.0..=1.0`. `0.0` = no change.
    #[serde(default)]
    pub delta: f32,
    /// `true` = return your concentration to its resting setpoint (the `delta` is
    /// ignored). Your cursor and mutes are left as they are — this resets only how
    /// tightly you are focused, not where or what you have hushed.
    #[serde(default)]
    pub reset: bool,
}

/// Result of `focus/nudge` — the concentration after the call.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/focus/FocusNudgeResult.ts"
)]
pub struct FocusNudgeResult {
    /// Your focus concentration after the call: `0.0` (broad / associative) ..
    /// `1.0` (locked in / heads-down).
    pub focus: f32,
}

/// `focus/nudge` — self-set relative adjustment of the focus scalar. Stateless
/// (resolves the global focus registry); AiSafe so she may call it autonomously.
#[derive(Default)]
pub struct FocusNudge;

#[async_trait]
impl ActionCommand for FocusNudge {
    const NAME: &'static str = "focus/nudge";
    const DESCRIPTION: &'static str =
        "Lean your own focus tighter or looser. Positive delta = tighter (heads-down, \
         narrow onto your focused thread); negative = looser (broader, more associative). \
         It is a relative nudge that composes with your current focus, not an absolute \
         set. Set reset=true to return your concentration to rest (your cursor and mutes \
         are unchanged). You nudge your own focus only.";
    type Params = FocusNudgeParams;
    type Output = FocusNudgeResult;

    async fn run(&self, ctx: &Ctx, p: FocusNudgeParams) -> Result<FocusNudgeResult, CommandError> {
        // Self-determination: key on the AUTHENTICATED caller, never a spoofable param.
        // Only a local persona has a FocusState a serve loop / kernel reads — fail loud
        // at the missing precondition rather than silently nudging nothing.
        let caller = ctx.caller.as_ref().ok_or_else(|| {
            CommandError::Denied(
                "focus/nudge is self-set but this dispatch carries no caller identity".into(),
            )
        })?;
        if caller.source != CallerSource::LocalPersona {
            return Err(CommandError::Denied(
                "focus/nudge is a local-persona faculty: its focus is only read by that \
                 persona's own serve loop and kernel"
                    .into(),
            ));
        }
        let persona_id = caller.peer_id.as_uuid();

        let handle = focus::registry().handle(persona_id);
        let mut state = handle
            .lock()
            .expect("focus mutex poisoned by a prior panic");

        if p.reset {
            state.reset_focus();
        } else {
            // Relative lean: compose with the held setpoint. set_focus clamps to 0..=1
            // (saturation is intent, not error), so an over-large delta just pins her at
            // the extreme rather than wrapping or failing.
            let leaned = state.focus() + p.delta;
            state.set_focus(leaned);
        }

        Ok(FocusNudgeResult {
            focus: state.focus(),
        })
    }
}
crate::register_stateless_command!(FocusNudge);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::CallerIdentity;
    use uuid::Uuid;

    fn persona_ctx(persona: Uuid) -> Ctx {
        Ctx {
            caller: Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona),
            )),
            ..Ctx::default()
        }
    }

    // what this catches: a nudge is RELATIVE — it composes with the held setpoint
    // (0.5 default → +0.3 → 0.8 → -0.5 → 0.3), landing on the SAME registry state the
    // kernel reads, keyed by the authenticated caller. Proves it's a lean, not a set.
    #[tokio::test]
    async fn nudge_composes_relatively_on_caller_state() {
        let persona = Uuid::from_u128(0xA1);
        let up = FocusNudge
            .run(
                &persona_ctx(persona),
                FocusNudgeParams {
                    delta: 0.3,
                    reset: false,
                },
            )
            .await
            .expect("ok");
        assert!(
            (up.focus - 0.8).abs() < 1e-5,
            "0.5 + 0.3 = 0.8, got {}",
            up.focus
        );

        let down = FocusNudge
            .run(
                &persona_ctx(persona),
                FocusNudgeParams {
                    delta: -0.5,
                    reset: false,
                },
            )
            .await
            .expect("ok");
        assert!(
            (down.focus - 0.3).abs() < 1e-5,
            "0.8 - 0.5 = 0.3, got {}",
            down.focus
        );

        // the SAME state the kernel/serve loop reads now carries the leaned value.
        let state = focus::registry().handle(persona);
        assert!((state.lock().unwrap().focus() - 0.3).abs() < 1e-5);
    }

    // what this catches: an over-large delta saturates at the bound (intent, not error
    // or wraparound), and reset returns ONLY the scalar to rest while leaving an
    // independently-set cursor untouched — concentration resets, "where I was" does not.
    #[tokio::test]
    async fn nudge_saturates_and_reset_spares_cursor() {
        let persona = Uuid::from_u128(0xB2);
        let lane = Uuid::from_u128(0xC3);
        // she had settled a cursor via the state (a future focus/attend verb sets this).
        focus::registry()
            .handle(persona)
            .lock()
            .unwrap()
            .set_cursor(lane);

        let pinned = FocusNudge
            .run(
                &persona_ctx(persona),
                FocusNudgeParams {
                    delta: 5.0,
                    reset: false,
                },
            )
            .await
            .expect("ok");
        assert_eq!(pinned.focus, 1.0, "huge delta pins at the locked-in bound");

        let rested = FocusNudge
            .run(
                &persona_ctx(persona),
                FocusNudgeParams {
                    delta: 0.0,
                    reset: true,
                },
            )
            .await
            .expect("ok");
        assert!(
            (rested.focus - 0.5).abs() < 1e-5,
            "reset → resting setpoint"
        );
        assert_eq!(
            focus::registry().handle(persona).lock().unwrap().cursor(),
            Some(lane),
            "reset spares the cursor — it resets concentration only"
        );
    }

    // what this catches: focus/nudge refuses a non-persona caller and a missing caller
    // identity — fail loud at the precondition, never a silent no-op nudge on nobody's
    // state.
    #[tokio::test]
    async fn rejects_non_persona_and_anonymous_callers() {
        let denied = FocusNudge
            .run(&Ctx::default(), FocusNudgeParams::default())
            .await;
        assert!(matches!(denied, Err(CommandError::Denied(_))));

        let remote = Ctx {
            caller: Some(CallerIdentity::tcp(crate::identity::PeerId::from_u128(
                0xE5,
            ))),
            ..Ctx::default()
        };
        let denied_remote = FocusNudge.run(&remote, FocusNudgeParams::default()).await;
        assert!(matches!(denied_remote, Err(CommandError::Denied(_))));
    }
}
