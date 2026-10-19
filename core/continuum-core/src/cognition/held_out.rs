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
//! held-out result. A review card names the task it reviews ([`CardTask::ReviewOf`]): her
//! review saw that task's patch, so it is never held out from her either.
//!
//! Only an explicit record proves anything (Astra on #5183). An example with no task field,
//! no metadata, or a field that does not read is UNKNOWN, so a set judged against it is
//! reported as not known clean, never silently counted as clean.

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
    /// A review of a task's fix, by its instance id: the title names no set, so it is
    /// matched in every set, which can only hold out more, never less.
    ReviewOf(String),
    /// Ordinary work or unlinked conversation: no grader, so never a held-out claim.
    NotATask,
}

impl CardTask {
    /// Through the one bench and review title parsers, so a card's task identity is the same
    /// string the dispatch wrote and the grader reads.
    pub fn of_card_title(title: &str) -> Self {
        use crate::cognition::bench_title::{parse_card_title, parse_review_title};
        if let Some((set, task)) = parse_card_title(title) {
            CardTask::Task(TaskIdentity { set, task })
        } else if let Some(task) = parse_review_title(title) {
            CardTask::ReviewOf(task)
        } else {
            CardTask::NotATask
        }
    }
}

/// Write `task` into one example's metadata: the ONE writer of the field.
fn record(metadata: &mut serde_json::Map<String, serde_json::Value>, task: &CardTask) {
    let encoded = serde_json::to_value(task); // training-trigger submit wire: example metadata IS the job's request.json
    if let Ok(value) = encoded {
        metadata.insert(TASK_METADATA_KEY.to_string(), value);
    }
    // A three-variant enum of strings always serializes; were it ever to fail, the field
    // stays absent and the example reads as unknown, never as clean.
}

/// Record a staged row's task on every example built from it. `None` (a row staged before
/// tasks were recorded) leaves no field, which [`trained`] reads as unknown.
pub fn stamp_examples(params: &mut serde_json::Value, task: Option<&CardTask>) {
    if let Some(examples) = params.get_mut("examples").and_then(|e| e.as_array_mut()) {
        for metadata in examples
            .iter_mut()
            .filter_map(|e| e.get_mut("metadata")?.as_object_mut())
        {
            match task {
                Some(task) => record(metadata, task),
                // Unknown must read as unknown: never leave an earlier NotATask standing.
                None => {
                    metadata.remove(TASK_METADATA_KEY);
                }
            }
        }
    }
}

/// What one training job was trained on, read from its own examples. Built only by
/// [`trained`]: its fields are private, so no caller can hand the judge a hand-made clean one.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Trained {
    tasks: BTreeSet<TaskIdentity>,
    /// Instance ids of tasks whose fix she reviewed (their set is not in a review title).
    reviewed: BTreeSet<String>,
    /// Examples with no readable task record: no metadata, no field, or a field that does
    /// not read. Any of these and the held-out set is not known clean.
    unknown: usize,
}

impl Trained {
    /// The graded tasks the job's examples recorded.
    pub fn tasks(&self) -> &BTreeSet<TaskIdentity> {
        &self.tasks
    }

    /// Examples with no readable task record.
    pub fn unknown(&self) -> usize {
        self.unknown
    }
}

/// The tasks a training job's examples worked. Only an explicit record counts: a task field,
/// or an unlinked producer's `source` with no card. Anything else is unknown.
pub fn trained(examples: &[TrainingExample]) -> Trained {
    use crate::persona::training_producer::UNLINKED_SOURCES;
    let mut out = Trained::default();
    for metadata in examples.iter().map(|e| e.metadata.as_ref()) {
        let field = metadata.and_then(|m| m.get(TASK_METADATA_KEY));
        let read = field.map(|value| serde_json::from_value::<CardTask>(value.clone())); // read back from the job's request.json on disk
        match read {
            Some(Ok(CardTask::Task(task))) => {
                out.tasks.insert(task);
            }
            Some(Ok(CardTask::ReviewOf(task))) => {
                out.reviewed.insert(task);
            }
            Some(Ok(CardTask::NotATask)) => {}
            // No task field: an UNLINKED producer's source (no card) is the explicit record
            // that it worked no graded task; anything else is unknown.
            None if metadata.is_some_and(|m| {
                m.get("cardId").is_none()
                    && m.get("source")
                        .and_then(|s| s.as_str())
                        .is_some_and(|s| UNLINKED_SOURCES.contains(&s))
            }) => {}
            Some(Err(_)) | None => out.unknown += 1,
        }
    }
    out
}

