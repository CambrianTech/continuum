//! The persona's async-dispatch listener — the consumer that closes the loop between
//! [`CommandExecutor::dispatch_background`] (#67, the producer) and the WorkingMemory async
//! recency channel (#66, the sink).
//!
//! A persona sends a long-running command away — a sentinel, a compile, a debugger — via
//! `dispatch_background`, which returns a UUID handle immediately and registers it in the
//! persona's [`WorkingMemory`] (`record_dispatch_event(..Running)`). This listener, spawned
//! once per persona, is subscribed to `command:completed` on the core bus; when a completion
//! carries a handle THIS persona registered, it folds the result in via
//! `record_dispatch_event(..Done/Failed)` — so the outcome streams back into the mind on the
//! next heartbeat, never blocking. It is pure REUSE of the pattern already under every
//! command: emit an event on a handle, subscribe, fold it in.
//! [[commands-are-agency-algs-are-pathways]] [[act-results-need-a-recency-channel-not-semantic-recall]]

use std::sync::Arc;

use crate::cognition::working_memory::{DispatchStatus, WorkingMemory};
use crate::runtime::command_events::{CommandCompletedEvent, COMMAND_COMPLETED_TOPIC};
use crate::runtime::message_bus::MessageBus;

// A dispatched command's result (a compile log, a sentinel's report) is bounded so one huge
// result can't dominate perception; the full text stays recoverable through the command's own
// handle. The bound is a FRACTION OF HER LIVE WINDOW, never a constant — the old
// `DISPATCH_RESULT_MAX_CHARS = 4_000` is exactly the "4k or smaller window" that makes a
// 1M-context model useless. [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
use crate::cognition::context_budget::ContextBudget;

/// Render a command result Value into the compact text the mind reads. A JSON string is
/// shown bare (no quotes); anything else is compact JSON. Bounded by
/// the live window (`ContextBudget::dispatch_result_chars`).
fn summarize_result(value: &serde_json::Value) -> String {
    let raw = match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let cap = ContextBudget::live().dispatch_result_chars();
    if raw.chars().count() > cap {
        raw.chars().take(cap).collect::<String>() + " …[truncated]"
    } else {
        raw
    }
}

/// Fold ONE completion event into the persona's working memory, IF the persona owns its
/// handle. Returns `true` when it was ours and recorded. Split out from the loop so it is
/// deterministically testable without a live bus.
pub fn fold_completion(wm: &WorkingMemory, ev: CommandCompletedEvent) -> bool {
    // Only tracked background dispatches carry a handle; sync commands don't.
    let Some(handle) = ev.handle else {
        return false;
    };
    // Only OUR handles — the WM registered the label at dispatch time. Other clients'
    // completions on the shared bus are not ours to fold.
    let Some(label) = wm.dispatched_label(handle) else {
        return false;
    };
    let (content, status) = if ev.success {
        let body = ev
            .result
            .as_ref()
            .map(summarize_result)
            .unwrap_or_else(|| "done".to_string());
        (body, DispatchStatus::Done)
    } else {
        let body = ev.error.unwrap_or_else(|| "failed".to_string());
        (body, DispatchStatus::Failed)
    };
    wm.record_dispatch_event(handle, &label, &content, status);
    true
}

/// Spawn the listener for one persona. Holds the persona's `WorkingMemory` (Arc) and a bus
/// receiver; runs until the process exits. A broadcast lag or a non-parsing payload is
/// skipped, never fatal — the mind keeps perceiving. Idempotent per persona: spawn once at
/// persona bring-up.
pub fn spawn(bus: Arc<MessageBus>, working_memory: Arc<WorkingMemory>) {
    let mut rx = bus.receiver();
    tokio::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(e) => e,
                // `Lagged` (slow consumer) or `Closed` — for a broadcast bus, keep going;
                // a closed bus only happens at shutdown, when this task is torn down anyway.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            };
            if event.name != COMMAND_COMPLETED_TOPIC {
                continue;
            }
            let Ok(ev) = serde_json::from_value::<CommandCompletedEvent>((*event.payload).clone()) else { // typed decode needs owned; one copy at THIS consumer, not per receiver
                continue;
            };
            fold_completion(&working_memory, ev);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(
        handle: Option<uuid::Uuid>,
        success: bool,
        result: serde_json::Value,
    ) -> CommandCompletedEvent {
        CommandCompletedEvent {
            command_name: "cargo/build".to_string(),
            duration_ms: 10,
            success,
            error: if success {
                None
            } else {
                Some("link error".to_string())
            },
            handle,
            result: if success { Some(result) } else { None },
        }
    }

    // what this catches: the listener folds a completion for a handle THIS persona
    // dispatched into working memory (Running → Done + result), and IGNORES handles it
    // never dispatched (another client's, or a sync command with no handle) — so a persona
    // only ever hears back about the sentinels IT sent away.
    #[test]
    fn folds_own_handles_ignores_others() {
        let wm = WorkingMemory::new(8);
        let mine = uuid::Uuid::from_u128(1);
        let not_mine = uuid::Uuid::from_u128(2);

        // The persona dispatched `mine` — registered as Running.
        wm.record_dispatch_event(mine, "cargo build", "dispatched…", DispatchStatus::Running);

        // A completion for a handle we never dispatched → ignored.
        assert!(!fold_completion(
            &wm,
            ev(Some(not_mine), true, serde_json::json!("ok"))
        ));
        // A synchronous completion (no handle) → ignored.
        assert!(!fold_completion(
            &wm,
            ev(None, true, serde_json::json!("ok"))
        ));

        // Our handle completes → folded in as Done with the result.
        assert!(fold_completion(
            &wm,
            ev(Some(mine), true, serde_json::json!("0 errors, 0 warnings"))
        ));
        let snap = wm.dispatched_snapshot();
        let ours = snap.iter().find(|(h, ..)| *h == mine).unwrap();
        assert_eq!(ours.3, DispatchStatus::Done);
        assert_eq!(
            ours.2, "0 errors, 0 warnings",
            "the result streamed back to the mind"
        );

        // A failure on our handle folds in as Failed with the error.
        assert!(fold_completion(
            &wm,
            ev(Some(mine), false, serde_json::Value::Null)
        ));
        let snap = wm.dispatched_snapshot();
        let ours = snap.iter().find(|(h, ..)| *h == mine).unwrap();
        assert_eq!(ours.3, DispatchStatus::Failed);
        assert_eq!(ours.2, "link error");
    }
}
