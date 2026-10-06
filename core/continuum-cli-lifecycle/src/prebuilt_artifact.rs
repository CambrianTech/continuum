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
    /// A CUDA build's toolkit, `major.minor` as nvcc reported it. Its runtime ships in
    /// [`Self::runtime_libs`]; the driver must support that major ([`gpu_verdict`]).
    #[serde(default)]
    pub cuda_version: Option<String>,
    /// The compute-capability floor its kernels were built for, as in `80`: a node whose
    /// lowest GPU is below it cannot run them.
    #[serde(default)]
    pub cuda_compute_cap: Option<u32>,
    /// The runtime DLLs bundled beside the exe, which staging must copy with the core so
    /// launch never depends on which CUDA tree is on PATH.
    #[serde(default)]
    pub runtime_libs: Vec<String>,
}

/// What this node's NVIDIA driver and GPUs can run, read from `nvidia-smi`. `None` fields
/// mean the fact could not be read, which refuses any CUDA artifact (never a guess).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeGpu {
    /// The CUDA version the driver supports, (major, minor) ("CUDA Version: 12.4" -> (12, 4)).
    /// The minor matters: the build's kernels are PTX, JIT-compiled on load, and a driver
    /// refuses PTX newer than its own CUDA version (CUDA_ERROR_UNSUPPORTED_PTX_VERSION).
    pub driver_cuda: Option<(u32, u32)>,
    /// The lowest compute capability among its GPUs ("12.0" -> 120, "8.6" -> 86).
    pub lowest_compute_cap: Option<u32>,
}

/// PURE: a `major.minor` version as (major, minor) ("12.9" -> (12, 9); "13" -> (13, 0)).
pub fn cuda_major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = match parts.next() {
        Some(minor) => minor.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?,
        None => 0,
    };
    Some((major, minor))
}

/// PURE: the driver's CUDA version from `nvidia-smi`'s banner ("... CUDA Version: 12.4 |").
pub fn driver_cuda(nvidia_smi_banner: &str) -> Option<(u32, u32)> {
    let after = nvidia_smi_banner.split("CUDA Version:").nth(1)?;
    cuda_major_minor(after.trim_start().split_whitespace().next()?)
}

/// PURE: the lowest compute capability from `nvidia-smi --query-gpu=compute_cap
/// --format=csv,noheader` (one "major.minor" per GPU), as major*10+minor.
pub fn lowest_compute_cap(csv: &str) -> Option<u32> {
    csv.lines()
        .filter_map(|line| {
            let (major, minor) = line.trim().split_once('.')?;
            Some(major.parse::<u32>().ok()? * 10 + minor.parse::<u32>().ok()?)
        })
        .min()
}

