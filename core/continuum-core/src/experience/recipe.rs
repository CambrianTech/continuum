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

use super::{Affordance, Experience, Layout, Member, ProofSpec, Region};

/// The authored, data-only shape of an [`Experience`] — everything a recipe owns,
/// nothing the system computes. Projected to a full manifest by [`Self::project`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/experience/ExperienceRecipe.ts")]
#[serde(rename_all = "camelCase")]
pub struct ExperienceRecipe {
    /// The activity nature — the content-dispatch key and this recipe's table key.
    pub purpose: String,
    /// The regions this room surfaces (authored verbatim — regions carry no
    /// computed fields).
    pub regions: Vec<Region>,
    /// The verbs available, authored as intent (`{verb, command, proves?}`); each
    /// gains its computed `who_may` at projection.
    #[serde(default)]
    pub affordances: Vec<AffordanceRecipe>,
    /// Optional explicit composition (level-3 layout). Omitted → organic placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub layout: Option<Layout>,
}

/// The authored shape of an [`Affordance`] — the verb and its command plus the
/// proof its result yields. `who_may` is intentionally ABSENT: it is computed from
/// the ACL at projection so authorization can never be forged in a recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/experience/AffordanceRecipe.ts")]
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
        serde_json::from_str(json)
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
