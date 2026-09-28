//! `forge::gh_publisher` — the GitHub [`Publisher`](super::publisher::Publisher) adapter
//! (outlier B; Joel 2026-09-28: genes hosted on HF AND GitHub, public by default, private by
//! configuration).
//!
//! A gene is a RELEASE whose assets are the staged bundle's identity files, byte-for-byte the
//! same as on HF. The release tag names the gene and its identity,
//! `gene-<trait>-<digest16>`, so one gene is one tag and a retry finds it by name. The card
//! is the release notes (provider-specific, outside the identity). Auth and transfer go
//! through the ONE GitHub client, `code/github`'s `run_gh` (the source-hygiene rule: GitHub
//! is never a second `gh` spawn); a private repo is only a matter of which repo the token
//! can write, not a different code path.
//!
//! What this adapter proves about the interface: nothing in the path is HF-shaped. No
//! repo-type, no model-card frontmatter, no revision model; only a destination
//! (`owner/repo`), a bundle, and a read-back.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::gene_bundle::GeneBundle;
use super::publish_request::{PublishError, RepoId};
use super::publisher::{PublicationReceipt, Publisher};

/// The release tag for a bundle: `gene-<trait>-<digest16>`, the bundle's one name.
fn release_tag(bundle: &GeneBundle) -> String {
    format!("gene-{}", bundle.name())
}

/// A definite absence (no such release or repo), as opposed to auth, network or rate limits,
/// which are uncertainty and never read as absent (Cormac, Codex on #4529).
fn is_definitely_absent(error: &str) -> bool {
    ["release not found", "HTTP 404", "Not Found"].iter().any(|m| error.contains(m))
}

fn view_args(repo: &str, tag: &str) -> Vec<String> {
    vec!["release".into(), "view".into(), tag.into(), "--repo".into(), repo.into(), "--json".into(), "tagName".into()]
}

fn create_args(repo: &str, tag: &str, title: &str, notes: &str, files: &[String]) -> Vec<String> {
    let mut args = vec!["release".into(), "create".into(), tag.into()];
    args.extend(files.iter().cloned());
    args.extend(["--repo".into(), repo.into(), "--title".into(), title.into(), "--notes-file".into(), notes.into()]);
    args
}

/// Re-upload every asset of an existing release: the repair of a partial publication.
fn upload_args(repo: &str, tag: &str, files: &[String]) -> Vec<String> {
    let mut args = vec!["release".into(), "upload".into(), tag.into()];
    args.extend(files.iter().cloned());
    args.extend(["--repo".into(), repo.into(), "--clobber".into()]);
    args
}

fn download_args(repo: &str, tag: &str, dir: &str) -> Vec<String> {
    vec!["release".into(), "download".into(), tag.into(), "--repo".into(), repo.into(), "--dir".into(), dir.into(), "--clobber".into()]
}

/// Publishes to GitHub releases through the `gh` CLI.
#[derive(Debug, Default)]
pub struct GhPublisher;

/// How a `gh` invocation failed: a definite absence, or anything else.
enum GhFailure {
    Absent,
    Other(String),
}

impl GhPublisher {
    pub fn new() -> Self {
        Self
    }

    fn uncertain(&self, detail: String) -> PublishError {
        PublishError::Uncertain { transport: self.name().to_string(), detail }
    }

    /// One `gh` call through the shared client. `--repo` names the destination explicitly, so
    /// the working directory is only a neutral place to run from.
    async fn gh(&self, args: &[String]) -> Result<String, GhFailure> {
        crate::commands::code::github::run_gh(std::env::temp_dir(), args.to_vec()).await.map_err(|e| {
            let text = e.to_string();
            if is_definitely_absent(&text) { GhFailure::Absent } else { GhFailure::Other(text) }
        })
    }

    fn receipt(&self, dest: &RepoId, tag: &str, digest: &str) -> PublicationReceipt {
        PublicationReceipt {
            transport: self.name().to_string(),
            location: format!("https://github.com/{}/releases/tag/{tag}", dest.as_str()),
            digest: digest.to_string(),
        }
    }

    fn tag_of(receipt: &PublicationReceipt) -> Option<&str> {
        receipt.location.rsplit_once("/releases/tag/").map(|(_, tag)| tag)
    }
}

