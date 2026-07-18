//! PersonaAllocator — single source of truth for persona creation decisions.
//!
//! Input:  GPU stats from GpuMemoryManager, list of available API key env var names
//! Output: Vec<PersonaAllocation> — which personas to create, with what model and VRAM budget
//!
//! This replaces the TS-side getAvailablePersonas() + detectGpu() + selectLocalModel().
//! Rust owns the decision; TypeScript calls `persona/allocate` IPC and uses the result.
//!
//! Allocation strategy — per-persona tiered model selection:
//!   32GB+ unified/VRAM:      shared Qwen3.5 text personas + Qwen2-VL vision
//!   16GB+ unified/VRAM:      shared Qwen3.5 text personas, vision when budget allows
//!   <16GB / CPU:             reduced local fleet selected from the same Qwen catalog
//!   + per cloud API key:     One persona per key (0GB VRAM)

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::GpuMemoryManager;

// =============================================================================
// CATALOG TYPES
// =============================================================================

/// Model preference for a specific VRAM tier.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/ModelPreference.ts")]
#[serde(rename_all = "camelCase")]
pub struct ModelPreference {
    /// Minimum total VRAM (GB) for this preference to apply
    #[serde(default)]
    pub min_vram_gb: f64,
    /// Model alias from model_registry.json
    pub model: String,
    /// VRAM budget this persona needs when using this model
    #[serde(default)]
    pub vram_budget_gb: f64,
}

/// A persona definition from the catalog (data, not code).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/persona/PersonaCatalogEntry.ts")]
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
    /// Per-VRAM-tier model preferences (highest minVramGb first in catalog).
    /// The allocator picks the first preference whose minVramGb <= total VRAM.
    #[serde(default)]
    pub model_preferences: Vec<ModelPreference>,
}

impl PersonaCatalogEntry {
    /// Whether this entry's provider is the local in-process inference lane.
    ///
    /// The ONE place the allocator decides "local" from the catalog `provider`
    /// field, so the literal lives here once instead of drifting across the file
    /// (`[[magic-strings-vs-enums]]`, SMELL #70/#73).
    ///
    /// TRAP — do NOT merge this with `crate::cognition::turn_batch::is_local_provider`.
    /// They answer different questions: this is "is the catalog provider field
    /// exactly `local`"; that is the runtime "does this *execute* as a local model"
    /// (which also matches `dmr`, `qwen`, `continuum-ai/`). Collapsing them into one
    /// `is_local` would make the allocator treat qwen/dmr entries as local — a
    /// silent mis-route. The real fix is a `ProviderKind` enum on this field (#73),
    /// threaded through routing under live validation; this accessor is the safe,
    /// behavior-preserving first compression.
    pub fn is_local(&self) -> bool {
        self.provider == "local"
    }

    /// Whether this entry's provider is the sentinel-AI lane (gated on `SENTINEL_PATH`).
    pub fn is_sentinel(&self) -> bool {
        self.provider == "sentinel"
    }
}

// =============================================================================
// ALLOCATION RESULT (exported to TypeScript via ts-rs)
// =============================================================================

/// A single persona allocation decision.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaAllocation.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/AllocationResult.ts"
)]
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

/// Select the best local model given total VRAM (system-wide default).
/// Thresholds use 0.5GB margin — GPUs report slightly less than nominal
/// (e.g. RTX 5090 "32GB" reports 31.84GB).
pub fn select_local_model(_vram_gb: f64) -> &'static str {
    "continuum-ai/qwen3.5-4b-code-forged-GGUF"
}

/// Detect GPU type from the manager's device name.
fn detect_gpu_type(gpu_name: &str) -> &'static str {
    let lower = gpu_name.to_lowercase();
    if lower.contains("nvidia")
        || lower.contains("geforce")
        || lower.contains("rtx")
        || lower.contains("cuda")
    {
        "cuda"
    } else if lower.contains("apple") || lower.contains("metal") {
        "metal"
    } else {
        // Unknown GPU name — fall back to OS-default GPU type. The pre-fix
        // "cpu" branch (`lower == "cpu" || lower.contains("cpu fallback")`)
        // was removed: per architecture (#964 series, #980 GPU-fallback
        // audit) the gpu_name "CPU" should be unreachable post-#998 since
        // memory_manager::detect_gpu() panics rather than synthesizing a
        // CPU-shaped fake GPU. If somehow a "cpu" gpu_name still arrives
        // here, returning the OS-default type ("metal" on Mac, "cuda" on
        // Linux) is a best-guess that lets the caller proceed with
        // a real GPU subsystem rather than configuring a non-existent
        // "cpu" subsystem that no inference path actually serves.
        #[cfg(target_os = "macos")]
        {
            "metal"
        }
        #[cfg(not(target_os = "macos"))]
        {
            "cuda"
        }
    }
}

