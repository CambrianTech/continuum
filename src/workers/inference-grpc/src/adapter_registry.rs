use hf_hub::{api::sync::Api, Repo, RepoType};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
/**
 * Adapter Registry - HuggingFace Hub integration for LoRA adapters
 *
 * Downloads and manages LoRA adapters from HuggingFace Hub:
 * - Automatic caching via hf-hub (uses ~/.cache/huggingface/hub/)
 * - Parses adapter_config.json for metadata
 * - Validates base model compatibility
 *
 * Note: Actual weight parsing is done by lora::load_lora_adapter() which
 * needs device and dtype from the loaded model.
 */
use std::path::PathBuf;

/// Adapter metadata from adapter_config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    /// Base model this adapter was trained on
    #[serde(default)]
    pub base_model_name_or_path: String,

    /// LoRA rank (r)
    #[serde(default)]
    pub r: u32,

    /// LoRA alpha scaling
    #[serde(default)]
    pub lora_alpha: u32,

    /// Target modules (e.g., ["q_proj", "v_proj"])
    #[serde(default)]
    pub target_modules: Vec<String>,

    /// PEFT type (should be "LORA")
    #[serde(default)]
    pub peft_type: String,

    /// Task type (e.g., "CAUSAL_LM")
    #[serde(default)]
    pub task_type: String,

    /// Dropout
    #[serde(default)]
    pub lora_dropout: f64,

    /// Bias handling
    #[serde(default)]
    pub bias: String,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            base_model_name_or_path: String::new(),
            r: 8,
            lora_alpha: 16,
            target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
            peft_type: "LORA".to_string(),
            task_type: "CAUSAL_LM".to_string(),
            lora_dropout: 0.0,
            bias: "none".to_string(),
        }
    }
}

/// Downloaded adapter with metadata (weights not parsed yet)
pub struct DownloadedAdapter {
    /// HuggingFace repo ID (e.g., "username/adapter-name")
    #[allow(dead_code)]
    pub repo_id: String,

    /// Local path to adapter_model.safetensors
    pub weights_path: PathBuf,

    /// Parsed adapter configuration
    pub config: AdapterConfig,
}

/// Download a LoRA adapter from HuggingFace Hub
///
/// # Arguments
/// * `repo_id` - HuggingFace repo ID (e.g., "Jiten1024/llama-3.2-3b-int-finetune-jav-rank-1-alpha-32")
/// * `revision` - Optional revision/branch (default: "main")
///
/// # Returns
/// Downloaded adapter with parsed weights and config
pub fn download_adapter(
    repo_id: &str,
    revision: Option<&str>,
) -> Result<DownloadedAdapter, Box<dyn std::error::Error + Send + Sync>> {
    info!("📥 Downloading adapter from HuggingFace: {repo_id}");

    let api = Api::new()?;
    let repo = api.repo(Repo::with_revision(
        repo_id.to_string(),
        RepoType::Model,
        revision.unwrap_or("main").to_string(),
    ));

    // Download adapter_config.json (optional, some adapters don't have it)
    let config = match repo.get("adapter_config.json") {
        Ok(config_path) => {
            info!("  Found adapter_config.json");
            let config_str = std::fs::read_to_string(&config_path)?;
            serde_json::from_str(&config_str).unwrap_or_else(|e| {
                warn!("  Failed to parse adapter_config.json: {e}");
                AdapterConfig::default()
            })
        }
        Err(_) => {
            info!("  No adapter_config.json, using defaults");
            AdapterConfig::default()
        }
    };

    debug!("  Base model: {}", config.base_model_name_or_path);
    debug!("  Rank: {}, Alpha: {}", config.r, config.lora_alpha);
    debug!("  Target modules: {:?}", config.target_modules);

    // Download adapter weights (to HF cache first)
    let hf_weights_path = repo
        .get("adapter_model.safetensors")
        .map_err(|e| format!("Failed to download adapter_model.safetensors: {e}"))?;

    info!("  Downloaded to HF cache: {hf_weights_path:?}");

    // Copy to our local registry directory (~/.continuum/adapters/)
    let local_path = copy_to_local_registry(repo_id, &hf_weights_path, &config)?;

    info!("  Stored in registry: {local_path:?}");
    info!("  Rank: {}, Alpha: {}", config.r, config.lora_alpha);

    Ok(DownloadedAdapter {
        repo_id: repo_id.to_string(),
        weights_path: local_path,
        config,
    })
}

