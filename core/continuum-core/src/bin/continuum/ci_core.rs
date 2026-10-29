//! CI-published prebuilt core: candidates, fetch, preparation and companions.

use super::*;

pub(super) use continuum_core::modules::deploy_candidate::{deploy_candidates, ci_core_published};

pub(super) struct CiPublishedCore<'a> {
    pub(super) repo: &'a Path,
    pub(super) platform: &'a str,
    pub(super) deferred: &'a std::collections::BTreeMap<String, continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral>,
}

#[async_trait::async_trait]
impl continuum_cli_lifecycle::prebuilt_artifact::PublishedCoreSource for CiPublishedCore<'_> {
    type Artifact = PathBuf;
    fn deferred(&self, key: &str) -> Option<continuum_cli_lifecycle::prebuilt_artifact::ResourceDeferral> {
        self.deferred.get(key).cloned()
    }
    async fn published(&self, key: &str) -> Result<bool, String> {
        let result = ci_core_published(key, self.platform).await;
        if let Err(why) = &result { deploy_note(&format!("deploy-consume: could not ask CI whether {key} is out: {why}")); }
        result
    }
    async fn checks(&self, key: &str) -> Result<continuum_cli_lifecycle::deploy_tracker::Checks, String> {
        let repo = self.repo.to_path_buf();
        let revision = key.to_string();
        let result = tokio::task::spawn_blocking(move || continuum_core::modules::deploy_tracker::revision_checks(&repo, &revision))
            .await.map_err(|e| format!("revision-check task failed: {e}"))?;
        match &result {
            Ok(continuum_cli_lifecycle::deploy_tracker::Checks::Green) => {},
            other => deploy_note(&format!("deploy-consume: published {key} is not authorized: {other:?}")),
        }
        result
    }
    async fn fetch(&self, key: &str) -> Result<Option<PathBuf>, continuum_cli_lifecycle::prebuilt_artifact::PublishedCoreError> {
        fetch_ci_core(self.repo, key, self.platform).await
    }
}
/// What the CI wait resolved to.
pub(super) enum CiCore {
    /// The extracted core, and the build key it is (the tip's, or an older published one).
    Built { core: PathBuf, key: String },
    ResourceBlocked(String),
    Refused(String),
    Superseded(String),
}

/// Beside a CI core: the names of the runtime libraries its manifest bundles, one per line.
pub(super) const RUNTIME_LIBS_FILE: &str = "runtime-libs.txt";

/// What this node's NVIDIA driver and GPUs can run, from `nvidia-smi`. A missing tool or an
/// unreadable answer leaves the field `None`, which refuses a CUDA artifact by name.
pub(super) async fn node_gpu() -> continuum_cli_lifecycle::prebuilt_artifact::NodeGpu {
    use continuum_cli_lifecycle::prebuilt_artifact::{driver_cuda, lowest_compute_cap, NodeGpu};
    let run = |args: &'static [&'static str]| async move {
        let out = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new("nvidia-smi").args(args).output(),
        )
        .await
        .ok()?
        .ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
    };
    NodeGpu {
        driver_cuda: run(&[]).await.as_deref().and_then(driver_cuda),
        lowest_compute_cap: run(&["--query-gpu=compute_cap", "--format=csv,noheader"])
            .await
            .as_deref()
            .and_then(lowest_compute_cap),
    }
}

/// `Ok(None)` when CI has published nothing for `tip` on `platform` (404); `Err` when what it
/// published is not THE build for this node, or does not match its checksum.
pub(super) async fn fetch_ci_core(repo: &Path, tip: &str, platform: &str) -> Result<Option<PathBuf>, continuum_cli_lifecycle::prebuilt_artifact::PublishedCoreError> {
    // A destination already below the shared free-space floor cannot accept any
    // service payload. Refuse before archive I/O; this is only a lower bound,
    // not a replacement for the installer's measured payload admission.
    // Keep offline prepare-prebuilt independent: recovery artifacts can still
    // be prepared on configured cold storage while the service volume is full.
    #[cfg(windows)]
    {
        let home = continuum_core::paths::continuum_home()
            .map_err(PublishedCoreError::OperationalBlocked)?;
        let destination = continuum_core::paths::artifacts::service_destination(&home)?;
        continuum_core::paths::artifacts::check_capacity(&destination, 0).await?;
    }
    prepare_ci_core(repo, tip, platform, None, deploy_note).await
}

pub(super) struct CiArtifactPreparation {
    pub(super) directory: PathBuf,
    pub(super) complete: bool,
}

