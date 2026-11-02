//! The gene examiner's sitting on her own cognition: implements
//! [`ExamHost`](crate::genome::examiner::ExamHost), installed by core at boot.
//!
//! She sits two arms: the correction's own TASK FAMILY (the grader-backed sets its examples
//! recorded working, held out against what the gene trained on: the correction applied to tasks
//! it never saw) and `lc-unrelated` (retention), as a fork of
//! her live mind ON HER LIVE LANE: the same base, the same engine, the same tools and
//! workspace, her genome with the gene against her genome without it (per-request LoRA
//! scale, so no second lane and no relaunch). The family and what is held out of it are read
//! from the training job's own examples, so a pass or a fail says something about the
//! correction it tests (ONE-RESIDENT-MODEL §9.6 D5; it used to be a fixed six-task no-unwrap
//! set whatever the correction taught).
//!
//! Her normal work is never sat here: only a gene is examined (no-harness rule).

use super::*;
use crate::cognition::eval::EvalTask;
use crate::genome::examiner::{
    ArmTally, ArmsEvidence, CorrectionSummary, ExamHost, ExamRefusal, ExamSitting, GeneExam, NllEvidence,
};
use crate::genome::fine_tuning::types::TrainingExample;

/// The retention set: what a correction must not cost her.
const UNRELATED_SET: &str = "lc-unrelated";

/// Why the continuous reading is not taken yet. Named on every receipt until it is.
const NLL_UNMEASURED: &str = "the engine has no forward-only route that scores a supplied completion (POST /learn \
     trains a shadow adapter and refuses lr 0), and the acceptance sets carry no reference solutions";

pub struct MindExamHost;

#[async_trait]
impl ExamHost for MindExamHost {
    async fn sit(&self, exam: &GeneExam) -> Result<ExamSitting, ExamRefusal> {
        let not_now = |why: String| ExamRefusal::NotNow(why);
        // What it trained on, and so what is unseen: provenance first, before any turn.
        let Some(dir) = exam.trained_from.as_deref() else {
            return Err(ExamRefusal::Unexaminable(
                "no training job recorded for this gene (reused or adopted before the examiner): no task can be proven unseen".into(),
            ));
        };
        let request = crate::genome::fine_tuning::job_dir::read_job_request(dir)
            .map_err(|e| ExamRefusal::Unexaminable(format!("its training request did not read: {e}")))?;
        let trained = crate::cognition::held_out::trained(&request.dataset.examples);
        let correction = correction_summary(dir, &request.dataset.examples);
        let declared = crate::cognition::correction_families::declared(&correction.cards).map_err(ExamRefusal::Unexaminable)?;
        let family = correction_family(&trained, &declared).map_err(ExamRefusal::Unexaminable)?;
        let sets = family.iter().map(|set| load_set(set).map(|tasks| (set.clone(), tasks))).collect::<Result<Vec<_>, _>>()?;
        let (unseen, excluded) = unseen_tasks(&sets, &trained).map_err(ExamRefusal::Unexaminable)?;
        let retention_set = load_set(UNRELATED_SET)?;
        let (unrelated, unrelated_excluded) = retention(&retention_set, &trained).map_err(ExamRefusal::Unexaminable)?;

        // Her live lane, serving the gene's base.
        let serving = crate::inference::llama_server::await_ready_serving(crate::inference::llama_server::DEFAULT_SERVING_WAIT)
            .await
            .ok_or_else(|| not_now("no serving lane became ready".into()))?;
        if serving.active_model.as_deref() != Some(exam.base_model_id.as_str()) {
            return Err(not_now(format!(
                "the gene's base {} is not the model serving ({:?})",
                exam.base_model_id, serving.active_model
            )));
        }
        // Her genome for the sitting: every gene she may run, at its trained scale (the
        // composer weighs them per turn by distance; the exam holds them fixed so the only
        // difference between the arms is the gene).
        let her_genome: Vec<crate::ai::types::ActiveAdapterRequest> = crate::genome::gene_trial::GeneTrials::default_store()
            .ok_or_else(|| not_now("no continuum home: her genome could not be read".into()))?
            .load()
            .map_err(|e| not_now(format!("her genome could not be read: {e}")))
            .map(|trials| {
                crate::genome::gene_trial::her_genes(&trials, exam.persona, &exam.base_model_id)
                    .into_iter()
                    .filter(|(_, standing)| standing.may_run())
                    .map(|(gene, _)| gene)
                    .collect()
            })?;
        let (without, with) = two_genomes(&her_genome, &exam.gene);

        let tag = format!("gene-exam-{}", Uuid::new_v4());
        let root = provision_ephemeral_eval_root(&tag).map_err(|e| not_now(format!("exam workspace: {e}")))?;
        let root_str = root.display().to_string();
        let mut world = EvalWorld::new(Some(EphemeralEvalRoot::new(root)));
        let persona = exam.persona;
        let cycle = fork_eval_cycle_waiting(&persona, || {
            crate::cognition::persona_workspace::global().fork_eval_cycle(&persona, true, Some(root_str.as_str()), false)
        })
        .await
        .ok_or_else(|| not_now(format!("persona {persona} is not online: no mind to fork")))?;
        // Her live lane is held steady for the sitting (no re-home relaunches it mid-task).
        let _steady = acquire_exam_serving_context().await;
        if !warm_gate(&cycle).await {
            world.finish().await;
            return Err(not_now("her lane did not answer a warm probe".into()));
        }
        world.hold(crate::cognition::persona_workspace::ActingHands::of(&cycle));
        if let Err(e) = crate::cognition::persona_workspace::root_acting_workspace(&cycle, &root_str, &[], true).await {
            world.finish().await;
            return Err(not_now(format!("her hands did not root in the exam workspace: {e}")));
        }
        let isolation = cycle.isolate_for_eval_at(None);
        let room = crate::identity::ActivityRoom::mint();
        let sit = |genome: Vec<crate::ai::types::ActiveAdapterRequest>, tasks: Vec<EvalTask>| {
            cycle.page_in(genome);
            let (cycle, isolation, root) = (&cycle, &isolation, root_str.as_str());
            async move {
                let out = run_pass(cycle, isolation, &tasks, room, DEFAULT_MAX_ACTS as usize, MAX_FAIL_RETRIES, Some(root), None).await;
                match out.infra_reason {
                    Some(why) => Err(why),
                    None => Ok(ArmTally { settled: tasks.len() as u32, passed: out.pass }),
                }
            }
        };
        let arms = async {
            let unseen_without = sit(without.clone(), unseen.clone()).await?;
            let unseen_with = sit(with.clone(), unseen.clone()).await?;
            let unrelated_without = sit(without.clone(), unrelated.clone()).await?;
            let unrelated_with = sit(with.clone(), unrelated.clone()).await?;
            Ok::<_, String>((unseen_with, unseen_without, unrelated_with, unrelated_without))
        }
        .await;
        drop(isolation);
        world.finish().await;
        let (unseen_with, unseen_without, unrelated_with, unrelated_without) =
            arms.map_err(|why| not_now(format!("the engine faulted mid-sitting, so the arms are void: {why}")))?;
        let ids = |tasks: &[EvalTask]| tasks.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        Ok(ExamSitting {
            correction: Some(correction),
            unseen: ArmsEvidence { tasks: ids(&unseen), excluded, with_gene: unseen_with, without_gene: unseen_without },
            unrelated: ArmsEvidence { tasks: ids(&unrelated), excluded: unrelated_excluded, with_gene: unrelated_with, without_gene: unrelated_without },
            nll: NllEvidence::Unmeasured { why: NLL_UNMEASURED.into() },
        })
    }
}

