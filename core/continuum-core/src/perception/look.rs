//! `perception/look` — a persona's own eyes on the LIVE VIDEO CALL it is in.
//!
//! The sibling verb to [`perception/observe`](super) under the ONE Perception
//! Surface (#187, `docs/architecture/PERCEPTION-SURFACE.md`). Both are the same
//! conceptual model (a `Percept` a mind reasons over); they are DIFFERENT surface
//! implementations, exactly as the polymorphic `Surface` trait predicts:
//!
//! - `perception/observe` is the CREATED-ARTIFACT / DOM surface — `Provided`,
//!   routed to an eye-node adapter that renders a URL and probes a DOM tree. It has
//!   an Actuator; its `target` is a URL.
//! - `perception/look` is the LIVE-CALL VIDEO surface — the doc's "live video of
//!   Joel" observe-only row. `Bare`/substrate-served: it reads the persona's OWN
//!   in-process [`PerceptionBuffer`](crate::media::PerceptionBuffer) (the frames
//!   already flowing in via the LiveKit media ingest, #192), resolved by the
//!   CALLER's authenticated identity via [`perception_registry`]. No URL, no DOM
//!   tree, no external adapter — the frames are already here.
//!
//! It exposes the à la carte PULL side of live perception
//! ([`PerceptionBuffer::look`](crate::media::PerceptionBuffer::look)): "into their
//! vision à la carte" — a persona asks for a current image of ONE participant or
//! EVERYONE at once, at thumbnail or full detail, satisfied ASAP off the shared
//! compute-once cache. The ambient PUSH (the warm band + the description sensory
//! bridge) flows separately through `MediaPerceptionSource` under the RAG budget;
//! this verb is the deliberate ask.
//!
//! The convergence dividend: a look's pixels channel is the SAME
//! [`ObservedImage`](super::ObservedImage) an observe returns, so a persona — and
//! every renderer — sees one `Percept` shape from both eyes.
//!
//! Self-scoped like [`focus/mute`](crate::commands): the buffer key is the
//! AUTHENTICATED caller ([`CallerIdentity::local_persona`], stamped onto the tool
//! connection by `CommandToolExecutor::for_persona`), NEVER a spoofable param — a
//! persona looks through its OWN eyes, never another's ([[persona-is-a-client]]).
//! The registry lookup IS the boundary (no redundant source gate,
//! [[dont-stack-redundant-permission-gates]]); a caller with no buffer (not in a
//! call, or substrate/owner code) is told so plainly, never handed a fabricated
//! look ([[fallbacks-are-illegal-fail-loud]]).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ObservedImage;
use crate::media::{perception_registry, LookFidelity, LookImage, LookScope};
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// How much detail a look returns for each frame — the persona's fidelity knob.
/// `Thumbnail` is the cheap default (the ~480w ambient look, shared with the warm
/// band); `Full` is the raw frame for looking closely. A specific pixel size is a
/// governor concern (the attention-priced resolution field, #173), not something a
/// persona names — so the persona surface offers only these two intents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/LookDetail.ts")]
pub enum LookDetail {
    /// The cheap ~480w ambient look — a quick glance (default).
    #[default]
    Thumbnail,
    /// The full-resolution frame — for looking closely.
    Full,
}

/// What to look at in the live call — the à la carte request.
///
/// Omit `participant` to see EVERYONE at once (the contact-sheet gallery a human
/// sees in their call widget); name one to see just that person. Deliberately tiny:
/// this is the persona's own view, composed by intent, not a knob-laden API.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/LookParams.ts")]
pub struct LookParams {
    /// Who to look at — a live-call participant id. Omit to see EVERYONE at once (a
    /// contact sheet of every participant's current frame).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub participant: Option<String>,
    /// How much detail: `thumbnail` (default — a quick small look) or `full` (the
    /// full-resolution frame, for looking closely).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<LookDetail>,
}

