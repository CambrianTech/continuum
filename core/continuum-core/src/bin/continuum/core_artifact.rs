//! Resolving the deployable core artifact and the runtime library environment of its launch.

use super::*;

/// Resolve the deployable `continuum-core-server` artifact — the SAME order
/// `tools/scripts/install-service.sh::resolve_core_bin` uses, centralized so the CLI and the
/// service installer can never disagree: `CONTINUUM_CORE_BIN` env → installed locations
/// (`/usr/local/bin`, `~/.continuum/bin`) → cargo target dir (release, then debug; default
/// target dir `~/.continuum/cache/cargo-target`, matching start-server.sh). NEVER "next to
/// the CLI exe" — that guess is what printed "could not locate continuum-core-server" on the
/// 2026-08-01 Windows node while a stale core kept serving.
pub(super) fn resolve_core_artifact() -> Result<PathBuf, String> {
    let env_bin = std::env::var("CONTINUUM_CORE_BIN").ok();
    if let Some(explicit) = &env_bin {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("CONTINUUM_CORE_BIN={explicit} is not a file"));
    }
    let target = continuum_core::paths::cargo::target_dir(&std::env::current_dir().map_err(|error| error.to_string())?)?;
    let payload = continuum_core::paths::payload_root(&continuum_core::paths::continuum_home()?)?;
    let candidates = core_artifact_candidates(&target, &payload);
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .ok_or_else(|| {
            format!(
                "no continuum-core-server artifact found. Searched (install-service.sh order): {}. \
                 Build one (`continuum reboot` from the repo, or npm start) or set CONTINUUM_CORE_BIN.",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The ordered candidate list behind [`resolve_core_artifact`] — pure (paths in, paths out)
/// so the shared resolution ORDER is pinned by a unit test against install-service.sh.
pub(super) fn core_artifact_candidates(target: &Path, payload: &Path) -> Vec<PathBuf> {
    let exe = if cfg!(windows) {
        "continuum-core-server.exe"
    } else {
        "continuum-core-server"
    };
    let mut out = Vec::new();
    if !cfg!(windows) {
        out.push(PathBuf::from("/usr/local/bin").join(exe));
    }
    out.push(payload.join("bin").join(exe));
    out.push(target.join("release").join(exe));
    out.push(target.join("debug").join(exe));
    out
}

/// The manifest-declared runtime library dirs that actually EXIST under
/// `root`. Pure over the filesystem so the selection rule is testable without
/// spawning anything.
///
/// Fixed-name tool dirs (`tools/<tool>/bin`), plus the runtime dir of ONE CUDA
/// tree: [`declared_cuda_runtime_dir`]. Every `cuda-*` tree on PATH at once is the
/// multi-tree bug `windows-build-env.sh` fixed on 2026-08-07: the first tree on PATH
/// decides which `cublas64_<major>.dll` the loader and linker bind, whatever
/// CUDA_PATH declares. #4653 re-introduced it here (`cuda-toolkit/bin`, CUDA 12,
/// ahead of `cuda-13.2/Library/bin`), and every deploy's warm build on the 5090 then
/// refused with "CUDA MAJOR MISMATCH" (28 times from 10-01 to 10-04), because the
/// build inherits this PATH.
pub(super) fn runtime_library_dirs(root: &std::path::Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for relative in ["tools/cmake/bin", "tools/llvm/bin", "tools/poppler/Library/bin"] {
        let bin = root.join(relative);
        if bin.is_dir() {
            dirs.push(bin);
        }
    }
    dirs.extend(declared_cuda_runtime_dir(root));
    dirs
}

/// The ONE CUDA tree's runtime dir: the same selection `windows-build-env.sh` makes
/// before a build, so the core runs on the major it was linked against. Over the
/// `cuda-*` trees under `root` that ship a runtime (`cublas64_<major>.dll` in
/// `Library/bin` or `bin`), prefer a linkable one (`cuda.lib` + `curand.lib` in
/// `Library/lib/x64` or `lib/x64`), then the highest major, then the most DLLs, then
/// the name, so two machines with the same trees choose the same one. A node with
/// only a runtime (a prebuilt install, nothing to link) still gets its runtime.
pub(super) fn declared_cuda_runtime_dir(root: &std::path::Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut best: Option<((bool, u32, usize, String), PathBuf)> = None;
    for entry in entries.flatten() {
        let tree = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !name.starts_with("cuda-") || !tree.is_dir() {
            continue;
        }
        let linkable = ["Library/lib/x64", "lib/x64"]
            .iter()
            .any(|sub| tree.join(sub).join("cuda.lib").is_file() && tree.join(sub).join("curand.lib").is_file());
        let runtime = ["Library/bin", "bin"].iter().find_map(|sub| {
            let dir = tree.join(sub);
            let major = std::fs::read_dir(&dir).ok()?.flatten().find_map(|f| {
                f.file_name()
                    .to_str()?
                    .strip_prefix("cublas64_")?
                    .strip_suffix(".dll")?
                    .parse::<u32>()
                    .ok()
            })?;
            Some((dir, major))
        });
        let Some((dir, major)) = runtime else {
            continue; // no runtime: nothing here a process can load
        };
        let dlls = std::fs::read_dir(&dir)
            .map(|d| d.flatten().filter(|f| f.path().extension().is_some_and(|x| x == "dll")).count())
            .unwrap_or(0); // unwrap_or: just listed; an unreadable dir ranks as having none
        let rank = (linkable, major, dlls, name);
        if best.as_ref().is_none_or(|(b, _)| rank > *b) {
            best = Some((rank, dir));
        }
    }
    best.map(|(_, dir)| dir)
}

/// Prepend the manifest-declared runtime library dirs to the child's PATH.
///
/// Only dirs that EXIST are added, so a node without CUDA is unaffected and a
/// node with several CUDA majors contributes each real one. Prepended (not
/// appended) so a provisioned toolchain wins over a stray system copy — the
/// same precedence `windows-build-env.sh` applies for the scripted path.
///
/// A selected payload volume must be available before launching or probing the
/// candidate. It supplies the same libraries in both paths, including cold installs.
pub(super) fn apply_runtime_library_path(cmd: &mut std::process::Command) -> Result<(), String> {
    let home = continuum_core::paths::continuum_home()?;
    let root = continuum_core::paths::payload_root(&home)?;
    apply_runtime_library_env_in(cmd, &root, std::env::consts::OS);
    Ok(())
}

/// The candidate's provenance probe and the actual launch need the same loader
/// environment. Configure only the child; never export into the calling shell.
pub(super) fn apply_core_runtime_env(cmd: &mut std::process::Command) -> Result<(), String> {
    for (k, v) in continuum_base::config_env::read_all() {
        cmd.env(k, v);
    }
    apply_runtime_library_path(cmd)
}

/// Direct launches retain the caller's cwd and the core's positional socket
/// contract. An explicit prebuilt path never goes through artifact discovery.
pub(super) fn direct_core_command(artifact: &Path, socket: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(artifact);
    cmd.arg(socket);
    cmd
}

/// One installed runtime launch contract: the sealed launcher starts media and
/// service-host owns the browser. A developer core override changes only the
/// already-validated core executable, not the companion or engine selection.
#[cfg(any(windows, test))]
pub(super) fn installed_runtime_command(
    shell: &Path,
    release: &continuum_cli_lifecycle::installed_release::Release,
    artifact: &Path,
    socket: &str,
) -> std::process::Command {
    let mut command = std::process::Command::new(shell);
    command.args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-ExecutionPolicy", "RemoteSigned", "-File", &release.launcher,
        "-ExecutablePath", &release.cli, "-CorePath"]);
    command.arg(artifact).args(["-SocketPath", socket, "-EnginePath", &release.engine,
        "-LogDirectory", &release.log_directory]);
    if let Some(root) = &release.eye_root { command.args(["-EyeRoot", root]); }
    command
}

#[cfg(any(windows, test))]
pub(super) fn explicit_prebuilt_command(
    candidate: &PrebuiltCore,
    socket: &str,
    runtime: Option<&continuum_cli_lifecycle::installed_release::Release>,
    shell: &Path,
) -> std::process::Command {
    match runtime {
        Some(release) => installed_runtime_command(shell, release, &candidate.path, socket),
        None => direct_core_command(&candidate.path, socket),
    }
}

/// Read the child's effective environment, including config.env assignments and
/// explicit removals. Looking only at our own environment loses launch overrides.
pub(super) fn command_env(cmd: &std::process::Command, key: &str) -> Option<std::ffi::OsString> {
    match cmd.get_envs().find(|(name, _)| {
        if cfg!(windows) {
            name.as_encoded_bytes().eq_ignore_ascii_case(key.as_bytes())
        } else {
            *name == key
        }
    }) {
        Some((_, value)) => value.map(std::ffi::OsStr::to_os_string),
        None => std::env::var_os(key),
    }
}

/// The installed-library contract also applies to binary-only launches. In
/// particular, Windows otherwise finds System32's older ONNX Runtime and voice
/// initialization panics while the rest of the core reports ready (card d2698cdc).
/// Configure only the child, before spawn; never mutate a running process's env.
pub(super) fn apply_runtime_library_env_in(cmd: &mut std::process::Command, root: &Path, os: &str) {
    if command_env(cmd, "ORT_DYLIB_PATH").is_none_or(|path| path.is_empty()) {
        let library_name = match os {
            "windows" => "onnxruntime.dll",
            "macos" => "libonnxruntime.dylib",
            _ => "libonnxruntime.so",
        };
        let installed = root.join("lib").join(library_name);
        let homebrew = Path::new("/opt/homebrew/lib/libonnxruntime.dylib");
        if installed.is_file() {
            cmd.env("ORT_DYLIB_PATH", installed);
        } else if os == "macos" && homebrew.is_file() {
            cmd.env("ORT_DYLIB_PATH", homebrew);
        } else if os == "windows" {
            eprintln!(
                "⚠ ONNX Runtime not provisioned at {}. Voice initialization may fail if Windows loads its system copy; install the onnxruntime module or set ORT_DYLIB_PATH.",
                installed.display()
            );
        }
    }

    let dirs = runtime_library_dirs(root);
    if dirs.is_empty() {
        return;
    }
    let existing = command_env(cmd, "PATH").unwrap_or_default();
    let joined = std::env::join_paths(dirs.into_iter().chain(std::env::split_paths(&existing)));
    match joined {
        Ok(path) => {
            cmd.env("PATH", path);
        }
        // A PATH entry containing the separator cannot be joined. Leaving PATH
        // untouched is the honest outcome: the child still launches, and on a
        // CUDA node it fails the same loud way it did before this fix rather
        // than silently inheriting a half-built PATH.
        Err(_) => {}
    }
}
