//! A prebuilt core artifact as an explicit, validated selection: its embedded build SHA
//! read from the binary itself, checked against the checkout's build key while the old
//! core still serves. Launch and verification borrow this SAME path/SHA pair.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use continuum_cli_lifecycle::deploy_provenance::deploy_verdict;
use super::checkout::{prebuilt_checkout_build_key, prebuilt_checkout_sha};
use super::runtime_env::apply_core_runtime_env;

/// An explicitly selected artifact whose loader and provenance were checked while
/// the previous core was still alive. Launch and verification borrow this SAME
/// path/SHA pair; neither may re-resolve an ambient installed-binary override.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct PrebuiltCore {
    pub(super) path: PathBuf,
    pub(super) build_sha: String,
}

impl PrebuiltCore {
    pub(super) async fn prepare(path: &Path) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("prebuilt core {} cannot be resolved: {e}", path.display()))?;
        if !path.is_file() {
            return Err(format!("prebuilt core {} is not a file", path.display()));
        }
        let build_sha = binary_build_sha(&path).await?;
        let cwd = std::env::current_dir()
            .map_err(|e| format!("cannot locate checkout for prebuilt verification: {e}"))?;
        let git_dir = std::env::var_os("GIT_DIR");
        let checkout_sha = prebuilt_checkout_sha(&cwd, git_dir.as_deref()).await?;
        // THE BUILD KEY, NOT HEAD (card 9080ffb0 / #4732). The core is a function of the
        // build inputs, so CI publishes one artifact per commit that touched them and a
        // docs-only tip is served by its key's artifact. Checking that artifact against
        // HEAD refused every such tip in two seconds ("DEPLOY MISMATCH ... shipped build
        // 937976559 (git HEAD)" on the M5, 2026-10-05 01:55Z, for the key f8c69966e).
        let expected = match checkout_sha.as_deref() {
            Some(head) => Some(prebuilt_checkout_build_key(&cwd, git_dir.as_deref(), head)),
            None => None,
        };
        Self::from_report(path, build_sha, expected.as_ref().map(|(sha, src)| (sha.as_str(), *src)))
    }

    pub(super) fn from_report(
        path: PathBuf,
        build_sha: String,
        expected: Option<(&str, &'static str)>,
    ) -> Result<Self, String> {
        // Reuse #194's credible-SHA and short/full-SHA comparison. Without a
        // checkout the artifact anchors its own receipt; unknown/malformed
        // provenance still fails. With a checkout it must match the checkout's
        // build key (its HEAD when git cannot name the key).
        let (expected_sha, source) = expected.unwrap_or((&build_sha, "selected prebuilt artifact"));
        deploy_verdict(Some(&build_sha), expected_sha, source, &path.display().to_string())?;
        Ok(Self { path, build_sha })
    }
}

/// Ask an on-disk `continuum-core-server` artifact for its embedded build SHA
/// (`--build-sha`, exits before any socket/side-effect). Loud on any failure — an artifact
/// that cannot state its provenance cannot anchor a deploy receipt.
pub(super) async fn binary_build_sha(artifact: &Path) -> Result<String, String> {
    let mut cmd = std::process::Command::new(artifact);
    apply_core_runtime_env(&mut cmd)?;
    cmd.arg("--build-sha").stdin(Stdio::null());
    let out = continuum_cli_lifecycle::prebuilt_validation::run(cmd, Duration::from_secs(30))
        .await
        .map_err(|e| format!("{} --build-sha: {e}", artifact.display()))?;
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(format!(
            "{} --build-sha printed nothing — rebuild the artifact",
            artifact.display()
        ));
    }
    Ok(sha)
}