impl Drop for CiArtifactPreparation {
    fn drop(&mut self) {
        if !self.complete {
            // This invocation created this fresh UUID directory; failed attempts
            // cannot accumulate full archives or remove another caller's candidate.
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

/// The first-install transport supplies a hash-checked archive; the existing consumer
/// performs the same authoritative compatibility and payload preparation as an update.
pub(super) async fn prepare_prebuilt(args: Vec<String>) -> Result<(), String> {
    if args.len() != 2 {
        return Err("prepare-prebuilt requires <checkout> <download-directory>".into());
    }
    let repo = Path::new(&args[0]);
    let head = git_in(repo, &["rev-parse", "HEAD"])?;
    let tip = continuum_cli_lifecycle::prebuilt_artifact::checkout_build_key(repo, None, head.trim()).sha().to_string();
    let platform = continuum_cli_lifecycle::prebuilt_artifact::platform_key(std::env::consts::OS, std::env::consts::ARCH)
        .ok_or("No compatible prebuilt is published for this platform")?;
    let core = prepare_ci_core(repo, &tip, platform, Some(Path::new(&args[1])), prepare_note)
        .await.map_err(|error| error.to_string())?.ok_or("The requested prebuilt is not published")?;
    println!("{}", serde_json::json!({"core": core, "git_sha": tip}));
    Ok(())
}

pub(super) async fn prepare_ci_core(repo: &Path, tip: &str, platform: &str, downloaded: Option<&Path>, note: fn(&str)) -> Result<Option<PathBuf>, continuum_cli_lifecycle::prebuilt_artifact::PublishedCoreError> {
    use continuum_cli_lifecycle::prebuilt_artifact::{gpu_verdict, manifest_url, manifest_verdict, ArtifactManifest};
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use continuum_core::paths::artifacts::{check_capacity, WriteAdmission, write_failure};
    use continuum_cli_lifecycle::prebuilt_artifact::{PublishedCoreError, archive_expanded_bytes};
    let url = manifest_url(tip, platform).ok_or_else(|| format!("{tip} is not a full commit sha"))?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let manifest: ArtifactManifest = if let Some(source) = downloaded {
        let bytes = std::fs::read(source.join(format!("continuum-core-{platform}.json")))
            .map_err(|e| format!("reading downloaded manifest: {e}"))?;
        if bytes.len() > 65536 { return Err("Downloaded manifest exceeds 64 KiB".into()); }
        serde_json::from_slice(&bytes).map_err(|e| format!("reading downloaded manifest: {e}"))?
    } else {
    let resp = client.get(&url).send().await.map_err(|e| format!("fetching {url}: {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    resp
        .error_for_status()
        .map_err(|e| format!("fetching {url}: {e}"))?
        .json()
        .await
        .map_err(|e| format!("reading {url}: {e}"))?
    };
    manifest_verdict(&manifest, tip, platform, &local_core_features(repo)?)?;
    // A CUDA build carries its runtime, and the node's driver and GPUs must be able to run it
    // (card cb587931): read only when the artifact names CUDA, so a Mac never probes.
    if manifest.cuda_version.is_some() {
        gpu_verdict(&manifest, &node_gpu().await)?;
    }
    if let Some(bad) = manifest
        .runtime_libs
        .iter()
        .find(|lib| lib.contains(['/', '\\']) || lib.contains("..") || lib.is_empty())
    {
        return Err(format!("runtime library name `{bad}` is not a plain file name").into());
    }
    if manifest.archive.contains(['/', '\\']) || manifest.archive.contains("..") {
        return Err(format!("archive name `{}` is not a plain file name", manifest.archive).into());
    }
    if downloaded.is_some() {
        let expected = git_in(repo, &["rev-parse", "HEAD:core/vendor/llama.cpp"])?;
        continuum_cli_lifecycle::prebuilt_artifact::fresh_engine_verdict(&manifest, expected.trim())?;
    }
    let root = continuum_core::paths::artifacts::root_under(&continuum_core::paths::continuum_home()?)?;
    check_capacity(&root, 0).await?;
    let dir = root.join(format!("{}-{}", &tip[..12], uuid::Uuid::new_v4()));
    if let Err(error) = std::fs::create_dir_all(&root) { return Err(write_failure(&root, 0, error).await); }
    if let Err(error) = std::fs::create_dir(&dir) { return Err(write_failure(&root, 0, error).await); }
    let mut preparation = CiArtifactPreparation { directory: dir.clone(), complete: false };
    // Stream to disk while hashing; actual response/source lengths, never a
    // historical archive-size estimate, drive capacity admission.
    let archive_url = format!("{}/{}", url.rsplit_once('/').map(|(base, _)| base).unwrap_or(&url), manifest.archive);
    let archive = dir.join(&manifest.archive);
    let mut file = match std::fs::File::create(&archive) { Ok(file) => file, Err(error) => return Err(write_failure(&root, 0, error).await) };
    let mut hasher = Sha256::new();
    let mut compressed_bytes = 0u64;
    let mut admission = WriteAdmission::observe(&root).await?;
    if let Some(source) = downloaded {
        use std::io::Read;
        let mut input = std::fs::File::open(source.join(&manifest.archive)).map_err(|e| format!("reading downloaded archive: {e}"))?;
        let expected = input.metadata().map_err(|e| format!("measuring archive: {e}"))?.len();
        admission.check(expected)?;
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.read(&mut buffer).map_err(|e| format!("reading downloaded archive: {e}"))?;
            if count == 0 { break; }
            compressed_bytes = compressed_bytes.checked_add(count as u64).ok_or("archive size overflow")?;
            admission.before_write(count as u64).await.map_err(|error| match error {
                PublishedCoreError::ResourceDeferred(mut condition) => { condition.projected_bytes = expected.max(compressed_bytes); PublishedCoreError::ResourceDeferred(condition) },
                error => error,
            })?;
            hasher.update(&buffer[..count]);
            if let Err(error) = file.write_all(&buffer[..count]) {
                let mut failure = write_failure(&root, expected.saturating_sub(compressed_bytes).saturating_add(count as u64), error).await;
                if let PublishedCoreError::ResourceDeferred(condition) = &mut failure { condition.projected_bytes = expected.max(compressed_bytes); }
                return Err(failure);
            }
        }
    } else {
        let mut resp = client.get(&archive_url).send().await.and_then(|r| r.error_for_status())
            .map_err(|e| format!("fetching {archive_url}: {e}"))?;
        let announced = resp.content_length();
        if let Some(bytes) = announced { admission.check(bytes)?; }
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("downloading {archive_url}: {e}"))? {
            compressed_bytes = compressed_bytes.checked_add(chunk.len() as u64).ok_or("archive size overflow")?;
            let retry_bytes = announced.unwrap_or(compressed_bytes).max(compressed_bytes);
            admission.before_write(chunk.len() as u64).await.map_err(|error| match error {
                PublishedCoreError::ResourceDeferred(mut condition) => { condition.projected_bytes = retry_bytes; PublishedCoreError::ResourceDeferred(condition) },
                error => error,
            })?;
            hasher.update(&chunk);
            if let Err(error) = file.write_all(&chunk) {
                let mut failure = write_failure(&root, retry_bytes.saturating_sub(compressed_bytes).saturating_add(chunk.len() as u64), error).await;
                if let PublishedCoreError::ResourceDeferred(condition) = &mut failure { condition.projected_bytes = retry_bytes; }
                return Err(failure);
            }
        }
    }
    drop(file);
    let digest = format!("{:x}", hasher.finalize());
    if digest != manifest.sha256.to_ascii_lowercase() {
        return Err(format!("{} has sha256 {digest}, the manifest says {}", manifest.archive, manifest.sha256).into());
    }
    // Extracted beside the archive: `tar_on` runs from its directory, so Git for Windows'
    // GNU tar never reads the drive letter as a host (Fable on #4834).
    let members = continuum_base::shell_portable::tar_on(&archive, "-tzf")?.output().map_err(|e| format!("listing archive: {e}"))?;
    let details = continuum_base::shell_portable::tar_on(&archive, "-tvzf")?.output().map_err(|e| format!("listing archive types: {e}"))?;
    if !members.status.success() || !details.status.success() { return Err("Cannot inspect published archive before extraction".into()); }
    continuum_cli_lifecycle::prebuilt_artifact::archive_members_verdict(&String::from_utf8_lossy(&members.stdout), &String::from_utf8_lossy(&details.stdout), platform)?;
    let expanded = archive_expanded_bytes(&String::from_utf8_lossy(&details.stdout))?;
    // Archive already occupies this volume; extraction is additional. On failure
    // RAII removes it, so the next attempt must admit both coexisting payloads.
    check_capacity(&root, expanded).await.map_err(|error| match error {
        PublishedCoreError::ResourceDeferred(mut deferred) => {
            deferred.projected_bytes = compressed_bytes.saturating_add(expanded);
            PublishedCoreError::ResourceDeferred(deferred)
        }
        error => error,
    })?;
    let status = continuum_base::shell_portable::tar_on(&archive, "-xzf")?
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    if !status.success() {
        // External writers may consume space after admission. Re-sample through
        // the same policy; capacity shortage is not a corrupt published build.
        check_capacity(&root, expanded).await.map_err(|error| match error {
            PublishedCoreError::ResourceDeferred(mut deferred) => {
                deferred.projected_bytes = compressed_bytes.saturating_add(expanded);
                deferred.reason = format!("tar exited {status}; subsequent conservative capacity check: {}", deferred.reason);
                PublishedCoreError::ResourceDeferred(deferred)
            }
            error => error,
        })?;
        return Err(format!("tar could not extract {} (exit {status})", archive.display()).into());
    }
    let _ = std::fs::remove_file(&archive);
    let core = dir.join(format!("continuum-core-{platform}")).join(install_cli::cli_file_name("continuum-core-server"));
    if !core.is_file() {
        return Err(format!("the archive has no {}", core.display()).into());
    }
    // The bundled runtime travels with the core: staging copies what this file lists beside
    // the staged exe (`PreparedCoreService::stage`). A warm local build has no such file.
    if !manifest.runtime_libs.is_empty() {
        for lib in &manifest.runtime_libs {
            if !core.with_file_name(lib).is_file() {
                return Err(format!("the archive lists runtime library {lib} but does not carry it").into());
            }
        }
        let runtime_manifest = manifest.runtime_libs.join("\n");
        check_capacity(&root, runtime_manifest.len() as u64).await.map_err(|mut error| {
            if let PublishedCoreError::ResourceDeferred(condition) = &mut error {
                condition.projected_bytes = compressed_bytes.saturating_add(expanded).saturating_add(runtime_manifest.len() as u64);
            }
            error
        })?;
        if let Err(error) = std::fs::write(core.with_file_name(RUNTIME_LIBS_FILE), &runtime_manifest) {
            let mut failure = write_failure(&root, runtime_manifest.len() as u64, error).await;
            if let PublishedCoreError::ResourceDeferred(condition) = &mut failure { condition.projected_bytes = compressed_bytes.saturating_add(expanded).saturating_add(runtime_manifest.len() as u64); }
            return Err(failure);
        }
    }
    note(&format!("deploy-consume: CI build {tip} for {platform} verified (sha256 {digest})"));
    preparation.complete = true;
    prune_ci_cores(&root, &dir);
    Ok(Some(core))
}

/// What the warm build did besides the core, done for a CI core: the CLI on PATH follows the
/// core, and the engine is rebuilt only when its stamp says the vendored fork moved
/// (`install-llama-server.sh` is a no-op otherwise).
pub(super) async fn install_ci_companions(repo: &Path, core: &Path) -> Result<(), PublishedCoreError> {
    #[cfg(windows)]
    if !core.parent().is_some_and(|dir| dir.join("engine/engine-install.json").is_file()) {
        return Err("Published Windows release lacks its engine companion; no source build will be attempted".into());
    }
    let built_cli = core.with_file_name(install_cli::cli_file_name("continuum"));
    let cli = install_cli::cli_dir(Path::new(&home_dir()?)).join(install_cli::cli_file_name("continuum"));
    let bytes = std::fs::metadata(&built_cli).map_err(|e| PublishedCoreError::OperationalBlocked(format!("cannot measure CLI companion: {e}")))?.len();
    continuum_core::paths::artifacts::check_capacity(&cli, bytes).await?;
    install_cli::copy_with_retry(&built_cli, &cli, Duration::from_secs(10)).map_err(PublishedCoreError::OperationalBlocked)?;
    #[cfg(windows)]
    { let _ = repo; Ok(()) }
    #[cfg(not(windows))]
    {
    let status = std::process::Command::new(locate_bash()?)
        .arg(repo.join("tools/scripts/install-llama-server.sh"))
        .current_dir(repo)
        .status()
        .map_err(|e| format!("install-llama-server.sh: {e}"))?;
    if !status.success() {
        return Err("install-llama-server.sh failed; the engine would not match the core".into());
    }
    Ok(())
    }
}

/// The core feature set this node's HARDWARE runs (with its config.env profile, read fresh:
/// an inherited value is from an older read), from the ONE mapping CI builds with. Not the
/// set it could compile now: a Windows NVIDIA box without MSVC runs CI's CUDA core (e391d449).
pub(super) fn local_core_features(repo: &Path) -> Result<String, String> {
    let out = std::process::Command::new(locate_bash()?)
        .args(["-c", "source tools/scripts/lib/core-features.sh && select_core_features && printf %s \"$CONTINUUM_HARDWARE_FEATURES\""])
        .env_remove("CONTINUUM_NODE_FEATURES")
        .current_dir(repo)
        .output()
        .map_err(|e| format!("core-features.sh: {e}"))?;
    if !out.status.success() {
        return Err(format!("core-features.sh failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Seconds since `tip` was committed (a merge's commit time is the time it landed).
pub(super) fn tip_age_secs(repo: &Path, tip: &str) -> u64 {
    let committed = git_in(repo, &["log", "-1", "--format=%ct", tip])
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    // A clock before 1970 reads as the epoch, so every tip is "new" and the consumer waits.
    let now = continuum_base::time::EpochMs::now().secs();
    committed.map(|c| now.saturating_sub(c)).unwrap_or(0) // unwrap_or: an unreadable commit time waits, it does not compile early
}

/// The writer bounds its own cache (disk_eviction: `core-artifacts`): the newest
/// `KEEP` CI cores stay for rollback, older ones go.
pub(super) fn prune_ci_cores(root: &Path, keep_dir: &Path) {
    const KEEP: usize = 3;
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().is_dir() && e.path() != keep_dir)
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    dirs.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, old) in dirs.into_iter().skip(KEEP - 1) {
        let _ = std::fs::remove_dir_all(&old);
    }
}

pub(super) fn git_head_short_sha() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Unlike the legacy best-effort HEAD lookup, a prebuilt deploy may self-anchor
/// only outside a repository. A broken/inaccessible checkout is not an absence.
pub(super) async fn prebuilt_checkout_sha(
    cwd: &Path,
    git_dir: Option<&std::ffi::OsStr>,
) -> Result<Option<String>, String> {
    let cwd = cwd
        .canonicalize()
        .map_err(|e| format!("cannot inspect prebuilt checkout {}: {e}", cwd.display()))?;
    if git_dir.is_none() && !has_git_checkout(&cwd)? {
        return Ok(None);
    }

    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(&cwd)
        .args(["rev-parse", "--short", "HEAD"])
        .env_remove("GIT_DIR")
        .stdin(Stdio::null());
    if let Some(git_dir) = git_dir {
        cmd.env("GIT_DIR", git_dir);
    }
    // owned (continuum_process::run_owned): a hung git is ended with anything it started
    let output = continuum_process::run_owned(
        tokio::process::Command::from(cmd),
        Some(Duration::from_secs(30)),
        "prebuilt-head-lookup",
        std::sync::Arc::default(),
    )
    .await
    .map_err(|e| format!("cannot read prebuilt checkout HEAD in {}: {e}", cwd.display()))?;
    let status = match output.ended {
        continuum_process::Ended::Exited(status) => status,
        continuum_process::Ended::TimedOut { .. } => {
            return Err(format!("prebuilt checkout HEAD lookup timed out in {}", cwd.display()))
        }
    };
    if let Some(note) = output.capture_note() {
        return Err(format!("cannot verify prebuilt checkout HEAD in {}: {note}", cwd.display()));
    }
    if !status.success() {
        return Err(format!(
            "cannot verify prebuilt checkout HEAD in {}: git exited {}: {}",
            cwd.display(),
            status,
            String::from_utf8_lossy(&output.stderr.bytes).trim(),
        ));
    }
    let head = String::from_utf8(output.stdout.bytes)
        .map_err(|e| format!("prebuilt checkout HEAD is not UTF-8: {e}"))?;
    let head = head.trim();
    if head.is_empty() {
        return Err("prebuilt checkout HEAD lookup returned no SHA".into());
    }
    Ok(Some(head.to_owned()))
}

/// Detect repository metadata without asking Git to read HEAD. Both a normal
/// `.git` directory and a linked worktree/submodule's `.git` file count, even if
/// broken; Git must then verify the selected checkout. Bare repositories count
/// too. Metadata permission errors must not downgrade verification to standalone.
pub(super) fn has_git_checkout(cwd: &Path) -> Result<bool, String> {
    fn present(path: &Path) -> Result<bool, String> {
        match std::fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!(
                "cannot inspect Git metadata {}: {e}",
                path.display()
            )),
        }
    }
    for ancestor in cwd.ancestors() {
        if present(&ancestor.join(".git"))?
            || (present(&ancestor.join("HEAD"))? && present(&ancestor.join("objects"))?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
