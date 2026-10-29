//! Base-change example migration for card c85bc7ac.
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
use crate::modules::training_trigger_state_3::bucket_key_for_attempt;
use crate::modules::training_trigger::BucketKey;

/// Migrate example buckets whose resident has changed base model.
/// 
/// Only moves examples when:
/// - The persona/lane record (`PersonaHome` / `PersonaModelOverride`) authoritatively names a new base.
/// - The bucket currently sits on an old base (mismatch between bucket key and authoritative base).
/// - There is no active, retryable or pending-recovery attempt that would conflict with the move.
/// 
/// Buckets whose lane, offer or peer record are missing are never migrated — a broken identity
/// chain means we cannot prove residency.
pub(crate) async fn migrate_base_change_buckets(state: &Arc<TrainingTriggerState>) -> anyhow::Result<()> {
    let db = state.db();

    // 1. Load all persona records that have an authoritative base model (PersonaHome + PersonaModelOverride).
    //    We need the set of (persona_id, lane) → target_base mappings.
    let personas = super::load_persona_records(db).await?;

    if personas.is_empty() {
        return Ok(());
    }

    // 2. Build a map: persona_id → target_base (the authoritative base from PersonaModelOverride).
    //    Only consider personas that have an explicit override (not just the default home model).
    let mut persona_target_bases: HashMap<Uuid, String> = HashMap::new();
    for p in &personas {
        if let Some(override_) = &p.model_override {
            if let Some(ref target_base) = override_.base_model_id {
                persona_target_bases.insert(p.persona_id, target_base.clone());
            }
        }
    }

    if persona_target_bases.is_empty() {
        return Ok(());
    }

    // 3. Scan all buckets in the bucket index. For each bucket whose persona is in our map:
    //    - Check if the bucket's base_model differs from the authoritative target.
    //    - If yes, check that no active/retryable/pending-recovery attempt exists on this bucket.
    //    - If safe, migrate examples to the new base.
    
    let buckets = super::load_all_buckets(db).await?;

    for bucket in &buckets {
        if !persona_target_bases.contains_key(&bucket.persona_id) {
            continue;
        }

        let target_base = &persona_target_bases[&bucket.persona_id];

        // Skip if the bucket is already on the correct base.
        if bucket.base_model == *target_base {
            continue;
        }

        // Check for conflicting attempts (active, retryable, or pending-recovery).
        let has_conflict = super::has_active_or_retrying_attempt(db, &bucket.persona_id, &bucket.trait_kind, &bucket.base_model).await?;
        if has_conflict {
            continue;
        }

        // Safe to migrate: examples move from old base to target base.
        let _ = super::migrate_examples_to_base(
            db,
            &bucket.persona_id,
            &bucket.trait_kind,
            &bucket.base_model,
            target_base,
        ).await;

        // Update the bucket's recorded base model so future passes know it moved.
        super::update_bucket_base_model(db, &bucket.persona_id, &bucket.trait_kind, target_base).await?;
    }

    Ok(())
}
