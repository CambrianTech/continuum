//! `perception/hot-edit` — HOT CSS, NO DEPLOYMENTS: apply a style patch to a
//! LIVE rendered page and re-observe it, without touching disk.
//!
//! The iterate-like-an-engineer verb of the design loop
//! (`docs/architecture/DESIGN-BENCH-VISUAL-CRAFT.md`: render → observe →
//! **hot-edit** → re-grade). [`perception/observe`](super) is SEE + REASON;
//! this is SEE + REASON **after a TWEAK** — the persona names a `target`, hands
//! over a stylesheet, and gets back the SAME observation shape it already knows
//! ([`ObserveResult`], flattened into [`HotEditResult`]) showing what the page
//! looks like WITH the patch applied, plus the before/after pixel delta (the
//! money signal: "did my change do what I intended?").
//!
//! Exactly like observe, this is a [`Provided`](crate::sdk_codegen::WireShape::Provided)
//! command: the headless core cannot render, so it ROUTES the call to a
//! connected eye-node adapter (`apps/eye-node/src/hotEditAdapter.ts`), which
//! drives `@continuum/perception` — the CSS lands as the page's single
//! `<style data-continuum-hot-edit>` hot-patch layer (REPLACED wholesale each
//! call, never appended) and the session's before/after `Delta` rides back.
//! No adapter connected ⇒ fail loud, never a fabricated observation
//! ([[fallbacks-are-illegal-fail-loud]]).
//!
//! # The stateless-but-effective contract (scope honesty)
//!
//! Today the adapter re-opens `target` per call (observe's session lifecycle):
//! each hot-edit loads the page FRESH and then applies `css`. So `css` must be
//! the persona's FULL accumulated patch, not an increment — re-apply everything
//! you've decided so far on every call, and the replace-wholesale layer makes
//! that idempotent. A persistent live session (true in-place hot-editing with
//! page state preserved across calls) is the next step; the wire contract here
//! does not change when it lands — only the adapter's session lifetime does.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{ObserveResult, ObserveViewport};

/// What to hot-edit — [`ObserveParams`](super::ObserveParams) plus the patch.
/// `target` keeps observe's reinterpret-per-adapter contract (a URL for a web
/// adapter; other surfaces map it — or refuse loudly when they have no style
/// system to patch).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/HotEditParams.ts"
)]
pub struct HotEditParams {
    /// What to look at. A web adapter treats this as a URL to open; other adapters
    /// map it to their own surface path.
    pub target: String,
    /// Render at this size. Adapter default when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub viewport: Option<ObserveViewport>,
    /// The stylesheet to apply as the page's hot-patch layer. REPLACES the whole
    /// layer (never appends), so pass your FULL accumulated CSS each call — the
    /// page is re-opened fresh per call today, and an empty string clears the
    /// layer entirely.
    pub css: String,
    /// Scope the re-observation to one region (a CSS selector in a browser; an
    /// equivalent node path elsewhere). Omit to re-observe the whole surface.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selector: Option<String>,
}

/// The before/after pixel delta of the patch — the fraction of the (re-)observed
/// frame the CSS actually moved. The wire mirror of `@continuum/perception`'s
/// `Delta`; present when the adapter's session computed it cheaply, absent
/// otherwise (never fabricated).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/HotEditDelta.ts"
)]
pub struct HotEditDelta {
    /// Pixels that changed between the pre-patch and post-patch frames.
    #[ts(type = "number")]
    pub pixels_changed: u64,
    /// Total pixels compared.
    #[ts(type = "number")]
    pub total_pixels: u64,
    /// `pixels_changed / total_pixels`, 0..1. Mismatched frame sizes read as 1
    /// (a layout-scale change).
    pub ratio: f64,
}

/// The result of `perception/hot-edit`: the SAME [`ObserveResult`] shape (the
/// AFTER-observation, flattened — one observation shape from every eye), plus
/// the applied-CSS echo and the before/after delta. BARE (not enveloped), like
/// observe: the adapter produces it and owns `success`/`error` (which live on
/// the flattened observation).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/HotEditResult.ts"
)]
pub struct HotEditResult {
    /// The after-observation — what the page looks like WITH the patch applied.
    /// Flattened: `success`/`url`/`title`/`image`/`structure`/`error` sit at the
    /// top level, exactly as an observe result does.
    #[serde(flatten)]
    #[ts(flatten)]
    pub observation: ObserveResult,
    /// Echo of the stylesheet that was applied — the receipt that ties this
    /// observation to the patch that produced it.
    pub applied_css: String,
    /// Before/after pixel delta of the patch, when the session computed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delta: Option<HotEditDelta>,
}

/// Typed declaration of `perception/hot-edit` — a `Provided` command, sibling of
/// [`ObserveCommand`](super::ObserveCommand) and shaped exactly like it: the
/// substrate can't render, so it routes the call to an eye-node adapter, which
/// exchanges bare [`HotEditParams`] → [`HotEditResult`] via `Commands.provide`.
/// One name, N platform adapters. `AiSafe`, so it auto-joins every persona's
/// tool surface.
pub struct HotEditCommand;

