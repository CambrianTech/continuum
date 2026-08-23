//! `perception/*` — a persona's eyes on an artifact it created or is inspecting.
//!
//! Like [`interface`](crate::interface), these are
//! [`Provided`](crate::sdk_codegen::WireShape::Provided) commands: one command
//! NAME, N platform adapters. The headless core CANNOT observe — it has no
//! browser, no renderer, no GPU, and a datacenter rack instance has no display at
//! all. It ROUTES `perception/observe` to whichever connected node holds an eye
//! (a laptop client, or a render-worker that CHOSE to install a Chromium-family
//! browser), which fulfils it via the `@continuum/perception` `Surface` — a
//! `DomSurface` for a web page today, a `SceneSurface`/`BevySurface` for 3D next.
//! No adapter connected ⇒ the call fails loud, never a fabricated observation
//! ([[fallbacks-are-illegal-fail-loud]]).
//!
//! # Why this is bigger than `interface/screenshot`
//!
//! `interface/screenshot` is SEE-only — it returns pixels. `perception/observe`
//! returns pixels AND the STRUCTURE the persona reasons over: the tree of named,
//! boxed nodes ([`ProbeNode`]) that lets a mind aim an action at *an element*, not
//! a pixel, and describe *what* it's looking at, not just *that* it looks like
//! something. It is the SEE + REASON pair of the Perception Surface (#187,
//! `docs/architecture/PERCEPTION-SURFACE.md`) delivered as one persona-callable
//! verb — the substrate half of "literally ask a persona what it can see".
//!
//! # The wire contract lives in Rust (single source of truth)
//!
//! Exactly as `interface` does it: the platform-AGNOSTIC contract is declared
//! ONCE here as ts-rs types and generated into `protocol/typescript/perception/`.
//! The adapter-private capability types (`@continuum/perception`'s `DomViewSpec`,
//! `StructuredState`, the Playwright driver) are NOT part of it — the Node
//! provider maps its internal `Surface` output onto this generated wire shape at
//! the `Commands.provide` boundary. Keeping the capability free of the wire type
//! is what let it be built and validated headlessly before this command existed.
//! The generated surface is BARE (no substrate envelope): the adapter, not a
//! `ServiceModule`, produces the result, so it carries its OWN `success`/`error`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Structural scoring of an observation against a UI spec — the functional
/// web-dev benchmark's "diff on the element tree" (works for every persona;
/// scores the text structure a non-visual model also reads).
pub mod scoring;

/// Static-HTML eye — parse a rendered `index.html` artifact into a [`ProbeNode`]
/// tree so the HEADLESS eval core can grade structural `ui_checks` with no browser
/// eye-node connected. `perception/observe` fails loud when no client provides it;
/// but a static file's tags/roles/text/counts are a pure-parse question. The full
/// browser eye stays the path for JS-rendered / dynamic pages (a persona's live
/// seeing loop) — this is the deterministic grader's eye for a static artifact.
pub mod static_html;

/// `perception/look` — the LIVE-CALL VIDEO surface: a persona's own eyes on the
/// call it is in (the observe-only sibling of `perception/observe`, reading the
/// in-process [`PerceptionBuffer`](crate::media::PerceptionBuffer) à la carte).
pub mod look;

/// Render size for an observation, in the surface's pixels (CSS px for a UI,
/// framebuffer px for a scene). Omit to use the adapter's current/default size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/ObserveViewport.ts"
)]
pub struct ObserveViewport {
    #[ts(type = "number")]
    pub width: u32,
    #[ts(type = "number")]
    pub height: u32,
}

/// What to observe — the adapter-agnostic request.
///
/// `target` is a URL for a web adapter; a scene/app route for others (the same
/// reinterpret-per-adapter contract `interface/screenshot`'s `query_selector`
/// uses). Deliberately narrow: adapter-private knobs (Playwright channel, device
/// scale, headless flag) are NOT here — a browser tab, a phone, and a render node
/// can ALL honor this.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/ObserveParams.ts"
)]
pub struct ObserveParams {
    /// What to look at. A web adapter treats this as a URL to open; other adapters
    /// map it to their own surface path.
    pub target: String,
    /// Render at this size. Adapter default when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub viewport: Option<ObserveViewport>,
    /// Focus the observation on one region (a CSS selector in a browser; an
    /// equivalent node path elsewhere). Omit to observe the whole surface.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selector: Option<String>,
}

/// A node's projected 2D bounds in the rendered frame — how a persona reasons
/// about position and aims an action at a node instead of a pixel. Valid for ANY
/// surface (a DOM layout box, a scene node's projected screen rect).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/ProbeBox.ts"
)]
pub struct ProbeBox {
    #[ts(type = "number")]
    pub x: f32,
    #[ts(type = "number")]
    pub y: f32,
    #[ts(type = "number")]
    pub width: f32,
    #[ts(type = "number")]
    pub height: f32,
}

