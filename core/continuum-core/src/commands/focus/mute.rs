//! `focus/mute` — the persona's own per-lane hush, the surgical complement to the
//! focus scalar (#91). She mutes a noisy thread/channel for herself; the never-stop
//! serve loop's wake floor (`FocusState::wakes_on`) honors it on the very next slice.
//!
//! SOFT (default) turns part-way from a lane's ambient chatter; HARD turns fully from
//! it (the allocation kernel pools no ambient attention there). NEITHER blinds her: a
//! direct address pierces every level — the inviolable interrupt floor. Mute is
//! attention allocation, never sensory shutoff: "I don't turn off my eyes and ears." A
//! snooze (`durationSecs`) auto-expires so ambient awareness self-restores without her
//! having to remember to un-mute — a mute can never silently calcify into neglect.
//!
//! Keyed on the AUTHENTICATED caller ([`CallerIdentity::local_persona`], stamped onto
//! her tool connection by `CommandToolExecutor::for_persona`), never a spoofable
//! param: she mutes her OWN lanes, never another persona's
//! ([[persona-is-a-client]], [[focus-is-self-allocation-not-siloing]]). This is the
//! decision half of "navigate away without going numb" — she owns where her attention
//! turns; the substrate still guarantees perception of a direct address.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::persona::focus::{self, MuteLevel};
use crate::persona::recall_metadata::now_ms;
use crate::routing::CallerSource;
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// Params for `focus/mute`. All fields optional — the bare call soft-mutes the room
/// she is currently acting in, held until she un-mutes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/focus/FocusMuteParams.ts"
)]
pub struct FocusMuteParams {
    /// The lane (room / thread / channel id) to act on. Omit to target the room you
    /// are currently acting in (this turn's context).
    #[ts(optional)]
    #[ts(type = "string")]
    pub lane: Option<Uuid>,
    /// `false` (default) = soft: turn part-way from the lane's ambient chatter. `true`
    /// = hard: turn fully from it (no ambient attention pooled there). Either way a
    /// direct address still reaches you — the inviolable interrupt floor; mute is
    /// attention allocation, never going deaf to the lane. Pair a hard mute with
    /// `durationSecs` so your ambient awareness self-restores.
    #[serde(default)]
    pub hard: bool,
    /// Seconds until the mute auto-expires (a snooze). Omit to hold the mute until
    /// you `unmute`. Prefer a duration with a hard mute so awareness self-restores.
    #[ts(optional)]
    #[ts(type = "number")]
    pub duration_secs: Option<u64>,
    /// `true` = clear any mute on the lane immediately (the other fields are ignored).
    #[serde(default)]
    pub unmute: bool,
}

/// Result of `focus/mute` — the lane's mute posture after the call.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/focus/FocusMuteResult.ts"
)]
pub struct FocusMuteResult {
    /// The lane acted on (resolved from `lane` or the turn's context).
    #[ts(type = "string")]
    pub lane: Uuid,
    /// The active level after the call: `"soft"`, `"hard"`, or `"none"` (unmuted).
    pub level: String,
    /// Unix-ms when the snooze auto-expires, if a duration was set.
    #[ts(optional)]
    #[ts(type = "number")]
    pub expires_at_ms: Option<u64>,
}

/// `focus/mute` — self-set per-lane hush. Stateless (resolves the global focus
/// registry); AiSafe so she may call it autonomously.
#[derive(Default)]
pub struct FocusMute;

#[async_trait]
impl ActionCommand for FocusMute {
    const NAME: &'static str = "focus/mute";
    const DESCRIPTION: &'static str =
        "Hush a noisy thread or channel for yourself. Soft (default) silences ambient \
         chatter but a direct address still reaches you; set hard=true to silence \
         everything. Set durationSecs to snooze (awareness auto-restores when it \
         lapses); set unmute=true to clear it. Omit lane to act on the room you're in. \
         You mute your own lanes only.";
    type Params = FocusMuteParams;
    type Output = FocusMuteResult;

