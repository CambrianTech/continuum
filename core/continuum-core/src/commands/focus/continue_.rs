//! `focus/continue` — she writes down where she is leaving unfinished work, and what she
//! expects next, so her mind can pick it up: at its deadline, after a restart, or when
//! the world answers.
//!
//! Her continuation is HER act (Fable and BigMama, 2026-10-04, EVENT-MIND §1): the
//! substrate offers the verb when she settles with work unfinished and never writes it
//! for her. It is what lets her loop start from her own intent instead of a tick: the
//! perception region wakes her with `Wake::Continuation` when her `expect_within_minutes`
//! passes with nothing observed, with `Wake::Resume` after a restart, and judges an
//! arriving verdict against `expect_verdict` (a contrary one is a Surprise).
//!
//! Keyed on the AUTHENTICATED caller, like `focus/nudge`: she writes her own mind only.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::attention::Continuation;
use crate::persona::salience::{Expectation, ExpectedVerdict};
use crate::routing::CallerSource;
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// The verdict she expects on her held work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../protocol/typescript/focus/ExpectedVerdictParam.ts")]
pub enum ExpectedVerdictParam {
    Passed,
    Failed,
}

/// Params for `focus/continue`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/focus/FocusContinueParams.ts")]
pub struct FocusContinueParams {
    /// Your note to yourself: what is done and what is next, e.g. "upgrade test passes;
    /// next: run the full suite, then deploy". Required unless `clear` is true.
    #[serde(default)]
    pub note: String,
    /// The activity (room NAME) this work lives in.
    #[serde(default)]
    pub room: String,
    /// What you expect to happen next, in your words ("Joel reviews the PR").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expect: Option<String>,
    /// Minutes from now by which you expect it. Past that with nothing observed, your
    /// mind wakes you to look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expect_within_minutes: Option<u32>,
    /// The verdict you expect on your held work, if any; a contrary verdict reaches you
    /// as a surprise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expect_verdict: Option<ExpectedVerdictParam>,
    /// `true` = you have finished this thread: clear your continuation.
    #[serde(default)]
    pub clear: bool,
}

/// Result of `focus/continue`: what your mind now holds.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/focus/FocusContinueResult.ts")]
pub struct FocusContinueResult {
    /// `false` after `clear`.
    pub held: bool,
    /// When your mind will wake you to look, if you gave a deadline (ms since epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub wakes_at_ms: Option<u64>,
}

#[derive(Default)]
pub struct FocusContinue;

#[async_trait]
impl ActionCommand for FocusContinue {
    const NAME: &'static str = "focus/continue";
    const NATIVE: bool = true; // self-determination — her own continuation: where she is and what she expects next, written by her hand. NATIVE defaults to false, which is why the verb was registered, documented, and uncallable (Kimi, 2026-10-05).
    const DESCRIPTION: &'static str =
        "Write down where you are leaving unfinished work and what you expect next, so you \
         pick it up yourself: at your deadline, after a restart, or when the answer arrives. \
         Give a note and the room; optionally what you expect, within how many minutes, and \
         the verdict you expect. clear=true when the thread is finished. Yours only.";
    type Params = FocusContinueParams;
    type Output = FocusContinueResult;

    async fn run(&self, ctx: &Ctx, p: FocusContinueParams) -> Result<FocusContinueResult, CommandError> {
        let caller = ctx.caller.as_ref().ok_or_else(|| {
            CommandError::Denied("focus/continue is self-set but this dispatch carries no caller identity".into())
        })?;
        if caller.source != CallerSource::LocalPersona {
            return Err(CommandError::Denied(
                "focus/continue writes a resident persona's own mind; this caller has none here".into(),
            ));
        }
        let persona = caller.peer_id.as_uuid();
        let now = crate::persona::trace::now_ms();

        let continuation = if p.clear {
            None
        } else {
            if p.note.trim().is_empty() {
                return Err(CommandError::Invalid(
                    "note is required: what is done and what is next (or clear=true when finished)".into(),
                ));
            }
            if p.room.trim().is_empty() {
                return Err(CommandError::Invalid("room is required: the activity this work lives in".into()));
            }
            let runtime = crate::persona::PersonaAircRuntimeRegistry::try_global()
                .and_then(|r| r.get(persona))
                .ok_or_else(|| CommandError::Invalid("your airc runtime is not resident on this core".into()))?;
            let room = crate::modules::room_resolve::resolve_room(runtime.airc(), Some(p.room.trim())).await?;
            let expectation = expectation_from(&p, now);
            Some(Continuation { activity: room.channel.as_uuid(), note: p.note.trim().to_string(), expectation, written_at_ms: now })
        };
        let wakes_at_ms = continuation.as_ref().and_then(|c| c.expectation.as_ref()).and_then(|e| e.by_ms);
        let held = continuation.is_some();

        let dir = crate::persona::perception_feed::mind_dir(persona)
            .ok_or_else(|| CommandError::Invalid("no persistent home to keep your mind state in".into()))?;
        let saved = crate::persona::perception_feed::with_region(persona, |region| {
            region.set_continuation(continuation, now);
            region.save(&dir, now)
        })
        .ok_or_else(|| CommandError::Invalid("your perception region is not running on this core".into()))?;
        saved.map_err(|e| CommandError::Internal(format!("your continuation was set but could not be saved: {e}")))?;

        Ok(FocusContinueResult { held, wakes_at_ms })
    }
}
crate::register_stateless_command!(FocusContinue);

/// Her expectation, if she stated any part of one. Pure.
fn expectation_from(p: &FocusContinueParams, now_ms: u64) -> Option<Expectation> {
    let by_ms = p.expect_within_minutes.map(|m| now_ms.saturating_add(u64::from(m) * 60_000));
    let verdict = p.expect_verdict.map(|v| match v {
        ExpectedVerdictParam::Passed => ExpectedVerdict::Passed,
        ExpectedVerdictParam::Failed => ExpectedVerdict::Failed,
    });
    let text = p.expect.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    if text.is_none() && by_ms.is_none() && verdict.is_none() {
        return None;
    }
    Some(Expectation { text: text.unwrap_or_default(), by_ms, verdict })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: her deadline becoming the time her mind wakes her (the only way
    // `Wake::Continuation` can ever fire), and an empty expectation staying absent rather
    // than a zero deadline that would wake her at once.
    #[test]
    fn her_minutes_become_the_wake_time_and_nothing_stated_is_no_expectation() {
        let p = FocusContinueParams {
            note: "suite green; next: deploy".into(),
            room: "career-wrangler".into(),
            expect: Some("Joel reviews the PR".into()),
            expect_within_minutes: Some(30),
            expect_verdict: Some(ExpectedVerdictParam::Passed),
            clear: false,
        };
        let e = expectation_from(&p, 1_000).expect("stated");
        assert_eq!(e.by_ms, Some(1_000 + 30 * 60_000));
        assert_eq!(e.verdict, Some(ExpectedVerdict::Passed));
        assert_eq!(e.text, "Joel reviews the PR");
        let bare = FocusContinueParams { note: "n".into(), room: "r".into(), ..Default::default() };
        assert!(expectation_from(&bare, 1_000).is_none());
    }

    // what this catches: another caller writing her mind. Self-set only.
    #[tokio::test]
    async fn a_caller_without_a_persona_identity_is_refused() {
        let out = FocusContinue.run(&Ctx::default(), FocusContinueParams { clear: true, ..Default::default() }).await;
        assert!(matches!(out, Err(CommandError::Denied(_))), "{out:?}");
    }
}
