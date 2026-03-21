//! PersonaAllocator — single source of truth for persona creation decisions.
//!
//! Input:  GPU stats from GpuMemoryManager, list of available API key env var names
//! Output: Vec<PersonaAllocation> — which personas to create, with what model and VRAM budget
//!
//! This replaces the TS-side getAvailablePersonas() + detectGpu() + selectLocalModel().
//! Rust owns the decision; TypeScript calls `persona/allocate` IPC and uses the result.
//!
//! Allocation strategy (inference budget = 75% of total VRAM):
//!   32GB+ CUDA (5090):      CodeReview(14B/9GB) + Teacher(8B/5GB) + Helper(3B/3GB) + Local(3B/3GB)
//!   16-31GB Metal (M1 Pro):  Teacher(8B/5GB) + Helper(3B/3GB) + Local(3B/3GB)
//!   8-15GB (MacBook Air):    Helper(3B/3GB)
//!   <8GB / CPU:              Helper(3B/3GB, CPU mode)
//!   + per cloud API key:     One persona per key (0GB VRAM)

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::GpuMemoryManager;

// =============================================================================
// CATALOG TYPES
// =============================================================================

/// A persona definition from the catalog (data, not code).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaCatalogEntry {
    pub unique_id: String,
    pub display_name: String,
    pub provider: String,
    #[serde(rename = "type")]
    pub persona_type: String, // "agent" | "persona"
    #[serde(default)]
    pub voice_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub is_audio_native: bool,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub min_vram_gb: Option<f64>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub speciality: Option<String>,
    #[serde(default)]
    pub accent_color: Option<String>,
}

// =============================================================================
// ALLOCATION RESULT (exported to TypeScript via ts-rs)
// =============================================================================

/// A single persona allocation decision.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/PersonaAllocation.ts")]
#[serde(rename_all = "camelCase")]
pub struct PersonaAllocation {
    pub unique_id: String,
    pub display_name: String,
    pub provider: String,
    #[ts(type = "'agent' | 'persona'")]
    pub persona_type: String,
    #[ts(optional)]
    pub voice_id: Option<String>,
    #[ts(optional)]
    pub model_id: Option<String>,
    pub is_audio_native: bool,
    #[ts(optional)]
    pub api_key_env: Option<String>,
    /// VRAM allocated to this persona in GB (0 for cloud personas)
    #[ts(type = "number")]
    pub vram_budget_gb: f64,
    /// The local model to use (resolved from VRAM budget)
    #[ts(optional)]
    pub resolved_model: Option<String>,
    /// Why this persona was included/excluded
    pub reason: String,
    // Profile data
    #[ts(optional)]
    pub bio: Option<String>,
    #[ts(optional)]
    pub speciality: Option<String>,
    #[ts(optional)]
    pub accent_color: Option<String>,
}

/// Full allocation result — personas + diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/AllocationResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct AllocationResult {
    pub allocations: Vec<PersonaAllocation>,
    pub skipped: Vec<PersonaAllocation>,
    pub summary: Vec<String>,
    pub gpu_name: String,
    #[ts(type = "number")]
    pub total_vram_gb: f64,
    #[ts(type = "'cuda' | 'metal' | 'cpu'")]
    pub gpu_type: String,
    /// The recommended local model for this hardware
    pub local_model: String,
}

// =============================================================================
// ALLOCATOR
// =============================================================================

/// System reserve in GB for Bevy renderer, TTS, etc.
const SYSTEM_RESERVE_GB: f64 = 2.0;

/// Select the best local model given total VRAM.
pub fn select_local_model(vram_gb: f64) -> &'static str {
    if vram_gb >= 32.0 {
        "coder" // 14B compacted
    } else if vram_gb >= 16.0 {
        "unsloth/Llama-3.1-8B-Instruct"
    } else {
        "unsloth/Llama-3.2-3B-Instruct"
    }
}