    async fn run(&self, ctx: &Ctx, p: FocusMuteParams) -> Result<FocusMuteResult, CommandError> {
        // Self-determination: key on the AUTHENTICATED caller, never a spoofable
        // param. Only a local persona has a FocusState a serve loop reads — fail loud
        // at the missing precondition rather than silently muting nothing.
        let caller = ctx.caller.as_ref().ok_or_else(|| {
            CommandError::Denied(
                "focus/mute is self-set but this dispatch carries no caller identity".into(),
            )
        })?;
        if caller.source != CallerSource::LocalPersona {
            return Err(CommandError::Denied(
                "focus/mute is a local-persona faculty: its mute is only honored by that \
                 persona's own serve loop"
                    .into(),
            ));
        }
        let persona_id = caller.peer_id.as_uuid();

        // Default the lane to the room she's acting in (the turn's context). Fail loud
        // if neither is available — a mute with no lane is meaningless.
        let lane = p.lane.or(ctx.context_id).ok_or_else(|| {
            CommandError::Invalid(
                "focus/mute needs a lane: none supplied and this turn carries no room context"
                    .into(),
            )
        })?;

        let handle = focus::registry().handle(persona_id);
        let mut state = handle
            .lock()
            .expect("focus mutex poisoned by a prior panic");

        if p.unmute {
            state.unmute(lane);
            return Ok(FocusMuteResult {
                lane,
                level: "none".into(),
                expires_at_ms: None,
            });
        }

        let level = if p.hard {
            MuteLevel::Hard
        } else {
            MuteLevel::Soft
        };
        let expires_at_ms = p
            .duration_secs
            .map(|s| now_ms().saturating_add(s.saturating_mul(1_000)));
        state.mute(lane, level, expires_at_ms);

        Ok(FocusMuteResult {
            lane,
            level: if p.hard { "hard" } else { "soft" }.into(),
            expires_at_ms,
        })
    }
}
crate::register_stateless_command!(FocusMute);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::CallerIdentity;

    fn persona_ctx(persona: Uuid, room: Uuid) -> Ctx {
        Ctx {
            caller: Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona),
            )),
            context_id: Some(room),
            ..Ctx::default()
        }
    }

    // what this catches: a self-set soft mute lands on the AUTHENTICATED caller's
    // focus state (keyed by persona_id, not a param), defaults its lane to the turn's
    // room, and the registry-shared state then reports a Soft active mute — the seam
    // the serve loop's wake floor reads. unmute clears it.
    #[tokio::test]
    async fn soft_mute_defaults_to_room_and_lands_on_caller() {
        let persona = Uuid::from_u128(0xA1);
        let room = Uuid::from_u128(0xB2);
        let out = FocusMute
            .run(&persona_ctx(persona, room), FocusMuteParams::default())
            .await
            .expect("ok");
        assert_eq!(out.lane, room, "lane defaulted to the turn's context");
        assert_eq!(out.level, "soft");
        assert_eq!(out.expires_at_ms, None, "no duration → held mute");

        // The SAME state the serve loop reads now carries the mute.
        let state = focus::registry().handle(persona);
        assert_eq!(
            state.lock().unwrap().active_mute(room, now_ms()),
            Some(MuteLevel::Soft),
        );

        let cleared = FocusMute
            .run(
                &persona_ctx(persona, room),
                FocusMuteParams {
                    unmute: true,
                    ..Default::default()
                },
            )
            .await
            .expect("ok");
        assert_eq!(cleared.level, "none");
        assert_eq!(state.lock().unwrap().active_mute(room, now_ms()), None);
    }

    // what this catches: a hard snooze sets a future expiry and suppresses AMBIENT wake
    // while active, yet a direct address still pierces it (never numb) and ambient
    // awareness self-restores once it lapses — the "navigate away, never numb, bounded"
    // contract, end to end through the command.
    #[tokio::test]
    async fn hard_snooze_sets_expiry_and_preserves_the_floor() {
        let persona = Uuid::from_u128(0xC3);
        let lane = Uuid::from_u128(0xD4);
        let before = now_ms();
        let out = FocusMute
            .run(
                // explicit lane (no room context) to prove `lane` overrides the default
                &Ctx {
                    caller: Some(CallerIdentity::local_persona(
                        crate::identity::PeerId::from_uuid(persona),
                    )),
                    ..Ctx::default()
                },
                FocusMuteParams {
                    lane: Some(lane),
                    hard: true,
                    duration_secs: Some(60),
                    unmute: false,
                },
            )
            .await
            .expect("ok");
        assert_eq!(out.level, "hard");
        let expiry = out.expires_at_ms.expect("snooze has an expiry");
        assert!(expiry >= before + 60_000, "expiry ~= now + 60s");

        let state = focus::registry().handle(persona);
        // hard mute suppresses AMBIENT wake while active...
        assert!(!state.lock().unwrap().wakes_on(lane, false, before + 1_000));
        // ...but a direct address still pierces it — never numb...
        assert!(state.lock().unwrap().wakes_on(lane, true, before + 1_000));
        // ...and ambient awareness self-restores once the snooze lapses.
        assert!(state.lock().unwrap().wakes_on(lane, false, expiry + 1));
    }

    // what this catches: focus/mute refuses a non-persona caller (it's a local-persona
    // faculty) and a missing caller identity — fail loud at the precondition, never a
    // silent no-op mute.
    #[tokio::test]
    async fn rejects_non_persona_and_anonymous_callers() {
        // no caller at all
        let denied = FocusMute
            .run(&Ctx::default(), FocusMuteParams::default())
            .await;
        assert!(matches!(denied, Err(CommandError::Denied(_))));

        // a non-persona caller (e.g. a remote peer) is denied even though AiSafe lets
        // it reach the body.
        let remote = Ctx {
            caller: Some(CallerIdentity::tcp(crate::identity::PeerId::from_u128(
                0xE5,
            ))),
            context_id: Some(Uuid::from_u128(0xF6)),
            ..Ctx::default()
        };
        let denied_remote = FocusMute.run(&remote, FocusMuteParams::default()).await;
        assert!(matches!(denied_remote, Err(CommandError::Denied(_))));
    }
}
