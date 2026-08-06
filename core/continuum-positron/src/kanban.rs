//! Typed kanban payloads — `KanbanViewState`, the substrate-shaped view
//! of a room's **work board**: cards (open → merged) grouped into lanes,
//! that fills `StateEnvelope.payload` for `kind="kanban"`.
//!
//! ## The kanban is the room's work board — the third face of a room
//!
//! [`crate::chat::ChatViewState`] projects the room's *conversation*,
//! [`crate::wall::WallViewState`] its *pinned documents*; this projects
//! the room's *work*: the cards agents claim, move through states, land
//! as PRs. In airc these are event-sourced work rows — `CardCreated`,
//! `CardStateChanged`, `WorkCardClaimed`, `LaneCreated` — folded by
//! airc's `WorkBoardProjection` into a current board. A human or persona
//! files/claims/moves a card with the airc work verbs; a widget renders
//! it here. One shared data layer, many faces — so the kanban widget is
//! defined once for web, terminal, mobile, and AI-observer surfaces
//! alike, exactly as the wall is.
//!
//! ## These fields are a VIEW onto airc-owned state
//!
//! `room_id` is the airc `RoomId`; each card/lane rides airc's
//! event-sourced work transcript. The **current board** is airc's fold
//! (`Airc::work_board` → `BoardSnapshot`) — the projector re-reads that
//! authority on each work change rather than re-implementing the
//! event fold (compression: one board projection, in airc — the same
//! discipline the wall projector applies to `wall_posts()`). This struct
//! is the projection the renderer reads; the airc rows are the truth,
//! never a second store of them.
//!
//! ## positron mirrors airc's enums, never depends on airc
//!
//! positron is a neutral, standalone contract library — it MUST NOT
//! depend on `airc-work`. So the card/lane *state* vocabularies here
//! ([`KanbanCardState`], [`KanbanLaneState`], [`KanbanPriority`]) MIRROR
//! airc's `CardState`/`LaneState`/`Priority` variant-for-variant, exactly
//! as [`crate::chat::SenderKind`] mirrors airc's actor kind. The
//! continuum-side projector maps airc → positron at the seam; the wire
//! schema stays license-clean and framework-neutral.
//!
//! ## Why structs, not `serde_json::Value`
//!
//! Same rationale as `chat.rs`/`wall.rs`
//! (`[[strong-typing-across-boundaries]]`): the substrate types here ARE
//! the schema; ts-rs mirrors them; the widget side reads typed objects,
//! not `unknown`. The schema grows by extending these structs; the wire
//! kind string stays `"kanban"`.
//!
//! ## Attribution is woven in from day one
//!
//! Filing a card is an authored, accountable act, and claiming one is a
//! second accountable act — a persona seeing "who filed this / who's
//! working it" must read them off the board's face, per
//! `[[positron-identity-security-first-class]]`. So the card's CREATOR
//! carries the SAME identity axes a chat/wall author does
//! ([`crate::chat::SenderKind`] + [`crate::chat::Provenance`] + the
//! opaque `integrations` badge map), resolved from the room roster at the
//! substrate side. The current ASSIGNEE (claimant) is a lighter status
//! pointer — a resolved id + display name — since it annotates *state*
//! ("claimed by X") rather than an authored document. positron stays
//! neutral: an AI creator is an `Agent`; whose agent it is rides
//! `integrations`, read at the app layer.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::chat::{Provenance, SenderKind};

/// A card's lifecycle state — mirrors airc's `CardState`
/// variant-for-variant. positron owns this copy so the contract library
/// stays free of an `airc-work` dependency; the continuum projector maps
/// airc `CardState` → this at the seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/KanbanCardState.ts"
)]
pub enum KanbanCardState {
    Open,
    Claimed,
    InProgress,
    Blocked,
    Review,
    Merged,
    Closed,
}

