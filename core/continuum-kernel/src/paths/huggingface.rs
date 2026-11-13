//! Where the Hugging Face cache lives on this machine: a path fact, read by the model
//! registry (finding weights) and by disk accounting (tracking the cache's bytes).

use std::path::PathBuf;

/// The user's home as the OS names it: `HOME`, else Windows' `USERPROFILE`.
fn home_dir() -> Option<String> {
    std::env::var("HOME").ok().or_else(|| std::env::var("USERPROFILE").ok())
}

pub fn huggingface_cache_root() -> Option<PathBuf> {
    // Process env wins (a launcher's explicit override), then config.env — the
    // ONE cross-platform source ([[config-env-single-owner]]): Windows persists
    // user env in the registry and Linux has no equivalent, so the cold-storage
    // installer writes HF_HOME to config.env and the core must honor it here
    // (BigMama 16TB routing, 2026-07-24).
    let hf_home = std::env::var("HF_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| continuum_base::config_env::read("HF_HOME").filter(|s| !s.trim().is_empty()));
    if let Some(hf_home) = hf_home {
        return Some(PathBuf::from(hf_home).join("hub"));
    }
    Some(
        PathBuf::from(home_dir()?)
            .join(".cache")
            .join("huggingface")
            .join("hub"),
    )
}
