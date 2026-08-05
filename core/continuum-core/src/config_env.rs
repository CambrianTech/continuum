//! config_env — read + UPSERT `~/.continuum/config.env` from the Rust core.
//!
//! [`crate::secrets`] is the read-only, cached-at-startup reader (API keys
//! loaded into a map once at boot). This is its mutable counterpart: a fresh
//! single-key read and a portable upsert writer, for runtime commands that
//! change a persisted setting (e.g. `system/launch-mode`).
//!
//! Three runtimes touch this one file — the bash bootstrap (`bin/continuum`
//! `config_set`), the Node desktop (`SecretManager`), and the Rust core (here).
//! They all agree on the format (`KEY=value` lines, `#` comments, blank lines
//! skipped) and the path, so a thin per-runtime accessor is coherence, not
//! duplication: the headless Rust server must be able to read/write the setting
//! with no Node and no shell in the loop.
//!
//! The `*_in` / `*_from` variants take an explicit path and hold ALL the logic
//! so they unit-test against a temp file; the bare [`read`] / [`upsert`] are
//! thin `dirs::home_dir()` wrappers.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Path to `~/.continuum/config.env` (mirrors [`crate::secrets`]).
pub fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("config.env"))
}

/// Read one key's value, fresh from disk. `None` if the file or key is absent.
/// Comments/blank lines skipped; value trimmed; last assignment wins (matching
/// the shell `source` semantics the bootstrap relies on).
pub fn read(key: &str) -> Option<String> {
    read_from(&config_path()?, key)
}

/// Upsert one key, preserving every other line. Creates the dir + file if
/// missing. Replaces the FIRST existing assignment in place (keeps file order),
/// drops any duplicates, appends if absent.
pub fn upsert(key: &str, value: &str) -> Result<(), String> {
    let path = config_path().ok_or("config_env: cannot resolve home dir")?;
    upsert_in(&path, key, value)
}

/// Strip one layer of surrounding single or double quotes from a config value.
///
/// Values are now WRITTEN single-quoted, because this file is `source`d by bash and an unquoted
/// Windows path is destroyed by it: bash treats each backslash as an escape, so
/// `HF_HOME=D:\continuum-cold\huggingface` sources as `D:continuum-coldhuggingface` — a
/// drive-relative path that Windows then resolves to a WHOLE SEPARATE cache root. Measured: a
/// 76 GB model download landing in `D:\continuum-coldhuggingface\` while every resolver looked
/// under `D:\continuum-cold\huggingface\`.
///
/// Reading has to tolerate both forms: installs predating the quoting fix still have bare values,
/// and an operator editing this file by hand may write either.
fn unquote(v: &str) -> String {
    let bytes = v.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if (first == b'\'' && last == b'\'') || (first == b'"' && last == b'"') {
            return v[1..v.len() - 1].to_string();
        }
    }
    v.to_string()
}

/// Path-taking core of [`read`] — testable without touching `$HOME`.
pub fn read_from(path: &Path, key: &str) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let mut found = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim() == key {
                found = Some(unquote(v.trim()));
            }
        }
    }
    found
}

