//! Portable resolution of a POSIX shell — sibling of [`crate::fs_portable`].
//!
//! ONE definition of "which bash do we exec", because this bug has now been
//! found twice in two call sites that could not see each other.

use std::path::PathBuf;

/// Resolve a bash that is actually a POSIX shell.
///
/// `Command::new("bash")` is WRONG on Windows: PATH lookup finds
/// `C:\Windows\System32\bash.exe`, which is the **WSL launcher**, not a shell.
/// It hands the command to a Linux distro that may not be installed and dies
/// with
///
/// ```text
/// WSL (10 - Relay) ERROR: CreateProcessCommon:800: execvpe(/bin/bash) failed:
/// No such file or directory
/// ```
///
/// Measured twice, in two subsystems that share no code:
///
/// 1. `continuum start` handed the start script to the WSL shim, so the core
///    could never start on Windows, so no governed command was reachable, so
///    every long-running job got hand-rolled instead.
/// 2. `code/shell` — a persona's HANDS. Kimi's execution failed with exactly
///    the error above on the Windows node while `code/read` / `code/search` /
///    `code/tree` all worked, because only execution goes through a shell. She
///    and Sahar were observed cycling through read-and-search for an evening
///    and never reaching execution: not inertness, a severed limb. Glass-boxed
///    from live traffic by M5 2026-08-05.
///
/// The second one is why this lives in the library instead of staying private
/// to the CLI. The first fix was written, documented, tested — and then the
/// identical bug sat unrepaired one directory away, because a private `fn` in
/// `bin/continuum.rs` cannot be reused by anything. Same shape as the
/// `pgrep`/`pkill` pair found the same day. A portability decision belongs in
/// exactly one place or it will be made twice, differently.
///
/// Resolution order: explicit `CONTINUUM_BASH` override → Git-for-Windows
/// locations → a PATH scan that SKIPS the System32 WSL shim. Fails LOUD and
/// names the remedy rather than returning a bash that cannot work — a spawn
/// that resolves to WSL fails deep inside the child with an error the caller
/// cannot interpret.
pub fn locate_bash() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("CONTINUUM_BASH") {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!(
            "CONTINUUM_BASH is set to `{explicit}` but that is not a file"
        ));
    }

    if !cfg!(windows) {
        return Ok(PathBuf::from("bash"));
    }

    Ok(windows_bash_candidates()
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            "no usable bash found. Windows' `bash.exe` on PATH is the WSL launcher, not a \
             POSIX shell. Install Git for Windows (which ships bash), or set CONTINUUM_BASH \
             to a bash.exe."
                .to_string()
        })?)
}

/// The candidate list, in priority order. Split out so the ORDERING and the
/// System32 exclusion are testable without depending on what happens to be
/// installed on the test machine.
fn windows_bash_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for env_key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Ok(root) = std::env::var(env_key) {
            let base = PathBuf::from(root);
            candidates.push(base.join("Git").join("bin").join("bash.exe"));
            candidates.push(
                base.join("Programs")
                    .join("Git")
                    .join("bin")
                    .join("bash.exe"),
            );
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if is_wsl_shim_dir(&dir) {
                continue;
            }
            candidates.push(dir.join("bash.exe"));
        }
    }
    candidates
}

/// Is this PATH entry the System32 directory that hosts the WSL `bash.exe`
/// shim? Taking it is the entire bug this module exists for.
fn is_wsl_shim_dir(dir: &std::path::Path) -> bool {
    dir.to_string_lossy().to_lowercase().contains("system32")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: the ONE decision this module exists to make. If a
    /// System32 entry ever survives into the candidate list, `bash` resolves to
    /// the WSL launcher and every shell exec dies with
    /// `execvpe(/bin/bash) failed` — which broke `continuum start` and, later
    /// and worse, every persona's `code/shell` on Windows.
    #[test]
    fn the_system32_wsl_shim_is_never_a_candidate() {
        assert!(is_wsl_shim_dir(std::path::Path::new(
            r"C:\Windows\System32"
        )));
        // Case must not matter — PATH casing is not normalized on Windows.
        assert!(is_wsl_shim_dir(std::path::Path::new(
            r"c:\windows\system32"
        )));
        assert!(is_wsl_shim_dir(std::path::Path::new(
            r"C:\WINDOWS\SYSTEM32\wbem"
        )));
        // A real shell location must NOT be excluded.
        assert!(!is_wsl_shim_dir(std::path::Path::new(
            r"C:\Program Files\Git\bin"
        )));
        assert!(!is_wsl_shim_dir(std::path::Path::new("/usr/bin")));
    }

    /// what this catches: a bad override resolving to something unusable. An
    /// operator pointing CONTINUUM_BASH at a missing path must be TOLD, not
    /// silently given the WSL shim — silent fallback here reintroduces the bug
    /// under the name of a fix.
    #[test]
    fn a_bogus_explicit_override_fails_loud_and_names_itself() {
        let key = "CONTINUUM_BASH";
        let prior = std::env::var(key).ok();
        std::env::set_var(key, "/definitely/not/a/real/bash-xyzzy");
        let err = locate_bash().expect_err("a non-file override must not resolve");
        assert!(err.contains("CONTINUUM_BASH"), "must name the knob: {err}");
        match prior {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    /// what this catches: on Unix this must stay a plain `bash` on PATH — the
    /// Windows candidate walk must never leak into the platform that was
    /// working fine.
    #[cfg(not(windows))]
    #[test]
    fn unix_resolves_to_plain_bash() {
        let key = "CONTINUUM_BASH";
        let prior = std::env::var(key).ok();
        std::env::remove_var(key);
        assert_eq!(locate_bash().unwrap(), PathBuf::from("bash"));
        if let Some(v) = prior {
            std::env::set_var(key, v);
        }
    }
}
