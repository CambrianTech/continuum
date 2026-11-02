//! Her tasks: one item per piece of work she sent away (a shell command handed back, a background
//! dispatch), keyed by its handle (HER-TASKS; HER-TOOLS, "One rendering"). Every level keeps the
//! task's line 1 (how it stands), so the first receipt that shows the item at ANY level
//! acknowledges its end; the output is the body, which the view sizes like any other item.

use continuum_context::cause::{Cause, ElementId, Origin};
use continuum_context::levels::{summarize, Shape};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::working_memory::{DispatchStatus, WorkingMemory};

/// The one mapping from a task's state to the occurrence her view renders and a render receipt
/// names: a task's terminal state is its own occurrence, so acknowledging an end never matches the
/// running line that came before it. The task ledger (#5585) calls this to read a receipt.
pub(crate) fn task_element(persona: uuid::Uuid, task: uuid::Uuid, status: &DispatchStatus) -> ElementId {
    let state = match status {
        DispatchStatus::Running => 0u8,
        DispatchStatus::Done => 1,
        DispatchStatus::Failed => 2,
    };
    element(persona, Producer::Task, &[task.as_bytes().as_slice(), &[state]].concat())
}

pub(crate) struct Tasks<'a> {
    pub memory: &'a WorkingMemory,
    pub persona: uuid::Uuid,
}

impl Source<ContextMessage> for Tasks<'_> {
    fn name(&self) -> &str {
        "tasks"
    }

    fn priority(&self) -> u8 {
        priority::ACTIONS
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        let tasks = self.memory.dispatched_snapshot();
        if tasks.is_empty() {
            return Vec::new();
        }
        let banner = super::standing(
            self.persona,
            "tasks",
            "banner",
            ContextMessage::user("[tasks] Work I sent away, by handle: each says how it stands; its end is delivered to me.".to_owned()),
        );
        let mut items = vec![banner];
        items.extend(tasks.into_iter().map(|(task, label, output, status)| {
            let word = match status {
                DispatchStatus::Running => "running",
                DispatchStatus::Done => "ok",
                DispatchStatus::Failed => "failed",
            };
            let line = format!("{word} task {} {label}", airc_core::shown_id::shown_form(task));
            let whole = if output.is_empty() { line.clone() } else { format!("{line}\n{output}") };
            let summary = summarize(&output, Shape::Output).map(|level| ContextMessage::user(format!("{line}\n{level}")));
            Item::whole(task_element(self.persona, task, &status), Cause::Root(Origin::ActivityStart), task.to_string(), ContextMessage::user(whole))
                .with_summary(summary)
                .with_pointer(ContextMessage::user(line))
        }));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_context::epoch::Block;

    // what this catches: a task's end that a receipt cannot name (the terminal state sharing the
    // running line's occurrence), or a level that drops how the task ended (Codex on #5639: the
    // ledger acknowledges from the first receipt listing the terminal element at any level).
    #[test]
    fn a_finished_task_is_its_own_occurrence_and_every_level_says_how_it_ended() {
        let memory = WorkingMemory::new(8);
        let persona = uuid::Uuid::from_u128(7);
        let task = uuid::Uuid::from_u128(42);
        memory.record_dispatch_event(task, "bash: cargo test", "running", DispatchStatus::Running);
        let running = Tasks { memory: &memory, persona }.items();
        let log: String = (1..=400).map(|n| format!("test case {n} ... ok\n")).collect();
        memory.record_dispatch_event(task, "bash: cargo test", &format!("exit 101\n{log}error: 1 failed"), DispatchStatus::Failed);
        let ended = Tasks { memory: &memory, persona }.items();
        assert_ne!(running[1].id(), ended[1].id(), "the end is its own occurrence");
        assert_eq!(ended[1].id(), task_element(persona, task, &DispatchStatus::Failed), "the one mapping names it");
        let starts_failed = |m: &ContextMessage| m.text().starts_with("failed task ");
        assert!(starts_failed(ended[1].full()));
        assert!(ended[1].summary().is_some_and(starts_failed), "the summary keeps line 1");
        assert!(ended[1].pointer().is_some_and(starts_failed), "the pointer is line 1");
    }
}
