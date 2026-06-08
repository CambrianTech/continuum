//! Local model artifact resolution.
//!
//! The registry owns model identity and artifact hints; this module owns
//! filesystem discovery for those artifacts. Adapters must consume resolved
//! paths from here instead of guessing cache layouts privately.

use super::types::Model;
use std::fs;
use std::path::{Path, PathBuf};

pub fn resolve_model_artifacts(model: &mut Model) {
    model.gguf_local_path = resolve_gguf_for_model(model);
    if let Some(p) = model.mmproj_local_path.take() {
        model.mmproj_local_path = Some(expand_user_path(&p));
    }
}

pub fn resolve_gguf_for_model(model: &Model) -> Option<PathBuf> {
    resolve_gguf(
        &model.id,
        model.gguf_hint.as_deref(),
        model.gguf_local_path.as_deref(),
    )
}

pub fn resolve_gguf_for_model_id(model_id: &str) -> Option<PathBuf> {
    if let Some(registry) = crate::model_registry::try_global() {
        if let Some(model) = registry.model(model_id) {
            return resolve_gguf_for_model(model);
        }
    }
    resolve_gguf(model_id, None, None)
}

pub fn resolve_local_model_dir_for_model_id(model_id: &str) -> Option<PathBuf> {
    resolve_from_local_model_roots(model_id).and_then(|gguf| gguf.parent().map(Path::to_path_buf))
}

pub fn find_first_local_gguf() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for dir in local_model_roots() {
        collect_ggufs_recursive(&dir, &mut candidates);
    }
    if let Some(cache) = huggingface_cache_root() {
        collect_ggufs_recursive(&cache, &mut candidates);
    }
    pick_best_candidate(candidates)
}

pub fn expand_user_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    let home = home_dir_string();
    if let Some(home) = home {
        if let Some(rest) = s.strip_prefix("~/") {
            return PathBuf::from(format!("{home}/{rest}"));
        }
        if s == "~" {
            return PathBuf::from(home);
        }
        if let Some(rest) = s.strip_prefix("$HOME/") {
            return PathBuf::from(format!("{home}/{rest}"));
        }
        if let Some(rest) = s.strip_prefix("%USERPROFILE%/") {
            return PathBuf::from(format!("{home}/{rest}"));
        }
        if let Some(rest) = s.strip_prefix("%USERPROFILE%\\") {
            return PathBuf::from(format!("{home}\\{rest}"));
        }
    }
    p.to_path_buf()
}

fn resolve_gguf(model_id: &str, hint: Option<&str>, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        let expanded = expand_user_path(path);
        if expanded.exists() {
            return Some(expanded);
        }
    }

    if let Some(path) = resolve_from_local_model_roots(model_id) {
        return Some(path);
    }

    if let Some(hint) = hint {
        if let Some(path) = resolve_from_huggingface_hint(hint) {
            return Some(path);
        }
    }

    resolve_from_huggingface_model_id(model_id)
}

fn resolve_from_local_model_roots(model_id: &str) -> Option<PathBuf> {
    for root in local_model_roots() {
        if let Some(dir) = find_model_dir_in_root(model_id, &root) {
            if let Some(gguf) = first_gguf_in_dir(&dir) {
                return Some(gguf);
            }
        }
    }
    None
}

fn local_model_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = home_dir_string() {
        roots.push(
            PathBuf::from(&home)
                .join(".continuum")
                .join("genome")
                .join("models"),
        );
    }
    let storage_models = storage_root().join("genome").join("models");
    if !roots.iter().any(|p| p == &storage_models) {
        roots.push(storage_models);
    }
    roots
}

fn storage_root() -> PathBuf {
    if let Ok(storage) = std::env::var("CONTINUUM_STORAGE_PATH") {
        if !storage.trim().is_empty() {
            return PathBuf::from(storage);
        }
    }
    if let Some(home) = home_dir_string() {
        let config_path = PathBuf::from(&home).join(".continuum").join("config.env");
        if let Ok(content) = fs::read_to_string(config_path) {
            for line in content.lines() {
                if let Some(value) = line.trim().strip_prefix("CONTINUUM_STORAGE_PATH=") {
                    let value = value.trim();
                    if !value.is_empty() {
                        return PathBuf::from(value);
                    }
                }
            }
        }
        return PathBuf::from(home).join(".continuum");
    }
    PathBuf::from("/tmp").join(".continuum")
}

fn find_model_dir_in_root(model_id: &str, root: &Path) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }

    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() || first_gguf_in_dir(&path).is_none() {
            continue;
        }
        let dir_name = path.file_name()?.to_str()?.to_lowercase();
        let model_lower = model_id.to_lowercase();
        if model_lower.contains("qwen")
            && model_lower.contains("compacted")
            && dir_name.contains("qwen")
            && dir_name.contains("compacted")
        {
            let size_match = ["14b", "32b", "7b", "4b", "3b", "1b"]
                .iter()
                .find(|s| model_lower.contains(*s));
            if let Some(size) = size_match {
                if dir_name.contains(size) {
                    return Some(path);
                }
            } else {
                return Some(path);
            }
        }
        if let Some(repo_name) = model_id.split('/').next_back() {
            let repo_lower = repo_name.to_lowercase().replace('.', "");
            if dir_name.contains(&repo_lower) {
                return Some(path);
            }
        }
    }
    None
}

fn resolve_from_huggingface_hint(hint: &str) -> Option<PathBuf> {
    let repo_slug = hf_repo_slug(hint)?;
    let cache = huggingface_cache_root()?;
    let model_dir = find_hf_model_dir(&cache, &repo_slug)?;
    find_ggufs_under_snapshots(&model_dir)
}

