//! Structural scoring of a `perception/observe` result against a UI spec — the
//! "diff on the element tree" money signal for the functional web-dev benchmark.
//!
//! # One Delta, three uses
//!
//! The [`UiScore`] a set of [`UiCheck`]s produces against an [`ObserveResult`] is
//! the SAME object three times, per the Perception Surface thesis:
//! - a persona's **"did my change actually render?"** self-check,
//! - the **training label** when we capture perceive→act→diff, and
//! - the functional web-dev **benchmark score** (a `Grader` calls this).
//!
//! # Every persona competes on equal footing
//!
//! This scores the **structure** tree (`ProbeNode`: tags, roles, names, text) —
//! which is plain text a non-visual model reads directly, exactly like a VLM. So
//! a lesser local model is judged on the same rendered-UI facts as Claude; no
//! persona is design-blind because of its base model
//! ([[built-to-teach-lesser-tuned-intelligences-win]]). Pixel-level aesthetic
//! judgment (rung 2) rides `imageDiff` + a vision aid later; THIS rung — does the
//! UI have the right elements, labelled correctly — is universal today.
//!
//! Pure + deterministic: no browser, no persona, no serving. A `Grader` runs the
//! persona's output through an eye-node to get the `ObserveResult`, then calls
//! [`score_observation`] here to turn it into a number.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{ObserveResult, ProbeNode};

fn default_min_count() -> u32 {
    1
}

/// One structural acceptance criterion on the observed element tree. A web-dev
/// task's spec is a list of these; the score is the fraction satisfied. All set
/// fields must hold on the SAME node for it to match (AND semantics).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/UiCheck.ts"
)]
pub struct UiCheck {
    /// Human-readable statement of what this asserts — rendered on the scorecard
    /// ("has a Submit button", "shows the heading 'Welcome'").
    pub description: String,
    /// Require this element tag (`button`, `h1`, `input`). Case-insensitive. None
    /// ⇒ any tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tag: Option<String>,
    /// Require this accessibility role (`button`, `heading`, `textbox`).
    /// Case-insensitive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub role: Option<String>,
    /// Require the node's accessible name OR visible text to CONTAIN this
    /// substring (case-insensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text_contains: Option<String>,
    /// Require at least this many matching nodes (default 1) — e.g. "at least 3
    /// list items".
    #[serde(default = "default_min_count")]
    #[ts(type = "number")]
    pub min_count: u32,
}

/// The outcome of one [`UiCheck`] against an observation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/UiCheckResult.ts"
)]
pub struct UiCheckResult {
    pub description: String,
    pub passed: bool,
    /// How many nodes matched (a check needs `>= min_count`).
    #[ts(type = "number")]
    pub matched: u32,
}

/// The functional-correctness score of an observation against a UI spec — the
/// money signal. `score` is `passed / total` in `0.0..=1.0`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/UiScore.ts"
)]
pub struct UiScore {
    #[ts(type = "number")]
    pub passed: u32,
    #[ts(type = "number")]
    pub total: u32,
    /// `passed / total`, or `0.0` when there are no checks.
    pub score: f32,
    /// Per-check breakdown for the scorecard (which criteria a persona met/missed).
    pub results: Vec<UiCheckResult>,
}

/// A functional web-dev benchmark VERDICT — the [`UiScore`] thresholded to
/// pass/fail for the eval runner's binary `(ok, grade)` seam, with the fractional
/// score riding along for a richer scorecard. This is the whole grade decision, so
/// the `cognition/eval` grading arm is a one-liner (`grade_ui(&obs, &checks, thr)`)
/// and the STOP-zone edit stays trivial.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/UiGrade.ts"
)]
pub struct UiGrade {
    /// Did the persona's UI meet the bar (`score >= pass_threshold`, on a real
    /// observation with at least one check)?
    pub passed: bool,
    /// The fraction of checks met, `0.0..=1.0` — the graded diff, not just pass/fail.
    pub score: f32,
    /// Human scorecard line (`"3/4 checks met (score 0.75)"`).
    pub summary: String,
}

