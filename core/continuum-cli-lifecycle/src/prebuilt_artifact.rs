//! prebuilt_artifact.rs — the pure decisions for deploying a core that CI built, instead of
//! compiling it on the node (card 50ca737e, slice 2 of 30a8b3ac).
//!
//! CI (`.github/workflows/core-binaries.yml`) builds each canary commit once per platform and
//! publishes `continuum-core-<platform>.tar.gz` + `.sha256` + `.json` under the prerelease
//! `canary-<sha12>`. The deploy consumer downloads its platform's archive, verifies it, and
//! hands the extracted core to `reboot --prebuilt`, which already checks the binary's
//! embedded sha against the checkout and does the supervisor handoff.
//!
//! Everything here is a verdict on values the bin fetched: which platform this node is,
//! whether a manifest is THE build for this tip on this node, whether a tip needs a deploy
//! at all, and what to do while CI has not published yet. The bin keeps the HTTP, the
//! hashing, the extraction and the handoff.

use serde::Deserialize;

use crate::deploy_provenance::sha_matches;

/// Where CI publishes canary builds.
pub const RELEASE_DOWNLOAD_BASE: &str =
    "https://github.com/CambrianTech/continuum/releases/download";

/// The four bins every archive carries, kept together in one directory: the core resolves
/// `forge-custodian` as its own sibling.
pub const REQUIRED_BINS: [&str; 4] = [
    "continuum-core-server",
    "continuum",
    "continuum-mcp",
    "forge-custodian",
];

/// The files whose change changes the binary. A tip that touches none of them since the
/// running build is not a deploy: the node keeps its core, and CI built nothing for it.
/// `core-binaries.yml`'s push `paths` must cover every entry (pinned by a test below), or a
/// change here would ship with no artifact behind it.
pub const BUILD_INPUTS: [&str; 6] = [
    "core/",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "tools/scripts/lib/core-features.sh",
    "tools/scripts/shared/cargo-features.sh",
];

/// `git log` arguments naming `tip`'s BUILD KEY: the newest commit at or before `tip` that
/// touched a [`BUILD_INPUTS`] path. The core binary is a function of those inputs, so every
/// commit between the key and the tip builds the same core, and CI only publishes a core for
/// a commit that touched them (the workflow's `paths`, pinned equal to `BUILD_INPUTS`). A
/// docs-only tip is therefore served by its key's artifact, and a node already running the
/// key is current: no download, no build, no restart (card 9080ffb0).
pub fn build_key_log_args(tip: &str) -> Vec<String> {
    let mut args = vec![
        "log".to_string(),
        "-1".to_string(),
        "--format=%H".to_string(),
        tip.to_string(),
        "--".to_string(),
    ];
    args.extend(BUILD_INPUTS.iter().map(|input| input.to_string()));
    args
}

/// How long a consumer waits for CI to publish a tip before compiling it itself. The
/// workflow's timeout is 150 min; the measured cold builds were 58 min (arm64) and 73 min
/// (x86_64), 2026-10-03. Past this, waiting longer only keeps the node on an old build.
pub const CI_PUBLISH_BUDGET_SECS: u64 = 180 * 60;

/// This node's platform key in CI's matrix, or `None` where no leg publishes yet.
pub fn platform_key(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        _ => None,
    }
}

/// The release that holds `sha`'s build, from a FULL commit sha (the tracker always has one).
pub fn release_tag(sha: &str) -> Option<String> {
    (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("canary-{}", &sha[..12]))
}

/// The manifest URL for `sha` on `platform`.
pub fn manifest_url(sha: &str, platform: &str) -> Option<String> {
    release_tag(sha)
        .map(|tag| format!("{RELEASE_DOWNLOAD_BASE}/{tag}/continuum-core-{platform}.json"))
}

/// The `.json` CI writes beside each archive.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ArtifactManifest {
    pub platform: String,
    pub git_sha: String,
    pub features: String,
    pub archive: String,
    pub sha256: String,
    pub bins: Vec<String>,
}

