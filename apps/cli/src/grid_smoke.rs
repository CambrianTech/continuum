//! `ctm grid-smoke` — coverage matrix for grid-shipped substrate commands.
//!
//! ## What this exists for
//!
//! PR #1563 closed the wire-level proof for cross-grid `ai/generate`
//! end-to-end. But the substrate exposes dozens of commands, and the
//! Tron-grid frame says ANY of them should compose across the grid —
//! ping/screenshot across envs scales up to ping/screenshot across
//! TOWERS once the grid is wired. The proof we're missing is a
//! systematic battery that says: for each command, dispatched at the
//! target peer, does it work?
//!
//! v1 is single-hop only — caller dispatches `path` at one peer, gets
//! one response, validates the shape. The richer shape (multi-hop
//! composition: `M -> A -> B -> C`, fan-out, mixed-modality) lands in
//! v2 once probe-sink trace ingestion is wired. The v1 design holds
//! that shape — `ChainShape` already names `Composition` /
//! `FanOut` / `NotYetWired` variants so v2 is an additive enum
//! extension, not a rewrite.
//!
//! ## Design (forward-compatible to v2)
//!
//! Each substrate command we want to cover is a `GridSmokeSpec`:
//!
//! ```ignore
//! GridSmokeSpec {
//!     name: "ai/generate (one-word reply)",
//!     path: "ai/generate",
//!     params: ...,
//!     validate: |response| { ... -> Ok(human_summary) | Err(reason) },
//!     expectation: ChainShape::SingleHop { peer_label: "target", path: "ai/generate" },
//! }
//! ```
//!
//! The runner dispatches each spec, times the round-trip wall-clock,
//! validates, and reports a per-spec line + a summary.
//!
//! ## Doctrinal alignment
//!
//! - `[[host-the-seemingly-impossible]]` — the Tron-grid frame.
//!   A constrained-locally host dispatches at a GPU-rich peer and
//!   ALL of these are expected to compose by construction, not by
//!   special-case wiring.
//! - `[[commands-are-kernel-level-and-compose]]` — the harness
//!   doesn't care WHICH command it dispatches; same `Commands.execute()`
//!   primitive every other CLI / persona / sentinel uses.
//! - `[[no-fallbacks-ever]]` — when a row fails, the report names
//!   the spec, the path, the dispatch error verbatim. No "looks
//!   fine" green-rubber-stamp. Failure = loud + actionable.

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use continuum_client::{AircIpcTransport, Connection};
use serde_json::Value;

