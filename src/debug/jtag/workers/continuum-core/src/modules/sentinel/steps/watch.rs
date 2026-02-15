//! Watch step execution — blocks until a matching event arrives on the MessageBus
//!
//! Uses the broadcast channel receiver to listen for events.
//! Supports configurable timeout (default 300s).

use serde_json::json;
use std::time::{Duration, Instant};

use crate::modules::sentinel::interpolation;
use crate::modules::sentinel::types::{ExecutionContext, PipelineContext, StepResult};

/// Default timeout for watch step: 5 minutes
const DEFAULT_WATCH_TIMEOUT_SECS: u64 = 300;

/// Block until a matching event arrives on the MessageBus
pub async fn execute(
    event_pattern: &str,
    timeout_secs: Option<u64>,
    index: usize,
    ctx: &mut ExecutionContext,
    pipeline_ctx: &PipelineContext<'_>,
) -> Result<StepResult, String> {
    use crate::runtime;
    let log = runtime::logger("sentinel");
    let start = Instant::now();

    let interpolated_pattern = interpolation::interpolate(event_pattern, ctx);
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_WATCH_TIMEOUT_SECS));

    log.info(&format!("[{}] Watch step: waiting for event '{}' (timeout={}s)",
        pipeline_ctx.handle_id, interpolated_pattern, timeout.as_secs()));

    let bus = pipeline_ctx.bus
        .ok_or_else(|| format!("[{}] Watch step requires MessageBus", pipeline_ctx.handle_id))?;

    let mut receiver = bus.receiver();

    let result = tokio::time::timeout(timeout, async {
        loop {
            match receiver.recv().await {
                Ok(bus_event) => {
                    if event_matches(&bus_event.name, &interpolated_pattern) {
                        return Ok((bus_event.name, bus_event.payload));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log.warn(&format!("[{}] Watch: receiver lagged by {} events",
                        pipeline_ctx.handle_id, n));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err("MessageBus channel closed".to_string());
                }
            }
        }
    }).await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(Ok((event_name, payload))) => {
            log.info(&format!("[{}] Watch step: received event '{}' after {}ms",
                pipeline_ctx.handle_id, event_name, duration_ms));

            Ok(StepResult {
                step_index: index,
                step_type: "watch".to_string(),
                success: true,
                duration_ms,
                output: Some(event_name.clone()),
                error: None,
                exit_code: None,
                data: json!({
                    "event": event_name,
                    "payload": payload,
                }),
            })
        }
        Ok(Err(e)) => {
            Err(format!("[{}] Watch step error: {}", pipeline_ctx.handle_id, e))
        }
        Err(_) => {
            log.warn(&format!("[{}] Watch step timed out after {}s waiting for '{}'",
                pipeline_ctx.handle_id, timeout.as_secs(), interpolated_pattern));

            Ok(StepResult {
                step_index: index,
                step_type: "watch".to_string(),
                success: false,
                duration_ms,
                output: None,
                error: Some(format!("Timed out after {}s waiting for event '{}'",
                    timeout.as_secs(), interpolated_pattern)),
                exit_code: None,
                data: json!({
                    "pattern": interpolated_pattern,
                    "timeoutSecs": timeout.as_secs(),
                }),
            })
        }
    }
}

/// Match an event name against a pattern with simple glob support.
///
/// Patterns:
/// - Exact match: `"build:complete"` matches `"build:complete"`
/// - Trailing wildcard: `"build:*"` matches `"build:complete"`, `"build:failed"`
/// - Single segment wildcard: `"*:complete"` matches `"build:complete"`
fn event_matches(event_name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern == event_name {
        return true;
    }

    let pattern_parts: Vec<&str> = pattern.split(':').collect();
    let event_parts: Vec<&str> = event_name.split(':').collect();

    // Trailing wildcard: "build:*" matches "build:anything:nested"
    if pattern.ends_with(":*") || pattern.ends_with("*") {
        let prefix = pattern.trim_end_matches(":*").trim_end_matches('*');
        return event_name.starts_with(prefix);
    }

    // Segment-level matching
    if pattern_parts.len() != event_parts.len() {
        return false;
    }

    pattern_parts.iter().zip(event_parts.iter()).all(|(p, e)| {
        *p == "*" || *p == *e
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_matching() {
        assert!(event_matches("build:complete", "build:complete"));
        assert!(event_matches("build:complete", "build:*"));
        assert!(event_matches("build:failed", "build:*"));
        assert!(event_matches("sentinel:abc:status", "sentinel:*"));
        assert!(event_matches("anything", "*"));
        assert!(!event_matches("build:complete", "build:failed"));
        assert!(!event_matches("build:complete", "deploy:complete"));
        assert!(event_matches("a:b:c", "a:*:c"));
        assert!(!event_matches("a:b:c", "a:*:d"));
    }
}
