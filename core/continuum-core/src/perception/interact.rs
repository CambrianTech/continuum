//! `perception/interact`: a persona DRIVES a live page and sees the result, in one
//! persistent browser session (card 3569675f).
//!
//! [`perception/observe`](super) is SEE + REASON and [`hot_edit`](super::hot_edit) is
//! SEE after a CSS tweak, and both reopen the page on every call. A citizen building a
//! website needs more: open her dev server, click through a flow, fill a form, and see what
//! each step did, with the page state kept between calls. That is also how her product's
//! automation drives a job site, and how she produces EVIDENCE that a slice works (a
//! screenshot and the element tree after each step, rather than a description of it).
//!
//! # The session
//!
//! The first call names a `target` (a URL) and gets back a `session` handle. Later calls
//! pass that handle, a list of `actions`, and get back the fresh observation plus the
//! before/after pixel delta, which is the "did my step do what I intended?" signal the
//! `@continuum/perception` `PerceptionSession` computes for free. The eye-node holds the
//! live browser behind the handle, and closes it on [`SessionCloseCommand`] or after it has
//! sat idle (`apps/eye-node/src/interactAdapter.ts`), so an abandoned session never leaks a
//! browser.
//!
//! Like observe, both verbs are [`Provided`](crate::sdk_codegen::WireShape::Provided): the
//! headless core cannot render, so it routes the call to a connected eye-node. No eye-node
//! connected means the call fails loud, never a fabricated observation.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::hot_edit::HotEditDelta;
use super::{ObserveResult, ObserveViewport};

/// One step a persona takes on a live page. A closed set: the eye-node maps each onto the
/// `DomSurface` driver (Playwright), and a surface that cannot perform one refuses it
/// loudly rather than skipping it. Selectors are CSS selectors, aimed at the element tree an
/// observation returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/PerceptionAction.ts")]
pub enum PerceptionAction {
    // Click the first element matching `selector`.
    Click { selector: String },
    // Fill the first element matching `selector` with `text` (replacing its value).
    Type { selector: String, text: String },
    // Press a key (`Enter`, `Tab`, `Escape`, `ArrowDown`, ...).
    Press { key: String },
    // Hover the first element matching `selector`.
    Hover { selector: String },
    // Navigate the session's page to `url`.
    Goto { url: String },
    /// Replace the session's CSS patch without reopening the page; empty CSS clears it.
    HotPatchCss { css: String },
}

/// What `perception/interact` takes: a new session (`target`) or an existing one
/// (`session`), then the steps to take.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/InteractParams.ts")]
pub struct InteractParams {
    /// The session to continue, from an earlier call's result. Omit to open a new one at
    /// `target`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session: Option<String>,
    /// The URL to open a NEW session at (e.g. your dev server at http://localhost:31004).
    /// Required when `session` is omitted; ignored otherwise (use a `goto` action).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target: Option<String>,
    /// Render size for a new session. Adapter default when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub viewport: Option<ObserveViewport>,
    /// The steps to take, in order, before observing. Empty just observes.
    #[serde(default)]
    pub actions: Vec<PerceptionAction>,
    /// Scope the observation (and its delta) to one region, a CSS selector. Omit for the
    /// whole page.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selector: Option<String>,
}

/// What `perception/interact` returns: the SAME observation shape observe returns (the page
/// AFTER the actions, flattened), the session handle to continue with, and the before/after
/// delta of the actions. BARE (not enveloped), like observe.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/InteractResult.ts")]
pub struct InteractResult {
    /// The page after the actions. Flattened: `success`/`url`/`title`/`image`/`structure`/
    /// `error` sit at the top level, exactly as an observe result does.
    #[serde(flatten)]
    #[ts(flatten)]
    pub observation: ObserveResult,
    /// The session to pass back to continue on this page. Absent when no session could be
    /// opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session: Option<String>,
    /// Before/after pixel delta of this call's actions, when there were any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delta: Option<HotEditDelta>,
}

/// What `perception/session-close` takes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/SessionCloseParams.ts")]
pub struct SessionCloseParams {
    /// The session handle an interact returned.
    pub session: String,
}