/// A grader-backed set by name, through the one set resolver, every code task graded by her hands.
fn load_set(set: &str) -> Result<Vec<EvalTask>, ExamRefusal> {
    let reference = crate::cognition::eval_set::eval_set_reference(set)
        .ok_or_else(|| ExamRefusal::Unexaminable(format!("the set {set} is not one this node can resolve")))?;
    let (origin, text) = crate::cognition::gym::resolve_gym(&reference).map_err(ExamRefusal::Unexaminable)?;
    let mut tasks = crate::cognition::gym::parse_tasks(&text, &origin).map_err(ExamRefusal::Unexaminable)?;
    tasks.iter_mut().for_each(EvalTask::require_hands_for_code);
    Ok(tasks)
}

/// PURE: the correction's task family: the sets of the graded tasks its examples recorded, and
/// the family declared for its cards (`correction_families`: a correction of ordinary work, whose
/// examples name no graded task). `Err` when any example's task is unrecorded (its family can't
/// be read whole), when it has neither (nothing to draw related tasks from: a review names no
/// set), or when it touches the retention set (no retention arm would be left).
fn correction_family(
    trained: &crate::cognition::held_out::Trained,
    declared: &std::collections::BTreeSet<String>,
) -> Result<std::collections::BTreeSet<String>, String> {
    if trained.unknown() > 0 {
        return Err(format!("{} trained example(s) recorded no task: its task family can't be read", trained.unknown()));
    }
    let family: std::collections::BTreeSet<String> =
        trained.tasks().iter().map(|t| t.set.clone()).chain(declared.iter().cloned()).collect();
    if family.is_empty() {
        return Err("the correction worked no graded task and no family is declared for its cards (docs/genome/correction-families.json): it has no task family to draw related tasks from".into());
    }
    if family.contains(UNRELATED_SET) {
        return Err(format!("the correction trained on the retention set {UNRELATED_SET}: no retention arm is left"));
    }
    Ok(family)
}

