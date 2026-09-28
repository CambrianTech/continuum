//! `forge::publisher` — the Publisher adapter seam (#99 L4, slice 2b).
//!
//! Publishing a genome layer to a destination is an ADAPTER behind a trait, the
//! same way inference is an adapter over base models + cloud APIs and
//! [`ForgeCustodian`](super::custodian_client::ForgeCustodian) is an adapter over
//! local-vs-grid export (Joel 2026-07-13: "HF would be an adapter of trait
//! publishers … so we can take each on, including cross-grid, like we do our base
//! models and cloud APIs for inference"). The Owner-gated `forge/publish` command
//! depends only on `dyn Publisher`; WHERE a layer lands — HuggingFace, a trusted
//! grid peer, a private mirror — is a swappable impl, never baked into the caller.
//!
//! Outliers that prove the interface (per the methodical process): `HfPublisher` (outlier
//! A) and `GhPublisher` (GitHub release assets, outlier B; Joel 2026-09-28: HF AND GitHub,
//! OCI later). Both deliver the SAME staged [`GeneBundle`]: one immutable, content-addressed
//! snapshot built once from a validated [`PublishRequest`], so no adapter re-validates or
//! reshapes what a gene is. They only find, deliver and fetch.
//!
//! A publication is proven by READ-BACK, never by an upload's exit status: [`publish_to`]
//! fetches what the destination serves and requires the staged digest. A retry reconciles
//! per (destination, digest): a copy already there and intact is reused, never re-uploaded,
//! and a partial one is repaired.

use std::path::Path;

use async_trait::async_trait;

use super::gene_bundle::{self, GeneBundle};
use super::publish_request::{PublishError, PublishRequest, RepoId};

/// Where a published bundle landed, and which bundle it is. `location` is the
/// transport-native address; `digest` is the bundle identity the destination was verified
/// to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationReceipt {
    /// The adapter that published (`"huggingface"`, `"github"`, …).
    pub transport: String,
    /// The transport-native address the bundle now lives at.
    pub location: String,
    /// The bundle's identity (sha256 of its manifest), as read back from the destination.
    pub digest: String,
}

/// A destination a staged gene bundle can be published to. One trait, many adapters, each a
/// swappable `dyn Publisher` selected by name through [`publisher_for`].
#[async_trait]
pub trait Publisher: Send + Sync {
    /// Short, stable name for logs and selection (`"huggingface"`, `"github"`, …).
    fn name(&self) -> &'static str;