/// Is this manifest THE build for `tip` on this node? A node with a different feature set
/// (another GPU flavor) must never take it: the same sha with other features is another
/// program. `local_features` is this node's `select_core_features` output.
pub fn manifest_verdict(
    manifest: &ArtifactManifest,
    tip: &str,
    platform: &str,
    local_features: &str,
) -> Result<(), String> {
    if manifest.platform != platform {
        return Err(format!(
            "artifact is for {}, this node is {platform}",
            manifest.platform
        ));
    }
    if !(manifest.git_sha.len() == 40 && sha_matches(&manifest.git_sha, tip)) {
        return Err(format!(
            "artifact is build {}, the tip is {tip}",
            manifest.git_sha
        ));
    }
    let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    if words(&manifest.features) != words(local_features) {
        return Err(format!(
            "artifact was built with `{}`, this node builds with `{}`",
            manifest.features, local_features
        ));
    }
    if !(manifest.sha256.len() == 64 && manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err(format!(
            "artifact checksum `{}` is not a sha256",
            manifest.sha256
        ));
    }
    if let Some(missing) = REQUIRED_BINS
        .iter()
        .find(|b| !manifest.bins.iter().any(|m| m == *b))
    {
        return Err(format!("artifact has no {missing}"));
    }
    Ok(())
}

/// Does a diff (repo-relative paths) change the binary? `false` means the tip is settled
/// without a deploy.
pub fn touches_build_inputs<'a>(changed: impl IntoIterator<Item = &'a str>) -> bool {
    changed.into_iter().any(|path| {
        BUILD_INPUTS
            .iter()
            .any(|input| match input.strip_suffix('/') {
                Some(dir) => path.starts_with(&format!("{dir}/")),
                None => path == *input,
            })
    })
}

/// What the consumer does when its tip has no artifact yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingArtifact {
    /// CI is still inside its budget: try again next tick, without spending an attempt.
    Wait(String),
    /// Compile on the node, saying why.
    BuildFromSource(String),
}