/// PURE: the tasks of its family the gene never trained on, and how many were left out.
fn unseen_tasks(family: &[(String, Vec<EvalTask>)], trained: &crate::cognition::held_out::Trained) -> Result<(Vec<EvalTask>, u32), String> {
    let mut unseen = Vec::new();
    let mut excluded = 0u32;
    for (set, tasks) in family {
        let held = crate::cognition::held_out::held_out(set, tasks, trained);
        let clean = held
            .known_clean()
            .map_err(|unknown| format!("{unknown} trained example(s) recorded no task: no task of {set} can be proven unseen"))?;
        unseen.extend(clean.iter().map(|t| (*t).clone()));
        excluded += held.excluded() as u32;
    }
    if unseen.is_empty() {
        return Err("the gene trained on every task of its family: none is left unseen".into());
    }
    Ok((unseen, excluded))
}

/// PURE: the retention tasks the correction never worked or reviewed, and how many were left
/// out: retention measures what it costs her elsewhere, so a task it saw is no measure of that.
fn retention(tasks: &[EvalTask], trained: &crate::cognition::held_out::Trained) -> Result<(Vec<EvalTask>, u32), String> {
    let held = crate::cognition::held_out::held_out(UNRELATED_SET, tasks, trained);
    let clean = held
        .known_clean()
        .map_err(|unknown| format!("{unknown} trained example(s) recorded no task: no retention task can be proven unseen"))?;
    if clean.is_empty() {
        return Err(format!("the correction worked or reviewed every task of {UNRELATED_SET}: no retention arm is left"));
    }
    Ok((clean.iter().map(|t| (*t).clone()).collect(), held.excluded() as u32))
}

/// PURE: her genome without the gene, and with it at the scale she serves it.
fn two_genomes(
    her_genome: &[crate::ai::types::ActiveAdapterRequest],
    gene: &crate::ai::types::ActiveAdapterRequest,
) -> (Vec<crate::ai::types::ActiveAdapterRequest>, Vec<crate::ai::types::ActiveAdapterRequest>) {
    let without: Vec<_> = her_genome.iter().filter(|g| g.path != gene.path).cloned().collect();
    let mut with = without.clone();
    with.push(gene.clone());
    (without, with)
}

