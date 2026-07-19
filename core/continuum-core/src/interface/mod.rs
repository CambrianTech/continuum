//! `interface/*` — commands the substrate cannot execute itself, only ROUTE to
//! whichever client holds the capability.
//!
//! These are the [`Provided`](crate::sdk_codegen::CommandKind::Provided) commands
//! of the SDK: one command NAME, N platform adapters. `interface/screenshot` is
//! the canonical example — the same call is fulfilled by `html2canvas` in a
//! browser tab, a native `CALayer`/`Window` snapshot on mobile/desktop, or a
//! framebuffer grab in VR. The substrate owns the *contract*; the client owns
//! the *capture*. ([[persona-is-a-client]] — a persona asking for a screenshot
//! is a citizen issuing the same command a human's browser tab does.)
//!
//! # Why the wire contract lives in Rust (not the TS command dir)
//!
//! The old screenshot command kept its types in TypeScript, which made the
//! shallow TS layer the source of truth and let `html2canvas`-specific options
//! leak into the cross-platform contract. Here the contract is the deepest,
//! platform-AGNOSTIC subset — `query_selector`, `format`, dimensions,
//! `destination` — declared once as ts-rs types. Adapter-private knobs
//! (`html2canvasOptions`, native capture flags) are NOT part of it; an adapter
//! carries them out-of-band. The generated SDK surface emits these BARE (no
//! substrate envelope), because the adapter — not a `ServiceModule` — produces
//! the result.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Image encoding for a capture. Platform-agnostic — every adapter (browser,
/// native, VR) maps these to its own encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../protocol/typescript/interface/ScreenshotFormat.ts")]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    Webp,
}

/// Where the captured bytes should land. `File` writes to the substrate and
/// returns a path; `Bytes` returns a data URL inline; `Both` does each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../protocol/typescript/interface/ScreenshotDestination.ts")]
pub enum ScreenshotDestination {
    File,
    Bytes,
    Both,
}

/// Cross-platform screenshot request — the adapter-agnostic contract.
///
/// Deliberately NARROWER than the legacy TS `ScreenshotParams`: no
/// `html2canvasOptions`, no preset arrays, no DOM-only crop knobs. Those are
/// adapter-private. This is what a browser tab, a phone, and a VR headset can
/// ALL honor.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/interface/ScreenshotParams.ts")]
pub struct ScreenshotParams {
    /// What to capture. A CSS selector in a browser; an equivalent node/scene
    /// path in other adapters. Omit to capture the whole surface.
    #[ts(optional)]
    pub query_selector: Option<String>,
    /// Desired encoding. Adapter default (usually PNG) when omitted.
    #[ts(optional)]
    pub format: Option<ScreenshotFormat>,
    /// Encoder quality 0.0–1.0 for lossy formats. Ignored for PNG.
    #[ts(optional, type = "number")]
    pub quality: Option<f32>,
    /// Target output width in px; adapter scales/crops to fit.
    #[ts(optional, type = "number")]
    pub width: Option<u32>,
    /// Target output height in px.
    #[ts(optional, type = "number")]
    pub height: Option<u32>,
    /// Device-scale multiplier (retina/HiDPI). 1.0 when omitted.
    #[ts(optional, type = "number")]
    pub scale: Option<f32>,
    /// Where the result should be delivered. Optional — adapters default to
    /// `File` when omitted, so the common case is just `{ querySelector }`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub destination: Option<ScreenshotDestination>,
    /// Suggested filename when `destination` writes a file.
    #[ts(optional)]
    pub filename: Option<String>,
}

/// Cross-platform screenshot result. BARE (not enveloped): the client adapter,
/// not a substrate `ServiceModule`, produces it — so it carries its OWN
/// `success`/`error` rather than the substrate `CommandResponse` envelope.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/interface/ScreenshotResult.ts")]
pub struct ScreenshotResult {
    /// Capture succeeded.
    pub success: bool,
    /// Substrate path when `destination` wrote a file.
    #[ts(optional)]
    pub filepath: Option<String>,
    /// `data:` URL when `destination` returned bytes inline.
    #[ts(optional)]
    pub data_url: Option<String>,
    /// Captured width in px.
    #[ts(optional, type = "number")]
    pub width: Option<u32>,
    /// Captured height in px.
    #[ts(optional, type = "number")]
    pub height: Option<u32>,
    /// Encoded byte size.
    #[ts(optional, type = "number")]
    pub size_bytes: Option<u64>,
    /// Adapter-side failure reason when `success == false`.
    #[ts(optional)]
    pub error: Option<String>,
}

/// Typed declaration of `interface/screenshot` — the PROVIDED outlier for
/// `sdk_codegen`. The substrate can't take a screenshot; it routes the call to a
/// client adapter (browser/native/VR), which exchanges bare `ScreenshotParams` →
/// `ScreenshotResult` via `Commands.provide`. So the generated wire shape is BARE
/// (no envelope) — the adapter owns the result. One name, N platform adapters;
/// the bare-but-adapter-served half of [`WireShape`](crate::sdk_codegen::WireShape).
pub struct ScreenshotCommand;

impl crate::sdk_codegen::CommandSpec for ScreenshotCommand {
    const NAME: &'static str = "interface/screenshot";
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const NATIVE: bool = true; // observation parity — seeing the screen is a first-class work verb
    const DESCRIPTION: &'static str =
        "Capture a screenshot of the UI — your way to SEE the screen (or a specific \
         element via a CSS selector). Use it to visually verify what a human or a UI is \
         showing before you act on it.";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Provided;
    type Params = ScreenshotParams;
    type Result = ScreenshotResult;
}

crate::register_command!(ScreenshotCommand);