/// Copy adapter from HF cache to our local registry
fn copy_to_local_registry(
    repo_id: &str,
    hf_path: &std::path::Path,
    config: &AdapterConfig,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    // Create registry directory
    let registry_dir = dirs::home_dir()
        .ok_or("Cannot find home directory")?
        .join(".continuum/adapters/installed");

    // Create adapter subdirectory (sanitize repo_id for filesystem)
    let adapter_dir_name = repo_id.replace('/', "--");
    let adapter_dir = registry_dir.join(&adapter_dir_name);
    std::fs::create_dir_all(&adapter_dir)?;

    // Copy weights file
    let dest_path = adapter_dir.join("adapter_model.safetensors");
    std::fs::copy(hf_path, &dest_path)?;

    // Write manifest.json
    let manifest = serde_json::json!({
        "repo_id": repo_id,
        "base_model": config.base_model_name_or_path,
        "rank": config.r,
        "alpha": config.lora_alpha,
        "target_modules": config.target_modules,
        "peft_type": config.peft_type,
    });
    let manifest_path = adapter_dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    info!("  Created manifest: {manifest_path:?}");

    Ok(dest_path)
}

/// Check if an adapter is compatible with a base model
#[allow(dead_code)]
pub fn check_base_model_compatibility(
    adapter_config: &AdapterConfig,
    loaded_model_id: &str,
) -> Result<(), String> {
    if adapter_config.base_model_name_or_path.is_empty() {
        // No base model specified, allow anyway
        return Ok(());
    }

    // Normalize model IDs for comparison
    let adapter_base = adapter_config.base_model_name_or_path.to_lowercase();
    let loaded = loaded_model_id.to_lowercase();

    // Check for common base model patterns
    // e.g., "unsloth/Llama-3.2-3B-Instruct" should match adapters trained on "meta-llama/Llama-3.2-3B-Instruct"
    let adapter_name = adapter_base.split('/').next_back().unwrap_or(&adapter_base);
    let loaded_name = loaded.split('/').next_back().unwrap_or(&loaded);

    if adapter_name == loaded_name
        || adapter_base.contains(&loaded)
        || loaded.contains(&adapter_base)
    {
        return Ok(());
    }

    // For Llama models, be more lenient about variants
    if (adapter_name.contains("llama") || adapter_name.contains("3.2"))
        && (loaded_name.contains("llama") || loaded_name.contains("3.2"))
    {
        info!("  ⚠ Adapter base model ({}) differs from loaded ({}), but both are Llama variants - allowing",
              adapter_config.base_model_name_or_path, loaded_model_id);
        return Ok(());
    }

    Err(format!(
        "Adapter trained on '{}' but loaded model is '{}'",
        adapter_config.base_model_name_or_path, loaded_model_id
    ))
}

/// List locally cached adapters
///
/// Scans the HuggingFace cache directory for downloaded adapters
#[allow(dead_code)]
pub fn list_cached_adapters() -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let cache_dir = dirs::home_dir()
        .ok_or("Cannot find home directory")?
        .join(".cache/huggingface/hub");

    if !cache_dir.exists() {
        return Ok(vec![]);
    }

    let mut adapters = vec![];

    // HF cache structure: models--owner--repo-name/
    for entry in std::fs::read_dir(&cache_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with("models--") {
            // Check if this has adapter_model.safetensors
            let snapshots = entry.path().join("snapshots");
            if snapshots.exists() {
                for snapshot in std::fs::read_dir(&snapshots)? {
                    let snapshot = snapshot?;
                    if snapshot.path().join("adapter_model.safetensors").exists() {
                        // Convert models--owner--repo to owner/repo
                        let repo_id = name
                            .strip_prefix("models--")
                            .unwrap_or(&name)
                            .replace("--", "/");
                        adapters.push(repo_id);
                        break;
                    }
                }
            }
        }
    }

    Ok(adapters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_model_compatibility() {
        let config = AdapterConfig {
            base_model_name_or_path: "meta-llama/Llama-3.2-3B-Instruct".to_string(),
            ..Default::default()
        };

        // Exact match after org
        assert!(check_base_model_compatibility(&config, "unsloth/Llama-3.2-3B-Instruct").is_ok());

        // Same base model, different org
        assert!(
            check_base_model_compatibility(&config, "meta-llama/Llama-3.2-3B-Instruct").is_ok()
        );

        // Llama variants should be lenient
        assert!(check_base_model_compatibility(&config, "unsloth/Llama-3.2-3B").is_ok());
    }

    #[test]
    fn test_default_adapter_config() {
        let config = AdapterConfig::default();
        assert_eq!(config.r, 8);
        assert_eq!(config.lora_alpha, 16);
        assert_eq!(config.peft_type, "LORA");
    }
}
