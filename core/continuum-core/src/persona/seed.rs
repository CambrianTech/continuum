//! Per-persona seed file — the continuum-side identity mapping.
//!
//! ### What this stores
//!
//! `seed.json` lives at `~/.continuum/personas/<agent_name>/seed.json`
//! alongside airc-lib's `airc/identity.key` (the Ed25519 keypair).
//! The two files together form the persona's durable identity layer:
//!
//! - **`identity.key`** — airc-lib's responsibility; the cryptographic
//!   keypair that anchors the persona on the substrate. Survives any
//!   change to her name/theme/bio. The persona's "who" at the
//!   cryptographic layer.
//! - **`seed.json`** — continuum's responsibility; the stable
//!   continuum-side `persona_id` (UUID) + her chosen `agent_name` +
//!   creation timestamp. The persona's "who" at the application layer.
//!
//! Per memory [[persona-identity-derives-from-source-id]]: both
//! derive from a single conceptual seed. The keypair derives the
//! cryptographic peer_id; the seed.json carries the
//! continuum-allocated persona_id that drives name + avatar + voice
//! + genome facet derivation via [[crate::persona::name_generator]].
//!
//! ### Atomic writes (crash-safe)
//!
//! Per the [[substrate-is-a-good-citizen-on-the-host]] doctrine, we
//! NEVER leave a half-written persona seed file on disk. The write
//! pattern is:
//!
//! 1. Serialize to JSON
//! 2. Write to `seed.json.tmp` (in the persona's airc home dir)
//! 3. fsync the temp file
//! 4. Rename to `seed.json` (atomic on POSIX)
//!
//! If the process crashes mid-write, the rename hasn't happened →
//! the persona's previous seed.json (or absence thereof) is
//! preserved. Either she's resumable from the prior state, or
//! she'll mint fresh next boot. No corruption-on-crash.
//!
//! ### Why JSON + serde, not bincode/CBOR
//!
//! The seed is small (~150 bytes), human-readable (operators can
//! inspect with `cat`), versionable (serde tag fields handle schema
//! evolution), and the parse cost is negligible. Performance is not
//! the constraint here; auditability is.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::live::avatar::types::AvatarGender;
use crate::persona::card::PersonaCard;
use crate::persona::role_template::RoleId;

/// The on-disk seed record. Schema-versioned so we can evolve
/// fields without breaking older installs.
///
/// v2 promotes the seed from a bare identity mapping to the persona's full,
/// durable, coherent [`PersonaCard`] — the "one identity, one card" record
/// ([[persona-is-the-airc-user-one-identity-one-card]]). A v1 row deserializes fine
/// (serde `tag = "version"`) and is UPGRADED to v2 on the next write
/// ([`ensure_seed`]), backfilling the card from the persona's current effective
/// values so nobody shifts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "version")]
pub enum PersonaSeedFile {
    /// v1 schema — persona_id + agent_name + created_at (+ pinned avatar).
    #[serde(rename = "1")]
    V1 {
        /// Stable continuum-side identifier. Drives name + avatar +
        /// voice + genome facet derivation. Must NOT change across
        /// restarts.
        persona_id: Uuid,
        /// Persona's airc agent_name (matches what airc peers / whois
        /// show). Derived from `persona_id` via
        /// `agent_name_from_identity` at first mint; stored here so
        /// resume doesn't have to recompute.
        agent_name: String,
        /// When this persona was first minted (ISO 8601, UTC, ms
        /// precision). Doesn't change on resume; only on initial
        /// mint.
        created_at_ms: u64,
        /// The persona's PINNED avatar VRM filename (a stable catalog key). Resolved
        /// ONCE at first spawn — when the roster is warm so gender is correct — and
        /// then NEVER re-derived. This is the sticky binding that stops the
        /// wrong-avatar-when-roster-cold thrash (#174,
        /// [[never-thrash-sticky-hysteresis-on-every-lane]]): her face becomes part
        /// of her durable self and travels across restarts + the grid. `None` on a
        /// pre-#174 seed (old JSON rows deserialize via serde default) until the next
        /// spawn pins it. Rehydrate the VRM path via `avatar_model_path(vrm)`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        avatar_vrm: Option<String>,
    },
    /// v2 schema — the full coherent [`PersonaCard`]. Adds the presentation spine
    /// (`gender`), the `voice_seed`, and the substrate `role` on top of v1. Pronouns
    /// are NOT stored — they derive from `gender` (compression; see
    /// [`PersonaCard::pronouns`]).
    #[serde(rename = "2")]
    V2 {
        persona_id: Uuid,
        agent_name: String,
        created_at_ms: u64,
        /// The presentation spine — avatar, voice, and pronouns all cohere with it.
        gender: AvatarGender,
        /// The pinned avatar VRM (sticky, resolve-once — #174). `None` until pinned.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        avatar_vrm: Option<String>,
        /// The seed the speak path picks a stable, gender-matched voice from (today
        /// the identity string; a field so voice can later be chosen independently).
        voice_seed: String,
        /// The substrate role, when known. `None` before role threading (later slice).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<RoleId>,
        /// The OPEN self-authored profile (bio/goals/desires/interests/blog/…). Empty
        /// by default; `#[serde(default)]` so a V2 row written before this field
        /// deserializes cleanly (empty), and an empty map is skipped on write.
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        profile: std::collections::BTreeMap<String, String>,
    },
}

