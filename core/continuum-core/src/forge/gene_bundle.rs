//! The GENE BUNDLE: one immutable, content-addressed snapshot of a gene, identical on every
//! destination it is published to.
//!
//! A gene is published to more than one provider (Joel, 2026-09-28: Hugging Face AND GitHub,
//! with OCI later), so what is published cannot be shaped by any one of them. The bundle is
//! staged ONCE into a directory of flat files: the gene itself, its behavior signature, its
//! signed provenance, and a `manifest.json` naming every file with its sha256 and size. The
//! bundle's identity is the sha256 of that manifest: the same gene, signature, provenance and
//! parents give the same digest wherever it lands. A provider only DELIVERS those files.
//!
//! The CARD is not part of the identity (Codex): each provider renders its own (HF's model
//! card with its frontmatter, a GitHub release's notes), so a card can change per destination
//! or be improved later without changing which gene this is.
//!
//! Read-back is the proof, never the upload's exit status (Codex, the dual-host acceptance):
//! [`verify_dir`] re-hashes every file a fetch returned against the manifest, and the digest a
//! destination serves must be the digest that was staged.
//!
//! Two different things share the word "signature" here, and the bundle keeps them apart
//! (Codex: `genome/push` reported `signed` when only a behavior descriptor rode along):
//! - `signature.json` is the gene's BEHAVIOR signature, its position in embedding space, used
//!   to route it by distance. Anyone can compute one; it proves nothing about who made it.
//! - `provenance.json` is the CRYPTOGRAPHIC provenance: the forging citizen's ed25519 key over
//!   the gene's content hash and its parents. That is what "signed" means.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::publish_request::{PublishError, PublishRequest};

/// The manifest's file name inside every bundle.
pub const MANIFEST: &str = "manifest.json";
/// The behavior signature's file name (routing by distance; not a cryptographic signature).
pub const BEHAVIOR_SIGNATURE: &str = "signature.json";
/// The cryptographic provenance's file name (ed25519 over the content hash plus parents).
pub const PROVENANCE: &str = "provenance.json";
/// The card's file name when a provider stages one beside the bundle. It is NOT part of the
/// bundle's identity.
pub const CARD: &str = "README.md";

/// One file of a bundle, as the manifest records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFile {
    pub name: String,
    pub sha256: String,
    pub bytes: u64,
}

/// What a bundle IS, independent of where it is published. Serialized with sorted files, so
/// the same content always yields the same bytes and therefore the same digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub schema: u32,
    pub base_model: String,
    pub trait_kind: String,
    /// The gene's direct parents (alloy hashes); empty for a lineage origin.
    pub parents: Vec<String>,
    /// Whether `provenance.json` (cryptographic) is present.
    pub provenance_signed: bool,
    /// Whether `signature.json` (behavior descriptor) is present.
    pub has_behavior_signature: bool,
    pub files: Vec<BundleFile>,
}

/// A staged bundle: its directory, its manifest, and its identity.
#[derive(Debug, Clone)]
pub struct GeneBundle {
    pub dir: PathBuf,
    pub manifest: BundleManifest,
    /// sha256 of `manifest.json`'s bytes, lowercase hex: the bundle's identity.
    pub digest: String,
    /// The gene file's name inside the bundle.
    pub gene_file: String,
    /// The provider-neutral card text; a provider may deliver it (HF's README) or render its
    /// own. Outside the identity.
    pub card: String,
}

impl GeneBundle {
    /// Every IDENTITY file a provider must deliver, manifest included, in a stable order. The
    /// card is not among them.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.manifest.files.iter().map(|f| self.dir.join(&f.name)).collect();
        out.push(self.dir.join(MANIFEST));
        out
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// sha256 of a file's bytes, lowercase hex, and its size.
fn hash_file(path: &Path) -> Result<(String, u64), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((hex(&Sha256::digest(&bytes)), bytes.len() as u64))
}

/// Stage `req` as a bundle in `dir` (which must be empty or absent). Every file the request
/// carries is written, including `provenance.json`, which the HF path used to drop.
pub fn stage(req: &PublishRequest, dir: &Path) -> Result<GeneBundle, PublishError> {
    let fail = |detail: String| PublishError::Transport { transport: "bundle".to_string(), detail };
    std::fs::create_dir_all(dir).map_err(|e| fail(format!("create {}: {e}", dir.display())))?;
    let gene_file = req
        .gene_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| fail("gene path has no file name".to_string()))?
        .to_string();
    let reserved = [MANIFEST, BEHAVIOR_SIGNATURE, PROVENANCE, CARD];
    if reserved.contains(&gene_file.as_str()) {
        return Err(fail(format!("gene file name '{gene_file}' collides with a bundle file")));
    }
    std::fs::copy(&req.gene_path, dir.join(&gene_file))
        .map_err(|e| fail(format!("stage gene {}: {e}", req.gene_path.display())))?;
    let mut names = vec![gene_file.clone()];
    if let Some(sig) = &req.signature_json {
        std::fs::write(dir.join(BEHAVIOR_SIGNATURE), sig).map_err(|e| fail(format!("write signature: {e}")))?;
        names.push(BEHAVIOR_SIGNATURE.to_string());
    }
    if let Some(prov) = &req.provenance_json {
        std::fs::write(dir.join(PROVENANCE), prov).map_err(|e| fail(format!("write provenance: {e}")))?;
        names.push(PROVENANCE.to_string());
    }
    names.sort();
    let mut files = Vec::with_capacity(names.len());
    for name in names {
        let (sha256, bytes) = hash_file(&dir.join(&name)).map_err(fail)?;
        files.push(BundleFile { name, sha256, bytes });
    }
    let manifest = BundleManifest {
        schema: 1,
        base_model: req.base_model.clone(),
        trait_kind: req.trait_kind.clone(),
        parents: req.parent_alloy_hashes.clone(),
        provenance_signed: req.provenance_json.is_some(),
        has_behavior_signature: req.signature_json.is_some(),
        files,
    };
    let body = serde_json::to_vec_pretty(&manifest).map_err(|e| fail(format!("manifest: {e}")))?;
    std::fs::write(dir.join(MANIFEST), &body).map_err(|e| fail(format!("write manifest: {e}")))?;
    let card = super::hf_publisher::render_model_card(req);
    Ok(GeneBundle { dir: dir.to_path_buf(), manifest, digest: hex(&Sha256::digest(&body)), gene_file, card })
}