/// One node of THE STRUCTURE a persona reasons over — a DOM element, a scene-graph
/// node, a layout box. Surface-neutral: identity, geometry, text, a few attrs.
/// Recursive (`children`), so the whole tree is one value. The wire mirror of
/// `@continuum/perception`'s internal `ProbeNode`; the provider maps one onto the
/// other at the boundary.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/ProbeNode.ts"
)]
pub struct ProbeNode {
    /// Element tag / node type (`div`, `button`; a scene node's payload kind).
    pub tag: String,
    /// Accessibility role, when the surface exposes one (`button`, `heading`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub role: Option<String>,
    /// Accessible / display name — the human-meaningful label (a scene node's id).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Visible text directly on this node (not its descendants).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    /// Projected 2D bounds in the rendered frame.
    #[serde(rename = "box", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bounds: Option<ProbeBox>,
    /// A curated set of load-bearing attributes (`id`, `class`, `href`; for a scene
    /// node: `position`, `scale`, `kind`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub attrs: Option<HashMap<String, String>>,
    /// The CRAFT FACTS a design grade measures — a DECLARED subset of computed
    /// style (color, background-color, font-size/-weight/-family, margin,
    /// padding, display, overflow, z-index). DOM surfaces fill it; surfaces
    /// with no style system (a scene graph) omit it. Additive + optional, so
    /// every adapter that predates it stays wire-compatible.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub style: Option<HashMap<String, String>>,
    /// Child nodes, in document/scene order.
    pub children: Vec<ProbeNode>,
}

/// The pixels channel of an observation — a rendered frame, delivered inline as a
/// `data:` URL and/or written to a substrate path (the adapter picks, mirroring
/// `ScreenshotResult`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/ObservedImage.ts"
)]
pub struct ObservedImage {
    /// `data:` URL of the encoded frame (usually PNG), when returned inline.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub data_url: Option<String>,
    /// Substrate path when the adapter wrote the frame to a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub filepath: Option<String>,
    /// Rendered width in px.
    #[ts(type = "number")]
    pub width: u32,
    /// Rendered height in px.
    #[ts(type = "number")]
    pub height: u32,
}

/// The result of `perception/observe` — SEE (`image`) + REASON (`structure`).
/// BARE (not enveloped): the client adapter, not a substrate `ServiceModule`,
/// produces it, so it carries its own `success`/`error` rather than the substrate
/// `CommandResponse` envelope.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/ObserveResult.ts"
)]
pub struct ObserveResult {
    /// Observation succeeded.
    pub success: bool,
    /// The surface's location identity, when it has one (the page URL). Absent for
    /// a scene, a live camera, an in-memory surface.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    /// The surface's human label (a page `<title>`, a scene's root name).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    /// The rendered frame (pixels to JUDGE).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub image: Option<ObservedImage>,
    /// The structural probe (the tree to REASON over and aim actions at).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub structure: Option<ProbeNode>,
    /// Adapter-side failure reason when `success == false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// Typed declaration of `perception/observe` — a `Provided` command, sibling of
/// [`ScreenshotCommand`](crate::interface::ScreenshotCommand). The substrate
/// can't observe; it routes the call to an eye-node adapter, which exchanges bare
/// [`ObserveParams`] → [`ObserveResult`] via `Commands.provide`. One name, N
/// platform adapters. `AiSafe`, so it auto-joins every persona's tool surface.
pub struct ObserveCommand;

impl crate::sdk_codegen::CommandSpec for ObserveCommand {
    const NAME: &'static str = "perception/observe";
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const NATIVE: bool = true; // SEE + REASON — offered natively beside interface/screenshot
    const DESCRIPTION: &'static str =
        "Observe a UI or web page — SEE it as pixels AND read its STRUCTURE (the \
         tree of elements with their names, text, and on-screen boxes). Use it to \
         look at what a human or a UI is showing and reason about the layout, or to \
         verify what your own change actually rendered before you act on it. Pass \
         `target` (a URL for a web page); the observation comes back with an image \
         and a structure tree.";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Provided;
    type Params = ObserveParams;
    type Result = ObserveResult;
}

crate::register_command!(ObserveCommand);

#[cfg(test)]
mod tests {
    use crate::cognition::persona_tools::native_tool_specs;
    use crate::sdk_codegen::{command_registry, AccessLevel, WireShape};

    // what this catches: `perception/observe` must (a) be in the registry as an
    // AiSafe Provided command — so it auto-joins every persona's tool surface and
    // routes to an eye-node adapter, never a substrate ServiceModule — and (b) be
    // in the native tool set beside `interface/screenshot`, so a persona is offered
    // its eyes every turn. A rename, an access-level bump, or dropping it from the
    // native list would silently blind personas (Provided commands fail closed:
    // spec_for_command drops an unregistered name), which this pins loudly.
    #[test]
    fn observe_is_an_ai_safe_provided_command_in_the_native_surface() {
        let descriptor = command_registry()
            .into_iter()
            .find(|d| d.name == "perception/observe")
            .expect("perception/observe must be registered");
        assert_eq!(
            descriptor.access_level,
            AccessLevel::AiSafe,
            "observe must be AiSafe so it joins the persona tool surface"
        );
        assert_eq!(
            descriptor.wire,
            WireShape::Provided,
            "observe is adapter-served (an eye-node), never a substrate ServiceModule"
        );
        assert!(
            native_tool_specs()
                .iter()
                .any(|s| s.name == "perception/observe"),
            "observe must be offered natively beside interface/screenshot"
        );
    }
}