/// Detect GPU type from the manager's device name.
fn detect_gpu_type(gpu_name: &str) -> &'static str {
    let lower = gpu_name.to_lowercase();
    if lower.contains("nvidia") || lower.contains("geforce") || lower.contains("rtx") || lower.contains("cuda") {
        "cuda"
    } else if lower.contains("apple") || lower.contains("metal") {
        "metal"
    } else if lower == "cpu" || lower.contains("cpu fallback") {
        "cpu"
    } else {
        // Unknown GPU — assume metal on macOS, cuda elsewhere
        #[cfg(target_os = "macos")]
        { "metal" }
        #[cfg(not(target_os = "macos"))]
        { "cuda" }
    }
}

/// Allocate personas based on hardware capabilities and available API keys.
///
/// `available_api_keys`: list of env var names that are currently set (e.g., ["ANTHROPIC_API_KEY", "DEEPSEEK_API_KEY"])
pub fn allocate(
    gpu_manager: &GpuMemoryManager,
    available_api_keys: &[String],
    catalog: &[PersonaCatalogEntry],
) -> AllocationResult {
    let total_vram_bytes = gpu_manager.total_vram_bytes();
    let total_vram_gb = total_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let gpu_name = gpu_manager.gpu_name().to_string();
    let gpu_type = detect_gpu_type(&gpu_name).to_string();
    let local_model = select_local_model(total_vram_gb).to_string();

    let usable_vram_gb = (total_vram_gb - SYSTEM_RESERVE_GB).max(0.0);
    let mut vram_allocated_gb: f64 = 0.0;

    let mut allocations = Vec::new();
    let mut skipped = Vec::new();
    let mut summary = Vec::new();

    summary.push(format!(
        "{}: {:.0}GB {} ({:.0}GB usable after {:.0}GB system reserve)",
        gpu_name,
        total_vram_gb,
        gpu_type.to_uppercase(),
        usable_vram_gb,
        SYSTEM_RESERVE_GB
    ));

    let has_api_key = |env_var: &str| -> bool {
        available_api_keys.iter().any(|k| k == env_var)
    };

    let mut any_candle_allocated = false;

    for entry in catalog {
        let mut allocation = PersonaAllocation {
            unique_id: entry.unique_id.clone(),
            display_name: entry.display_name.clone(),
            provider: entry.provider.clone(),
            persona_type: entry.persona_type.clone(),
            voice_id: entry.voice_id.clone(),
            model_id: entry.model_id.clone(),
            is_audio_native: entry.is_audio_native,
            api_key_env: entry.api_key_env.clone(),
            vram_budget_gb: 0.0,
            resolved_model: None,
            reason: String::new(),
            bio: entry.bio.clone(),
            speciality: entry.speciality.clone(),
            accent_color: entry.accent_color.clone(),
        };

        // Sentinel: special case — needs SENTINEL_PATH env var
        if entry.provider == "sentinel" {
            if has_api_key("SENTINEL_PATH") {
                allocation.reason = "SENTINEL_PATH set".to_string();
                allocations.push(allocation);
            } else {
                allocation.reason = "SENTINEL_PATH not set".to_string();
                skipped.push(allocation);
            }
            continue;
        }

        // Local candle inference: check VRAM budget
        if entry.provider == "candle" {
            let needed_gb = entry.min_vram_gb.unwrap_or(4.0);

            if vram_allocated_gb + needed_gb <= usable_vram_gb {
                allocation.vram_budget_gb = needed_gb;
                allocation.resolved_model = Some(resolve_model_for_persona(
                    entry,
                    total_vram_gb,
                    &local_model,
                ));
                allocation.reason = format!("{:.0}GB VRAM allocated", needed_gb);
                vram_allocated_gb += needed_gb;
                any_candle_allocated = true;
                allocations.push(allocation);
            } else if !any_candle_allocated && usable_vram_gb <= 0.0 {
                // No GPU at all — create ONE local persona for CPU fallback mode
                allocation.vram_budget_gb = 0.0;
                allocation.resolved_model = Some("unsloth/Llama-3.2-3B-Instruct".to_string());
                allocation.reason = "CPU fallback mode (no GPU)".to_string();
                any_candle_allocated = true;
                allocations.push(allocation);
                summary.push(format!("{}: CPU fallback mode (no GPU)", entry.display_name));
            } else {
                allocation.reason = format!(
                    "needs {:.0}GB VRAM, {:.0}GB left",
                    needed_gb,
                    usable_vram_gb - vram_allocated_gb
                );
                skipped.push(allocation);
            }
            continue;
        }

        // Cloud providers: check API key
        if let Some(ref api_key_env) = entry.api_key_env {
            if has_api_key(api_key_env) {
                allocation.reason = format!("{} configured", api_key_env);
                allocations.push(allocation);
            } else {
                allocation.reason = format!("{} not set", api_key_env);
                skipped.push(allocation);
            }
            continue;
        }

        // No requirements — always include
        allocation.reason = "no requirements".to_string();
        allocations.push(allocation);
    }

    if !skipped.is_empty() {
        let skipped_names: Vec<String> = skipped
            .iter()
            .map(|s| format!("{} ({})", s.display_name, s.reason))
            .collect();
        summary.push(format!("Skipped {} personas: {}", skipped.len(), skipped_names.join(", ")));
    }
    summary.push(format!("Creating {} personas", allocations.len()));
    summary.push(format!("Local inference model: {}", local_model));

    AllocationResult {
        allocations,
        skipped,
        summary,
        gpu_name,
        total_vram_gb,
        gpu_type,
        local_model,
    }
}

