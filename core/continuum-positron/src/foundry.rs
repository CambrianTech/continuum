//! `kind = "foundry"` — the foundry activity's room content.
//!
//! Per `docs/architecture/ACTIVITY-ROOM-PATTERNS.md`, an activity IS a room and a
//! room's `purpose` selects its content. A `"foundry"` room's content is the
//! forge workbench: configuration/recipes in the centre and the available model
//! catalogue as the right-hand context `Listing` (the "HuggingFace model list on
//! the right" the doc names). This is outlier B to chat — same shell, a different
//! `Content` + `ContextPanel`, proving the pattern seam generalizes past chat.
//!
//! Started intentionally minimal (like `ChatViewState` was): the data-backed
//! **model listing** lands first, fed from the live `ModelCatalog` (#78); the
//! centre config/recipe widgets grow the struct additively (additive ts-rs deltas
//! are wire-compatible), and the projection that folds catalogue → this state is a
//! sibling slice. Mirrors `WallViewState`'s shape/discipline (a `kind` top-level
//! payload that is a first-class positron `ViewState`, replace-on-change).

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// One row of the foundry model `Listing` — an available base/genome the
/// workbench can forge from. Display fields only; a renderer draws these (a
/// `Listing` cell) and does not re-resolve from the catalogue, the same
/// source-of-truth discipline `WallPostView` keeps for authorship.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/ForgeModelView.ts"
)]
pub struct ForgeModelView {
    /// Stable catalogue id a renderer keys a row on (e.g. a HF repo id
    /// `"Qwen/Qwen3-4B"` or a local model id). Opaque — the substrate makes no
    /// inference from its shape.
    pub model_id: String,
    /// Human-readable name for the row.
    pub display_name: String,
    /// Where the model comes from (`"huggingface"`, `"local"`, a provider name).
    /// Consumer-defined, open string — grouped/filtered by the renderer the way
    /// middleware filters on a header, never an enum
    /// ([[room-purpose-is-per-recipe-not-an-enum]]).
    pub source: String,
    /// Parameter count in billions, when the catalogue knows it. `None` renders as
    /// an unlabelled row rather than a fabricated `0` ([[fallbacks-are-illegal-fail-loud]]).
    #[ts(optional)]
    pub params_b: Option<f32>,
}

/// Top-level state for `kind = "foundry"` — the focused room plus the model
/// catalogue it can forge from. REPLACE-on-change, like the wall: each catalogue
/// change re-reads the whole set and swaps `models`, so a removed model is
/// reflected by absence, never a stale merged row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/ForgeViewState.ts"
)]
pub struct ForgeViewState {
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Human-readable room name. Substrate-resolved; the renderer must not derive
    /// it from a URL slug.
    pub room_name: String,
    /// The room's activity purpose — `"foundry"` for a foundry room. Carried for
    /// the same reason `ChatViewState` carries it: the client's `Content` primitive
    /// dispatches on it (`activity == room == content == tab`). Neutral/opaque —
    /// positron transports it, continuum sets it (`RoomPurposeSource`, #6).
    pub purpose: String,
    /// The available models as the foundry's right-hand context `Listing`. Empty
    /// when the catalogue is empty — an empty workbench renders an empty list, not
    /// an error.
    pub models: Vec<ForgeModelView>,
}

/// `ForgeViewState` is a first-class positron `ViewState` — the SAME contract
/// renderers and the O6 observer bridge key off for chat/wall, so a foundry
/// widget routes through the identical seam, not a continuum-private shape.
/// Sibling of `ChatViewState`/`WallViewState`'s impl.
impl ForgeViewState {
    /// The on-wire `kind` string this view is published under. Owned by the view
    /// (open self-registration), NOT a central enum: a new view adds a file, never
    /// edits a shared catalog. Unknown kind on the wire fails loud at the dispatch
    /// seam — `[[fallbacks-are-illegal-fail-loud]]` preserved without a closed enum.
    pub const KIND: &'static str = "foundry";
}

impl positron_core::ViewState for ForgeViewState {
    fn kind(&self) -> &'static str {
        // Single-source the wire string through the view's own `KIND` const so the
        // trait's view of the kind can never drift from the envelope's.
        Self::KIND
    }

    // `revision()` is the trait default (`None`), same as chat/wall: the monotonic
    // revision is an ENVELOPE-level counter framed in by `StateBuilder`, not a
    // payload field — one counter per kind, no drifting copy on the struct.
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a field rename / type tweak that breaks the serde wire
    // shape (the minimum bar for a wire type), and that the model listing + the
    // optional param count round-trip.
    #[test]
    fn forge_view_state_round_trips() {
        let state = ForgeViewState {
            room_id: Uuid::from_u128(0xf),
            room_name: "foundry".into(),
            purpose: "foundry".into(),
            models: vec![
                ForgeModelView {
                    model_id: "Qwen/Qwen3-4B".into(),
                    display_name: "Qwen3 4B".into(),
                    source: "huggingface".into(),
                    params_b: Some(4.0),
                },
                ForgeModelView {
                    model_id: "local/asha-coder".into(),
                    display_name: "Asha Coder".into(),
                    source: "local".into(),
                    params_b: None,
                },
            ],
        };
        let json = serde_json::to_string(&state).expect("serialize");
        let back: ForgeViewState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
        assert_eq!(back.models.len(), 2);
        assert_eq!(back.models[1].params_b, None);
    }

    // what this catches: the kind is the view's own KIND const (open
    // self-registration), never a drifting literal — the dispatch seam routes on it.
    #[test]
    fn forge_view_state_is_a_positron_view_state() {
        use positron_core::ViewState;
        let state = ForgeViewState {
            room_id: Uuid::from_u128(0xf),
            room_name: "foundry".into(),
            purpose: "foundry".into(),
            models: vec![],
        };
        assert_eq!(state.kind(), "foundry");
        assert_eq!(state.kind(), ForgeViewState::KIND);
        // The monotonic revision is an ENVELOPE-level counter, never a payload
        // field — the same contract chat/wall pin (chat.rs / wall.rs sibling tests).
        assert_eq!(state.revision(), None);
    }

    // what this catches: a foundry room with an empty catalogue is a valid view —
    // an empty workbench renders an empty list, not an error (the same honest-empty
    // discipline wall's `empty_board_is_a_valid_view_not_an_error` pins).
    #[test]
    fn empty_models_is_a_valid_view() {
        let state = ForgeViewState {
            room_id: Uuid::from_u128(0xf),
            room_name: "foundry".into(),
            purpose: "foundry".into(),
            models: vec![],
        };
        let json = serde_json::to_string(&state).expect("serialize");
        let back: ForgeViewState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
        assert!(back.models.is_empty());
    }
}
