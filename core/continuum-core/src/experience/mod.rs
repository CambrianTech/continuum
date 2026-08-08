//! `experience/*` — the **Join Contract**: one typed manifest per room that
//! declares what the room *is*, so every paradigm can render and act on it
//! without per-experience code.
//!
//! See `docs/architecture/THE-JOIN-CONTRACT.md`. An **Experience** = room =
//! activity = content = tab (`[[activity-room-content-tab-pattern-primitives]]`),
//! keyed by the airc `RoomId`. It is the **manifest layer** that was missing above
//! positron's existing substrate primitives — not a replacement for them:
//!
//! | Layer | What it is | Cadence | Where it lives |
//! |---|---|---|---|
//! | `StateEnvelope{kind}` / `ObserverSpec` | a region's **live reactive payload** (messages tick, scoreboard ticks) | high — every revision | positron (`ChatViewState`, `BenchmarkObserveResult`, …) |
//! | **`Experience` (this module)** | the room's **manifest** — which regions exist, their scope/role, the affordances, membership | low — on join / membership change | here |
//!
//! Define-once → render-many: a **generic renderer reads this manifest** and lays
//! its `regions[]` out in its *native* idiom — desktop draggable panes, mobile
//! segmented full-screen, an agent's `cognition/observe` fields, a persona's RAG
//! proprioception, an AR panel, a TUI cell — then binds each region to its live
//! payload by `kind`. A new experience is a new manifest + registered region
//! kinds, **zero renderer code** (`[[app-shell-layout-left-global-right-per-activity]]`).
//! The contract carries *intent* (region role/scope, verb, authz) and **never
//! pixels**, so every paradigm stays organically itself.
//!
//! ## What is reused vs. new
//! Almost all of it composes primitives already carrying production traffic:
//! `ObserverSpec`/`StateEnvelope`/`StateLayer` (subscriptions), `CommandEnvelope`/
//! `CommandSource` (fan-in), `RosterSlotView` (roster projection),
//! `is_command_authorized`/`TrustLevel` (authz). The genuinely-new type-space is
//! small: [`Region`] (+ [`RegionScope`]/[`RegionRole`], codifying
//! `LAYOUT-PHILOSOPHY.md`'s prose), [`Standing`] (absent everywhere in airc), and
//! the [`Affordance`] composition. `purpose` is the open string a
//! `RoomPurposeSource` already resolves (task #6) — never an enum
//! (`[[room-purpose-is-per-recipe-not-an-enum]]`).
//!
//! ## Where provenance lives
//! Deliberately NOT a manifest field. The honesty stamp (benchmark's
//! `cleanLane` → `BenchmarkProvenance`) rides on the flowing region **payload**,
//! where "is this number a lie?" is a fact about the live truth, not the static
//! frame. The manifest points at the region; the region's ViewState carries its
//! own provenance. Keeping it there avoids a second Clean/Contended/Unknown type
//! (`[[compression]]`).
//!
//! ## Layout flexibility — any app, three levels, never pixels
//! The contract must express *any* app, not the 3-pane form we happened to start
//! with. It does so at three levels, none of which embeds HTML/React (those couple
//! to one renderer and break on AR / TUI / a persona's RAG — they are **compile
//! targets** a renderer maps intent onto, not the contract):
//!
//! 1. **Inside a region — unbounded.** A [`Region`]'s `kind` is open: a chat, a
//!    scoreboard, a form, a WebGL canvas, a whole game. The manifest never
//!    constrains a region's internal content.
//! 2. **Across regions, default — semantic, not spatial.** A region carries
//!    *meaning* ([`RegionScope`] = app-wide vs this-activity; [`RegionRole`] =
//!    emphasis; `slot` = an open hint like `"content"`/`"context"`/`"nav"`). Each
//!    paradigm maps that meaning to *its own* layout — the 3-pane is only the
//!    desktop renderer's mapping; mobile → full-screen + sheets, a dashboard →
//!    grid, AR → floating panels. Nothing is pane-locked.
//! 3. **Across regions, explicit — the builder.** An optional [`Layout`] tree
//!    (`row`/`col`/`grid`/`stack`/`tabs` over regions, with relative *weights*,
//!    never pixels) for authors who want a specific composition. Always optional;
//!    a renderer that can't honor it falls back to level 2.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::grid::acl::required_trust;
use crate::modules::grid::node::TrustLevel;

