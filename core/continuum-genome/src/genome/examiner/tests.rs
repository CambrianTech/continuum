#![cfg(test)]
use super::*;
use crate::genome::gene_trial::{GeneTrials, TrialState};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

fn a(passed: u32, settled: u32) -> ArmTally {
    ArmTally { settled, passed }
}

fn arms(with: ArmTally, without: ArmTally) -> ArmsEvidence {
    ArmsEvidence { tasks: vec!["t".into(); with.settled as usize], excluded: 0, with_gene: with, without_gene: without }
}

fn sitting(unseen: (ArmTally, ArmTally), unrelated: (ArmTally, ArmTally), nll: NllEvidence) -> ExamSitting {
    ExamSitting { correction: None, unseen: arms(unseen.0, unseen.1), unrelated: arms(unrelated.0, unrelated.1), nll }
}

fn unmeasured() -> NllEvidence {
    NllEvidence::Unmeasured { why: "fixture".into() }
}

// what this catches: a probability that is not one (equal evidence must be a coin, the two
// directions must sum to one, more passes must raise it), and a verdict that admits noise,
// a gene that costs her unrelated work past one task, or one that raises her unrelated NLL.
#[test]
fn the_verdict_admits_only_a_likely_gain_that_keeps_unrelated_work() {
    for (c, s) in [(a(3, 6), a(3, 6)), (a(0, 6), a(0, 6)), (a(6, 6), a(6, 6))] {
        assert!((p_gene_better(c, s) - 0.5).abs() < 1e-9, "equal evidence is a coin: {c:?}");
    }
    assert!((p_gene_better(a(5, 6), a(1, 6)) + p_gene_better(a(1, 6), a(5, 6)) - 1.0).abs() < 1e-9);
    assert!(p_gene_better(a(5, 6), a(1, 6)) > p_gene_better(a(4, 6), a(1, 6)));

    let gain = (a(5, 6), a(1, 6));
    let kept = (a(6, 6), a(5, 6));
    assert!(decide(&sitting(gain, kept, unmeasured())).admit, "a clear gain, unrelated work kept");
    assert!(!decide(&sitting((a(3, 6), a(2, 6)), kept, unmeasured())).admit, "one task more is noise at six");
    let lost = decide(&sitting(gain, (a(3, 6), a(5, 6)), unmeasured()));
    assert!(!lost.admit && lost.unrelated_delta == -2 && lost.reason.contains("unrelated work"));
    assert!(decide(&sitting(gain, (a(4, 6), a(5, 6)), unmeasured())).admit, "one unrelated task is tolerated");
    let worse_nll = NllEvidence::Measured { related_with: 0.5, related_without: 0.9, unrelated_with: 1.2, unrelated_without: 1.1 };
    let v = decide(&sitting(gain, kept, worse_nll));
    assert!(!v.admit && v.unrelated_nll_no_worse == Some(false));
    let unmeasured_v = decide(&sitting(gain, kept, unmeasured()));
    assert_eq!(unmeasured_v.unrelated_nll_no_worse, None);
    assert!(unmeasured_v.reason.contains("unmeasured"), "an unmeasured NLL is named on the receipt: {}", unmeasured_v.reason);
}

fn trial(store: &GeneTrials, persona: Uuid, n: u128) -> crate::genome::gene_trial::GeneTrial {
    let path = format!("/genes/{n}.gguf");
    store.open(persona, "code", Path::new(&path), "qwen-27b", 10).expect("test: open")
}

fn admitted(t: &crate::genome::gene_trial::GeneTrial, boot: Uuid) -> GeneExamReceipt {
    let s = sitting((a(5, 6), a(1, 6)), (a(6, 6), a(6, 6)), unmeasured());
    GeneExamReceipt::of_sitting(t, &s, decide(&s), boot, 20)
}

// what this catches: an examiner that sits a gene twice, never sits a new one, re-sits
// retention in the same process that admitted it (that is no restart), never sits it after
// one, or leaves a decision a receipt already made out of the trial file.
#[test]
fn what_is_owed_follows_the_trials_and_their_receipts() {
    let dir = tempfile::tempdir().expect("test: dir");
    let store = GeneTrials::at(dir.path().join("trials.json"));
    let p = Uuid::from_u128(7);
    let (boot, later) = (Uuid::from_u128(0xb1), Uuid::from_u128(0xb2));
    let fresh = trial(&store, p, 1);
    let promoted = trial(&store, p, 2);
    let unsettled = trial(&store, p, 3);
    let unexaminable = trial(&store, p, 4);
    let v = crate::genome::gene_trial::TrialVerdict { with_gene: a(5, 6), without_gene: a(1, 6), reason: "r".into(), receipt: None };
    store.decide(promoted.id, TrialState::Promoted, v, 30).expect("test: decide");
    let receipts = vec![
        admitted(&promoted, boot),
        admitted(&unsettled, boot),
        GeneExamReceipt::unexaminable(&unexaminable, "no provenance".into(), boot, 20),
    ];
    let trials = store.load().expect("test: load");
    let owed = |b| due(&trials, &receipts, b).into_iter().map(|d| (d.trial, d.kind)).collect::<Vec<_>>();
    assert_eq!(owed(boot), vec![(fresh.id, ExamKind::Initial), (unsettled.id, ExamKind::Settle)], "same process: no retention");
    assert_eq!(
        owed(later),
        vec![(fresh.id, ExamKind::Initial), (promoted.id, ExamKind::Retention), (unsettled.id, ExamKind::Settle)],
        "a later process owes the retention re-run"
    );
    let mut retained = admitted(&promoted, boot);
    let s = sitting((a(5, 6), a(1, 6)), (a(6, 6), a(6, 6)), unmeasured());
    retained.restart = RestartEvidence::Sat { boot: later, unseen: s.unseen.clone(), unrelated: s.unrelated.clone(), nll: s.nll.clone(), verdict: decide(&s) };
    assert!(due(&trials, &[retained], later).iter().all(|d| d.trial != promoted.id), "retained once: done");
}

