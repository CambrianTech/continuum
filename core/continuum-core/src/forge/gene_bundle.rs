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
//! WHAT A MATCHING DIGEST PROVES, AND WHAT IT DOES NOT (Codex on #4529). Digest equality is
//! TRANSPORT INTEGRITY: the destination serves the bytes that were staged. It is not a
//! learning receipt, and it says nothing about who made the gene. Who made it is
//! `provenance.json`, verified here with [`GenomeProvenance::verify`] against the gene's
//! bytes and the manifest's parents, both when staging and on every read-back.
//!
//! Two different things share the word "signature" here, and the bundle keeps them apart
//! (Codex: `genome/push` reported `signed` when only a behavior descriptor rode along):
//! - `signature.json` is the gene's BEHAVIOR signature, its position in embedding space, used
//!   to route it by distance. Anyone can compute one; it proves nothing about who made it.
//! - `provenance.json` is the CRYPTOGRAPHIC provenance: the forging citizen's ed25519 key over
//!   the gene's content hash and its parents. `provenance_verified` means it VERIFIED.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::provenance::GenomeProvenance;
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
/// The only manifest schema this code reads.
pub const SCHEMA: u32 = 1;

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
    /// The gene's file name (a `.gguf`, listed in `files`).
    pub gene: String,
    /// The gene's direct parents (alloy hashes); empty for a lineage origin.
    pub parents: Vec<String>,
    /// `provenance.json` is present AND verified against the gene's bytes and these parents.
    pub provenance_verified: bool,
    /// `signature.json` (the behavior descriptor) is present.
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

    /// The first 16 hex of the digest: enough to name a bundle in a path or a tag.
    pub fn short(&self) -> &str {
        &self.digest[..self.digest.len().min(16)]
    }

    /// The bundle's name on every provider: `<trait>-<digest16>`, the trait reduced to
    /// `[a-z0-9-]`. One name per bundle, so a second gene can never land where the first is
    /// (Cormac on #4529: HF bundles at the repo root overwrote one another).
    pub fn name(&self) -> String {
        let slug: String = self
            .manifest
            .trait_kind
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let slug = slug.trim_matches('-');
        format!("{}-{}", if slug.is_empty() { "gene" } else { slug }, self.short())
    }
}

/// Why a fetched bundle is not the staged one. The split decides what a caller may do: a
/// CORRUPT copy was positively read and is wrong (partial, altered, malformed), so it may be
/// repaired; an UNREADABLE one could not be judged, so nothing is overwritten on a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Custody {
    Corrupt(String),
    Unreadable(String),
}

impl std::fmt::Display for Custody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Custody::Corrupt(d) => write!(f, "corrupt: {d}"),
            Custody::Unreadable(d) => write!(f, "unreadable: {d}"),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// sha256 of bytes, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// A bundle file name: one flat, ordinary component. No separators, no `.`/`..`, not hidden,
/// not empty. Checked before any name is joined to a path or handed to a CLI (Codex on #4529).
fn is_flat_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0', ':'])
        && name != ".."
}

/// The manifest is well-formed: the known schema, flat unique names, the gene listed and a
/// `.gguf`, no reserved name (the manifest or the card) listed as a file, and the sidecar
/// flags consistent with the files actually listed.
pub fn check_manifest(m: &BundleManifest) -> Result<(), String> {
    if m.schema != SCHEMA {
        return Err(format!("manifest schema {} is not {SCHEMA}", m.schema));
    }
    let mut seen = std::collections::HashSet::new();
    for f in &m.files {
        if !is_flat_name(&f.name) {
            return Err(format!("file name '{}' is not a flat, ordinary name", f.name));
        }
        if f.name == MANIFEST || f.name == CARD {
            return Err(format!("'{}' is reserved and cannot be a bundle file", f.name));
        }
        if !seen.insert(f.name.as_str()) {
            return Err(format!("'{}' is listed twice", f.name));
        }
    }
    if !m.gene.ends_with(".gguf") || !seen.contains(m.gene.as_str()) {
        return Err(format!("the gene '{}' is not a listed .gguf", m.gene));
    }
    if m.has_behavior_signature != seen.contains(BEHAVIOR_SIGNATURE) {
        return Err("has_behavior_signature disagrees with the files listed".into());
    }
    if m.provenance_verified && !seen.contains(PROVENANCE) {
        return Err("provenance_verified with no provenance.json listed".into());
    }
    Ok(())
}

/// Verify `provenance` (the JSON of a `GenomeProvenance`) against the gene's bytes, and its
/// signed parents against `parents`. The signature binds the parents, so a manifest that
/// claims other parents than the signed ones is refused.
fn verify_provenance(provenance: &[u8], gene: &[u8], parents: &[String]) -> Result<(), String> {
    let pv: GenomeProvenance = serde_json::from_slice(provenance).map_err(|e| format!("provenance.json unreadable: {e}"))?;
    pv.verify(gene).map_err(|e| format!("provenance does not verify against the gene: {e}"))?;
    if pv.parent_alloy_hashes != parents {
        return Err(format!("signed parents {:?} differ from the manifest's {parents:?}", pv.parent_alloy_hashes));
    }
    Ok(())
}

