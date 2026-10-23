//! StyleCheck — MEASURED-CRAFT criteria over an observation's craft facts
//! (design-bench V2: the SWE tier of visual work).
//!
//! [`UiCheck`](super::scoring::UiCheck) asserts per-node facts with AND
//! semantics (tag/role/text/contrast on the SAME node). The V2 craft criteria
//! are TREE-level: rhythm is a relation BETWEEN siblings, hierarchy a relation
//! BETWEEN heading levels, reflow a relation between every node and the
//! viewport. Those don't fit a per-node matcher, so they live here — graded
//! from the same `ObserveResult` (rects + the style subset PR #2397 added),
//! producing the same scorecard shapes (`UiCheckResult` rows), so the eval's
//! scorecard renders one uniform list.
//!
//! Honesty rail (same law as `min_contrast`): a criterion that cannot be
//! MEASURED on this observation (no bounds, no style facts, too few samples)
//! is UNMET, never green-lit. Theme discipline (light AND dark passes) is
//! eval-side orchestration — two observations, the same checks against each —
//! so it needs no kind here.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::scoring::{UiCheckResult, UiScore};
use super::{ObserveResult, ProbeNode};

/// One measured-craft criterion over the whole observed tree.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/StyleCheck.ts"
)]
pub struct StyleCheck {
    /// Human-readable statement — rendered on the scorecard.
    pub description: String,
    /// Which craft relation this asserts.
    #[serde(flatten)]
    pub kind: StyleCheckKind,
}

/// The craft relations of the V2 tier. Tagged so a task file reads as prose:
/// `{"description": "...", "kind": "rhythm", "scalePx": [8], "tolerancePx": 2}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/perception/StyleCheckKind.ts"
)]
pub enum StyleCheckKind {
    /// Vertical spacing between visible siblings is drawn from a declared
    /// scale: every gap must sit within `tolerance_px` of an integer multiple
    /// of some scale step. Needs at least `min_gaps` measurable gaps or the
    /// criterion is unmet (a page with one element has no rhythm to attest).
    #[serde(rename_all = "camelCase")]
    Rhythm {
        /// The spacing scale's base steps in px (e.g. [8.0] for an 8px grid).
        scale_px: Vec<f32>,
        /// How far a gap may sit from the scale (px).
        tolerance_px: f32,
        /// Minimum measurable sibling gaps for the criterion to be attestable.
        #[serde(default = "default_min_gaps")]
        #[ts(type = "number")]
        min_gaps: u32,
    },
    /// Type hierarchy holds: computed font-size strictly decreases along the
    /// heading ladder that is PRESENT (h1 > h2 > h3 > …), and every present
    /// heading is at least as large as body (`p`) text. Needs h1 plus at
    /// least one comparison partner, else unmet.
    #[serde(rename_all = "camelCase")]
    Hierarchy {},
    /// Reflow discipline at this observation's width: no visible node's box
    /// extends past `viewport_width` (the "no horizontal overflow" fact —
    /// graded per-viewport by observing at each width the task declares).
    #[serde(rename_all = "camelCase")]
    NoHorizontalOverflow {
        #[ts(type = "number")]
        viewport_width: f32,
        /// Sub-pixel/scrollbar forgiveness (px).
        #[serde(default = "default_overflow_slack")]
        tolerance_px: f32,
    },
}

fn default_min_gaps() -> u32 {
    2
}
fn default_overflow_slack() -> f32 {
    1.0
}

/// Grade a set of style checks against one observation — same scorecard
/// shapes as `score_observation`, so the two check families render as one
/// list. A failed/empty observation meets nothing.
pub fn score_style(result: &ObserveResult, checks: &[StyleCheck]) -> UiScore {
    let root = if result.success { result.structure.as_ref() } else { None };
    let results: Vec<UiCheckResult> = checks
        .iter()
        .map(|c| {
            let passed = root.is_some_and(|r| check_holds(r, &c.kind));
            UiCheckResult {
                description: c.description.clone(),
                passed,
                matched: u32::from(passed),
            }
        })
        .collect();
    let passed = results.iter().filter(|r| r.passed).count() as u32;
    let total = results.len() as u32;
    UiScore {
        passed,
        total,
        score: if total == 0 { 0.0 } else { passed as f32 / total as f32 },
        results,
    }
}

fn check_holds(root: &ProbeNode, kind: &StyleCheckKind) -> bool {
    match kind {
        StyleCheckKind::Rhythm {
            scale_px,
            tolerance_px,
            min_gaps,
        } => rhythm_holds(root, scale_px, *tolerance_px, *min_gaps),
        StyleCheckKind::Hierarchy {} => hierarchy_holds(root),
        StyleCheckKind::NoHorizontalOverflow {
            viewport_width,
            tolerance_px,
        } => no_overflow(root, *viewport_width, *tolerance_px),
    }
}