/// Grade an observation against a UI spec: score it, then threshold to pass/fail.
/// `pass_threshold` is the fraction required (1.0 = every check must hold; 0.5 =
/// half). A failed/empty observation never passes (it meets no criteria).
pub fn grade_ui(result: &ObserveResult, checks: &[UiCheck], pass_threshold: f32) -> UiGrade {
    let score = score_observation(result, checks);
    let passed = result.success && score.total > 0 && score.score >= pass_threshold;
    UiGrade {
        passed,
        score: score.score,
        summary: format!(
            "{}/{} checks met (score {:.2})",
            score.passed, score.total, score.score
        ),
    }
}

/// Score an observation against a UI spec — the deterministic diff on the element
/// tree. A failed observation (`success == false` / no structure) scores 0 with
/// every check unmatched (the honest floor — a UI that didn't render meets no
/// criteria).
pub fn score_observation(result: &ObserveResult, checks: &[UiCheck]) -> UiScore {
    let mut results = Vec::with_capacity(checks.len());
    let mut passed = 0u32;

    for check in checks {
        let matched = result
            .structure
            .as_ref()
            .map(|root| count_matches(root, check))
            .unwrap_or(0);
        let ok = matched >= check.min_count.max(1);
        if ok {
            passed += 1;
        }
        results.push(UiCheckResult {
            description: check.description.clone(),
            passed: ok,
            matched,
        });
    }

    let total = checks.len() as u32;
    let score = if total == 0 {
        0.0
    } else {
        passed as f32 / total as f32
    };
    UiScore {
        passed,
        total,
        score,
        results,
    }
}

/// Count nodes in the subtree rooted at `node` that satisfy `check`.
fn count_matches(node: &ProbeNode, check: &UiCheck) -> u32 {
    let mut n = u32::from(node_matches(node, check));
    for child in &node.children {
        n += count_matches(child, check);
    }
    n
}

