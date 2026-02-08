/// RuntimeControl — Priority adjustment API for UI.
///
/// Allows runtime modification of module priorities.
/// Exposed via runtime/control/* commands.
/// TypeScript types generated via ts-rs for Ares (RTOS controller) integration.

use super::registry::ModuleRegistry;
use super::service_module::ModulePriority;
use super::module_metrics::ModuleStats;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

/// Complete module information for UI/Ares control
#[derive(Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/runtime/ModuleInfo.ts")]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    pub name: String,
    pub default_priority: ModulePriority,
    pub effective_priority: ModulePriority,
    pub needs_dedicated_thread: bool,
    pub command_prefixes: Vec<String>,
    #[ts(optional)]
    pub stats: Option<ModuleStats>,
}

pub struct RuntimeControl {
    registry: Arc<ModuleRegistry>,
    priority_overrides: DashMap<String, ModulePriority>,
}

impl RuntimeControl {
    pub fn new(registry: Arc<ModuleRegistry>) -> Self {
        Self {
            registry,
            priority_overrides: DashMap::new(),
        }
    }

    /// Adjust module priority at runtime
    pub fn set_priority(&self, module_name: &str, priority: ModulePriority) -> Result<(), String> {
        // Verify module exists
        if !self.registry.has_module(module_name) {
            return Err(format!("Module not found: {}", module_name));
        }

        self.priority_overrides.insert(module_name.to_string(), priority);
        Ok(())
    }

    /// Get current effective priority (override or default)
    pub fn effective_priority(&self, module_name: &str) -> Option<ModulePriority> {
        // Check override first
        if let Some(p) = self.priority_overrides.get(module_name) {
            return Some(*p);
        }

        // Fall back to module default
        self.registry.get_priority(module_name)
    }

    /// Clear priority override, revert to default
    pub fn clear_override(&self, module_name: &str) {
        self.priority_overrides.remove(module_name);
    }

    /// List all modules with their info
    pub fn list_modules(&self) -> Vec<ModuleInfo> {
        self.registry.module_names()
            .into_iter()
            .filter_map(|name| {
                let config = self.registry.get_config(&name)?;
                let stats = self.registry.get_metrics(&name).map(|m| m.stats());

                Some(ModuleInfo {
                    name: name.clone(),
                    default_priority: config.priority,
                    effective_priority: self.effective_priority(&name).unwrap_or(config.priority),
                    needs_dedicated_thread: config.needs_dedicated_thread,
                    command_prefixes: config.command_prefixes.iter().map(|s| s.to_string()).collect(),
                    stats,
                })
            })
            .collect()
    }

    /// Get info for specific module
    pub fn module_info(&self, module_name: &str) -> Option<ModuleInfo> {
        let config = self.registry.get_config(module_name)?;
        let stats = self.registry.get_metrics(module_name).map(|m| m.stats());

        Some(ModuleInfo {
            name: module_name.to_string(),
            default_priority: config.priority,
            effective_priority: self.effective_priority(module_name).unwrap_or(config.priority),
            needs_dedicated_thread: config.needs_dedicated_thread,
            command_prefixes: config.command_prefixes.iter().map(|s| s.to_string()).collect(),
            stats,
        })
    }
}