/// Every measurable vertical gap between box-bearing siblings sits within
/// tolerance of an integer multiple of some scale step. Unmet when fewer than
/// `min_gaps` gaps are measurable — no rhythm without repetition.
fn rhythm_holds(root: &ProbeNode, scale_px: &[f32], tolerance_px: f32, min_gaps: u32) -> bool {
    if scale_px.iter().all(|s| *s <= 0.0) {
        return false; // a degenerate scale measures nothing — unmet, never trivially true
    }
    let mut gaps: Vec<f32> = Vec::new();
    collect_sibling_gaps(root, &mut gaps);
    if (gaps.len() as u32) < min_gaps {
        return false;
    }
    gaps.iter().all(|gap| {
        scale_px.iter().any(|step| {
            if *step <= 0.0 {
                return false;
            }
            let multiple = (gap / step).round();
            multiple >= 0.0 && (gap - multiple * step).abs() <= tolerance_px
        })
    })
}

fn collect_sibling_gaps(node: &ProbeNode, out: &mut Vec<f32>) {
    let boxed: Vec<&ProbeNode> = node
        .children
        .iter()
        .filter(|c| c.bounds.is_some_and(|b| b.height > 0.0))
        .collect();
    for pair in boxed.windows(2) {
        let (a, b) = (
            pair[0].bounds.expect("filtered on is_some above"), // filter guarantees bounds; windows(2) preserves it
            pair[1].bounds.expect("filtered on is_some above"), // same guarantee, second element
        );
        let gap = b.y - (a.y + a.height);
        // Overlapping or horizontally-flowing siblings have no vertical rhythm
        // to attest — skip, don't fail: rhythm is about the stacked flow.
        if gap >= 0.0 {
            out.push(gap);
        }
    }
    for c in &node.children {
        collect_sibling_gaps(c, out);
    }
}

/// Font-size strictly decreases along the PRESENT heading ladder, and every
/// present heading ≥ body text. Unmet without h1 + at least one partner.
fn hierarchy_holds(root: &ProbeNode) -> bool {
    let mut sizes: [Option<f32>; 6] = [None; 6];
    let mut body: Option<f32> = None;
    collect_type_sizes(root, &mut sizes, &mut body);
    let Some(h1) = sizes[0] else { return false };
    let mut partners = 0;
    let mut prev = h1;
    for s in sizes.iter().skip(1).flatten() {
        partners += 1;
        if *s >= prev {
            return false;
        }
        prev = *s;
    }
    if let Some(b) = body {
        partners += 1;
        if sizes.iter().flatten().any(|h| *h < b) {
            return false;
        }
    }
    partners > 0
}

fn collect_type_sizes(node: &ProbeNode, sizes: &mut [Option<f32>; 6], body: &mut Option<f32>) {
    let px = node
        .style
        .as_ref()
        .and_then(|s| s.get("font-size"))
        .and_then(|v| v.trim_end_matches("px").trim().parse::<f32>().ok());
    if let Some(px) = px {
        let tag = node.tag.to_ascii_lowercase();
        if let Some(level) = tag
            .strip_prefix('h')
            .and_then(|d| d.parse::<usize>().ok())
            .filter(|l| (1..=6).contains(l))
        {
            // First of each level wins — the page's declared ladder, not a
            // stray styled heading deep in a card.
            if sizes[level - 1].is_none() {
                sizes[level - 1] = Some(px);
            }
        } else if tag == "p" && body.is_none() {
            *body = Some(px);
        }
    }
    for c in &node.children {
        collect_type_sizes(c, sizes, body);
    }
}