/// Whether a single node satisfies every set field of `check` (AND semantics).
fn node_matches(node: &ProbeNode, check: &UiCheck) -> bool {
    if let Some(tag) = &check.tag {
        if !node.tag.eq_ignore_ascii_case(tag) {
            return false;
        }
    }
    if let Some(role) = &check.role {
        if node.role.as_deref().map(|r| r.eq_ignore_ascii_case(role)) != Some(true) {
            return false;
        }
    }
    if let Some(needle) = &check.text_contains {
        let hay = format!(
            "{} {}",
            node.name.as_deref().unwrap_or(""),
            node.text.as_deref().unwrap_or("")
        )
        .to_lowercase();
        if !hay.contains(&needle.to_lowercase()) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::{ObserveResult, ProbeNode};

    /// A tiny login-form observation: <h1>Sign in</h1>, two inputs, a Submit
    /// button — the shape a "build a login form" web-dev task would check.
    fn login_form() -> ObserveResult {
        fn node(tag: &str, role: Option<&str>, text: Option<&str>) -> ProbeNode {
            ProbeNode {
                tag: tag.to_string(),
                role: role.map(str::to_string),
                name: text.map(str::to_string),
                text: text.map(str::to_string),
                bounds: None,
                attrs: None,
                children: vec![],
            }
        }
        ObserveResult {
            success: true,
            url: Some("file:///tmp/login.html".into()),
            title: Some("Sign in".into()),
            image: None,
            structure: Some(ProbeNode {
                tag: "form".into(),
                role: Some("form".into()),
                name: None,
                text: None,
                bounds: None,
                attrs: None,
                children: vec![
                    node("h1", Some("heading"), Some("Sign in")),
                    node("input", Some("textbox"), Some("Email")),
                    node("input", Some("textbox"), Some("Password")),
                    node("button", Some("button"), Some("Submit")),
                ],
            }),
            error: None,
        }
    }

    // what this catches: the core money signal — a spec that the rendered UI
    // MEETS scores 1.0, with every check matched. This is the persona's
    // "did it render right?", the training label, and the benchmark score.
    #[test]
    fn a_met_spec_scores_full_marks() {
        let checks = vec![
            UiCheck {
                description: "has a heading 'Sign in'".into(),
                tag: Some("h1".into()),
                role: None,
                text_contains: Some("sign in".into()),
                min_count: 1,
            },
            UiCheck {
                description: "has a Submit button".into(),
                tag: None,
                role: Some("button".into()),
                text_contains: Some("submit".into()),
                min_count: 1,
            },
            UiCheck {
                description: "has at least two text inputs".into(),
                tag: Some("input".into()),
                role: None,
                text_contains: None,
                min_count: 2,
            },
        ];
        let score = score_observation(&login_form(), &checks);
        assert_eq!(score.passed, 3);
        assert_eq!(score.total, 3);
        assert_eq!(score.score, 1.0);
        assert!(score.results.iter().all(|r| r.passed));
    }

    // what this catches: partial credit — a missing element fails only its own
    // check, and the score is the fraction met (the graded diff, not pass/fail).
    #[test]
    fn a_missing_element_costs_exactly_its_check() {
        let checks = vec![
            UiCheck {
                description: "has a Submit button".into(),
                tag: None,
                role: Some("button".into()),
                text_contains: Some("submit".into()),
                min_count: 1,
            },
            UiCheck {
                description: "has a 'Forgot password?' link".into(),
                tag: None,
                role: Some("link".into()),
                text_contains: Some("forgot password".into()),
                min_count: 1,
            },
        ];
        let score = score_observation(&login_form(), &checks);
        assert_eq!(score.passed, 1);
        assert_eq!(score.total, 2);
        assert_eq!(score.score, 0.5);
        assert!(score.results[0].passed);
        assert!(
            !score.results[1].passed,
            "the absent link must fail its check"
        );
    }

    // what this catches: a UI that DIDN'T render (failed observation / no
    // structure) meets no criteria — score 0, never a false pass. The honest floor.
    #[test]
    fn a_failed_observation_meets_no_criteria() {
        let failed = ObserveResult {
            success: false,
            url: None,
            title: None,
            image: None,
            structure: None,
            error: Some("page crashed".into()),
        };
        let checks = vec![UiCheck {
            description: "has a Submit button".into(),
            tag: None,
            role: Some("button".into()),
            text_contains: Some("submit".into()),
            min_count: 1,
        }];
        let score = score_observation(&failed, &checks);
        assert_eq!(score.score, 0.0);
        assert_eq!(score.passed, 0);
        assert!(!score.results[0].passed);
    }

    // what this catches: an empty spec is a well-defined 0.0 (no criteria to
    // meet), never a divide-by-zero panic.
    #[test]
    fn an_empty_spec_is_zero_not_a_panic() {
        let score = score_observation(&login_form(), &[]);
        assert_eq!(score.total, 0);
        assert_eq!(score.score, 0.0);
    }

    // what this catches: grade_ui thresholds the fraction to pass/fail — a met
    // spec passes at 1.0; a half-met spec fails at 1.0 but passes at 0.5; the
    // fractional score always rides along. This is the bench's whole verdict.
    #[test]
    fn grade_ui_thresholds_the_score() {
        let met = vec![UiCheck {
            description: "heading 'Sign in'".into(),
            tag: Some("h1".into()),
            role: None,
            text_contains: Some("sign in".into()),
            min_count: 1,
        }];
        let g = grade_ui(&login_form(), &met, 1.0);
        assert!(g.passed);
        assert_eq!(g.score, 1.0);

        let mixed = vec![
            UiCheck {
                description: "has Submit".into(),
                tag: None,
                role: Some("button".into()),
                text_contains: Some("submit".into()),
                min_count: 1,
            },
            UiCheck {
                description: "has 'Forgot password?'".into(),
                tag: None,
                role: Some("link".into()),
                text_contains: Some("forgot".into()),
                min_count: 1,
            },
        ];
        assert!(
            !grade_ui(&login_form(), &mixed, 1.0).passed,
            "half-met fails at 1.0"
        );
        assert!(
            grade_ui(&login_form(), &mixed, 0.5).passed,
            "half-met passes at 0.5"
        );
        assert_eq!(grade_ui(&login_form(), &mixed, 1.0).score, 0.5);
    }

    // what this catches: a UI that didn't render never passes, even at a zero
    // threshold — a crash is not a pass.
    #[test]
    fn grade_ui_never_passes_a_failed_observation() {
        let failed = ObserveResult {
            success: false,
            url: None,
            title: None,
            image: None,
            structure: None,
            error: Some("crashed".into()),
        };
        let checks = vec![UiCheck {
            description: "has Submit".into(),
            tag: None,
            role: Some("button".into()),
            text_contains: None,
            min_count: 1,
        }];
        assert!(!grade_ui(&failed, &checks, 0.0).passed);
    }
}
