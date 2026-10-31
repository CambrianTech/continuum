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
use std::path::Path;
use std::process::Stdio;

/// Cooperating consumers hold the shared ledger lease for their entire command.
pub const CONSUMER_LEASE_PROTOCOL: u32 = 1;

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
pub const BUILD_INPUTS: [&str; 8] = [
    "core/",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "tools/scripts/lib/",
    "tools/scripts/shared/cargo-features.sh",
    // The Windows build's environment, GPU floor, packaged sidecar script, and pinned CUDA
    // version (Mod-CUDA reads the projection).
    "tools/scripts/start-livekit-windows.ps1",
    "tools/scripts/generated/manifest.windows.ps1",
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

/// How long a consumer waits for CI to publish a tip before reporting a refusal. The
/// workflow's timeout is 150 min; the measured cold builds were 58 min (arm64) and 73 min
/// (x86_64), 2026-10-03. Expiry never authorizes compilation on a user node.
pub const CI_PUBLISH_BUDGET_SECS: u64 = 180 * 60;

/// This node's platform key in CI's matrix, or `None` where no leg publishes yet.
pub fn platform_key(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("macos-arm64"),
        ("macos", "x86_64") => Some("macos-x86_64"),
        // CI's Windows leg (#4834) builds the NVIDIA flavor; a non-NVIDIA Windows node is
        // kept off it by manifest_verdict's feature check, and gpu_verdict refuses a driver
        // or GPU the build cannot run on.
        ("windows", "x86_64") => Some("windows-x86_64"),
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
    /// Optional for older core-only publications. Fresh installs require this complete
    /// companion; its existing engine receipt owns per-file integrity and provenance.
    #[serde(default)]
    pub engine: Option<EngineArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EngineArtifact {
    pub source_revision: String,
    pub backend: String,
    pub relative_path: String,
}

/// Fresh installations need the published serving companion, not a locally built
/// engine or an unrelated binary inherited from a developer's machine.
pub fn fresh_engine_verdict(
    manifest: &ArtifactManifest,
    expected_revision: &str,
) -> Result<(), String> {
    let engine = manifest.engine.as_ref().ok_or(
        "This core-only publication cannot serve a fresh install; publish its engine companion",
    )?;
    if manifest.platform != "windows-x86_64"
        || engine.relative_path != "engine"
        || engine.backend != "cuda"
    {
        return Err("Unsupported published engine companion".into());
    }
    if engine.source_revision.len() != 40
        || !engine
            .source_revision
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || engine.source_revision != expected_revision
    {
        return Err("Published engine revision differs from the checkout".into());
    }
    Ok(())
}

/// Both network updates and first-install transports use the same extraction boundary.
pub fn archive_members_verdict(names: &str, details: &str, platform: &str) -> Result<(), String> {
    let root = format!("continuum-core-{platform}");
    let mut seen = std::collections::BTreeSet::new();
    for name in names.lines() {
        let name = name.trim_end_matches('/');
        if seen.len() >= 10000
            || name.contains(['\\', ':'])
            || (name != root && !name.starts_with(&format!("{root}/")))
            || name
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || !seen.insert(name.to_ascii_lowercase())
        {
            return Err("Archive contains an unsafe or duplicate member".into());
        }
    }
    if seen.is_empty()
        || details.lines().count() != seen.len()
        || details.lines().any(|line| !line.starts_with(['-', 'd']))
    {
        return Err("Archive contains a link or unsupported member type".into());
    }
    Ok(())
}

/// Declared regular-file payload from the already validated GNU/BSD tar listing.
/// Unknown layouts refuse admission; compressed length is never an expansion estimate.
pub fn archive_expanded_bytes(details: &str) -> Result<u64, String> {
    let mut total = 0u64;
    for line in details.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let mode = fields.first().ok_or("empty archive member detail")?;
        if !mode.starts_with(['-', 'd']) { return Err("unsupported archive member type".into()); }
        // GNU: mode owner/group bytes ISO-date time path.
        // BSD: mode links owner group bytes month day time-or-year path.
        let size = if fields.len() >= 6 && fields[1].contains('/') {
            fields[2]
        } else if fields.len() >= 9 && fields[1].parse::<u64>().is_ok() {
            fields[4]
        } else { return Err("unrecognized archive member size layout".into()); };
        let size = size.parse::<u64>().map_err(|_| "invalid archive member size")?;
        if mode.starts_with('-') {
            total = total.checked_add(size).ok_or("archive expanded size overflow")?;
        }
    }
    if details.lines().next().is_none() { return Err("empty archive member details".into()); }
    Ok(total)
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
        Some(minor) => minor
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()?,
        None => 0,
    };
    Some((major, minor))
}

