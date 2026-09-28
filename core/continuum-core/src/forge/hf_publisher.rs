//! `forge::hf_publisher` — HuggingFace `Publisher` adapter (#99 L4, outlier A).
//!
//! The public-market impl of [`Publisher`](super::publisher::Publisher): renders a
//! Continuum model card and uploads a staged [`GeneBundle`](super::gene_bundle::GeneBundle)
//! to a HF repo via the `hf` CLI (which owns auth, repo creation, and large-file
//! transfer; we don't re-implement any of that). This is one adapter behind the trait;
//! `GhPublisher` (outlier B, GitHub release assets) satisfies the SAME trait with the SAME
//! bundle bytes, so neither publishing caller learns HF specifics.
//!
//! Testability at the ML boundary: the two things that decide WHAT reaches the
//! world — the model card and the upload command — are pure, tested functions.
//! The network spawn itself is integration (needs `hf` + an `HF_TOKEN`) and fails
//! LOUD via [`PublishError::Transport`] ([[fallbacks-are-illegal-fail-loud]]).

use std::path::Path;

use async_trait::async_trait;

use super::gene_bundle::{self, GeneBundle};
use super::publish_request::{PublishError, PublishRequest, RepoId};
use super::publisher::{PublicationReceipt, Publisher};

/// Renders the Continuum HuggingFace model card (README.md) for a validated
/// publish — a faithful Rust port of the legacy `hf-publish.py::build_model_card`.
/// Pure: everything it needs is denormalized onto the [`PublishRequest`]. The tag
/// frontmatter is the market's facet filter; the body is what a human (or a peer's
/// recall) reads to decide adoption.
pub fn render_model_card(req: &PublishRequest) -> String {
    let repo = req.repo_id.as_str();
    let name = repo.rsplit('/').next().unwrap_or(repo);
    let who = req.persona_name.as_deref().unwrap_or("a Continuum persona");
    let gguf = req
        .gene_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("adapter.gguf");

    let mut s = String::new();
    // --- YAML frontmatter (tags + HF-native fields) ---
    s.push_str("---\ntags:\n");
    for t in &req.tags {
        s.push_str(&format!("- {t}\n"));
    }
    s.push_str("library_name: peft\n");
    s.push_str(&format!("base_model: {}\n", req.base_model));
    s.push_str("---\n\n");

    // --- Body ---
    s.push_str(&format!("# {name}\n\n"));
    s.push_str("## Trained by [Continuum](https://github.com/CambrianTech/continuum)\n\n");
    s.push_str(&format!(
        "This LoRA adapter was trained by **{who}** (role: {}).\n\n",
        req.trait_kind
    ));

    // Lineage + provenance (commons trust spine): a stranger can verify WHO made
    // this and walk its ancestry from the card alone.
    if req.provenance_json.is_some() || !req.parent_alloy_hashes.is_empty() {
        s.push_str("## Provenance & Lineage\n\n");
        if req.provenance_json.is_some() {
            s.push_str(
                "- **Signed** by the forging citizen's key — `provenance.json` (beside the \
                 gene) binds signer + content hash + parents. Verify before trust.\n",
            );
        }
        if req.parent_alloy_hashes.is_empty() {
            s.push_str("- **Root gene** — no parents; this is a lineage origin.\n");
        } else {
            s.push_str("- **Parents** (walk up the tree):\n");
            for h in &req.parent_alloy_hashes {
                s.push_str(&format!("  - `{h}`\n"));
            }
        }
        s.push('\n');
    }

    s.push_str("## Training Results\n\n");
    s.push_str(&format!(
        "- **Held-out lift:** +{:.2} points over the base model \
         (only layers that beat their baseline are published)\n",
        req.lift_pct
    ));
    if let Some(sc) = req.score {
        s.push_str(&format!("- **Score:** {sc}/100\n"));
    }
    if let Some(ep) = req.epochs {
        s.push_str(&format!("- **Epochs:** {ep}\n"));
    }
    if let Some(r) = req.rank {
        s.push_str(&format!("- **LoRA rank:** {r}\n"));
    }
    s.push_str(&format!("- **Base model:** `{}`\n\n", req.base_model));

    s.push_str("## Quick Start\n\n```bash\n");
    s.push_str(&format!("hf download {repo} {gguf} --local-dir .\n"));
    s.push_str("# page it into llama-server with:  --lora ./");
    s.push_str(gguf);
    s.push_str("\n```\n");
    s
}

