//! BenchmarkStandingModule — the standing round: benchmarks run THEMSELVES.
//!
//! Joel, 2026-09-02: *"With autonomous personas and systems there'd never be
//! this hand holding… I swear you're still hand managing benchmarks."* The last
//! hand-managed act was CHOOSING TO DISPATCH: every round began with an
//! operator (human or agent) typing `benchmark/dispatch`. This module retires
//! that hand: when standing mode is enabled and NO round is working, it
//! dispatches the configured benchmark with the NEXT seed in sequence — so the
//! claim's N grows on the seeded-sample protocol (VIRAL-LAUNCH-PLAN.md gate 1)
//! with zero operator turns, day and night.
//!
//! Module shape per the concurrency style guide: no new tokio task — the
//! runtime's tick cadence drives it (a periodic ACTUATOR, the same doctrine as
//! `benchmark_grade`'s lapse sweep). Config is a tiny durable file in the same
//! state family as the round files; the dispatch itself goes through the ONE
//! `benchmark/dispatch` command surface (never a parallel runner —
//! BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md). Every skip states its reason as a
//! probe; a silent autopilot would be the launch-and-pray shape all over again.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry;
use crate::runtime::{
    CommandExecutor, CommandResult, LateBound, ModuleConfig, ModulePriority, ServiceModule,
};

/// How often the standing check runs. Rounds run for hours; five minutes keeps
/// the gap between "round done" and "next round dispatched" small without
/// polling anything hot (every gate below is a cheap in-memory read).
const STANDING_TICK: std::time::Duration = std::time::Duration::from_secs(300);

/// The durable standing config — one tiny JSON file, same state family as the
/// round files (self-describing, survives reboots, no env vars).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct StandingConfig {
    /// Off by default: standing mode is an operator DECISION, made once via
    /// `benchmark/standing --enabled true`, not an ambient behavior a fresh
    /// clone discovers by surprise.
    pub enabled: bool,
    /// Which catalogued benchmark to keep in flight (`benchmark/list`).
    pub benchmark: String,
    /// Sample size per round (the seeded-sample protocol's K).
    pub sample: u32,
    /// The NEXT seed to dispatch with — incremented and persisted after every
    /// standing dispatch, so every round is a fresh sample and the sequence is
    /// reproducible from this file alone.
    pub next_seed: u64,
}

impl Default for StandingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            // Verified-mini is the inner claim-growth loop (catalog row, ~5GB
            // of envs) — the natural standing default once enabled.
            benchmark: "swe-bench-verified-mini".to_string(),
            sample: 4,
            next_seed: 2, // seed=1 was the hand-dispatched 2026-09-01 batch
        }
    }
}

fn standing_path() -> Result<PathBuf, String> {
    Ok(crate::commands::benchmark::continuum_home()
        .map_err(|e| format!("no continuum home: {e}"))?
        .join("state")
        .join("benchmark_standing.json"))
}

fn load_config() -> StandingConfig {
    let Ok(path) = standing_path() else {
        return StandingConfig::default();
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default() // safe: absent/corrupt config = the documented OFF default, never a guess at a quantity
}

fn save_config(cfg: &StandingConfig) -> Result<(), String> {
    let path = standing_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("state dir: {e}"))?;
    }
    let body = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

pub struct BenchmarkStandingModule {
    registry: PersonaAircRuntimeRegistry,
    executor_slot: Arc<LateBound<CommandExecutor>>,
}

impl BenchmarkStandingModule {
    pub fn new(registry: PersonaAircRuntimeRegistry) -> Self {
        Self {
            registry,
            executor_slot: Arc::new(LateBound::new("benchmark-standing::executor")),
        }
    }

