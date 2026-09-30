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
    s
}

/// HF's own fetch instructions for a bundle: the gene is in its own bundle folder, never at
/// the repo root. The card is written before its upload's commit exists, so it names the
/// branch and says so; the VERIFIED copy is the publish receipt's pinned commit (Codex on
/// #4529).
fn quick_start(repo: &str, revision: &str, path: &str, gene: &str) -> String {
    format!(
        "## Quick Start\n\n```bash\nhf download {repo} {path}/{gene} --revision {revision} --local-dir .\n\
         # page it into llama-server with:  --lora ./{path}/{gene}\n```\n\n\
         `{revision}` is a moving branch. The copy verified at publication is the commit named in \
         the publish receipt; pass that commit as `--revision` to fetch exactly it. Its \
         `manifest.json` lists every file's sha256.\n"
    )
}

/// Where a bundle lives in an HF repo: its own folder, named by the bundle.
fn bundle_path(bundle: &GeneBundle) -> String {
    format!("genes/{}", bundle.name())
}

/// The argv (after the `hf` program) for uploading a staged bundle into its own folder, the
/// digest in the commit message. Assertable without spawning anything.
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

/// `hf download <repo> <files…> --revision <commit> --local-dir <dir>`: only the named files,
/// at one pinned commit, never whatever `main` is by then (Codex on #4529).
fn download_args(repo_id: &str, files: &[String], revision: &str, dir: &str) -> Vec<String> {
    let mut args = vec!["download".to_string(), repo_id.to_string()];
    args.extend(files.iter().cloned());
    args.extend([
        "--revision".to_string(),
        revision.to_string(),
        "--local-dir".to_string(),
        dir.to_string(),
        "--repo-type".to_string(),
        "model".to_string(),
    ]);
    args
}

/// A definite absence is a 404 that is NOT an auth refusal. HF answers a private repo the
/// caller cannot read with a 401 and a `RepositoryNotFound` class, so a class name never
/// decides it; only the status does (Codex on #4529).
fn is_definitely_absent(stderr: &str) -> bool {
    stderr.contains("404") && !stderr.contains("401") && !stderr.contains("403")
}

/// A full 40-hex commit id, and nothing shorter: a receipt pins exactly one commit.
fn is_full_commit(sha: &str) -> bool {
    sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit())
}

/// The full commit a finished upload landed as, when the CLI reports it (`…/commit/<sha>`).
fn commit_of(stdout: &str) -> Option<String> {
    let (_, after) = stdout.rsplit_once("/commit/")?;
    let sha: String = after.chars().take_while(char::is_ascii_hexdigit).collect();
    is_full_commit(&sha).then_some(sha)
}

/// `(revision, path)` from a receipt location `https://huggingface.co/<repo>/tree/<rev>/<path>`.
fn revision_and_path(location: &str) -> Option<(&str, &str)> {
    let (_, rest) = location.split_once("/tree/")?;
    rest.split_once('/')
}

/// HF's API token: `HF_TOKEN`, else the token the `hf` CLI stored at login.
fn hf_token() -> Option<String> {
    std::env::var("HF_TOKEN").ok().filter(|t| !t.trim().is_empty()).or_else(|| {
        let home = std::env::var("HF_HOME")
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| dirs::home_dir().map(|h| h.join(".cache").join("huggingface")))?;
        std::fs::read_to_string(home.join("token")).ok().map(|t| t.trim().to_string())
    })
}

/// Publishes to a Hugging Face model repo through the `hf` CLI (auth: HF_TOKEN).
#[derive(Debug, Default)]
pub struct HfPublisher;

