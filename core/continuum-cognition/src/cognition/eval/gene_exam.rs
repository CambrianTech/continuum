//! The gene examiner's sitting on her own cognition: implements
//! [`ExamHost`](crate::genome::examiner::ExamHost), installed by core at boot.
//!
//! She sits the continual-learning acceptance sets (`lc-related`: the correction a review
//! gave her, applied to tasks the prompts never name; `lc-unrelated`: retention) as a fork of
//! her live mind ON HER LIVE LANE: the same base, the same engine, the same tools and
//! workspace, her genome with the gene against her genome without it (per-request LoRA
//! scale, so no second lane and no relaunch). The related set is held out against what the
//! gene trained on, read from its training job's own examples.
//!
//! Her normal work is never sat here: only a gene is examined (no-harness rule).

use super::*;
use crate::cognition::eval::EvalTask;
use crate::genome::examiner::{
    ArmTally, ArmsEvidence, CorrectionSummary, ExamHost, ExamRefusal, ExamSitting, GeneExam, NllEvidence,
};
use crate::genome::fine_tuning::types::TrainingExample;

/// The related set: the correction applied to unseen tasks.
const RELATED_SET: &str = "lc-related";
/// The retention set.
const UNRELATED_SET: &str = "lc-unrelated";

/// Why the continuous reading is not taken yet. Named on every receipt until it is.
const NLL_UNMEASURED: &str = "the engine has no forward-only route that scores a supplied completion (POST /learn \
     trains a shadow adapter and refuses lr 0), and lc-related/lc-unrelated carry no reference solutions";

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
        let related = load_set(RELATED_SET)?;
        let unrelated = load_set(UNRELATED_SET)?;
        let (unseen, excluded) = unseen_tasks(&related, &request.dataset.examples).map_err(ExamRefusal::Unexaminable)?;
        let correction = correction_summary(dir, &request.dataset.examples);

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
            unrelated: ArmsEvidence { tasks: ids(&unrelated), excluded: 0, with_gene: unrelated_with, without_gene: unrelated_without },
            nll: NllEvidence::Unmeasured { why: NLL_UNMEASURED.into() },
        })
    }
}

/// A committed acceptance set, every code task graded by her hands.
fn load_set(set: &str) -> Result<Vec<EvalTask>, ExamRefusal> {
    let (origin, text) = crate::cognition::gym::resolve_gym(&format!("{set}.jsonl")).map_err(ExamRefusal::Unexaminable)?;
    let mut tasks = crate::cognition::gym::parse_tasks(&text, &origin).map_err(ExamRefusal::Unexaminable)?;
    tasks.iter_mut().for_each(EvalTask::require_hands_for_code);
    Ok(tasks)
}

/// PURE: the related tasks the gene never trained on, and how many were left out. `Err` when
/// any example's task is unrecorded (nothing can be proven unseen) or none is left.
fn unseen_tasks(related: &[EvalTask], trained_on: &[TrainingExample]) -> Result<(Vec<EvalTask>, u32), String> {
    let trained = crate::cognition::held_out::trained(trained_on);
    let held = crate::cognition::held_out::held_out(RELATED_SET, related, &trained);
    let clean = held
        .known_clean()
        .map_err(|unknown| format!("{unknown} trained example(s) recorded no task: no related task can be proven unseen"))?;
    if clean.is_empty() {
        return Err("the gene trained on every related task: none is left unseen".into());
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

    // what this catches: a gene judged on a task it trained on (memorisation read as
    // learning), a sitting run when provenance is unknown, the without arm still carrying
    // the gene, and a correction summary that loses which card taught it.
    #[test]
    fn a_sitting_holds_out_what_it_trained_on_and_drops_only_this_gene() {
        let set = [task("lc-related/env-port"), task("lc-related/first-word")];
        let trained = [example(json!({"cardId": "c1", "source": "card-credit",
            "task": {"task": {"set": "lc-related", "task": "lc-related/env-port"}}}))];
        let (unseen, excluded) = unseen_tasks(&set, &trained).expect("known clean");
        assert_eq!((unseen.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), excluded), (vec!["lc-related/first-word"], 1));
        assert!(unseen_tasks(&set, &[example(json!({}))]).is_err(), "an unrecorded example: nothing provable");

        let page = |p: &str| crate::ai::types::ActiveAdapterRequest { name: p.into(), path: p.into(), domain: String::new(), scale: 1.0 };
        let (without, with) = two_genomes(&[page("/a"), page("/g")], &page("/g"));
        assert_eq!(without.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(), ["/a"]);
        assert_eq!(with.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(), ["/a", "/g"]);

        let c = correction_summary(std::path::Path::new("/job"), &trained);
        assert_eq!((c.examples, c.cards, c.sources), (1, vec!["c1".to_string()], vec!["card-credit".to_string()]));
    }
}
