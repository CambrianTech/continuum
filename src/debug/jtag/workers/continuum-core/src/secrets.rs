//! Secrets - Load API keys from ~/.continuum/config.env
//!
//! Single source of truth for secrets in Rust, mirrors TypeScript's SecretManager.
//! Reads from:
//! 1. ~/.continuum/config.env (primary)
//! 2. Environment variables (fallback)

use std::collections::HashMap;
use std::fs;
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

        // 1. Load from ~/.continuum/config.env
        if let Some(home) = dirs::home_dir() {
            let config_path = home.join(".continuum").join("config.env");
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            continue;
                        }
                        if let Some((key, value)) = trimmed.split_once('=') {
                            let key = key.trim();
                            let mut value = value.trim().to_string();

                            // Expand tilde
                            if value.starts_with("~/") {
                                if let Some(home) = dirs::home_dir() {
                                    value = home.join(&value[2..]).to_string_lossy().to_string();
                                }
                            }

                            secrets.insert(key.to_string(), value);
                        }
                    }
                }
            }
        }

        // 2. Load from environment variables (override/fallback)
        for (key, value) in std::env::vars() {
            if key.ends_with("_API_KEY") || key.ends_with("_KEY") {
                secrets.insert(key, value);
            }
        }

        Self { secrets }
    }

    /// Get a secret by key
    pub fn get(&self, key: &str) -> Option<&str> {
        self.secrets.get(key).map(|s| s.as_str())
    }

    /// Get a secret, returning error if missing
    pub fn require(&self, key: &str) -> Result<&str, String> {
        self.get(key)
            .ok_or_else(|| format!("Missing required secret: {}. Add it to ~/.continuum/config.env", key))
    }

    /// Check if a secret exists
    pub fn has(&self, key: &str) -> bool {
        self.secrets.contains_key(key)
    }

    /// Get all available keys (for debugging)
    pub fn available_keys(&self) -> Vec<&str> {
        self.secrets.keys().map(|s| s.as_str()).collect()
    }
}

/// Get the global secrets instance
pub fn secrets() -> &'static Secrets {
    SECRETS.get_or_init(Secrets::load)
}

/// Get a secret by key
pub fn get_secret(key: &str) -> Option<&'static str> {
    secrets().get(key)
}

/// Require a secret (returns error if missing)
pub fn require_secret(key: &str) -> Result<&'static str, String> {
    secrets().require(key)
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