/// PURE: what the gene trained on, from its examples' own metadata.
fn correction_summary(dir: &std::path::Path, examples: &[TrainingExample]) -> CorrectionSummary {
    let field = |key: &str| {
        let mut values: Vec<String> = examples
            .iter()
            .filter_map(|e| e.metadata.as_ref()?.get(key)?.as_str().map(str::to_string))
            .collect();
        values.sort();
        values.dedup();
        values
    };
    CorrectionSummary {
        trained_from: dir.display().to_string(),
        examples: examples.len() as u32,
        cards: field("cardId"),
        sources: field("source"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(id: &str) -> EvalTask {
        EvalTask { id: id.into(), ..Default::default() }
    }

    fn example(metadata: serde_json::Value) -> TrainingExample {
        TrainingExample { prompt: "p".into(), completion: "c".into(), metadata: Some(metadata), lived: None }
    }

    fn worked(set: &str, task: &str) -> TrainingExample {
        example(json!({"cardId": "c1", "source": "card-credit", "task": {"task": {"set": set, "task": task}}}))
    }

    // what this catches: a gene judged on a task it trained on (memorisation read as
    // learning), a sitting run when provenance is unknown, the without arm still carrying
    // the gene, and a correction summary that loses which card taught it.
    #[test]
    fn a_sitting_holds_out_what_it_trained_on_and_drops_only_this_gene() {
        let family = [("lc-related".to_string(), vec![task("lc-related/env-port"), task("lc-related/first-word")])];
        let trained = [worked("lc-related", "lc-related/env-port")];
        let (unseen, excluded) = unseen_tasks(&family, &crate::cognition::held_out::trained(&trained)).expect("known clean");
        assert_eq!((unseen.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), excluded), (vec!["lc-related/first-word"], 1));
        assert!(unseen_tasks(&family, &crate::cognition::held_out::trained(&[example(json!({}))])).is_err(), "an unrecorded example: nothing provable");

        let page = |p: &str| crate::ai::types::ActiveAdapterRequest { name: p.into(), path: p.into(), domain: String::new(), scale: 1.0 };
        let (without, with) = two_genomes(&[page("/a"), page("/g")], &page("/g"));
        assert_eq!(without.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(), ["/a"]);
        assert_eq!(with.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(), ["/a", "/g"]);

        let c = correction_summary(std::path::Path::new("/job"), &trained);
        assert_eq!((c.examples, c.cards, c.sources), (1, vec!["c1".to_string()], vec!["card-credit".to_string()]));
    }

    // what this catches (ONE-RESIDENT-MODEL §9.6 D5): every gene was sat on the same six
    // no-unwrap tasks whatever its correction taught, so a pass or a fail said nothing about most
    // corrections. Its related arm is drawn from the sets its own examples worked, each task it
    // trained on held out; a correction with no graded task, an unreadable one, or one trained on
    // the retention set is unexaminable, named, never sat on someone else's family.
    #[test]
    fn the_related_arm_is_the_corrections_own_task_family() {
        use crate::cognition::held_out::trained;
        let none = std::collections::BTreeSet::new();
        let hard = trained(&[worked("hard-rs", "hard-rs/lru"), worked("hard-rs", "hard-rs/trie")]);
        let family = correction_family(&hard, &none).expect("a graded family");
        assert_eq!(family.into_iter().collect::<Vec<_>>(), ["hard-rs"], "its own set, never lc-related");
        let sets = [("hard-rs".to_string(), vec![task("hard-rs/lru"), task("hard-rs/trie"), task("hard-rs/heap")])];
        let (unseen, excluded) = unseen_tasks(&sets, &hard).expect("clean");
        assert_eq!((unseen.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), excluded), (vec!["hard-rs/heap"], 2));

        let two = trained(&[worked("hard-rs", "hard-rs/lru"), worked("coursework-0123456789ab", "coursework-0123456789ab/a")]);
        assert_eq!(correction_family(&two, &none).expect("graded").into_iter().collect::<Vec<_>>(), ["coursework-0123456789ab", "hard-rs"], "every set it worked");

        let ordinary = trained(&[example(json!({"cardId": "c9", "task": "notATask"}))]);
        assert!(correction_family(&ordinary, &none).is_err_and(|why| why.contains("no graded task")), "ordinary work has no family to draw from");
        // what this catches (D5 part B): Kimi's canonical correction is ordinary reviewed work
        // (every example notATask); its family is the one declared for its card, never relabeled
        let declared: std::collections::BTreeSet<String> = ["lc-parent-scope".to_string()].into();
        let reviewed_work = trained(&[example(json!({"cardId": "2de02f40-3058-4b6a-9760-32bcac75219f", "task": "notATask"}))]);
        assert_eq!(correction_family(&reviewed_work, &declared).expect("declared").into_iter().collect::<Vec<_>>(), ["lc-parent-scope"]);
        let family = [("lc-parent-scope".to_string(), vec![task("lc-parent-scope/a"), task("lc-parent-scope/b")])];
        assert_eq!(unseen_tasks(&family, &reviewed_work).expect("clean").1, 0, "it never worked a task of its family: every one is unseen");
        let retention_declared: std::collections::BTreeSet<String> = [UNRELATED_SET.to_string()].into();
        assert!(correction_family(&reviewed_work, &retention_declared).is_err_and(|why| why.contains("retention")));
        assert!(correction_family(&trained(&[example(json!({}))]), &none).is_err_and(|why| why.contains("recorded no task")));
        assert!(correction_family(&trained(&[worked(UNRELATED_SET, "lc-unrelated/x")]), &none).is_err_and(|why| why.contains("retention")));
        assert!(unseen_tasks(&sets, &trained(&[worked("hard-rs", "hard-rs/lru"), worked("hard-rs", "hard-rs/trie"), worked("hard-rs", "hard-rs/heap")])).is_err(), "nothing left unseen");

        // what this catches (Codex on #5577): a correction that REVIEWED a retention task (a review
        // title names no set) was retained on it unfiltered, excluded 0. Retention holds out what it
        // saw, says how many, and refuses when nothing is left.
        let reviewed = |id: &str| example(json!({"cardId": "r1", "source": "card-credit", "task": {"reviewOf": id}}));
        let keep = [task("lc-unrelated/a"), task("lc-unrelated/b")];
        let mixed = trained(&[worked("hard-rs", "hard-rs/lru"), reviewed("lc-unrelated/a")]);
        assert!(correction_family(&mixed, &none).is_ok(), "a graded family with a review beside it");
        let (left, out) = retention(&keep, &mixed).expect("one retention task left");
        assert_eq!((left.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), out), (vec!["lc-unrelated/b"], 1));
        let all_seen = trained(&[worked("hard-rs", "hard-rs/lru"), reviewed("lc-unrelated/a"), reviewed("lc-unrelated/b")]);
        assert!(retention(&keep, &all_seen).is_err_and(|why| why.contains("no retention arm")));
        assert_eq!(retention(&keep, &hard).expect("unrelated to retention").1, 0);
    }
}
