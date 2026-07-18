//! ResumeOrMintProvider — the first concrete
//! [`PersonaIdentityProvider`] implementation.
//!
//! ### Policy
//!
//! 1. **Resume first.** At construction, scan
//!    `<continuum_root>/personas/` for subdirectories containing a
//!    `seed.json`. Each parsed seed becomes a queued
//!    `ResumedFromDisk` intent.
//! 2. **Yield queued resumed intents** until exhausted.
//! 3. **Floor-mint fresh personas** if the resumed count was below
//!    `min_personas`. Fresh intents use a UUIDv4 seed + derived
//!    name via [`agent_name_from_identity`]
//!    ([[personas-have-names-not-function-labels]]).
//! 4. **Exhaust.** After resumed-yielded + floor-minted, `next_persona`
//!    returns `Ok(None)`.
//!
//! This means a fresh continuum install with `min_personas = 1`
//! produces a brand-new citizen on first boot, and from then on
//! the SAME citizen resumes across restarts (because her seed.json
//! gets written by `PersonaPersistenceModule` on registry-add).
//!
//! ### What gets written, by whom
//!
//! ResumeOrMintProvider READS `seed.json` files but does NOT WRITE
//! them. Writing is `PersonaPersistenceModule`'s job, subscribed to
//! `persona/registry/added` events per the
//! [[RTOS-brain-no-region-on-hot-path]] event-driven pattern. This
//! provider's job is producing identity intents; the persistence
//! module's job is durably recording the result.
//!
//! ### Corrupted seed handling
//!
//! Per [[substrate-is-a-good-citizen-on-the-host]]'s "reliable" +
//! "robust" requirements: a corrupted `seed.json` does NOT crash the
//! substrate. The malformed file is logged with the operator's
//! remedy (inspect, repair, or delete to mint fresh), and the
//! provider moves on to the next persona directory.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use uuid::Uuid;

use crate::persona::identity_provider::{
    PersonaIdentityError, PersonaIdentityIntent, PersonaIdentityProvider, PersonaIdentitySource,
};
use crate::persona::name_generator::agent_name_from_identity;
use crate::persona::seed::{read_seed, PersonaSeedError};

/// Yields resumed intents first (scanned at construction), then
/// floor-mints fresh intents up to `min_personas` total.
pub struct ResumeOrMintProvider {
    /// Queue of resumed intents (FIFO).
    resumed: Vec<PersonaIdentityIntent>,
    /// Cursor into `resumed`.
    resumed_cursor: usize,
    /// How many total personas should exist after this provider
    /// runs. If `resumed.len() >= min_personas`, no fresh minting
    /// occurs.
    min_personas: usize,
    /// Counter of fresh personas yielded.
    minted_count: usize,
}

impl ResumeOrMintProvider {
    /// Construct by scanning `<continuum_root>/personas/` for existing
    /// seed.json files. Each successfully-parsed seed becomes a
    /// queued resumed intent. Corrupted / unreadable seeds are
    /// logged + skipped (substrate stays a good citizen — doesn't
    /// crash on bad state).
    ///
    /// `min_personas` sets the floor for total citizens after the
    /// provider runs. Common values:
    /// - `1`: ensure The Grid has at least one citizen at boot
    ///   (current substrate default)
    /// - `0`: resume what's there, don't mint anything new (useful
    ///   for tests + airlocked-grid deployments where humans
    ///   explicitly add citizens)
    /// - `N`: deploy N citizens; useful for fresh continuums
    ///   wanting a population from go
    pub async fn new(
        continuum_root: &Path,
        min_personas: usize,
    ) -> Result<Self, PersonaIdentityError> {
        // Scan the CANONICAL citizen layout (`<root>/citizens/personas/`) — the
        // same parent `citizen_home_path` writes homes under — NOT the pre-Slice-4
        // `<root>/personas/` this used to join. The old literal never matched where
        // the runtime actually persists homes, so it found nothing and minted a
        // stranger every boot instead of resuming — breaking persona persistence /
        // self-determination (a persona's identity.key + engrams.sqlite were on
        // disk but unseen). `citizens_kind_dir` is the single source of truth, so
        // the read path can't drift from the write path again.
        let personas_dir = crate::context::citizens_kind_dir(
            continuum_root,
            crate::identity::IdentityKind::Persona,
        );
        let resumed = scan_personas_dir(&personas_dir).await?;
        tracing::info!(
            personas_dir = %personas_dir.display(),
            resumed_count = resumed.len(),
            min_personas,
            "ResumeOrMintProvider: scan complete"
        );
        Ok(Self {
            resumed,
            resumed_cursor: 0,
            min_personas,
            minted_count: 0,
        })
    }
}