/// Can this node's driver and GPUs run `manifest`'s CUDA build? A build with no
/// `cuda_version` needs nothing (macOS, CPU). A CUDA build needs a driver whose CUDA version
/// is at least its toolkit's, MINOR included (its kernels are PTX; Fable on #4835), and GPUs
/// at or above its compute floor, each refused by name when not.
pub fn gpu_verdict(manifest: &ArtifactManifest, node: &NodeGpu) -> Result<(), String> {
    let Some(version) = manifest.cuda_version.as_deref() else {
        return Ok(());
    };
    let needed = cuda_major_minor(version)
        .ok_or_else(|| format!("artifact names CUDA `{version}`, which is not a major.minor version"))?;
    match node.driver_cuda {
        None => {
            return Err(format!(
                "artifact bundles CUDA {version}, and this node's driver CUDA version could not be read (nvidia-smi)"
            ))
        }
        Some(have) if have < needed => {
            return Err(format!(
                "artifact bundles CUDA {version}, and this node's driver supports CUDA {}.{}: update the NVIDIA driver",
                have.0, have.1
            ))
        }
        Some(_) => {}
    }
    if let Some(floor) = manifest.cuda_compute_cap {
        match node.lowest_compute_cap {
            None => {
                return Err(format!(
                    "artifact needs compute capability {floor}, and this node's GPUs could not be read (nvidia-smi)"
                ))
            }
            Some(lowest) if lowest < floor => {
                return Err(format!(
                    "artifact needs compute capability {floor}, and this node has a GPU at {lowest}"
                ))
            }
            Some(_) => {}
        }
    }
    Ok(())
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

/// How many build keys behind the requested tip the consumer looks for a published core.
/// Canary moves faster than CI publishes a slow platform (IntelMac, 2026-10-06: a tip about
/// every 45 min against about 50 min of build plus up to 55 min queued), so the newest
/// PUBLISHED key is usually one or two behind the tip.
pub const DEPLOY_CANDIDATE_LIMIT: usize = 8;

/// `git log` arguments listing the build keys after `running` up to `tip`, newest first,
/// at most [`DEPLOY_CANDIDATE_LIMIT`]: every core this node could move to that is newer
/// than the one it runs.
pub fn candidate_keys_log_args(running: &str, tip: &str) -> Vec<String> {
    let mut args = vec![
        "log".to_string(),
        "--format=%H".to_string(),
        format!("-{DEPLOY_CANDIDATE_LIMIT}"),
        format!("{running}..{tip}"),
        "--".to_string(),
    ];
    args.extend(BUILD_INPUTS.iter().map(|input| input.to_string()));
    args
}

/// The build to deploy, given the candidates newest first and whether CI published each:
/// the newest PUBLISHED one. `None` = nothing newer than the running core is out yet, so
/// the consumer waits on the newest candidate.
pub fn newest_published<'a>(candidates: &'a [String], published: &[bool]) -> Option<&'a str> {
    candidates
        .iter()
        .zip(published)
        .find(|(_, &out)| out)
        .map(|(key, _)| key.as_str())
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