pub mod membership;
pub mod recipe;
pub mod source;
pub mod standing;

pub use membership::project_membership;
pub use recipe::{AffordanceRecipe, ExperienceRecipe};
pub use source::{ExperienceSource, RecipeExperienceSource, SharedExperienceSource};

/// One room's Join Contract — the whole activity as a declaration.
///
/// This is the outlier-validated shape: it fits a **benchmark** room (structured,
/// gradeable, ephemeral — [`benchmark_experience`]) and a **chat** room (social,
/// freeform, durable — [`chat_experience`]) with identical structure. If it holds
/// at both extremes, every other experience (profile, settings, a live video call,
/// med-bay, a game world) is an interpolation — a point between these anchors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/Experience.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct Experience {
    /// The activity nature — the content-dispatch key (`"chat"`, `"benchmark/hard-rs"`,
    /// `"profile"`, `"settings"`, …). Open string, recipe-defined, resolved by a
    /// `RoomPurposeSource`; NEVER an enum. A client dispatches on this to pick the
    /// room's renderer set.
    pub purpose: String,
    /// View-intent: the regions this room surfaces, each bound to a live payload
    /// `kind`. A renderer lays these out in its own idiom by `scope` + `role`.
    pub regions: Vec<Region>,
    /// Fan-in: the verbs available in this room, each carrying its authz predicate
    /// and the proof its result yields.
    pub affordances: Vec<Affordance>,
    /// Who is in the room and their structural standing.
    pub membership: Vec<Member>,
    /// OPTIONAL explicit composition (layout level 3). `None` (the common case) =
    /// the renderer lays regions out from their `scope`/`role`/`slot` (level 2, and
    /// each paradigm does it organically). `Some` = a renderer-agnostic tree of
    /// `row`/`col`/`grid`/`stack`/`tabs` over the regions, sized by relative weight,
    /// never pixels — the "builder" for authors who want a specific arrangement. A
    /// renderer that can't honor it falls back to level 2, so it never traps an
    /// experience in one form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub layout: Option<Layout>,
}

impl Experience {
    /// The on-wire `kind` a room's manifest publishes under — the value a
    /// `StateEnvelope` carries and a renderer routes on (like `ChatViewState::KIND`).
    /// Open self-registration; never a central enum. Paired with
    /// `StateBuilder::session_raw` at the emit site so `Experience` stays
    /// renderer-agnostic (no positron-core dependency on the contract type).
    pub const KIND: &'static str = "experience";
}

/// An optional, renderer-agnostic composition tree over a room's regions — the
/// level-3 layout escape hatch. Containers nest freely; a leaf names a [`Region`].
/// Sizing is by relative `weight` (flex-like), **never pixels**, so a phone, a wide
/// desktop, and an AR panel each realize the same intent at their own scale.
/// HTML/React/SwiftUI/TUI are compile targets a renderer maps this onto — never
/// embedded here (that would couple the contract to one surface).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/Layout.ts"
)]
#[serde(tag = "container", rename_all = "lowercase")]
pub enum Layout {
    /// Leaf: the region with this `name`.
    Region { name: String },
    /// Children arranged left-to-right.
    Row { children: Vec<LayoutChild> },
    /// Children arranged top-to-bottom.
    Col { children: Vec<LayoutChild> },
    /// Children z-stacked / overlaid (last on top).
    Stack { children: Vec<LayoutChild> },
    /// Children flowed into `cols` columns.
    Grid {
        cols: u32,
        children: Vec<LayoutChild>,
    },
    /// Children as tabs — one visible at a time.
    Tabs { children: Vec<LayoutChild> },
}

/// A child in a [`Layout`] with its relative sizing weight (flex-like share of the
/// parent, never pixels). `weight: None` = an equal share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/LayoutChild.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct LayoutChild {
    /// The nested container or region leaf.
    pub node: Layout,
    /// Relative sizing share among siblings. `None` = equal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub weight: Option<f32>,
}

impl LayoutChild {
    /// A leaf child naming a region, with an optional weight — the common builder call.
    pub fn region(name: &str, weight: Option<f32>) -> Self {
        Self {
            node: Layout::Region {
                name: name.to_string(),
            },
            weight,
        }
    }
}