/// Stage `req` as a bundle in `dir` (which must be empty or absent). Provenance, when the
/// request carries it, must VERIFY against the gene and its parents before anything is
/// staged: a gene is never published claiming a signature it does not have.
pub fn stage(req: &PublishRequest, dir: &Path) -> Result<GeneBundle, PublishError> {
    let fail = |detail: String| PublishError::Transport { transport: "bundle".to_string(), detail };
    let gene = req
        .gene_path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| is_flat_name(n) && n.ends_with(".gguf"))
        .ok_or_else(|| fail(format!("gene {} is not a flat .gguf file name", req.gene_path.display())))?
        .to_string();
    let gene_bytes = std::fs::read(&req.gene_path).map_err(|e| fail(format!("read gene {}: {e}", req.gene_path.display())))?;
    if let Some(prov) = &req.provenance_json {
        verify_provenance(prov.as_bytes(), &gene_bytes, &req.parent_alloy_hashes)
            .map_err(|e| fail(format!("refusing to publish: {e}")))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| fail(format!("create {}: {e}", dir.display())))?;
    std::fs::write(dir.join(&gene), &gene_bytes).map_err(|e| fail(format!("stage gene: {e}")))?;
    let mut names = vec![gene.clone()];
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
        let bytes = std::fs::read(dir.join(&name)).map_err(|e| fail(format!("{name}: {e}")))?;
        files.push(BundleFile { name, sha256: sha256_hex(&bytes), bytes: bytes.len() as u64 });
    }
    let manifest = BundleManifest {
        schema: SCHEMA,
        base_model: req.base_model.clone(),
        trait_kind: req.trait_kind.clone(),
        gene,
        parents: req.parent_alloy_hashes.clone(),
        provenance_verified: req.provenance_json.is_some(),
        has_behavior_signature: req.signature_json.is_some(),
        files,
    };
    check_manifest(&manifest).map_err(|e| fail(format!("staged manifest invalid: {e}")))?;
    let body = serde_json::to_vec_pretty(&manifest).map_err(|e| fail(format!("manifest: {e}")))?;
    std::fs::write(dir.join(MANIFEST), &body).map_err(|e| fail(format!("write manifest: {e}")))?;
    let card = super::hf_publisher::render_model_card(req);
    Ok(GeneBundle { dir: dir.to_path_buf(), manifest, digest: sha256_hex(&body), card })
}