    /// Does `dest` already hold this bundle? `Some` names where; the caller still verifies it
    /// by read-back before trusting it. Used to make a retry idempotent.
    async fn find(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<Option<PublicationReceipt>, PublishError>;

    /// Deliver the bundle's identity files to `dest`, creating or REPAIRING (overwriting a
    /// partial copy of the same bundle). Idempotent. The receipt's digest is the staged one;
    /// [`publish_to`] proves it by read-back.
    async fn deliver(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<PublicationReceipt, PublishError>;

    /// Fetch what `receipt` names at `dest` into `into` (flat files, manifest included).
    async fn fetch(&self, dest: &RepoId, receipt: &PublicationReceipt, into: &Path) -> Result<(), PublishError>;
}

/// Read `receipt` back from `dest` and require that it serves `bundle`'s digest.
async fn verify(publisher: &dyn Publisher, dest: &RepoId, bundle: &GeneBundle, receipt: &PublicationReceipt) -> Result<(), PublishError> {
    let transport = publisher.name().to_string();
    let into = std::env::temp_dir().join(format!("continuum-readback-{}", uuid::Uuid::new_v4()));
    let served = async {
        publisher.fetch(dest, receipt, &into).await?;
        gene_bundle::verify_dir(&into).map_err(|detail| PublishError::Transport { transport: transport.clone(), detail })
    }
    .await;
    let _ = std::fs::remove_dir_all(&into);
    let served = served?;
    if served != bundle.digest {
        return Err(PublishError::DigestMismatch { transport, staged: bundle.digest.clone(), served });
    }
    Ok(())
}

/// Publish `bundle` to `dest` through one provider, proven by read-back. An intact copy
/// already there is reused (no second upload); a partial or wrong one is repaired by
/// delivering again, then verified.
pub async fn publish_to(publisher: &dyn Publisher, dest: &RepoId, bundle: &GeneBundle) -> Result<PublicationReceipt, PublishError> {
    if let Some(existing) = publisher.find(dest, bundle).await? {
        if verify(publisher, dest, bundle, &existing).await.is_ok() {
            return Ok(existing);
        }
    }
    let receipt = publisher.deliver(dest, bundle).await?;
    verify(publisher, dest, bundle, &receipt).await?;
    Ok(receipt)
}

/// The one place a target name becomes a provider. Both publishing callers (`forge/publish`,
/// `genome/push`) select here; neither names a provider type.
pub fn publisher_for(target: &str) -> Result<Box<dyn Publisher>, PublishError> {
    match target {
        "huggingface" | "hf" => Ok(Box::new(super::hf_publisher::HfPublisher::new())),
        "github" | "gh" => Ok(Box::new(super::gh_publisher::GhPublisher::new())),
        other => Err(PublishError::Transport {
            transport: other.to_string(),
            detail: "unknown publish target; the publishers are 'huggingface' and 'github'".to_string(),
        }),
    }
}

/// Stage `req` ONCE and publish that same bundle to every target. Each target succeeds or
/// fails on its own (a partial success is reported per target, and a retry reconciles the
/// ones that already landed rather than re-uploading them).
pub async fn publish_everywhere(req: &PublishRequest, targets: &[String]) -> Result<(GeneBundle, Vec<(String, Result<PublicationReceipt, PublishError>)>), PublishError> {
    let staging = std::env::temp_dir().join(format!("continuum-bundle-{}", uuid::Uuid::new_v4()));
    let bundle = gene_bundle::stage(req, &staging)?;
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        let outcome = match publisher_for(target) {
            Ok(publisher) => publish_to(publisher.as_ref(), &req.repo_id, &bundle).await,
            Err(e) => Err(e),
        };
        results.push((target.clone(), outcome));
    }
    let _ = std::fs::remove_dir_all(&staging);
    Ok((bundle, results))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::publish_request::PublishInputs;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// An in-memory destination: a map from location to the files it serves. `corrupt` makes
    /// every delivery store a tampered copy (a destination serving the wrong bytes).
    #[derive(Default)]
    struct MemoryPublisher {
        store: Mutex<HashMap<String, HashMap<String, Vec<u8>>>>,
        deliveries: Mutex<u32>,
        corrupt: bool,
    }

    impl MemoryPublisher {
        fn location(dest: &RepoId, bundle: &GeneBundle) -> String {
            format!("memory://{}/{}", dest.as_str(), bundle.digest)
        }
    }

    #[async_trait]
    impl Publisher for MemoryPublisher {
        fn name(&self) -> &'static str {
            "memory"
        }
        async fn find(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<Option<PublicationReceipt>, PublishError> {
            let location = Self::location(dest, bundle);
            Ok(self.store.lock().unwrap().contains_key(&location).then(|| PublicationReceipt {
                transport: "memory".into(),
                location,
                digest: bundle.digest.clone(),
            }))
        }
        async fn deliver(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<PublicationReceipt, PublishError> {
            *self.deliveries.lock().unwrap() += 1;
            let mut files = HashMap::new();
            for path in bundle.files() {
                let mut bytes = std::fs::read(&path).unwrap();
                if self.corrupt && path.file_name().unwrap() == bundle.gene_file.as_str() {
                    bytes.push(b'!');
                }
                files.insert(path.file_name().unwrap().to_string_lossy().into_owned(), bytes);
            }
            let location = Self::location(dest, bundle);
            self.store.lock().unwrap().insert(location.clone(), files);
            Ok(PublicationReceipt { transport: "memory".into(), location, digest: bundle.digest.clone() })
        }
        async fn fetch(&self, _dest: &RepoId, receipt: &PublicationReceipt, into: &Path) -> Result<(), PublishError> {
            std::fs::create_dir_all(into).unwrap();
            for (name, bytes) in self.store.lock().unwrap().get(&receipt.location).cloned().unwrap_or_default() {
                std::fs::write(into.join(name), bytes).unwrap();
            }
            Ok(())
        }
    }

    fn staged(dir: &Path) -> (RepoId, GeneBundle) {
        let gene = dir.join("adapter.gguf");
        std::fs::write(&gene, b"GGUF-lora").unwrap();
        let req = PublishRequest::build(
            &PublishInputs {
                repo_id: "continuum-ai/kimi-code".to_string(),
                gene_path: gene,
                base_model: "ggml-org/Qwen3.8-27B-GGUF".to_string(),
                trait_kind: "code".to_string(),
                lift: 0.05,
                provenance_json: Some("{\"signer\":\"k\"}".to_string()),
                parent_alloy_hashes: vec!["aaaa".into()],
                ..Default::default()
            },
            |p| p.exists(),
        )
        .unwrap();
        let bundle = gene_bundle::stage(&req, &dir.join("bundle")).unwrap();
        (req.repo_id, bundle)
    }

    // what this catches (Codex, the dual-host acceptance): a publish trusted on an upload's
    // exit status, a retry that re-uploads a bundle already there, and a destination serving
    // different bytes. Publication is proven by read-back of the staged digest; a retry finds
    // the intact copy and delivers nothing; a partial copy is repaired; a destination that
    // serves the wrong bytes is a typed DigestMismatch.
    #[tokio::test]
    async fn publication_is_proven_by_read_back_and_a_retry_never_reuploads() {
        let dir = tempfile::tempdir().unwrap();
        let (dest, bundle) = staged(dir.path());
        let good = MemoryPublisher::default();
        let first = publish_to(&good, &dest, &bundle).await.expect("test: published");
        assert_eq!(first.digest, bundle.digest);
        publish_to(&good, &dest, &bundle).await.expect("test: retry");
        assert_eq!(*good.deliveries.lock().unwrap(), 1, "an intact copy is reused, never re-uploaded");

        // a partial copy (the gene missing) is repaired by delivering again
        good.store.lock().unwrap().get_mut(&first.location).unwrap().remove(&bundle.gene_file);
        publish_to(&good, &dest, &bundle).await.expect("test: repaired");
        assert_eq!(*good.deliveries.lock().unwrap(), 2);

        // a destination serving a tampered gene fails read-back: never reported as published
        let bad = MemoryPublisher { corrupt: true, ..Default::default() };
        assert!(publish_to(&bad, &dest, &bundle).await.is_err(), "wrong bytes served is not a publication");
    }

    // what this catches: a caller naming a provider type directly (genome/push hardcoded HF).
    // Every target resolves through publisher_for, and an unknown one is a loud, named refusal.
    #[test]
    fn every_target_resolves_through_one_dispatcher() {
        assert_eq!(publisher_for("huggingface").unwrap().name(), "huggingface");
        assert_eq!(publisher_for("github").unwrap().name(), "github");
        assert!(publisher_for("ftp").is_err());
    }
}
