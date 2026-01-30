//! Consciousness context builder — assembles RAG context from timeline + memory.
//!
//! Replaces TS UnifiedConsciousness.getContext() which times out due to
//! event loop saturation. All operations run on in-memory MemoryCorpus data.
//!
//! The consciousness context provides:
//! - Temporal continuity: "what was I doing before?"
//! - Cross-context awareness: "what happened in other rooms?"
//! - Active intentions: "what am I working on?"
//! - Peripheral activity: "is anything happening elsewhere?"

use crate::memory::corpus::MemoryCorpus;
use crate::memory::timeline;
use crate::memory::types::*;
use std::time::Instant;

// ─── Consciousness Context Builder ──────────────────────────────────────────

/// Build a complete consciousness context for a persona in a specific room.
///
/// All queries operate on the in-memory MemoryCorpus — no SQL, no filesystem.
/// Total target: <20ms (was 3+ seconds in TS).
pub fn build_consciousness_context(
    corpus: &MemoryCorpus,
    req: &ConsciousnessContextRequest,
) -> ConsciousnessContextResponse {
    let start = Instant::now();

    // 1. Temporal continuity
    let temporal = timeline::build_temporal_info(corpus, &req.room_id);

    // 2. Cross-context events
    let since_24h = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::hours(24))
        .map(|t| t.to_rfc3339())
        .unwrap_or_default();

    let cross_context_events = corpus.cross_context_events(&req.room_id, &since_24h, 10);
    let cross_context_event_count = cross_context_events.len();

    // 3. Active intentions + peripheral activity
    let active_intention_count = timeline::count_active_intentions(corpus);
    let has_peripheral = timeline::has_peripheral_activity(corpus, &req.room_id);

    // 4. Format prompt
    let formatted_prompt = format_consciousness_prompt(
        &temporal,
        &cross_context_events,
        active_intention_count,
        has_peripheral,
    );

    let build_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    ConsciousnessContextResponse {
        formatted_prompt: if formatted_prompt.is_empty() {
            None
        } else {
            Some(formatted_prompt)
        },
        build_time_ms,
        temporal,
        cross_context_event_count,
        active_intention_count,
        has_peripheral_activity: has_peripheral,
    }
}

// ─── Prompt Formatting ───────────────────────────────────────────────────────

fn format_consciousness_prompt(
    temporal: &TemporalInfo,
    cross_context_events: &[&TimelineEvent],
    active_intention_count: usize,
    has_peripheral: bool,
) -> String {
    let mut sections = Vec::new();

    // Temporal continuity
    if let Some(ref context_name) = temporal.last_active_context_name {
        let away_desc = format_time_away(temporal.time_away_ms);
        sections.push(format!(
            "Last active in: #{} ({})",
            context_name, away_desc
        ));

        if temporal.was_interrupted {
            if let Some(ref task) = temporal.interrupted_task {
                sections.push(format!("Interrupted task: {}", task));
            }
        }
    }

    // Cross-context awareness
    if !cross_context_events.is_empty() {
        let event_summaries: Vec<String> = cross_context_events
            .iter()
            .take(5)
            .map(|e| {
                format!(
                    "- [#{}] {}: {}",
                    e.context_name,
                    e.actor_name,
                    truncate_content(&e.content, 80)
                )
            })
            .collect();
        sections.push(format!(
            "Activity in other contexts:\n{}",
            event_summaries.join("\n")
        ));
    }

    // Active intentions
    if active_intention_count > 0 {
        sections.push(format!(
            "Active intentions: {} task(s) in progress",
            active_intention_count
        ));
    }

    // Peripheral activity indicator
    if has_peripheral {
        sections.push("Background activity detected in other contexts.".into());
    }

    if sections.is_empty() {
        return String::new();
    }

    format!("[CONSCIOUSNESS CONTEXT]\n{}", sections.join("\n"))
}

fn format_time_away(ms: i64) -> String {
    if ms < 0 {
        return "just now".into();
    }
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if days > 0 {
        format!("{} day(s) ago", days)
    } else if hours > 0 {
        format!("{} hour(s) ago", hours)
    } else if minutes > 0 {
        format!("{} minute(s) ago", minutes)
    } else {
        "just now".into()
    }
}

fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        format!("{}...", &content[..max_len])
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time_away() {
        assert_eq!(format_time_away(0), "just now");
        assert_eq!(format_time_away(5000), "just now"); // 5s
        assert_eq!(format_time_away(120_000), "2 minute(s) ago");
        assert_eq!(format_time_away(7_200_000), "2 hour(s) ago");
        assert_eq!(format_time_away(172_800_000), "2 day(s) ago");
    }

    #[test]
    fn test_truncate_content() {
        assert_eq!(truncate_content("short", 80), "short");
        let long = "x".repeat(100);
        let truncated = truncate_content(&long, 80);
        assert_eq!(truncated.len(), 83); // 80 + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_format_consciousness_prompt_empty() {
        let temporal = TemporalInfo {
            last_active_context: None,
            last_active_context_name: None,
            time_away_ms: 0,
            was_interrupted: false,
            interrupted_task: None,
        };
        let prompt = format_consciousness_prompt(&temporal, &[], 0, false);
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_format_consciousness_prompt_full() {
        let temporal = TemporalInfo {
            last_active_context: Some("room-1".into()),
            last_active_context_name: Some("general".into()),
            time_away_ms: 300_000, // 5 minutes
            was_interrupted: true,
            interrupted_task: Some("Reviewing code changes".into()),
        };

        let events = vec![TimelineEvent {
            id: "e1".into(),
            persona_id: "p1".into(),
            timestamp: "2025-01-01T00:00:00Z".into(),
            context_type: "room".into(),
            context_id: "room-2".into(),
            context_name: "academy".into(),
            event_type: "message_sent".into(),
            actor_id: "u1".into(),
            actor_name: "Joel".into(),
            content: "Teaching a new concept".into(),
            importance: 0.7,
            topics: vec![],
        }];

        let event_refs: Vec<&TimelineEvent> = events.iter().collect();
        let prompt = format_consciousness_prompt(&temporal, &event_refs, 2, true);
        assert!(prompt.contains("[CONSCIOUSNESS CONTEXT]"));
        assert!(prompt.contains("Last active in: #general"));
        assert!(prompt.contains("5 minute(s) ago"));
        assert!(prompt.contains("Interrupted task: Reviewing code changes"));
        assert!(prompt.contains("Activity in other contexts:"));
        assert!(prompt.contains("[#academy]"));
        assert!(prompt.contains("2 task(s) in progress"));
        assert!(prompt.contains("Background activity detected"));
    }
}