impl PersonaSeedFile {
    /// Build a v2 seed from a persona's durable [`PersonaCard`].
    pub fn from_card(card: &PersonaCard) -> Self {
        Self::V2 {
            persona_id: card.persona_id,
            agent_name: card.agent_name.clone(),
            created_at_ms: card.created_at_ms,
            gender: card.gender,
            avatar_vrm: card.avatar_vrm.clone(),
            voice_seed: card.voice_seed.clone(),
            role: card.role,
            profile: card.profile.clone(),
        }
    }

    /// The persona's full coherent card. A v2 seed returns its stored card verbatim
    /// (the durable, editable truth). A v1 seed DERIVES the card via
    /// [`PersonaCard::genesis`] — which reproduces the exact effective gender the live
    /// seams computed pre-v2, so reading an un-upgraded v1 shifts nobody.
    pub fn card(&self) -> PersonaCard {
        match self {
            Self::V2 {
                persona_id,
                agent_name,
                created_at_ms,
                gender,
                avatar_vrm,
                voice_seed,
                role,
                profile,
            } => PersonaCard {
                persona_id: *persona_id,
                agent_name: agent_name.clone(),
                created_at_ms: *created_at_ms,
                gender: *gender,
                avatar_vrm: avatar_vrm.clone(),
                voice_seed: voice_seed.clone(),
                role: *role,
                profile: profile.clone(),
            },
            Self::V1 {
                persona_id,
                agent_name,
                created_at_ms,
                avatar_vrm,
            } => PersonaCard::genesis(
                *persona_id,
                agent_name.clone(),
                *created_at_ms,
                avatar_vrm.clone(),
            ),
        }
    }

    pub fn persona_id(&self) -> Uuid {
        match self {
            Self::V1 { persona_id, .. } | Self::V2 { persona_id, .. } => *persona_id,
        }
    }

    pub fn agent_name(&self) -> &str {
        match self {
            Self::V1 { agent_name, .. } | Self::V2 { agent_name, .. } => agent_name,
        }
    }

    pub fn created_at_ms(&self) -> u64 {
        match self {
            Self::V1 { created_at_ms, .. } | Self::V2 { created_at_ms, .. } => *created_at_ms,
        }
    }

    /// The pinned avatar VRM filename, if this persona's face has been resolved yet.
    pub fn avatar_vrm(&self) -> Option<&str> {
        match self {
            Self::V1 { avatar_vrm, .. } | Self::V2 { avatar_vrm, .. } => avatar_vrm.as_deref(),
        }
    }

    /// Pin the avatar VRM (sticky, resolve-once). Callers MUST only set this when it
    /// is currently `None` — a live pin is never overwritten, so the face never
    /// thrashes once chosen.
    pub fn set_avatar_vrm(&mut self, vrm: String) {
        match self {
            Self::V1 { avatar_vrm, .. } | Self::V2 { avatar_vrm, .. } => *avatar_vrm = Some(vrm),
        }
    }
}

