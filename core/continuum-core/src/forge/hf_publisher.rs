//! `forge::hf_publisher` — HuggingFace `Publisher` adapter (#99 L4, outlier A).
//!
//! The public-market impl of [`Publisher`](super::publisher::Publisher): renders a
//! Continuum model card + uploads the gguf-lora to a HF repo via the `hf` CLI
//! (which owns auth, repo creation, and large-file transfer — we don't
//! re-implement any of that). This is one adapter behind the trait; a
//! `GridPublisher` (outlier B) satisfies the SAME trait for peer-to-peer, so the
//! `forge/publish` command never learns HF specifics.
//!
//! Testability at the ML boundary: the two things that decide WHAT reaches the
//! world — the model card and the upload command — are pure, tested functions.
//! The network spawn itself is integration (needs `hf` + an `HF_TOKEN`) and fails
//! LOUD via [`PublishError::Transport`] ([[fallbacks-are-illegal-fail-loud]]).

use async_trait::async_trait;

use super::publish_request::{PublishError, PublishRequest};
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
fn upload_args(repo_id: &str, staging_dir: &str) -> Vec<String> {
    vec![
        "upload".to_string(),
        repo_id.to_string(),
        staging_dir.to_string(),
        ".".to_string(),
        "--repo-type".to_string(),
        "model".to_string(),
    ]
}

/// HuggingFace publisher. Stateless; the `hf` CLI carries the credential
/// (`HF_TOKEN`), so this holds no secret.
#[derive(Debug, Default)]
pub struct HfPublisher;

impl HfPublisher {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Publisher for HfPublisher {
    fn name(&self) -> &'static str {
        "huggingface"
    }

    async fn publish(&self, req: &PublishRequest) -> Result<PublicationReceipt, PublishError> {
        // Stage into a unique temp dir; clean it up on EVERY path (success or
        // failure) so a failed publish never leaks a staging dir.
        let staging =
            std::env::temp_dir().join(format!("continuum-publish-{}", uuid::Uuid::new_v4()));
        let result = self.publish_from_staging(req, &staging).await;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        result
    }
}

impl HfPublisher {
    /// The staged upload, split out so [`publish`](Publisher::publish) can always
    /// clean up the staging dir afterwards. Stages the gguf-lora + rendered card
    /// into `staging`, uploads the folder via the `hf` CLI, returns the receipt.
    async fn publish_from_staging(
        &self,
        req: &PublishRequest,
        staging: &std::path::Path,
    ) -> Result<PublicationReceipt, PublishError> {
        let transport = self.name().to_string();
        let fail = |detail: String| PublishError::Transport {
            transport: transport.clone(),
            detail,
        };

        tokio::fs::create_dir_all(staging)
            .await
            .map_err(|e| fail(format!("could not create staging dir: {e}")))?;
        let gguf_name = req
            .gene_path
            .file_name()
            .ok_or_else(|| fail("gene path has no file name".to_string()))?;
        tokio::fs::copy(&req.gene_path, staging.join(gguf_name))
            .await
            .map_err(|e| {
                fail(format!(
                    "could not stage gene {}: {e}",
                    req.gene_path.display()
                ))
            })?;
        tokio::fs::write(staging.join("README.md"), render_model_card(req))
            .await
            .map_err(|e| fail(format!("could not write model card: {e}")))?;
        // The self-describing half of the gene card: a pulling node stamps its
        // own signature sidecar from this and routes the gene by DISTANCE from
        // the first minute (GENOME-REPOSITORY-ON-HF.md §2). Absent for
        // pre-signature genes — the card still publishes.
        if let Some(sig) = &req.signature_json {
            tokio::fs::write(staging.join("signature.json"), sig)
                .await
                .map_err(|e| fail(format!("could not write signature.json: {e}")))?;
        }

        // Upload via the `hf` CLI (owns auth + large-file transfer). Loud on any
        // non-success — a failed publish is never a silent no-op.
        let args = upload_args(req.repo_id.as_str(), &staging.to_string_lossy());
        let output = tokio::process::Command::new("hf")
            .args(&args)
            .output()
            .await
            .map_err(|e| {
                fail(format!(
                    "`hf` CLI not runnable ({e}) — install huggingface_hub and authenticate (HF_TOKEN)"
                ))
            })?;
        if !output.status.success() {
            return Err(fail(format!(
                "hf upload failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        Ok(PublicationReceipt {
            transport,
            location: format!("https://huggingface.co/{}", req.repo_id.as_str()),
        })
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
    #[test]
    fn upload_args_target_repo_and_model_type() {
        let args = upload_args("continuum-ai/qwen3-coder-30b", "/tmp/stage");
        assert_eq!(
            args,
            vec![
                "upload",
                "continuum-ai/qwen3-coder-30b",
                "/tmp/stage",
                ".",
                "--repo-type",
                "model"
            ]
        );
    }

    #[test]
    fn publisher_name_is_stable() {
        assert_eq!(HfPublisher::new().name(), "huggingface");
    }
}
