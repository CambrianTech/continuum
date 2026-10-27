//! Secrets - Load API keys from the continuum home's config.env
//!
//! Single source of truth for secrets in Rust, mirrors TypeScript's SecretManager.
//! Reads from:
//! 1. `config.env` in the continuum home ([`crate::config_env::config_path`]), through the one
//!    parser for that file (primary)
//! 2. Environment variables (fallback)

use std::collections::HashMap;
use std::sync::OnceLock;

static SECRETS: OnceLock<Secrets> = OnceLock::new();

/// Secrets manager for API keys
pub struct Secrets {
    secrets: HashMap<String, String>,
}

impl Secrets {
    /// Initialize and load secrets
    fn load() -> Self {
        let mut secrets = HashMap::new();

        // 1. config.env in the continuum home, read by config_env's parser (one parser for the
        // file; this one used to parse it a second time from a hand-built ~/.continuum).
        if let Some(path) = crate::config_env::config_path() {
            for (key, value) in crate::config_env::read_all_from(&path) {
                secrets.insert(key, normalize_env_value(&expand_tilde(value)));
            }
        }

        // 2. Load from environment variables (override/fallback)
        // Accept API keys, secrets, tokens, and URLs — all patterns used by our adapters
        for (key, value) in std::env::vars() {
            if key.ends_with("_API_KEY")
                || key.ends_with("_KEY")
                || key.ends_with("_API_SECRET")
                || key.ends_with("_SECRET")
                || key.ends_with("_TOKEN")
                || key.ends_with("_URL")
            {
                let value = normalize_env_value(&value);
                if !value.is_empty() {
                    secrets.insert(key, value);
                }
            }
        }

        Self { secrets }
    }

    /// Get a secret by key
    pub fn get(&self, key: &str) -> Option<&str> {
        self.secrets
            .get(key)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
    }

    /// Get a secret, returning error if missing
    pub fn require(&self, key: &str) -> Result<&str, String> {
        self.get(key).ok_or_else(|| {
            format!(
                "Missing required secret: {}. Add it to ~/.continuum/config.env",
                key
            )
        })
    }

    /// Check if a secret exists
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

}

/// A value starting `~/` names a path under the user's home, as the shell would expand it.
fn expand_tilde(value: String) -> String {
    match (value.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => value,
    }
}

fn normalize_env_value(raw: &str) -> String {
    let value = raw.trim();
    let unquoted = if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    };
    unquoted.trim().to_string()
}

/// Get the global secrets instance
pub fn secrets() -> &'static Secrets {
    SECRETS.get_or_init(Secrets::load)
}

/// Get a secret by key
pub fn get_secret(key: &str) -> Option<&'static str> {
    secrets().get(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secrets_load() {
        // Just verify it doesn't panic
        let _ = secrets();
    }
}