/// A view-intent: one surface of the room bound to a live payload `kind`. Carries
/// *where it belongs* and *how central it is* — never how it looks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/Region.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    /// Stable region name within the room (`"scoreboard"`, `"messages"`, `"roster"`).
    pub name: String,
    /// The live payload `kind` this region binds to — the same string an
    /// `ObserverSpec` subscribes to and a `StateEnvelope` carries (e.g. a
    /// `ChatViewState`'s `KIND`). The renderer resolves the payload by this kind.
    pub kind: String,
    /// Where this region sits in the app hierarchy (app-wide vs this activity).
    pub scope: RegionScope,
    /// How much emphasis this region gets.
    pub role: RegionRole,
    /// Open semantic hint for what this region is FOR (`"content"`, `"context"`,
    /// `"nav"`, `"composer"`, `"status"`) — a renderer maps it to its own idiom
    /// (our desktop shell routes `"context"` to the right inspector, `"content"` to
    /// the center tab). Open string, never an enum
    /// (`[[room-purpose-is-per-recipe-not-an-enum]]`); `None` = let `scope`/`role`
    /// decide placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub slot: Option<String>,
    /// Whether the region streams (subscribe + re-render on revision) or is static.
    pub live: bool,
}

/// Where a region sits in the app hierarchy — the only paradigm-INDEPENDENT
/// structural axis (the who/where split, `docs/architecture/LAYOUT-PHILOSOPHY.md`).
/// Deliberately NOT a pane count: a *renderer* maps these to its own zones. Our
/// desktop (VS Code-style) shell maps `Global` → the left rail and `Activity` → the
/// current tab, then uses `role`/`slot` to place activity regions (`"content"` →
/// center, `"context"`/peripheral → the right inspector). Mobile maps `Global` →
/// drawer + bottom tabs and `Activity` → the full screen; a dashboard renderer may
/// ignore scope and drive purely off [`Experience::layout`]. The 3-pane form is one
/// renderer's choice, never the contract's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/RegionScope.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum RegionScope {
    /// App-wide, the same across every room (rooms list, live activities, identity).
    Global,
    /// This activity's own surface — its content and its context.
    Activity,
}

/// How central a region is within its zone — drives emphasis, not size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/RegionRole.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum RegionRole {
    /// The thing the room is *for* (the message stream; the score + its provenance).
    Primary,
    /// Supporting context (roster; recent-run feed).
    Peripheral,
}

/// Fan-in: a verb a participant can invoke in the room. The one genuinely-new weld
/// of the Join Contract — it folds authz (`who_may`, the same gate
/// `is_command_authorized` enforces at the door) and proof (`proves`) into the SAME
/// artifact the renderer shows, so trust is a spine through the contract, never a
/// layer bolted on after.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/Affordance.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct Affordance {
    /// The user-facing verb (`"observe"`, `"post"`, `"quiesce"`).
    pub verb: String,
    /// The command URI a `CommandEnvelope` carries when this affordance fires.
    pub command: String,
    /// Minimum caller [`TrustLevel`] required — the wire projection of the ACL's
    /// required tier (`[[adaptive-tool-surface-meets-you-in-the-middle]]`). `None`
    /// = the command is local-only and must not be offered to a remote caller.
    pub who_may: Option<TrustLevel>,
    /// What proof this affordance's result yields — the forge-alloy seam.
    pub proves: ProofSpec,
}

impl Affordance {
    /// Build an affordance for a real command, deriving `who_may` from the ACL —
    /// the single source of "who may invoke this," never a hand-set tier that could
    /// drift from what the door actually enforces.
    pub fn for_command(verb: &str, command: &str) -> Self {
        Self {
            verb: verb.to_string(),
            command: command.to_string(),
            who_may: required_trust(command),
            proves: ProofSpec::None,
        }
    }

    /// Set the proof this affordance's result carries (builder style).
    pub fn proving(mut self, proves: ProofSpec) -> Self {
        self.proves = proves;
        self
    }
}