/// Errors that can arise reading or writing a seed file. Typed so
/// callers can dispatch on the failure shape (corrupt → log + mint
/// fresh; permission → escalate; not-found → mint fresh quietly).
#[derive(Debug, thiserror::Error)]
pub enum PersonaSeedError {
    #[error("seed file I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("seed file at {path} is malformed JSON: {source}")]
    Malformed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("seed file at {path} did not exist (not necessarily an error — caller decides)")]
    NotFound { path: PathBuf },
}

impl PersonaSeedError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }
}

/// Read a seed file from the given path. Returns `Ok(seed)` if
/// present + valid; `Err(NotFound)` if absent; `Err(Malformed)` if
/// present but unparseable; `Err(Io)` for any other I/O failure.
///
/// Async — uses `tokio::fs` because file I/O is off-the-hot-path per
/// [[substrate-is-a-good-citizen-on-the-host]]. Never blocks the
/// runtime.
pub async fn read_seed(path: &Path) -> Result<PersonaSeedFile, PersonaSeedError> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(PersonaSeedError::NotFound {
                path: path.to_path_buf(),
            });
        }
        Err(e) => {
            return Err(PersonaSeedError::Io {
                path: path.to_path_buf(),
                source: e,
            });
        }
    };
    let seed: PersonaSeedFile =
        serde_json::from_slice(&bytes).map_err(|e| PersonaSeedError::Malformed {
            path: path.to_path_buf(),
            source: e,
        })?;
    Ok(seed)
}

/// Atomically write a seed file. Writes to `<path>.tmp`, fsyncs,
/// then renames to `<path>`. If anything fails midway, the original
/// (if any) is preserved and the temp file is left on disk for the
/// operator to inspect.
///
/// Per [[substrate-is-a-good-citizen-on-the-host]] doctrine: never
/// leave a half-written persona seed on disk; never crash on write
/// failure; surface the error to the caller for principled handling.
pub async fn write_seed_atomic(
    path: &Path,
    seed: &PersonaSeedFile,
) -> Result<(), PersonaSeedError> {
    let json = serde_json::to_vec_pretty(seed).map_err(|e| PersonaSeedError::Malformed {
        path: path.to_path_buf(),
        source: e,
    })?;

    // Construct the tmp path explicitly from parent + "<filename>.tmp"
    // rather than via `path.with_extension("json.tmp")` — the latter
    // breaks for paths without a `.json` suffix (e.g. `with_extension`
    // would yield `seed.tmp` for a caller passing `seed`, which would
    // then rename OVER `seed`). Reviewer-defect-driven (continuum
    // #1507 finding 3).
    let parent = path.parent().ok_or_else(|| PersonaSeedError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "seed path must have a parent directory",
        ),
    })?;
    let filename =
        path.file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| PersonaSeedError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "seed path must have a UTF-8 file name",
                ),
            })?;
    let tmp_path = parent.join(format!("{filename}.tmp"));

    // Ensure parent directory exists.
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|source| PersonaSeedError::Io {
            path: parent.to_path_buf(),
            source,
        })?;

    // Write to tmp, fsync the file, rename, then fsync the parent
    // directory. The directory fsync is what makes the rename
    // genuinely durable against hard power loss — without it, the
    // rename may not be in the filesystem journal when the system
    // crashes, even though the file contents are. Reviewer-defect-
    // driven (continuum #1507 finding 4); substrate-is-a-good-
    // citizen "reliable" non-negotiable.
    use tokio::io::AsyncWriteExt;
    let mut file =
        tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|source| PersonaSeedError::Io {
                path: tmp_path.clone(),
                source,
            })?;
    file.write_all(&json)
        .await
        .map_err(|source| PersonaSeedError::Io {
            path: tmp_path.clone(),
            source,
        })?;
    file.sync_all()
        .await
        .map_err(|source| PersonaSeedError::Io {
            path: tmp_path.clone(),
            source,
        })?;
    drop(file);

    tokio::fs::rename(&tmp_path, path)
        .await
        .map_err(|source| PersonaSeedError::Io {
            path: tmp_path.clone(),
            source,
        })?;

    // Fsync the parent dir so the rename is durable against crash.
    // Opening dir read-only + sync_all is the standard POSIX
    // pattern. Errors here are surfaced (the caller knows the
    // rename happened in-memory but may not be on disk), per
    // every-error-is-an-opportunity-to-battle-harden — failure to
    // durably persist is signal, not noise.
    let dir = tokio::fs::File::open(parent)
        .await
        .map_err(|source| PersonaSeedError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    dir.sync_all()
        .await
        .map_err(|source| PersonaSeedError::Io {
            path: parent.to_path_buf(),
            source,
        })?;

    Ok(())
}