/// Shape of the dispatch chain a spec expects. v1 only ever asserts
/// SingleHop; the other variants are scaffolding for v2 (probe-trace
/// ingestion + composition rows). Declaring them now means v2 is an
/// additive extension, not a rewrite — and the spec table can pin
/// "expected NotYetWired" intent for commands that don't cross the
/// wire yet (Handle/Stream/Lambda-shaped responses, room broadcast,
/// env-wildcard, etc.).
///
/// The `#[allow(dead_code)]` annotations on the per-variant fields
/// are intentional: v1 doesn't read them yet, but v2's report
/// formatter + probe-trace validator will. Suppressing the warnings
/// here is cheaper than churning the struct shape later.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ChainShape {
    /// Caller -> one peer -> caller. The v1 case.
    SingleHop {
        peer_label: &'static str,
        path: &'static str,
    },
    /// Caller -> A -> B -> ... -> caller. The substrate composes
    /// because `CommandUri::Peer` routes through `AircTransport`
    /// whether the executor is processing local or remote input.
    /// v2 reads probe-sink JSONL to reconstruct + validate.
    #[allow(dead_code)]
    Composition { hops: Vec<&'static str> },
    /// Caller -> A; A fans to N peers; aggregates; returns.
    #[allow(dead_code)]
    FanOut {
        path: &'static str,
        min_peers: usize,
    },
    /// Pin commands the substrate's grid transport can't carry
    /// today (Handle/Stream/Lambda shapes per Slice 60). Spec
    /// remains in the battery so any future change is loud.
    #[allow(dead_code)]
    NotYetWired { reason: &'static str },
}

/// One row of the coverage battery.
pub struct GridSmokeSpec {
    /// Human-readable label printed at the head of the row.
    pub name: &'static str,
    /// Substrate command path dispatched at the target peer.
    pub path: &'static str,
    /// Builds the params JSON every time the spec runs. A closure
    /// so each invocation gets a fresh value (e.g. a fresh
    /// correlation id) without sharing state across runs.
    pub params: fn() -> Value,
    /// Validates the substrate's response. Returns a short
    /// human-readable summary on success, or a typed reason on
    /// failure. The harness prints the summary on Ok and the
    /// reason on Err — both append to the per-row line.
    pub validate: fn(&Value) -> Result<String, String>,
    /// The chain shape this spec expects. v1 only asserts
    /// SingleHop / NotYetWired; v2 starts asserting Composition /
    /// FanOut via probe-trace ingestion.
    pub expectation: ChainShape,
    /// What the AUTHORIZATION gate is expected to do with this row.
    pub gate: GateExpectation,
}

/// The authorization outcome a row expects. An airc caller resolves to
/// `Provisional` trust until the airc↔grid trust bridge lands (task #38 —
/// `modules/grid/registry.rs` NOTE): `Provisional` admits `ai/generate` +
/// AiSafe-declared commands and refuses the `Privileged`/`Owner` tiers.
/// A refusal is a CORRECT, loud answer — the battery's transport claim is
/// "typed response, fast", never "everything is authorized". Encoding the
/// expectation keeps both failure modes visible: a DEADLINE on a
/// refused-expected row is a transport regression; an ADMITTED result on
/// one means the trust bridge landed and the battery must be updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateExpectation {
    /// The command is admitted at Provisional; the row passes on a
    /// validated response.
    Admitted,
    /// The command needs `Trusted`+ (unwired until task #38); the row
    /// passes on a FAST typed refusal.
    RefusedUntilTrustBridge,
}