/// What proof an affordance's *result* yields — the forge-alloy plane's foothold in
/// the contract. Minimal today; the concrete carriers already exist in tree
/// (`QuiesceLease`/`cleanLane` for [`ProofSpec::CleanLane`]; forge-alloy's
/// `IntegrityAttestation`/`AlloyReceipt` for [`ProofSpec::Attestation`]) and get
/// bound in a follow-up slice. Settlement/invoice remain forge-alloy aspiration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/ProofSpec.ts"
)]
#[serde(rename_all = "camelCase")]
pub enum ProofSpec {
    /// No attestation — an ordinary action.
    #[default]
    None,
    /// The result is measured on a quiesced (clean) lane — the `cleanLane` honesty
    /// stamp (`[[benchmark-numbers-carry-gpu-provenance]]`).
    CleanLane,
    /// The result carries a signed forge-alloy `IntegrityAttestation`.
    Attestation,
}

/// A participant and their structural standing in the room.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/Member.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    /// The canonical who — the airc `PeerId`, stringified for the wire
    /// (`[[identity-context-session-three-axes]]`).
    pub peer_id: String,
    /// This participant's structural role in *this* room.
    pub standing: Standing,
}

/// A participant's structural role in a room — cross-cutting, distinct from their
/// identity bio-`role` and from the room's `purpose`. Unlike `purpose` (an open,
/// recipe-defined string), standing is a small closed set of structural join-roles.
/// New type-space: no `Standing` exists anywhere in airc today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/Standing.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum Standing {
    /// Owns the room (the human host; creator).
    Owner,
    /// The subject under measurement (the benchmark examinee).
    Examinee,
    /// Present, observing, not driving (a spectator; an observer-agent).
    Watcher,
    /// A regular participant.
    Member,
    /// Present with limited standing (unpaired / provisional).
    Guest,
}

/// The **benchmark** room's manifest — outlier A (structured · gradeable ·
/// ephemeral). Primary surface is *a number and how much to trust it*.
///
/// Now hydrated from recipe DATA: the static manifest (purpose / regions /
/// affordances / layout) is authored in the embedded `recipes/benchmark.json` and
/// projected via [`recipe::ExperienceRecipe`]; this fn only supplies the runtime
/// examinee/owner roster. `who_may` on the observe affordance is COMPUTED from the
/// ACL at projection, never authored. This is the "manifests are recipe content"
/// property — the builder is a thin roster-hydrator, not a hand-authored manifest.
pub fn benchmark_experience(examinee_peer_id: &str, owner_peer_id: &str) -> Experience {
    recipe::ExperienceRecipe::from_json(include_str!("recipes/benchmark.json"))
        .expect("embedded benchmark recipe must be valid JSON")
        .project(vec![
            Member {
                peer_id: examinee_peer_id.to_string(),
                standing: Standing::Examinee,
            },
            Member {
                peer_id: owner_peer_id.to_string(),
                standing: Standing::Owner,
            },
        ])
}