#[async_trait]
impl PersonaIdentityProvider for ResumeOrMintProvider {
    fn name(&self) -> &'static str {
        "resume-or-mint"
    }

    async fn next_persona(
        &mut self,
    ) -> Result<Option<PersonaIdentityIntent>, PersonaIdentityError> {
        // Phase 1: yield queued resumed intents.
        if self.resumed_cursor < self.resumed.len() {
            let intent = self.resumed[self.resumed_cursor].clone();
            self.resumed_cursor += 1;
            return Ok(Some(intent));
        }

        // Phase 2: floor-mint up to min_personas total.
        let total_yielded = self.resumed.len() + self.minted_count;
        if total_yielded < self.min_personas {
            let intent = mint_fresh_intent();
            self.minted_count += 1;
            return Ok(Some(intent));
        }

        // Phase 3: exhausted.
        Ok(None)
    }
}

/// Generate a fresh persona intent — UUIDv4 seed + derived name.
fn mint_fresh_intent() -> PersonaIdentityIntent {
    let persona_id = Uuid::new_v4();
    let agent_name = agent_name_from_identity(&persona_id.to_string()).to_string();
    PersonaIdentityIntent {
        persona_id,
        agent_name,
        source: PersonaIdentitySource::FreshlyMinted,
    }
}