/// PURE: the driver's CUDA version from `nvidia-smi`'s banner ("... CUDA Version: 12.4 |").
pub fn driver_cuda(nvidia_smi_banner: &str) -> Option<(u32, u32)> {
    let after = nvidia_smi_banner.split("CUDA Version:").nth(1)?;
    cuda_major_minor(after.split_whitespace().next()?)
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
    let needed = cuda_major_minor(version).ok_or_else(|| {
        format!("artifact names CUDA `{version}`, which is not a major.minor version")
    })?;
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

/// A measured resource condition, not evidence that a published artifact is bad.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCondition {
    #[default]
    VolumeCapacity,
    /// The filesystem refused space/quota while volume admission passed. Only an
    /// explicit evidence-bound retry can clear this; a free-space sample cannot.
    UnmeasuredFilesystem,
    /// Installer observation or receipt uncertainty; only explicit retry clears it.
    OperationalUncertainty,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResourceDeferral {
    pub condition: ResourceCondition,
    pub path: std::path::PathBuf,
    /// Additional bytes required once the failed invocation has cleaned its staging.
    pub projected_bytes: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PublishedCoreError {
    Artifact(String),
    /// Publication or CI authorization is not established yet. Recheck inside the
    /// existing consumer; returning would spend an actuation before any install.
    SourcePending(String),
    CapabilityDeferred(String),
    OperationalBlocked(String),
    ResourceDeferred(ResourceDeferral),
    /// The node is held for live work a handoff would end, an in-engine training run (card
    /// 9beab2da). Rechecked on the consumer's next pass; it spends no attempt and records no
    /// durable deferral, since the hold releases on its own.
    Held(String),
}
impl From<String> for PublishedCoreError {
    fn from(reason: String) -> Self { Self::Artifact(reason) }
}
impl From<&str> for PublishedCoreError {
    fn from(reason: &str) -> Self { Self::Artifact(reason.into()) }
}
impl std::fmt::Display for PublishedCoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Artifact(reason) | Self::SourcePending(reason) | Self::CapabilityDeferred(reason) | Self::OperationalBlocked(reason) | Self::Held(reason) => f.write_str(reason),
            Self::ResourceDeferred(deferred) => write!(f, "resource admission deferred at {} for {} bytes: {}", deferred.path.display(), deferred.projected_bytes, deferred.reason),
        }
    }
}
impl std::error::Error for PublishedCoreError {}

/// Recheck a deferred candidate before any fetch or extraction. The existing consumer
/// tick owns cadence and request supersession; a still-refused resource spends no attempt.
pub fn recheck_resource_deferral(
    pending: &mut Option<(String, ResourceDeferral)>,
    measured: Result<(), PublishedCoreError>,
) -> Option<MissingArtifact> {
    let (key, condition) = pending.as_mut()?;
    if condition.condition != ResourceCondition::VolumeCapacity {
        return Some(MissingArtifact::AdmissionWait(format!(
            "published build {key} awaits explicit resource retry: {}", condition.reason
        )));
    }
    match measured {
        Ok(()) => { *pending = None; None }
        Err(PublishedCoreError::ResourceDeferred(next)) => {
            let why = format!("published build {key} awaits resource admission: {}", next.reason);
            *condition = next;
            Some(MissingArtifact::AdmissionWait(why))
        }
        Err(PublishedCoreError::Artifact(reason)) => Some(MissingArtifact::Refuse(reason)),
        Err(PublishedCoreError::SourcePending(reason) | PublishedCoreError::CapabilityDeferred(reason) | PublishedCoreError::OperationalBlocked(reason) | PublishedCoreError::Held(reason)) => Some(MissingArtifact::AdmissionWait(reason)),
    }
}

/// The existing consumer's IO boundary. Selection owns the order: publication,
/// exact-revision authorization, then fetch. A source failure never reaches fetch.
#[async_trait::async_trait]
pub trait PublishedCoreSource: Sync {
    type Artifact: Send;
    fn deferred(&self, _key: &str) -> Option<ResourceDeferral> { None }
    async fn published(&self, key: &str) -> Result<bool, String>;
    async fn checks(&self, key: &str) -> Result<crate::deploy_tracker::Checks, String>;
    async fn fetch(&self, key: &str) -> Result<Option<Self::Artifact>, PublishedCoreError>;
}

pub async fn select_published_core<S: PublishedCoreSource>(candidates: &[String], source: &S)
    -> Option<(String, Result<Option<S::Artifact>, PublishedCoreError>)>
{
    use crate::deploy_tracker::Checks;
    let mut blocked = None;
    for key in candidates {
        match source.published(key).await {
            Ok(false) => continue,
            Err(why) => {
                blocked.get_or_insert_with(|| (key.clone(), format!("cannot determine publication for {key}: {why}")));
                continue;
            }
            Ok(true) => {}
        }
        let why = match source.checks(key).await {
            Ok(Checks::Green) => {
                if let Some(deferred) = source.deferred(key) {
                    return Some((key.clone(), Err(PublishedCoreError::ResourceDeferred(deferred))));
                }
                return Some((key.clone(), source.fetch(key).await));
            }
            Ok(checks) => format!("published build {key} is not authorized: {checks:?}"),
            Err(why) => format!("cannot read CI authorization for published build {key}: {why}"),
        };
        blocked.get_or_insert_with(|| (key.clone(), why));
    }
    // Keep looking for an older authorized candidate, but never turn an unreadable
    // source or a known published build into an expired publication deadline.
    blocked.map(|(key, why)| (key, Err(PublishedCoreError::SourcePending(why))))
}