/// One participant's current frame, projected for the wire — the pixels a persona
/// SEES. Reuses [`ObservedImage`] so a look and an observe deliver the identical
/// `Percept` pixels shape.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/LookView.ts")]
pub struct LookView {
    /// The participant this frame belongs to (the airc roster id).
    pub participant: String,
    /// Content address of the frame (sha256-hex) — a stable id for this exact image.
    pub content_hash: String,
    /// The rendered frame (the pixels to SEE), when it resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub image: Option<ObservedImage>,
    /// Failure reason when THIS participant's frame could not be produced — surfaced,
    /// never a fabricated image ([[fallbacks-are-illegal-fail-loud]]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// The result of `perception/look` — one view per participant looked at. BARE (like
/// [`ObserveResult`](super::ObserveResult)): it carries its own `success`/`error`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/LookResult.ts")]
pub struct LookResult {
    /// The look was satisfied (`views` are your current frames).
    pub success: bool,
    /// One view per participant: one for a named participant, the whole gallery for
    /// an everyone-look. Empty with `success == true` only if the call has no video
    /// flowing yet.
    pub views: Vec<LookView>,
    /// Why the look could NOT be satisfied (not in a call, unknown participant) —
    /// fail loud, never an empty success that reads as "I saw nothing".
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

impl LookResult {
    /// A satisfied look over the projected views.
    fn seen(views: Vec<LookView>) -> Self {
        Self {
            success: true,
            views,
            error: None,
        }
    }

    /// A look that could not happen — the honest, teaching failure (no fabricated
    /// image; explains what a persona CAN do instead).
    fn cannot(reason: impl Into<String>) -> Self {
        Self {
            success: false,
            views: Vec::new(),
            error: Some(reason.into()),
        }
    }
}

impl LookView {
    /// Project one satisfied [`LookImage`] onto the wire view — decoding an honest
    /// mime + dimensions from the SAME bytes so the view never claims a size or type
    /// the pixels don't have. An `Err` look becomes a surfaced `error`, never an image.
    fn from_look_image(img: LookImage) -> Self {
        match &*img.image {
            Ok(bytes) => {
                let (width, height, mime) = image_meta(bytes);
                LookView {
                    participant: img.participant,
                    content_hash: img.content_hash,
                    image: Some(ObservedImage {
                        data_url: Some(to_data_url(&mime, bytes)),
                        filepath: None,
                        width,
                        height,
                    }),
                    error: None,
                }
            }
            Err(e) => LookView {
                participant: img.participant,
                content_hash: img.content_hash,
                image: None,
                error: Some(e.clone()),
            },
        }
    }
}

/// Dimensions + mime of an encoded image, both read from the same bytes. The mime
/// falls back to PNG only for our own scaled derivatives (always PNG); a `Full` pull
/// carries the source frame's real encoding.
fn image_meta(bytes: &[u8]) -> (u32, u32, String) {
    let mime = image::guess_format(bytes)
        .map(|f| f.to_mime_type().to_string())
        .unwrap_or_else(|_| "image/png".to_string());
    let (width, height) = image::load_from_memory(bytes)
        .map(|img| (img.width(), img.height()))
        .unwrap_or((0, 0));
    (width, height, mime)
}

/// Encode frame bytes as a `data:` URL — the inline pixels channel, exactly the
/// shape a client-produced [`ObservedImage`] uses.
fn to_data_url(mime: &str, bytes: &[u8]) -> String {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{b64}")
}

/// `perception/look` — the persona-facing live-call PULL. `AiSafe`, so it auto-joins
/// every persona's tool surface; substrate-served (`Bare`) off the in-process
/// perception buffer. Stateless: it reaches the process-global perception registry
/// and the ONE shared compute cache, so it holds no deps.
#[derive(Default)]
pub struct LookCommand;

#[async_trait]
impl ActionCommand for LookCommand {
    const NAME: &'static str = "perception/look";
    const NATIVE: bool = true; // live-call eyes — offered natively beside perception/observe
    const DESCRIPTION: &'static str =
        "Look at the live video call you are in — get a current image of the people on \
         the call. Pass `participant` (an id) to see one person, or omit it to see \
         EVERYONE at once (a contact sheet of every participant). Pass `detail` = \
         \"thumbnail\" (default, a quick small look) or \"full\" (the full-resolution \
         frame) when you need to look closely. Returns the current frame(s) as images. \
         This is your own eyes on the call; to look at a web page or UI instead, use \
         perception/observe with a URL.";

    type Params = LookParams;
    type Output = LookResult;

