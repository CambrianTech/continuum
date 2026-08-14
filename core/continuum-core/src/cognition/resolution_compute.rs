//! Compute-depth resolution — the escalation axis that climbs THINKING budget on the
//! ONE resident model instead of swapping models (slice 2 of #168; axis chosen with
//! Joel 2026-07-15).
//!
//! On a single machine the local gateway serves one model and refuses a mismatched
//! `request.model` loud, so "more resolution" cannot mean "a bigger model" locally —
//! it means **more compute spent on the same weights**: a larger generation/thinking
//! budget (and, later, self-consistency samples). A cheap reflexive draft gets a
//! tight budget (answer fast); escalation loosens it (room to reason), re-verified by
//! the objective code verifier. This is the misdirection-budget doctrine made
//! mechanical — the reflexive surface is a small budget, depth is a large one, the
//! same model throughout — and it stays local and non-disruptive (no page-swap of the
//! shared model, no grid hop). Grid-route-to-a-bigger-model is the natural scale-out
//! ceiling on the SAME [`Drafter`]/[`ResolutionLadder`] spine, later.
//!
//! [[conversational-latency-is-a-misdirection-budget]]
//! [[intelligence-is-a-resolution-field-shared-across-the-mesh]]

use super::resolution::{Drafter, ResolutionError, ResolutionLadder};

/// The compute a single draft may spend, derived from a resolution. One knob today
/// (the token ceiling); a `samples` field for self-consistency is the documented
/// growth point, deferred so no sampling policy is invented before it is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputeBudget {
    /// Generation/thinking token ceiling for this draft. Low resolution → tight
    /// ceiling (a fast, shallow answer); high → room to reason before the answer.
    pub max_tokens: u32,
}

/// Produces one draft at a given compute budget on the resident model. The real impl
/// wraps the deliberation faculty / adapter — overriding the per-turn `max_tokens`
/// with the budget — while tests use a mock. Decoupling the backend keeps the
/// resolution→budget mapping unit-testable without standing up live inference.
pub trait DraftBackend: Send + Sync {
    fn generate(
        &self,
        budget: ComputeBudget,
        feedback: Option<&str>,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send;
}

/// Maps a resolution in `[0,1]` onto a token budget between a reflexive floor and the
/// model's full generation window, and drafts via a [`DraftBackend`]. The floor
/// guarantees even the cheapest draft can produce a *complete* short answer (never a
/// truncated stub — a truncation would be a fake verifier failure); resolution `1.0`
/// spends the whole window.
pub struct ComputeDepthDrafter<B: DraftBackend> {
    backend: B,
    min_tokens: u32,
    max_tokens: u32,
}

impl<B: DraftBackend> ComputeDepthDrafter<B> {
    /// `min_tokens` = the reflexive floor (a complete short answer fits); `max_tokens`
    /// = the model's full generation budget (the served completion reserve). BOTH come
    /// from the caller, derived from the live served window — there is no token
    /// constant baked in here. A caller that passes `min > max` is coerced to
    /// `min = max` (a single-budget drafter) rather than panicking.
    pub fn new(backend: B, min_tokens: u32, max_tokens: u32) -> Self {
        let min_tokens = min_tokens.min(max_tokens);
        Self {
            backend,
            min_tokens,
            max_tokens,
        }
    }

    /// The token ceiling this resolution buys — linear from `min_tokens` at res `0` to
    /// `max_tokens` at res `1`.
    pub fn budget_for(&self, resolution: f32) -> ComputeBudget {
        let r = resolution.clamp(0.0, 1.0);
        let span = self.max_tokens.saturating_sub(self.min_tokens) as f32;
        let tokens = self.min_tokens + (span * r).round() as u32;
        ComputeBudget { max_tokens: tokens }
    }
}

impl<B: DraftBackend> Drafter for ComputeDepthDrafter<B> {
    type Draft = String;
    async fn draft(
        &self,
        resolution: f32,
        feedback: Option<&str>,
    ) -> Result<String, ResolutionError> {
        let budget = self.budget_for(resolution);
        self.backend
            .generate(budget, feedback)
            .await
            .map_err(|reason| ResolutionError::DraftFailed { resolution, reason })
    }
}

/// A ladder of compute-depth rungs — normalized resolutions the escalator climbs on
/// the SAME model. The rung COUNT is a granularity of the climb (how many budget
/// steps between the reflexive floor and full compute), NOT a lane/model count: every
/// rung is the one resident model at a different budget, so there is no serving
/// capacity constant here for §6 to forbid. Rungs are evenly spaced in `[lowest, 1.0]`.
pub struct ComputeDepthLadder {
    rungs: Vec<f32>,
}

impl ComputeDepthLadder {
    /// `steps` evenly-spaced rungs from `lowest` (the reflexive starting resolution)
    /// up to `1.0` inclusive. `steps` is clamped to `>= 1`; a single step is the full
    /// budget (no laddering).
    pub fn new(lowest: f32, steps: usize) -> Self {
        let steps = steps.max(1);
        let lowest = lowest.clamp(0.0, 1.0);
        let rungs = if steps == 1 {
            vec![1.0]
        } else {
            (0..steps)
                .map(|i| {
                    let t = i as f32 / (steps - 1) as f32;
                    lowest + (1.0 - lowest) * t
                })
                .collect()
        };
        Self { rungs }
    }
}

impl ResolutionLadder for ComputeDepthLadder {
    fn rungs(&self) -> Vec<f32> {
        self.rungs.clone()
    }
}

// ── The live backend: draft on the resident model at a compute budget ──────────

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ChatMessage, TextGenerationRequest};
use std::sync::Arc;

