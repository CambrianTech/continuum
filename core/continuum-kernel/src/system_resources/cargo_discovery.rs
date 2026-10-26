//! Cargo build trees under the continuum cache, found by what they ARE, never by their name.
//!
//! The eviction owner knew the build caches by name (`cargo-target`, `cargo-target-wt`,
//! `cargo-target-airc`), and agents name their own per session through `CARGO_TARGET_DIR`. On the
//! M5 (2026-10-26) `cache/wt-target` (56 GB), `cache/cargo-target-keep` (22 GB),
//! `cache/airc-target` (12 GB) and a 22 GB `cache/cargo-target-appendonly` sat outside it, the
//! disk passed 95%, and every KV page save was refused from 23:12Z: 158 turns went cold. Any
//! directory directly under the cache that cargo wrote (its cachedir signature beside
//! `.rustc_info.json`, [`is_cargo_target`]) is a build cache whatever its name, and is
//! registered with the same owner at boot.

use std::path::{Path, PathBuf};

use super::citizen_workspace_pool::cargo_target::is_cargo_target;

/// Every directory directly under `cache` that cargo wrote, sorted. An unreadable cache is no
/// trees: discovery only ever ADDS what the owner may evict, and the named classes stand.
pub fn cargo_targets_under(cache: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && is_cargo_target(path))
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cargo_tree(at: &Path) {
        std::fs::create_dir_all(at).expect("tree");
        std::fs::write(at.join("CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55\n").expect("tag");
        std::fs::write(at.join(".rustc_info.json"), "{}").expect("rustc info");
    }

    // what this catches: a build cache named by an agent's session (`wt-target`) staying invisible
    // to the owner, while a generic cache with only the cachedir signature, and a plain directory,
    // are never taken for one.
    #[test]
    fn a_cargo_tree_is_found_by_what_it_is_not_its_name() {
        let cache = tempfile::tempdir().expect("cache");
        cargo_tree(&cache.path().join("wt-target"));
        cargo_tree(&cache.path().join("cargo-target-keep"));
        std::fs::create_dir_all(cache.path().join("kv-pages")).expect("pages");
        let other = cache.path().join("other-cache");
        std::fs::create_dir_all(&other).expect("other");
        std::fs::write(other.join("CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55\n").expect("tag");
        assert_eq!(
            cargo_targets_under(cache.path()),
            vec![cache.path().join("cargo-target-keep"), cache.path().join("wt-target")]
        );
        assert!(cargo_targets_under(&cache.path().join("absent")).is_empty());
    }
}