/// `platform` is [`platform_key`]'s answer; `tip_age_secs` is how long ago the tip landed.
pub fn when_artifact_missing(platform: Option<&str>, tip_age_secs: u64) -> MissingArtifact {
    match platform {
        None => MissingArtifact::BuildFromSource(
            "CI publishes no build for this platform yet; compiling here".into(),
        ),
        Some(p) if tip_age_secs < CI_PUBLISH_BUDGET_SECS => MissingArtifact::Wait(format!(
            "CI has not published {p} for this tip yet ({} of {} min); waiting for it",
            tip_age_secs / 60,
            CI_PUBLISH_BUDGET_SECS / 60
        )),
        Some(p) => MissingArtifact::BuildFromSource(format!(
            "CI published no {p} build within {} min of the tip; compiling here",
            CI_PUBLISH_BUDGET_SECS / 60
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIP: &str = "54cbe937f0123456789abcdef0123456789abcde";

    fn manifest() -> ArtifactManifest {
        ArtifactManifest {
            platform: "macos-x86_64".into(),
            git_sha: TIP.into(),
            features: "--no-default-features --features livekit-webrtc,llama/mac-cpu-only".into(),
            archive: "continuum-core-macos-x86_64.tar.gz".into(),
            sha256: "9e08699366264e94d11b6f51c545a5fa0a9a0d3254224cd1afc18c7d11898cea".into(),
            bins: REQUIRED_BINS.iter().map(|b| b.to_string()).collect(),
        }
    }

    // what this catches: a node taking a build that is not the program it would compile —
    // another commit, another platform, or the same commit with another GPU flavor's features.
    #[test]
    fn only_the_exact_build_for_this_node_is_accepted() {
        let local = "--no-default-features  --features livekit-webrtc,llama/mac-cpu-only";
        assert_eq!(
            manifest_verdict(&manifest(), TIP, "macos-x86_64", local),
            Ok(()),
            "whitespace is not a difference"
        );
        assert!(
            manifest_verdict(&manifest(), &TIP.replace('5', "6"), "macos-x86_64", local).is_err(),
            "another commit"
        );
        assert!(
            manifest_verdict(&manifest(), TIP, "macos-arm64", local).is_err(),
            "another platform"
        );
        assert!(
            manifest_verdict(
                &manifest(),
                TIP,
                "macos-x86_64",
                "--features metal,accelerate"
            )
            .is_err(),
            "another feature set"
        );
        let mut short = manifest();
        short.git_sha = TIP[..9].into();
        assert!(
            manifest_verdict(&short, TIP, "macos-x86_64", local).is_err(),
            "a short sha is not provenance"
        );
        let mut partial = manifest();
        partial.bins.retain(|b| b != "forge-custodian");
        assert!(
            manifest_verdict(&partial, TIP, "macos-x86_64", local).is_err(),
            "a sibling bin is missing"
        );
    }

    // what this catches: a docs-only tip forcing every node to rebuild (and CI to build) a
    // binary whose inputs did not change; and the reverse, a dependency bump in the ROOT
    // Cargo.lock being treated as not-a-build (#4691 first shipped with that hole).
    #[test]
    fn only_a_change_to_a_build_input_is_a_deploy() {
        assert!(!touches_build_inputs([
            "README.md",
            "docs/planning/VOICE-ENGINE-PLAN.md"
        ]));
        assert!(
            !touches_build_inputs(["core-notes.md", "coreutils/x"]),
            "a prefix is a directory, not a substring"
        );
        assert!(touches_build_inputs([
            "docs/x.md",
            "core/continuum-core/src/lib.rs"
        ]));
        assert!(touches_build_inputs(["Cargo.lock"]));
        assert!(touches_build_inputs(["tools/scripts/lib/core-features.sh"]));
    }

    // what this catches: the Rust list and CI's trigger drifting apart, so a tip the node
    // treats as a deploy has no artifact (it waits out the budget, then compiles anyway).
    #[test]
    fn ci_builds_on_every_build_input() {
        let workflow = include_str!("../../../.github/workflows/core-binaries.yml");
        let push = workflow
            .split("  push:")
            .nth(1)
            .expect("core-binaries.yml has a push trigger");
        for input in BUILD_INPUTS {
            let pattern = match input.strip_suffix('/') {
                Some(dir) => format!("- '{dir}/**'"),
                None => format!("- '{input}'"),
            };
            assert!(
                push.contains(&pattern),
                "core-binaries.yml push.paths lacks {pattern}"
            );
        }
    }

    // what this catches: a consumer compiling for hours while CI's artifact is minutes away,
    // or waiting forever for a platform CI never builds.
    #[test]
    fn a_missing_artifact_waits_only_while_ci_can_still_deliver() {
        assert!(matches!(
            when_artifact_missing(Some("macos-x86_64"), 20 * 60),
            MissingArtifact::Wait(_)
        ));
        assert!(matches!(
            when_artifact_missing(Some("macos-x86_64"), CI_PUBLISH_BUDGET_SECS),
            MissingArtifact::BuildFromSource(_)
        ));
        assert!(matches!(
            when_artifact_missing(None, 0),
            MissingArtifact::BuildFromSource(_)
        ));
        assert_eq!(platform_key("macos", "x86_64"), Some("macos-x86_64"));
        assert_eq!(platform_key("linux", "x86_64"), None);
        assert_eq!(release_tag(TIP).as_deref(), Some("canary-54cbe937f012"));
        assert_eq!(
            release_tag("54cbe937f"),
            None,
            "a short sha cannot name a release"
        );
    }

    // what this catches: the build key must be computed over exactly the inputs CI builds
    // on (the workflow's paths are pinned to BUILD_INPUTS by the drift test above), with
    // the tip before `--` so git reads every input as a path.
    #[test]
    fn the_build_key_query_covers_every_build_input() {
        let args = build_key_log_args("4e3bc6477");
        let sep = args.iter().position(|a| a == "--").expect("a -- separator");
        assert_eq!(&args[..sep], ["log", "-1", "--format=%H", "4e3bc6477"]);
        assert_eq!(&args[sep + 1..], BUILD_INPUTS.map(String::from));
    }
}