#[async_trait]
impl Publisher for GhPublisher {
    fn name(&self) -> &'static str {
        "github"
    }

    async fn find(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<Option<PublicationReceipt>, PublishError> {
        let tag = release_tag(bundle);
        match self.gh(&view_args(dest.as_str(), &tag)).await {
            Ok(_) => Ok(Some(self.receipt(dest, &tag, &bundle.digest))),
            Err(GhFailure::Absent) => Ok(None),
            Err(GhFailure::Other(detail)) => Err(self.uncertain(detail)),
        }
    }

    async fn deliver(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<PublicationReceipt, PublishError> {
        let tag = release_tag(bundle);
        let files: Vec<String> = bundle.files().iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let exists = match self.gh(&view_args(dest.as_str(), &tag)).await {
            Ok(_) => true,
            Err(GhFailure::Absent) => false,
            Err(GhFailure::Other(detail)) => return Err(self.uncertain(detail)),
        };
        if exists {
            // the release exists and was read back as corrupt: repair every asset in place
            self.gh(&upload_args(dest.as_str(), &tag, &files)).await.map_err(|f| match f {
                GhFailure::Absent => self.uncertain(format!("release {tag} vanished during repair")),
                GhFailure::Other(detail) => self.uncertain(detail),
            })?;
        } else {
            let notes = std::env::temp_dir().join(format!("continuum-gh-notes-{}.md", uuid::Uuid::new_v4()));
            // GitHub's card: the shared card plus GitHub's own fetch instructions (outside identity)
            let card = format!(
                "{}## Quick Start\n\n```bash\ngh release download {tag} --repo {}\n# page it into llama-server with:  --lora ./{}\n```\n",
                bundle.card,
                dest.as_str(),
                bundle.manifest.gene
            );
            tokio::fs::write(&notes, card).await.map_err(|e| self.uncertain(format!("could not write release notes: {e}")))?;
            let title = format!("gene {} {}", bundle.manifest.trait_kind, bundle.short());
            let created = self.gh(&create_args(dest.as_str(), &tag, &title, &notes.to_string_lossy(), &files)).await;
            let _ = tokio::fs::remove_file(&notes).await;
            created.map_err(|f| match f {
                GhFailure::Absent => self.uncertain(format!("repo {} not found or not writable", dest.as_str())),
                GhFailure::Other(detail) => self.uncertain(detail),
            })?;
        }
        Ok(self.receipt(dest, &tag, &bundle.digest))
    }

    async fn fetch(&self, dest: &RepoId, receipt: &PublicationReceipt, into: &Path) -> Result<PathBuf, PublishError> {
        let tag = Self::tag_of(receipt).ok_or_else(|| self.uncertain(format!("not a release location: {}", receipt.location)))?;
        std::fs::create_dir_all(into).map_err(|e| self.uncertain(format!("{}: {e}", into.display())))?;
        match self.gh(&download_args(dest.as_str(), tag, &into.to_string_lossy())).await {
            // an absent release fetches nothing, and verify_dir reports that as corrupt custody
            Ok(_) | Err(GhFailure::Absent) => Ok(into.to_path_buf()),
            Err(GhFailure::Other(detail)) => Err(self.uncertain(detail)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the argv the network spawn runs, assertable without a network. One
    // gene is one tag named by its trait and identity, so a retry finds it by name; a create
    // carries every identity file plus the card as notes; a repair re-uploads with --clobber;
    // a fetch downloads the release's assets; and the location round-trips to its tag.
    #[test]
    fn gh_args_address_one_release_per_bundle() {
        let digest = "0123456789abcdef0123456789abcdef";
        assert!(is_definitely_absent("code/github: `gh release view` failed (exit Some(1)): release not found"));
        assert!(!is_definitely_absent("code/github: `gh release view` failed: HTTP 401: Bad credentials"), "auth is uncertainty, never absence");
        let files = vec!["/b/adapter.gguf".to_string(), "/b/manifest.json".to_string()];
        assert_eq!(
            create_args("CambrianTech/genes", "gene-code-01", "gene code 01", "/n.md", &files),
            vec!["release", "create", "gene-code-01", "/b/adapter.gguf", "/b/manifest.json", "--repo", "CambrianTech/genes", "--title", "gene code 01", "--notes-file", "/n.md"]
        );
        assert_eq!(upload_args("o/r", "t", &files).last().map(String::as_str), Some("--clobber"));
        assert_eq!(download_args("o/r", "t", "/d"), vec!["release", "download", "t", "--repo", "o/r", "--dir", "/d", "--clobber"]);
        let dest = RepoId::parse("o/r").unwrap();
        let receipt = GhPublisher::new().receipt(&dest, "gene-code-01", digest);
        assert_eq!(GhPublisher::tag_of(&receipt), Some("gene-code-01"));
    }
}
