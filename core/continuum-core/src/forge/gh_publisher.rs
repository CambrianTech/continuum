//! `forge::gh_publisher` — the GitHub [`Publisher`](super::publisher::Publisher) adapter
//! (outlier B; Joel 2026-09-28: genes hosted on HF AND GitHub, public by default, private by
//! configuration).
//!
//! A gene is a RELEASE whose assets are the staged bundle's identity files, byte-for-byte the
//! same as on HF. The release tag names the gene and its identity,
//! `gene-<trait>-<digest16>`, so one gene is one tag and a retry finds it by name. The card
//! is the release notes (provider-specific, outside the identity). Auth and transfer ride
//! the `gh` CLI (`GH_TOKEN` or `gh auth login`); a private repo is only a matter of which
//! repo the token can write, not a different code path.
//!
//! What this adapter proves about the interface: nothing in the path is HF-shaped. No
//! repo-type, no model-card frontmatter, no revision model; only a destination
//! (`owner/repo`), a bundle, and a read-back.

use std::path::Path;

use async_trait::async_trait;

use super::gene_bundle::GeneBundle;
use super::publish_request::{PublishError, RepoId};
use super::publisher::{PublicationReceipt, Publisher};

/// The release tag for a bundle: `gene-<trait>-<first 16 hex of the digest>`. A trait is
/// reduced to `[a-z0-9-]` so any trait name forms a valid tag.
fn release_tag(trait_kind: &str, digest: &str) -> String {
    let slug: String = trait_kind
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "gene" } else { slug };
    format!("gene-{slug}-{}", &digest[..digest.len().min(16)])
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

impl GhPublisher {
    pub fn new() -> Self {
        Self
    }

    fn fail(&self, detail: String) -> PublishError {
        PublishError::Transport { transport: self.name().to_string(), detail }
    }

    async fn gh(&self, args: &[String]) -> Result<(), PublishError> {
        let out = tokio::process::Command::new("gh").args(args).output().await.map_err(|e| {
            self.fail(format!("`gh` CLI not runnable ({e}); install it and authenticate (GH_TOKEN or `gh auth login`)"))
        })?;
        if !out.status.success() {
            return Err(self.fail(format!("gh {} {} failed: {}", args[0], args[1], String::from_utf8_lossy(&out.stderr).trim())));
        }
        Ok(())
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
        let tag = release_tag(&bundle.manifest.trait_kind, &bundle.digest);
        // an absent release is "not there yet", never an error; read-back decides whether it is whole
        Ok(self
            .gh(&view_args(dest.as_str(), &tag))
            .await
            .ok()
            .map(|()| self.receipt(dest, &tag, &bundle.digest)))
    }

    async fn deliver(&self, dest: &RepoId, bundle: &GeneBundle) -> Result<PublicationReceipt, PublishError> {
        let tag = release_tag(&bundle.manifest.trait_kind, &bundle.digest);
        let files: Vec<String> = bundle.files().iter().map(|p| p.to_string_lossy().into_owned()).collect();
        if self.gh(&view_args(dest.as_str(), &tag)).await.is_ok() {
            // the release exists but did not verify: repair every asset in place
            self.gh(&upload_args(dest.as_str(), &tag, &files)).await?;
        } else {
            let notes = std::env::temp_dir().join(format!("continuum-gh-notes-{}.md", uuid::Uuid::new_v4()));
            tokio::fs::write(&notes, &bundle.card).await.map_err(|e| self.fail(format!("could not write release notes: {e}")))?;
            let title = format!("gene {} {}", bundle.manifest.trait_kind, &bundle.digest[..bundle.digest.len().min(16)]);
            let created = self.gh(&create_args(dest.as_str(), &tag, &title, &notes.to_string_lossy(), &files)).await;
            let _ = tokio::fs::remove_file(&notes).await;
            created?;
        }
        Ok(self.receipt(dest, &tag, &bundle.digest))
    }

    async fn fetch(&self, dest: &RepoId, receipt: &PublicationReceipt, into: &Path) -> Result<(), PublishError> {
        let tag = Self::tag_of(receipt).ok_or_else(|| self.fail(format!("not a release location: {}", receipt.location)))?;
        std::fs::create_dir_all(into).map_err(|e| self.fail(format!("{}: {e}", into.display())))?;
        self.gh(&download_args(dest.as_str(), tag, &into.to_string_lossy())).await
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
        assert_eq!(release_tag("code", digest), "gene-code-0123456789abcdef");
        assert_eq!(release_tag("Tool Use/v2", digest), "gene-tool-use-v2-0123456789abcdef");
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