/// The argv (after the `hf` program) for uploading a staged folder to a repo —
/// factored out so it's assertable without spawning anything. Uploads the whole
/// staging dir (the gguf-lora + the rendered README) to the repo root.
fn upload_args(repo_id: &str, staging_dir: &str, digest: &str) -> Vec<String> {
    vec![
        "upload".to_string(),
        repo_id.to_string(),
        staging_dir.to_string(),
        ".".to_string(),
        "--repo-type".to_string(),
        "model".to_string(),
        "--commit-message".to_string(),
        format!("gene bundle {digest}"),
    ]
}

/// `hf download <repo> <files…> --local-dir <dir>`: only the named files, never the whole repo.
fn download_args(repo_id: &str, files: &[String], dir: &str) -> Vec<String> {
    let mut args = vec!["download".to_string(), repo_id.to_string()];
    args.extend(files.iter().cloned());
    args.extend(["--local-dir".to_string(), dir.to_string(), "--repo-type".to_string(), "model".to_string()]);
    args
}

/// Publishes to a Hugging Face model repo through the `hf` CLI (auth: HF_TOKEN).
#[derive(Debug, Default)]
pub struct HfPublisher;

impl HfPublisher {
    pub fn new() -> Self {
        Self
    }

    fn fail(&self, detail: String) -> PublishError {
        PublishError::Transport { transport: self.name().to_string(), detail }
    }

    async fn hf(&self, args: &[String]) -> Result<(), PublishError> {
        let out = tokio::process::Command::new("hf").args(args).output().await.map_err(|e| {
            self.fail(format!("`hf` CLI not runnable ({e}); install huggingface_hub and authenticate (HF_TOKEN)"))
        })?;
        if !out.status.success() {
            return Err(self.fail(format!("hf {} failed: {}", args[0], String::from_utf8_lossy(&out.stderr).trim())));
        }
        Ok(())
    }

    fn receipt(&self, dest: &RepoId, digest: &str) -> PublicationReceipt {
        PublicationReceipt {
            transport: self.name().to_string(),
            location: format!("https://huggingface.co/{}", dest.as_str()),
            digest: digest.to_string(),
        }
    }
}

