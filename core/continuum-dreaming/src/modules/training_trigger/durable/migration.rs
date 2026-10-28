//! Base-change example migration for card c85bc7ac (card 5f198677).
//!
//! When a resident's lane record (`PersonaModelOverride`) authoritatively names a new base,
//! and a bucket has no active, retryable, or pending-recovery attempt on its current base,
//! its examples move to that served base. A missing lane, offer or peer never moves anything.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use super::super::TrainingTriggerState;
use crate::continuum_mind::persona::model_override::{PersonaHome, PersonaModelOverride};
use crate::modules::training_trigger::{BucketKey, PendingBatch};

/// Enumerate all persona homes on disk and map their UUIDs to the authoritative base model
/// declared in their `model_override.json`. Missing or malformed overrides are skipped (abstain).
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
            None => continue, // No airc dir means it's not a valid persona home yet
        };

        if let Ok(Some(override_)) = PersonaModelOverride::load(&home) {
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

pub(crate) async fn migrate_base_change_buckets(state: &Arc<TrainingTriggerState>) -> Result<(), String> {
    let continuum_home = crate::paths::continuum_home();
    let persona_bases = collect_persona_bases(&continuum_home)?;
    
    if persona_bases.is_empty() { return Ok(()); }

    // Identify buckets to migrate: (old_key, new_target_key)
    let mut migrations: Vec<(BucketKey, BucketKey)> = Vec::new();
    
    for entry in state.buckets.iter() {
        let key = entry.key().clone();
        
        if let Some(&served_base) = persona_bases.get(&key.persona_id) {
            // Only migrate if the bucket's current base differs from the served base
            if key.base_model != served_base {
                // Skip if there's an active, retryable, or pending-recovery attempt on this bucket
                if has_active_attempt(state, &key) { continue; }

                let target_key = BucketKey {
                    persona_id: key.persona_id,
                    base_model: served_base.clone(),
                    lane: key.lane.clone(),
                };
                
                // Only migrate if the target bucket already exists on the new base
                if state.buckets.contains_key(&target_key) {
                    migrations.push((key, target_key));
                }
            }
        }
    }

    // Execute migrations: move pending examples from old buckets to new ones
    for (old_key, new_key) in migrations {
        if let Some(old_entry) = state.buckets.remove(&old_key) {
            let mut merged = (*old_entry.value()).clone();
            
            // Find or create target bucket
            let new_bucket = state.buckets.entry(new_key.clone()).or_insert_with(|| {
                Arc::new(PendingBatch::default())
            });

            // Merge pending examples, preserving original provenance
            if let Some(existing) = state.buckets.get(&new_key) {
                merged.examples.extend_from_slice(&existing.value().examples);
            }
            
            // Update target bucket with merged data
            if let Some(mut new_entry) = state.buckets.get_mut(&new_key) {
                *new_entry = Arc::new(merged);
            }
        }
    }

    Ok(())
}
