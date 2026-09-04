//! `modules::genome_fitness_sentinel` — the daemon that measures each resident
//! genome layer's value-density fitness on a slow cadence and glass-boxes the
//! ranking. The autonomic decision-maker of the self-evolving genome
//! (`docs/genome/SELF-EVOLVING-GENOME.md`), built as a `ServiceModule` sentinel —
//! own `tokio::time::interval`, watch-free read-only tick, `Background` priority —
//! exactly like `training_completion_sentinel` and the serving daemon. NOT a
//! bespoke orchestrator.
//!
//! ## Observe-only (this slice) — earn the emergent version (§5)
//! "A subtly-wrong fitness function makes the machine confidently accumulate
//! garbage AND report improvement — the worst failure in the design." So this first
//! slice MEASURES and REPORTS; it does NOT evict. The retire verdict is computed and
//! surfaced (glass box) so the signal can be validated against ground truth BEFORE
//! it is ever wired to actually retire a layer. Destructive eviction is a later
//! slice, gated on that validation. This also IS the "admit state" half of
//! [[continuous-learning-autonomic-first-then-admit-controls-and-state]]: the tick
//! emits the fitness landscape via a probe, so positron / a telemetry widget can
//! render the living genome without any of it being a black box.
//!
//! ## Inputs (honest half only — §6 slice-2 gap noted)
//! - **cost** — the on-disk gguf-lora byte size (the resident VRAM footprint proxy).
//!   Have it: `adapter_manifest` + `fs::metadata`.
//! - **lift** — the latest A/B eval delta for the layer, from the progress ledger
//!   (`~/.continuum/progress/<persona>.jsonl`). Have it where a `cognition/eval`
//!   has run. A layer with NO ledger row is `Unmeasured` (needs an exam, NOT
//!   eviction — unknown ≠ zero).
//! - **demand / redundancy** — default to neutral (1.0). Their instrumentation
//!   (page-in telemetry; capability-space overlap) is the §6 slice-2 / geometry
//!   gap; until it lands, fitness = lift-per-GB, which is already an honest signal.
//!
//! ## Known join gap (a finding this slice surfaces)
//! The manifest keys a layer by its `alias` (`adapters-<hash>`); the ledger keys
//! lift by `geneId`, which the L3 sentinel writes as the `trait_kind`. So a layer
//! is matched to its lift only when the two coincide. Layers that don't match read
//! as `Unmeasured` — honestly flagging the alias↔geneId join to thread in a later
//! slice, rather than silently mis-scoring.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use serde_json::Value;

use crate::genome::fitness::{rank_by_fitness, retire_verdict, FitnessVerdict, LayerFitness};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

/// Slow cadence — genome fitness is a housekeeping concern, not a hot path. Rounds
/// of learning take minutes-to-hours; re-ranking every 5 minutes is ample and keeps
/// the substrate quiet (the tick is cheap file I/O; it emits one probe).
const TICK: Duration = Duration::from_secs(300);

/// The retire floor for the OBSERVE-ONLY report. `0.0`: only a layer with
/// value-density `≤ 0` (i.e. `lift ≤ 0`) is flagged a retire-candidate — the honest
/// default that flags exactly the layers that made the persona no better. The real
/// eviction floor becomes a governor knob when eviction is wired (later slice).
const REPORT_RETIRE_FLOOR: f64 = 0.0;

/// One layer's assessment for the glass-box report.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerAssessment {
    pub alias: String,
    /// `None` when the layer has no eval-ledger lift yet — it is UNMEASURED (needs
    /// an exam, not eviction). `Some(fitness)` when a lift is known.
    pub value_density: Option<f64>,
    pub category: LayerCategory,
}

/// The observe-only verdict category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerCategory {
    /// Fitness pays for footprint — keep.
    Keep,
    /// value-density ≤ the report floor (lift ≤ 0) — would be retired once eviction
    /// is wired. Surfaced, never acted on, this slice.
    RetireCandidate,
    /// No eval-ledger lift yet — needs an exam before it can be scored. NOT a
    /// retire candidate (unknown ≠ zero).
    Unmeasured,
}

/// One resident layer's measured inputs, gathered by the tick's I/O.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerInputs {
    pub alias: String,
    pub cost_bytes: u64,
    /// Latest measured A/B lift, or `None` if the layer has no ledger row.
    pub lift: Option<f64>,
}