// what this catches: a receipt that does not survive its own store (the four fields, the
// tagged restart and outcome, the ids), which would leave the citizen's view and the
// retention re-run reading nothing.
#[tokio::test]
async fn a_receipt_round_trips_through_the_orm() {
    let (adapter, _tmp) = crate::orm::store::fresh_adapter().await;
    let receipts = crate::orm::OrmStore::<GeneExamReceipt>::new(adapter).await.expect("test: store");
    let dir = tempfile::tempdir().expect("test: dir");
    let store = GeneTrials::at(dir.path().join("trials.json"));
    let t = trial(&store, Uuid::from_u128(9), 1);
    let mut r = admitted(&t, Uuid::from_u128(0xb1));
    r.correction = Some(CorrectionSummary { trained_from: "/job".into(), examples: 1, cards: vec!["c".into()], sources: vec!["card-credit".into()] });
    receipts.save(receipt_id(t.id), &r).await.expect("test: save");
    let back = receipts.find_by_id(receipt_id(t.id)).await.expect("test: read").expect("test: present");
    assert_eq!(back, r);
}

/// A host that sits every exam with fixed arms (test-only: the examiner's seam, never a mind).
struct FixedHost(ExamSitting);

#[async_trait::async_trait]
impl ExamHost for FixedHost {
    async fn sit(&self, exam: &GeneExam) -> Result<ExamSitting, ExamRefusal> {
        match &exam.trained_from {
            Some(_) => Ok(self.0.clone()),
            None => Err(ExamRefusal::Unexaminable("no training provenance".into())),
        }
    }
}

// what this catches: the end-to-end examiner on fixture trials. A trained gene is sat,
// receipted and promoted; a gene with no provenance is receipted unexaminable and stays in
// her head; a second pass in the same process repeats nothing; the next process re-sits the
// promoted gene and, when she lost it, retires it.
#[tokio::test]
async fn a_pass_sits_receipts_decides_and_retention_retires_a_lost_gene() {
    let (adapter, _tmp) = crate::orm::store::fresh_adapter().await;
    let receipts = crate::orm::OrmStore::<GeneExamReceipt>::new(adapter).await.expect("test: store");
    let dir = tempfile::tempdir().expect("test: dir");
    let path = dir.path().join("trials.json");
    let store = GeneTrials::at(&path);
    let p = Uuid::from_u128(5);
    let trained = store
        .open_from(p, "code", Path::new("/genes/a.gguf"), "qwen-27b", Some(Path::new("/job/a")), 10)
        .expect("test: open");
    let reused = trial(&store, p, 2);
    let gain = sitting((a(5, 6), a(1, 6)), (a(6, 6), a(6, 6)), unmeasured());
    let examiner = Examiner::new(GeneTrials::at(&path), receipts.clone(), Arc::new(FixedHost(gain)), Uuid::from_u128(1));
    let report = examiner.pass().await.expect("test: pass");
    assert_eq!(report.decided, vec![(trained.id, TrialState::Promoted)]);
    assert_eq!(report.unexaminable, vec![reused.id]);
    assert_eq!(examiner.pass().await.expect("test: pass"), Default::default(), "nothing owed twice");

    let lost = sitting((a(1, 6), a(1, 6)), (a(6, 6), a(6, 6)), unmeasured());
    let next = Examiner::new(GeneTrials::at(&path), receipts.clone(), Arc::new(FixedHost(lost)), Uuid::from_u128(2));
    let second = next.pass().await.expect("test: pass");
    assert_eq!(second.decided, vec![(trained.id, TrialState::Retired)], "{second:?} {:?}", store.load());
    let rows = store.load().expect("test: load");
    assert_eq!(rows.iter().find(|t| t.id == trained.id).map(|t| t.state), Some(TrialState::Retired));
    assert_eq!(rows.iter().find(|t| t.id == reused.id).map(|t| t.state), Some(TrialState::Trial), "unjudged, still hers");
    let r = receipts.find_by_id(receipt_id(trained.id)).await.expect("test: read").expect("test: present");
    assert!(matches!(r.restart, RestartEvidence::Sat { ref verdict, .. } if !verdict.admit), "the restart field records the loss");
}
