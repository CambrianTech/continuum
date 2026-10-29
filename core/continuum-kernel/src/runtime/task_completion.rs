//! Every task's end, published as `command:completed` (the one sink the kernel installs on its
//! [`continuum_tasks::TaskRegistry`]). The registry guarantees one end on every path, a panic
//! included; this sink makes each end the event the mind's dispatch listener files and wakes on.

use std::sync::Arc;

use continuum_tasks::{EndSink, TaskEnd, TaskKind, TaskRow};

use super::command_events::{CommandCompletedEvent, COMMAND_COMPLETED_TOPIC};
use super::message_bus::MessageBus;

pub(crate) struct CompletionPublisher {
    pub(crate) bus: Arc<MessageBus>,
}

impl EndSink for CompletionPublisher {
    fn ended(&self, row: &TaskRow, end: &TaskEnd) {
        let event = completed_event(row, end, now_ms());
        match serde_json::to_value(&event) {
            Ok(payload) => self.bus.publish_async_only(COMMAND_COMPLETED_TOPIC, payload),
            // Plain fields with no exotic types: unreachable, and a telemetry encode must
            // never fail the task's end. Said loudly, never silently.
            Err(e) => super::logger("command-executor").warn(&format!("task end for {} not published: {e}", row.id)),
        }
    }
}

/// The event an end becomes: the result rides it, so its owner learns the outcome from the
/// event itself.
pub(crate) fn completed_event(row: &TaskRow, end: &TaskEnd, now_ms: u64) -> CommandCompletedEvent {
    let command_name = match &row.kind {
        TaskKind::Command(path) | TaskKind::Sentinel(path) => path.clone(),
    };
    let (result, error) = match end {
        TaskEnd::Done(value) => (Some(value.clone()), None),
        TaskEnd::Failed(why) => (None, Some(why.clone())),
        TaskEnd::TimedOut => (None, Some("timed out".to_string())),
        TaskEnd::Stopped => (None, Some("stopped".to_string())),
        TaskEnd::Lost(why) => (None, Some(format!("lost: {why}"))),
    };
    CommandCompletedEvent {
        command_name,
        duration_ms: now_ms.saturating_sub(row.started_ms),
        success: end.succeeded(),
        error,
        handle: Some(row.id.as_uuid()),
        result,
    }
}

fn now_ms() -> u64 {
    continuum_base::time::EpochMs::now().millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an end the mind cannot read. A lost task (a panicking command) becomes
    // a failed completion naming why, under the task's own handle, with its duration.
    #[test]
    fn a_lost_task_becomes_a_failed_completion_under_its_handle() {
        let registry = continuum_tasks::TaskRegistry::new();
        let (guard, _watch) = registry.start(None, TaskKind::Command("code/cargo/test".to_string()));
        let row = registry.list(None).remove(0);
        let event = completed_event(&row, &TaskEnd::Lost("its runner panicked".to_string()), row.started_ms + 1_500);
        assert_eq!(event.handle, Some(guard.id().as_uuid()));
        assert!(!event.success);
        assert_eq!(event.error.as_deref(), Some("lost: its runner panicked"));
        assert_eq!(event.duration_ms, 1_500);
        assert_eq!(event.command_name, "code/cargo/test");
        guard.end(TaskEnd::Stopped);
    }
}