/// Whether a claimed card's LEASE is still good — the fact a column alone
/// cannot carry.
///
/// A claim is a lease, not a permanent assignment. When it expires the holder
/// has stopped and the card is takeable, but `state` still reads `Claimed`,
/// so a board rendering only the column shows dead work as active work.
/// Measured 2026-08-06: 19 cards, 17 expired leases, and six citizens across
/// two machines spent a night reporting they had nothing to do. The renderer
/// must be able to grey a lapsed hold and offer it, which requires the
/// substrate to SAY it — renderers never re-derive
/// ([[fallbacks-are-illegal-fail-loud]]).
///
/// Kept free of an `airc-work` dependency like [`KanbanCardState`]; the
/// continuum projector maps its `card_holder::Hold` → this at the seam, so
/// the persona's board line and the human's card agree by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/positron/KanbanHold.ts")]
pub enum KanbanHold {
    /// A live claim — someone is genuinely on this card right now.
    Held,
    /// The lease expired; the holder stopped. Takeable, and `assignee_name`
    /// names who to ask before taking it.
    Lapsed,
    /// No claim at all.
    Unclaimed,
}

/// A lane's state — mirrors airc's `LaneState` variant-for-variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/KanbanLaneState.ts"
)]
pub enum KanbanLaneState {
    Planned,
    Active,
    Blocked,
    Landing,
    Done,
}

/// A card's priority — mirrors airc's `Priority` (P0 highest … P3
/// lowest; P2 the airc default). Kept as the same 4-level scale (P0
/// highest) in the same declaration order so a renderer can sort/badge
/// off the wire strings without inventing a mapping. (This is a
/// wire-vocabulary contract, not a Rust `Ord` one — ordering never
/// crosses the boundary; the consumer sorts on the string values.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/KanbanPriority.ts"
)]
pub enum KanbanPriority {
    P0,
    P1,
    P2,
    P3,
}

/// A card's pull-request link — the landing half of the work item.
/// Mirrors the render-relevant slice of airc's `PullRequestRef` (repo +
/// number); the branch pair is a build detail the board face doesn't
/// need. `None` until a PR is opened for the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/KanbanPullRequest.ts"
)]
pub struct KanbanPullRequest {
    /// Repository the PR targets, as airc's `RepoId` display string
    /// (e.g. `"CambrianTech/continuum"`). Opaque to positron.
    pub repo: String,
    /// PR number within `repo`.
    #[ts(type = "number")]
    pub number: u64,
}

