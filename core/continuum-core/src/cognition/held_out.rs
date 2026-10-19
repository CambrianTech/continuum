//! What a gene may be judged on (goal 8; ONE-RESIDENT-MODEL §9.3 and §10.2).
//!
//! A gene's lift is only evidence of learning when it is measured on tasks the gene was
//! never trained on. The task a turn worked is recorded ONCE, when the turn roots at its
//! card ([`CardTask::of_card_title`], carried on the staged credit row), and written into
//! every training example it becomes ([`stamp_examples`]). The judge reads it back from the
//! training job's own examples ([`trained`]) and holds out every other task of the set
//! ([`held_out`]).
//!
//! Only grader-backed tasks (bench and coursework cards) can be held out: an ordinary card
//! has no check to grade, so it is recorded as [`CardTask::NotATask`] and never claimed as a
//! held-out result. An example written before tasks were recorded names none, so a set
//! judged against it is reported as NOT known clean, never silently counted as clean.

use crate::cognition::eval::EvalTask;
use crate::genome::fine_tuning::types::TrainingExample;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The example-metadata key the task rides under: ONE writer ([`stamp_examples`]), ONE
/// reader ([`trained`]).
const TASK_METADATA_KEY: &str = "task";

/// One task of one grader-backed set: the bench or coursework set and the task id inside it
/// (for a SWE bench, the instance id).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskIdentity {
    pub set: String,
    pub task: String,
}

/// What a card was, as a task, read when a turn roots at it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CardTask {
    /// A bench or coursework task: graded, so it can be held out.
    Task(TaskIdentity),
    /// Ordinary work: no grader, so never a held-out claim either way.
    NotATask,
}

impl CardTask {
    /// Through the one bench-title parser, so a card's task identity is the same string the
    /// dispatch wrote and the grader reads.
    pub fn of_card_title(title: &str) -> Self {
        crate::cognition::bench_title::parse_card_title(title)
            .map_or(CardTask::NotATask, |(set, task)| {
                CardTask::Task(TaskIdentity { set, task })
            })
    }
}

/// Record a staged row's task on every example built from it. `None` (a row staged before
/// tasks were recorded) writes nothing, which [`trained`] reads as unknown.
pub fn stamp_examples(params: &mut serde_json::Value, task: Option<&CardTask>) {
    let Some(task) = task else {
        return;
    };
    let encoded = serde_json::to_value(task); // training-trigger submit wire: example metadata IS the job's request.json
    let Ok(value) = encoded else {
        // A two-variant enum of strings always serializes; a failure here is a serde bug,
        // and writing nothing leaves the example reading as unknown, never as clean.
        return;
    };
    if let Some(examples) = params.get_mut("examples").and_then(|e| e.as_array_mut()) {
        for example in examples {
            if let Some(metadata) = example.get_mut("metadata").and_then(|m| m.as_object_mut()) {
                metadata.insert(TASK_METADATA_KEY.to_string(), value.clone());
            }
        }
    }
}

/// What one training job was trained on, read from its own examples.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Trained {
    pub tasks: BTreeSet<TaskIdentity>,
    /// Card-linked examples whose task is unknown (staged before tasks were recorded, or a
    /// task field that does not read). Any of these and the held-out set is not known clean.
    pub unknown: usize,
}

/// The tasks a training job's examples worked. An example with no card (live conversation)
/// worked no task; a card-linked example with no task field is counted unknown.
pub fn trained(examples: &[TrainingExample]) -> Trained {
    let mut out = Trained::default();
    for metadata in examples.iter().filter_map(|e| e.metadata.as_ref()) {
        match metadata.get(TASK_METADATA_KEY) {
            Some(value) => {
                let read = serde_json::from_value::<CardTask>(value.clone()); // read back from the job's request.json on disk
                match read {
                    Ok(CardTask::Task(task)) => {
                        out.tasks.insert(task);
                    }
                    Ok(CardTask::NotATask) => {}
                    Err(_) => out.unknown += 1,
                }
            }
            None if metadata.get("cardId").is_some() => out.unknown += 1,
            None => {}
        }
    }
    out
}

