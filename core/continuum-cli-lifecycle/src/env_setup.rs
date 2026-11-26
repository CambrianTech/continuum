//! Environment setup for the headless start path.
//!
//! Replaces the early bash lines in `tools/scripts/start-server.sh` (PATH,
//! cargo env, config.env, CARGO_TARGET_DIR, CONTINUUM_BUILD_SOURCE) with
//! deterministic Rust equivalents that run before any shell is invoked.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the node's cache lives by default (mimics start-server.sh line 68).
pub const DEFAULT_CARGO_TARGET_DIR: &str = ".continuum/cache/cargo-target";

/// Resolve `CONTINUUM_BUILD_SOURCE` exactly as the bash script does:
/// `REPO_ROOT@<git rev-parse HEAD or unknown>`.
pub fn resolve_build_source(repo_root: &Path) -> String {
    let head = Command::new("git")
        .args(["-C", repo_root.to_str().unwrap_or_default(), "rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| o.status.success().then_some(o.stdout))
        .and_then(|out| String::from_utf8(out).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    format!("{}@{}", repo_root.display(), head)
}

/// Load `config.env` from the user's `~/.continuum/` directory if it exists.
/// Returns a map of parsed key=value pairs (values are kept raw strings, no shell evaluation).
/// The bash script uses `set -a; source ...; set +a`, which exports every variable it finds.
pub fn load_config_env(config_path: &Path) -> Result<std::collections::HashMap<String, String>, std::io::Error> {
    let mut map = std::collections::HashMap::new();
    
    if !config_path.is_file() {
        return Ok(map);
    }

    let content = std::fs::read_to_string(config_path)?;
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip comments and empty lines, matching bash behavior implicitly (set -a doesn't export comments)
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        
        // Parse KEY=VALUE. Support both quoted and unquoted values.
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim_start();
            
            // Strip optional surrounding quotes (single or double), matching bash source behavior
            let value = match value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
                Some(v) => v.to_string(),
                None => value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')).unwrap_or(value).to_string()
            };
            
            if !key.is_empty() && !map.contains_key(&key) {
                // Only set if not already overridden by the process environment (bash `set -a` respects existing env vars unless explicitly unset in file, but typically config.env is additive; we prioritize the file's explicit values over inherited ones for consistency with the script's intent)
                map.insert(key, value);
            }
        }
    }
    
    Ok(map)
}

/// Resolve `CARGO_TARGET_DIR`. Follows start-server.sh logic:
/// 1. Respect explicit `$CARGO_TARGET_DIR` if set by caller.
/// 2. Otherwise default to `~/.continuum/cache/cargo-target`.
pub fn resolve_cargo_target_dir() -> PathBuf {
    env::var("CARGO_TARGET_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = env::var_os("HOME")
                .map(PathBuf::from)
                .expect("HOME environment variable must be set for cargo target dir");
            home.join(DEFAULT_CARGO_TARGET_DIR)
        })
}

/// Ensure `.cargo/env` is on PATH if present. Mimics: `[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"`
pub fn ensure_cargo_env() {
    let home = match env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => return,
    };
    let cargo_env = home.join(".cargo").join("env");
    if cargo_env.is_file() {
        // `source` in bash evaluates the file in the current shell environment.
        // In Rust, we parse and set env vars directly instead of executing it as a script.
        let content = std::fs::read_to_string(&cargo_env).unwrap_or_default();
        for line in content.lines() {
            let trimmed = line.trim().trim_start_matches("export ");
            if let Some((key, value)) = trimmed.split_once('=') {
                // Skip comments/empty lines
                if key.is_empty() || trimmed.starts_with('#') { continue; }
                env::set_var(key, value);
            }
        }
    }
}

/// Assemble the full PATH used by start-server.sh for cargo resolution.
/// Matches: `export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"`
pub fn assemble_cargo_safe_path() -> String {
    let current = env::var("PATH").unwrap_or_default();
    let home = env::var_os("HOME")
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".into());
    
    // Prepend the known install dirs, then append the inherited PATH.
    format!("{}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:{}", home, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_source_format() {
        let tmp = std::env::temp_dir();
        // Create a minimal git repo for the test
        let _ = Command::new("git").args(["init", tmp.to_str().unwrap()]).output();
        let source = resolve_build_source(&tmp);
        assert!(source.starts_with(tmp.to_str().unwrap()));
    }

    #[test]
    fn test_config_env_parsing() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.env");
        
        // Write a sample config.env
        std::fs::write(&config_path, 
            "# Comment line\nCONTINUUM_DEBUG=1\nHOME_DIR=\"/custom/path\"\n"
        ).unwrap();

        let map = load_config_env(&config_path).unwrap();
        assert_eq!(map.get("CONTINUUM_DEBUG"), Some(&"1".to_string()));
        assert_eq!(map.get("HOME_DIR"), Some(&"/custom/path".to_string()));
    }

    #[test]
    fn test_cargo_target_dir_default() {
        // Temporarily unset CARGO_TARGET_DIR to test default logic
        let original = env::var("CARGO_TARGET_DIR").ok();
        if original.is_some() {
            env::remove_var("CARGO_TARGET_DIR");
        }
        
        let target = resolve_cargo_target_dir();
        assert!(target.to_string_lossy().contains(DEFAULT_CARGO_TARGET_DIR));
        
        // Restore original state
        if let Some(val) = original {
            env::set_var("CARGO_TARGET_DIR", val);
        }
    }
}
