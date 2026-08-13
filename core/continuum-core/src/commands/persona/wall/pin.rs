//! `persona/wall/pin` — publish (or supersede) a post on a room's shared board
//! through a live persona's airc citizen.
//!
//! This is the WRITE face of the dual-faced wall: humans + widgets + personas
//! pin operating docs (plan / rules / agenda / principles / a recipe) to the
//! room's board; personas READ the same posts as concise grounding via
//! [`WallSource`](crate::persona::wall_source::WallSource). Both sides touch the
//! EXACT same airc `wall_posts` rows — there is no continuum-side copy of the
//! board (`publish_wall_post` emits a `WallPostPublished` substrate event; the
//! reader scans the transcript). A persona curating the board she lives in is
//! normal citizen behavior — she shapes the shared layer she is grounded in.
//!
//! Dep-holding: captures the module's shared
//! [`PersonaAircRuntimeRegistry`](crate::persona::PersonaAircRuntimeRegistry), so
//! it publishes through the SAME live citizen `persona/instances/*` act on.
//!
//! ## Edits are supersedes, never mutations
//!
//! airc's wall is append-only: an edit publishes a NEW post carrying
//! `supersedes = Some(prior_post_id)`; the projection drops the prior version
//! from the live board but keeps it in the transcript as an audit trail. Pass
//! `supersedes` to replace an earlier post; omit it to pin a fresh one.
//!
//! ## Fail loud
//!
//! Mal-formed `persona_id`/`supersedes` ⇒ [`CommandError::Invalid`]. A
//! well-formed `persona_id` that is not online ⇒ [`CommandError::NotFound`]. An
//! airc publish failure ⇒ [`CommandError::Internal`] naming the airc error — the
//! post is never silently dropped.
//!
//! ## Gating
//!
//! `Privileged` — pinning shapes the shared operating context for EVERY citizen
//! in the room, so it is a trusted-citizen action, not a freely-AiSafe one any
//! outsider agent can invoke.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::CommandError;

/// Which persona's citizen publishes the post, and the post itself.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaWallPinParams.ts")]
pub struct PersonaWallPinParams {
    /// The persona (airc peer_id Uuid, as in `persona/instances/list`) whose
    /// citizen publishes the post. The post lands on that citizen's current
    /// room's board. Fails loud if mal-formed or not currently online.
    pub persona_id: crate::identity::PersonaRef,
    /// Consumer-defined category label — common values: `plan`, `rules`,
    /// `agenda`, `principles`, `recipe`, `decision`. The substrate has no
    /// opinion on the string; `WallSource` renders it as the per-post header
    /// inside the `[room-board]` grounding block.
    pub category: String,
    /// The post body, rendered verbatim (markdown or JSON — never parsed).
    pub body: String,
    /// When replacing an earlier post, its `post_id` (as returned by a prior
    /// pin). The new post supersedes it on the live board; the prior version
    /// stays in the transcript. Omit to pin a fresh post.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub supersedes: Option<String>,
}

/// The published post's identity, echoed so the caller can later supersede it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaWallPinResult.ts")]
pub struct PersonaWallPinResult {
    /// The new post's `post_id` — pass this back as `supersedes` to edit it.
    pub post_id: String,
    /// The room the post landed on (the publishing citizen's default room).
    pub room_id: String,
    /// The category it was pinned under (echoed for confirmation).
    pub category: String,
}