/// How an `hf` call failed: a definite absence, or anything else.
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

    /// The full commit `main` points at now, through HF's API. A 404 (with no auth refusal)
    /// is a repo that does not exist; any other failure is uncertainty.
    async fn resolve_main(&self, repo: &str) -> Result<String, HfFailure> {
        let mut request = reqwest::Client::new()
            .get(format!("https://huggingface.co/api/models/{repo}/revision/main"))
            .timeout(std::time::Duration::from_secs(30));
        if let Some(token) = hf_token() {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|e| HfFailure::Other(format!("resolve {repo}@main: {e}")))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(HfFailure::Absent);
        }
        if !status.is_success() {
            return Err(HfFailure::Other(format!("resolve {repo}@main: HTTP {status}")));
        }
        let body: serde_json::Value = response.json().await.map_err(|e| HfFailure::Other(format!("resolve {repo}@main: {e}")))?;
        body.get("sha")
            .and_then(|s| s.as_str())
            .filter(|s| is_full_commit(s))
            .map(str::to_string)
            .ok_or_else(|| HfFailure::Other(format!("resolve {repo}@main: no full commit sha in the answer")))
    }

    fn receipt(&self, dest: &RepoId, bundle: &GeneBundle, revision: &str) -> PublicationReceipt {
        PublicationReceipt {
            transport: self.name().to_string(),
            location: format!("https://huggingface.co/{}/tree/{revision}/{}", dest.as_str(), bundle_path(bundle)),
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
        let revision = match self.resolve_main(dest.as_str()).await {
            Ok(sha) => sha,
            Err(HfFailure::Absent) => return Ok(None),
            Err(HfFailure::Other(detail)) => return Err(self.uncertain(detail)),
        };
        let dir = std::env::temp_dir().join(format!("continuum-hf-find-{}", uuid::Uuid::new_v4()));
        let manifest = format!("{}/{}", bundle_path(bundle), gene_bundle::MANIFEST);
        let result = self.hf(&download_args(dest.as_str(), &[manifest.clone()], &revision, &dir.to_string_lossy())).await;
        let served = std::fs::read(dir.join(&manifest)).is_ok();
        let _ = std::fs::remove_dir_all(&dir);
        match result {
            Err(HfFailure::Absent) => Ok(None),
            Err(HfFailure::Other(detail)) => Err(self.uncertain(detail)),
            // the bundle's own folder is named by its digest, so a manifest there is this
            // bundle's; the receipt is pinned to the commit it was found at, and read-back
            // decides whether that copy is whole
            Ok(_) if served => Ok(Some(self.receipt(dest, bundle, &revision))),
            Ok(_) => Err(self.uncertain("hf download reported success but wrote no manifest".into())),
        }
    }

    async fn deliver(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<PublicationReceipt, PublishError> {
        let path = bundle_path(bundle);
        // HF's card is its README: the shared card plus HF's own pinned fetch instructions,
        // staged beside the bundle and outside its identity
        tokio::fs::write(bundle.dir.join(gene_bundle::CARD), format!(
            "{}{}",
            bundle.card,
            quick_start(dest.as_str(), "main", &path, &bundle.manifest.gene)
        ))
        .await
        .map_err(|e| self.uncertain(format!("could not write model card: {e}")))?;
        let args = upload_args(dest.as_str(), &bundle.dir.to_string_lossy(), &path, &bundle.digest);
        let stdout = self.hf(&args).await.map_err(|f| match f {
            HfFailure::Absent => self.uncertain(format!("hf upload: repo {} not found or not writable", dest.as_str())),
            HfFailure::Other(detail) => self.uncertain(detail),
        })?;
        // the receipt pins a full commit: the upload's own when the CLI reports it; otherwise
        // main resolved right after the upload, which is a verified snapshot CONTAINING this
        // bundle's folder but not necessarily the upload's exact commit (Codex). Either way
        // read-back verifies that commit. Unresolved is uncertainty, never a silent "main"
        let revision = match commit_of(&stdout) {
            Some(sha) => sha,
            None => self.resolve_main(dest.as_str()).await.map_err(|f| match f {
                HfFailure::Absent => self.uncertain(format!("repo {} vanished after upload", dest.as_str())),
                HfFailure::Other(detail) => self.uncertain(detail),
            })?,
        };
        Ok(self.receipt(dest, bundle, &revision))
    }

    async fn fetch(&self, dest: &RepoId, receipt: &PublicationReceipt, into: &Path) -> Result<PathBuf, PublishError> {
        let (revision, path) = revision_and_path(&receipt.location)
            .filter(|(rev, _)| is_full_commit(rev))
            .ok_or_else(|| self.uncertain(format!("receipt not pinned to a full commit: {}", receipt.location)))?;
        let dir = into.to_string_lossy().into_owned();
        let manifest = format!("{path}/{}", gene_bundle::MANIFEST);
        match self.hf(&download_args(dest.as_str(), &[manifest], revision, &dir)).await {
            Ok(_) | Err(HfFailure::Absent) => {}
            Err(HfFailure::Other(detail)) => return Err(self.uncertain(detail)),
        }
        let bundle_dir = into.join(path);
        // names reach the CLI only after the served manifest passes the bundle's own check;
        // a malformed one is left for verify_dir to report as corrupt
        let names = std::fs::read(bundle_dir.join(gene_bundle::MANIFEST))
            .ok()
            .and_then(|b| serde_json::from_slice::<gene_bundle::BundleManifest>(&b).ok())
            .filter(|m| gene_bundle::check_manifest(m).is_ok())
            .map(|m| m.files.into_iter().map(|f| format!("{path}/{}", f.name)).collect::<Vec<_>>());
        if let Some(names) = names {
            match self.hf(&download_args(dest.as_str(), &names, revision, &dir)).await {
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
        assert!(!card.contains("hf download"), "fetch instructions are each provider's, not the shared card's");
    }

    // what this catches: the upload targets the right repo + repo-type, and uploads
    // the staged folder — the argv the network spawn will run, assertable without a
    // network.
    // what this catches (Cormac, Codex on #4529): bundles at the repo ROOT overwrote one
    // another; a receipt that named a commit but read `main`, so after main advanced the
    // read-back checked other bytes; and a 401 private-repo refusal read as "absent". Each
    // bundle uploads into its own folder; every download is pinned to the receipt's FULL
    // commit (so main advancing after publication changes nothing read back); a short or
    // missing commit is refused; only a 404 without an auth refusal is absent.
    #[test]
    fn hf_reads_are_pinned_to_the_receipts_commit_and_only_a_404_is_absent() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let args = upload_args("continuum-ai/q", "/tmp/stage", "genes/code-0123456789abcdef", "abc123");
        assert_eq!(
            args,
            vec!["upload", "continuum-ai/q", "/tmp/stage", "genes/code-0123456789abcdef", "--repo-type", "model", "--commit-message", "gene bundle abc123"]
        );
        let location = format!("https://huggingface.co/continuum-ai/q/tree/{sha}/genes/code-0123456789abcdef");
        let (rev, path) = revision_and_path(&location).unwrap();
        assert_eq!((rev, path), (sha, "genes/code-0123456789abcdef"));
        let dl = download_args("continuum-ai/q", &[format!("{path}/manifest.json")], rev, "/tmp/x");
        assert_eq!(
            dl,
            vec!["download", "continuum-ai/q", "genes/code-0123456789abcdef/manifest.json", "--revision", sha, "--local-dir", "/tmp/x", "--repo-type", "model"],
            "read-back names the commit, never main"
        );
        assert!(is_definitely_absent("huggingface_hub.errors.EntryNotFoundError: 404 Client Error"));
        assert!(!is_definitely_absent("RepositoryNotFoundError: 401 Client Error: Unauthorized"), "a private repo's 401 is uncertainty");
        assert!(!is_definitely_absent("Read timed out"));
        assert_eq!(commit_of(&format!("done: https://huggingface.co/o/r/commit/{sha}\n")).as_deref(), Some(sha));
        assert_eq!(commit_of("https://huggingface.co/o/r/commit/0a1b2c3"), None, "a short commit does not pin a receipt");
        assert!(quick_start("o/r", sha, "genes/x", "a.gguf").contains("hf download o/r genes/x/a.gguf --revision"));
    }

    #[test]
    fn publisher_name_is_stable() {
        assert_eq!(HfPublisher::new().name(), "huggingface");
    }
}
