//! `PersonaHome` — the per-citizen filesystem scope.
//!
//! Foundation slice for [[entity-chain-of-custody-vision]] (Joel
//! 2026-06-03). Every citizen (persona / human / external AI) has a
//! home dir on disk where their identity (airc keypair), their entity
//! stores (SQLite per collection), and their seed / config files
//! live. This module is the typed surface for resolving those paths
//! — every other module that needs "where do this persona's engrams
//! live" reaches through here.
//!
//! ### Convention
//!
//! ```
//! <continuum_root>/personas/<agent_name>/
//!     airc/              ← airc keypair + state (managed by airc-lib)
//!     seed.json          ← PersonaIdentityProvider's seed
//!     engrams.sqlite     ← OrmStore<Engram> + OrmStore<EngramRecallMetadata>
//!     <future>           ← signing-key derivation, Merkle chain head
//!                          cache, future per-collection databases
//! ```
//!
//! The `airc/` subdir is owned by airc-lib's `Airc::attach_as` path
//! (see `persona/airc_runtime.rs`). The rest is owned by continuum-
//! core. Both share the same `<agent_name>` root so the cryptographic
//! identity, the cognition state, and (next slice) the entity chains
//! all hang off the same directory tree.
//!
//! ### Why a typed home rather than passing paths around
//!
//! Per [[organization-purity-as-we-migrate]]: one logical concept,
//! one place. "Where this persona's stuff lives" is a logical
//! concept; passing raw `PathBuf`s and joining `"engrams.sqlite"` at
//! every callsite invites typos + drift. `PersonaHome::engrams_db()`
//! is the typed answer; the test below pins it.

use std::path::{Path, PathBuf};

/// What KIND of citizen a home belongs to — the subdir under
/// `<continuum_root>/` where their scope lives. First-class citizenship
/// for ALL (Joel 2026-07-25): a coding agent (Claude Code / Codex) and a
/// human get the SAME home shape a persona has — identity in `airc/`,
/// engrams in `engrams.sqlite` — differing only in the top-level bucket,
/// so an agent's OWN durable memory (the `/continuum:memory` skill's
/// store) lives in its own dir and survives session death exactly like a
/// persona's. The engram write-through + recall paths are kind-agnostic;
/// they reach through the typed home either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitizenKind {
    /// An internal persona (RAG + optional genome) — `personas/<name>/`.
    Persona,
    /// An external coding agent (Claude Code, Codex, …) — `agents/<name>/`.
    Agent,
    /// A human citizen (their notes/context engrams) — `humans/<name>/`.
    Human,
}

impl CitizenKind {
    /// The top-level bucket dir for this kind under the continuum root.
    pub fn bucket(self) -> &'static str {
        match self {
            CitizenKind::Persona => "personas",
            CitizenKind::Agent => "agents",
            CitizenKind::Human => "humans",
        }
    }
}

/// The on-disk scope for a single citizen — persona, coding agent, or
/// human. Every kind gets the SAME layout (identity in `airc/`, engrams
/// in `engrams.sqlite`); only the bucket differs, so the engram
/// substrate is one mechanism for every citizen ([[first-class-citizenship]]).
///
/// Construct via [`for_citizen`](Self::for_citizen) (any kind) or the
/// [`for_persona`](Self::for_persona) convenience wrapper. The home
/// directory is created lazily — callers that need it call
/// [`ensure_exists`](Self::ensure_exists) first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaHome {
    root: PathBuf,
}

impl PersonaHome {
    /// Resolve the home for any citizen KIND under
    /// `<continuum_root>/<bucket>/<name>/`. The one constructor; the
    /// persona/agent/human split is just the bucket.
    pub fn for_citizen(continuum_root: &Path, kind: CitizenKind, name: &str) -> Self {
        let root = continuum_root.join(kind.bucket()).join(name);
        Self { root }
    }

    /// Resolve the home for a given persona under
    /// `<continuum_root>/personas/<agent_name>/`. A thin wrapper over
    /// [`for_citizen`](Self::for_citizen) — kept so existing persona
    /// callsites don't churn. Does NOT create the directory.
    pub fn for_persona(continuum_root: &Path, agent_name: &str) -> Self {
        Self::for_citizen(continuum_root, CitizenKind::Persona, agent_name)
    }

    /// Resolve a coding AGENT's home (`agents/<name>/`) — the durable
    /// scope for a Claude Code / Codex session's own engram memory.
    pub fn for_agent(continuum_root: &Path, agent_name: &str) -> Self {
        Self::for_citizen(continuum_root, CitizenKind::Agent, agent_name)
    }

    /// Resolve a HUMAN's home (`humans/<name>/`) — their own engram
    /// scope for notes/context that persists across sessions.
    pub fn for_human(continuum_root: &Path, name: &str) -> Self {
        Self::for_citizen(continuum_root, CitizenKind::Human, name)
    }