    async fn run(&self, ctx: &Ctx, p: LookParams) -> Result<LookResult, CommandError> {
        // Self-scoped: the persona looks through its OWN eyes. The buffer key is the
        // authenticated caller the executor's gate already saw — never a param (a
        // persona cannot look through another's eyes). Substrate/owner code (caller
        // None) has no live perception buffer.
        let Some(caller) = ctx.caller.as_ref() else {
            return Ok(LookResult::cannot(
                "perception/look shows the live video call you are in — it reads your own \
                 eyes. This dispatch carries no persona identity (substrate/owner code has \
                 no live perception buffer).",
            ));
        };
        let persona_id = caller.peer_id.as_uuid();

        // Resolve the caller's OWN perception buffer. `get` (peek, not create): a
        // persona with no buffer has never joined a live call — say so plainly.
        let Some(buffer) = perception_registry().get(&persona_id) else {
            return Ok(LookResult::cannot(
                "You are not in a live video call, so there is nothing to look at right \
                 now. perception/look shows the participants in a live call; to look at a \
                 web page or UI, use perception/observe with a URL.",
            ));
        };

        let compute = crate::runtime::shared_compute::global();
        let fidelity = match p.detail.unwrap_or_default() {
            LookDetail::Thumbnail => LookFidelity::Thumbnail,
            LookDetail::Full => LookFidelity::Full,
        };
        let scope = match p.participant.as_deref() {
            Some(id) => LookScope::Source(id.to_string()),
            None => LookScope::Everyone,
        };

        let images = buffer.look(scope, fidelity, &compute).await;

        // A NAMED participant that yields nothing is an honest miss — teach who is
        // actually present (fail loud, never a fabricated look).
        if let Some(who) = p.participant.as_deref() {
            if images.is_empty() {
                let present: Vec<String> = buffer
                    .current_percepts(&compute)
                    .into_iter()
                    .map(|pc| pc.participant)
                    .collect();
                let hint = if present.is_empty() {
                    "No video is flowing in this call yet.".to_string()
                } else {
                    format!("Present participants: {}.", present.join(", "))
                };
                return Ok(LookResult::cannot(format!(
                    "No participant '{who}' in your live view. {hint}"
                )));
            }
        }

        Ok(LookResult::seen(
            images.into_iter().map(LookView::from_look_image).collect(),
        ))
    }
}

crate::register_stateless_command!(LookCommand);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::persona_tools::native_tool_specs;
    use crate::identity::PeerId;
    use crate::media::MediaFrame;
    use crate::routing::CallerIdentity;
    use crate::sdk_codegen::{command_registry, AccessLevel, WireShape};
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;
    use uuid::Uuid;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// A caller ctx for a persona (as the executor stamps it onto the tool connection).
    fn persona_ctx(persona_id: Uuid) -> Ctx {
        Ctx {
            caller: Some(CallerIdentity::local_persona(PeerId::from_uuid(persona_id))),
            ..Default::default()
        }
    }

    // what this catches: `perception/look` must be a registered AiSafe command on the
    // NATIVE tool surface — so a persona is offered its live-call eyes every turn beside
    // observe — and it is substrate-served (Bare), NOT a Provided eye-node call: the
    // frames are already in-process, resolved from the persona's own buffer. A rename,
    // an access bump, or a drop from the native set would silently blind personas to
    // the call, which this pins loudly.
    #[test]
    fn look_is_an_ai_safe_bare_command_in_the_native_surface() {
        let descriptor = command_registry()
            .into_iter()
            .find(|d| d.name == "perception/look")
            .expect("perception/look must be registered");
        assert_eq!(
            descriptor.access_level,
            AccessLevel::AiSafe,
            "look must be AiSafe so it joins the persona tool surface"
        );
        assert_eq!(
            descriptor.wire,
            WireShape::Bare,
            "look is substrate-served off the in-process buffer, never a Provided eye-node"
        );
        assert!(
            native_tool_specs().iter().any(|s| s.name == "perception/look"),
            "look must be offered natively beside perception/observe"
        );
    }