/// Verify a fetched bundle directory: its manifest parses, every file it names is present
/// with the recorded sha256 and size, and nothing it names is missing. Returns the digest of
/// the manifest as served. The caller compares that digest with the one it staged.
pub fn verify_dir(dir: &Path) -> Result<String, String> {
    let body = std::fs::read(dir.join(MANIFEST)).map_err(|e| format!("no {MANIFEST} in {}: {e}", dir.display()))?;
    let manifest: BundleManifest =
        serde_json::from_slice(&body).map_err(|e| format!("{MANIFEST} is not a bundle manifest: {e}"))?;
    for f in &manifest.files {
        let (sha256, bytes) = hash_file(&dir.join(&f.name))?;
        if sha256 != f.sha256 || bytes != f.bytes {
            return Err(format!("{} does not match its manifest (sha256 {sha256} vs {})", f.name, f.sha256));
        }
    }
    Ok(hex(&Sha256::digest(&body)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::publish_request::PublishInputs;

    fn request(dir: &Path, signature: Option<&str>, provenance: Option<&str>, parents: &[&str]) -> PublishRequest {
        let gene = dir.join("adapter.gguf");
        std::fs::write(&gene, b"GGUF-lora-bytes").unwrap();
        PublishRequest::build(
            &PublishInputs {
                repo_id: "continuum-ai/kimi-code".to_string(),
                gene_path: gene,
                base_model: "ggml-org/Qwen3.8-27B-GGUF".to_string(),
                trait_kind: "code".to_string(),
                lift: 0.05,
                signature_json: signature.map(str::to_string),
                provenance_json: provenance.map(str::to_string),
                parent_alloy_hashes: parents.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            |p| p.exists(),
        )
        .expect("test: valid request")
    }

    // what this catches (Codex, the dual-host acceptance): the HF path dropped provenance.json,
    // so a published gene lost its lineage; and a destination serving different bytes than
    // were staged would pass on an upload's exit status. A bundle carries every file with its
    // hash, the same content gives the same digest, read-back verifies it, and any tampered or
    // missing file fails verification.
    #[test]
    fn a_bundle_carries_its_provenance_and_parents_and_read_back_proves_its_bytes() {
        let src = tempfile::tempdir().unwrap();
        let req = request(src.path(), Some("{\"centroid\":[0.1]}"), Some("{\"signer\":\"k\"}"), &["aaaa", "bbbb"]);
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let one = stage(&req, a.path()).unwrap();
        let two = stage(&req, b.path()).unwrap();
        assert_eq!(one.digest, two.digest, "same content, same identity, wherever it is staged");
        let names: Vec<&str> = one.manifest.files.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&PROVENANCE), "provenance rides along: {names:?}");
        assert!(names.contains(&BEHAVIOR_SIGNATURE));
        assert_eq!(one.manifest.parents, vec!["aaaa", "bbbb"]);
        assert!(one.manifest.provenance_signed && one.manifest.has_behavior_signature);
        assert_eq!(verify_dir(a.path()).unwrap(), one.digest, "read-back reproduces the identity");

        std::fs::write(a.path().join(PROVENANCE), "{\"signer\":\"someone-else\"}").unwrap();
        assert!(verify_dir(a.path()).is_err(), "a tampered file fails read-back");
        std::fs::remove_file(b.path().join(&two.gene_file)).unwrap();
        assert!(verify_dir(b.path()).is_err(), "a missing file fails read-back");
        // the card is outside the identity: a provider's own card does not change the digest
        assert!(!one.manifest.files.iter().any(|f| f.name == CARD));

        // behavior signature and cryptographic provenance are separate facts
        let c = tempfile::tempdir().unwrap();
        let unsigned = stage(&request(src.path(), Some("{}"), None, &[]), c.path()).unwrap();
        assert!(unsigned.manifest.has_behavior_signature && !unsigned.manifest.provenance_signed);
        assert_ne!(unsigned.digest, one.digest);
    }
}
