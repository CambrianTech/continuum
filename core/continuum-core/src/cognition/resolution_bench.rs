//! `cognition/resolution-bench` — prove the will-driven resolution escalator on the
//! LIVE resident model (#168 slice 2, the end-to-end run).
//!
//! Runs an easy and a hard Rust coding task through the real escalator:
//! [`resolve`](super::resolution::resolve) over a [`ComputeDepthDrafter`] that spends
//! a climbing token budget on the ONE resident model, gated by the objective
//! [`CodeVerifier`](super::gym_grader::CodeVerifier) (real `rustc`). The expected
//! proof: the easy task passes at the cheap reflexive budget (0 escalations); the
//! hard task fails cheap (a long solution truncates / underthinks), climbs compute on
//! the same model, and passes — or exhausts loud with the compiler's reason. Either
//! way the loop is demonstrated on real inference; nothing is faked.
//!
//! Mirrors [`cognition/eval`](super::eval) exactly: it holds the
//! `[[benchmark-is-a-governor-preemption-lease]]` quiesce lease for the whole
//! measurement (the fleet goes quiet so the escalator runs on an uncontended GPU,
//! restored on drop incl. panic), builds a fresh `from_registry` adapter to the
//! resident llama-server, and self-registers as a stateless `DynCommand`. A long run
//! (many generations) can exceed an IPC client timeout, so `detach` fires it on the
//! runtime and reports per-task results to the tracing log (fire-and-poll, #86).
//!
//! [[conversational-latency-is-a-misdirection-budget]]
//! [[intelligence-is-a-resolution-field-shared-across-the-mesh]]

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

use crate::cognition::gym_grader::CodeVerifier;
use crate::cognition::resolution::{resolve, Resolved};
use crate::cognition::resolution_compute::{
    ComputeDepthDrafter, ComputeDepthLadder, FacultyDraftBackend,
};
use crate::ai::adapter::AIProviderAdapter;
use crate::cognition::will::Will;
use crate::inference::llama_server::PROVIDER_ID;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// The coder framing every draft gets — keep the output a clean fenced solution the
/// grader can extract and compile.
const SYSTEM_PROMPT: &str = "You are an expert Rust programmer. Reply with ONLY the \
    solution inside a ```rust code block — no explanation, no `fn main`, no tests.";

/// A benchmark fixture: a prompt + the objective test the grader compiles + runs.
#[derive(Debug, Clone, Copy)]
struct BenchTask {
    id: &'static str,
    prompt: &'static str,
    test: &'static str,
}

/// Trivial — a correct solution fits in a tiny budget, so it should pass at the cheap
/// reflexive rung with zero escalations (the compute-saved case). From
/// `docs/genome/coder-write-eval.jsonl`.
const EASY: BenchTask = BenchTask {
    id: "range_sum",
    prompt: "Write a Rust function with the EXACT signature `fn range_sum(a: i64, b: i64) -> i64` \
             that returns the sum of all integers from a to b inclusive (assume a <= b). Reply with \
             ONLY the function inside a ```rust code block — no `fn main`, no tests.",
    test: "assert_eq!(range_sum(1,5), 15); assert_eq!(range_sum(3,3), 3);",
};