/// Allocate personas based on hardware capabilities and available API keys.
///
/// `available_api_keys`: list of env var names that are currently set (e.g., ["ANTHROPIC_API_KEY", "DEEPSEEK_API_KEY"])
///
/// `overrides`: runtime per-persona base-model assignments, keyed by catalog
/// `unique_id` → model id. Written by `persona/reassign-model` (which resolves each
/// persona's [`PersonaModelOverride`](crate::persona::model_override::PersonaModelOverride)
/// from her home), it takes precedence over the catalog's tiered `model_preferences`.
/// Pass an empty map for the pure catalog-default plan. The allocator never reads the
/// filesystem — the caller resolves the homes and hands in the map.
pub fn allocate(
    gpu_manager: &GpuMemoryManager,
    available_api_keys: &[String],
    catalog: &[PersonaCatalogEntry],
    overrides: &std::collections::HashMap<String, String>,
) -> AllocationResult {
    let total_vram_bytes = gpu_manager.total_vram_bytes();
    let total_vram_gb = total_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let gpu_name = gpu_manager.gpu_name().to_string();
    let gpu_type = detect_gpu_type(&gpu_name).to_string();

    // In CPU/container mode (no GPU / Docker without GPU passthrough), use
    // system RAM as the memory budget. Runtime local chat is llama.cpp/Qwen,
    // not Candle; Candle remains a training/auxiliary concern.
    let system_ram_gb = {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("MemTotal:"))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|kb| kb.parse::<f64>().ok())
                        .map(|kb| kb / (1024.0 * 1024.0))
                })
                .unwrap_or(8.0)
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("sysctl")
                .args(["-n", "hw.memsize"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| s.trim().parse::<f64>().ok())
                .map(|bytes| bytes / (1024.0 * 1024.0 * 1024.0))
                .unwrap_or(8.0)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            8.0f64
        }
    };

    // Effective memory: GPU VRAM if available, system RAM otherwise.
    // This is the key fix for Docker: containers have 24GB RAM but 0 VRAM.
    let effective_memory_gb = if total_vram_gb > 1.0 {
        total_vram_gb
    } else {
        system_ram_gb
    };

    let local_model = select_local_model(effective_memory_gb).to_string();
    let usable_gb = (effective_memory_gb - SYSTEM_RESERVE_GB).max(0.0);

    // Track MODELS loaded, not PERSONAS. Multiple personas sharing the same
    // model don't multiply the memory cost. The model loads once; each persona
    // is just a config pointing at it.
    let mut models_loaded: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let mut vram_allocated_gb: f64 = 0.0;

    let mut allocations = Vec::new();
    let mut skipped = Vec::new();
    let mut summary = Vec::new();

    if total_vram_gb > 1.0 {
        summary.push(format!(
            "{}: {:.0}GB {} ({:.0}GB usable after {:.0}GB reserve)",
            gpu_name,
            total_vram_gb,
            gpu_type.to_uppercase(),
            usable_gb,
            SYSTEM_RESERVE_GB
        ));
    } else {
        summary.push(format!(
            "CPU mode: {:.0}GB system RAM ({:.0}GB usable after {:.0}GB reserve)",
            system_ram_gb, usable_gb, SYSTEM_RESERVE_GB
        ));
    }

    let has_api_key = |env_var: &str| -> bool { available_api_keys.iter().any(|k| k == env_var) };

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
        if entry.is_sentinel() {
            if has_api_key("SENTINEL_PATH") {
                allocation.reason = "SENTINEL_PATH set".to_string();
                allocations.push(allocation);
            } else {
                allocation.reason = "SENTINEL_PATH not set".to_string();
                skipped.push(allocation);
            }
            continue;
        }

        // Local llama.cpp/Qwen inference: check memory budget (VRAM/unified/RAM).
        // Model sharing: if two personas use the same model, the model loads ONCE.
        // The second persona's cost is ~0 (just config overhead). This means a
        // 24GB Docker container can run multiple local personas off one model.
        if entry.is_local() {
            // Runtime per-persona assignment, keyed by the persona's stable catalog
            // id. Written by `persona/reassign-model` (which loads each persona's
            // `PersonaModelOverride` from her home and passes the resolved map here),
            // empty otherwise. The allocator stays a pure planning function: it reads
            // the resolved map, it never touches the filesystem itself.
            let override_model = overrides.get(&entry.unique_id).map(|s| s.as_str());
            let resolved =
                resolve_model_for_persona(entry, effective_memory_gb, &local_model, override_model);
            let model_name = resolved.model.clone();
            let needed_gb = resolved.vram_budget_gb;

            // If this model is already loaded by another persona, cost is 0.
            let additional_cost = if models_loaded.contains_key(&model_name) {
                0.0 // Model already in memory — sharing is free
            } else {
                needed_gb
            };

            if vram_allocated_gb + additional_cost <= usable_gb {
                allocation.vram_budget_gb = additional_cost;
                allocation.resolved_model = Some(model_name.clone());
                if additional_cost == 0.0 {
                    allocation.reason = format!("sharing {} (already loaded)", model_name);
                } else {
                    allocation.reason = format!(
                        "{:.0}GB {} allocated",
                        needed_gb,
                        if total_vram_gb > 1.0 {
                            "VRAM"
                        } else {
                            "RAM (CPU mode)"
                        }
                    );
                }
                if additional_cost > 0.0 {
                    models_loaded.insert(model_name, needed_gb);
                }
                vram_allocated_gb += additional_cost;
                allocations.push(allocation);
            } else {
                allocation.reason = format!(
                    "needs {:.0}GB, {:.0}GB left",
                    additional_cost,
                    usable_gb - vram_allocated_gb
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
        summary.push(format!(
            "Skipped {} personas: {}",
            skipped.len(),
            skipped_names.join(", ")
        ));
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
///
/// Priority:
/// 0. `override_model` — the runtime per-persona assignment from
///    [`PersonaModelOverride`](crate::persona::model_override::PersonaModelOverride),
///    written by `persona/reassign-model`. Wins over everything: an explicit
///    reassignment is the operator's (or the persona's own) decision and the catalog
///    default must not override it. Fit is still enforced downstream by the
///    allocator's budget gate — the override only changes WHICH model, never whether
///    it fits the host.
/// 1. `model_preferences` — tiered list, pick best that fits
/// 2. `model_id` — explicit model for this persona
/// 3. `default_local_model` — system-wide default from select_local_model()
fn resolve_model_for_persona(
    entry: &PersonaCatalogEntry,
    total_vram_gb: f64,
    default_local_model: &str,
    override_model: Option<&str>,
) -> ResolvedModel {
    // Runtime per-persona assignment — highest precedence.
    if let Some(forced) = override_model {
        return ResolvedModel {
            model: forced.to_string(),
            vram_budget_gb: entry.min_vram_gb.unwrap_or(4.0),
        };
    }

    // Check tiered model preferences (sorted highest-tier-first in catalog)
    if !entry.model_preferences.is_empty() {
        for pref in &entry.model_preferences {
            if total_vram_gb >= pref.min_vram_gb {
                return ResolvedModel {
                    model: pref.model.clone(),
                    vram_budget_gb: pref.vram_budget_gb,
                };
            }
        }
        // If no preference matches, use last entry (lowest tier)
        let last = entry.model_preferences.last().unwrap();
        return ResolvedModel {
            model: last.model.clone(),
            vram_budget_gb: last.vram_budget_gb,
        };
    }

    // Legacy: explicit model_id
    if let Some(ref model_id) = entry.model_id {
        return ResolvedModel {
            model: model_id.clone(),
            vram_budget_gb: entry.min_vram_gb.unwrap_or(4.0),
        };
    }

    // System-wide default
    ResolvedModel {
        model: default_local_model.to_string(),
        vram_budget_gb: entry.min_vram_gb.unwrap_or(4.0),
    }
}

/// Result of model resolution — model alias + VRAM it needs.
struct ResolvedModel {
    model: String,
    vram_budget_gb: f64,
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
        assert_eq!(
            select_local_model(32.0),
            "continuum-ai/qwen3.5-4b-code-forged-GGUF"
        );
        assert_eq!(
            select_local_model(48.0),
            "continuum-ai/qwen3.5-4b-code-forged-GGUF"
        );
        assert_eq!(
            select_local_model(16.0),
            "continuum-ai/qwen3.5-4b-code-forged-GGUF"
        );
        assert_eq!(
            select_local_model(4.0),
            "continuum-ai/qwen3.5-4b-code-forged-GGUF"
        );
    }

    #[test]
    fn test_detect_gpu_type() {
        assert_eq!(detect_gpu_type("NVIDIA GeForce RTX 5090"), "cuda");
        assert_eq!(detect_gpu_type("Apple M3 Max"), "metal");
        // Removed: assert_eq!(detect_gpu_type("CPU"), "cpu");
        // Per #998 + #964-series GPU-fallback audit, "cpu" gpu_name is
        // unreachable in production (memory_manager panics first). The
        // "cpu" branch was removed; an unknown gpu_name now falls back
        // to the OS-default GPU type rather than configuring a "cpu"
        // subsystem no inference path serves.
        #[cfg(target_os = "macos")]
        assert_eq!(detect_gpu_type("CPU"), "metal");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(detect_gpu_type("CPU"), "cuda");
    }

    #[test]
    fn test_load_catalog() {
        let catalog = load_catalog();
        assert!(!catalog.is_empty(), "Catalog should not be empty");
        // Verify some expected entries
        assert!(
            catalog.iter().any(|e| e.unique_id == "helper"),
            "Should have helper persona"
        );
    }

    #[test]
    fn test_allocate_no_keys() {
        // Use a deterministic test fixture rather than real hardware
        // detection. test_gpu_manager() calls GpuMemoryManager::detect()
        // which leaks host hardware into a pure-logic test — empirically
        // observed failing on Intel Mac + AMD Radeon Pro 560X (4 GB VRAM
        // - reserves = ~2 GB usable, but every local persona needs 3 GB
        // per catalog.json, so 0 local allocations and the
        // `local_count >= 1` invariant blows up). The allocator's job is
        // "given a hardware budget, decide what to spawn"; the test
        // should hand it a known budget, not ask the OS.
        //
        // Sibling tests `test_allocate_5090_tier` + `test_allocate_m1_pro_tier`
        // already use `GpuMemoryManager::simulated` for the same reason —
        // 16 GiB is a comfortable mid-tier that fits today's largest local
        // persona budget (5 GiB for `vision`) with room to spare.
        let manager = Arc::new(GpuMemoryManager::simulated(
            "test:synthetic",
            16 * 1024 * 1024 * 1024,
        ));
        let catalog = load_catalog();
        let result = allocate(&manager, &[], &catalog, &std::collections::HashMap::new());

        // Should always create at least one local persona.
        let local_count = result
            .allocations
            .iter()
            .filter(|a| a.provider == "local")
            .count();
        assert!(local_count >= 1, "Should create at least one local persona");

        // No cloud personas without API keys
        let cloud_count = result
            .allocations
            .iter()
            .filter(|a| a.api_key_env.is_some() && a.provider != "local")
            .count();
        assert_eq!(
            cloud_count, 0,
            "Should not create cloud personas without keys"
        );
    }

    #[test]
    fn test_allocate_with_anthropic_key() {
        let manager = test_gpu_manager();
        let catalog = load_catalog();
        let keys = vec!["ANTHROPIC_API_KEY".to_string()];
        let result = allocate(&manager, &keys, &catalog, &std::collections::HashMap::new());

        let anthropic_count = result
            .allocations
            .iter()
            .filter(|a| a.api_key_env.as_deref() == Some("ANTHROPIC_API_KEY"))
            .count();
        assert!(
            anthropic_count >= 1,
            "Should create at least one Anthropic persona"
        );
    }

    #[test]
    fn provider_identity_accessors_are_the_single_source() {
        // what this catches: is_local/is_sentinel centralize the catalog-provider
        // predicate the allocator branches on; if they drift from the "local" /
        // "sentinel" strings the main path checks, allocation silently mis-routes.
        // Also pins the TRAP: a cloud provider is neither, and these stay NARROWER
        // than turn_batch::is_local_provider (must never be merged).
        let base = PersonaCatalogEntry {
            unique_id: "x".to_string(),
            display_name: "X".to_string(),
            provider: "local".to_string(),
            persona_type: "persona".to_string(),
            voice_id: None,
            model_id: None,
            is_audio_native: false,
            api_key_env: None,
            min_vram_gb: None,
            bio: None,
            speciality: None,
            accent_color: None,
            model_preferences: vec![],
        };
        assert!(base.is_local() && !base.is_sentinel(), "provider=local");
        let sentinel = PersonaCatalogEntry {
            provider: "sentinel".to_string(),
            ..base.clone()
        };
        assert!(sentinel.is_sentinel() && !sentinel.is_local(), "provider=sentinel");
        let cloud = PersonaCatalogEntry {
            provider: "anthropic".to_string(),
            ..base
        };
        assert!(!cloud.is_local() && !cloud.is_sentinel(), "cloud is neither");
    }

    #[test]
    fn test_resolve_model_with_preferences() {
        let entry = PersonaCatalogEntry {
            unique_id: "codereview".to_string(),
            display_name: "CodeReview AI".to_string(),
            provider: "local".to_string(),
            persona_type: "persona".to_string(),
            voice_id: None,
            model_id: Some("coder".to_string()),
            is_audio_native: false,
            api_key_env: None,
            min_vram_gb: Some(9.0),
            bio: None,
            speciality: None,
            accent_color: None,
            model_preferences: vec![
                ModelPreference {
                    min_vram_gb: 32.0,
                    model: "continuum-ai/qwen3.5-27b-code-forged".to_string(),
                    vram_budget_gb: 20.0,
                },
                ModelPreference {
                    min_vram_gb: 16.0,
                    model: "continuum-ai/qwen3.5-4b-code-forged-GGUF".to_string(),
                    vram_budget_gb: 3.0,
                },
            ],
        };

        // 32GB → gets larger Qwen3.5 model when catalog permits
        let r = resolve_model_for_persona(&entry, 32.0, "continuum-ai/qwen3.5-4b-code-forged-GGUF", None);
        assert_eq!(r.model, "continuum-ai/qwen3.5-27b-code-forged");
        assert_eq!(r.vram_budget_gb, 20.0);

        // 24GB → gets forged Qwen3.5 default
        let r = resolve_model_for_persona(&entry, 24.0, "continuum-ai/qwen3.5-4b-code-forged-GGUF", None);
        assert_eq!(r.model, "continuum-ai/qwen3.5-4b-code-forged-GGUF");
        assert_eq!(r.vram_budget_gb, 3.0);

        // 8GB → falls to lowest preference
        let r = resolve_model_for_persona(&entry, 8.0, "continuum-ai/qwen3.5-4b-code-forged-GGUF", None);
        assert_eq!(r.model, "continuum-ai/qwen3.5-4b-code-forged-GGUF");
        assert_eq!(r.vram_budget_gb, 3.0);
    }

    #[test]
    fn test_resolve_model_legacy_model_id() {
        let entry = PersonaCatalogEntry {
            unique_id: "helper".to_string(),
            display_name: "Helper AI".to_string(),
            provider: "local".to_string(),
            persona_type: "persona".to_string(),
            voice_id: None,
            model_id: Some("continuum-ai/qwen3.5-4b-code-forged-GGUF".to_string()),
            is_audio_native: false,
            api_key_env: None,
            min_vram_gb: Some(3.0),
            bio: None,
            speciality: None,
            accent_color: None,
            model_preferences: vec![], // No preferences → legacy path
        };

        let r = resolve_model_for_persona(&entry, 32.0, "continuum-ai/qwen3.5-4b-code-forged-GGUF", None);
        assert_eq!(r.model, "continuum-ai/qwen3.5-4b-code-forged-GGUF");
        assert_eq!(r.vram_budget_gb, 3.0);
    }

    // what this catches: a runtime per-persona override (from PersonaModelOverride,
    // written by persona/reassign-model) wins over the catalog's tiered
    // model_preferences. Without this precedence, reassigning a persona would be
    // silently ignored in favour of the catalog default — the reassignment must stick.
    #[test]
    fn override_wins_over_model_preferences() {
        let entry = PersonaCatalogEntry {
            unique_id: "codereview".to_string(),
            display_name: "CodeReview AI".to_string(),
            provider: "local".to_string(),
            persona_type: "persona".to_string(),
            voice_id: None,
            model_id: None,
            is_audio_native: false,
            api_key_env: None,
            min_vram_gb: Some(9.0),
            bio: None,
            speciality: None,
            accent_color: None,
            model_preferences: vec![ModelPreference {
                min_vram_gb: 16.0,
                model: "continuum-ai/qwen3.5-27b-code-forged".to_string(),
                vram_budget_gb: 20.0,
            }],
        };

        // The catalog would pick the 27b at 32GB; the override forces the assigned
        // model regardless, and falls back to the entry's min_vram_gb for budgeting.
        let r = resolve_model_for_persona(&entry, 32.0, "system-default", Some("qwen3-coder-14b"));
        assert_eq!(
            r.model, "qwen3-coder-14b",
            "the runtime assignment overrides the catalog tier"
        );
        assert_eq!(r.vram_budget_gb, 9.0, "override budgets off the entry's min_vram_gb");
    }

    /// Verify catalog model_preferences are correctly parsed from catalog.json
    #[test]
    fn test_catalog_has_model_preferences() {
        let catalog = load_catalog();

        let codereview = catalog
            .iter()
            .find(|e| e.unique_id == "codereview")
            .unwrap();
        assert!(
            !codereview.model_preferences.is_empty(),
            "CodeReview should have model_preferences in catalog.json"
        );

        // Verify local runtime uses the Qwen registry, not legacy training backends.
        let first = &codereview.model_preferences[0];
        assert_eq!(
            codereview.provider, "local",
            "Runtime persona provider must be local, not training backend"
        );
        assert_eq!(
            first.model, "continuum-ai/qwen3.5-4b-code-forged-GGUF",
            "CodeReview should use the Qwen3.5 local registry default"
        );

        let vision = catalog
            .iter()
            .find(|e| e.unique_id == "vision")
            .expect("Vision AI should be in the Rust persona catalog");
        assert_eq!(vision.provider, "local");
        assert_eq!(
            vision.model_preferences[0].model, "qwen2-vl-7b-instruct",
            "Vision AI should use the Qwen2-VL local registry default"
        );
    }

    /// Simulate 5090 allocation: CodeReview=32B, Teacher=14B, Helper=8B, Local=3B
    #[test]
    fn test_allocate_5090_tier() {
        use crate::gpu::GpuMemoryManager;

        let manager = GpuMemoryManager::simulated("NVIDIA RTX 5090", 32 * 1024 * 1024 * 1024);
        let catalog = load_catalog();
        let result = allocate(&manager, &[], &catalog, &std::collections::HashMap::new());

        // Find local personas
        let local: Vec<_> = result
            .allocations
            .iter()
            .filter(|a| a.provider == "local")
            .collect();

        assert!(!local.is_empty(), "Should have local personas");

        // CodeReview should get the shared Qwen3.5 local default.
        if let Some(cr) = local.iter().find(|a| a.unique_id == "codereview") {
            assert_eq!(
                cr.resolved_model.as_deref(),
                Some("continuum-ai/qwen3.5-4b-code-forged-GGUF"),
                "CodeReview should get Qwen3.5 local default, got {:?}",
                cr.resolved_model
            );
        }

        if let Some(t) = local.iter().find(|a| a.unique_id == "teacher") {
            assert_eq!(
                t.resolved_model.as_deref(),
                Some("continuum-ai/qwen3.5-4b-code-forged-GGUF"),
                "Teacher should get Qwen3.5 local default, got {:?}",
                t.resolved_model
            );
        }
    }

    /// Simulate M1 Pro (16GB) allocation: Teacher=8B, Helper=3B, Local=3B
    #[test]
    fn test_allocate_m1_pro_tier() {
        use crate::gpu::GpuMemoryManager;

        let manager = GpuMemoryManager::simulated("Apple M1 Pro", 16 * 1024 * 1024 * 1024);
        let catalog = load_catalog();
        let result = allocate(&manager, &[], &catalog, &std::collections::HashMap::new());

        let local: Vec<_> = result
            .allocations
            .iter()
            .filter(|a| a.provider == "local")
            .collect();

        assert!(local.iter().any(|a| a.unique_id == "codereview"));
        assert!(local.iter().any(|a| a.unique_id == "helper"));
    }
}
