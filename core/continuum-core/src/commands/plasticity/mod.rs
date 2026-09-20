//! `plasticity/*` — the typed command surface for the adaptive plasticity engine.
//!
//! These five verbs (`analyze`, `compact`, `compress`, `topology`, `pipeline`)
//! are STATELESS [`ActionCommand`](crate::sdk_codegen::ActionCommand)s — the
//! plasticity engine holds no per-instance state, so each command self-registers
//! via the unit-struct `action_command!` form and needs no module `commands()`
//! wiring. The topology-construction domain logic stays in
//! [`crate::modules::plasticity`] (`build_topology`); architecture dims are sourced
//! from the base model artifact via [`crate::model_registry::ModelArchConfig::from_artifact`]
//! — never guessed from the model name. These command bodies orchestrate over it.
//!
//! All five are `access: Privileged`: they read/write arbitrary filesystem paths
//! and perform heavy model surgery (head pruning, mixed-precision quantization,
//! GGUF export) — not an AiSafe surface.

pub mod analyze;
pub mod compact;
pub mod compress;
pub mod pipeline;
pub mod topology;

use crate::modules::plasticity::types::CompactionConfig;

/// Fold the optional top-level `targetSizeGb` convenience override into the
/// config block. A caller may pass `targetSizeGb` either inside `config` (the
/// canonical place) or at the top level of the params (the convenience the
/// legacy `parse_config` honored); the config block wins when both are set.
///
/// This is the only merge the typed params can't express declaratively — the
/// nested `config` object already deserializes partial overrides onto
/// `CompactionConfig::default()` via the type's container `#[serde(default)]`.
pub(crate) fn effective_config(
    mut config: CompactionConfig,
    target_size_gb: Option<f64>,
) -> CompactionConfig {
    if config.target_size_gb.is_none() {
        config.target_size_gb = target_size_gb;
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the top-level targetSizeGb convenience still reaches the
    // engine after the typed-param migration (legacy parse_config parity). A caller
    // passing targetSizeGb outside the config block must not silently lose it.
    #[test]
    fn top_level_target_size_merges_when_config_unset() {
        let merged = effective_config(CompactionConfig::default(), Some(18.5));
        assert_eq!(merged.target_size_gb, Some(18.5));
    }

    // what this catches: an explicit config.targetSizeGb wins over the top-level
    // convenience — the config block is canonical, the top-level is only a fallback.
    #[test]
    fn config_target_size_wins_over_top_level() {
        let config = CompactionConfig {
            target_size_gb: Some(20.0),
            ..CompactionConfig::default()
        };
        let merged = effective_config(config, Some(18.5));
        assert_eq!(merged.target_size_gb, Some(20.0));
    }
}