crate::action_command! {
    /// Pin a post to a room's shared board through a persona's airc citizen — the
    /// WRITE face of the wall the persona READS as grounding. Pass `category` +
    /// `body`; pass `supersedes` with a prior `post_id` to edit (append-only —
    /// the old version stays in the transcript). Fails loud on a mal-formed or
    /// offline `persona_id`, or an airc publish error.
    pub struct PersonaWallPin {
        registry: PersonaAircRuntimeRegistry,
    }
    name: "persona/wall/pin",
    access: Privileged,
    params: PersonaWallPinParams,
    output: PersonaWallPinResult,
    run(this, _ctx, p) => {
        // #164: resolve the short/mistyped id a caller quotes back against the
        // personas this process knows — the ONE id_resolve primitive, same as
        // persona/identity/get. What a surface displays (8-char short form), its
        // verbs must accept.
        let persona_id =
            crate::id_resolve::resolve(p.persona_id.as_str(), &crate::persona::card::ids(), "persona")
                .map_err(CommandError::Invalid)?;
        let supersedes = match p.supersedes.as_deref() {
            Some(s) => Some(Uuid::parse_str(s).map_err(|e| {
                CommandError::Invalid(format!("supersedes '{s}' is not a valid post_id uuid: {e}"))
            })?),
            None => None,
        };
        let runtime = this.registry.get(persona_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "no persona with id {persona_id} is currently online — call persona/instances/list"
            ))
        })?;
        let post_id = runtime
            .airc()
            .publish_wall_post(p.category.clone(), p.body, supersedes)
            .await
            .map_err(|e| {
                CommandError::Internal(format!("airc publish_wall_post failed: {e}"))
            })?;
        Ok(PersonaWallPinResult {
            post_id: post_id.to_string(),
            room_id: runtime.default_room().as_uuid().to_string(),
            category: p.category,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: name/access wiring — pinning shapes the shared room
    // context, so it is Privileged, not AiSafe.
    #[test]
    fn name_and_access_wired() {
        use crate::sdk_codegen::{AccessLevel, ActionCommand};
        assert_eq!(PersonaWallPin::NAME, "persona/wall/pin");
        assert!(matches!(PersonaWallPin::ACCESS, AccessLevel::Privileged));
    }

    // what this catches: a mal-formed persona_id is rejected as Invalid before any
    // registry lookup or airc publish — a typo never silently pins nothing.
    #[tokio::test]
    async fn pin_with_malformed_persona_id_is_invalid() {
        use crate::sdk_codegen::{ActionCommand, Ctx};
        let cmd = PersonaWallPin {
            registry: PersonaAircRuntimeRegistry::new(),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                PersonaWallPinParams {
                    persona_id: "not-a-uuid".to_string().into(),
                    category: "plan".to_string(),
                    body: "x".to_string(),
                    supersedes: None,
                },
            )
            .await
            .expect_err("malformed id must fail loud");
        assert!(matches!(err, CommandError::Invalid(_)), "got {err:?}");
    }

    // what this catches: a mal-formed supersedes id is rejected as Invalid even
    // when the persona_id is well-formed — an edit pointer typo fails loud rather
    // than silently pinning a fresh post that loses the supersede link.
    #[tokio::test]
    async fn pin_with_malformed_supersedes_is_invalid() {
        use crate::sdk_codegen::{ActionCommand, Ctx};
        let cmd = PersonaWallPin {
            registry: PersonaAircRuntimeRegistry::new(),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                PersonaWallPinParams {
                    persona_id: Uuid::new_v4().to_string().into(),
                    category: "plan".to_string(),
                    body: "x".to_string(),
                    supersedes: Some("not-a-uuid".to_string()),
                },
            )
            .await
            .expect_err("malformed supersedes must fail loud");
        assert!(matches!(err, CommandError::Invalid(_)), "got {err:?}");
    }

    // what this catches: a well-formed but offline persona_id fails loud as
    // NotFound — pinning through a citizen that isn't online is surfaced, never
    // answered with a fabricated post_id.
    #[tokio::test]
    async fn pin_through_offline_persona_is_not_found() {
        use crate::sdk_codegen::{ActionCommand, Ctx};
        let cmd = PersonaWallPin {
            registry: PersonaAircRuntimeRegistry::new(),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                PersonaWallPinParams {
                    persona_id: Uuid::new_v4().to_string().into(),
                    category: "plan".to_string(),
                    body: "x".to_string(),
                    supersedes: None,
                },
            )
            .await
            .expect_err("offline persona must fail loud");
        assert!(matches!(err, CommandError::NotFound(_)), "got {err:?}");
    }
}
