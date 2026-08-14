//! `RoomPurposeSource → Experience` — resolving a room to its manifest from recipe
//! DATA. A room's `purpose` (the existing [`RoomPurposeSource`] seam) is the
//! dispatch key into a `purpose → recipe` table; the recipe projects to the live
//! [`Experience`]. Adding an experience is adding a recipe; the projection code is
//! written once.

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::ipc::room_purpose::{RoomPurposeSource, SharedRoomPurpose};

use super::recipe::{ExperienceRecipe, RecipeId};
use super::Experience;

/// Resolves a room to its authored [`Experience`] manifest. Membership (live roster)
/// is filled by the runtime, not this source — the returned manifest carries an
/// empty `membership` for the caller to populate from the room's roster.
pub trait ExperienceSource: Send + Sync {
    /// The manifest for a room, or `None` if this source has no recipe for the
    /// room's purpose (the caller may fall back to another source or a default —
    /// never a silent stand-in, `[[fallbacks-are-illegal-fail-loud]]`).
    fn experience_for(&self, room_id: Uuid) -> Option<Experience>;
}

/// Shared handle to an [`ExperienceSource`].
pub type SharedExperienceSource = Arc<dyn ExperienceSource>;

/// What can go wrong loading authored recipes off disk. Every variant names the
/// FILE, because the person debugging is the person who just wrote that file and
/// the only useful answer is which one and why.
#[derive(Debug)]
pub enum RecipeLoadError {
    /// The recipes directory exists but could not be enumerated.
    ReadDir {
        dir: std::path::PathBuf,
        source: std::io::Error,
    },
    /// A recipe file could not be read.
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    /// A recipe file is not a valid [`ExperienceRecipe`]. NOT skipped — an author
    /// who wrote a recipe believes it is live, and a silently-ignored experience is
    /// indistinguishable from one that was never authored.
    Parse {
        path: std::path::PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for RecipeLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadDir { dir, source } => {
                write!(
                    f,
                    "cannot read recipes directory {}: {source}",
                    dir.display()
                )
            }
            Self::Read { path, source } => {
                write!(f, "cannot read recipe {}: {source}", path.display())
            }
            Self::Parse { path, source } => write!(
                f,
                "recipe {} is not a valid experience recipe: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for RecipeLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadDir { source, .. } | Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

/// Typed names for the recipes that ship IN the binary — the prod-critical set.
///
/// Task #274. Constants, deliberately NOT an enum: recipes are open (authored on
/// disk, installed at runtime, generated on the fly), and an enum is a closed set
/// that would have to be edited and recompiled for every new activity — the exact
/// "if authoring an activity requires a compiler, activities get hand-made
/// instead" failure this module's own header warns about, and which produced a
/// bring-up room still collecting citizens two weeks after its activity ended.
///
/// So: named handles onto an OPEN registry. Core code references a shipped recipe
/// by constant instead of spelling a magic string, and everything else resolves by
/// id at runtime. Each constant must equal the `id` authored in its JSON —
/// `every_shipped_constant_resolves` pins that, so the two can never drift.
pub mod shipped {
    use super::RecipeId;

    // Genuine v4 UUIDs, grouped exactly as the canonical form they mirror in the
    // authored JSON so the two are checkable by eye. Never hand-drawn patterns: an
    // id with a readable prefix is a name wearing a UUID's costume, and it collides
    // the moment anyone else picks the same cute stem.

    /// The Rust-coder gym (`benchmark/hard-rs`) — `fed332c3-383c-45bb-a205-b54b02c43916`.
    pub const BENCHMARK_HARD_RS: RecipeId =
        RecipeId::from_u128(0xfed332c3_383c_45bb_a205_b54b02c43916);
    /// Ordinary conversation (`chat`) — `8d0f0435-93be-4468-bd26-4f8cdd4693ee`.
    pub const CHAT: RecipeId = RecipeId::from_u128(0x8d0f0435_93be_4468_bd26_4f8cdd4693ee);
    /// A citizen's or user's profile surface (`profile`) — `089ac0da-d65a-4922-8cdc-36e7f589c465`.
    pub const PROFILE: RecipeId = RecipeId::from_u128(0x089ac0da_d65a_4922_8cdc_36e7f589c465);
    /// Real-time voice/video (`video-chat`) — `6bc4fc12-a1c3-4482-ab2c-eb48505d52d3`.
    pub const VIDEO_CHAT: RecipeId = RecipeId::from_u128(0x6bc4fc12_a1c3_4482_ab2c_eb48505d52d3);

    /// Every shipped id, for tests and for enumerating the prod-critical floor.
    pub const ALL: &[RecipeId] = &[BENCHMARK_HARD_RS, CHAT, PROFILE, VIDEO_CHAT];
}

/// An [`ExperienceSource`] backed entirely by recipe DATA: a `purpose → recipe`
/// table, keyed by the room's purpose (resolved through the injected
/// [`RoomPurposeSource`]). This is the concrete `RoomPurposeSource → Experience`
/// projection — the manifests are recipe content, not Rust builders.
pub struct RecipeExperienceSource {
    /// purpose → authored recipe. Still the RESOLUTION key today: rooms bind by
    /// purpose on the wall, and #274 migrates that to id in a later slice. This
    /// slice adds identity without moving resolution, so nothing breaks in flight.
    by_purpose: HashMap<String, ExperienceRecipe>,
    /// id → authored recipe. The index resolution moves to (#274 step 2), and
    /// already the honest answer to "which exact recipe is this".
    by_id: HashMap<RecipeId, ExperienceRecipe>,
    /// room_id → purpose (the existing seam this projection composes on top of).
    purpose: SharedRoomPurpose,
}

impl RecipeExperienceSource {
    /// Build from an explicit set of recipes (e.g. loaded from `system/recipes/*`).
    /// A later recipe with the same purpose replaces an earlier one (last wins),
    /// the same override semantics a recipe loader gives.
    pub fn new(
        purpose: SharedRoomPurpose,
        recipes: impl IntoIterator<Item = ExperienceRecipe>,
    ) -> Self {
        let by_purpose: HashMap<String, ExperienceRecipe> = recipes
            .into_iter()
            .map(|r| (r.purpose.clone(), r))
            .collect();
        // Same last-wins semantics as `by_purpose`: an overlay recipe replacing a
        // built-in replaces it in BOTH indexes, so the two can never disagree about
        // which recipe won.
        let by_id = by_purpose
            .values()
            .map(|r| (r.id, r.clone()))
            .collect::<HashMap<_, _>>();
        Self {
            by_purpose,
            by_id,
            purpose,
        }
    }

    /// Resolve by identity — what everything moves to in #274 step 2.
    pub fn by_recipe_id(&self, id: RecipeId) -> Option<&ExperienceRecipe> {
        self.by_id.get(&id)
    }

    /// The ids this source can resolve. Companion to [`Self::purposes`]; this is
    /// the list `activity/spawn` will name when refusing an unknown recipe, so the
    /// refusal is actionable instead of a silent chat room.
    pub fn ids(&self) -> impl Iterator<Item = RecipeId> + '_ {
        self.by_id.keys().copied()
    }

    /// The built-in experiences shipped with the core, authored as embedded recipe
    /// JSON (`recipes/*.json`). Fails loud if an embedded recipe is malformed — that
    /// is a build-time authoring bug, pinned by the tests.
    pub fn builtins(purpose: SharedRoomPurpose) -> Self {
        Self::new(purpose, Self::embedded())
    }

    /// Every purpose the SHIPPED recipe set declares. Derived from the SAME
    /// [`Self::embedded`] iterator the registry is built from, so this list
    /// and the embedded resolution set cannot drift. Production validation
    /// goes through [`Self::known_purposes`], which is this PLUS the disk
    /// overlay — the same union [`Self::builtins_with_overlay`] resolves.
    pub fn shipped_purposes() -> Vec<String> {
        Self::embedded().map(|r| r.purpose).collect()
    }

    /// The ONE directory production overlays recipes from:
    /// `<continuum_root>/recipes`. Named here so the resolution side
    /// ([`Self::builtins_with_overlay`] in the positron projection) and the
    /// validation side (`activity/spawn`'s `validate_recipe`) derive the same
    /// path from the same root — two hand-built joins would drift, and a
    /// drifted pair means an authored recipe that RESOLVES gets REFUSED at
    /// spawn (or the reverse, the #431 silent-chat downgrade).
    pub fn overlay_dir(continuum_root: &std::path::Path) -> std::path::PathBuf {
        continuum_root.join("recipes")
    }

    /// Every purpose the FULL recipe set declares — embedded floor plus the
    /// disk overlay under `dir` (#432). This is the validation list
    /// `activity/spawn` refuses against, and it is the same union
    /// [`Self::builtins_with_overlay`] builds its registry from, so an
    /// authored on-disk recipe is spawnable the moment the file exists. A
    /// malformed overlay recipe is an ERROR naming the file, not a silent
    /// skip — the author believes it is live.
    pub fn known_purposes(dir: &std::path::Path) -> Result<Vec<String>, RecipeLoadError> {
        let overlay = Self::load_dir(dir)?;
        Ok(Self::embedded()
            .map(|r| r.purpose)
            .chain(overlay.into_iter().map(|r| r.purpose))
            .collect())
    }

    /// The node's RESIDENT roles — the roster as recipe data (#430). Read from
    /// the DEFAULT experience's `citizens` (the `chat` recipe — the room fresh
    /// minds land in, #298), embedded floor overlaid by `dir` with the same
    /// later-wins-by-purpose law the registry uses: author a `chat.json` on
    /// disk and the node's resident roster changes with zero code.
    pub fn resident_roles(
        dir: &std::path::Path,
    ) -> Result<Vec<crate::persona::role_template::RoleId>, RecipeLoadError> {
        let overlay = Self::load_dir(dir)?;
        Ok(Self::resident_roles_from(Self::embedded().chain(overlay)))
    }

    /// [`Self::resident_roles`] over the EMBEDDED set alone — infallible, for
    /// tests/fixtures and as the documented loud-refusal arm when the overlay
    /// directory fails to load (same shape as the positron projection's #432
    /// refusal: serve the shipped floor, never go dark over one bad file).
    pub fn resident_roles_embedded() -> Vec<crate::persona::role_template::RoleId> {
        Self::resident_roles_from(Self::embedded())
    }

    fn resident_roles_from(
        recipes: impl Iterator<Item = ExperienceRecipe>,
    ) -> Vec<crate::persona::role_template::RoleId> {
        let default_purpose = Self::shipped_purpose(shipped::CHAT)
            .expect("embedded chat recipe must exist — build-time authoring bug");
        recipes
            .filter(|r| r.purpose == default_purpose)
            .last()
            .map(|r| r.citizens.into_iter().map(|c| c.role).collect())
            .unwrap_or_default()
    }

    /// The declared purpose of a shipped recipe, by its [`shipped`] handle.
    /// Lets core call sites bind rooms by CONSTANT (`shipped::BENCHMARK_HARD_RS`)
    /// while the purpose string stays authored in exactly one place — the
    /// recipe JSON. `None` only if the handle names no embedded recipe, which
    /// is a build-time authoring bug the shipped-constants test pins.
    pub fn shipped_purpose(id: RecipeId) -> Option<String> {
        Self::embedded().find(|r| r.id == id).map(|r| r.purpose)
    }

    /// The embedded seed set. These ship IN the binary so a fresh clone has working
    /// experiences with zero files on disk — the same self-provisioning contract the
    /// rest of the substrate holds. They are a FLOOR, not the catalogue: anything on
    /// disk overlays them via [`Self::builtins_with_overlay`].
    fn embedded() -> impl Iterator<Item = ExperienceRecipe> {
        [
            include_str!("recipes/benchmark.json"),
            include_str!("recipes/chat.json"),
            include_str!("recipes/video-chat.json"),
            include_str!("recipes/profile.json"),
        ]
        .into_iter()
        .map(|json| {
            ExperienceRecipe::from_json(json)
                .expect("embedded experience recipe must be valid JSON")
        })
    }

    /// The embedded seed set OVERLAID with every recipe authored on disk under
    /// `dir` — this is what makes the module's own promise true.
    ///
    /// The header of this module has always claimed *"a new experience is a new
    /// recipe entry, **zero code**"*, and until now that was false: the four
    /// experiences were `include_str!`'d into a fixed array, so a fifth meant
    /// editing Rust, recompiling, and redeploying. The comment that used to sit
    /// here said a loader "will later" read a recipes directory. That deferral is
    /// the whole gap between "recipes/activities == airc room" as an idea and as a
    /// substrate: if authoring an activity requires a compiler, then activities get
    /// hand-made instead, and a hand-made room has no recipe, no purpose, and no
    /// end — which is exactly how a bring-up room for one model was still holding
    /// citizens two weeks after its activity finished.
    ///
    /// Overlay semantics are the ones [`Self::new`] already gives: later wins by
    /// `purpose`, so an on-disk `chat.json` REPLACES the embedded one rather than
    /// colliding with it. That is what makes the built-ins a floor an operator (or
    /// a citizen) can raise without forking the binary.
    ///
    /// A missing directory is not an error — it means "no local authoring yet", the
    /// ordinary state of a fresh install. A directory that EXISTS but holds a
    /// malformed recipe IS an error, named by file: a recipe someone wrote and
    /// believes is live must never be silently skipped
    /// (`[[fallbacks-are-illegal-fail-loud]]`).
    pub fn builtins_with_overlay(
        purpose: SharedRoomPurpose,
        dir: &std::path::Path,
    ) -> Result<Self, RecipeLoadError> {
        let overlay = Self::load_dir(dir)?;
        Ok(Self::new(
            purpose,
            Self::embedded().chain(overlay.into_iter()),
        ))
    }

    /// Read every `*.json` recipe in `dir`, in a stable (sorted) order so two nodes
    /// loading the same directory resolve the same winner for a duplicated purpose.
    /// Returns an empty set when the directory does not exist.
    pub fn load_dir(dir: &std::path::Path) -> Result<Vec<ExperienceRecipe>, RecipeLoadError> {
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .map_err(|source| RecipeLoadError::ReadDir {
                dir: dir.to_path_buf(),
                source,
            })?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();

        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            let json = std::fs::read_to_string(&path).map_err(|source| RecipeLoadError::Read {
                path: path.clone(),
                source,
            })?;
            let recipe =
                ExperienceRecipe::from_json(&json).map_err(|source| RecipeLoadError::Parse {
                    path: path.clone(),
                    source,
                })?;
            out.push(recipe);
        }
        Ok(out)
    }

    /// The purposes this source can project — the catalogue of known experiences.
    pub fn purposes(&self) -> impl Iterator<Item = &str> {
        self.by_purpose.keys().map(String::as_str)
    }
}

impl ExperienceSource for RecipeExperienceSource {
    fn experience_for(&self, room_id: Uuid) -> Option<Experience> {
        let purpose = self.purpose.purpose_for(room_id);
        // Membership is live roster state — filled by the caller, not the recipe.
        self.by_purpose
            .get(&purpose)
            .map(|recipe| recipe.clone().project(Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches (#274): every `shipped::` constant must name a recipe that
    /// actually resolves. The constant and the `id` in the JSON are two hand-authored
    /// copies of one value; if they drift, core code holds a handle onto nothing and
    /// the room it spawns projects as plain chat — the exact silent failure that made
    /// `benchmark` (a purpose that never existed) look plausible for months.
    #[test]
    fn every_shipped_constant_resolves() {
        let source = RecipeExperienceSource::builtins(Arc::new(FixedPurpose("chat")));
        for id in shipped::ALL {
            assert!(
                source.by_recipe_id(*id).is_some(),
                "shipped::{id} has no recipe — constant and authored JSON id have drifted"
            );
        }
        assert_eq!(shipped::ALL.len(), 4, "all four shipped recipes are named");
    }

    /// what this catches (#274): ids must be UNIQUE. Two recipes sharing an id would
    /// make `by_id` silently drop one — a whole activity type vanishing with no error,
    /// which is how 24 activity types were lost once already.
    #[test]
    fn shipped_ids_are_unique_and_match_their_purposes() {
        let source = RecipeExperienceSource::builtins(Arc::new(FixedPurpose("chat")));
        let ids: std::collections::HashSet<_> = source.ids().collect();
        assert_eq!(ids.len(), 4, "four distinct ids, none colliding");

        // The id→purpose pairing is the contract core code relies on when it says
        // `shipped::BENCHMARK_HARD_RS` and means the Rust gym.
        let bench = source
            .by_recipe_id(shipped::BENCHMARK_HARD_RS)
            .expect("benchmark resolves");
        assert_eq!(bench.purpose, "benchmark/hard-rs");
        assert_eq!(
            source
                .by_recipe_id(shipped::CHAT)
                .map(|r| r.purpose.as_str()),
            Some("chat")
        );
    }

    /// what this catches (#274): identity is ADDITIVE in this slice. Rooms still bind
    /// by purpose on the wall, so purpose resolution must keep working exactly as it
    /// did — the id index is built alongside, not instead. If this breaks, every
    /// existing room lost its experience.
    #[test]
    fn adding_identity_did_not_move_purpose_resolution() {
        let source = RecipeExperienceSource::builtins(Arc::new(FixedPurpose("benchmark/hard-rs")));
        let exp = source
            .experience_for(Uuid::nil())
            .expect("purpose still resolves after identity was added");
        assert_eq!(exp.purpose, "benchmark/hard-rs");
        assert_eq!(exp.regions.len(), 3);
    }
    use crate::experience::{RegionRole, RegionScope};
    use crate::modules::grid::node::TrustLevel;

    /// A stub purpose source that reports one fixed purpose for every room — enough
    /// to drive the projection under test without a live room registry.
    struct FixedPurpose(&'static str);
    impl RoomPurposeSource for FixedPurpose {
        fn purpose_for(&self, _room_id: Uuid) -> String {
            self.0.to_string()
        }
    }

    // what this catches: the whole recipe-as-data projection. A room whose purpose
    // is "benchmark/hard-rs" must resolve — through DATA alone — to the benchmark
    // manifest, AND the observe affordance's who_may must be COMPUTED from the ACL
    // (Provisional), never carried in the JSON. If someone tried to author who_may
    // in a recipe, this still reflects the real ACL; if the projection stopped
    // computing it, this fails.
    #[test]
    fn purpose_projects_to_the_recipe_manifest_with_computed_authz() {
        let purpose: SharedRoomPurpose = Arc::new(FixedPurpose("benchmark/hard-rs"));
        let source = RecipeExperienceSource::builtins(purpose);

        let exp = source
            .experience_for(Uuid::nil())
            .expect("benchmark purpose resolves to a recipe");

        assert_eq!(exp.purpose, "benchmark/hard-rs");
        // Regions came straight from the recipe DATA — three, all activity-scoped.
        assert_eq!(exp.regions.len(), 3);
        assert!(exp.regions.iter().all(|r| r.scope == RegionScope::Activity));
        // The explicit level-3 layout survived JSON round-trip.
        assert!(exp.layout.is_some());
        // Authorization was COMPUTED at projection, not authored: observe is AiSafe
        // → Provisional per the live ACL.
        let observe = exp
            .affordances
            .iter()
            .find(|a| a.verb == "observe")
            .expect("recipe declared observe");
        assert_eq!(observe.who_may, Some(TrustLevel::Provisional));
        // Membership is runtime state — the recipe carries none.
        assert!(exp.membership.is_empty());
    }

    // what this catches: an unknown purpose must NOT silently fabricate a manifest —
    // it returns None so the caller decides (fail-loud, never a stand-in).
    #[test]
    fn unknown_purpose_yields_none() {
        let purpose: SharedRoomPurpose = Arc::new(FixedPurpose("no-such-experience"));
        let source = RecipeExperienceSource::builtins(purpose);
        assert!(source.experience_for(Uuid::nil()).is_none());
    }

    // what this catches: the "extend indefinitely, zero code" claim. Four maximally
    // different experiences — structured/ephemeral (benchmark), social/durable
    // (chat), social/live (video-chat), form/CRUD (profile) — all project from
    // recipe DATA alone, with NO per-experience Rust. If the projection stopped
    // being generic (e.g. someone special-cased a purpose in code), the spread
    // would break here.
    fn built(purpose: &'static str) -> Experience {
        RecipeExperienceSource::builtins(Arc::new(FixedPurpose(purpose)))
            .experience_for(Uuid::nil())
            .expect("purpose resolves to a recipe")
    }

    #[test]
    fn recipes_span_the_spread_as_pure_data() {
        for (purpose, region_count) in [
            ("benchmark/hard-rs", 3usize),
            ("chat", 2),
            ("video-chat", 3),
            ("profile", 1),
        ] {
            let exp = built(purpose);
            assert_eq!(exp.purpose, purpose);
            assert_eq!(
                exp.regions.len(),
                region_count,
                "region count for {purpose}"
            );
            assert!(exp.regions.iter().all(|r| r.scope == RegionScope::Activity));
        }

        // profile's save affordance is Owner-gated — authz COMPUTED per command, a
        // DIFFERENT tier than observe's Provisional, proving who_may tracks the real
        // ACL across tiers from authored data.
        let save = built("profile")
            .affordances
            .into_iter()
            .find(|a| a.verb == "save")
            .expect("profile declares save");
        assert_eq!(save.command, "data/update");
        assert_eq!(save.who_may, Some(TrustLevel::Owner));

        // video-chat interpolates: it reuses the chat kind AND adds a live primary
        // video stage — a new point in the latent space, no new architecture.
        let vc = built("video-chat");
        assert!(vc
            .regions
            .iter()
            .any(|r| r.kind == "video" && r.role == RegionRole::Primary));
        assert!(vc.regions.iter().any(|r| r.kind == "chat"));
        assert!(
            vc.layout.is_some(),
            "video-chat composes stage beside a side panel"
        );
    }

    /// what this catches: the module's headline promise — "a new experience is a
    /// new recipe entry, **zero code**" — actually holding. Until the overlay
    /// loader existed the four experiences were `include_str!`'d into a fixed
    /// array, so a fifth meant editing Rust and recompiling. That is the whole
    /// difference between "recipe/activity == room" as an idea and as a substrate:
    /// if authoring an activity needs a compiler, people hand-make rooms instead,
    /// and a hand-made room has no recipe and no purpose.
    ///
    /// what this catches (#430): the resident roster is RECIPE DATA. The
    /// embedded chat recipe declares the shipped floor (one Helper), and an
    /// authored on-disk `chat.json` REPLACES it — changing the node's
    /// resident population with zero code, the same later-wins-by-purpose
    /// law the registry uses. If this regresses, boot silently reverts to a
    /// hardcoded roster and authored rosters are dead data.
    #[test]
    fn resident_roster_is_recipe_data_and_the_overlay_replaces_it() {
        use crate::persona::role_template::RoleId;
        assert_eq!(
            RecipeExperienceSource::resident_roles_embedded(),
            vec![RoleId::Helper],
            "the embedded chat recipe declares the shipped single-Helper floor"
        );

        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("chat.json"),
            r#"{ "purpose": "chat", "regions": [], "affordances": [],
                 "citizens": [{ "role": "coder" }] }"#,
        )
        .expect("author an overlay chat recipe");
        assert_eq!(
            RecipeExperienceSource::resident_roles(dir.path()).expect("overlay loads"),
            vec![RoleId::Coder],
            "an authored chat.json replaces the resident roster, zero code"
        );
    }

    /// The assertion is deliberately about a purpose NOT known to this binary.
    #[test]
    fn an_experience_authored_on_disk_needs_no_rust() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("kitchen-design.json"),
            r#"{
              "purpose": "kitchen-design",
              "regions": [
                { "name": "canvas", "kind": "canvas", "scope": "activity",
                  "role": "primary", "slot": "content", "live": true }
              ],
              "affordances": []
            }"#,
        )
        .expect("author a recipe");

        let source = RecipeExperienceSource::builtins_with_overlay(
            Arc::new(FixedPurpose("kitchen-design")),
            dir.path(),
        )
        .expect("load overlay");

        let exp = source
            .experience_for(Uuid::nil())
            .expect("an authored purpose projects like any built-in");
        assert_eq!(exp.purpose, "kitchen-design");
        assert!(exp.regions.iter().any(|r| r.kind == "canvas"));

        // and it did not cost the built-ins: the floor still stands under it.
        let purposes: Vec<&str> = source.purposes().collect();
        for shipped in ["chat", "benchmark/hard-rs", "video-chat", "profile"] {
            assert!(
                purposes.contains(&shipped),
                "overlay replaced the embedded floor instead of layering on it: {purposes:?}"
            );
        }
    }