/// What the consumer does when its tip has no artifact yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingArtifact {
    /// CI is still inside its budget: try again next tick, without spending an attempt.
    Wait(String),
    /// A published artifact is blocked by admission, not pending publication.
    AdmissionWait(String),
    /// End this attempt without changing the running installation.
    Refuse(String),
}

/// `platform` is [`platform_key`]'s answer; `tip_age_secs` is how long ago the tip landed.
pub fn when_artifact_missing(platform: Option<&str>, tip_age_secs: u64) -> MissingArtifact {
    match platform {
        None => MissingArtifact::Refuse(
            "CI publishes no build for this platform yet; publish a compatible prebuilt artifact before retrying; running core preserved".into(),
        ),
        Some(p) if tip_age_secs < CI_PUBLISH_BUDGET_SECS => MissingArtifact::Wait(format!(
            "CI has not published {p} for this tip yet ({} of {} min); waiting for it",
            tip_age_secs / 60,
            CI_PUBLISH_BUDGET_SECS / 60
        )),
        Some(p) => MissingArtifact::Refuse(format!(
            "CI published no {p} build within {} min of the tip; repair or publish the prebuilt artifact before retrying; running core preserved",
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

/// What a prebuilt artifact stamped `stamp` must be, for the checkout at `head`: the
/// checkout's build key (CI publishes at key commits), or the stamp itself when the stamp's
/// own build key IS that key. The second case is a core built locally at a commit above the
/// key (a docs- or baseline-only commit): the same build inputs, so the same core. Without
/// it, every local deploy after such a commit refused as a false mismatch (#5210 on the M5,
/// 10-19: stamped 29a474d34, key 9f97bc775; card 98c0ae23).
pub fn prebuilt_expectation(
    cwd: &Path,
    git_dir: Option<&std::ffi::OsStr>,
    head: &str,
    stamp: &str,
) -> (String, &'static str) {
    let checkout = checkout_build_key(cwd, git_dir, head);
    if sha_matches(stamp, checkout.sha()) {
        return (checkout.sha().to_string(), checkout.source());
    }
    match checkout_build_key(cwd, git_dir, stamp) {
        KeyResolution::BuildKey(stamp_key) if sha_matches(&stamp_key, checkout.sha()) => {
            (stamp.to_string(), "built at a commit above this checkout's build key (same build inputs)")
        }
        _ => (checkout.sha().to_string(), checkout.source()),
    }
}

/// How a commit's build key was resolved. Only [`KeyResolution::BuildKey`] is evidence about
/// build inputs; the other two name `head` itself because git could not say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyResolution {
    /// The newest commit at or before `head` touching a [`BUILD_INPUTS`] path.
    BuildKey(String),
    /// Git answered, but no commit at or before `head` touched a build input: `head` itself.
    NoKey(String),
    /// Git could not answer: `head` itself.
    Unreadable(String),
}

impl KeyResolution {
    /// The SHA this resolution names.
    pub fn sha(&self) -> &str {
        match self {
            Self::BuildKey(sha) | Self::NoKey(sha) | Self::Unreadable(sha) => sha,
        }
    }

    /// Where the SHA came from, for receipts; never consulted for a decision.
    pub fn source(&self) -> &'static str {
        match self {
            Self::BuildKey(_) => "build key of this checkout (newest commit touching a build input)",
            Self::NoKey(_) => "git HEAD of this checkout (no build key found)",
            Self::Unreadable(_) => "git HEAD of this checkout (build key unreadable)",
        }
    }
}

/// `head`'s build key in the checkout at `cwd` (the newest commit at or before it touching a
/// [`BUILD_INPUTS`] path); `head` itself, typed as such, when git cannot say.
pub fn checkout_build_key(cwd: &Path, git_dir: Option<&std::ffi::OsStr>, head: &str) -> KeyResolution {
    let args = build_key_log_args(head);
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(cwd).args(&args).env_remove("GIT_DIR").stdin(Stdio::null());
    if let Some(git_dir) = git_dir {
        cmd.env("GIT_DIR", git_dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let key = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if key.is_empty() {
                KeyResolution::NoKey(head.to_string())
            } else {
                KeyResolution::BuildKey(key)
            }
        }
        _ => KeyResolution::Unreadable(head.to_string()),
    }
}

#[cfg(test)]
mod tests;