    // what this catches: self-scope + fail-loud — a dispatch with NO caller identity
    // (substrate/owner code, which has no perception buffer) does not fabricate a look;
    // it returns success=false with a teaching message. This is the boundary that keeps
    // a persona from looking through a buffer that isn't its own.
    #[tokio::test]
    async fn a_look_with_no_caller_identity_fails_loud() {
        let r = LookCommand
            .run(&Ctx::default(), LookParams::default())
            .await
            .expect("command runs");
        assert!(!r.success, "no identity → cannot look");
        assert!(r.views.is_empty());
        assert!(
            r.error.as_deref().unwrap_or("").contains("live video"),
            "the failure teaches what look is for"
        );
    }

    // what this catches: a persona with no perception buffer (never joined a live call)
    // is told plainly it is not in a call — not handed an empty success that reads as
    // "saw nothing" — and pointed at perception/observe for a URL.
    #[tokio::test]
    async fn a_look_when_not_in_a_call_teaches_the_alternative() {
        // A fresh persona id with no buffer resolved (peek returns None).
        let r = LookCommand
            .run(&persona_ctx(Uuid::new_v4()), LookParams::default())
            .await
            .expect("command runs");
        assert!(!r.success, "no buffer → not in a call");
        let msg = r.error.unwrap_or_default();
        assert!(msg.contains("not in a live video call"), "says why");
        assert!(msg.contains("perception/observe"), "points at the URL alternative");
    }

    // what this catches: THE happy path — a persona looks through its OWN buffer and
    // gets the current frame(s) as images, projected as ObservedImage (data_url + real
    // dimensions decoded from the bytes). An everyone-look returns the gallery; the
    // pixels channel is the same Percept shape observe returns.
    #[tokio::test]
    async fn look_reads_the_callers_own_buffer_and_returns_current_frames() {
        let pid = Uuid::new_v4();
        let buffer = perception_registry().handle(pid);
        buffer.seed_frame_for_test("alice", MediaFrame::from_bytes(png(120, 90)));
        buffer.seed_frame_for_test("bob", MediaFrame::from_bytes(png(64, 48)));

        // Everyone-look: the gallery — one view per participant, each with an image.
        let r = LookCommand
            .run(&persona_ctx(pid), LookParams::default())
            .await
            .expect("command runs");
        assert!(r.success, "in a call with frames → satisfied");
        assert_eq!(r.views.len(), 2, "everyone-look = one view per participant");
        assert!(
            r.views.iter().all(|v| v.image.is_some() && v.error.is_none()),
            "each view carries a resolved image"
        );
        let alice = r.views.iter().find(|v| v.participant == "alice").unwrap();
        let img = alice.image.as_ref().unwrap();
        assert!(
            img.data_url.as_deref().unwrap_or("").starts_with("data:image/png;base64,"),
            "the pixels channel is an inline data URL"
        );
        assert!(img.width > 0 && img.height > 0, "honest dimensions decoded from the bytes");

        // Source-look at one participant → just that view.
        let one = LookCommand
            .run(
                &persona_ctx(pid),
                LookParams {
                    participant: Some("alice".into()),
                    detail: Some(LookDetail::Full),
                },
            )
            .await
            .expect("command runs");
        assert!(one.success);
        assert_eq!(one.views.len(), 1);
        assert_eq!(one.views[0].participant, "alice");
        // Full detail returns the raw source frame (120×90), not the thumbnail.
        assert_eq!(one.views[0].image.as_ref().unwrap().width, 120, "full = raw frame");

        perception_registry().remove(&pid);
    }

    // what this catches: naming a participant who isn't in the call fails loud and
    // LISTS who is actually present — never an empty success. This is the honest miss
    // that lets a persona correct its aim.
    #[tokio::test]
    async fn look_at_an_absent_participant_lists_who_is_present() {
        let pid = Uuid::new_v4();
        let buffer = perception_registry().handle(pid);
        buffer.seed_frame_for_test("alice", MediaFrame::from_bytes(png(50, 40)));

        let r = LookCommand
            .run(
                &persona_ctx(pid),
                LookParams {
                    participant: Some("nobody".into()),
                    detail: None,
                },
            )
            .await
            .expect("command runs");
        assert!(!r.success, "absent participant → honest miss");
        let msg = r.error.unwrap_or_default();
        assert!(msg.contains("nobody"), "names who was asked for");
        assert!(msg.contains("alice"), "lists who is actually present");

        perception_registry().remove(&pid);
    }
}