/// Build the generation request for one compute-depth draft. Pure (no adapter, no
/// I/O) so the resolution→request mapping is unit-testable in isolation. The ONE
/// load-bearing field is `max_tokens = budget.max_tokens` — the compute-depth knob;
/// everything else mirrors what the deliberation faculty sends. On a re-draft the
/// verifier's failing reason is threaded into the user turn so the higher-budget
/// attempt is INFORMED by why the cheaper one fell short (not a blind retry).
fn draft_request(
    model: Option<String>,
    system_prompt: Option<String>,
    task_prompt: &str,
    temperature: f32,
    persona_id: Option<String>,
    budget: ComputeBudget,
    feedback: Option<&str>,
) -> TextGenerationRequest {
    let user = match feedback {
        Some(f) if !f.trim().is_empty() => format!(
            "{task_prompt}\n\nYour previous attempt did NOT pass the tests:\n{f}\n\n\
             Fix it and reply with the corrected solution."
        ),
        _ => task_prompt.to_string(),
    };
    TextGenerationRequest {
        messages: vec![ChatMessage::text("user", user)],
        system_prompt,
        model,
        provider: None,
        // Greedy for a reproducible verifier signal — same reasoning as the eval
        // window in the faculty; the caller may raise it for exploratory sampling.
        temperature: Some(temperature),
        // THE compute-depth knob: the escalator's resolution → this token ceiling.
        max_tokens: Some(budget.max_tokens),
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: Some("resolution-draft".to_string()),
        persona_id,
    }
}

/// [`DraftBackend`] over a live inference adapter — the wiring that makes compute-depth
/// escalation real on the resident model. It re-poses the same coding task at each
/// budget (overriding `max_tokens` with the compute-depth budget), so the escalator's
/// climb spends progressively more thinking room on the SAME weights until the code
/// verifier passes. The adapter/model come from the caller (the faculty's binding, a
/// dedicated eval lane, or later a grid route) — this backend does not choose a model,
/// it spends a budget on the one it was handed.
pub struct FacultyDraftBackend {
    adapter: Arc<dyn AIProviderAdapter>,
    model: Option<String>,
    task_prompt: String,
    system_prompt: Option<String>,
    temperature: f32,
    persona_id: Option<String>,
}

impl FacultyDraftBackend {
    /// `model = None` → the adapter's own resident model (the single-resident local
    /// gateway). `system_prompt` frames the coder (e.g. "reply with a ```rust fenced
    /// solution"). `persona_id` attributes the inference for per-persona resource
    /// accounting (None for a benchmark / eval-lane run).
    pub fn new(
        adapter: Arc<dyn AIProviderAdapter>,
        model: Option<String>,
        task_prompt: impl Into<String>,
        system_prompt: Option<String>,
        temperature: f32,
        persona_id: Option<String>,
    ) -> Self {
        Self {
            adapter,
            model,
            task_prompt: task_prompt.into(),
            system_prompt,
            temperature,
            persona_id,
        }
    }
}