/// The tasks of `set` a gene trained as `trained` may be judged on.
#[derive(Debug)]
pub struct HeldOut<'a> {
    pub tasks: Vec<&'a EvalTask>,
    /// Tasks of the set left out because the gene trained on them.
    pub excluded: usize,
    /// Carried from [`Trained::unknown`]: non-zero means contamination cannot be ruled out.
    pub unknown: usize,
}

impl HeldOut<'_> {
    /// Known clean: every trained example named its task, so nothing held out was seen.
    pub fn is_known_clean(&self) -> bool {
        self.unknown == 0
    }
}

pub fn held_out<'a>(set: &str, tasks: &'a [EvalTask], trained: &Trained) -> HeldOut<'a> {
    let seen = |t: &EvalTask| trained.tasks.iter().any(|s| s.set == set && s.task == t.id);
    let kept: Vec<&EvalTask> = tasks.iter().filter(|t| !seen(t)).collect();
    HeldOut {
        excluded: tasks.len() - kept.len(),
        tasks: kept,
        unknown: trained.unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(id: &str) -> EvalTask {
        EvalTask {
            id: id.into(),
            ..Default::default()
        }
    }

    fn example(metadata: serde_json::Value) -> TrainingExample {
        TrainingExample {
            prompt: "p".into(),
            completion: "c".into(),
            metadata: Some(metadata),
            lived: None,
        }
    }

    // what this catches (goal 8, card 67af4fca): a gene judged on a task it was trained on
    // reports memorisation as learning. The identity the card title yields must survive the
    // trip through example metadata, so exactly the trained task of that set is held out,
    // and the same task id in ANOTHER set is not.
    #[test]
    fn a_gene_is_judged_only_on_tasks_it_never_trained_on() {
        let worked = CardTask::of_card_title("[bench coursework-0123456789ab] t2: fix the parser");
        assert_eq!(
            worked,
            CardTask::Task(TaskIdentity {
                set: "coursework-0123456789ab".into(),
                task: "t2".into()
            })
        );
        assert_eq!(
            CardTask::of_card_title("fix the parser"),
            CardTask::NotATask
        );

        let mut params = json!({"examples": [
            {"prompt": "p", "completion": "c", "metadata": {"cardId": "a"}},
            {"prompt": "p", "completion": "c", "metadata": {"cardId": "a"}},
        ]});
        stamp_examples(&mut params, Some(&worked));
        let examples: Vec<TrainingExample> =
            serde_json::from_value(params["examples"].clone()).expect("examples");
        let trained = trained(&examples);

        let set = [task("t1"), task("t2"), task("t3")];
        let judged = held_out("coursework-0123456789ab", &set, &trained);
        assert_eq!(
            judged
                .tasks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["t1", "t3"]
        );
        assert_eq!(judged.excluded, 1);
        assert!(judged.is_known_clean());
        assert_eq!(
            held_out("coursework-ffffffffffff", &set, &trained).excluded,
            0,
            "same id, other set"
        );
    }

    // what this catches (Astra on goal 8): an example staged before tasks were recorded must
    // not make a held-out set look clean, while ordinary work and live conversation, which
    // worked no graded task, must not make it look dirty.
    #[test]
    fn an_example_with_an_unrecorded_task_is_never_counted_clean() {
        let mut ordinary =
            json!({"examples": [{"prompt": "p", "completion": "c", "metadata": {"cardId": "b"}}]});
        stamp_examples(&mut ordinary, Some(&CardTask::NotATask));
        let mut examples: Vec<TrainingExample> =
            serde_json::from_value(ordinary["examples"].clone()).expect("examples");
        examples.push(example(json!({"source": "live-turn"})));
        let set = [task("t1")];
        assert!(held_out("s", &set, &trained(&examples)).is_known_clean());

        examples.push(example(json!({"cardId": "legacy"})));
        let judged = held_out("s", &set, &trained(&examples));
        assert_eq!(judged.unknown, 1);
        assert!(!judged.is_known_clean());
    }
}