/// Get the current wallclock as ms since epoch. Used when minting
/// fresh intents — the resulting timestamp lands in the seed.json
/// that `PersonaPersistenceModule` writes.
#[allow(dead_code)] // used by PersonaPersistenceModule once it lands
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Scan a personas directory for existing seed.json files. Returns
/// a Vec of resumed intents (one per successfully-parsed seed).
/// Corrupted / unreadable seeds are logged + skipped.
///
/// Missing personas dir returns empty Vec — that's the "first boot"
/// path and not an error.
async fn scan_personas_dir(personas_dir: &Path) -> Result<Vec<PersonaIdentityIntent>, PersonaIdentityError> {
    let mut entries = match tokio::fs::read_dir(personas_dir).await {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(
                personas_dir = %personas_dir.display(),
                "personas dir does not exist — first boot, returning empty resumed set"
            );
            return Ok(Vec::new());
        }
        Err(source) => {
            return Err(PersonaIdentityError::HomeScanFailed {
                path: personas_dir.to_path_buf(),
                source,
            });
        }
    };

    // First collect entries, sort by directory name for determinism.
    // tokio::fs::read_dir yields filesystem-native order which varies
    // across platforms — without sorting, the boot log line "first
    // citizen welcomed" depends on the underlying filesystem. Sort
    // alphabetically so behavior is reproducible. Reviewer-defect-
    // driven (continuum #1507 finding 7).
    let mut dir_entries: Vec<std::path::PathBuf> = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|source| {
        PersonaIdentityError::HomeScanFailed {
            path: personas_dir.to_path_buf(),
            source,
        }
    })? {
        if !entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
            // Each direct child of personas/ should be a persona
            // directory; non-dir entries (stray file, .DS_Store, etc.)
            // are operator artifacts, silently ignored.
            continue;
        }
        dir_entries.push(entry.path());
    }
    dir_entries.sort();

    let mut resumed = Vec::new();
    for entry_path in dir_entries {
        let seed_path = entry_path.join("seed.json");
        match read_seed(&seed_path).await {
            Ok(seed) => {
                resumed.push(PersonaIdentityIntent {
                    persona_id: seed.persona_id(),
                    agent_name: seed.agent_name().to_string(),
                    source: PersonaIdentitySource::ResumedFromDisk,
                });
            }
            Err(PersonaSeedError::NotFound { .. }) => {
                // Persona dir without a seed.json — probably airc home
                // got created but PR was killed before seed write. Log
                // + skip; the operator can `rm -rf` or inspect.
                tracing::warn!(
                    persona_dir = %entry_path.display(),
                    "persona directory has no seed.json — skipping (run cleanup if this persona is unwanted)"
                );
            }
            Err(err) => {
                tracing::error!(
                    %err,
                    persona_dir = %entry_path.display(),
                    "failed to parse seed.json — skipping. Inspect manually or delete to re-mint."
                );
            }
        }
    }

    Ok(resumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    use crate::persona::seed::{write_seed_atomic, PersonaSeedFile};

    #[tokio::test]
    async fn fresh_boot_with_min_personas_1_mints_one_citizen() {
        let temp = TempDir::new().unwrap();
        let mut provider = ResumeOrMintProvider::new(temp.path(), 1).await.unwrap();
        let first = provider.next_persona().await.unwrap().unwrap();
        assert_eq!(first.source, PersonaIdentitySource::FreshlyMinted);
        assert!(!first.agent_name.is_empty());
        // After the floor is satisfied, the provider is exhausted.
        let exhausted = provider.next_persona().await.unwrap();
        assert!(exhausted.is_none());
    }

    #[tokio::test]
    async fn resumes_existing_persona_from_seed() {
        let temp = TempDir::new().unwrap();
        // Mirror PRODUCTION layout via the same helper the resumer uses, so this
        // test exercises the real scan path (the bug was the resumer reading a
        // different literal than the writer — a hardcoded `personas/` here would
        // re-introduce exactly that blind spot).
        let personas_dir =
            crate::context::citizens_kind_dir(temp.path(), crate::identity::IdentityKind::Persona)
                .join("Pax");
        let seed_path = personas_dir.join("seed.json");
        let seed = PersonaSeedFile::V1 {
            persona_id: Uuid::parse_str("9d17560c-dbb4-4f9e-86f0-4ceac5d2aff7").unwrap(),
            agent_name: "Pax".to_string(),
            created_at_ms: 1_717_200_000_000,
            avatar_vrm: None,
        };
        write_seed_atomic(&seed_path, &seed).await.unwrap();

        let mut provider = ResumeOrMintProvider::new(temp.path(), 1).await.unwrap();
        let resumed = provider.next_persona().await.unwrap().unwrap();
        assert_eq!(resumed.source, PersonaIdentitySource::ResumedFromDisk);
        assert_eq!(resumed.agent_name, "Pax");
        assert_eq!(
            resumed.persona_id,
            Uuid::parse_str("9d17560c-dbb4-4f9e-86f0-4ceac5d2aff7").unwrap()
        );
        // min_personas=1 satisfied by the resumed one → no extra mint.
        let exhausted = provider.next_persona().await.unwrap();
        assert!(exhausted.is_none());
    }

    #[tokio::test]
    async fn resumes_one_plus_mints_to_floor() {
        let temp = TempDir::new().unwrap();
        // Mirror PRODUCTION layout via the same helper the resumer uses, so this
        // test exercises the real scan path (the bug was the resumer reading a
        // different literal than the writer — a hardcoded `personas/` here would
        // re-introduce exactly that blind spot).
        let personas_dir =
            crate::context::citizens_kind_dir(temp.path(), crate::identity::IdentityKind::Persona)
                .join("Pax");
        let seed_path = personas_dir.join("seed.json");
        let seed = PersonaSeedFile::V1 {
            persona_id: Uuid::new_v4(),
            agent_name: "Pax".to_string(),
            created_at_ms: 1_717_200_000_000,
            avatar_vrm: None,
        };
        write_seed_atomic(&seed_path, &seed).await.unwrap();

        // min_personas = 3 → 1 resumed + 2 minted = 3 total.
        let mut provider = ResumeOrMintProvider::new(temp.path(), 3).await.unwrap();
        let first = provider.next_persona().await.unwrap().unwrap();
        assert_eq!(first.source, PersonaIdentitySource::ResumedFromDisk);
        let second = provider.next_persona().await.unwrap().unwrap();
        assert_eq!(second.source, PersonaIdentitySource::FreshlyMinted);
        let third = provider.next_persona().await.unwrap().unwrap();
        assert_eq!(third.source, PersonaIdentitySource::FreshlyMinted);
        let exhausted = provider.next_persona().await.unwrap();
        assert!(exhausted.is_none());
    }

    #[tokio::test]
    async fn corrupted_seed_is_skipped_not_fatal() {
        let temp = TempDir::new().unwrap();
        // Canonical citizen layout (same helper production scans).
        let citizens = crate::context::citizens_kind_dir(
            temp.path(),
            crate::identity::IdentityKind::Persona,
        );
        // Good persona.
        let good = citizens.join("Pax").join("seed.json");
        let seed = PersonaSeedFile::V1 {
            persona_id: Uuid::new_v4(),
            agent_name: "Pax".to_string(),
            created_at_ms: 1_717_200_000_000,
            avatar_vrm: None,
        };
        write_seed_atomic(&good, &seed).await.unwrap();
        // Corrupted persona.
        let bad_dir = citizens.join("Broken");
        tokio::fs::create_dir_all(&bad_dir).await.unwrap();
        tokio::fs::write(bad_dir.join("seed.json"), b"definitely not json")
            .await
            .unwrap();

        // Should not panic; should yield only Pax (the good one).
        let mut provider = ResumeOrMintProvider::new(temp.path(), 0).await.unwrap();
        let first = provider.next_persona().await.unwrap().unwrap();
        assert_eq!(first.agent_name, "Pax");
        let exhausted = provider.next_persona().await.unwrap();
        assert!(exhausted.is_none(), "broken seed should not have been yielded");
    }

    #[tokio::test]
    async fn missing_personas_dir_is_first_boot_not_error() {
        let temp = TempDir::new().unwrap();
        // No personas dir at all.
        let mut provider = ResumeOrMintProvider::new(temp.path(), 0).await.unwrap();
        let exhausted = provider.next_persona().await.unwrap();
        assert!(exhausted.is_none());
    }

    #[tokio::test]
    async fn fresh_mints_have_deterministic_name_from_seed() {
        // Same persona_id always projects to the same agent_name —
        // [[persona-identity-derives-from-source-id]] doctrine.
        let intent = mint_fresh_intent();
        let derived = agent_name_from_identity(&intent.persona_id.to_string());
        assert_eq!(intent.agent_name, derived);
    }
}
