//! Recipe-as-data authoring surface for the Join Contract — the "author once,
//! extend indefinitely" seam. A room's [`Experience`] manifest is authored as
//! DATA (`recipes/*.json`), not a hand-written Rust builder; a new experience is a
//! new recipe entry, **zero code**.
//!
//! The recipe carries only what an author legitimately owns — `purpose`, `regions`,
//! the verbs (`affordances` as `{verb, command}`), and an optional `layout`. It does
//! NOT carry:
//! - **`who_may`** — authorization is *computed* from the live ACL at
//!   [`ExperienceRecipe::project`] time (`acl::required_trust`), so an authored
//!   manifest can never advertise a trust tier the door doesn't enforce
//!   (`[[fallbacks-are-illegal-fail-loud]]` sibling: trust is derived at the seam,
//!   never hand-set).
//! - **`membership`** — that is live roster state, filled by the runtime, not the
//!   author.
//!
//! This keeps the ACL invariant that [`Experience`]'s tests pin: `who_may` always
//! equals `required_trust(command)`, whether the manifest was hand-built or loaded
//! from recipe JSON.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::{Affordance, Experience, Layout, Member, ProofSpec, Region};

/// Stable identity for a recipe. Task #274.
///
/// A newtype rather than a bare `Uuid` so a recipe id can never be passed where a
/// room id, peer id, or card id is expected — the same discipline #396 is applying
/// to airc identities. Serializes as a plain UUID string, so authored JSON stays
/// readable and hand-editable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/RecipeId.ts"
)]
#[serde(transparent)]
pub struct RecipeId(#[ts(type = "string")] pub Uuid);

impl RecipeId {
    /// Build from a literal — how `shipped::` constants are authored, so the
    /// prod-critical set is `const`-nameable and never a magic string in core code.
    pub const fn from_u128(value: u128) -> Self {
        Self(Uuid::from_u128(value))
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// The identity an authored recipe gets when its file names no `id`.
    ///
    /// A real RFC 4122 v5 UUID under a fixed namespace — deterministic, so every
    /// node that loads the same authored file derives the same identity without
    /// coordinating, and collision-resistant, unlike the readable stems a human
    /// (or a model) reaches for when inventing an id by hand.
    pub fn derived_from_purpose(purpose: &str) -> Self {
        Self(Uuid::new_v5(&RECIPE_NAMESPACE, purpose.as_bytes()))
    }
}

/// The v5 namespace all purpose-derived recipe ids live under. Itself a genuine v4,
/// generated once and frozen: changing it re-identifies every derived recipe.
const RECIPE_NAMESPACE: Uuid = Uuid::from_u128(0x2a9a1f1e_8bd2_4a56_9c0f_7d3f1a4e6b85);

impl std::fmt::Display for RecipeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Recipes authored before `version` existed are revision 1, not revision 0 — a
/// recipe that exists has shipped at least once.
fn default_version() -> u32 {
    1
}

/// The authored, data-only shape of an [`Experience`] — everything a recipe owns,
/// nothing the system computes. Projected to a full manifest by [`Self::project`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/ExperienceRecipe.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ExperienceRecipe {
    /// Stable identity. Task #274.
    ///
    /// Recipes are open — shipped in the binary, authored on disk, installed at
    /// runtime, GENERATED on the fly — so identity cannot be a Rust enum and must
    /// not be a name. `purpose` used to carry identity AND taxonomy at once, which
    /// is why `benchmark` (a family) and `benchmark/hard-rs` (an instance) were
    /// indistinguishable and why a room could bind to a purpose that resolves to
    /// nothing and silently render as plain chat.
    ///
    /// Deliberately NOT a content hash: a shipped recipe is prod-critical and
    /// long-lived, so a bugfix to `chat.json` must not orphan every chat room in
    /// existence. Identity survives content edits. Reproducibility lives in the RUN
    /// RECEIPT instead — `(recipe_id, version, content_hash)` — which is what pins
    /// the exact bytes an exam was administered under without making identity
    /// brittle. Same split forge-alloy uses for models.
    pub id: RecipeId,

    /// Which revision of this recipe. Bumped on install when the id already exists,
    /// so a room's receipt can name the revision it ran under.
    #[serde(default = "default_version")]
    pub version: u32,

    /// The activity nature — the content-dispatch key and this recipe's table key.
    ///
    /// As of #274 this is a hierarchical LABEL (`benchmark/hard-rs`,
    /// `academy/bench/swe-lite`), not identity: it groups for discovery and drives
    /// UI sectioning, while [`Self::id`] is what anything resolves by.
    pub purpose: String,
    /// The regions this room surfaces (authored verbatim — regions carry no
    /// computed fields).
    pub regions: Vec<Region>,
    /// The verbs available, authored as intent (`{verb, command, proves?}`); each
    /// gains its computed `who_may` at projection.
    #[serde(default)]
    pub affordances: Vec<AffordanceRecipe>,
    /// The resident citizens this experience wants HOSTED — the roster as
    /// authored DATA (#430). The DEFAULT experience's citizens are the node's
    /// resident population: what the persona spawner plans at boot, replacing
    /// the hardcoded `plan_for_tier` role vec. Empty (the default) means this
    /// experience declares no residents — for most activity recipes that is
    /// the ordinary state; membership is live roster state, not authorship.
    #[serde(default)]
    pub citizens: Vec<CitizenRecipe>,
    /// Optional explicit composition (level-3 layout). Omitted → organic placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub layout: Option<Layout>,
}

/// One resident citizen an experience declares (#430) — a ROLE, not an
/// identity. Identity (peer_id, name) is minted or resumed by the identity
/// provider at hosting time; the MODEL is the serving daemon's per-host
/// decision. A recipe authoring a device-specific model id would make the
/// recipe non-portable across the grid, so the role is all it declares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/CitizenRecipe.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CitizenRecipe {
    /// Role identifier (snake_case: `helper`, `coder`, `sentinel`, …) —
    /// `persona/role_template::RoleId`'s own serde form.
    #[ts(type = "string")]
    pub role: crate::persona::role_template::RoleId,
}

