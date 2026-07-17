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

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::grid::acl::required_trust;
use crate::modules::grid::node::TrustLevel;

/// One room's Join Contract — the whole activity as a declaration.
///
/// This is the outlier-validated shape: it fits a **benchmark** room (structured,
/// gradeable, ephemeral — [`benchmark_experience`]) and a **chat** room (social,
/// freeform, durable — [`chat_experience`]) with identical structure. If it holds
/// at both extremes, every other experience (profile, settings, a live video call,
/// med-bay, a game world) is an interpolation — a point between these anchors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/experience/Experience.ts")]
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
}

/// A view-intent: one surface of the room bound to a live payload `kind`. Carries
/// *where it belongs* and *how central it is* — never how it looks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/experience/Region.ts")]
#[serde(rename_all = "camelCase")]
pub struct Region {
    /// Stable region name within the room (`"scoreboard"`, `"messages"`, `"roster"`).
    pub name: String,
    /// The live payload `kind` this region binds to — the same string an
    /// `ObserverSpec` subscribes to and a `StateEnvelope` carries (e.g. a
    /// `ChatViewState`'s `KIND`). The renderer resolves the payload by this kind.
    pub kind: String,
    /// Which layout zone this region belongs to.
    pub scope: RegionScope,
    /// How central this region is within its zone.
    pub role: RegionRole,
    /// Whether the region streams (subscribe + re-render on revision) or is static.
    pub live: bool,
}

/// The layout zone a region belongs to — the who/where/which axes made structural
/// (`docs/architecture/LAYOUT-PHILOSOPHY.md`). Meaning, not pixels: desktop maps
/// these to three draggable panes, mobile to a segmented full-screen + nav, an
/// agent to grouped `observe` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/experience/RegionScope.ts")]
#[serde(rename_all = "lowercase")]
pub enum RegionScope {
    /// App-wide, same across every room (rooms list, live activities). Left/explorer.
    Global,
    /// The current activity's own content. Center.
    Activity,
    /// The current activity's context/inspector (scoreboard, participants,
    /// recipe-scoped tools). Right.
    Inspector,
}

/// How central a region is within its zone — drives emphasis, not size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/experience/RegionRole.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/experience/Affordance.ts")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/experience/ProofSpec.ts")]
#[serde(rename_all = "camelCase")]
pub enum ProofSpec {
    /// No attestation — an ordinary action.
    None,
    /// The result is measured on a quiesced (clean) lane — the `cleanLane` honesty
    /// stamp (`[[benchmark-numbers-carry-gpu-provenance]]`).
    CleanLane,
    /// The result carries a signed forge-alloy `IntegrityAttestation`.
    Attestation,
}

/// A participant and their structural standing in the room.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/experience/Member.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/experience/Standing.ts")]
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
/// Hand-built here to validate the type against the real anchor; the eventual
/// source is a `RoomPurposeSource` projecting a recipe (task #6) — this becomes
/// data, not code. Region kind `"benchmark"` binds to the live `BenchmarkObserveResult`
/// surface (`cognition/observe`); per-region kinds (scoreboard/central/feed as their
/// own subscribable ViewStates) are a migration follow-up.
pub fn benchmark_experience(examinee_peer_id: &str, owner_peer_id: &str) -> Experience {
    Experience {
        purpose: "benchmark/hard-rs".to_string(),
        regions: vec![
            Region {
                name: "scoreboard".to_string(),
                kind: "benchmark".to_string(),
                scope: RegionScope::Inspector,
                role: RegionRole::Primary,
                live: true,
            },
            Region {
                name: "central".to_string(),
                kind: "benchmark".to_string(),
                scope: RegionScope::Activity,
                role: RegionRole::Primary,
                live: true,
            },
            Region {
                name: "feed".to_string(),
                kind: "benchmark".to_string(),
                scope: RegionScope::Activity,
                role: RegionRole::Peripheral,
                live: true,
            },
        ],
        affordances: vec![
            // Read-only observe — AiSafe → Provisional, any citizen may watch.
            Affordance::for_command("observe", "cognition/observe"),
        ],
        membership: vec![
            Member { peer_id: examinee_peer_id.to_string(), standing: Standing::Examinee },
            Member { peer_id: owner_peer_id.to_string(), standing: Standing::Owner },
        ],
    }
}

/// The **chat** room's manifest — outlier B (social · freeform · durable). Primary
/// surface is a durable message stream; roster is peripheral context.
///
/// Region kind is single-sourced from `ChatViewState::KIND` — the same `"chat"`
/// string the live `StateEnvelope` carries — so the manifest can never drift from
/// the payload it points at. Chat's send verb routes through airc (there is no
/// `chat/*` command in continuum-core), so no affordance is fabricated here; the
/// airc-routed post affordance is added when that command surfaces on this plane.
pub fn chat_experience(owner_peer_id: &str, member_peer_id: &str) -> Experience {
    let chat_kind = continuum_positron::ChatViewState::KIND.to_string();
    Experience {
        purpose: "chat".to_string(),
        regions: vec![
            Region {
                name: "messages".to_string(),
                kind: chat_kind.clone(),
                scope: RegionScope::Activity,
                role: RegionRole::Primary,
                live: true,
            },
            Region {
                name: "roster".to_string(),
                kind: chat_kind,
                scope: RegionScope::Inspector,
                role: RegionRole::Peripheral,
                live: true,
            },
        ],
        affordances: vec![],
        membership: vec![
            Member { peer_id: owner_peer_id.to_string(), standing: Standing::Owner },
            Member { peer_id: member_peer_id.to_string(), standing: Standing::Member },
        ],
    }
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

        // Benchmark: structured, its primary surface is the score in the inspector.
        assert_eq!(bench.purpose, "benchmark/hard-rs");
        assert!(bench
            .regions
            .iter()
            .any(|r| r.name == "scoreboard"
                && r.scope == RegionScope::Inspector
                && r.role == RegionRole::Primary));
        assert!(bench.membership.iter().any(|m| m.standing == Standing::Examinee));

        // Chat: social, its primary surface is the message stream in the activity zone.
        assert_eq!(chat.purpose, "chat");
        assert!(chat
            .regions
            .iter()
            .any(|r| r.name == "messages"
                && r.scope == RegionScope::Activity
                && r.role == RegionRole::Primary));
        // Region kind is single-sourced from the live ViewState, never a literal.
        assert!(chat.regions.iter().all(|r| r.kind == continuum_positron::ChatViewState::KIND));

        // Both are the SAME type — the whole point.
        for exp in [&bench, &chat] {
            let json = serde_json::to_string(exp).expect("Experience must serialize");
            let round: Experience = serde_json::from_str(&json).expect("and round-trip");
            assert_eq!(&&round, &exp);
        }
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