/// Long + non-trivial — the correct solution overflows the cheap budget (truncates /
/// underthinks) and needs the climb. From `docs/genome/hard-rs.jsonl`.
const HARD: BenchTask = BenchTask {
    id: "expr_eval",
    prompt: "Implement `pub fn eval(expr: &str) -> i64`: evaluate an integer arithmetic expression \
             supporting + - * / (integer division), parentheses, correct operator precedence (* / \
             bind tighter than + -), left-associativity, and arbitrary whitespace. No external \
             crates. Return the complete function(s) inside a ```rust code block.",
    test: "assert_eq!(eval(\"3 + 4 * 2\"), 11);\n    assert_eq!(eval(\"(3 + 4) * 2\"), 14);\n    \
           assert_eq!(eval(\"10 - 2 - 3\"), 5);\n    assert_eq!(eval(\"2 * (3 + (4 - 1))\"), 12);\n    \
           assert_eq!(eval(\"100 / 5 / 2\"), 10);\n    assert_eq!(eval(\"  7  +  3 * 2 \"), 13);\n    \
           assert_eq!(eval(\"2 * 3 + 4 * 5\"), 26);",
};

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct ResolutionBenchParams {
    /// Resident model to draft on. None → the adapter's own default (whatever the
    /// llama-server on 58057 currently serves). Must be a served id if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_id: Option<String>,
    /// Ladder granularity — how many evenly-spaced compute-budget rungs between the
    /// reflexive floor and full. Default 4. This is the climb's resolution, not a
    /// serving-capacity constant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub steps: Option<u32>,
    /// The reflexive floor budget (tokens) — the cheapest draft. Small enough that a
    /// long solution truncates here (so the verifier escalates), large enough for a
    /// complete short answer. Default 160. The benchmark's independent variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub min_tokens: Option<u32>,
    /// The full-resolution budget (tokens) — the deepest draft. Default 2048.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_tokens: Option<u32>,
    /// Fire-and-poll (#86): a multi-generation run can outlast an IPC timeout. `true`
    /// spawns the run on the runtime and returns immediately; per-task results land in
    /// the tracing log (`resolution-bench task complete`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS, Default)]
pub struct ResolutionTaskOutcome {
    pub id: String,
    /// Did the objective verifier (real rustc) pass at the resolution reached.
    pub passed: bool,
    /// The operating point (0..1) that settled the task — low = cheap draft sufficed,
    /// high = had to climb.
    pub resolution: f64,
    /// How many times the escalator had to climb (0 = the first cheap draft passed).
    #[ts(type = "number")]
    pub escalations: u32,
    /// The token budget spent at the settling resolution — the concrete compute the
    /// climb bought.
    #[ts(type = "number")]
    pub final_budget_tokens: u32,
    /// Wall-clock for the whole escalation of this task (all rungs).
    #[ts(type = "number")]
    pub latency_ms: u64,
    /// The verifier's last word — "tests passed" or the compiler/panic reason.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, TS, Default)]
pub struct ResolutionBenchResult {
    /// True = a fire-and-poll job handle (#86); `tasks` is empty, real results are in
    /// the tracing log.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detached: bool,
    pub tasks: Vec<ResolutionTaskOutcome>,
}

#[derive(Default)]
pub struct ResolutionBench;

#[async_trait]
impl ActionCommand for ResolutionBench {
    const NAME: &'static str = "cognition/resolution-bench";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Prove the will-driven resolution escalator on the resident model: draft an easy + a hard \
         Rust task at a climbing compute budget on ONE model, gated by real rustc; easy passes \
         cheap, hard escalates. Holds the fleet quiesce lease for the measurement. Use detach for \
         long runs (results in the tracing log).";
    type Params = ResolutionBenchParams;
    type Output = ResolutionBenchResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: ResolutionBenchParams,
    ) -> Result<ResolutionBenchResult, CommandError> {
        if p.detach.unwrap_or(false) {
            tokio::spawn(async move {
                match ResolutionBench::run_bench(p).await {
                    Ok(r) => tracing::info!(
                        tasks = r.tasks.len(),
                        "resolution-bench detached run complete"
                    ),
                    Err(e) => tracing::error!(error = %e, "resolution-bench detached run failed"),
                }
            });
            return Ok(ResolutionBenchResult {
                detached: true,
                tasks: vec![],
            });
        }
        ResolutionBench::run_bench(p).await
    }
}

impl ResolutionBench {
    /// The measurement body — ctx-free (builds its own adapter, reaches the fleet only
    /// through the global registry) so it runs inline OR spawned detached, one path.
    async fn run_bench(p: ResolutionBenchParams) -> Result<ResolutionBenchResult, CommandError> {
        // Preemption lease: quiesce the whole live fleet for the measurement so the
        // escalator runs on an uncontended GPU; restored on every exit path (Drop rides
        // the unwind). None → no live fleet (tests) → measure as-is.
        // [[benchmark-is-a-governor-preemption-lease]] [[first-class-citizens-even-during-benchmarks]]
        let _fleet_lease = match crate::persona::PersonaAircRuntimeRegistry::try_global() {
            Some(r) => {
                let lease = r.quiesce_all();
                tracing::info!(
                    personas = lease.count(),
                    "resolution-bench: fleet quiesced for the measurement"
                );
                Some(lease)
            }
            None => {
                tracing::warn!("resolution-bench: no live fleet to quiesce — measuring as-is");
                None
            }
        };

        // Fresh adapter to the resident llama-server (from_registry carries the 58057
        // default base_url). No dedicated-lane override: we WANT the resident serving
        // snapshot to accept these generations against the live model.
        let mut adapter = crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(PROVIDER_ID);
        if let Some(m) = p.model_id.as_ref() {
            adapter = adapter.with_default_model(m.clone());
        }
        adapter.initialize().await.map_err(|e| {
            CommandError::Internal(format!("resolution-bench adapter failed to initialize: {e}"))
        })?;
        let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> = Arc::new(adapter);

        let steps = p.steps.unwrap_or(4).max(1) as usize;
        let min_tokens = p.min_tokens.unwrap_or(160);
        let max_tokens = p.max_tokens.unwrap_or(2048);

        let mut tasks = Vec::new();
        for task in [EASY, HARD] {
            let backend = FacultyDraftBackend::new(
                adapter.clone(),
                p.model_id.clone(),
                task.prompt,
                Some(SYSTEM_PROMPT.to_string()),
                0.0,
                None,
            );
            let drafter = ComputeDepthDrafter::new(backend, min_tokens, max_tokens);
            let verifier = CodeVerifier::new("rust", task.test);
            let ladder = ComputeDepthLadder::new(0.0, steps);

            let t0 = std::time::Instant::now();
            // Bound the whole per-task escalation. A single draft against a slow or
            // wedged lane (a mid-run serving re-home, a degenerate long generation)
            // must never hang forever — while the fleet quiesce lease is held, a hung
            // draft would pin every citizen silent (glass-boxed 2026-07-15: a hard-task
            // draft hung ~20min, suppressing 4 personas). On timeout we fail LOUD and
            // return, dropping the lease (Drop) so the fleet is restored immediately.
            // [[fallbacks-are-illegal-fail-loud]] [[first-class-citizens-even-during-benchmarks]]
            const PER_TASK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(240);
            let resolved = match tokio::time::timeout(
                PER_TASK_TIMEOUT,
                resolve(Will::bootstrap(), &drafter, &verifier, &ladder),
            )
            .await
            {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    return Err(CommandError::Internal(format!(
                        "resolution-bench escalator failed for '{}': {e}",
                        task.id
                    )))
                }
                Err(_) => {
                    return Err(CommandError::Internal(format!(
                        "resolution-bench '{}' exceeded {}s — the draft lane is too slow or wedged; \
                         releasing the fleet quiesce lease",
                        task.id,
                        PER_TASK_TIMEOUT.as_secs()
                    )))
                }
            };
            let latency_ms = t0.elapsed().as_millis() as u64;

            let outcome = match resolved {
                Resolved::Passed {
                    resolution,
                    escalations,
                    verdict,
                    ..
                } => ResolutionTaskOutcome {
                    id: task.id.to_string(),
                    passed: true,
                    resolution: resolution as f64,
                    escalations,
                    final_budget_tokens: drafter.budget_for(resolution).max_tokens,
                    latency_ms,
                    detail: verdict.detail,
                },
                Resolved::Exhausted {
                    resolution,
                    escalations,
                    verdict,
                    ..
                } => ResolutionTaskOutcome {
                    id: task.id.to_string(),
                    passed: false,
                    resolution: resolution as f64,
                    escalations,
                    final_budget_tokens: drafter.budget_for(resolution).max_tokens,
                    latency_ms,
                    detail: verdict.detail,
                },
            };

            tracing::info!(
                task = %outcome.id,
                passed = outcome.passed,
                resolution = outcome.resolution,
                escalations = outcome.escalations,
                budget = outcome.final_budget_tokens,
                latency_ms = outcome.latency_ms,
                detail = %outcome.detail,
                "resolution-bench task complete"
            );
            tasks.push(outcome);
        }

        Ok(ResolutionBenchResult {
            detached: false,
            tasks,
        })
    }
}

crate::register_stateless_command!(ResolutionBench);
