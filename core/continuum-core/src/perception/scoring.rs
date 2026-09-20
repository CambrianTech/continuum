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
    /// Require the node's text/background CONTRAST RATIO (WCAG relative
    /// luminance, 1.0..=21.0) to be at least this — the first MEASURED-CRAFT
    /// criterion (design-bench V2 tier; 4.5 = WCAG AA body text). Needs the
    /// observation's `style` craft facts; a node with no measurable pair does
    /// not match, so an adapter that omits styles fails the check LOUDLY on
    /// the scorecard instead of green-lighting unmeasured craft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub min_contrast: Option<f32>,
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
    if let Some(min) = check.min_contrast {
        match node_contrast(node) {
            Some(ratio) if ratio >= min => {}
            _ => return false, // unmeasurable = unmet — never green-light unmeasured craft
        }
    }
    true
}

/// The node's text/background contrast ratio from its style craft facts, when
/// both colors are present and parseable. `None` = not measurable on this node.
fn node_contrast(node: &ProbeNode) -> Option<f32> {
    let style = node.style.as_ref()?;
    let fg = parse_css_color(style.get("color")?)?;
    let bg = parse_css_color(style.get("background-color")?)?;
    // A fully transparent background paints nothing — the visible ground is an
    // ancestor's, which THIS node cannot attest. Not measurable here.
    if bg.3 == 0.0 {
        return None;
    }
    Some(contrast_ratio(fg, bg))
}

/// Parse the CSS color forms `getComputedStyle` actually emits — `rgb(r, g, b)`
/// and `rgba(r, g, b, a)` — plus `#rrggbb` for hand-written fixtures. Anything
/// else (gradients, named colors the browser would have resolved anyway) is
/// `None`: unparseable means unmeasurable, never a guessed color.
fn parse_css_color(s: &str) -> Option<(f32, f32, f32, f32)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some((r as f32, g as f32, b as f32, 1.0));
        }
        return None;
    }
    let inner = s
        .strip_prefix("rgba(")
        .or_else(|| s.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let r: f32 = parts[0].parse().ok()?;
    let g: f32 = parts[1].parse().ok()?;
    let b: f32 = parts[2].parse().ok()?;
    let a: f32 = if parts.len() > 3 { parts[3].parse().ok()? } else { 1.0 };
    Some((r, g, b, a))
}

/// WCAG 2.x contrast ratio between two colors: `(L_lighter + 0.05) /
/// (L_darker + 0.05)` over relative luminance — 1.0 (identical) to 21.0
/// (black on white). The official formula, not an approximation, so a 4.5
/// threshold in a check MEANS WCAG AA.
fn contrast_ratio(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn relative_luminance(c: (f32, f32, f32, f32)) -> f32 {
    fn chan(v: f32) -> f32 {
        let v = v / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * chan(c.0) + 0.7152 * chan(c.1) + 0.0722 * chan(c.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the first MEASURED-CRAFT criterion end-to-end at the
    // scoring seam (design-bench V2, build-order outlier test): the WCAG formula
    // is the official one (black-on-white = 21:1, identical = 1:1), a passing
    // pair passes, a failing pair fails, and — the honesty rail — a node with
    // NO style facts (older adapter, scene surface, static-html) can never
    // green-light a contrast check.
    #[test]
    fn contrast_criterion_measures_wcag_and_never_passes_unmeasured() {
        let styled = |fg: &str, bg: &str| ProbeNode {
            tag: "p".into(),
            role: None,
            name: None,
            text: Some("body copy".into()),
            bounds: None,
            style: Some(
                [("color".to_string(), fg.to_string()),
                 ("background-color".to_string(), bg.to_string())]
                .into_iter()
                .collect(),
            ),
            attrs: None,
            children: vec![],
        };
        let check = UiCheck {
            description: "AA body contrast".into(),
            tag: Some("p".into()),
            role: None,
            text_contains: None,
            min_count: 1,
            min_contrast: Some(4.5),
        };
        // The official anchors.
        let black_on_white = node_contrast(&styled("rgb(0, 0, 0)", "rgb(255, 255, 255)")).expect("measurable");
        assert!((black_on_white - 21.0).abs() < 0.01, "black-on-white is 21:1, got {black_on_white}");
        let identical = node_contrast(&styled("#808080", "#808080")).expect("measurable");
        assert!((identical - 1.0).abs() < 0.001, "identical colors are 1:1");
        // Pass and fail through the real matcher.
        assert!(node_matches(&styled("rgb(0, 0, 0)", "rgb(255, 255, 255)"), &check));
        assert!(!node_matches(&styled("rgb(200, 200, 200)", "rgb(255, 255, 255)"), &check),
            "light-grey-on-white fails AA");
        // Honesty rails: no style, transparent background, unparseable color — all UNMET.
        let mut bare = styled("rgb(0, 0, 0)", "rgb(255, 255, 255)");
        bare.style = None;
        assert!(!node_matches(&bare, &check), "no craft facts must never pass a craft check");
        assert!(!node_matches(&styled("rgb(0, 0, 0)", "rgba(0, 0, 0, 0)"), &check),
            "transparent background is not measurable on this node");
        assert!(!node_matches(&styled("linear-gradient(red, blue)", "rgb(255, 255, 255)"), &check),
            "unparseable color is unmeasurable, never guessed");
        // A check with no min_contrast is untouched by all of this.
        let plain = UiCheck { min_contrast: None, ..check };
        assert!(node_matches(&bare, &plain));
    }
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
                style: None,
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
                style: None,
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
            min_contrast: None,
            },
            UiCheck {
                description: "has a Submit button".into(),
                tag: None,
                role: Some("button".into()),
                text_contains: Some("submit".into()),
                min_count: 1,
            min_contrast: None,
            },
            UiCheck {
                description: "has at least two text inputs".into(),
                tag: Some("input".into()),
                role: None,
                text_contains: None,
                min_count: 2,
            min_contrast: None,
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
            min_contrast: None,
            },
            UiCheck {
                description: "has a 'Forgot password?' link".into(),
                tag: None,
                role: Some("link".into()),
                text_contains: Some("forgot password".into()),
                min_count: 1,
            min_contrast: None,
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
            min_contrast: None,
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
            min_contrast: None,
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
            min_contrast: None,
            },
            UiCheck {
                description: "has 'Forgot password?'".into(),
                tag: None,
                role: Some("link".into()),
                text_contains: Some("forgot".into()),
                min_count: 1,
            min_contrast: None,
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
            min_contrast: None,
        }];
        assert!(!grade_ui(&failed, &checks, 0.0).passed);
    }
}