/// What `perception/session-close` returns. BARE, like observe.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/perception/SessionCloseResult.ts")]
pub struct SessionCloseResult {
    /// The call itself succeeded.
    pub success: bool,
    /// A live session was found and closed (false: it had already ended or expired).
    pub closed: bool,
    /// Adapter-side failure reason when `success == false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// Typed declaration of `perception/interact`: a `Provided` command routed to an eye-node,
/// sibling of observe and hot-edit. `AiSafe` and native, so a citizen can drive and check
/// her own site every turn.
pub struct InteractCommand;

impl crate::sdk_codegen::CommandSpec for InteractCommand {
    const NAME: &'static str = "perception/interact";
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const NATIVE: bool = true; // the DRIVE half of the loop, offered beside perception/observe
    const DESCRIPTION: &'static str =
        "Drive a live web page and see each result. Open with `target` URL; reuse the returned \
         `session` for later `actions`: click, type, press, hover, goto, hotPatchCss. \
         Selectors are CSS. Each call returns the image, element tree and pixel-change `delta`. \
         hotPatchCss replaces the CSS patch while preserving page state; empty css clears it. \
         Use this to test flows and iterate visually.";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Provided;
    type Params = InteractParams;
    type Result = InteractResult;
}

crate::register_command!(InteractCommand);

/// Typed declaration of `perception/session-close`. Catalog-only (not native): sessions also
/// close themselves when idle, so this is for releasing a browser early.
pub struct SessionCloseCommand;

impl crate::sdk_codegen::CommandSpec for SessionCloseCommand {
    const NAME: &'static str = "perception/session-close";
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Close a perception/interact session and release its browser. Sessions also close \
         themselves when idle.";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Provided;
    type Params = SessionCloseParams;
    type Result = SessionCloseResult;
}

crate::register_command!(SessionCloseCommand);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::persona_tools::native_tool_specs;
    use crate::sdk_codegen::{command_registry, AccessLevel, WireShape};

    // what this catches: the action wire drifting from the eye-node's `DomAction` kinds
    // (`click`/`type`/`press`/`hover`, plus `goto`), which would make every step a persona
    // takes an unknown-kind refusal at the adapter; and an unknown kind being accepted.
    #[test]
    fn actions_are_tagged_with_the_dom_action_kinds() {
        let actions = vec![
            PerceptionAction::Click { selector: "#approve".into() },
            PerceptionAction::Type { selector: "input[name=q]".into(), text: "rust".into() },
            PerceptionAction::Press { key: "Enter".into() },
            PerceptionAction::Hover { selector: "nav".into() },
            PerceptionAction::Goto { url: "http://localhost:31004/gates".into() },
            PerceptionAction::HotPatchCss { css: "body{background:purple}".into() },
        ];
        let json = serde_json::to_value(&actions).expect("actions serialize");
        let kinds: Vec<&str> = json.as_array().expect("array").iter().map(|a| a["kind"].as_str().expect("kind")).collect();
        assert_eq!(kinds, ["click", "type", "press", "hover", "goto", "hotPatchCss"]);
        assert_eq!(json[1]["text"], serde_json::json!("rust"));
        let back: Vec<PerceptionAction> = serde_json::from_value(json).expect("actions round-trip");
        assert_eq!(back, actions);
        assert!(
            serde_json::from_value::<PerceptionAction>(serde_json::json!({"kind": "drag", "selector": "x"})).is_err(),
            "an unknown step is refused, never guessed"
        );
    }

    // what this catches: the result losing observe's flattened shape (every eye returns one
    // observation shape) or the session handle a persona needs to continue.
    #[test]
    fn an_interact_result_is_an_observation_plus_its_session() {
        let result = InteractResult {
            observation: ObserveResult {
                success: true,
                url: Some("http://localhost:31004/".into()),
                title: Some("Tracker".into()),
                image: None,
                structure: None,
                error: None,
            },
            session: Some("s-1".into()),
            delta: Some(HotEditDelta { pixels_changed: 10, total_pixels: 100, ratio: 0.1 }),
        };
        let json = serde_json::to_value(&result).expect("serialize");
        assert!(json.get("observation").is_none(), "flattened, got {json}");
        assert_eq!(json["title"], serde_json::json!("Tracker"));
        assert_eq!(json["session"], serde_json::json!("s-1"));
        assert_eq!(json["delta"]["pixelsChanged"], serde_json::json!(10));
        let params: InteractParams =
            serde_json::from_value(serde_json::json!({"target": "http://localhost:31004/"})).expect("params");
        assert!(params.actions.is_empty() && params.session.is_none(), "a first call needs only a target");
    }

    // what this catches: the drive verb not reaching the persona (not AiSafe, not native, or
    // not Provided, so routed to a substrate module instead of an eye-node), and the close
    // verb bloating the per-turn tool surface.
    #[test]
    fn interact_is_a_native_provided_verb_and_close_is_catalog_only() {
        let registry = command_registry();
        for (name, native) in [("perception/interact", true), ("perception/session-close", false)] {
            let d = registry.iter().find(|d| d.name == name).unwrap_or_else(|| panic!("{name} registered"));
            assert_eq!(d.access_level, AccessLevel::AiSafe, "{name}");
            assert_eq!(d.wire, WireShape::Provided, "{name} is served by an eye-node");
            assert_eq!(native_tool_specs().iter().any(|s| s.name == name), native, "{name} native={native}");
        }
    }
}
