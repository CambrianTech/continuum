//! `FilesystemPersonaResolver` — production impl of `PersonaResolver`
//! per task #100/#104 follow-up.
//!
//! Joel (2026-05-31): "PersonaResolver impl that reads
//! `~/.continuum/personas/<name>/seed.json` + attaches via
//! `airc_lib::Airc::attach_as` from the persona's airc home."
//!
//! Resolves the canonical persona-on-disk layout:
//!
//! ```text
//! ~/.continuum/personas/<agent_name>/
//!   ├── seed.json         (persona_id + agent_name, written by PersonaPersistenceModule)
//!   └── airc/             (airc-side home — keypair + per-persona events.sqlite)
//! ```
//!
//! Steps:
//! 1. Read `seed.json` via the existing `persona::seed::read_seed`
//!    (typed errors, async I/O off the hot path).
//! 2. Attach to the running airc daemon at `socket_path` via
//!    `airc_lib::Airc::attach_as(home, name, socket_path)`.
//! 3. Wrap the `airc_lib::Airc` as an `AircTranscriptReader` (the
//!    same `impl AircTranscriptReader for airc_lib::Airc` that
//!    `airc_rag_demo` uses).
//! 4. Optionally attach the host's default inference adapter
//!    (heuristic for CI / sandboxes; LlamaCppAdapter for daily
//!    drivers).
//!
//! ### Doctrine alignment
//!
//! - [[personas-are-citizens-airc-is-identity-provider]] — the
//!   persona's identity lives in seed.json + the airc keypair;
//!   continuum reads, doesn't mint.
//! - [[observability-is-half-the-architecture]] — every resolve
//!   call emits a tracing line so operators see when a persona
//!   was attached (with `agent_name`, persona_id prefix, and
//!   adapter id).
//! - [[substrate-is-a-good-citizen-on-the-host]] — async file
//!   I/O via tokio::fs in `read_seed`; airc attach is async;
//!   the resolver never blocks the tokio runtime.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use crate::ai::adapter::AIProviderAdapter;
use crate::context::citizens_kind_dir;
use crate::identity::IdentityKind;
use crate::persona::airc_source::AircTranscriptReader;
use crate::persona::seed::read_seed;

use super::persona_rag_inspect::{PersonaResolution, PersonaResolver};

/// Production resolver. Reads from the continuum root + the airc
/// socket discovered at construction time.
pub struct FilesystemPersonaResolver {
    continuum_root: PathBuf,
    airc_socket_path: PathBuf,
    /// Optional default adapter for the inference probe. When set,
    /// every resolved persona inherits it (the substrate doesn't
    /// yet model per-persona adapter preferences). When None, the
    /// rag-inspect chain stays RAG-only.
    default_adapter: Option<Arc<dyn AIProviderAdapter>>,
}

impl FilesystemPersonaResolver {
    /// Construct with the continuum root + airc socket path.
    /// Typical args:
    /// - `continuum_root = dirs::home_dir().join(".continuum")`
    /// - `airc_socket_path` discovered via `airc::discover_airc_socket()`
    pub fn new(continuum_root: PathBuf, airc_socket_path: PathBuf) -> Self {
        Self {
            continuum_root,
            airc_socket_path,
            default_adapter: None,
        }
    }

    /// Attach a default inference adapter — every resolved
    /// PersonaResolution will carry this Arc. Production wiring
    /// typically passes `HeuristicInferenceAdapter` for CI hosts
    /// and `LlamaCppAdapter` (Arc-wrapped from
    /// `AIProviderModule`) for production hosts.
    pub fn with_default_adapter(mut self, adapter: Arc<dyn AIProviderAdapter>) -> Self {
        self.default_adapter = Some(adapter);
        self
    }

