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

use std::path::{Path, PathBuf};

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
/// Where a bundle lives in an HF repo: its own folder, named by the bundle.
fn bundle_path(bundle: &GeneBundle) -> String {
    format!("genes/{}", bundle.name())
}

fn upload_args(repo_id: &str, staging_dir: &str, path_in_repo: &str, digest: &str) -> Vec<String> {
    vec![
        "upload".to_string(),
        repo_id.to_string(),
        staging_dir.to_string(),
        path_in_repo.to_string(),
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

/// A definite absence (the repo or the file does not exist), as opposed to auth, network or
/// timeout, which are uncertainty and never read as absent (Cormac, Codex on #4529).
fn is_definitely_absent(stderr: &str) -> bool {
    ["404", "EntryNotFound", "Entry Not Found", "RepositoryNotFound", "Repository Not Found"]
        .iter()
        .any(|m| stderr.contains(m))
}

/// The commit a finished upload landed as, when the CLI reports it (`…/commit/<sha>`).
fn commit_of(stdout: &str) -> Option<String> {
    let (_, after) = stdout.rsplit_once("/commit/")?;
    let sha: String = after.chars().take_while(char::is_ascii_hexdigit).collect();
    (sha.len() >= 7).then_some(sha)
}

/// Publishes to a Hugging Face model repo through the `hf` CLI (auth: HF_TOKEN).
#[derive(Debug, Default)]
pub struct HfPublisher;

/// How an `hf` invocation failed: a definite absence, or anything else.
enum HfFailure {
    Absent,
    Other(String),
}

impl HfPublisher {
    pub fn new() -> Self {
        Self
    }

    fn uncertain(&self, detail: String) -> PublishError {
        PublishError::Uncertain { transport: self.name().to_string(), detail }
    }

    async fn hf(&self, args: &[String]) -> Result<String, HfFailure> {
        let out = tokio::process::Command::new("hf").args(args).output().await.map_err(|e| {
            HfFailure::Other(format!("`hf` CLI not runnable ({e}); install huggingface_hub and authenticate (HF_TOKEN)"))
        })?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if is_definitely_absent(&stderr) { HfFailure::Absent } else { HfFailure::Other(format!("hf {} failed: {stderr}", args[0])) })
    }

    fn receipt(&self, dest: &RepoId, bundle: &GeneBundle, revision: Option<&str>) -> PublicationReceipt {
        PublicationReceipt {
            transport: self.name().to_string(),
            location: format!("https://huggingface.co/{}/tree/{}/{}", dest.as_str(), revision.unwrap_or("main"), bundle_path(bundle)),
            digest: bundle.digest.clone(),
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
        let manifest = format!("{}/{}", bundle_path(bundle), gene_bundle::MANIFEST);
        let result = self.hf(&download_args(dest.as_str(), &[manifest.clone()], &dir.to_string_lossy())).await;
        let served = std::fs::read(dir.join(&manifest)).ok();
        let _ = std::fs::remove_dir_all(&dir);
        match result {
            Err(HfFailure::Absent) => Ok(None),
            Err(HfFailure::Other(detail)) => Err(self.uncertain(detail)),
            // the bundle's own folder is named by its digest, so a manifest there is this
            // bundle's; read-back decides whether the copy is whole
            Ok(_) if served.is_some() => Ok(Some(self.receipt(dest, bundle, None))),
            Ok(_) => Err(self.uncertain("hf download reported success but wrote no manifest".into())),
        }
    }

    async fn deliver(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<PublicationReceipt, PublishError> {
        // HF's card is its README: staged beside the bundle, outside its identity
        tokio::fs::write(bundle.dir.join(gene_bundle::CARD), &bundle.card)
            .await
            .map_err(|e| self.uncertain(format!("could not write model card: {e}")))?;
        let args = upload_args(dest.as_str(), &bundle.dir.to_string_lossy(), &bundle_path(bundle), &bundle.digest);
        let stdout = self.hf(&args).await.map_err(|f| match f {
            HfFailure::Absent => self.uncertain(format!("hf upload: repo {} not found or not writable", dest.as_str())),
            HfFailure::Other(detail) => self.uncertain(detail),
        })?;
        Ok(self.receipt(dest, bundle, commit_of(&stdout).as_deref()))
    }

    async fn fetch(&self, dest: &RepoId, receipt: &PublicationReceipt, into: &Path) -> Result<PathBuf, PublishError> {
        let path = receipt
            .location
            .split_once("/tree/")
            .and_then(|(_, rest)| rest.split_once('/'))
            .map(|(_, path)| path.to_string())
            .ok_or_else(|| self.uncertain(format!("not a bundle location: {}", receipt.location)))?;
        let dir = into.to_string_lossy().into_owned();
        let manifest = format!("{path}/{}", gene_bundle::MANIFEST);
        match self.hf(&download_args(dest.as_str(), &[manifest], &dir)).await {
            Ok(_) | Err(HfFailure::Absent) => {}
            Err(HfFailure::Other(detail)) => return Err(self.uncertain(detail)),
        }
        let bundle_dir = into.join(&path);
        // names reach the CLI only after the served manifest passes the bundle's own check;
        // a malformed one is left for verify_dir to report as corrupt
        let names = std::fs::read(bundle_dir.join(gene_bundle::MANIFEST))
            .ok()
            .and_then(|b| serde_json::from_slice::<gene_bundle::BundleManifest>(&b).ok())
            .filter(|m| gene_bundle::check_manifest(m).is_ok())
            .map(|m| m.files.into_iter().map(|f| format!("{path}/{}", f.name)).collect::<Vec<_>>());
        if let Some(names) = names {
            match self.hf(&download_args(dest.as_str(), &names, &dir)).await {
                Ok(_) | Err(HfFailure::Absent) => {}
                Err(HfFailure::Other(detail)) => return Err(self.uncertain(detail)),
            }
        }
        Ok(bundle_dir)
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
    // what this catches (Cormac on #4529): bundles uploaded to the repo ROOT, so a second gene
    // overwrote the first and its receipt read as a mismatch. Each bundle uploads into its own
    // folder, the digest in the commit; a download names only files; only a definite 404 is
    // "absent" (auth or network is uncertainty); the commit a CLI reports is parsed.
    #[test]
    fn hf_puts_each_bundle_in_its_own_folder_and_only_a_404_is_absent() {
        let args = upload_args("continuum-ai/q", "/tmp/stage", "genes/code-0123456789abcdef", "abc123");
        assert_eq!(
            args,
            vec!["upload", "continuum-ai/q", "/tmp/stage", "genes/code-0123456789abcdef", "--repo-type", "model", "--commit-message", "gene bundle abc123"]
        );
        let dl = download_args("continuum-ai/q", &["genes/x/manifest.json".into()], "/tmp/x");
        assert_eq!(dl, vec!["download", "continuum-ai/q", "genes/x/manifest.json", "--local-dir", "/tmp/x", "--repo-type", "model"]);
        assert!(is_definitely_absent("huggingface_hub.errors.EntryNotFoundError: 404 Client Error"));
        assert!(!is_definitely_absent("401 Client Error: Unauthorized"), "auth is uncertainty, never absence");
        assert!(!is_definitely_absent("Read timed out"));
        assert_eq!(commit_of("done: https://huggingface.co/o/r/commit/0a1b2c3d4e5f\n").as_deref(), Some("0a1b2c3d4e5f"));
        assert_eq!(commit_of("no url here"), None);
    }

    #[test]
    fn publisher_name_is_stable() {
        assert_eq!(HfPublisher::new().name(), "huggingface");
    }
}