impl crate::sdk_codegen::CommandSpec for HotEditCommand {
    const NAME: &'static str = "perception/hot-edit";
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const NATIVE: bool = true; // the TWEAK half of the design loop — offered beside perception/observe
    const DESCRIPTION: &'static str =
        "Apply a CSS patch to a live page and re-observe it — hot css, no deployments. \
         Pass `target` (a URL) and `css` (a stylesheet); you get back the same image + \
         structure an observe returns, now showing the patched page, plus `delta` (the \
         fraction of pixels the patch moved). The page reloads fresh each call and the \
         patch layer is replaced, so pass your FULL accumulated CSS every time (empty \
         clears it). Iterate on styling before writing anything to disk.";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Provided;
    type Params = HotEditParams;
    type Result = HotEditResult;
}

crate::register_command!(HotEditCommand);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::persona_tools::native_tool_specs;
    use crate::sdk_codegen::{command_registry, AccessLevel, WireShape};

    // what this catches: the flattened wire shape IS the contract the adapter and
    // every persona parse — `observation` must serialize INLINE (success/url/title
    // at top level, `box` for bounds, camelCase appliedCss/pixelsChanged), and a
    // round-trip must reproduce the value. A dropped #[serde(flatten)], a renamed
    // field, or a snake_case leak would silently fork the wire from the generated
    // TS type; this pins the exact JSON both sides exchange.
    #[test]
    fn hot_edit_result_round_trips_flattened_camel_case_json() {
        let result = HotEditResult {
            observation: ObserveResult {
                success: true,
                url: Some("http://localhost:5173/".into()),
                title: Some("Dashboard".into()),
                image: None,
                structure: None,
                error: None,
            },
            applied_css: "body{background:#111318}".into(),
            delta: Some(HotEditDelta {
                pixels_changed: 1200,
                total_pixels: 4800,
                ratio: 0.25,
            }),
        };

        let json = serde_json::to_value(&result).expect("HotEditResult must serialize");
        // Flattened: the observation's fields sit at the TOP level, no `observation` key.
        assert!(json.get("observation").is_none(), "must flatten, got {json}");
        assert_eq!(json["success"], serde_json::json!(true));
        assert_eq!(json["title"], serde_json::json!("Dashboard"));
        assert_eq!(json["appliedCss"], serde_json::json!("body{background:#111318}"));
        assert_eq!(json["delta"]["pixelsChanged"], serde_json::json!(1200));
        assert_eq!(json["delta"]["ratio"], serde_json::json!(0.25));
        // Absent optionals are OMITTED, not null — the bare-wire hygiene observe keeps.
        assert!(json.get("error").is_none() && json.get("image").is_none());

        let back: HotEditResult =
            serde_json::from_value(json).expect("HotEditResult must deserialize");
        assert_eq!(back.applied_css, result.applied_css);
        assert_eq!(back.delta, result.delta);
        assert_eq!(back.observation.title.as_deref(), Some("Dashboard"));

        // Params round-trip on the same wire law (camelCase, optionals omitted).
        let params = HotEditParams {
            target: "http://localhost:5173/".into(),
            viewport: None,
            css: "h1{color:tomato}".into(),
            selector: Some("main".into()),
        };
        let pj = serde_json::to_value(&params).expect("HotEditParams must serialize");
        assert!(pj.get("viewport").is_none(), "absent viewport must be omitted");
        let pback: HotEditParams =
            serde_json::from_value(pj).expect("HotEditParams must deserialize");
        assert_eq!(pback.css, params.css);
        assert_eq!(pback.selector.as_deref(), Some("main"));
    }

    // what this catches: `perception/hot-edit` must (a) be registered AiSafe +
    // Provided — adapter-served, routed to an eye-node, never a substrate
    // ServiceModule — and (b) sit in the native tool set beside perception/observe,
    // so the design loop's TWEAK verb is offered every turn. A rename, an
    // access-level bump, or dropping NATIVE would silently take hot-editing away
    // from every citizen (Provided commands fail closed); this pins it loudly.
    #[test]
    fn hot_edit_is_an_ai_safe_provided_command_in_the_native_surface() {
        let descriptor = command_registry()
            .into_iter()
            .find(|d| d.name == "perception/hot-edit")
            .expect("perception/hot-edit must be registered");
        assert_eq!(
            descriptor.access_level,
            AccessLevel::AiSafe,
            "hot-edit must be AiSafe so it joins the persona tool surface"
        );
        assert_eq!(
            descriptor.wire,
            WireShape::Provided,
            "hot-edit is adapter-served (an eye-node), never a substrate ServiceModule"
        );
        assert!(
            native_tool_specs()
                .iter()
                .any(|s| s.name == "perception/hot-edit"),
            "hot-edit must be offered natively beside perception/observe"
        );
    }
}