/// The tasks of `set` a gene trained as `trained` may be judged on. The tasks are reachable
/// only through [`HeldOut::known_clean`], so no caller can take them from a set whose
/// contamination is unknown (Astra on #5183).
#[derive(Debug)]
pub struct HeldOut<'a> {
    tasks: Vec<&'a EvalTask>,
    /// Tasks of the set left out because the gene trained on them.
    excluded: usize,
    /// Carried from [`Trained::unknown`]: non-zero means contamination cannot be ruled out.
    unknown: usize,
}

impl<'a> HeldOut<'a> {
    /// Tasks of the set left out because the gene trained on them.
    pub fn excluded(&self) -> usize {
        self.excluded
    }

    /// Trained examples with no readable task record. Read-only: every field is private, so
    /// [`HeldOut::known_clean`] cannot be talked past (Astra on #5183).
    pub fn unknown(&self) -> usize {
        self.unknown
    }

    /// The tasks a held-out result may be claimed on, when every trained example recorded its
    /// task; otherwise `Err` with how many did not, which the caller reports instead of a
    /// lift. It proves the RECORDED training provenance only: not that a lesson's text never
    /// mentions a benchmark task.
    pub fn known_clean(&self) -> Result<&[&'a EvalTask], usize> {
        if self.unknown == 0 {
            Ok(self.tasks.as_slice())
        } else {
            Err(self.unknown)
        }
    }
}

pub fn held_out<'a>(set: &str, tasks: &'a [EvalTask], trained: &Trained) -> HeldOut<'a> {
    let seen = |t: &EvalTask| {
        trained.reviewed.contains(&t.id)
            || trained.tasks.iter().any(|s| s.set == set && s.task == t.id)
    };
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
                .known_clean()
                .expect("every trained example recorded its task")
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["t1", "t3"]
        );
        assert_eq!(judged.excluded(), 1);
        assert!(judged.known_clean().is_ok());
        assert_eq!(
            held_out("coursework-ffffffffffff", &set, &trained).excluded(),
            0,
            "same id, other set"
        );
    }

    // what this catches (Astra on #5183): only an explicit record may prove an example worked
    // no held-out task. No metadata, empty metadata, a missing field (a row staged before tasks
    // were recorded) and an unreadable field are all UNKNOWN; explicit NotATask is not; and a
    // review of a task holds that task out in every set, because she saw its fix.
    #[test]
    fn only_an_explicit_record_proves_an_example_clean() {
        let set = [task("t1"), task("t2")];
        let lesson = crate::persona::training_producer::RECEIVED_LESSON_SOURCE;
        let clean = vec![
            example(json!({"task": "notATask"})),
            example(json!({"source": lesson})),
        ];
        assert!(held_out("s", &set, &trained(&clean)).known_clean().is_ok());

        let mut legacy = json!({"examples": [{"prompt": "p", "completion": "c", "metadata": {"cardId": "b", "task": "notATask"}}]});
        stamp_examples(&mut legacy, None);
        let legacy: Vec<TrainingExample> =
            serde_json::from_value(legacy["examples"].clone()).expect("examples");
        let unknowns = [
            TrainingExample {
                prompt: "p".into(),
                completion: "c".into(),
                metadata: None,
                lived: None,
            },
            example(json!({})),
            example(json!({"source": "live-turn"})),
            example(json!({"source": lesson, "cardId": "a card's example without its task"})),
            example(json!({"task": {"task": "missing its set"}})),
            legacy[0].clone(),
        ];
        for unknown in &unknowns {
            let judged = held_out("s", &set, &trained(std::slice::from_ref(unknown)));
            assert_eq!(
                judged.unknown(),
                1,
                "{:?} must read as unknown",
                unknown.metadata
            );
            assert_eq!(judged.known_clean().err(), Some(1));
        }

        let mut mixed = clean;
        mixed.push(example(
            json!({"task": {"task": {"set": "s", "task": "t1"}}}),
        ));
        mixed.push(unknowns[1].clone());
        let judged = held_out("s", &set, &trained(&mixed));
        assert_eq!(
            (judged.excluded(), judged.unknown()),
            (1, 1),
            "a known task is excluded AND the unknown is still reported"
        );

        let review = CardTask::of_card_title("[review 0a1b2c3d] t2: review Kimi's fix");
        assert_eq!(review, CardTask::ReviewOf("t2".into()));
        let mut reviewed =
            json!({"examples": [{"prompt": "p", "completion": "c", "metadata": {}}]});
        stamp_examples(&mut reviewed, Some(&review));
        let reviewed: Vec<TrainingExample> =
            serde_json::from_value(reviewed["examples"].clone()).expect("examples");
        let judged = held_out("any-set", &set, &trained(&reviewed));
        assert_eq!(
            judged
                .known_clean()
                .expect("every trained example recorded its task")
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["t1"]
        );
        assert!(judged.known_clean().is_ok());
    }
}