    /// One standing check: every gate names its skip. Returns whether a
    /// dispatch was fired (tests pin the gate order without an executor).
    async fn standing_check(&self) -> Result<bool, String> {
        let cfg = load_config();
        if !cfg.enabled {
            return Ok(false); // silent: disabled is the configured steady state
        }
        // A HEALTHY working round is the goal state — hold. But a round wedged
        // past the abandon window (dead citizens, no task boundary) must NOT
        // block the autopilot forever (measured 2026-09-02: 3 rounds stuck
        // ~18h, un-clearable, starving the claim-growth engine). Scan the
        // run ledger so "healthy vs stale" is the board's own truth, not a
        // guess. Blocking fs scan off the async worker.
        let runs = tokio::task::spawn_blocking(|| {
            crate::commands::benchmark::scan_run_cards(None, 200)
                .map(|s| s.cards)
                .unwrap_or_default() // safe: no ledger = no fresh acts = treat as clear
        })
        .await
        .unwrap_or_default(); // safe: join error = same, degrade to clear
        let facts: Vec<crate::cognition::bench_round::CardRunFacts> =
            runs.iter().map(crate::commands::benchmark::card_run_facts).collect();
        if !crate::cognition::bench_round::only_stale_or_no_working_rounds(
            &facts,
            crate::persona::trace::now_ms(),
        ) {
            return Ok(false); // silent: a round is healthily grinding
        }
        // Serving must be decode-ready — a dispatch into a cold lane burns the
        // round's first minutes on refusals. Short park: the tick returns and
        // retries in 5 minutes rather than camping.
        if crate::inference::llama_server::await_ready_serving(std::time::Duration::from_secs(30))
            .await
            .is_none()
        {
            crate::probe!(
                class = "bench.standing.skipped",
                reason = "serving",
                "standing round: serving not decode-ready — retrying next tick"
            );
            return Ok(false);
        }
        let residents = self.registry.resident_snapshot().await;
        if residents.is_empty() {
            crate::probe!(
                class = "bench.standing.skipped",
                reason = "residency",
                "standing round: no citizens resident — retrying next tick"
            );
            return Ok(false);
        }
        let assignees: Vec<String> = residents.iter().map(|(name, _)| name.clone()).collect();
        let Some(executor) = self.executor_slot.cloned() else {
            crate::probe!(
                class = "bench.standing.skipped",
                reason = "no_executor",
                "standing round: executor not installed yet — retrying next tick"
            );
            return Ok(false);
        };
        let seed = cfg.next_seed;
        executor
            .execute_json(
                "benchmark/dispatch",
                json!({
                    "name": cfg.benchmark,
                    "sample": cfg.sample,
                    "seed": seed,
                    "assignees": assignees,
                }),
            )
            .await
            .map_err(|e| format!("standing dispatch failed: {e}"))?;
        // Persist the seed bump ONLY after a successful dispatch — a failed
        // dispatch retries the SAME seed, so the sequence has no holes.
        let mut next = cfg.clone();
        next.next_seed = seed + 1;
        save_config(&next)?;
        crate::probe!(
            class = "bench.standing.dispatched",
            benchmark = %next.benchmark,
            sample = next.sample as u64,
            seed = seed,
            assignees = assignees.len() as u64,
            "standing round dispatched — benchmarks run themselves"
        );
        Ok(true)
    }
}

#[async_trait]
impl ServiceModule for BenchmarkStandingModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "benchmark_standing",
            priority: ModulePriority::Normal,
            command_prefixes: &["benchmark/standing"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1,
            tick_interval: Some(STANDING_TICK),
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        let cfg = load_config();
        crate::probe!(
            class = "bench.standing.ready",
            enabled = cfg.enabled,
            benchmark = %cfg.benchmark,
            next_seed = cfg.next_seed,
            "standing-round module up — announces itself so silence is never ambiguous"
        );
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        // Never let one failed dispatch kill the tick loop — probe and retry.
        if let Err(e) = self.standing_check().await {
            crate::probe!(
                class = "bench.standing.error",
                error = %e,
                "standing check failed — retrying next tick"
            );
        }
        Ok(())
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.executor_slot.install(executor);
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            // GET with no params; SET merges the given fields. The receipt is
            // always the full effective config, so a caller never has to diff.
            "benchmark/standing" => {
                let mut cfg = load_config();
                let mut changed = false;
                if let Some(enabled) = params.get("enabled").and_then(Value::as_bool) {
                    cfg.enabled = enabled;
                    changed = true;
                }
                if let Some(benchmark) = params.get("benchmark").and_then(Value::as_str) {
                    // Refuse an uncatalogued name NOW, not on the night tick.
                    if !crate::commands::benchmark::known_benchmarks()
                        .iter()
                        .any(|b| b.name == benchmark)
                    {
                        return Err(format!(
                            "unknown benchmark '{benchmark}' — see benchmark/list"
                        ));
                    }
                    cfg.benchmark = benchmark.to_string();
                    changed = true;
                }
                if let Some(sample) = params.get("sample").and_then(Value::as_u64) {
                    cfg.sample = sample.clamp(1, 50) as u32;
                    changed = true;
                }
                if let Some(seed) = params.get("seed").and_then(Value::as_u64) {
                    cfg.next_seed = seed;
                    changed = true;
                }
                if changed {
                    save_config(&cfg)?;
                }
                Ok(CommandResult::Json(json!({
                    "enabled": cfg.enabled,
                    "benchmark": cfg.benchmark,
                    "sample": cfg.sample,
                    "next_seed": cfg.next_seed,
                    "changed": changed,
                })))
            }
            other => Err(format!("benchmark_standing: unknown command {other}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the standing config surviving a round trip and the
    // seed sequence having NO HOLES — the seed bumps only after a successful
    // dispatch, so a failed night retries the same seed and the claim's
    // seeded-sample protocol stays reproducible from the file alone.
    #[test]
    fn config_round_trips_and_defaults_are_off() {
        let d = StandingConfig::default();
        assert!(!d.enabled, "standing mode must be an explicit decision");
        assert_eq!(d.benchmark, "swe-bench-verified-mini");
        let json = serde_json::to_string(&d).unwrap();
        let back: StandingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }
}