/// One card on the work board — the bits the kanban widget needs to
/// render a board entry.
///
/// `card_id`, `room_id`, `creator_id`, `assignee_id` are continuum's
/// substrate UUIDs rendered as strings on the wire (the ts-rs default
/// for `Uuid`) — airc's `WorkCardId` / `PeerId` projected to their inner
/// UUIDs at the seam.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/KanbanCardView.ts"
)]
pub struct KanbanCardView {
    /// Stable id of the card across its event history — the anchor a
    /// renderer keys a board row on, and the key a lane's `card_ids`
    /// references.
    #[ts(type = "string")]
    pub card_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Short imperative title — the card's headline on the board.
    pub title: String,
    /// Optional longer description. `None` for a bare title-only card;
    /// the renderer shows an expandable body when present. Markdown by
    /// convention (no substrate-side schema).
    #[ts(optional)]
    pub body: Option<String>,
    /// Current lifecycle state — which column the card sits in.
    pub state: KanbanCardState,
    /// Priority badge (P0 highest … P3 lowest).
    pub priority: KanbanPriority,
    /// The lane this card belongs to, if any — matches a
    /// [`KanbanLaneView::lane_id`] in the same board. `None` for an
    /// unlaned card (the board renders it outside any lane grouping).
    #[ts(optional, type = "string | null")]
    pub lane_id: Option<Uuid>,
    /// Peer that filed the card (airc's authoritative `created_by`). The
    /// identity below is resolved from this id via the roster, never
    /// carried on the card itself — same discipline as `WallPostView`
    /// (identity is a presence fact, not a content fact).
    #[ts(type = "string")]
    pub creator_id: Uuid,
    /// Display name resolved at the substrate side. Renderers must not
    /// re-resolve from `creator_id`. Provisional (a short peer-id label)
    /// until the creator's presence card folds in, then upgraded in
    /// place ([[fallbacks-are-illegal-fail-loud]]).
    pub creator_name: String,
    /// Neutral creator kind. `Agent` covers every AI creator; whose agent
    /// it is rides `integrations`, read at the app layer.
    pub creator_kind: SenderKind,
    /// Opaque cross-system identity badges, transported straight from the
    /// authoritative airc `Identity.integrations`. positron does NOT
    /// interpret these — the app layer does. Empty until the creator's
    /// card resolves.
    #[ts(type = "Record<string, string>")]
    pub integrations: BTreeMap<String, String>,
    /// Verifiable provenance of the creator — the accountability half of
    /// identity. Makes a filed card attributable on its face; grows to
    /// carry trust tier + verification with no wire break.
    pub provenance: Provenance,
    /// Peer currently claiming/owning the card (airc's `owner`), if any.
    /// A status pointer, not an authored act — so it carries a resolved
    /// id + name rather than the full creator identity axes. `None` for
    /// an unclaimed (Open) card.
    #[ts(optional, type = "string | null")]
    pub assignee_id: Option<Uuid>,
    /// Display name of the assignee, resolved substrate-side from
    /// `assignee_id` exactly as `creator_name` is. `None` iff
    /// `assignee_id` is `None`.
    #[ts(optional)]
    pub assignee_name: Option<String>,
    /// Whether the assignee's LEASE is still live — see [`KanbanHold`].
    /// `Lapsed` means takeable: the renderer should show it as available and
    /// name `assignee_name` as who to ask, not as who is busy with it.
    /// Without this the board renders a hold that died hours ago exactly like
    /// one someone is actively working, which is the defect that stalled six
    /// citizens for a night.
    pub hold: KanbanHold,
    /// Landing link, once a PR exists for the card. `None` before then.
    #[ts(optional)]
    pub pull_request: Option<KanbanPullRequest>,
    /// Unix-ms airc creation time — the board MAY show card age.
    #[ts(type = "number")]
    pub created_at: u64,
    /// Unix-ms time of the card's most recent event (state change,
    /// claim, edit) — the board renders recency from this.
    #[ts(type = "number")]
    pub updated_at: u64,
}

/// One lane on the work board — a named grouping of cards (a swimlane /
/// epic / milestone). Projected from airc's `LaneRecord`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/KanbanLaneView.ts"
)]
pub struct KanbanLaneView {
    #[ts(type = "string")]
    pub lane_id: Uuid,
    /// Lane title — the swimlane's header.
    pub title: String,
    /// Lane lifecycle state (planned → done).
    pub state: KanbanLaneState,
    /// Ids of the cards assigned to this lane, in airc's stored order.
    /// Each matches a [`KanbanCardView::card_id`] whose `lane_id` points
    /// back here. A renderer MAY render the lane from these ids or by
    /// filtering `cards` on `lane_id`; both agree because the projector
    /// re-reads the whole board on each change.
    #[ts(type = "Array<string>")]
    pub card_ids: Vec<Uuid>,
}

/// Top-level kanban state for `kind="kanban"` — the focused room plus its
/// current work board (lanes + cards).
///
/// The board is a REPLACE-on-change projection: each work change re-reads
/// airc's `work_board()` fold and swaps the whole `lanes`/`cards`
/// vectors, so a closed/merged card leaving the active board is reflected
/// by absence, never a stale merged entry — the same "full snapshot,
/// replace not merge" discipline the wall board and chat roster use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/KanbanViewState.ts"
)]
pub struct KanbanViewState {
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Lanes on the board, in airc's stored order. Empty when the room
    /// has no lanes (cards may still exist, unlaned) — an empty board is
    /// a valid view, not an error ([[substrate-is-a-good-citizen-on-the-host]]).
    pub lanes: Vec<KanbanLaneView>,
    /// Cards on the board, in airc's stored order. Empty when the room
    /// has no active work.
    pub cards: Vec<KanbanCardView>,
}

