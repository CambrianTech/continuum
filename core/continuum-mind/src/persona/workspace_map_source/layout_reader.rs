//! Workspace evidence reader: one filesystem/Git observation path for all root adapters.
//! Allocation receipts are immutable; current checkout state and private refs are not.
use super::WorkspaceLayout;
use crate::cognition::faculty_ids::WORKSPACE_MAP;
use crate::code::types::FsEntryKind;
use crate::code::{FileEngine, PathSecurity};
use std::path::PathBuf;
#[derive(Debug, Clone, PartialEq)]
pub struct RepositoryEvidence {
    pub checkout: Result<CheckoutObservation, String>,
    /// Sanitized board identity only; never retain the credential-bearing remote URL.
    pub origin_repo: Option<airc_work::RepoId>,
    pub allocation: Result<Option<airc_lib::work_worktree::AllocationEvidence>, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckoutObservation {
    pub head: String,
    pub branch: String,
    pub shallow: bool,
}

#[derive(Default)]
pub(super) struct LayoutReader {
    allocation: tokio::sync::Mutex<Option<AllocationMemo>>,
}

struct AllocationMemo {
    oid: String,
    creation: String,
    evidence: airc_lib::work_worktree::AllocationEvidence,
}

impl LayoutReader {
    pub(super) async fn read_layout(&self, root: PathBuf) -> Result<WorkspaceLayout, String> {
        let security =
            PathSecurity::new(&root).map_err(|e| format!("workspace security init failed: {e}"))?;
        let engine = FileEngine::new(WORKSPACE_MAP, security);
        // Hidden entries are real workspace contents (#336); every root adapter shares this read.
        let listing = engine
            .list_dir(".", true)
            .map_err(|e| format!("workspace list failed: {e}"))?;
        let top_level_entries = listing.entries.len();
        let mut top_level_dirs: Vec<String> = listing
            .entries
            .into_iter()
            .filter(|e| e.kind == FsEntryKind::Directory)
            .map(|e| e.name)
            .collect();
        top_level_dirs.sort();
        // Match git_bridge's workspace-root discovery ceiling: a parent checkout
        // is not this workspace's repository. Broken markers are not absence.
        let repository_present = match std::fs::symlink_metadata(root.join(".git")) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("repository marker unavailable: {error}")),
        };
        if matches!(repository_present, Ok(false)) {
            return Ok(WorkspaceLayout {
                root,
                top_level_dirs,
                top_level_entries,
                repository: None,
            });
        }
        let checkout = match crate::code::git_bridge::run_git(
            &root,
            &[
                "rev-parse",
                "--is-shallow-repository",
                "HEAD",
                "--symbolic-full-name",
                "HEAD",
            ],
        )
        .await
        {
            Ok(output) => parse_checkout(&output),
            Err(error) => Err(error),
        };
        let allocation = match repository_present {
            Ok(_) => self.read_allocation(&root).await,
            Err(error) => Err(error),
        };
        let origin_repo = crate::code::git_bridge::origin_repo_slug(&root)
            .await
            .and_then(|slug| airc_work::RepoId::new(slug).ok());
        Ok(WorkspaceLayout {
            root,
            top_level_dirs,
            top_level_entries,
            repository: Some(RepositoryEvidence {
                checkout,
                origin_repo,
                allocation,
            }),
        })
    }

    pub(super) async fn read_allocation(
        &self,
        root: &std::path::Path,
    ) -> Result<Option<airc_lib::work_worktree::AllocationEvidence>, String> {
        use crate::code::git_bridge::run_git;
        use airc_lib::work_worktree::{
            decode_allocation_evidence, ALLOCATION_REF, CREATION_BASE_REF,
        };
        // Empty successful enumeration means legacy/no receipt. A failed Git read is
        // unavailable, never absence. The immutable blob pins the remainder of this read.
        let refs = run_git(
            root,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                ALLOCATION_REF,
                CREATION_BASE_REF,
            ],
        )
        .await?;
        let refs: Vec<_> = refs
            .lines()
            .map(|line| {
                line.split_once(' ')
                    .ok_or("malformed allocation ref observation")
            })
            .collect::<Result<_, _>>()?;
        let oid = refs
            .iter()
            .find(|(name, _)| *name == ALLOCATION_REF)
            .map(|(_, oid)| *oid);
        let Some(oid) = oid else {
            return Ok(None);
        };
        let creation = refs
            .iter()
            .find(|(name, _)| *name == CREATION_BASE_REF)
            .map(|(_, oid)| *oid)
            .ok_or("allocation receipt exists without creation anchor")?;
        // Memoize only a validated immutable blob. Re-read both refs on dirty events:
        // replacing a checkout at the same path must not reuse its predecessor's intent.
        let mut cached = self.allocation.lock().await;
        if let Some(known) = cached.as_ref() {
            if known.oid == oid && known.creation == creation {
                return Ok(Some(known.evidence.clone()));
            }
        }
        let bytes = run_git(root, &["cat-file", "blob", oid]).await?;
        let evidence = decode_allocation_evidence(bytes.as_bytes(), creation)?;
        *cached = Some(AllocationMemo {
            oid: oid.into(),
            creation: creation.into(),
            evidence: evidence.clone(),
        });
        Ok(Some(evidence))
    }
}

pub(super) fn parse_checkout(output: &str) -> Result<CheckoutObservation, String> {
    let mut lines = output.lines();
    let shallow = match lines.next() {
        Some("true") => true,
        Some("false") => false,
        _ => return Err("git did not report whether the checkout is shallow".into()),
    };
    let head = lines
        .next()
        .filter(|line| !line.is_empty())
        .ok_or("git did not report HEAD")?
        .to_string();
    let branch = lines
        .next()
        .filter(|line| !line.is_empty())
        .ok_or("git did not report the symbolic HEAD or detached HEAD")?
        .to_string();
    if lines.next().is_some() {
        return Err("git returned unexpected checkout evidence".into());
    }
    Ok(CheckoutObservation {
        head,
        branch,
        shallow,
    })
}

pub(super) fn render_repository(evidence: &RepositoryEvidence) -> String {
    let mut body = match &evidence.allocation {
        Ok(Some(a)) => format!(
            "Recorded allocation: repo {}; requested branch {}; integration base {}; start ref {}; creation commit {}.\n",
            a.repo, a.requested_branch, a.integration_base.as_deref().unwrap_or("unrecorded"), // absent policy is explicitly unknown, not inferred from HEAD
            a.start_ref, a.creation_sha,
        ),
        Ok(None) => "Allocation evidence is unavailable for this checkout; its current HEAD does not establish its intended integration base.\n".into(),
        Err(error) => format!("Allocation evidence could not be read: {error}\n"),
    };
    match &evidence.origin_repo {
        Some(repo) => body.push_str(&format!("Observed origin repository: {repo}.\n")),
        None => body.push_str("Observed origin repository identity unavailable (absent, local, unreadable or unrecognized remote).\n"),
    }
    match &evidence.checkout {
        Ok(git) => {
            body.push_str(&format!(
                "Observed checkout: HEAD {}; branch {}; shallow {}.",
                git.head, git.branch, git.shallow
            ));
            if git.shallow {
                body.push_str(" Missing ancestry in a shallow checkout does not prove a change is absent; source content and available history are distinct evidence.");
            }
        }
        Err(error) => body.push_str(&format!("Checkout observation unavailable: {error}")),
    }
    body
}
