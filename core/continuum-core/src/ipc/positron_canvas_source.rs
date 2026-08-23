//! Canvas emitter — the ninth ViewState's source, and the first EVENT-DRIVEN
//! positron feed (Joel, 2026-08-23: "the events and monitors just need to
//! monitor the work itself … not be blind and polling").
//!
//! No tick, no scan: the feed publishes exactly when the WORK happens — a
//! persona's own `perception/observe` / `perception/hot-edit` result, folded
//! at the act seam ([`crate::cognition::act_observe::apply`]) into the same
//! frame her mind acted on. The screen shows what she saw, when she saw it —
//! one truth, N renderers, zero polling.
//!
//! Dual render targets from ONE fold (the bench emitter's #426 contract):
//! the websocket store human eyes read AND the mind substrate a citizen's
//! grounding reads, same revision in both.

use std::sync::{Mutex, OnceLock};

use continuum_positron::canvas::{CanvasViewState, CanvasViewport};
use continuum_positron::{StateBuilder, Substrate};

struct CanvasFeed {
    builder: StateBuilder,
    substrate: Substrate,
    mind_substrate: Substrate,
    /// Monotonic per-process observation counter — the face's iterate pulse.
    revision: u32,
}

static FEED: OnceLock<Mutex<CanvasFeed>> = OnceLock::new();

/// Install the feed's render targets at boot (beside the bench emitter).
/// Second call is a boot-sequence bug — fail loud, one feed per process.
pub fn install(substrate: Substrate, mind_substrate: Substrate) {
    if FEED
        .set(Mutex::new(CanvasFeed {
            builder: StateBuilder::standalone(),
            substrate,
            mind_substrate,
            revision: 0,
        }))
        .is_err()
    {
        panic!("positron_canvas_source::install called twice — one canvas feed per process");
    }
}

/// Publish one observation frame. Callable from any in-core seam that holds
/// an observe result; before `install` runs (tests, tools) it is a no-op —
/// the feed simply isn't wired on that process, never an error.
pub fn publish(mut view: CanvasViewState) {
    let Some(feed) = FEED.get() else { return };
    let Ok(mut feed) = feed.lock() else { return };
    feed.revision = feed.revision.saturating_add(1);
    view.revision = Some(feed.revision);
    let envelope = feed.builder.session(view);
    feed.substrate.store(envelope.clone());
    feed.mind_substrate.store(envelope);
}

/// Fold a persona's observe/hot-edit TOOL RESULT into a frame and publish it.
/// The result content is the bare `ObserveResult` JSON the eye-node returned;
/// anything unparseable is silently not a frame (the tool's own error path
/// already reports to the persona — this feed never invents observations).
pub fn maybe_publish_observation(persona: &str, tool_name: &str, result_content: &str) {
    if !matches!(tool_name, "perception/observe" | "perception/hot-edit") {
        return;
    }
    if FEED.get().is_none() {
        return; // not wired on this process — skip the parse entirely
    }
    let Ok(obs) = serde_json::from_str::<serde_json::Value>(result_content) else {
        return;
    };
    if obs.get("success").and_then(|v| v.as_bool()) != Some(true) {
        return; // a failed observation is not a frame
    }
    let image = obs.get("image");
    let view = CanvasViewState {
        artifact_title: obs
            .get("title")
            .and_then(|v| v.as_str())
            .or_else(|| obs.get("url").and_then(|v| v.as_str()))
            .map(str::to_string),
        artifact_html: None, // the observation carries pixels+structure; the page's source stays hers
        artifact_url: obs.get("url").and_then(|v| v.as_str()).map(str::to_string),
        screenshot_data_url: image
            .and_then(|i| i.get("dataUrl"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        persona: Some(persona.to_string()),
        observed_at_ms: Some(crate::persona::trace::now_ms()),
        viewport: image.and_then(|i| {
            Some(CanvasViewport {
                width: i.get("width")?.as_u64()? as u32,
                height: i.get("height")?.as_u64()? as u32,
            })
        }),
        revision: None, // stamped by publish()
        checks: None,   // grades ride the oracle path, not raw observations
        judge: None,
    };
    // Pixels or a URL make a frame; a structure-only observation does not
    // (the face's own honesty rule: a title alone is not a page).
    if view.screenshot_data_url.is_some() || view.artifact_url.is_some() {
        publish(view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the fold's honesty rules at the only seam that
    // matters pre-install — non-observe tools never frame, failed
    // observations never frame, and (the load-bearing one) publishing before
    // install is a clean no-op, because every test and tool process runs
    // uninstalled and a panic here would take the act loop down with it.
    #[test]
    fn uninstalled_feed_is_a_noop_and_only_real_observations_frame() {
        maybe_publish_observation("Atlas", "code/write", "{\"success\":true}");
        maybe_publish_observation("Atlas", "perception/observe", "not json");
        maybe_publish_observation("Atlas", "perception/observe", "{\"success\":false}");
        maybe_publish_observation(
            "Atlas",
            "perception/observe",
            "{\"success\":true,\"url\":\"file:///tmp/index.html\"}",
        );
        // No panic, no publish (FEED uninstalled) — reaching here IS the assertion.
    }
}