/// `KanbanViewState` is a first-class positron `ViewState` — the SAME
/// contract renderers (positron-lit's `LitHost`) and the O6 observer
/// bridge key off for chat and wall, so the kanban widget routes through
/// the identical seam, not a continuum-private shape. Sibling of
/// `WallViewState`'s impl: a `kind="kanban"` top-level payload is a
/// `ViewState` exactly as `kind="wall"` is.
///
/// The `Clone + Send + Sync + Debug + 'static` bound the trait requires
/// is already satisfied by the struct's derives + its owned-data fields,
/// so no new bounds are introduced.
impl KanbanViewState {
    /// The on-wire `kind` string this view is published under. Owned by
    /// the view (open self-registration), NOT a central enum: a new view
    /// adds a file, never edits a shared catalog. Unknown kind on the
    /// wire fails loud at the dispatch seam — `[[fallbacks-are-illegal-fail-loud]]`
    /// preserved without a closed enum.
    pub const KIND: &'static str = "kanban";
}

impl positron_core::ViewState for KanbanViewState {
    fn kind(&self) -> &'static str {
        // Single-source the wire string through the view's own `KIND`
        // const — the same "kanban" `StateEnvelope.kind` carries — so the
        // trait's view of the kind can never drift from the envelope's
        // ([[strong-typing-across-boundaries]]: encoded once, on the type).
        Self::KIND
    }

    // `revision()` is intentionally the trait default (`None`), for the
    // same reason as `WallViewState`: the monotonic kanban revision is an
    // ENVELOPE-level counter (`Revisions` keyed by the kind string, framed
    // in by `StateBuilder`), NOT a payload field. Carrying a copy here
    // would be two sources of truth for one counter (`[[compression]]`).
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_card() -> KanbanCardView {
        KanbanCardView {
            card_id: Uuid::from_u128(1),
            room_id: Uuid::from_u128(9),
            title: "Wire the kanban projector".to_string(),
            body: Some("Mirror the wall projector against work_board().".to_string()),
            state: KanbanCardState::InProgress,
            priority: KanbanPriority::P1,
            lane_id: Some(Uuid::from_u128(5)),
            creator_id: Uuid::from_u128(2),
            creator_name: "Asha".to_string(),
            creator_kind: SenderKind::Agent,
            integrations: BTreeMap::from([("continuum.persona".to_string(), "asha".to_string())]),
            provenance: Provenance::unresolved(),
            assignee_id: Some(Uuid::from_u128(3)),
            assignee_name: Some("BigMama".to_string()),
            hold: KanbanHold::Held,
            pull_request: Some(KanbanPullRequest {
                repo: "CambrianTech/continuum".to_string(),
                number: 1735,
            }),
            created_at: 1_720_000_000_000,
            updated_at: 1_720_000_500_000,
        }
    }

    fn sample_lane() -> KanbanLaneView {
        KanbanLaneView {
            lane_id: Uuid::from_u128(5),
            title: "positron define-once".to_string(),
            state: KanbanLaneState::Active,
            card_ids: vec![Uuid::from_u128(1)],
        }
    }

    #[test]
    fn kanban_view_state_round_trips() {
        // what this catches: a serde-shape regression on the wire type the
        // kanban widget's ts-rs binding is generated from. If a field is
        // renamed / retyped / dropped, the JSON the renderer parses drifts
        // from the generated TS — a silent UI bug this pins to a compile-
        // and-assert failure. Mirrors wall::tests::wall_view_state_round_trips.
        let state = KanbanViewState {
            room_id: Uuid::from_u128(9),
            lanes: vec![sample_lane()],
            cards: vec![sample_card()],
        };
        let json = serde_json::to_string(&state).expect("serialize");
        let back: KanbanViewState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
    }

    #[test]
    fn empty_board_is_a_valid_view_not_an_error() {
        // what this catches: the "room with no active work renders an empty
        // board" contract. A regression that made `cards`/`lanes` required-
        // non-empty, or that omitted the fields, would break the honest-
        // empty projection the projector depends on. Mirrors
        // wall::tests::empty_board_is_a_valid_view_not_an_error.
        let state = KanbanViewState {
            room_id: Uuid::from_u128(9),
            lanes: vec![],
            cards: vec![],
        };
        let json = serde_json::to_string(&state).expect("serialize");
        let back: KanbanViewState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
        assert!(back.cards.is_empty());
        assert!(back.lanes.is_empty());
    }

    #[test]
    fn unclaimed_card_has_no_assignee_or_pr() {
        // what this catches: the optionality contract for Open cards — an
        // unclaimed, un-PR'd card must serialize with absent assignee/PR
        // (not an empty-string or zero placeholder that a renderer would
        // mis-badge as "claimed by ''"). Guards against a projector or
        // wire-shape change that fills these with defaults instead of
        // `None` ([[fallbacks-are-illegal-fail-loud]]).
        let card = KanbanCardView {
            state: KanbanCardState::Open,
            lane_id: None,
            assignee_id: None,
            assignee_name: None,
            pull_request: None,
            body: None,
            ..sample_card()
        };
        let json = serde_json::to_string(&card).expect("serialize");
        let back: KanbanCardView = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(card, back);
        assert!(back.assignee_id.is_none());
        assert!(back.assignee_name.is_none());
        assert!(back.pull_request.is_none());
    }

    #[test]
    fn card_states_serialize_snake_case() {
        // what this catches: drift between positron's mirrored card-state
        // wire strings and airc's `CardState` (which is `snake_case` serde).
        // The projector maps airc→positron by VALUE, but the wire JSON a
        // renderer reads must match airc's vocabulary so a board shared
        // across surfaces agrees on column names. A rename here (or losing
        // `rename_all`) is a silent cross-surface mismatch.
        assert_eq!(
            serde_json::to_string(&KanbanCardState::InProgress).unwrap(),
            "\"in_progress\""
        );
        assert_eq!(
            serde_json::to_string(&KanbanLaneState::Landing).unwrap(),
            "\"landing\""
        );
        assert_eq!(
            serde_json::to_string(&KanbanPriority::P0).unwrap(),
            "\"p0\""
        );
    }

    #[test]
    fn kanban_view_state_is_a_positron_view_state() {
        // what this catches: regression where `KanbanViewState` stops being
        // a positron `ViewState` (the impl deleted, or `kind()` hand-rolled
        // to a literal that drifts from the wire name). Renderers and the O6
        // observer bridge route/subscribe off `ViewState::kind()`; if it
        // stops equalling the `StateEnvelope.kind` the substrate emits
        // (`KanbanViewState::KIND`), the kanban widget silently
        // receives state it can't match to a renderer. Also pins
        // `revision()` to the trait default (`None`): the kanban revision is
        // an envelope-level counter, never a payload field. Mirrors
        // wall::tests::wall_view_state_is_a_positron_view_state.
        use positron_core::ViewState;
        let state = KanbanViewState {
            room_id: Uuid::from_u128(9),
            lanes: vec![],
            cards: vec![],
        };
        assert_eq!(state.kind(), "kanban");
        assert_eq!(
            state.kind(),
            KanbanViewState::KIND,
            "ViewState::kind() must single-source the view's own KIND const, never a drifting literal"
        );
        assert_eq!(state.revision(), None);
    }
}
