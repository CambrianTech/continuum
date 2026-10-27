//! Base-change example migration for card c85bc7ac.
//! 
//! When a resident's lane record (`PersonaModelOverride`) authoritatively names a new base,
//! and a bucket has no active, retryable, or pending-recovery attempt on its current base,
//! its examples move to that served base. A missing lane, offer, or peer never moves anything.

use std::collections::HashMap;
use uuid::Uuid;

use super::super::TrainingTriggerState;
use crate::modules::training_trigger::{BucketKey, PendingBatch};

/// Enumerate all persona homes on disk and map their UUIDs to the authoritative base model
/// declared in their `model_override.json`. Missing or malformed overrides are skipped (abstain).
fn collect_persona_bases(continuum_home: &std::path::Path) -> Result<HashMap<Uuid, String>, String> {
    let personas_dir = continuum_home.join("citizens/personas");
    let mut persona_bases = HashMap::new();

    for entry in std::fs::read_dir(&personas_dir).map_err(|e| format!("failed to read persona dir {}: {}", personas_dir.display(), e))? {
        let entry = entry.map_err(|e| format!("error iterating persona dir: {}", e))?;
        if !entry.file_type().map_or(false, |ft| ft.is_dir()) { continue; }

        let home_path = entry.path();
        
        // Attempt to load the override using the canonical PersonaHome / PersonaModelOverride path.
        let home = match std::fs::read_dir(home_path.join("airc")).ok() {
            Some(_) => crate::continuum_mind::persona::model_override::PersonaHome::from_root(home_path),
            None => continue, // No airc dir means it's not a valid persona home yet
        };

        if let Ok(Some(override_)) = crate::continuum_mind::persona::model_override::PersonaModelOverride::load(&home) {
            // Resolve the persona UUID from their identity file to match against BucketKey.persona_id
            let id_path = home_path.join("airc").join("identity.key");
            if let Ok(uuid_str) = std::fs::read_to_string(&id_path) {
                if let Ok(persona_id) = uuid_str.trim().parse::<Uuid>() {
                    persona_bases.insert(persona_id, override_.model_id);
                }
            }
        }
    }

    Ok(persona_bases)
}

/// Check if a bucket has an active, retryable, or pending-recovery attempt.
fn has_active_attempt(state: &TrainingTriggerState, key: &BucketKey) -> bool {
    state.active_dispatches.contains_key(key) || state.hydrating.contains_key(key)
}

pub(crate) async fn migrate_base_change_buckets(state: &std::sync::Arc<TrainingTriggerState>) -> Result<(), String> {
    let continuum_home = crate::paths::continuum_home();
    let persona_bases = collect_persona_bases(&continuum_home)?;
    
    if persona_bases.is_empty() { return Ok(()); }

    // Identify buckets to migrate: (old_key, new_target_key)
    let mut migrations: Vec<(BucketKey, BucketKey)> = Vec::new();
    
    for entry in state.buckets.iter() {
        let key = entry.key().clone();
        
        if let Some(&served_base) = persona_bases.get(&key.persona_id) {
            // Only migrate if the bucket's current base differs from the authoritative served base
            if key.base_model != served_base {
                // Skip if there is an active, retryable, or pending-recovery attempt on this old base
                if !has_active_attempt(state, &key) {
                    let new_key = BucketKey {
                        persona_id: key.persona_id,
                        trait_kind: key.trait_kind.clone(),
                        base_model: served_base.clone(),
                    };
                    
                    // Avoid duplicate migration targets in the same pass (idempotency)
                    if !migrations.iter().any(|(_, target)| target == &new_key) {
                        migrations.push((key, new_key));
                    }
                }
            }
        }
    }

    // Execute migrations
    for (old_key, new_key) in migrations {
        if let Some(mut old_batch) = state.buckets.remove(&old_key) {
            if let Some(new_entry) = state.buckets.get_mut(&new_key) {
                // Merge examples and submission IDs into the target bucket's existing batch.
                // Policy fields (lora, schedule, etc.) remain untouched from first arrival.
                new_entry.value_mut().submission_ids.extend(old_batch.submission_ids);
                new_entry.value_mut().examples.extend(old_batch.examples);
            } else {
                // No existing target bucket for this persona/trait on the served base: move the whole batch.
                state.buckets.insert(new_key.clone(), old_batch);
            }
        }

        // Clean up in-memory tracking for the old key so it doesn't trigger recovery or dispatch again
        if let Some(active) = state.active_dispatches.remove(&old_key) {
            drop(active);
        }
        state.hydrating.remove(&old_key);
    }

    Ok(())
}
