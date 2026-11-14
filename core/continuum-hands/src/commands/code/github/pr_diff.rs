//! `code/github/pr-diff` — a pull request's change as a reviewer reads it: one item per file, with
//! its counts and its patch, never one undivided dump. Each file is its own item her view can show
//! whole or summarize, and the whole result stays reachable through its handle. `path` narrows it
//! to a file or a directory, the way a reviewer opens one file at a time.

use std::path::Path;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::pr_head::PrAddress;
use super::{run_gh_owned, checkout_root_for, GhRunError};
use crate::modules::code::CodeState;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrDiffParams.ts")]
pub struct GithubPrDiffParams {
    /// `owner/name`; omit for your workspace's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo: Option<String>,
    /// PR number.
    #[ts(type = "number")]
    pub number: u64,
    /// Only the files at this path: a file, or a directory and everything under it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
}

/// One file's change in the pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrFileDiff.ts")]
pub struct GithubPrFileDiff {
    pub path: String,
    #[ts(type = "number")]
    pub added: u64,
    #[ts(type = "number")]
    pub removed: u64,
    /// The file's unified diff, from its `diff --git` header.
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/code/GithubPrDiffResult.ts")]
pub struct GithubPrDiffResult {
    #[ts(type = "number")]
    pub number: u64,
    pub files: Vec<GithubPrFileDiff>,
    /// Files the pull request changes that `path` left out.
    #[ts(type = "number")]
    pub omitted: u64,
}

/// The path a file's `diff --git a/<old> b/<new>` header names: the new side, where it lives now.
fn header_path(header: &str) -> Option<&str> {
    let sides = header.strip_prefix("diff --git a/")?;
    sides.rsplit_once(" b/").map(|(_, new)| new)
}

/// A unified diff split into one item per file, with its added and removed line counts.
fn files_of(diff: &str) -> Vec<GithubPrFileDiff> {
    let mut files: Vec<GithubPrFileDiff> = Vec::new();
    let mut in_hunk = false;
    for line in diff.lines() {
        if let Some(path) = header_path(line) {
            files.push(GithubPrFileDiff { path: path.to_string(), added: 0, removed: 0, patch: String::new() });
            in_hunk = false;
        }
        let Some(file) = files.last_mut() else { continue };
        in_hunk |= line.starts_with("@@");
        if in_hunk && line.starts_with('+') {
            file.added += 1;
        } else if in_hunk && line.starts_with('-') {
            file.removed += 1;
        }
        file.patch.push_str(line);
        file.patch.push('\n');
    }
    files
}

/// Whether `path` is `filter` or lies under it.
fn within(path: &str, filter: &str) -> bool {
    let filter = filter.trim().trim_end_matches('/');
    filter.is_empty() || path == filter || path.strip_prefix(filter).is_some_and(|rest| rest.starts_with('/'))
}

/// `files`, narrowed to `filter` when one is given, with the count it left out.
fn narrowed(files: Vec<GithubPrFileDiff>, filter: Option<&str>) -> (Vec<GithubPrFileDiff>, u64) {
    let Some(filter) = filter else { return (files, 0) };
    let (kept, left): (Vec<_>, Vec<_>) = files.into_iter().partition(|file| within(&file.path, filter));
    (kept, left.len() as u64)
}

/// Pull request `pr`'s change, read from `root`, narrowed to `filter`.
pub async fn diff_of(root: &Path, pr: &PrAddress<'_>, filter: Option<&str>) -> Result<GithubPrDiffResult, GhRunError> {
    let diff = run_gh_owned(root, &pr.gh_args(&["pr", "diff"]), None).await?;
    let (files, omitted) = narrowed(files_of(&diff), filter);
    Ok(GithubPrDiffResult { number: pr.number, files, omitted })
}

crate::action_command! {
    /// Read a pull request's change (`gh pr diff`), one item per file with its added and removed
    /// line counts and its patch. `path` narrows it to a file or a directory.
    pub struct CodeGithubPrDiff { state: Arc<CodeState> }
    name: "code/github/pr-diff",
    access: AiSafe,
    native: false, // found through help when a review needs it, like pr-view
    effect: Observes,
    disclosure: OnDemand,
    params: GithubPrDiffParams,
    output: GithubPrDiffResult,
    run(this, ctx, p) => {
        let root = checkout_root_for(&this.state, ctx).await?;
        let pr = PrAddress { repo: p.repo.as_deref(), number: p.number };
        Ok(diff_of(&root, &pr, p.path.as_deref()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 fn a() {}
-fn b() {}
+fn b() -> u8 { 1 }
+fn c() {}
diff --git a/docs/old.md b/docs/new.md
similarity index 90%
rename from docs/old.md
rename to docs/new.md
--- a/docs/old.md
+++ b/docs/new.md
@@ -1 +1 @@
-old
+new
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
";

    // what this catches: a PR read as one undivided dump, a file's `---`/`+++` header counted as a
    // removed or added line, a rename filed under its old path, or a deletion dropped.
    #[test]
    fn a_pull_requests_diff_reads_one_item_per_file_with_true_counts() {
        let files = files_of(DIFF);
        let summary: Vec<(&str, u64, u64)> = files.iter().map(|f| (f.path.as_str(), f.added, f.removed)).collect();
        assert_eq!(summary, [("src/lib.rs", 2, 1), ("docs/new.md", 1, 1), ("gone.txt", 0, 1)]);
        assert!(files[0].patch.starts_with("diff --git a/src/lib.rs") && files[0].patch.ends_with("+fn c() {}\n"), "{}", files[0].patch);
        assert_eq!(files_of("").len(), 0);
    }

    // what this catches: a path filter that matches a sibling sharing a prefix (`src` taking
    // `src2/`), drops the directory's own files, or loses count of what it left out.
    #[test]
    fn a_path_narrows_to_a_file_or_a_directory() {
        let (kept, omitted) = narrowed(files_of(DIFF), Some("docs/"));
        assert_eq!((kept.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), omitted), (vec!["docs/new.md"], 2));
        let (kept, _) = narrowed(files_of(DIFF), Some("src/lib.rs"));
        assert_eq!(kept.len(), 1);
        assert!(!within("src2/x.rs", "src"));
        let (all, omitted) = narrowed(files_of(DIFF), None);
        assert_eq!((all.len(), omitted), (3, 0));
    }
}