fn resolve_from_huggingface_model_id(model_id: &str) -> Option<PathBuf> {
    let cache = huggingface_cache_root()?;
    let wanted = model_id.to_lowercase().replace('/', "--");
    let mut candidates = Vec::new();
    for entry in fs::read_dir(cache).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("models--") && name.contains(&wanted) {
            if let Some(gguf) = find_ggufs_under_snapshots(&entry.path()) {
                candidates.push(gguf);
            }
        }
    }
    pick_best_candidate(candidates)
}

fn hf_repo_slug(hint: &str) -> Option<String> {
    let trimmed = hint
        .strip_prefix("huggingface.co/")
        .unwrap_or(hint)
        .split(':')
        .next()?
        .trim_matches('/');
    let parts: Vec<&str> = trimmed.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    Some(format!(
        "{}--{}",
        parts[parts.len() - 2],
        parts[parts.len() - 1]
    ))
}

fn huggingface_cache_root() -> Option<PathBuf> {
    if let Ok(hf_home) = std::env::var("HF_HOME") {
        if !hf_home.trim().is_empty() {
            return Some(PathBuf::from(hf_home).join("hub"));
        }
    }
    Some(
        PathBuf::from(home_dir_string()?)
            .join(".cache")
            .join("huggingface")
            .join("hub"),
    )
}

fn find_hf_model_dir(cache: &Path, repo_slug: &str) -> Option<PathBuf> {
    let wanted = format!("models--{}", repo_slug).to_lowercase();
    for entry in fs::read_dir(cache).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name == wanted {
            return Some(entry.path());
        }
    }
    None
}

fn find_ggufs_under_snapshots(model_dir: &Path) -> Option<PathBuf> {
    let snapshots = model_dir.join("snapshots");
    let mut candidates = Vec::new();
    for snap in fs::read_dir(snapshots).ok()?.flatten() {
        let Ok(files) = fs::read_dir(snap.path()) else {
            continue;
        };
        for file in files.flatten() {
            let p = file.path();
            if is_gguf(&p) {
                candidates.push(p);
            }
        }
    }
    pick_best_candidate(candidates)
}

fn collect_ggufs_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_ggufs_recursive(&p, out);
        } else if is_gguf(&p) {
            out.push(p);
        }
    }
}

fn first_gguf_in_dir(dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if is_gguf(&p) {
            candidates.push(p);
        }
    }
    pick_best_candidate(candidates)
}

fn pick_best_candidate(mut candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.sort_by(|a, b| {
        let ma = fs::metadata(a).and_then(|m| m.modified()).ok();
        let mb = fs::metadata(b).and_then(|m| m.modified()).ok();
        mb.cmp(&ma).then_with(|| a.cmp(b))
    });
    candidates.into_iter().next()
}

fn is_gguf(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
}

fn home_dir_string() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
}

#[cfg(test)]
pub(crate) fn with_test_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let prior_home = std::env::var("HOME").ok();
    let prior_userprofile = std::env::var("USERPROFILE").ok();
    let prior_hf_home = std::env::var("HF_HOME").ok();
    std::env::set_var("HOME", home);
    std::env::remove_var("USERPROFILE");
    std::env::remove_var("HF_HOME");
    let result = f();
    if let Some(value) = prior_home {
        std::env::set_var("HOME", value);
    } else {
        std::env::remove_var("HOME");
    }
    if let Some(value) = prior_userprofile {
        std::env::set_var("USERPROFILE", value);
    } else {
        std::env::remove_var("USERPROFILE");
    }
    if let Some(value) = prior_hf_home {
        std::env::set_var("HF_HOME", value);
    } else {
        std::env::remove_var("HF_HOME");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::types::{Arch, Capability};
    use std::collections::BTreeSet;

    fn model(id: &str, hint: Option<&str>, explicit: Option<PathBuf>) -> Model {
        Model {
            id: id.to_string(),
            name: None,
            provider: "llamacpp-local".into(),
            arch: Arch::Qwen35,
            context_window: 262144,
            max_output_tokens: 32768,
            tokens_per_second: 33.0,
            capabilities: BTreeSet::from([
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
            ]),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: hint.map(str::to_string),
            gguf_local_path: explicit,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: Default::default(),
            stop_sequences: Vec::new(),
        }
    }

    #[test]
    fn resolves_huggingface_cache_from_hint_when_explicit_path_is_stale() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let cached = home.path().join(
                ".cache/huggingface/hub/models--continuum-ai--qwen3.5-4b-code-forged-GGUF/snapshots/abc",
            );
            fs::create_dir_all(&cached).unwrap();
            let gguf = cached.join("qwen3.5-4b-code-forged-Q4_K_M.gguf");
            fs::write(&gguf, b"gguf").unwrap();

            let resolved = resolve_gguf_for_model(&model(
                "continuum-ai/qwen3.5-4b-code-forged-GGUF",
                Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
                Some(PathBuf::from("~/missing/docker/bundle/model.gguf")),
            ));

            assert_eq!(resolved.as_deref(), Some(gguf.as_path()));
        });
    }

    #[test]
    fn explicit_existing_path_wins() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let explicit = home.path().join("models").join("model.gguf");
            fs::create_dir_all(explicit.parent().unwrap()).unwrap();
            fs::write(&explicit, b"gguf").unwrap();
            let resolved = resolve_gguf_for_model(&model(
                "continuum-ai/qwen3.5-4b-code-forged-GGUF",
                Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
                Some(PathBuf::from("~/models/model.gguf")),
            ));
            assert_eq!(resolved.as_deref(), Some(explicit.as_path()));
        });
    }
}