    /// Read just the seed.json — pure file I/O, no airc. Useful
    /// for tests + callers who want the persona_id without
    /// committing to an airc attach.
    pub async fn read_persona_seed(
        continuum_root: &Path,
        agent_name: &str,
    ) -> Result<crate::persona::seed::PersonaSeedFile, String> {
        let seed_path = seed_path_for(continuum_root, agent_name);
        read_seed(&seed_path)
            .await
            .map_err(|e| format!("read_seed at {}: {e}", seed_path.display()))
    }

    /// Compute the airc home for a persona — exposed for tests +
    /// the production demo binary that needs the same path.
    ///
    /// Derives the scan root from `citizens_kind_dir` — the single
    /// source of truth for the citizen layout — so this read path
    /// can never drift from the Slice-4 write path again (the seed
    /// lives at `citizens/personas/<name>/`, NOT the pre-Slice-4
    /// `personas/<name>/` this file used to hardcode).
    pub fn airc_home_for(continuum_root: &Path, agent_name: &str) -> PathBuf {
        citizens_kind_dir(continuum_root, IdentityKind::Persona)
            .join(agent_name)
            .join("airc")
    }
}

#[async_trait]
impl PersonaResolver for FilesystemPersonaResolver {
    async fn resolve(&self, name: &str) -> Result<PersonaResolution, String> {
        let seed = Self::read_persona_seed(&self.continuum_root, name).await?;
        let persona_id = seed.persona_id();

        let airc_home = Self::airc_home_for(&self.continuum_root, name);
        tokio::fs::create_dir_all(&airc_home)
            .await
            .map_err(|e| format!("ensure airc home {}: {e}", airc_home.display()))?;

        let airc =
            airc_lib::Airc::attach_as(airc_home.clone(), name, self.airc_socket_path.clone())
                .await
                .map_err(|e| {
                    format!(
                        "airc attach_as for persona '{name}' at {}: {e}",
                        airc_home.display()
                    )
                })?;

        let adapter_id = self
            .default_adapter
            .as_ref()
            .map(|a| a.provider_id().to_string());
        tracing::info!(
            persona = name,
            persona_id_prefix = %&persona_id.to_string()[..8],
            adapter = ?adapter_id,
            "FilesystemPersonaResolver: resolved persona"
        );

        let airc_reader: Arc<dyn AircTranscriptReader> = Arc::new(airc);
        Ok(PersonaResolution {
            persona_id,
            airc_reader,
            inference_adapter: self.default_adapter.clone(),
        })
    }
}