/// Pure assessment: score every measured layer, categorize, and return them ranked
/// by descending value-density (unmeasured layers listed last, in input order). No
/// I/O, no eviction — the decide-logic split from the tick so it is unit-testable.
/// `demand`/`redundancy` are neutral (1.0) until their instrumentation lands.
pub fn assess_layers(inputs: &[LayerInputs]) -> Vec<LayerAssessment> {
    // Measured layers → fitness + category; ranked by value-density.
    let mut measured: Vec<(LayerFitness, String)> = Vec::new();
    let mut unmeasured: Vec<LayerAssessment> = Vec::new();
    for inp in inputs {
        match inp.lift {
            Some(lift) => measured.push((
                LayerFitness {
                    lift,
                    // Neutral until the being-axis A/B lands (same posture as
                    // demand/redundancy). NOT a claim of "no harm" — a claim of "not yet
                    // measured", and the follow-up is to measure it before this sentinel
                    // is allowed to promote anything on its own.
                    harm: 0.0,
                    demand: 1.0,
                    cost_bytes: inp.cost_bytes,
                    redundancy: 1.0,
                },
                inp.alias.clone(),
            )),
            None => unmeasured.push(LayerAssessment {
                alias: inp.alias.clone(),
                value_density: None,
                category: LayerCategory::Unmeasured,
            }),
        }
    }
    let fitnesses: Vec<LayerFitness> = measured.iter().map(|(f, _)| *f).collect();
    let order = rank_by_fitness(&fitnesses);
    let mut out: Vec<LayerAssessment> = order
        .into_iter()
        .map(|i| {
            let (fit, alias) = &measured[i];
            let vd = fit.value_density();
            LayerAssessment {
                alias: alias.clone(),
                value_density: Some(vd),
                category: match retire_verdict(vd, REPORT_RETIRE_FLOOR) {
                    FitnessVerdict::Keep => LayerCategory::Keep,
                    FitnessVerdict::Retire => LayerCategory::RetireCandidate,
                },
            }
        })
        .collect();
    out.extend(unmeasured);
    out
}

/// The self-evolving genome's fitness daemon. Stateless — it reads the manifest,
/// the on-disk layer sizes, and the progress ledger fresh each tick (no cached
/// snapshot to drift). Observe-only in this slice.
#[derive(Default)]
pub struct GenomeFitnessSentinel;

impl GenomeFitnessSentinel {
    pub fn new() -> Self {
        Self
    }

    /// Latest A/B lift per `geneId`, scanned from every persona's progress ledger
    /// under `~/.continuum/progress/*.jsonl`. Keeps the row with the greatest
    /// `capturedAtMs` per gene. Best-effort: an unreadable dir / malformed row is
    /// skipped (this is observability, never a fail-loud path). Returns a map from
    /// `geneId` → lift.
    fn latest_lift_by_gene() -> std::collections::HashMap<String, f64> {
        use std::collections::HashMap;
        let mut best: HashMap<String, (u64, f64)> = HashMap::new();
        let Some(home) = dirs_home() else {
            return HashMap::new();
        };
        let dir = home.join(".continuum").join("progress");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return HashMap::new();
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines() {
                let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let (Some(gene), Some(lift)) = (
                    row.get("geneId").and_then(|v| v.as_str()),
                    row.get("lift").and_then(|v| v.as_f64()),
                ) else {
                    continue;
                };
                let at = row
                    .get("capturedAtMs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                match best.get(gene) {
                    Some((prev_at, _)) if *prev_at >= at => {}
                    _ => {
                        best.insert(gene.to_string(), (at, lift));
                    }
                }
            }
        }
        best.into_iter().map(|(g, (_, lift))| (g, lift)).collect()
    }
}

