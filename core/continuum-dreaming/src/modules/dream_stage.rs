//! The dream stage (docs/architecture/ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md §8.1, §9.6 D3):
//! learning as a step per arrival, never a batch. An experience accepted into her training journal
//! is one learn on her shadow adapter now, on the same loaded base between her turns, at the share
//! the engine grants (her wakefulness sets it: a trickle while she is awake, deep dreaming asleep;
//! the amygdala wakes her). One queue, one consumer: arrivals flow in as they are accepted, a step
//! the engine defers waits exactly as long as it says and is taken then, and nothing is dropped
//! or saved up. The journal still holds every arrival, so the batch interim (§9.4) remains the
//! fallback on a lane without the route until promotion at boundaries (D4) replaces it.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::genome::fine_tuning::engine_lora_adapter::{learn_conversation, shadow_shape};
use crate::genome::fine_tuning::types::{LoRAHyperparams, TrainingExample};
use crate::inference::llama_server::{fresh_shadow, learn, load_shadow, DreamError};

/// One accepted experience for one mind.
pub(crate) struct Arrival {
    pub persona: Uuid,
    pub base_model: String,
    pub example: TrainingExample,
    /// Her gene's shape, when the batch named it.
    pub lora: Option<LoRAHyperparams>,
    pub learning_rate: f32,
}

/// The stage's inbox: arrivals go in as they are accepted; one task takes them in order.
pub(crate) struct DreamStage {
    tx: mpsc::UnboundedSender<Arrival>,
}

/// The process's one dream stage, started on first use (inside the runtime that accepted it).
pub(crate) fn dream_stage() -> &'static DreamStage {
    static STAGE: OnceLock<DreamStage> = OnceLock::new();
    STAGE.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run(rx));
        DreamStage { tx }
    })
}

impl DreamStage {
    /// She has a new experience to learn from. Never blocks the caller: the stage takes it in turn.
    pub(crate) fn arrive(&self, arrival: Arrival) {
        if self.tx.send(arrival).is_err() {
            // The stage task ended (the runtime is shutting down): the journal still holds it.
            continuum_base::probe!(class = "dream.stage.closed", "the dream stage is gone; the arrival waits in the journal");
        }
    }
}

/// Her shadow's name on the lane: one per mind and base, so a base change never trains the old shape.
fn shadow_name(persona: Uuid, base_model: &str) -> String {
    let base: String = base_model.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    format!("shadow-{}-{base}.gguf", persona.simple())
}

/// Her shadow's init seed: fixed per mind, so a fresh shadow is reproducible for her.
fn shadow_seed(persona: Uuid) -> u32 {
    let b = persona.as_bytes();
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

async fn run(mut rx: mpsc::UnboundedReceiver<Arrival>) {
    // Each mind's shadow on the lane. An engine restart forgets loaded adapters: a step it refuses
    // drops the entry, and the next arrival makes her shadow fresh again (session checkpoints, §9.2,
    // are the slice that keeps her momentum across that).
    let mut shadows: HashMap<(Uuid, String), i64> = HashMap::new();
    while let Some(arrival) = rx.recv().await {
        let key = (arrival.persona, arrival.base_model.clone());
        let shadow = match shadows.get(&key) {
            Some(&id) => id,
            None => match open_shadow(&arrival).await {
                Some(id) => {
                    shadows.insert(key.clone(), id);
                    id
                }
                None => continue,
            },
        };
        let (messages, tools) = learn_conversation(&arrival.example);
        let messages = serde_json::Value::Array(messages);
        loop {
            match learn(shadow, &messages, tools.as_ref(), arrival.learning_rate).await {
                Ok(step) => {
                    continuum_base::probe!(
                        class = "engine.dream.step",
                        persona = %arrival.persona,
                        loss = step.loss,
                        n_labels = step.n_labels as i64,
                        ms = step.t_ms,
                        "one experience learned on her shadow"
                    );
                    break;
                }
                Err(DreamError::Deferred { retry_after_ms }) => {
                    // She is busy: the step waits for the share the engine names, then goes.
                    tokio::time::sleep(Duration::from_millis(retry_after_ms.max(1))).await;
                }
                Err(error) => {
                    continuum_base::probe!(
                        class = "dream.stage.refused",
                        persona = %arrival.persona,
                        error = %error,
                        "the lane did not take the step; the journal keeps the experience for the fallback path"
                    );
                    shadows.remove(&key);
                    break;
                }
            }
        }
    }
}

/// Her shadow on the lane: the file she already has (an engine restart forgets loaded adapters,
/// not files, and a published name is immutable, so it is loaded, never made again), else fresh
/// at her gene's shape. `None` (probed) when the lane can do neither.
async fn open_shadow(arrival: &Arrival) -> Option<i64> {
    let name = shadow_name(arrival.persona, &arrival.base_model);
    if let Ok(id) = load_shadow(&name).await {
        return Some(id);
    }
    let shape = match shadow_shape(arrival.lora.as_ref(), shadow_seed(arrival.persona)) {
        Ok(shape) => shape,
        Err(why) => {
            continuum_base::probe!(class = "dream.stage.no_shape", persona = %arrival.persona, why = %why, "her gene's shape has no engine form");
            return None;
        }
    };
    match fresh_shadow(&name, &shape).await {
        Ok(id) => Some(id),
        Err(error) => {
            continuum_base::probe!(
                class = "dream.stage.no_shadow",
                persona = %arrival.persona,
                error = %error,
                "the lane made no shadow; the journal keeps the experience for the fallback path"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two minds, or one mind on two bases, sharing (and overwriting) a shadow.
    #[test]
    fn a_shadow_is_one_per_mind_and_base() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        assert_ne!(shadow_name(a, "qwen3.6-35b"), shadow_name(b, "qwen3.6-35b"));
        assert_ne!(shadow_name(a, "qwen3.6-35b"), shadow_name(a, "qwen3.8-27b"));
        assert_eq!(shadow_name(a, "q/w:e n"), shadow_name(a, "q-w-e-n"), "a base name never escapes the train dir");
        assert_eq!(shadow_seed(a), shadow_seed(a));
    }
}