    /// Wrap an ALREADY-resolved persona home directory (e.g. the
    /// `identity.home` the spawn path already computed as
    /// `<root>/personas/<name>`). Use this when the caller holds the resolved
    /// home path directly and must NOT re-join `personas/<name>` (which
    /// `for_persona` does).
    pub fn from_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// The root directory for this persona. Used by callers that
    /// need to compose their own sub-paths (e.g. the airc-lib
    /// `attach_as` call that owns `airc/`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ensure the home directory exists. Idempotent.
    pub fn ensure_exists(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)
    }

    /// Path to the engram + recall-metadata SQLite database. One file
    /// per persona — the OrmStore for Engram and EngramRecallMetadata
    /// share it (their schemas live as separate tables with a real
    /// FK between them, per the relational ORM design).
    ///
    /// Path: `<home>/engrams.sqlite`.
    pub fn engrams_db(&self) -> PathBuf {
        self.root.join("engrams.sqlite")
    }

    /// Path to the airc keypair + state subdir, owned by airc-lib.
    /// Path: `<home>/airc/`.
    pub fn airc_dir(&self) -> PathBuf {
        self.root.join("airc")
    }

    /// Path to the persona's `seed.json` file, owned by
    /// `PersonaIdentityProvider` per slice 4 of the persona-
    /// persistence work.
    pub fn seed_json(&self) -> PathBuf {
        self.root.join("seed.json")
    }

    /// Path to the persona's runtime base-model override, owned by
    /// [`PersonaModelOverride`](crate::persona::model_override::PersonaModelOverride).
    /// Present ⇒ this persona is force-assigned to a specific base
    /// model (operator or self via `persona/reassign-model`), taking
    /// precedence over the catalog's tiered `model_preferences`.
    /// Absent ⇒ the allocator resolves her model from the catalog as
    /// usual. Lives under the home root so it rides the
    /// [`PersonaHomeBundle`](crate::persona::portability::PersonaHomeBundle)
    /// for free — move the home, keep the self (and her assignment).
    ///
    /// Path: `<home>/model_override.json`.
    pub fn model_override_json(&self) -> PathBuf {
        self.root.join("model_override.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: the home root composes as expected from
    /// `<continuum_root>/personas/<agent_name>`. If this drifts, the
    /// airc keypair + the engram DB + the seed file end up in
    /// inconsistent locations and the citizen's identity + state
    /// become disconnected.
    #[test]
    fn home_resolves_under_personas_subdir() {
        let root = Path::new("/tmp/continuum-test-root");
        let home = PersonaHome::for_persona(root, "Paige");
        assert_eq!(
            home.root(),
            Path::new("/tmp/continuum-test-root/personas/Paige")
        );
    }

    /// What this catches: first-class citizenship (Joel 2026-07-25) — an
    /// AGENT and a HUMAN get the SAME home layout as a persona, differing
    /// only in the top-level bucket. If the bucket mapping drifts, an
    /// agent's durable engram memory lands in the wrong place (or collides
    /// with a persona of the same name), breaking the amnesia fix the
    /// per-agent home exists to deliver.
    #[test]
    fn every_citizen_kind_gets_the_same_home_shape() {
        let root = Path::new("/tmp/continuum-test-root");
        let agent = PersonaHome::for_agent(root, "claude-code");
        let human = PersonaHome::for_human(root, "operator");
        assert_eq!(
            agent.engrams_db(),
            Path::new("/tmp/continuum-test-root/agents/claude-code/engrams.sqlite")
        );
        assert_eq!(
            human.engrams_db(),
            Path::new("/tmp/continuum-test-root/humans/operator/engrams.sqlite")
        );
        // Same layout invariant every kind: airc identity beside engrams.
        assert_eq!(
            agent.airc_dir(),
            Path::new("/tmp/continuum-test-root/agents/claude-code/airc")
        );
        // The persona wrapper and for_citizen(Persona) resolve identically.
        assert_eq!(
            PersonaHome::for_persona(root, "Asha"),
            PersonaHome::for_citizen(root, CitizenKind::Persona, "Asha")
        );
    }

    /// What this catches: sub-path accessors compose correctly off
    /// the same root. Tested together because their relative layout
    /// is the invariant — drift between airc_dir and seed_json means
    /// the persona's identity + their cognition state stop sharing
    /// a parent directory, and migration / backup / forensic tooling
    /// breaks.
    #[test]
    fn sub_path_accessors_compose_off_root() {
        let root = Path::new("/tmp/continuum-test-root");
        let home = PersonaHome::for_persona(root, "Niko");

        assert_eq!(
            home.engrams_db(),
            Path::new("/tmp/continuum-test-root/personas/Niko/engrams.sqlite")
        );
        assert_eq!(
            home.airc_dir(),
            Path::new("/tmp/continuum-test-root/personas/Niko/airc")
        );
        assert_eq!(
            home.seed_json(),
            Path::new("/tmp/continuum-test-root/personas/Niko/seed.json")
        );
        assert_eq!(
            home.model_override_json(),
            Path::new("/tmp/continuum-test-root/personas/Niko/model_override.json")
        );
    }

    /// What this catches: ensure_exists creates the directory and
    /// is idempotent. The "create_dir_all" semantics matter — if it
    /// fails on existing dirs the bootstrap path breaks; if it fails
    /// on missing intermediate dirs the first-boot path breaks.
    #[test]
    fn ensure_exists_creates_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let home = PersonaHome::for_persona(tmp.path(), "Camille");
        assert!(!home.root().exists(), "fresh tempdir doesn't have it yet");
        home.ensure_exists().expect("first ensure_exists succeeds");
        assert!(home.root().exists(), "directory now exists");
        home.ensure_exists()
            .expect("second ensure_exists is a no-op");
        assert!(home.root().exists(), "still exists after idempotent call");
    }

    /// What this catches: two personas with different names get
    /// completely separate homes. The chain-of-custody design depends
    /// on per-citizen isolation; this is the first defense.
    #[test]
    fn different_personas_have_disjoint_homes() {
        let root = Path::new("/tmp/continuum-test-root");
        let paige = PersonaHome::for_persona(root, "Paige");
        let niko = PersonaHome::for_persona(root, "Niko");
        assert_ne!(paige.root(), niko.root());
        assert_ne!(paige.engrams_db(), niko.engrams_db());
    }
}