impl DraftBackend for FacultyDraftBackend {
    async fn generate(
        &self,
        budget: ComputeBudget,
        feedback: Option<&str>,
    ) -> Result<String, String> {
        let request = draft_request(
            self.model.clone(),
            self.system_prompt.clone(),
            &self.task_prompt,
            self.temperature,
            self.persona_id.clone(),
            budget,
            feedback,
        );
        self.adapter.generate_text(request).await.map(|r| r.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::resolution::{resolve, Resolved, Verdict, Verifier};
    use crate::cognition::will::Will;

    /// A backend that only produces a solved draft once its budget clears a threshold
    /// — models "more thinking budget → correct code". Records every budget it saw.
    struct BudgetGatedBackend {
        required_tokens: u32,
        seen: std::sync::Mutex<Vec<u32>>,
    }
    impl DraftBackend for BudgetGatedBackend {
        async fn generate(
            &self,
            budget: ComputeBudget,
            _feedback: Option<&str>,
        ) -> Result<String, String> {
            self.seen.lock().unwrap().push(budget.max_tokens);
            if budget.max_tokens >= self.required_tokens {
                Ok("SOLVED".to_string())
            } else {
                Ok("stub".to_string())
            }
        }
    }

    struct SolvedVerifier;
    impl Verifier for SolvedVerifier {
        type Draft = String;
        async fn verify(&self, draft: &String) -> Verdict {
            if draft == "SOLVED" {
                Verdict::pass("solved")
            } else {
                Verdict::fail("stub — needs more compute budget")
            }
        }
    }

    // what this catches: budget_for is a clean linear map from the reflexive floor at
    // res 0 to the full window at res 1 — the reflexive/deep budget contract. A stub
    // truncation floor must never collapse to 0.
    #[test]
    fn budget_for_maps_resolution_linearly_between_floor_and_full() {
        struct Noop;
        impl DraftBackend for Noop {
            async fn generate(
                &self,
                _b: ComputeBudget,
                _f: Option<&str>,
            ) -> Result<String, String> {
                Ok(String::new())
            }
        }
        let d = ComputeDepthDrafter::new(Noop, 200, 1000);
        assert_eq!(
            d.budget_for(0.0).max_tokens,
            200,
            "reflexive floor at res 0"
        );
        assert_eq!(d.budget_for(1.0).max_tokens, 1000, "full window at res 1");
        assert_eq!(d.budget_for(0.5).max_tokens, 600, "halfway");
        // Out-of-range clamps, never underflows below the floor.
        assert_eq!(d.budget_for(-1.0).max_tokens, 200);
        assert_eq!(d.budget_for(9.0).max_tokens, 1000);
    }

    // what this catches (#168 slice 2 live wiring): the compute-depth budget lands on
    // the request as max_tokens — the one load-bearing field — and a re-draft threads
    // the verifier's failing reason into the user turn so the higher-budget attempt is
    // INFORMED, not a blind retry. This is the pure seam the live FacultyDraftBackend
    // sends to the resident model.
    #[test]
    fn draft_request_sets_budget_as_max_tokens_and_threads_feedback() {
        use crate::ai::types::MessageContent;

        let budget = ComputeBudget { max_tokens: 512 };
        let fresh = draft_request(
            Some("resident".into()),
            Some("You are a Rust coder.".into()),
            "Write add(a,b).",
            0.0,
            None,
            budget,
            None,
        );
        assert_eq!(fresh.max_tokens, Some(512), "budget becomes max_tokens");
        assert_eq!(fresh.model.as_deref(), Some("resident"));
        match &fresh.messages[0].content {
            MessageContent::Text(t) => {
                assert!(t.contains("Write add(a,b)."));
                assert!(
                    !t.contains("previous attempt"),
                    "fresh draft carries no feedback"
                );
            }
            other => panic!("expected text message, got {other:?}"),
        }

        let escalated = draft_request(
            None,
            None,
            "Write add(a,b).",
            0.0,
            None,
            ComputeBudget { max_tokens: 2048 },
            Some("assertion `left == right` failed"),
        );
        assert_eq!(escalated.max_tokens, Some(2048), "escalated budget");
        match &escalated.messages[0].content {
            MessageContent::Text(t) => {
                assert!(t.contains("Write add(a,b)."), "still poses the task");
                assert!(
                    t.contains("assertion `left == right` failed"),
                    "threads the verifier's failing reason into the re-draft"
                );
            }
            other => panic!("expected text message, got {other:?}"),
        }
    }

    // what this catches (#168 slice 2, the whole point): the escalator drafting on the
    // compute-depth axis CLIMBS the token budget on the same model until the objective
    // verifier passes — proving the reflexive→deep loop end-to-end without swapping
    // models. A task needing 800 tokens starts cheap, fails at the low budgets, and
    // passes once the climb buys enough compute.
    #[tokio::test]
    async fn escalator_climbs_compute_budget_until_verified() {
        let backend = BudgetGatedBackend {
            required_tokens: 800,
            seen: std::sync::Mutex::new(Vec::new()),
        };
        // Floor 200, full 1000 → rungs 0/.25/.5/.75/1 map to 200/400/600/800/1000.
        let drafter = ComputeDepthDrafter::new(backend, 200, 1000);
        let ladder = ComputeDepthLadder::new(0.0, 5);
        // Bootstrap will starts cheap (start_point ≈ 0.185) and leans on escalation.
        let will = Will::bootstrap();

        let out = resolve(will, &drafter, &SolvedVerifier, &ladder)
            .await
            .unwrap();
        match out {
            Resolved::Passed {
                resolution,
                escalations,
                draft,
                ..
            } => {
                assert_eq!(draft, "SOLVED");
                assert!(
                    escalations >= 1,
                    "cheap budget did not suffice — had to climb"
                );
                assert!(
                    resolution >= 0.75 - 1e-6,
                    "passed at the budget that met 800 tokens"
                );
            }
            other => panic!("expected Passed after climbing compute, got {other:?}"),
        }
        // The climb spent strictly increasing budgets on the same model, ending at the
        // rung that cleared the requirement — the escalation path is monotonic compute.
        let seen = drafter.backend.seen.lock().unwrap().clone();
        assert!(
            seen.windows(2).all(|w| w[0] < w[1]),
            "budgets strictly increased: {seen:?}"
        );
        assert!(
            seen.last().copied().unwrap() >= 800,
            "final budget cleared the requirement"
        );
    }
}
