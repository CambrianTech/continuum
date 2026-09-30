//! The CLI arm of `continuum install`: the `continuum` and `uu` a human types are
//! the installed release's CLI, on PATH, the same bytes as the slot.
//!
//! Joel, 2026-09-18: *"continuum or uu (alias) need to work from path… must work
//! across os's."* and *"You want users to remember almost nothing."* `uu` is THE
//! short alias (2026-08-01: the double-U of contin-UU-m; `cu` and `co` are taken on
//! Unix). One name on every platform.
//!
//! The seam this closes (#422, the STALE CLI): the CLI on PATH was refreshed only by
//! the installer's last step, so a node whose unattended build kept failing ran a
//! CLI from the morning it was installed — which is what kept the build failing
//! (2026-09-19: the 07:32 `continuum.exe` on the 5090 lacked #4233's fix for a day).
//! `reboot --service` stages the built CLI into the supervisor's slot; this arm
//! makes the PATH copy follow the slot, and says when it doesn't.
//!
//! Read half: byte-hash the slot CLI against each PATH copy, and is the directory on
//! the user's PATH. Write half: copy (with a bounded retry for the file another
//! terminal still has open) and, when missing, add the directory to the USER PATH.
//! Never the machine PATH, never elevated.

use std::path::{Path, PathBuf};

/// The names a human types. On Windows both carry `.exe` so PowerShell and cmd
/// resolve them, not only Git bash (the installer's bare `uu` copy resolves in bash
/// alone). On Unix the second is a symlink to the first.
pub const CLI_NAMES: [&str; 2] = ["continuum", "uu"];

/// Where the CLI lives for a user: `~/.local/bin`, user-writable, conventionally on
/// PATH, the same on every OS.
pub fn cli_dir(home: &Path) -> PathBuf {
    home.join(".local").join("bin")
}

pub fn cli_file_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// One way the PATH copy differs from the slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliDrift {
    /// No `~/.local/bin/<name>` at all.
    Missing(String),
    /// The bytes differ from the slot's CLI: a stale copy.
    Stale(String),
    /// `~/.local/bin` is not on the user's PATH; the names resolve nowhere.
    NotOnPath(PathBuf),
}

pub fn digest_file(path: &Path) -> Result<String, String> {
    use sha2::Digest;
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut h = sha2::Sha256::new();
    h.update(&bytes);
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The drift of the PATH copies from `slot_cli`. `user_path` is the user's PATH
/// variable as stored (semicolon- or colon-separated per OS). Pure.
pub fn cli_drift(slot_cli: &Path, dir: &Path, user_path: &str) -> Result<Vec<CliDrift>, String> {
    let want = digest_file(slot_cli)?;
    let mut out = Vec::new();
    for name in CLI_NAMES {
        let file = dir.join(cli_file_name(name));
        if !file.is_file() {
            out.push(CliDrift::Missing(name.to_string()));
        } else if digest_file(&file)? != want {
            out.push(CliDrift::Stale(name.to_string()));
        }
    }
    if !path_contains(user_path, dir) {
        out.push(CliDrift::NotOnPath(dir.to_path_buf()));
    }
    Ok(out)
}

/// Does a PATH value name `dir`? Trailing separators and case (on Windows) are not a
/// difference; `~` is not expanded because the stored user PATH never carries it.
pub fn path_contains(path_value: &str, dir: &Path) -> bool {
    let norm = |s: &str| {
        let t = s.trim().trim_end_matches(['\\', '/']).replace('/', "\\");
        if cfg!(windows) { t.to_ascii_lowercase() } else { t }
    };
    let want = norm(&dir.to_string_lossy());
    std::env::split_paths(path_value).any(|p| norm(&p.to_string_lossy()) == want)
}

/// Copy `from` over `to`, retrying for `budget` while another terminal holds the old
/// file open (Windows: a running CLI's image is locked; the installer waits the same
/// ten seconds rather than terminating a user's command).
pub fn copy_with_retry(from: &Path, to: &Path, budget: std::time::Duration) -> Result<(), String> {
    let deadline = std::time::Instant::now() + budget;
    loop {
        // Move the live file aside first: a mapped image cannot be overwritten, but it
        // can be renamed, and the running process keeps its mapping.
        if to.exists() {
            let aside = to.with_extension("prev");
            let _ = std::fs::remove_file(&aside);
            if let Err(e) = std::fs::rename(to, &aside) {
                if std::time::Instant::now() >= deadline {
                    return Err(format!("cannot move {} aside after {}s: {e} — let active CLI commands finish and rerun", to.display(), budget.as_secs()));
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
                continue;
            }
        }
        return std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the read half of the CLI arm — a missing name, a stale
    // copy (bytes differ from the slot), and a dir absent from PATH are each named;
    // a converged dir is silent; PATH matching ignores a trailing separator.
    #[test]
    fn cli_drift_names_missing_stale_and_off_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let slot = dir.path().join("slot").join(cli_file_name("continuum"));
        std::fs::create_dir_all(slot.parent().unwrap()).unwrap(); // unwrap: the test's own dir
        std::fs::write(&slot, b"release-bytes").unwrap(); // unwrap: the test's own file
        let bin = dir.path().join("local").join("bin");
        std::fs::create_dir_all(&bin).unwrap(); // unwrap: the test's own dir
        let on_path = format!("{}{}{}", r"C:\other", if cfg!(windows) { ";" } else { ":" }, bin.display());

        let d = cli_drift(&slot, &bin, "").unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert_eq!(
            d,
            vec![CliDrift::Missing("continuum".into()), CliDrift::Missing("uu".into()), CliDrift::NotOnPath(bin.clone())]
        );

        std::fs::write(bin.join(cli_file_name("continuum")), b"release-bytes").unwrap(); // unwrap: the test's own file
        std::fs::write(bin.join(cli_file_name("uu")), b"yesterday").unwrap(); // unwrap: the test's own file
        let d = cli_drift(&slot, &bin, &on_path).unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert_eq!(d, vec![CliDrift::Stale("uu".into())], "a stale alias is named; the dir is on PATH");

        copy_with_retry(&slot, &bin.join(cli_file_name("uu")), std::time::Duration::from_secs(1)).unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert!(cli_drift(&slot, &bin, &on_path).unwrap().is_empty(), "converged is silent"); // unwrap: the valid case — an Err here IS the failure
        assert!(bin.join("uu.prev").is_file() || !cfg!(windows), "the old copy is moved aside, not deleted under a running process");

        let with_slash = format!("{}{}", bin.display(), std::path::MAIN_SEPARATOR);
        assert!(path_contains(&with_slash, &bin), "a trailing separator is not a different dir");
    }
}
