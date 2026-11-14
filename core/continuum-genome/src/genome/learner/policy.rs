//! What a bucket learns with: its shadow's geometry and its step's rate (card 4114a2c6).
//!
//! A bucket's own policy (`lora`, `schedule`, pinned at its first arrival) where it set one;
//! otherwise the one owner of training defaults, `fine_tuning::native_jobs`, so in-place learning
//! and the trainer it replaces never disagree on a number. Target modules are named the way the
//! engine's adapter writer names its tensors.

use crate::genome::fine_tuning::types::{LoRAHyperparams, ScheduleParams};

/// A shadow's geometry and the rate each step learns at.
#[derive(Debug, Clone, PartialEq)]
pub struct LearnPolicy {
    pub rank: u32,
    pub alpha: f32,
    /// GGUF tensor names (`attn_q`, `attn_v`, ...).
    pub targets: Vec<String>,
    /// The blocks adapted, counted from the top; 0 adapts every block.
    pub top_layers: u32,
    pub learning_rate: f32,
}

/// The policy for a bucket that pinned `lora` and `schedule` (either may be absent).
pub fn learn_policy(lora: Option<&LoRAHyperparams>, schedule: Option<&ScheduleParams>) -> Result<LearnPolicy, String> {
    let defaults = crate::genome::fine_tuning::native_jobs_defaults();
    let lora = lora.cloned().unwrap_or(defaults.0); // the bucket set none: the trainer's one default owner
    let schedule = schedule.cloned().unwrap_or(defaults.1); // the same owner, for the step's rate
    Ok(LearnPolicy {
        rank: lora.rank,
        alpha: lora.alpha as f32,
        targets: gguf_targets(&lora.target_modules)?,
        top_layers: lora.top_layers.unwrap_or(0), // None adapts every block, which the engine writes as 0
        learning_rate: schedule.learning_rate as f32,
    })
}

/// HF module names (what [`LoRAHyperparams::target_modules`] carries) to the GGUF tensor names the
/// engine's adapter writer targets. An unknown name is refused: silently dropping a target learns a
/// different adapter.
pub fn gguf_targets(modules: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for module in modules {
        let target = match module.as_str() {
            "q_proj" | "attn_q" => "attn_q",
            "k_proj" | "attn_k" => "attn_k",
            "v_proj" | "attn_v" => "attn_v",
            "o_proj" | "attn_output" => "attn_output",
            "gate_proj" | "ffn_gate" => "ffn_gate",
            "up_proj" | "ffn_up" => "ffn_up",
            "down_proj" | "ffn_down" => "ffn_down",
            other => return Err(format!("LoRA target module {other:?} has no engine (GGUF) equivalent")),
        };
        if !out.iter().any(|held| held == target) {
            out.push(target.to_owned());
        }
    }
    if out.is_empty() {
        return Err("no LoRA target modules".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: in-place learning and the trainer disagreeing on a default, or a target
    // silently dropped. An unset bucket learns with the trainer's defaults; names map or refuse.
    #[test]
    fn an_unset_bucket_learns_with_the_trainers_defaults_and_targets_map_or_refuse() {
        let policy = learn_policy(None, None).expect("test: the defaults are learnable");
        assert_eq!((policy.rank, policy.alpha, policy.targets.clone(), policy.top_layers), (8, 16.0, vec!["attn_q".to_owned(), "attn_v".to_owned()], 0));
        assert_eq!(policy.learning_rate, 1e-4);
        assert_eq!(gguf_targets(&["q_proj".into(), "attn_v".into(), "q_proj".into()]).expect("test: maps"), vec!["attn_q", "attn_v"]);
        assert!(gguf_targets(&["lm_head".into()]).is_err());
    }
}