/// The **chat** room's manifest — outlier B (social · freeform · durable). Primary
/// surface is a durable message stream; roster is peripheral context.
///
/// Hydrated from `recipes/chat.json` plus the owner/member roster. The recipe's
/// region `kind` is `"chat"`; the `both_outliers_fit_one_contract` test pins that
/// value to `ChatViewState::KIND`, so the authored data cannot drift from the live
/// `StateEnvelope` payload it points at. Chat's send verb routes through airc (no
/// `chat/*` command exists in continuum-core), so the recipe declares no affordance
/// yet — the airc-routed post affordance is added when that command surfaces here.
pub fn chat_experience(owner_peer_id: &str, member_peer_id: &str) -> Experience {
    recipe::ExperienceRecipe::from_json(include_str!("recipes/chat.json"))
        .expect("embedded chat recipe must be valid JSON")
        .project(vec![
            Member {
                peer_id: owner_peer_id.to_string(),
                standing: Standing::Owner,
            },
            Member {
                peer_id: member_peer_id.to_string(),
                standing: Standing::Member,
            },
        ])
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the outlier validation itself — if a future change makes
    // the Join Contract fit chat but not benchmark (or vice versa), one of these
    // constructors stops compiling or stops round-tripping. The two anchors are
    // maximally different (structured/ephemeral vs social/durable); holding both
    // is the proof every other experience is interpolation.
    #[test]
    fn both_outliers_fit_one_contract() {
        let bench = benchmark_experience("examinee-1", "joel");
        let chat = chat_experience("joel", "asha");

        // Benchmark: structured, its primary surface is the score — activity-scoped,
        // slotted as context (the desktop shell routes that to the right inspector),
        // emphasised as primary. Placement is meaning, not a hardcoded pane.
        assert_eq!(bench.purpose, "benchmark/hard-rs");
        assert!(bench.regions.iter().any(|r| r.name == "scoreboard"
            && r.scope == RegionScope::Activity
            && r.slot.as_deref() == Some("context")
            && r.role == RegionRole::Primary));
        assert!(bench
            .membership
            .iter()
            .any(|m| m.standing == Standing::Examinee));
        // Benchmark opts into explicit composition (level 3); chat does not (level 2).
        assert!(
            bench.layout.is_some(),
            "benchmark declares an explicit layout tree"
        );
        assert!(
            chat.layout.is_none(),
            "chat relies on organic semantic placement"
        );

        // Chat: social, its primary surface is the message stream in the activity zone.
        assert_eq!(chat.purpose, "chat");
        assert!(chat.regions.iter().any(|r| r.name == "messages"
            && r.scope == RegionScope::Activity
            && r.role == RegionRole::Primary));
        // Region kinds bind to their live ViewStates (path-3 per-region decomposition):
        // messages → ChatViewState::KIND, roster → RosterViewState::KIND. Single-sourced
        // from the payload consts, never literals.
        assert!(chat
            .regions
            .iter()
            .any(|r| r.name == "messages" && r.kind == continuum_positron::ChatViewState::KIND));
        assert!(chat
            .regions
            .iter()
            .any(|r| r.name == "roster" && r.kind == continuum_positron::RosterViewState::KIND));

        // Both are the SAME type — the whole point.
        for exp in [&bench, &chat] {
            let json = serde_json::to_string(exp).expect("Experience must serialize");
            let round: Experience = serde_json::from_str(&json).expect("and round-trip");
            assert_eq!(&&round, &exp);
        }
    }

    // what this catches: the level-3 layout tree must express ARBITRARY composition
    // — not just the 3-pane form we started with — without pixels. If a refactor
    // ever collapsed Layout back toward a fixed pane model, a grid-of-tabs would stop
    // round-tripping. Proves a designer isn't limited to the VS Code shell's shape.
    #[test]
    fn layout_tree_expresses_arbitrary_composition() {
        // A 2-column dashboard whose left cell is itself a tab group — nothing a
        // fixed 3-pane scope could describe.
        let dash = Layout::Grid {
            cols: 2,
            children: vec![
                LayoutChild {
                    node: Layout::Tabs {
                        children: vec![
                            LayoutChild::region("chart", None),
                            LayoutChild::region("table", None),
                        ],
                    },
                    weight: Some(2.0),
                },
                LayoutChild::region("sidebar", Some(1.0)),
            ],
        };
        let exp = Experience {
            purpose: "dashboard".to_string(),
            regions: vec![],
            affordances: vec![],
            membership: vec![],
            layout: Some(dash),
        };
        let json = serde_json::to_string(&exp).expect("arbitrary layout serializes");
        // Tagged by `container` — a clean discriminated union on the wire.
        assert!(json.contains(r#""container":"grid""#));
        assert!(json.contains(r#""container":"tabs""#));
        let round: Experience = serde_json::from_str(&json).expect("and round-trips");
        assert_eq!(round, exp);
    }

    // what this catches: `who_may` must be DERIVED from the ACL, never hand-set —
    // otherwise an affordance could advertise a trust tier the door doesn't enforce
    // (or under-advertise and confuse a caller). `cognition/observe` is AiSafe, which
    // the ACL maps to Provisional; if that mapping regresses, this fails.
    #[test]
    fn observe_affordance_authz_tracks_the_real_acl() {
        let bench = benchmark_experience("examinee-1", "joel");
        let observe = bench
            .affordances
            .iter()
            .find(|a| a.verb == "observe")
            .expect("benchmark offers observe");
        assert_eq!(observe.command, "cognition/observe");
        assert_eq!(
            observe.who_may,
            Some(TrustLevel::Provisional),
            "observe is AiSafe → Provisional; who_may must equal required_trust(command)"
        );
    }
}
