//! `vdd/score` — the deterministic measurement core of the self-evolving-genome
//! A/B harness (slice 1).
//!
//! Score a model's answers against a held-out set → accuracy ∈ [0,1]. The A/B
//! *lift* = score(base+LoRA) − score(base): run this twice with two labels and
//! diff the `score`. The generation half (run the set through the gateway) is a
//! separate brick; this is the pure scorer it composes with — no model run, so
//! the number is pinned independently of any provider. Stateless. AiSafe — a
//! persona scoring its own eval set is exactly the loop we want self-served.
//!
//! See docs/genome/SELF-EVOLVING-GENOME.md.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::logging::TimingGuard;

/// One held-out evaluation case: the model's `actual` answer vs the `expected`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/vdd/ScoreCase.ts")]
#[serde(rename_all = "camelCase")]
pub struct ScoreCase {
    /// The prompt that produced `actual` — echoed into the per-case verdict so a
    /// failing case is self-describing.
    #[serde(default)]
    pub prompt: String,
    /// The expected answer (the held-out ground truth).
    pub expected: String,
    /// The model's actual answer being scored.
    pub actual: String,
}

/// Params for `vdd/score`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vdd/VddScoreParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VddScoreParams {
    /// The held-out cases to score.
    pub cases: Vec<ScoreCase>,
    /// Match method: "exact" (case-insensitive equality, trimmed) or "contains"
    /// (expected appears in actual, case-insensitive — the lenient default).
    #[serde(default = "default_score_method")]
    pub method: String,
    /// What's being scored, e.g. "base" or "base+lora-rust" — so the A/B can
    /// label the two runs it diffs.
    #[serde(default)]
    pub label: String,
    /// Scenario tag for the record.
    #[serde(default = "default_score_scenario")]
    pub scenario: String,
}

fn default_score_method() -> String {
    "contains".to_string()
}
fn default_score_scenario() -> String {
    "genome-ab-eval".to_string()
}

/// Per-case verdict surfaced in the result so a failing case is debuggable.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vdd/ScoreCaseVerdict.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ScoreCaseVerdict {
    pub prompt: String,
    pub expected: String,
    pub correct: bool,
}

/// Result of `vdd/score` — the accuracy measurement + per-case verdicts. `score`
/// ∈ [0,1] is the number the A/B's *lift* is a difference of.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vdd/VddScoreResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VddScoreResult {
    pub scenario: String,
    pub label: String,
    pub method: String,
    pub total: u32,
    pub correct: u32,
    pub score: f64,
    pub cases: Vec<ScoreCaseVerdict>,
}

/// Is one case correct under `method`? Pure — the unit of measurement.
pub(crate) fn case_correct(method: &str, expected: &str, actual: &str) -> bool {
    let e = expected.trim();
    let a = actual.trim();
    match method {
        "exact" => a.eq_ignore_ascii_case(e),
        // "contains" (default): the expected answer appears in the response.
        _ => a.to_lowercase().contains(&e.to_lowercase()),
    }
}

/// Score a set → (correct, total). Pure.
pub(crate) fn score_cases(method: &str, cases: &[ScoreCase]) -> (u32, u32) {
    let total = cases.len() as u32;
    let correct = cases
        .iter()
        .filter(|c| case_correct(method, &c.expected, &c.actual))
        .count() as u32;
    (correct, total)
}

/// Score an eval set → the measurement. Deterministic — no model run, so the
/// scorer is pinned independently of any provider.
pub(crate) fn score_eval_set(p: &VddScoreParams) -> VddScoreResult {
    let (correct, total) = score_cases(&p.method, &p.cases);
    let score = if total == 0 {
        0.0
    } else {
        correct as f64 / total as f64
    };
    let cases = p
        .cases
        .iter()
        .map(|c| ScoreCaseVerdict {
            prompt: c.prompt.clone(),
            expected: c.expected.clone(),
            correct: case_correct(&p.method, &c.expected, &c.actual),
        })
        .collect();
    VddScoreResult {
        scenario: p.scenario.clone(),
        label: p.label.clone(),
        method: p.method.clone(),
        total,
        correct,
        score,
        cases,
    }
}

crate::action_command! {
    /// Score a model's answers against a held-out eval set and return the
    /// accuracy (correct / total ∈ [0,1]) plus a per-case pass/fail verdict.
    /// Deterministic — no model is run, so the score is provider-independent.
    /// Run it twice with two `label`s ("base", "base+lora") on the same set and
    /// the *lift* = score(lora) − score(base) is the keystone number of the
    /// self-evolving-genome A/B. `method`: "contains" (lenient default) or
    /// "exact". Read-only / pure.
    pub struct VddScore;
    name: "vdd/score",
    access: AiSafe,
    params: VddScoreParams,
    output: VddScoreResult,
    run(_this, _ctx, _p) => {
        let _timer = TimingGuard::new("module", "vdd_score");
        Ok(score_eval_set(&_p))
    }
}

#[cfg(test)]
mod tests {
    //! Pin the measurement unit + the command path: get `case_correct` wrong and
    //! every downstream lift number is wrong, so these guard the keystone.
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn case(expected: &str, actual: &str) -> ScoreCase {
        ScoreCase {
            prompt: String::new(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        }
    }

    // what this catches: name/access wiring — a pure self-served scorer is AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(VddScore::NAME, "vdd/score");
        assert!(matches!(
            VddScore::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: the unit of measurement. "contains" is lenient +
    // case-insensitive; "exact" is strict equality (trimmed, case-insensitive).
    #[test]
    fn case_correct_methods() {
        assert!(case_correct("contains", "Paris", "The capital is paris."));
        assert!(!case_correct("contains", "Paris", "The capital is London."));
        assert!(case_correct("exact", "4", " 4 "));
        assert!(!case_correct("exact", "4", "the answer is 4"));
    }

    // what this catches: accuracy over a set — correct/total. This IS the score
    // the A/B diffs.
    #[test]
    fn score_cases_counts_accuracy() {
        let cases = vec![
            case("4", "4"),                 // correct (contains)
            case("Paris", "paris is it"),   // correct
            case("blue", "the sky is red"), // wrong
        ];
        let (correct, total) = score_cases("contains", &cases);
        assert_eq!((correct, total), (2, 3));
    }

    // what this catches: the command end-to-end returns the accuracy measurement,
    // AND that two labeled runs COMPOSE into a lift (the whole point of slice 1:
    // lift = score(base+LoRA) − score(base)).
    #[tokio::test]
    async fn vdd_score_measures_and_composes_into_lift() {
        let base = VddScoreParams {
            label: "base".into(),
            method: "contains".into(),
            scenario: "genome-ab-eval".into(),
            cases: vec![case("4", "4"), case("spatial hash", "use a quadtree")],
        };
        let lora = VddScoreParams {
            label: "base+lora".into(),
            method: "contains".into(),
            scenario: "genome-ab-eval".into(),
            cases: vec![
                case("4", "4"),
                case("spatial hash", "use a spatial hash grid"),
            ],
        };

        let s_base = VddScore
            .run(&Ctx::default(), base)
            .await
            .expect("score must succeed")
            .score;
        let s_lora = VddScore
            .run(&Ctx::default(), lora)
            .await
            .expect("score must succeed")
            .score;
        assert_eq!(s_base, 0.5);
        assert_eq!(s_lora, 1.0);

        let lift = s_lora - s_base;
        assert!(
            (lift - 0.5).abs() < 1e-9,
            "lift = score(lora) − score(base) = 0.5"
        );
    }
}