/// The authored shape of an [`Affordance`] — the verb and its command plus the
/// proof its result yields. `who_may` is intentionally ABSENT: it is computed from
/// the ACL at projection so authorization can never be forged in a recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/AffordanceRecipe.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct AffordanceRecipe {
    /// The user-facing verb.
    pub verb: String,
    /// The command URI this affordance fires.
    pub command: String,
    /// What proof the result yields. Defaults to [`ProofSpec::None`].
    #[serde(default)]
    pub proves: ProofSpec,
}

impl ExperienceRecipe {
    /// Parse a recipe from its JSON text (an embedded `recipes/*.json` or a loaded
    /// recipe file). Fails loud on malformed data — a broken embedded recipe is a
    /// build-time authoring bug, caught by the tests.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut value: serde_json::Value = serde_json::from_str(json)?;

        // An author writing a recipe file must never have to hand-mint a UUID —
        // "author a file, zero code" includes zero `uuidgen`. A recipe that arrives
        // without an id gets one DERIVED from its purpose (RFC 4122 v5), so the
        // same authored file resolves to the same identity on every node that ever
        // loads it. That determinism is what makes it safe across a partition: two
        // machines that independently load the same file agree, with nothing to
        // reconcile. A recipe that DOES carry an id keeps it verbatim — shipped
        // recipes pin theirs so identity survives a purpose rename.
        if let Some(object) = value.as_object_mut() {
            if !object.contains_key("id") {
                if let Some(purpose) = object.get("purpose").and_then(|p| p.as_str()) {
                    let derived = RecipeId::derived_from_purpose(purpose);
                    object.insert("id".into(), serde_json::json!(derived.to_string()));
                }
                // No purpose either → fall through and let serde report the missing
                // field, which names the real authoring error.
            }
        }

        serde_json::from_value(value)
    }

    /// Project this authored recipe into a live [`Experience`]: compute each
    /// affordance's `who_may` from the ACL and attach the runtime `membership`.
    /// This is the `RoomPurposeSource → Experience` projection made concrete — the
    /// data becomes the manifest, with authorization and roster supplied by the
    /// system, never the author.
    pub fn project(self, membership: Vec<Member>) -> Experience {
        Experience {
            purpose: self.purpose,
            regions: self.regions,
            affordances: self
                .affordances
                .into_iter()
                .map(|a| Affordance::for_command(&a.verb, &a.command).proving(a.proves))
                .collect(),
            membership,
            layout: self.layout,
        }
    }
}