/// `$HOME` as a PathBuf, or `None`. Kept local so the sentinel has no new dep.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[async_trait]
impl ServiceModule for GenomeFitnessSentinel {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "genome-fitness-sentinel",
            priority: ModulePriority::Background,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(TICK),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    /// Measure every resident genome layer's value-density and glass-box the
    /// ranking. Read-only: no eviction this slice. Cheap file I/O; emits one probe.
    async fn tick(&self) -> Result<(), String> {
        // Enumerate the registered genes. A missing manifest = zero layers = nothing
        // to assess (not an error — the honest empty state).
        let genes = match crate::forge::adapter_manifest::load() {
            Ok(g) => g,
            Err(e) => {
                crate::probe!(
                    class = "genome.fitness",
                    error = e.as_str(),
                    "adapter manifest unreadable — skipping fitness assessment this tick",
                );
                return Ok(());
            }
        };
        if genes.is_empty() {
            return Ok(());
        }

        let lifts = Self::latest_lift_by_gene();
        let inputs: Vec<LayerInputs> = genes
            .iter()
            .map(|g| LayerInputs {
                alias: g.alias.clone(),
                // On-disk gguf-lora size = the resident VRAM footprint proxy. An
                // unreadable file → 0 cost (it will read as high fitness and get
                // surfaced; a missing gene file is the manifest's fail-loud concern,
                // not the fitness tick's).
                cost_bytes: std::fs::metadata(&g.path).map(|m| m.len()).unwrap_or(0),
                lift: lifts.get(&g.alias).copied(),
            })
            .collect();

        let assessed = assess_layers(&inputs);
        let retire_candidates = assessed
            .iter()
            .filter(|a| a.category == LayerCategory::RetireCandidate)
            .count();
        let unmeasured = assessed
            .iter()
            .filter(|a| a.category == LayerCategory::Unmeasured)
            .count();

        // Glass-box the whole landscape (best-first). Observe-only: this is the
        // signal we validate against ground truth before wiring eviction.
        let ranking = assessed
            .iter()
            .map(|a| match a.value_density {
                Some(vd) => format!("{}={:.3}[{:?}]", a.alias, vd, a.category),
                None => format!("{}=?[{:?}]", a.alias, a.category),
            })
            .collect::<Vec<_>>()
            .join(" ");
        crate::probe!(
            class = "genome.fitness",
            layers = assessed.len(),
            retire_candidates,
            unmeasured,
            ranking = ranking.as_str(),
            "genome fitness landscape (observe-only: value-density lift/GB, best-first; no eviction)",
        );
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Pure background sentinel — no commands. A command routed here is a wiring
        // bug; fail loud naming it.
        Err(format!(
            "genome-fitness-sentinel exposes no commands (got '{command}')"
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: measured layers rank by value-density (lift/GB), a lift ≤ 0
    // layer is a RetireCandidate (not silently kept), and a layer with NO ledger lift
    // is Unmeasured (needs an exam, NOT eviction — unknown ≠ zero). The whole
    // observe-only verdict in one assertion.
    #[test]
    fn assess_ranks_measured_flags_zero_lift_and_separates_unmeasured() {
        let gb = 1_000_000_000u64;
        let inputs = vec![
            LayerInputs {
                alias: "strong".into(),
                cost_bytes: gb / 50,
                lift: Some(0.30),
            }, // high lift/GB
            LayerInputs {
                alias: "weak".into(),
                cost_bytes: gb / 2,
                lift: Some(0.02),
            }, // low lift/GB
            LayerInputs {
                alias: "dead".into(),
                cost_bytes: gb / 50,
                lift: Some(0.0),
            }, // lift 0 → retire
            LayerInputs {
                alias: "new".into(),
                cost_bytes: gb / 50,
                lift: None,
            }, // unmeasured
        ];
        let out = assess_layers(&inputs);
        // Measured, ranked best-first, then unmeasured last.
        let order: Vec<&str> = out.iter().map(|a| a.alias.as_str()).collect();
        assert_eq!(order, vec!["strong", "weak", "dead", "new"]);
        assert_eq!(out[0].category, LayerCategory::Keep);
        assert_eq!(out[1].category, LayerCategory::Keep);
        assert_eq!(
            out[2].category,
            LayerCategory::RetireCandidate,
            "lift 0 → retire candidate"
        );
        assert_eq!(
            out[3].category,
            LayerCategory::Unmeasured,
            "no lift → unmeasured, not retire"
        );
        assert!(out[0].value_density.unwrap() > out[1].value_density.unwrap());
        assert!(out[3].value_density.is_none());
    }

    // what this catches: an empty genome assesses to nothing (the honest empty state,
    // no panic on zero layers).
    #[test]
    fn empty_genome_assesses_empty() {
        assert!(assess_layers(&[]).is_empty());
    }
}