/// Resolve the best model for a specific persona based on its config and available VRAM.
fn resolve_model_for_persona(
    entry: &PersonaCatalogEntry,
    total_vram_gb: f64,
    default_local_model: &str,
) -> String {
    // If persona has explicit model_id, use it (unless it's 'coder' which is a size class)
    if let Some(ref model_id) = entry.model_id {
        if model_id == "coder" && total_vram_gb >= 32.0 {
            return "coder".to_string();
        }
        if model_id != "coder" {
            return model_id.clone();
        }
    }

    // Fall back to the default model for this VRAM tier
    default_local_model.to_string()
}

/// Load the persona catalog from the embedded JSON.
pub fn load_catalog() -> Vec<PersonaCatalogEntry> {
    let json = include_str!("catalog.json");
    serde_json::from_str(json).expect("Failed to parse persona catalog.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_gpu_manager() -> Arc<GpuMemoryManager> {
        Arc::new(GpuMemoryManager::detect())
    }

    #[test]
    fn test_select_local_model() {
        assert_eq!(select_local_model(32.0), "coder");
        assert_eq!(select_local_model(16.0), "unsloth/Llama-3.1-8B-Instruct");
        assert_eq!(select_local_model(8.0), "unsloth/Llama-3.2-3B-Instruct");
        assert_eq!(select_local_model(4.0), "unsloth/Llama-3.2-3B-Instruct");
    }

    #[test]
    fn test_detect_gpu_type() {
        assert_eq!(detect_gpu_type("NVIDIA GeForce RTX 5090"), "cuda");
        assert_eq!(detect_gpu_type("Apple M3 Max"), "metal");
        assert_eq!(detect_gpu_type("CPU"), "cpu");
    }

    #[test]
    fn test_load_catalog() {
        let catalog = load_catalog();
        assert!(!catalog.is_empty(), "Catalog should not be empty");
        // Verify some expected entries
        assert!(catalog.iter().any(|e| e.unique_id == "helper"), "Should have helper persona");
    }

    #[test]
    fn test_allocate_no_keys() {
        let manager = test_gpu_manager();
        let catalog = load_catalog();
        let result = allocate(&manager, &[], &catalog);

        // Should always create at least one candle persona (CPU fallback)
        let candle_count = result.allocations.iter().filter(|a| a.provider == "candle").count();
        assert!(candle_count >= 1, "Should create at least one local persona");

        // No cloud personas without API keys
        let cloud_count = result.allocations.iter().filter(|a| a.api_key_env.is_some() && a.provider != "candle").count();
        assert_eq!(cloud_count, 0, "Should not create cloud personas without keys");
    }

    #[test]
    fn test_allocate_with_anthropic_key() {
        let manager = test_gpu_manager();
        let catalog = load_catalog();
        let keys = vec!["ANTHROPIC_API_KEY".to_string()];
        let result = allocate(&manager, &keys, &catalog);

        let anthropic_count = result.allocations.iter().filter(|a| {
            a.api_key_env.as_deref() == Some("ANTHROPIC_API_KEY")
        }).count();
        assert!(anthropic_count >= 1, "Should create at least one Anthropic persona");
    }
}