fn seed_path_for(continuum_root: &Path, agent_name: &str) -> PathBuf {
    // Single source of truth for the citizen layout — mirrors the Slice-4
    // write path (`citizens/personas/<name>/seed.json`). Re-literaling
    // `join("personas")` here is exactly the drift `citizens_kind_dir`'s
    // doc comment forbids: the write moved, this read didn't, and the
    // glass-box inspector silently 404'd on every live persona.
    citizens_kind_dir(continuum_root, IdentityKind::Persona)
        .join(agent_name)
        .join("seed.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::seed::PersonaSeedFile;
    use uuid::Uuid;

    // Write through the SAME `seed_path_for` the resolver reads — so a
    // future move of the on-disk layout can never let the test's write
    // path drift from production's read path (which is exactly the bug
    // this file just fixed: seeds moved to `citizens/personas/`, the
    // reader still looked in `personas/`).
    fn write_seed_file(root: &Path, agent_name: &str, seed: &PersonaSeedFile) {
        let path = seed_path_for(root, agent_name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let json = serde_json::to_string_pretty(seed).unwrap();
        std::fs::write(path, json).unwrap();
    }

    // ── read_persona_seed (no airc daemon required) ─────────────

    #[tokio::test]
    async fn read_persona_seed_round_trips_a_well_formed_seed() {
        let tmp = tempfile::tempdir().unwrap();
        let persona_id = Uuid::from_u128(0xCAFEBABE);
        let seed = PersonaSeedFile::V1 {
            persona_id,
            agent_name: "Paige".to_string(),
            created_at_ms: 1_700_000_000_000,
            avatar_vrm: None,
        };
        write_seed_file(tmp.path(), "Paige", &seed);

        let loaded = FilesystemPersonaResolver::read_persona_seed(tmp.path(), "Paige")
            .await
            .unwrap();
        assert_eq!(loaded.persona_id(), persona_id);
        assert_eq!(loaded.agent_name(), "Paige");
        assert_eq!(loaded.created_at_ms(), 1_700_000_000_000);
    }

    #[tokio::test]
    async fn read_persona_seed_missing_file_returns_typed_error() {
        let tmp = tempfile::tempdir().unwrap();
        let err = FilesystemPersonaResolver::read_persona_seed(tmp.path(), "Nobody")
            .await
            .unwrap_err();
        // Error message should reference seed_path so operators
        // know where the substrate looked.
        assert!(err.contains("Nobody"));
        assert!(err.contains("seed.json"));
    }

    #[tokio::test]
    async fn read_persona_seed_malformed_returns_typed_error() {
        let tmp = tempfile::tempdir().unwrap();
        let seed_path = seed_path_for(tmp.path(), "Garbage");
        std::fs::create_dir_all(seed_path.parent().unwrap()).unwrap();
        std::fs::write(&seed_path, "{ not valid json ").unwrap();

        let err = FilesystemPersonaResolver::read_persona_seed(tmp.path(), "Garbage")
            .await
            .unwrap_err();
        assert!(err.contains("Garbage"));
        // The malformed error variant gets surfaced through.
        assert!(err.contains("malformed") || err.contains("JSON"));
    }

    // ── path helpers ────────────────────────────────────────────

    // what this catches: the read path must track the Slice-4 write path
    // (`citizens/personas/<name>/`). Before the fix these asserted the dead
    // `personas/<name>/` layout, so the inspector 404'd on every live persona.
    #[test]
    fn airc_home_for_matches_canonical_layout() {
        let root = PathBuf::from("/Users/operator/.continuum");
        let home = FilesystemPersonaResolver::airc_home_for(&root, "Paige");
        assert_eq!(
            home,
            PathBuf::from("/Users/operator/.continuum/citizens/personas/Paige/airc")
        );
    }

    #[test]
    fn seed_path_matches_canonical_layout() {
        let root = PathBuf::from("/Users/operator/.continuum");
        let p = seed_path_for(&root, "Paige");
        assert_eq!(
            p,
            PathBuf::from("/Users/operator/.continuum/citizens/personas/Paige/seed.json")
        );
    }

    // ── default adapter wiring ─────────────────────────────────

    #[tokio::test]
    async fn with_default_adapter_threads_adapter_through_to_resolution_indirectly() {
        // Can't run the full resolve() without a live airc daemon;
        // but we can assert the builder stores the adapter for the
        // production path to pick up.
        use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("airc.sock"); // doesn't exist; we won't attach
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let resolver = FilesystemPersonaResolver::new(tmp.path().to_path_buf(), socket.clone())
            .with_default_adapter(adapter.clone());
        // Adapter is stored — verified by Arc strong_count >= 2
        // (the resolver's clone + ours).
        assert!(Arc::strong_count(&adapter) >= 2);
        let _ = resolver; // keep variable used
    }

    // ── what we deliberately don't unit-test ────────────────────

    // The full `resolve(name)` flow that:
    // 1. Reads seed.json (covered above via read_persona_seed)
    // 2. Ensures airc home dir (trivial)
    // 3. Calls `airc_lib::Airc::attach_as` (requires live daemon)
    //
    // ...is integration-tested by the `airc_rag_demo` binary in
    // src/bin/airc_rag_demo.rs which exercises the same attach path
    // against the operator's live airc daemon. The CI harness slice
    // (next, see strategy doc) wraps this resolver + the demo flow
    // into an automated end-to-end test once an airc-daemon-in-CI
    // story is in place.
}
