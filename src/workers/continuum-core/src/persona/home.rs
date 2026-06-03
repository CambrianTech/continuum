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

/// The on-disk scope for a single citizen (persona for now; humans +
/// external AIs follow per the chain-of-custody design doc).
///
/// Construct via `PersonaHome::for_persona(continuum_root, agent_name)`.
/// The home directory is created lazily on first sub-path access
/// (most callers want the dir to exist before they open a DB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaHome {
    root: PathBuf,
}

impl PersonaHome {
    /// Resolve the home for a given persona under
    /// `<continuum_root>/personas/<agent_name>/`. Does NOT create the
    /// directory — callers that need it to exist call
    /// `ensure_exists()` first.
    pub fn for_persona(continuum_root: &Path, agent_name: &str) -> Self {
        let root = continuum_root.join("personas").join(agent_name);
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
        home.ensure_exists().expect("second ensure_exists is a no-op");
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
