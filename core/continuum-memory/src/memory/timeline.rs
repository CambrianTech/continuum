//! Timeline queries — temporal thread and cross-context awareness.
//!
//! Provides the "what was I doing before?" and "what happened in other rooms?"
//! context that feeds into the consciousness context builder.
//!
//! All operations run on in-memory MemoryCorpus data. Zero SQL.

use crate::memory::corpus::MemoryCorpus;
use crate::memory::types::*;

// ─── Temporal Thread ─────────────────────────────────────────────────────────

/// Build temporal continuity info for a persona in a specific context.
/// Answers: "When was I last active here? Was I interrupted? What was I doing?"
pub fn build_temporal_info(corpus: &MemoryCorpus, context_id: &str) -> TemporalInfo {
    let last_event = corpus.last_event_in_context(context_id);

    match last_event {
        Some(event) => {
            let now = chrono::Utc::now();
            let event_time = chrono::DateTime::parse_from_rfc3339(&event.timestamp)
                .map(|t| t.with_timezone(&chrono::Utc))
                .unwrap_or(now);

            let time_away_ms = (now - event_time).num_milliseconds();

            // Check if there was an uncompleted intention (interrupted task)
            let interrupted_task = find_interrupted_intention(corpus, context_id);
            let was_interrupted = interrupted_task.is_some();

            TemporalInfo {
                last_active_context: Some(event.context_id.clone()),
                last_active_context_name: Some(event.context_name.clone()),
                time_away_ms,
                was_interrupted,
                interrupted_task,
            }
        }
        None => TemporalInfo {
            last_active_context: None,
            last_active_context_name: None,
            time_away_ms: 0,
            was_interrupted: false,
            interrupted_task: None,
        },
    }
}

/// Look for an intention_formed event without a corresponding intention_completed.
fn find_interrupted_intention(corpus: &MemoryCorpus, context_id: &str) -> Option<String> {
    // Look for recent intentions (last 6 hours)
    let since = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::hours(6))
        .map(|t| t.to_rfc3339())
        .unwrap_or_default();

    let events = corpus.events_since(&since, 50);

    // Find intention_formed events for this context
    let mut formed_intentions: Vec<&&TimelineEvent> = events
        .iter()
        .filter(|e| e.context_id == context_id && e.event_type == "intention_formed")
        .collect();

    // Find completed intentions
    let completed_ids: Vec<&str> = events
        .iter()
        .filter(|e| e.context_id == context_id && e.event_type == "intention_completed")
        .map(|e| e.content.as_str())
        .collect();

    // Return first unfinished intention
    formed_intentions.retain(|e| !completed_ids.iter().any(|c| e.content.contains(c)));

    formed_intentions.first().map(|e| e.content.clone())
}

// ─── Cross-Context Summary ───────────────────────────────────────────────────

/// Count active intentions across all contexts (not just current room).
pub fn count_active_intentions(corpus: &MemoryCorpus) -> usize {
    let since = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::hours(6))
        .map(|t| t.to_rfc3339())
        .unwrap_or_default();

    let events = corpus.events_since(&since, 200);

    let formed: usize = events
        .iter()
        .filter(|e| e.event_type == "intention_formed")
        .count();
    let completed: usize = events
        .iter()
        .filter(|e| e.event_type == "intention_completed")
        .count();

    formed.saturating_sub(completed)
}

/// Check if there's peripheral activity (events in non-current contexts recently).
pub fn has_peripheral_activity(corpus: &MemoryCorpus, current_context_id: &str) -> bool {
    let since = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::minutes(30))
        .map(|t| t.to_rfc3339())
        .unwrap_or_default();

    !corpus
        .cross_context_events(current_context_id, &since, 1)
        .is_empty()
}
