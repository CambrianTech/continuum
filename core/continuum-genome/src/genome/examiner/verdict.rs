//! PURE: the examiner's verdict on one sitting. No I/O, no clock: what the evidence says.

use serde::{Deserialize, Serialize};

/// Graded tasks and how many passed, for one arm.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmTally {
    pub settled: u32,
    pub passed: u32,
}

/// One task set sat twice by her own cognition: with the gene, and without it, everything
/// else the same.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmsEvidence {
    /// The tasks both arms sat, by id.
    pub tasks: Vec<String>,
    /// Tasks of the set left out because the gene trained on them (0 for the unrelated set).
    pub excluded: u32,
    pub with_gene: ArmTally,
    pub without_gene: ArmTally,
}

/// What the gene trained on, read back from its training job's own examples.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionSummary {
    /// The training job's directory.
    pub trained_from: String,
    pub examples: u32,
    /// The cards the examples came from (a reviewed correction names its card).
    pub cards: Vec<String>,
    /// The producer `source` labels the examples carry.
    pub sources: Vec<String>,
}

/// The mean negative log-likelihood (nats/token) of each set's reference solutions, under
/// each arm: a continuous reading that cuts the variance of six pass/fail tasks. `Unmeasured`
/// names why the engine could not take it; the verdict then rests on the task arms alone and
/// the receipt says so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NllEvidence {
    #[serde(rename_all = "camelCase")]
    Measured {
        related_with: f64,
        related_without: f64,
        unrelated_with: f64,
        unrelated_without: f64,
    },
    Unmeasured { why: String },
}

/// One sitting of the acceptance test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamSitting {
    pub correction: Option<CorrectionSummary>,
    /// Related tasks the gene never trained on.
    pub unseen: ArmsEvidence,
    /// Retention: tasks unrelated to what it trained on.
    pub unrelated: ArmsEvidence,
    pub nll: NllEvidence,
}

/// How sure the examiner must be that she passes the unseen related tasks MORE often with
/// the gene than without it (Joel's acceptance bar, 2026-10-24). Six tasks a sitting: 4/5
/// against 1/5 clears it, 3/6 against 2/6 does not.
pub const ADMIT_AT: f64 = 0.9;
/// How many unrelated tasks the gene may cost her (Joel's bar: "unrelated retention intact",
/// `Δunrel ≥ −1 task`): one task of six is inside a single sitting's sampling noise.
pub const UNRELATED_TASKS_TOLERATED: i64 = 1;

/// The examiner's decision and the numbers it read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamVerdict {
    pub admit: bool,
    /// P(her pass rate on unseen related tasks is higher with the gene).
    pub p_better: f64,
    /// Unrelated tasks passed with the gene minus without it.
    pub unrelated_delta: i64,
    /// Whether the unrelated NLL is no worse with the gene; `None` when it was not measured.
    pub unrelated_nll_no_worse: Option<bool>,
    pub reason: String,
}

/// ln Γ(n) for a whole n ≥ 1: ln((n-1)!), exact as a sum (every argument formed here is a
/// whole number of tasks plus one).
fn ln_gamma_whole(n: u32) -> f64 {
    (2..n).map(|k| f64::from(k).ln()).sum()
}

fn ln_beta(a: u32, b: u32) -> f64 {
    ln_gamma_whole(a) + ln_gamma_whole(b) - ln_gamma_whole(a + b)
}

/// PURE: the probability that the gene's pass rate is above the rate without it, each rate
/// uniform before any task (Beta(1,1)), so Beta(passed+1, failed+1) after. Exact, by the
/// closed form for two Beta variables (Evan Miller):
/// P(B > A) = Σ_{i<αB} B(αA+i, βA+βB) / ((βB+i)·B(1+i, βB)·B(αA, βA)).
pub fn p_gene_better(with_gene: ArmTally, without_gene: ArmTally) -> f64 {
    let failed = |a: ArmTally| a.settled.saturating_sub(a.passed);
    let (ab, bb) = (with_gene.passed + 1, failed(with_gene) + 1);
    let (aa, ba) = (without_gene.passed + 1, failed(without_gene) + 1);
    (0..ab)
        .map(|i| (ln_beta(aa + i, ba + bb) - f64::from(bb + i).ln() - ln_beta(1 + i, bb) - ln_beta(aa, ba)).exp())
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

/// PURE: admit the gene when it is likely better on unseen related tasks, costs her at most
/// [`UNRELATED_TASKS_TOLERATED`] unrelated task, and does not raise her unrelated NLL.
pub fn decide(s: &ExamSitting) -> ExamVerdict {
    let p_better = p_gene_better(s.unseen.with_gene, s.unseen.without_gene);
    let unrelated_delta = i64::from(s.unrelated.with_gene.passed) - i64::from(s.unrelated.without_gene.passed);
    let unrelated_nll_no_worse = match &s.nll {
        NllEvidence::Measured { unrelated_with, unrelated_without, .. } => Some(unrelated_with <= unrelated_without),
        NllEvidence::Unmeasured { .. } => None,
    };
    let numbers = format!(
        "unseen {}/{} with the gene, {}/{} without (P(better) {:.3}); unrelated {}/{} with, {}/{} without (delta {}); unrelated NLL {}",
        s.unseen.with_gene.passed,
        s.unseen.with_gene.settled,
        s.unseen.without_gene.passed,
        s.unseen.without_gene.settled,
        p_better,
        s.unrelated.with_gene.passed,
        s.unrelated.with_gene.settled,
        s.unrelated.without_gene.passed,
        s.unrelated.without_gene.settled,
        unrelated_delta,
        match (&s.nll, unrelated_nll_no_worse) {
            (NllEvidence::Unmeasured { why }, _) => format!("unmeasured ({why})"),
            (_, Some(true)) => "no worse".to_string(),
            _ => "worse".to_string(),
        },
    );
    let refusals: Vec<&str> = [
        (p_better < ADMIT_AT, "not shown better on unseen related tasks"),
        (unrelated_delta < -UNRELATED_TASKS_TOLERATED, "costs her unrelated work"),
        (unrelated_nll_no_worse == Some(false), "raises her unrelated NLL"),
    ]
    .into_iter()
    .filter_map(|(failed, why)| failed.then_some(why))
    .collect();
    let admit = refusals.is_empty();
    let reason = if admit {
        format!("admitted: {numbers}")
    } else {
        format!("rejected, {}: {numbers}", refusals.join("; "))
    };
    ExamVerdict { admit, p_better, unrelated_delta, unrelated_nll_no_worse, reason }
}
