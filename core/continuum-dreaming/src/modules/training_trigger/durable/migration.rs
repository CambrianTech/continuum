//! Base-change example migration for card c85bc7ac (card 5f198677).
//!
//! When a resident's lane record (`PersonaModelOverride`) authoritatively names a new base,
//! and a bucket has no active, retryable, or pending-recovery attempt on its current base,
//! its examples move to that served base. A missing lane, offer or peer never moves anything.

use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use super::super::TrainingTriggerState;
use crate::continuum_mind::persona::model_override::{PersonaHome, PersonaModelOverride};
use crate::modules::training_trigger::BucketKey;

/// Enumerate persona homes and map their UUIDs to the authoritative base model.
fn collect_persona_bases(continuum_home: &Path) -> Result<HashMap<Uuid, String>, String> {
    let personas_dir = continuum_home.join("citizens/personas");
    let mut persona_bases = HashMap::new();

    for entry in std::fs::read_dir(&personas_dir).map_err(|e| format!("failed to read persona dir {}: {}", personas_dir.display(), e))? {
        let entry = entry.map_err(|e| format!("error iterating persona dir: {}", e))?;
        if !entry.file_type().map_or(false, |ft| ft.is_dir()) { continue; }

        let home_path = entry.path();
        
        // Attempt to load the override using the canonical PersonaHome / PersonaModelOverride path.
        let home = match std::fs::read_dir(home_path.join("airc")).ok() {
            Some(_) => PersonaHome::from_root(&home_path),
            None => continue,
        };

        if let Some(override_data) = home.load_override().ok().flatten() {
            persona_bases.insert(*home.id(), override_data.base_model);
        }
    }

    Ok(persona_bases)
}

/// Determine which buckets need migration and move their examples to the new base.
pub(super) async fn migrate_base_change_buckets(state: &TrainingTriggerState) -> Result<(), String> {
    let continuum_home = state.config.continuum_home.as_path();
    let persona_bases = collect_persona_bases(continuum_home)?;

    if persona_bases.is_empty() {
        return Ok(());
    }

    // Iterate over pending batches in the queue
    let mut to_update = Vec::new();
    
    for (key, batch) in state.pending_batches.lock().await.iter() {
        let bucket_key = &key.bucket;
        let persona_id = bucket_key.persona_id;
        
        // Check if this persona has a new authoritative base
        let Some(new_base) = persona_bases.get(&persona_id) else { continue };
        
        // Check current attempt state for this bucket
        let current_attempt = state.attempt_tracker.get_current_attempt(bucket_key).await;
        
        // Only migrate if there's no active, retryable, or pending-recovery attempt on the old base.
        match &current_attempt {
            Some(attempt) if attempt.state.is_active() || attempt.state.is_retryable() || attempt.state.is_pending_recovery() => {
                continue; // Skip buckets with ongoing work
            }
            _ => {}
        }

        to_update.push((key.clone(), batch.clone()));
    }

    // Process migrations sequentially to avoid contention
    for (key, mut batch) in to_update {
        let new_base = persona_bases.get(&key.bucket.persona_id).unwrap().clone();
        
        batch.update_base(&new_base);
        state.pending_batches.lock().await.insert(key, batch);
    }

    Ok(())
}