/// No visible box extends past the viewport width (within slack).
fn no_overflow(root: &ProbeNode, viewport_width: f32, tolerance_px: f32) -> bool {
    fn walk(n: &ProbeNode, w: f32, tol: f32) -> bool {
        if let Some(b) = n.bounds {
            if b.width > 0.0 && b.x + b.width > w + tol {
                return false;
            }
        }
        n.children.iter().all(|c| walk(c, w, tol))
    }
    // A tree with no boxes at all cannot attest reflow — unmet.
    fn any_box(n: &ProbeNode) -> bool {
        n.bounds.is_some_and(|b| b.width > 0.0) || n.children.iter().any(any_box)
    }
    any_box(root) && walk(root, viewport_width, tolerance_px)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::ProbeBox;

    fn boxed(tag: &str, x: f32, y: f32, w: f32, h: f32, font_px: Option<f32>) -> ProbeNode {
        ProbeNode {
            tag: tag.into(),
            role: None,
            name: None,
            text: None,
            bounds: Some(ProbeBox {
                x,
                y,
                width: w,
                height: h,
            }),
            style: font_px.map(|px| {
                [("font-size".to_string(), format!("{px}px"))]
                    .into_iter()
                    .collect()
            }),
            attrs: None,
            children: vec![],
        }
    }

    fn tree(children: Vec<ProbeNode>) -> ProbeNode {
        ProbeNode {
            tag: "body".into(),
            role: None,
            name: None,
            text: None,
            bounds: Some(ProbeBox {
                x: 0.0,
                y: 0.0,
                width: 360.0,
                height: 800.0,
            }),
            style: None,
            attrs: None,
            children,
        }
    }

    // what this catches: the three V2 craft relations end-to-end at the tree
    // level, each with its honesty rail — rhythm on an 8px scale passes and a
    // ragged gap fails (and one element = unmeasurable = unmet); hierarchy
    // fails when h2 ≥ h1 and is unmet with h1 alone; overflow catches a box
    // past the viewport and an empty tree attests nothing.
    #[test]
    fn craft_relations_measure_and_never_pass_unmeasured() {
        // Rhythm: gaps 16 and 24 on an 8px scale pass; 16 and 21 fail.
        let rhythmic = tree(vec![
            boxed("div", 0.0, 0.0, 300.0, 40.0, None),
            boxed("div", 0.0, 56.0, 300.0, 40.0, None), // gap 16
            boxed("div", 0.0, 120.0, 300.0, 40.0, None), // gap 24
        ]);
        let ragged = tree(vec![
            boxed("div", 0.0, 0.0, 300.0, 40.0, None),
            boxed("div", 0.0, 56.0, 300.0, 40.0, None), // gap 16
            boxed("div", 0.0, 117.0, 300.0, 40.0, None), // gap 21 — off-scale
        ]);
        let lone = tree(vec![boxed("div", 0.0, 0.0, 300.0, 40.0, None)]);
        assert!(rhythm_holds(&rhythmic, &[8.0], 2.0, 2));
        assert!(!rhythm_holds(&ragged, &[8.0], 2.0, 2));
        assert!(!rhythm_holds(&lone, &[8.0], 2.0, 2), "no repetition, no rhythm — unmet");
        assert!(!rhythm_holds(&rhythmic, &[0.0], 2.0, 2), "degenerate scale is unmeasurable");

        // Hierarchy: 32 > 24 > 16 body passes; inverted h2 fails; h1 alone unmet.
        let laddered = tree(vec![
            boxed("h1", 0.0, 0.0, 300.0, 40.0, Some(32.0)),
            boxed("h2", 0.0, 60.0, 300.0, 30.0, Some(24.0)),
            boxed("p", 0.0, 100.0, 300.0, 20.0, Some(16.0)),
        ]);
        let inverted = tree(vec![
            boxed("h1", 0.0, 0.0, 300.0, 40.0, Some(24.0)),
            boxed("h2", 0.0, 60.0, 300.0, 30.0, Some(32.0)),
        ]);
        let alone = tree(vec![boxed("h1", 0.0, 0.0, 300.0, 40.0, Some(32.0))]);
        assert!(hierarchy_holds(&laddered));
        assert!(!hierarchy_holds(&inverted));
        assert!(!hierarchy_holds(&alone), "no partner, no hierarchy — unmet");

        // Reflow: a 500px-wide box at 360 viewport fails; in-bounds passes;
        // a boxless tree attests nothing.
        let overflowing = tree(vec![boxed("table", 0.0, 0.0, 500.0, 100.0, None)]);
        let contained = tree(vec![boxed("div", 0.0, 0.0, 344.0, 100.0, None)]);
        let mut boxless = tree(vec![]);
        boxless.bounds = None;
        assert!(!no_overflow(&overflowing, 360.0, 1.0));
        assert!(no_overflow(&contained, 360.0, 1.0));
        assert!(!no_overflow(&boxless, 360.0, 1.0), "no boxes, no reflow attestation");

        // The scorecard seam: one failed + one passed check → 1/2.
        let obs = crate::perception::ObserveResult {
            success: true,
            url: None,
            title: None,
            image: None,
            structure: Some(overflowing),
            error: None,
        };
        let checks = vec![
            StyleCheck {
                description: "no horizontal overflow at 360".into(),
                kind: StyleCheckKind::NoHorizontalOverflow {
                    viewport_width: 360.0,
                    tolerance_px: 1.0,
                },
            },
            StyleCheck {
                description: "no horizontal overflow at 1440".into(),
                kind: StyleCheckKind::NoHorizontalOverflow {
                    viewport_width: 1440.0,
                    tolerance_px: 1.0,
                },
            },
        ];
        let score = score_style(&obs, &checks);
        assert_eq!((score.passed, score.total), (1, 2));
    }
}