/// Verify a fetched bundle directory, in the order that keeps a hostile manifest harmless:
/// parse, check the manifest (flat unique names, the gene listed) BEFORE any name touches a
/// path, then hash every file against it, then verify provenance against the gene and the
/// manifest's parents. Returns the digest of the manifest as served; the caller compares it
/// with the digest it staged.
pub fn verify_dir(dir: &Path) -> Result<String, Custody> {
    let body = match std::fs::read(dir.join(MANIFEST)) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Custody::Corrupt(format!("no {MANIFEST} was served")))
        }
        Err(e) => return Err(Custody::Unreadable(format!("{MANIFEST}: {e}"))),
    };
    let manifest: BundleManifest =
        serde_json::from_slice(&body).map_err(|e| Custody::Corrupt(format!("{MANIFEST} is not a bundle manifest: {e}")))?;
    check_manifest(&manifest).map_err(Custody::Corrupt)?;
    for f in &manifest.files {
        let bytes = match std::fs::read(dir.join(&f.name)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Custody::Corrupt(format!("{} was not served", f.name)))
            }
            Err(e) => return Err(Custody::Unreadable(format!("{}: {e}", f.name))),
        };
        if sha256_hex(&bytes) != f.sha256 || bytes.len() as u64 != f.bytes {
            return Err(Custody::Corrupt(format!("{} does not match its manifest", f.name)));
        }
    }
    if manifest.provenance_verified {
        let read = |n: &str| std::fs::read(dir.join(n)).map_err(|e| Custody::Unreadable(format!("{n}: {e}")));
        verify_provenance(&read(PROVENANCE)?, &read(&manifest.gene)?, &manifest.parents).map_err(Custody::Corrupt)?;
    }
    Ok(sha256_hex(&body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::signing::ContractSigningKey;
    use crate::forge::publish_request::PublishInputs;

    const GENE: &[u8] = b"GGUF-lora-bytes";

    fn provenance(gene: &[u8], parents: &[&str]) -> String {
        let key = ContractSigningKey::from_bytes(&[7u8; 32]);
        let pv = GenomeProvenance::sign(&key, gene, parents.iter().map(|s| s.to_string()).collect()).unwrap();
        serde_json::to_string(&pv).unwrap()
    }

    fn request(dir: &Path, signature: Option<&str>, provenance: Option<String>, parents: &[&str]) -> PublishRequest {
        let gene = dir.join("adapter.gguf");
        std::fs::write(&gene, GENE).unwrap();
        PublishRequest::build(
            &PublishInputs {
                repo_id: "continuum-ai/kimi-code".to_string(),
                gene_path: gene,
                base_model: "ggml-org/Qwen3.8-27B-GGUF".to_string(),
                trait_kind: "code".to_string(),
                lift: 0.05,
                signature_json: signature.map(str::to_string),
                provenance_json: provenance,
                parent_alloy_hashes: parents.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            |p| p.exists(),
        )
        .expect("test: valid request")
    }

    // what this catches (Codex, the dual-host acceptance and #4529): the HF path dropped
    // provenance.json, and a published gene could claim a signature it did not have. A bundle
    // carries its provenance VERIFIED with a real key against the gene and its parents; the
    // same content gives the same digest anywhere; read-back re-verifies; and a tampered,
    // missing or unverifiable file is CORRUPT, never accepted. The card is outside identity.
    #[test]
    fn a_bundle_carries_verified_provenance_and_read_back_proves_bytes_and_signer() {
        let src = tempfile::tempdir().unwrap();
        let req = request(src.path(), Some("{\"centroid\":[0.1]}"), Some(provenance(GENE, &["aaaa", "bbbb"])), &["aaaa", "bbbb"]);
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let one = stage(&req, a.path()).unwrap();
        let two = stage(&req, b.path()).unwrap();
        assert_eq!(one.digest, two.digest, "same content, same identity, wherever it is staged");
        assert!(one.manifest.provenance_verified && one.manifest.has_behavior_signature);
        assert_eq!(one.manifest.parents, vec!["aaaa", "bbbb"]);
        assert!(!one.manifest.files.iter().any(|f| f.name == CARD), "the card is outside the identity");
        assert_eq!(verify_dir(a.path()).unwrap(), one.digest, "read-back reproduces the identity");

        std::fs::write(a.path().join(PROVENANCE), provenance(b"other bytes", &["aaaa", "bbbb"])).unwrap();
        assert!(matches!(verify_dir(a.path()), Err(Custody::Corrupt(_))), "a swapped provenance fails read-back");
        std::fs::remove_file(b.path().join(&two.manifest.gene)).unwrap();
        assert!(matches!(verify_dir(b.path()), Err(Custody::Corrupt(_))), "a missing gene is corrupt custody");

        // behavior signature and cryptographic provenance are separate facts
        let c = tempfile::tempdir().unwrap();
        let unsigned = stage(&request(src.path(), Some("{}"), None, &[]), c.path()).unwrap();
        assert!(unsigned.manifest.has_behavior_signature && !unsigned.manifest.provenance_verified);
    }

    // what this catches (Codex on #4529): provenance_signed was `is_some`, so a fake signer
    // JSON read as signed. Staging refuses a provenance that does not verify: a bad signature,
    // parents other than the signed ones, and a gene mutated after it was signed.
    #[test]
    fn staging_refuses_provenance_that_does_not_verify() {
        let src = tempfile::tempdir().unwrap();
        let stage_with = |prov: String, parents: &[&str]| {
            let d = tempfile::tempdir().unwrap();
            stage(&request(src.path(), None, Some(prov), parents), d.path()).map(|_| ())
        };
        assert!(stage_with("{\"signer\":\"k\"}".into(), &[]).is_err(), "not a provenance at all");
        let mut forged: GenomeProvenance = serde_json::from_str(&provenance(GENE, &[])).unwrap();
        forged.signature_hex = "00".repeat(64);
        assert!(stage_with(serde_json::to_string(&forged).unwrap(), &[]).is_err(), "a bad signature");
        assert!(stage_with(provenance(GENE, &["aaaa"]), &["bbbb"]).is_err(), "parents other than the signed ones");
        assert!(stage_with(provenance(b"the gene before it changed", &[]), &[]).is_err(), "a gene mutated after signing");
        assert!(stage_with(provenance(GENE, &["aaaa"]), &["aaaa"]).is_ok());
    }

    // what this catches (Codex on #4529): fetched manifest names reached path.join and CLI
    // args unchecked. A served manifest with a traversal, a duplicate, a reserved name or no
    // gene is refused before any name is used.
    #[test]
    fn a_hostile_manifest_is_refused_before_any_name_touches_a_path() {
        let base = BundleManifest {
            schema: SCHEMA,
            base_model: "b".into(),
            trait_kind: "t".into(),
            gene: "g.gguf".into(),
            parents: vec![],
            provenance_verified: false,
            has_behavior_signature: false,
            files: vec![BundleFile { name: "g.gguf".into(), sha256: "x".into(), bytes: 1 }],
        };
        assert!(check_manifest(&base).is_ok());
        let with = |name: &str| {
            let mut m = base.clone();
            m.files.push(BundleFile { name: name.into(), sha256: "x".into(), bytes: 1 });
            check_manifest(&m)
        };
        for bad in ["../escape", "a/b", "..", ".hidden", "", MANIFEST, CARD, "g.gguf"] {
            assert!(with(bad).is_err(), "'{bad}' must be refused");
        }
        let mut no_gene = base.clone();
        no_gene.gene = "missing.gguf".into();
        assert!(check_manifest(&no_gene).is_err());
        let mut future = base;
        future.schema = 2;
        assert!(check_manifest(&future).is_err(), "an unknown schema is not read as this one");
    }
}