#[async_trait]
impl Publisher for HfPublisher {
    fn name(&self) -> &'static str {
        "huggingface"
    }

    async fn find(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<Option<PublicationReceipt>, PublishError> {
        let dir = std::env::temp_dir().join(format!("continuum-hf-find-{}", uuid::Uuid::new_v4()));
        let args = download_args(dest.as_str(), &[gene_bundle::MANIFEST.to_string()], &dir.to_string_lossy());
        // an absent repo or manifest is "not there yet", never an error
        let served = match self.hf(&args).await {
            Ok(()) => std::fs::read(dir.join(gene_bundle::MANIFEST)).ok(),
            Err(_) => None,
        };
        let _ = std::fs::remove_dir_all(&dir);
        let same = served.is_some_and(|body| {
            use sha2::{Digest, Sha256};
            Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect::<String>() == bundle.digest
        });
        Ok(same.then(|| self.receipt(dest, &bundle.digest)))
    }

    async fn deliver(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<PublicationReceipt, PublishError> {
        // HF's card is its README: staged beside the bundle, outside its identity
        tokio::fs::write(bundle.dir.join(gene_bundle::CARD), &bundle.card)
            .await
            .map_err(|e| self.fail(format!("could not write model card: {e}")))?;
        self.hf(&upload_args(dest.as_str(), &bundle.dir.to_string_lossy(), &bundle.digest)).await?;
        Ok(self.receipt(dest, &bundle.digest))
    }

    async fn fetch(&self, dest: &RepoId, _receipt: &PublicationReceipt, into: &Path) -> Result<(), PublishError> {
        let dir = into.to_string_lossy().into_owned();
        self.hf(&download_args(dest.as_str(), &[gene_bundle::MANIFEST.to_string()], &dir)).await?;
        let body = std::fs::read(into.join(gene_bundle::MANIFEST)).map_err(|e| self.fail(format!("manifest not fetched: {e}")))?;
        let manifest: gene_bundle::BundleManifest =
            serde_json::from_slice(&body).map_err(|e| self.fail(format!("served manifest unreadable: {e}")))?;
        let names: Vec<String> = manifest.files.iter().map(|f| f.name.clone()).collect();
        self.hf(&download_args(dest.as_str(), &names, &dir)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::publish_request::{PublishInputs, PublishRequest};
    use std::path::PathBuf;

    fn request() -> PublishRequest {
        PublishRequest::build(
            &PublishInputs {
                repo_id: "continuum-ai/devstral-code-asha".to_string(),
                gene_path: PathBuf::from("/genome/asha/code/adapters-abc123.gguf"),
                base_model: "unsloth/Devstral-Small-2507-GGUF".to_string(),
                trait_kind: "code".to_string(),
                persona_name: Some("Asha".to_string()),
                score: Some(87),
                epochs: Some(3),
                rank: Some(16),
                lift: 0.051,
                ..Default::default()
            },
            |_| true,
        )
        .expect("valid inputs")
    }

    // what this catches: the card frontmatter carries the market's facet tags + the
    // HF-native base_model/library fields, and the body states the lift provenance
    // + a reproducible quick-start. Drift here changes what the world sees + filters
    // on.
    #[test]
    fn model_card_has_frontmatter_tags_and_lift_provenance() {
        let card = render_model_card(&request());
        assert!(
            card.starts_with("---\ntags:\n"),
            "opens with YAML frontmatter"
        );
        assert!(card.contains("- continuum:role=code"));
        assert!(card.contains("- continuum:base=devstral-small-2507-gguf"));
        assert!(card.contains("library_name: peft"));
        assert!(card.contains("base_model: unsloth/Devstral-Small-2507-GGUF"));
        assert!(card.contains("# devstral-code-asha"), "title = repo name");
        assert!(card.contains("trained by **Asha** (role: code)"));
        assert!(
            card.contains("Held-out lift:** +5.10 points"),
            "lift provenance on the card"
        );
        assert!(card.contains("hf download continuum-ai/devstral-code-asha adapters-abc123.gguf"));
    }

    // what this catches: the upload targets the right repo + repo-type, and uploads
    // the staged folder — the argv the network spawn will run, assertable without a
    // network.
    // what this catches: the upload targets the right repo and repo type, uploads the staged
    // bundle folder, and names the bundle digest in the commit; the fetch downloads only the
    // named files. The argv the network spawn runs, assertable without a network.
    #[test]
    fn hf_args_target_the_repo_and_name_the_bundle() {
        let args = upload_args("continuum-ai/qwen3-coder-30b", "/tmp/stage", "abc123");
        assert_eq!(
            args,
            vec!["upload", "continuum-ai/qwen3-coder-30b", "/tmp/stage", ".", "--repo-type", "model", "--commit-message", "gene bundle abc123"]
        );
        let dl = download_args("continuum-ai/q", &["manifest.json".into(), "a.gguf".into()], "/tmp/x");
        assert_eq!(dl, vec!["download", "continuum-ai/q", "manifest.json", "a.gguf", "--local-dir", "/tmp/x", "--repo-type", "model"]);
    }

    #[test]
    fn publisher_name_is_stable() {
        assert_eq!(HfPublisher::new().name(), "huggingface");
    }
}