/// Outcome of one row.
pub enum Outcome {
    /// Dispatch + validation succeeded. Holds the validator's
    /// summary for the report line.
    Ok(String),
    /// Dispatch failed (transport error, deadline) or validation
    /// rejected the response. The string surfaces the failure
    /// reason verbatim — no rewrite to "user-friendly" text per
    /// `[[no-fallbacks-ever]]`.
    Failed(String),
    /// Spec was declared NotYetWired today. Skipped intentionally
    /// (the reason is the value).
    Skipped(&'static str),
}

#[allow(dead_code)]
pub struct RunResult {
    pub name: &'static str,
    /// The substrate command path that was dispatched. Not read in
    /// the v1 report (the name is enough), but v2's structured
    /// trace output will key on it — same forward-compat shape as
    /// `ChainShape`'s unread fields.
    pub path: &'static str,
    pub elapsed: Duration,
    pub outcome: Outcome,
}

/// The v1 battery. Three rows; each proves a different stack layer.
///
/// Add new rows here. Keep the constructor `pub fn` rather than a
/// `const SLICE: &[GridSmokeSpec]` so callers can extend / filter at
/// runtime once we ship a `--only path1,path2` selector.
pub fn default_battery() -> Vec<GridSmokeSpec> {
    vec![
        // Substrate alive. AiSafe → admitted at Provisional. If THIS
        // fails the rest of the battery is meaningless.
        GridSmokeSpec {
            name: "ping",
            path: "ping",
            params: || serde_json::json!({}),
            validate: |v| {
                let obj = v
                    .as_object()
                    .ok_or_else(|| "expected ping result object".to_string())?;
                let sha = obj
                    .get("buildSha")
                    .and_then(|s| s.as_str())
                    .unwrap_or("<no buildSha>");
                Ok(format!("pong build={sha}"))
            },
            expectation: ChainShape::SingleHop {
                peer_label: "target",
                path: "ping",
            },
            gate: GateExpectation::Admitted,
        },
        // Capability discovery — the exact question a grid peer asks
        // before placing work ("what compute do you have?"). AiSafe →
        // admitted at Provisional.
        GridSmokeSpec {
            name: "gpu/stats",
            path: "gpu/stats",
            params: || serde_json::json!({}),
            validate: |v| {
                let obj = v
                    .as_object()
                    .ok_or_else(|| "expected gpu stats object".to_string())?;
                if obj.is_empty() {
                    return Err("gpu stats object empty".into());
                }
                Ok(format!("{} field(s)", obj.len()))
            },
            expectation: ChainShape::SingleHop {
                peer_label: "target",
                path: "gpu/stats",
            },
            gate: GateExpectation::Admitted,
        },
        // Privileged tier — refused at Provisional until the trust
        // bridge (task #38). The row proves the GATE answers loudly:
        // a fast typed refusal passes, a deadline fails.
        GridSmokeSpec {
            name: "runtime/metrics/all",
            path: "runtime/metrics/all",
            params: || serde_json::json!({}),
            validate: |v| {
                let obj = v.as_object().ok_or_else(|| {
                    "expected object of module->metrics, got non-object".to_string()
                })?;
                Ok(format!("{} modules reporting", obj.len()))
            },
            expectation: ChainShape::SingleHop {
                peer_label: "target",
                path: "runtime/metrics/all",
            },
            gate: GateExpectation::RefusedUntilTrustBridge,
        },
        // Privileged tier — same refusal contract as metrics.
        GridSmokeSpec {
            name: "ai/providers/list",
            path: "ai/providers/list",
            params: || serde_json::json!({}),
            validate: |v| {
                let providers = v
                    .get("providers")
                    .or_else(|| v.get("available"))
                    .and_then(|p| p.as_array())
                    .ok_or_else(|| {
                        format!(
                            "expected providers or available array; got top-level keys: {:?}",
                            v.as_object().map(|o| o.keys().collect::<Vec<_>>())
                        )
                    })?;
                Ok(format!("{} provider(s) registered", providers.len()))
            },
            expectation: ChainShape::SingleHop {
                peer_label: "target",
                path: "ai/providers/list",
            },
            gate: GateExpectation::RefusedUntilTrustBridge,
        },
        // Real inference dispatch — the proof PR #1563 covered for
        // HeuristicInferenceAdapter, now run against whatever adapter
        // the target peer has. If the target is a GPU host running
        // a real LLM, this row's response IS that model. If the
        // target only has the heuristic registered, the response
        // starts with [heuristic:...]. The validator just confirms
        // the wire-shape; the operator reads the summary line to
        // see WHO answered.
        GridSmokeSpec {
            name: "ai/generate (one-word reply)",
            path: "ai/generate",
            params: || {
                serde_json::json!({
                    "messages": [
                        { "role": "user", "content": "Reply with one short word." }
                    ],
                    // "local" is the designed sentinel for "your best local
                    // adapter" — an EXPLICIT specifier, so the registry's
                    // no-specifier guard ([[no-fallbacks-ever]]) admits it.
                    // Exactly the cross-grid ask: "whatever you serve".
                    "provider": "local",
                    // Room for reasoning-style models that spend tokens
                    // before the visible word — 16 came back empty-text.
                    "maxTokens": 128,
                })
            },
            validate: |v| {
                let text = v
                    .get("text")
                    .and_then(|t| t.as_str())
                    .ok_or_else(|| "missing required `text` field".to_string())?;
                let model = v
                    .get("model")
                    .and_then(|m| m.as_str())
                    .ok_or_else(|| "missing required `model` field".to_string())?;
                // Truncate the echoed text so the report line stays
                // one screen wide even for chatty models.
                let trimmed: String = text.chars().take(50).collect();
                Ok(format!(
                    "model={model} text={trimmed:?}{}",
                    if text.chars().count() > 50 { " (truncated)" } else { "" }
                ))
            },
            expectation: ChainShape::SingleHop {
                peer_label: "target",
                path: "ai/generate",
            },
            gate: GateExpectation::Admitted,
        },
    ]
}

/// Dispatch one spec, time the wall-clock, validate.
async fn run_spec(
    conn: &Connection<AircIpcTransport>,
    spec: &GridSmokeSpec,
) -> RunResult {
    // NotYetWired rows skip dispatch entirely — the substrate would
    // error on something we already know doesn't cross the wire.
    if let ChainShape::NotYetWired { reason } = &spec.expectation {
        return RunResult {
            name: spec.name,
            path: spec.path,
            elapsed: Duration::ZERO,
            outcome: Outcome::Skipped(reason),
        };
    }

    let params = (spec.params)();
    let started = Instant::now();
    let dispatch: Result<Value, _> = conn.commands().execute(spec.path, params).await;
    let elapsed = started.elapsed();

    let outcome = match (dispatch, spec.gate) {
        (Ok(value), GateExpectation::Admitted) => match (spec.validate)(&value) {
            Ok(summary) => Outcome::Ok(summary),
            Err(reason) => Outcome::Failed(format!("validate: {reason}")),
        },
        // The trust bridge landed and this tier is now admitted — the
        // battery's expectation is stale. Fail LOUDLY so the row gets
        // flipped to Admitted rather than silently rubber-stamped.
        (Ok(_), GateExpectation::RefusedUntilTrustBridge) => Outcome::Failed(
            "UNEXPECTEDLY ADMITTED — trust bridge (task #38) landed? \
             Flip this row's gate to Admitted and validate the payload."
                .to_string(),
        ),
        (Err(e), gate) => {
            let msg = e.to_string();
            // A typed substrate refusal on a refusal-expected row IS the
            // pass condition: it proves request + gate + reply all cross
            // the wire fast. Anything else (deadline, transport error)
            // stays a failure.
            if gate == GateExpectation::RefusedUntilTrustBridge && msg.contains("refused") {
                Outcome::Ok(format!(
                    "refused at the gate as expected until task #38 (Provisional < Privileged)"
                ))
            } else {
                Outcome::Failed(format!("dispatch: {msg}"))
            }
        }
    };

    RunResult {
        name: spec.name,
        path: spec.path,
        elapsed,
        outcome,
    }
}

/// Run the whole battery sequentially and return results in
/// declaration order. Sequential because v1 wants the wall-clock
/// timing to be uncontended; parallel comes later when we add
/// concurrency-stress rows.
pub async fn run_battery(
    conn: Connection<AircIpcTransport>,
    specs: Vec<GridSmokeSpec>,
) -> Vec<RunResult> {
    let mut results = Vec::with_capacity(specs.len());
    for spec in &specs {
        results.push(run_spec(&conn, spec).await);
    }
    results
}

/// Print the per-row report + a summary footer.
pub fn print_report(target_peer: &str, results: &[RunResult]) {
    println!("\n🔌 Grid smoke — target peer {target_peer}\n");

    let name_width = results.iter().map(|r| r.name.len()).max().unwrap_or(40);

    for r in results {
        let (glyph, body) = match &r.outcome {
            Outcome::Ok(summary) => ("✅", summary.clone()),
            Outcome::Failed(reason) => ("❌", reason.clone()),
            Outcome::Skipped(reason) => ("⏭ ", (*reason).to_string()),
        };
        let ms = match r.outcome {
            Outcome::Skipped(_) => String::from("    -"),
            _ => format!("{:>5}", r.elapsed.as_millis()),
        };
        println!("  {glyph}  {:width$}  {} ms   {}", r.name, ms, body, width = name_width);
    }

    let pass = results.iter().filter(|r| matches!(r.outcome, Outcome::Ok(_))).count();
    let fail = results.iter().filter(|r| matches!(r.outcome, Outcome::Failed(_))).count();
    let skip = results.iter().filter(|r| matches!(r.outcome, Outcome::Skipped(_))).count();
    let total = results.len();

    println!("\n{pass}/{total} passed  ({fail} failed, {skip} skipped)");
}

/// Top-level entry point invoked from `main.rs`. Runs the default
/// battery, prints the report, and returns Ok iff every spec passed.
/// Failure -> nonzero exit so CI / scripts can gate on grid-smoke
/// directly without parsing the report.
pub async fn run(
    conn: Connection<AircIpcTransport>,
    target_peer: uuid::Uuid,
) -> Result<()> {
    let specs = default_battery();
    let results = run_battery(conn, specs).await;
    print_report(&target_peer.to_string(), &results);
    let failed = results
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Failed(_)))
        .count();
    if failed > 0 {
        Err(anyhow!(
            "grid smoke FAILED: {failed} spec(s) errored — see report above"
        ))
    } else {
        Ok(())
    }
}