/// Ensure `seed_path` holds a seed for the LIVE identity (`persona_id` +
/// `agent_name`), self-healing a missing, corrupt, or drifted seed. Idempotent: a
/// resumed persona rewrites the same content; a persona whose seed was deleted or
/// damaged gets it re-written from her running identity — so she can never be
/// re-minted as a stranger while her home (engrams + airc key) sits on disk.
///
/// CRUCIALLY preserves `created_at_ms` (her birth time is stable across restarts):
/// the existing seed's timestamp wins; `fallback_created_at_ms` is used ONLY when
/// no readable seed exists (first mint, or healing a corrupt one). Without this,
/// rewriting every boot would reset her age.
pub async fn ensure_seed(
    seed_path: &Path,
    persona_id: Uuid,
    agent_name: &str,
    fallback_created_at_ms: u64,
) -> Result<(), PersonaSeedError> {
    // Resolve the card to persist, self-healing across all three prior states:
    //
    // - **existing v2** → the durable card is AUTHORITATIVE + editable. Preserve its
    //   card fields (gender, voice_seed, role, avatar_vrm, birth time) and drift-heal
    //   only the live identity + name. Re-deriving gender here would wipe a future
    //   user override every boot — the same clobber class as the avatar pin (#174).
    // - **existing v1** → UPGRADE to v2: derive the coherent card via
    //   `PersonaCard::genesis` from the LIVE identity + name, which reproduces the
    //   exact effective gender the seams used pre-v2 (no shift), preserving the birth
    //   time + pinned avatar the v1 row already carried.
    // - **missing / corrupt** → genesis with the fallback birth time (a corrupt
    //   seed's timestamp is untrusted, so the live boot stands in).
    let card = match read_seed(seed_path).await {
        Ok(existing @ PersonaSeedFile::V2 { .. }) => {
            let mut card = existing.card();
            card.persona_id = persona_id;
            card.agent_name = agent_name.to_string();
            card
        }
        Ok(existing @ PersonaSeedFile::V1 { .. }) => PersonaCard::genesis(
            persona_id,
            agent_name,
            existing.created_at_ms(),
            existing.avatar_vrm().map(String::from),
        ),
        Err(_) => PersonaCard::genesis(persona_id, agent_name, fallback_created_at_ms, None),
    };
    write_seed_atomic(seed_path, &PersonaSeedFile::from_card(&card)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_seed() -> PersonaSeedFile {
        PersonaSeedFile::V1 {
            persona_id: Uuid::parse_str("9d17560c-dbb4-4f9e-86f0-4ceac5d2aff7").unwrap(),
            agent_name: "Pax".to_string(),
            created_at_ms: 1_717_200_000_000,
            avatar_vrm: None,
        }
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        let seed = sample_seed();
        write_seed_atomic(&path, &seed).await.unwrap();
        let read = read_seed(&path).await.unwrap();
        assert_eq!(read, seed);
        assert_eq!(read.agent_name(), "Pax");
    }

    // what this catches (#174): the STICKY invariant — ensure_seed rewrites the seed
    // on every spawn, so it must PRESERVE a pinned avatar_vrm. If it clobbered it to
    // None, the face would re-derive (and thrash) on the next cold boot. This is the
    // exact regression the fix hinges on.
    #[tokio::test]
    async fn ensure_seed_preserves_a_pinned_avatar_across_respawn() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        let pid = Uuid::parse_str("9d17560c-dbb4-4f9e-86f0-4ceac5d2aff7").unwrap();

        // First spawn pins her face.
        let mut seed = PersonaSeedFile::V1 {
            persona_id: pid,
            agent_name: "Pax".to_string(),
            created_at_ms: 1_717_200_000_000,
            avatar_vrm: None,
        };
        assert!(seed.avatar_vrm().is_none());
        seed.set_avatar_vrm("asha.vrm".to_string());
        write_seed_atomic(&path, &seed).await.unwrap();

        // A later spawn calls ensure_seed (a full rewrite) — the pin must survive.
        ensure_seed(&path, pid, "Pax", 9_999_999_999_999)
            .await
            .unwrap();
        let after = read_seed(&path).await.unwrap();
        assert_eq!(
            after.avatar_vrm(),
            Some("asha.vrm"),
            "pin clobbered by ensure_seed"
        );
        // And the original birth time is preserved (ensure_seed doesn't reset it).
        assert_eq!(after.created_at_ms(), 1_717_200_000_000);
    }

    #[tokio::test]
    async fn read_missing_returns_not_found() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nonexistent-seed.json");
        let err = read_seed(&path).await.unwrap_err();
        assert!(err.is_not_found(), "expected NotFound, got {err:?}");
    }

    // what this catches: ensure_seed writes a missing seed from the live identity
    // (self-heal) using the fallback birth time — so a persona whose seed was lost
    // is NOT re-minted as a stranger next boot.
    #[tokio::test]
    async fn ensure_seed_creates_missing_with_fallback_birth_time() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        let id = Uuid::new_v4();
        ensure_seed(&path, id, "Asha", 4242).await.unwrap();
        let read = read_seed(&path).await.unwrap();
        assert_eq!(read.persona_id(), id);
        assert_eq!(read.agent_name(), "Asha");
        assert_eq!(
            read.created_at_ms(),
            4242,
            "no prior seed → fallback is birth time"
        );
    }

    // what this catches: re-running ensure_seed on an EXISTING seed PRESERVES the
    // original created_at_ms (the persona's stable age) even though a new fallback
    // is passed — the bug that would silently reset a resumed persona's birth time
    // every boot. The live persona_id/name still get refreshed (drift-heal).
    #[tokio::test]
    async fn ensure_seed_preserves_birth_time_on_resume() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        let id = Uuid::new_v4();
        ensure_seed(&path, id, "Asha", 1000).await.unwrap();
        // Second boot: a different fallback, but the original 1000 must survive.
        ensure_seed(&path, id, "Asha", 9_999_999).await.unwrap();
        let read = read_seed(&path).await.unwrap();
        assert_eq!(
            read.created_at_ms(),
            1000,
            "birth time is stable across resumes"
        );
        assert_eq!(read.persona_id(), id);
    }

    // what this catches: a CORRUPT seed is healed (overwritten from the live
    // identity) rather than left to poison resume — the persona stays herself.
    #[tokio::test]
    async fn ensure_seed_heals_corrupt_seed() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        tokio::fs::write(&path, b"definitely not json")
            .await
            .unwrap();
        let id = Uuid::new_v4();
        ensure_seed(&path, id, "Asha", 5555).await.unwrap();
        let read = read_seed(&path).await.unwrap();
        assert_eq!(read.persona_id(), id);
        assert_eq!(
            read.created_at_ms(),
            5555,
            "corrupt seed's timestamp is untrusted → fallback"
        );
    }

    // what this catches (#199 migration a): a v1 seed UPGRADES to v2 on the next
    // ensure_seed WITHOUT shifting the persona — her v2 gender equals the effective
    // gender the live seams derived from her NAME pre-v2, and her birth time survives.
    // This is the load-bearing "the existing 8 stabilize, nobody reshuffles" guard.
    #[tokio::test]
    async fn ensure_seed_upgrades_v1_to_v2_without_shifting() {
        use crate::live::avatar::types::AvatarGender;
        use crate::persona::name_generator::gender_from_name;

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        let pid = Uuid::new_v4();
        // "Asha" is unambiguously in the FEMALE pool — the effective pre-v2 gender.
        assert_eq!(gender_from_name("Asha"), Some(AvatarGender::Female));
        let v1 = PersonaSeedFile::V1 {
            persona_id: pid,
            agent_name: "Asha".to_string(),
            created_at_ms: 1_717_200_000_000,
            avatar_vrm: Some("asha.vrm".to_string()),
        };
        write_seed_atomic(&path, &v1).await.unwrap();

        ensure_seed(&path, pid, "Asha", 9_999_999_999_999)
            .await
            .unwrap();
        let after = read_seed(&path).await.unwrap();
        assert!(
            matches!(after, PersonaSeedFile::V2 { .. }),
            "v1 must upgrade to v2"
        );
        let card = after.card();
        assert_eq!(
            card.gender,
            AvatarGender::Female,
            "gender must NOT shift on upgrade"
        );
        assert_eq!(
            card.created_at_ms, 1_717_200_000_000,
            "birth time preserved"
        );
        assert_eq!(
            card.avatar_vrm.as_deref(),
            Some("asha.vrm"),
            "pinned face preserved"
        );
        assert_eq!(card.voice_seed, pid.to_string(), "voice seeds on identity");
    }

    // what this catches: a v2 card is AUTHORITATIVE + editable — ensure_seed must NOT
    // re-derive its gender from the name each boot (a future `airc identity set
    // --gender/--pronouns` override would be wiped otherwise, the same clobber class
    // as the avatar pin #174). We store a gender that GENESIS would NOT produce and
    // prove it survives a respawn.
    #[tokio::test]
    async fn ensure_seed_preserves_a_v2_card_gender_across_respawn() {
        use crate::live::avatar::types::AvatarGender;
        use crate::persona::name_generator::gender_from_name;

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        let pid = Uuid::new_v4();
        // "Niko" is a MALE-pool name, so genesis would derive Male. We deliberately
        // pin Female (a hypothetical override) and require it to stick.
        assert_eq!(gender_from_name("Niko"), Some(AvatarGender::Male));
        let v2 = PersonaSeedFile::V2 {
            persona_id: pid,
            agent_name: "Niko".to_string(),
            created_at_ms: 500,
            gender: AvatarGender::Female,
            avatar_vrm: None,
            voice_seed: pid.to_string(),
            role: None,
            profile: Default::default(),
        };
        write_seed_atomic(&path, &v2).await.unwrap();

        ensure_seed(&path, pid, "Niko", 9_999).await.unwrap();
        let after = read_seed(&path).await.unwrap();
        assert_eq!(
            after.card().gender,
            AvatarGender::Female,
            "a stored v2 gender must be preserved, not re-derived from the name"
        );
        assert_eq!(
            after.created_at_ms(),
            500,
            "birth time preserved on v2 respawn"
        );
    }

    // what this catches: from_card → write → read → card() is a lossless round-trip,
    // so the durable card survives a reboot exactly (the whole point of persisting it).
    #[tokio::test]
    async fn from_card_round_trips_through_disk() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        let pid = Uuid::new_v4();
        let card = PersonaCard::genesis(pid, "Maya", 4242, Some("maya.vrm".to_string()));
        write_seed_atomic(&path, &PersonaSeedFile::from_card(&card))
            .await
            .unwrap();
        let read = read_seed(&path).await.unwrap();
        assert_eq!(
            read.card(),
            card,
            "card must survive the disk round-trip intact"
        );
    }

    #[tokio::test]
    async fn read_malformed_returns_malformed() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("malformed.json");
        tokio::fs::write(&path, b"{ not json at all }")
            .await
            .unwrap();
        let err = read_seed(&path).await.unwrap_err();
        assert!(
            matches!(err, PersonaSeedError::Malformed { .. }),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn write_creates_parent_directory() {
        let temp = TempDir::new().unwrap();
        let nested = temp.path().join("personas").join("Pax").join("seed.json");
        let seed = sample_seed();
        write_seed_atomic(&nested, &seed).await.unwrap();
        assert!(nested.exists());
        let read = read_seed(&nested).await.unwrap();
        assert_eq!(read, seed);
    }

    #[tokio::test]
    async fn write_leaves_no_tmp_file_on_success() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("seed.json");
        let seed = sample_seed();
        write_seed_atomic(&path, &seed).await.unwrap();
        let tmp_path = path.with_extension("json.tmp");
        assert!(
            !tmp_path.exists(),
            "tmp file should be renamed away on success: {}",
            tmp_path.display()
        );
    }
}