/// Is the request the consumer is waiting on still the request? A moved request ends the
/// wait so the consumer re-lists its candidates against the new tip. It does NOT mean the
/// old tip's build will never exist: on push, core-binaries.yml cancels only a QUEUED run,
/// and a running one finishes and publishes (8de501e35, 2026-10-06), which is why the
/// candidates include every key since the running core, not only the tip's. `now_requested` is the request file's tip as of
/// this tick; `None` (no request) keeps the wait, since nothing newer was asked for.
pub fn request_superseded(waiting_on: &str, now_requested: Option<&str>) -> Option<String> {
    match now_requested {
        Some(now) if !sha_matches(now, waiting_on) => Some(now.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (M5, 2026-10-04 22:32-00:20Z): a consumer waiting on b85287366,
    // whose CI run was cancelled as superseded, while 3d81d1b09 was built and ready. A moved
    // request ends the wait; the same tip (short or long) and no request do not.
    #[test]
    fn a_moved_request_ends_the_wait_and_the_same_tip_does_not() {
        let waiting = "b852873662ecbeff1004f13b4fe929b9012ac2d6";
        assert_eq!(request_superseded(waiting, Some("3d81d1b09abc")), Some("3d81d1b09abc".into()));
        assert_eq!(request_superseded(waiting, Some("b85287366")), None, "the same tip, short");
        assert_eq!(request_superseded(waiting, Some(waiting)), None, "the same tip, long");
        assert_eq!(request_superseded(waiting, None), None, "no request = nothing newer asked for");
    }

    const TIP: &str = "54cbe937f0123456789abcdef0123456789abcde";

    fn manifest() -> ArtifactManifest {
        ArtifactManifest {
            platform: "macos-x86_64".into(),
            git_sha: TIP.into(),
            features: "--no-default-features --features livekit-webrtc,llama/mac-cpu-only".into(),
            archive: "continuum-core-macos-x86_64.tar.gz".into(),
            sha256: "9e08699366264e94d11b6f51c545a5fa0a9a0d3254224cd1afc18c7d11898cea".into(),
            bins: REQUIRED_BINS.iter().map(|b| b.to_string()).collect(),
            cuda_version: None,
            cuda_compute_cap: None,
            runtime_libs: Vec::new(),
        }
    }

    // what this catches (the 5090, 2026-10-06): a CUDA core staged onto a node whose driver
    // cannot load its runtime, or whose GPU is below its kernels' floor, dies at launch.
    // A build without CUDA needs nothing; the driver's major and the lowest GPU decide.
    #[test]
    fn a_cuda_build_runs_only_where_the_driver_and_gpus_can() {
        let banner = "| NVIDIA-SMI 580.97  Driver Version: 580.97  CUDA Version: 13.0 |";
        assert_eq!(driver_cuda(banner), Some((13, 0)));
        assert_eq!(driver_cuda("no gpu here"), None);
        assert_eq!(lowest_compute_cap("12.0\n8.6\n"), Some(86));
        assert_eq!(lowest_compute_cap(""), None);

        let cpu = manifest();
        assert!(gpu_verdict(&cpu, &NodeGpu::default()).is_ok(), "no CUDA, nothing to check");

        let mut cuda = manifest();
        cuda.cuda_version = Some("12.9".into());
        cuda.cuda_compute_cap = Some(80);
        let the_5090 = NodeGpu { driver_cuda: Some((13, 0)), lowest_compute_cap: Some(120) };
        assert!(gpu_verdict(&cuda, &the_5090).is_ok());
        let old_driver = NodeGpu { driver_cuda: Some((11, 8)), ..the_5090.clone() };
        assert!(gpu_verdict(&cuda, &old_driver).unwrap_err().contains("update the NVIDIA driver"));
        // the same major is not enough: PTX from nvcc 12.9 does not JIT on a 12.4 driver
        let older_minor = NodeGpu { driver_cuda: Some((12, 4)), ..the_5090.clone() };
        assert!(gpu_verdict(&cuda, &older_minor).unwrap_err().contains("supports CUDA 12.4"));
        let same = NodeGpu { driver_cuda: Some((12, 9)), ..the_5090.clone() };
        assert!(gpu_verdict(&cuda, &same).is_ok());
        let old_gpu = NodeGpu { lowest_compute_cap: Some(75), ..the_5090.clone() };
        assert!(gpu_verdict(&cuda, &old_gpu).unwrap_err().contains("a GPU at 75"));
        assert!(gpu_verdict(&cuda, &NodeGpu::default()).is_err(), "unreadable is refused");
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

    // what this catches (IntelMac, 2026-10-06): a node that waits forever on the newest tip
    // while an older build is published. Core 5b496e307 ran from 12:30Z while 8de501e35's
    // core had been out since 15:18Z: 5dbfadbca was building and e428cbd50 queued, and the
    // consumer only ever waited on the tip. The newest PUBLISHED candidate is the deploy.
    #[test]
    fn the_newest_published_build_is_deployed_not_only_the_tip() {
        let candidates: Vec<String> =
            ["e428cbd50", "5dbfadbca", "8de501e35"].map(String::from).to_vec();
        assert_eq!(newest_published(&candidates, &[false, false, true]), Some("8de501e35"));
        assert_eq!(
            newest_published(&candidates, &[false, true, true]),
            Some("5dbfadbca"),
            "the newest out wins"
        );
        assert_eq!(
            newest_published(&candidates, &[true, false, true]),
            Some("e428cbd50"),
            "the tip, when it is out"
        );
        assert_eq!(newest_published(&candidates, &[false, false, false]), None, "none out: wait");
        let args = candidate_keys_log_args("5b496e307", "e428cbd50");
        assert!(args.contains(&"5b496e307..e428cbd50".to_string()), "newer than the running core only");
        assert!(args.contains(&format!("-{DEPLOY_CANDIDATE_LIMIT}")));
        assert!(
            args.iter().skip_while(|a| *a != "--").any(|a| a == "core/"),
            "build inputs only"
        );
    }
}
