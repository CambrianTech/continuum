//! The child core's launch environment and command shape: `config.env`, the manifest's
//! runtime library dirs (ONE CUDA tree, like `windows-build-env.sh`), the ONNX Runtime
//! selection, and the direct / installed-runtime / explicit-prebuilt command forms.
//! Configured on the child only; never exported into the calling shell.

use std::path::{Path, PathBuf};

#[cfg(any(windows, test))]
use super::prebuilt::PrebuiltCore;

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
    for (k, v) in continuum_core::config_env::read_all() {
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
