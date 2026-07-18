//! `foundry` projection — folds the live `ModelCatalog` into a positron
//! [`ForgeViewState`] (`kind = "foundry"`).
//!
//! The foundry activity's `Content`/`ContextPanel` is the forge workbench; its
//! right-hand model `Listing` is the available model universe. This module is the
//! projection half of ACTIVITY-ROOM-PATTERNS.md brick 1: a pure fold from the
//! catalogue's `CatalogSnapshot` (the canonical `watch`-published model state,
//! #78) to the neutral `ForgeViewState` wire type. Like the chat/wall
//! projections, it is REPLACE-on-change — each catalogue generation rebuilds the
//! whole `models` vector, so a removed model is reflected by absence.
//!
//! Pure by design (snapshot in → view out, no I/O): the live wiring that
//! subscribes to the catalogue `watch::Receiver` and publishes a rebuilt view to
//! the `Substrate` per foundry room lands with structured room-purpose tagging
//! (`RoomPurposeSource`, #6) — a foundry room needs to exist to project *for*.
//! Keeping the fold pure keeps it unit-testable without a running catalogue.

use uuid::Uuid;

use continuum_positron::foundry::{ForgeModelView, ForgeViewState};

use crate::model_registry::live::{CatalogSnapshot, LiveModel};

/// Project one catalogue entry into a foundry model `Listing` row. Display fields
/// only — the renderer draws them and never re-resolves from the catalogue.
fn forge_model_view(live: &LiveModel) -> ForgeModelView {
    let m = &live.model;
    ForgeModelView {
        model_id: m.id.clone(),
        // Name is optional on the row; fall back to the id so a row always labels
        // itself, never blank.
        display_name: m.name.clone().unwrap_or_else(|| m.id.clone()),
        source: m.provider.clone(),
        // The "0 means unknown → None" contract lives once, on the Model helper —
        // read it, don't restate the sentinel here ([[fallbacks-are-illegal-fail-loud]],
        // one-decision-one-place).
        params_b: m.parameter_count_billions(),
    }
}

/// Build the foundry view for a room from a catalogue snapshot. `purpose` is
/// single-sourced from the view's own `KIND` ("foundry"), so it can never drift
/// from the wire kind. Models come out in the snapshot's deterministic `BTreeMap`
/// order (stable `models/list` ordering, stable tests).
pub fn build_forge_view(
    room_id: Uuid,
    room_name: String,
    snapshot: &CatalogSnapshot,
) -> ForgeViewState {
    ForgeViewState {
        room_id,
        room_name,
        purpose: ForgeViewState::KIND.to_string(),
        models: snapshot.models.values().map(forge_model_view).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use crate::model_registry::live::{Availability, LiveModel, ModelStatus};
    use crate::model_registry::types::{Arch, Model};

    fn model(id: &str, name: Option<&str>, provider: &str, params: u64) -> LiveModel {
        // Full literal — `Model`/`ModelStatus` don't derive `Default` (a "default
        // model" is meaningless); only the fields under test carry meaning here.
        LiveModel {
            model: Model {
                id: id.to_string(),
                name: name.map(str::to_string),
                provider: provider.to_string(),
                arch: Arch::Qwen35,
                context_window: 4096,
                max_output_tokens: 512,
                tokens_per_second: 10.0,
                capabilities: BTreeSet::new(),
                cost_input_per_1k: 0.0,
                cost_output_per_1k: 0.0,
                gguf_hint: None,
                hf_source: None,
                gguf_local_path: None,
                mmproj_local_path: None,
                chat_template: None,
                multi_party_strategy: Default::default(),
                stop_sequences: Vec::new(),
                parameter_count: params,
                sampling: crate::model_registry::types::ModelSampling::default(),
                persona_serving_eligible: true,
            },
            status: ModelStatus {
                availability: Availability::Ready,
                verified: None,
            },
        }
    }

    fn snapshot(models: Vec<LiveModel>) -> CatalogSnapshot {
        let mut snap = CatalogSnapshot::default();
        for lm in models {
            snap.models.insert(lm.model.id.clone(), lm);
        }
        snap
    }

    // what this catches: an empty catalogue projects a valid empty foundry view
    // (empty workbench, not an error), with the purpose single-sourced to the kind.
    #[test]
    fn empty_catalogue_projects_empty_foundry_view() {
        let view = build_forge_view(Uuid::from_u128(0xf), "foundry".into(), &snapshot(vec![]));
        assert_eq!(view.purpose, "foundry");
        assert_eq!(view.purpose, ForgeViewState::KIND);
        assert!(view.models.is_empty());
    }

    // what this catches: catalogue rows map to Listing rows — id/name/source carry
    // through, a missing name falls back to the id (never blank), and param count
    // converts to billions.
    #[test]
    fn catalogue_rows_project_into_model_listing() {
        let view = build_forge_view(
            Uuid::from_u128(0xf),
            "foundry".into(),
            &snapshot(vec![
                model(
                    "Qwen/Qwen3-4B",
                    Some("Qwen3 4B"),
                    "huggingface",
                    4_000_000_000,
                ),
                model("local/asha", None, "local", 3_000_000_000),
            ]),
        );
        assert_eq!(view.models.len(), 2);
        // BTreeMap order: "Qwen/..." sorts before "local/..." (uppercase < lowercase).
        let qwen = &view.models[0];
        assert_eq!(qwen.model_id, "Qwen/Qwen3-4B");
        assert_eq!(qwen.display_name, "Qwen3 4B");
        assert_eq!(qwen.source, "huggingface");
        assert_eq!(qwen.params_b, Some(4.0));
        // no name → id is the label, never blank
        assert_eq!(view.models[1].display_name, "local/asha");
    }

    // what this catches: an unknown parameter count (0) projects to None, not a
    // fabricated 0.0 — the honest-unknown discipline ForgeModelView promises.
    #[test]
    fn unknown_param_count_projects_none() {
        let view = build_forge_view(
            Uuid::from_u128(0xf),
            "foundry".into(),
            &snapshot(vec![model("m/x", Some("X"), "local", 0)]),
        );
        assert_eq!(view.models[0].params_b, None);
    }
}
