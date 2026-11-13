//! PersonaIdentityProvider — the polymorphism rail for "where does
//! a persona's seed come from."
//!
//! ### Why a trait
//!
//! Per the [[organization-purity-as-we-migrate]] + adapter-first
//! methodology Joel articulates ("code the adapters even if there's
//! just ONE to start, that is how I do it"): the substrate ships
//! the interface BEFORE any specific implementation seems
//! "necessary." The interface IS the architectural commitment; the
//! implementation evolves.
//!
//! Concrete providers expected over the next few slices:
//!
//! 1. **`ResumeOrMintProvider`** (slice 4, this PR): scan
//!    `~/.continuum/personas/*/seed.json` at boot; resume each
//!    existing persona; mint fresh on first-run.
//! 2. **`GridImportProvider`** (later): when migrating a citizen
//!    across continuums, the provider sources the seed (and the
//!    associated airc keypair) from a grid-distributed mirror copy.
//! 3. **`HostCustomizedProvider`** (later): the human host explicitly
//!    requests a new persona with a chosen name + theme + initial
//!    genome stack — per [[human-meddling-is-a-substrate-feature]],
//!    customization is welcomed at the substrate level.
//!
//! ### Async by design
//!
//! `next_persona` is async because some providers will do file I/O
//! (ResumeOrMintProvider reads seed.json files) or network I/O
//! (GridImportProvider). Per [[substrate-is-a-good-citizen-on-the-
//! host]] doctrine, file/network ops are never blocking; tokio::fs
//! and friends are mandatory.
//!
//! ### Iterator-shaped vs single-shot
//!
//! The trait yields ONE seed per call rather than a `Vec` because:
//!
//! - Resume + mint policies can interleave (resume existing first,
//!   THEN mint fresh if needed)
//! - Streaming lets the bootstrap path process personas one-at-a-
//!   time, integrating with the registry's event-driven pattern
//! - Future providers (grid-import) might page large populations
//!   from a remote source; iterator shape supports that without
//!   buffering everything
//!
//! When the provider has no more personas to yield, returns
//! `Ok(None)`. This is the "exhausted" signal — bootstrap loop
//! breaks.

use std::path::PathBuf;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::persona::seed::PersonaSeedError;

/// A persona's identity intent, ready to be handed to
/// `PersonaAircRuntime::bootstrap`. Either resumed from disk or
/// freshly minted; the consumer doesn't care which (though the
/// distinction is preserved in telemetry).
#[derive(Debug, Clone)]
pub struct PersonaIdentityIntent {
    pub persona_id: Uuid,
    pub agent_name: String,
    pub source: PersonaIdentitySource,
}

/// Where this identity came from, for telemetry / observability. Per
/// [[substrate-is-a-good-citizen-on-the-host]] — observability honest
/// — the substrate distinguishes resumed vs newly-minted citizens so
/// operators see what happened at boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaIdentitySource.ts"
)]
pub enum PersonaIdentitySource {
    /// Existing persona found on disk + resumed. The airc-side
    /// keypair (identity.key) is loaded by airc-lib; the continuum-
    /// side mapping was read from seed.json.
    ResumedFromDisk,
    /// Fresh persona minted — UUIDv4 + derived name + new keypair
    /// created by airc-lib's identity ceremony. This is the
    /// "first boot" or "explicitly requested new citizen" path.
    FreshlyMinted,
}

/// Errors providers may raise.
#[derive(Debug, thiserror::Error)]
pub enum PersonaIdentityError {
    #[error("seed file error: {0}")]
    Seed(#[from] PersonaSeedError),
    #[error("failed to scan persona home directory {path}: {source}")]
    HomeScanFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The polymorphism rail. Concrete impls decide where seeds come from.
#[async_trait]
pub trait PersonaIdentityProvider: Send + Sync {
    /// Human-readable provider name for telemetry / logs.
    fn name(&self) -> &'static str;

    /// Yield the next persona's identity intent, or `Ok(None)` if
    /// the provider is exhausted.
    async fn next_persona(&mut self)
        -> Result<Option<PersonaIdentityIntent>, PersonaIdentityError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal stub provider used in tests + as a concrete example
    // of the trait shape. Yields a fixed list of intents from a
    // Vec.
    struct StubProvider {
        intents: Vec<PersonaIdentityIntent>,
        cursor: usize,
    }

    #[async_trait]
    impl PersonaIdentityProvider for StubProvider {
        fn name(&self) -> &'static str {
            "stub"
        }
        async fn next_persona(
            &mut self,
        ) -> Result<Option<PersonaIdentityIntent>, PersonaIdentityError> {
            let intent = self.intents.get(self.cursor).cloned();
            if intent.is_some() {
                self.cursor += 1;
            }
            Ok(intent)
        }
    }

    #[tokio::test]
    async fn stub_provider_yields_then_exhausts() {
        let mut provider = StubProvider {
            intents: vec![
                PersonaIdentityIntent {
                    persona_id: Uuid::new_v4(),
                    agent_name: "Pax".to_string(),
                    source: PersonaIdentitySource::ResumedFromDisk,
                },
                PersonaIdentityIntent {
                    persona_id: Uuid::new_v4(),
                    agent_name: "Maya".to_string(),
                    source: PersonaIdentitySource::FreshlyMinted,
                },
            ],
            cursor: 0,
        };
        let first = provider.next_persona().await.unwrap().unwrap();
        assert_eq!(first.agent_name, "Pax");
        assert_eq!(first.source, PersonaIdentitySource::ResumedFromDisk);
        let second = provider.next_persona().await.unwrap().unwrap();
        assert_eq!(second.agent_name, "Maya");
        let exhausted = provider.next_persona().await.unwrap();
        assert!(exhausted.is_none());
    }
}