/// Path-taking core of [`upsert`] — testable without touching `$HOME`. Writes
/// through a temp file + rename so a concurrent reader never sees a half-write.
pub fn upsert_in(path: &Path, key: &str, value: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("config_env: create_dir_all {parent:?}: {e}"))?;
    }
    let existing = fs::read_to_string(path).unwrap_or_default();

    let mut out = String::new();
    let mut replaced = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        let is_assignment_for_key = !trimmed.starts_with('#')
            && trimmed
                .split_once('=')
                .map(|(k, _)| k.trim() == key)
                .unwrap_or(false);
        if is_assignment_for_key {
            if !replaced {
                out.push_str(key);
                out.push('=');
                out.push_str(value);
                out.push('\n');
                replaced = true;
            }
            // drop this (and any later duplicate) assignment line
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !replaced {
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }

    let tmp = path.with_extension("env.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("config_env: create {tmp:?}: {e}"))?;
        f.write_all(out.as_bytes())
            .map_err(|e| format!("config_env: write {tmp:?}: {e}"))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("config_env: rename {tmp:?} -> {path:?}: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_config() -> PathBuf {
        // Unique per test via thread id + a counter — no Date/rand needed.
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("continuum-config-env-test-{n}/config.env"))
    }

    /// What this catches: a Windows path written by the installer must survive being read back,
    /// quoted or not. The installer now single-quotes values because this file is `source`d by
    /// bash, which eats backslashes in an unquoted value:
    ///   HF_HOME=D:\continuum-cold\huggingface  ->  D:continuum-coldhuggingface
    /// Windows resolves that drive-relative string into a SEPARATE cache root, and a 76 GB model
    /// download really did land there while every resolver looked at the correct path. The reader
    /// must therefore strip the new quotes AND still accept pre-fix installs that have bare values.
    #[test]
    fn windows_paths_read_back_intact_quoted_or_bare() {
        let p = tmp_config();
        let win = r"D:\continuum-cold\huggingface";
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(
            &p,
            format!("# header\nHF_HOME='{win}'\nCONTINUUM_STORAGE_PATH={win}\nQ=\"{win}\"\n"),
        )
        .unwrap();
        assert_eq!(read_from(&p, "HF_HOME").as_deref(), Some(win), "single-quoted value");
        assert_eq!(
            read_from(&p, "CONTINUUM_STORAGE_PATH").as_deref(),
            Some(win),
            "bare value from a pre-fix install must still work"
        );
        assert_eq!(read_from(&p, "Q").as_deref(), Some(win), "double-quoted value");
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    /// What this catches: unquote() must not eat a lone quote or mangle a value that merely
    /// CONTAINS one — only a matched surrounding pair is a quote wrapper.
    #[test]
    fn unquote_only_strips_matched_surrounding_pairs() {
        assert_eq!(unquote("plain"), "plain");
        assert_eq!(unquote("'quoted'"), "quoted");
        assert_eq!(unquote("\"quoted\""), "quoted");
        assert_eq!(unquote("'mismatched\""), "'mismatched\"");
        assert_eq!(unquote("it's"), "it's");
        assert_eq!(unquote("'"), "'");
        assert_eq!(unquote(""), "");
    }

    /// What this catches: a freshly written key reads back, and a sibling key is
    /// untouched — the get/set round-trip + non-destructiveness the command relies on.
    #[test]
    fn upsert_then_read_roundtrips_and_preserves_siblings() {
        let p = tmp_config();
        upsert_in(&p, "HTTP_PORT", "9000").unwrap();
        upsert_in(&p, "CONTINUUM_LAUNCH_MODE", "headless").unwrap();
        assert_eq!(read_from(&p, "HTTP_PORT").as_deref(), Some("9000"));
        assert_eq!(read_from(&p, "CONTINUUM_LAUNCH_MODE").as_deref(), Some("headless"));
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    /// What this catches: upsert REPLACES (not duplicates) — exactly one line per
    /// key after re-setting, or the shell `source` (last-wins) and our reader
    /// could disagree across runtimes.
    #[test]
    fn upsert_replaces_in_place_no_duplicates() {
        let p = tmp_config();
        upsert_in(&p, "CONTINUUM_LAUNCH_MODE", "ui").unwrap();
        upsert_in(&p, "CONTINUUM_LAUNCH_MODE", "headless").unwrap();
        let content = fs::read_to_string(&p).unwrap();
        let count = content
            .lines()
            .filter(|l| l.trim_start().starts_with("CONTINUUM_LAUNCH_MODE="))
            .count();
        assert_eq!(count, 1, "expected exactly one assignment line, got:\n{content}");
        assert_eq!(read_from(&p, "CONTINUUM_LAUNCH_MODE").as_deref(), Some("headless"));
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    /// What this catches: comments + blank lines survive an upsert (we never
    /// clobber the human-authored config.env scaffold the installer writes).
    #[test]
    fn upsert_preserves_comments_and_blanks() {
        let p = tmp_config();
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "# Continuum Configuration\n\nHTTP_PORT=9000\n").unwrap();
        upsert_in(&p, "CONTINUUM_LAUNCH_MODE", "headless").unwrap();
        let content = fs::read_to_string(&p).unwrap();
        assert!(content.contains("# Continuum Configuration"), "comment dropped:\n{content}");
        assert!(content.contains("HTTP_PORT=9000"), "sibling dropped:\n{content}");
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }

    /// What this catches: missing file/key reads as None (the command defaults to
    /// 'auto'), and comments are not mistaken for assignments.
    #[test]
    fn read_missing_is_none_and_comments_ignored() {
        let p = tmp_config();
        assert_eq!(read_from(&p, "CONTINUUM_LAUNCH_MODE"), None);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "# CONTINUUM_LAUNCH_MODE=ui (this is a comment)\n").unwrap();
        assert_eq!(read_from(&p, "CONTINUUM_LAUNCH_MODE"), None);
        let _ = fs::remove_dir_all(p.parent().unwrap());
    }
}