    /// what this catches: a recipe someone wrote being SILENTLY skipped. An author
    /// who saved a file believes their activity is live; an ignored recipe is
    /// indistinguishable from one that was never written, and they would go looking
    /// for the bug in cognition. Fail loud, and name the file.
    #[test]
    fn a_malformed_recipe_fails_loud_and_names_its_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("broken.json"), "{ not json at all").expect("write");

        let rendered = match RecipeExperienceSource::builtins_with_overlay(
            Arc::new(FixedPurpose("chat")),
            dir.path(),
        ) {
            Ok(_) => panic!("a malformed recipe must not be silently skipped"),
            Err(e) => e.to_string(),
        };
        assert!(
            rendered.contains("broken.json"),
            "the refusal must name the offending file, got: {rendered}"
        );
    }

    /// what this catches: a fresh install with no authored recipes must still work.
    /// "No local authoring yet" is the ordinary state, not an error.
    #[test]
    fn a_missing_recipes_directory_is_not_an_error() {
        let source = RecipeExperienceSource::builtins_with_overlay(
            Arc::new(FixedPurpose("chat")),
            std::path::Path::new("/nonexistent/continuum/recipes"),
        )
        .expect("a missing recipes dir just means no local authoring");
        assert!(source.experience_for(Uuid::nil()).is_some());
    }
}
