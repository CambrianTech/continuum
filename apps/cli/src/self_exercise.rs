//! Self-exercise battery — runs automatically after deploy to verify the core works end-to-end.
//!
//! Dispatches a small set of substrate commands (ping, runtime/metrics/all, ai/generate) and
//! reports pass/fail with wall-clock latency. Designed to be called from `boot_plan.rs`'s
//! run_beside_phase() so every deploy ends with a verdict — no agent in the loop.
//!
//! The exercise is deliberately minimal: it exercises the IPC path, probe ingestion, and inference
//! without requiring external tools (eye-node, STT). Each row validates a capability floor that
//! the core must satisfy before citizens start working on it.

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use continuum_client::{AircIpcTransport, Connection};
use serde_json::Value;
use uuid::Uuid;

/// A single exercise spec: dispatch a command and validate the response shape.
#[derive(Debug)]
pub struct ExerciseSpec {
    /// Display name for the report row.
    pub name: &'static str,
    /// Substrate command path (e.g., "ping", "runtime/metrics/all").
    pub path: &'static str,
    /// Parameters to pass with the dispatch.
    pub params: fn() -> Value,
    /// Validate the response; returns Ok(summary) or Err(failure reason).
    pub validate: fn(&Value) -> Result<String>,
}

/// Outcome of a single exercise row.
pub enum Outcome {
    Ok(String),
    Failed(String),
}

/// Full result for one exercise spec.
#[derive(Debug)]
pub struct RunResult {
    pub name: &'static str,
    pub elapsed: Duration,
    pub outcome: Outcome,
}

/// The default self-exercise battery — the minimal capability floor every deploy must pass.
pub fn default_battery() -> Vec<ExerciseSpec> {
    vec![
        // ping: basic IPC connectivity and substrate responsiveness.
        ExerciseSpec {
            name: "ping",
            path: "ping",
            params: || serde_json::json!({}),
            validate: |v| {
                v.get("ok")
                    .and_then(|x| x.as_bool())
                    .filter(|b| *b)
                    .map(|_| "substrate responded".into())
                    .ok_or_else(|| "ping returned non-boolean ok or missing field".into())
            },
        },
        // runtime/metrics/all: all probe sinks are registered and returning data.
        ExerciseSpec {
            name: "runtime/metrics",
            path: "runtime/metrics/all",
            params: || serde_json::json!({}),
            validate: |v| {
                let modules = v.get("modules").and_then(|m| m.as_object());
                match modules {
                    Some(m) if !m.is_empty() => {
                        Ok(format!("{} probe module(s) responding", m.len()))
                    }
                    _ => Err("runtime/metrics/all: no modules or empty response".into()),
                }
            },
        },
        // ai/generate with temperature=0 (deterministic): inference pipeline works.
        ExerciseSpec {
            name: "ai/generate",
            path: "ai/generate",
            params: || {
                serde_json::json!({
                    "messages": [
                        {"role": "user", "content": "Say only the word 'ack'."}
                    ],
                    "maxTokens": 8,
                    "temperature": 0.0,
                })
            },
            validate: |v| {
                let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    Err("ai/generate returned empty text".into())
                } else {
                    Ok(format!("generated {} token(s)", text.split_whitespace().count()))
                }
            },
        },
    ]
}

/// Run the full exercise battery against the given connection. Returns results for every spec,
/// plus a summary count of passed/failed rows.
pub async fn run_battery(
    conn: &Connection<AircIpcTransport>,
    specs: &[ExerciseSpec],
) -> Vec<RunResult> {
    let mut results = Vec::with_capacity(specs.len());
    for spec in specs {
        let started = Instant::now();
        let params = (spec.params)();
        let dispatch: Result<Value, _> = conn.commands().execute(spec.path, params).await;

        let outcome = match dispatch {
            Ok(response) => match (spec.validate)(&response) {
                Ok(summary) => Outcome::Ok(summary),
                Err(e) => Outcome::Failed(format!("validation: {e}")),
            },
            Err(e) => Outcome::Failed(format!("dispatch: {e}")),
        };

        let elapsed = started.elapsed();
        results.push(RunResult {
            name: spec.name,
            elapsed,
            outcome,
        });
    }
    results
}

/// Print a compact report of exercise results.
pub fn print_report(results: &[RunResult]) {
    println!("\n���️  Self-exercise\n");
    for r in results {
        let (status, detail) = match &r.outcome {
            Outcome::Ok(detail) => ("✓", detail),
            Outcome::Failed(detail) => ("��", detail),
        };
        println!(
            "  {:<24} [{:>6.2}s] {} {}",
            r.name,
            r.elapsed.as_secs_f64(),
            status,
            detail
        );
    }

    let passed = results.iter().filter(|r| matches!(r.outcome, Outcome::Ok(_))).count();
    let failed = results.iter().filter(|r| matches!(r.outcome, Outcome::Failed(_))).count();
    let total = results.len();
    println!("\n{passed}/{total} passed  ({failed} failed)");
}

/// Run the self-exercise battery and exit with success (0) if all rows pass, failure (1) otherwise.
pub async fn run(conn: Connection<AircIpcTransport>) -> Result<()> {
    // Use a slightly shorter deadline per-row for the exercise — it should be fast if the core is healthy.
    let conn = conn.with_deadline(Duration::from_secs(90));
    let results = run_battery(&conn, &specs).await;
    print_report(&results);

    let failed = results.iter().filter(|r| matches!(r.outcome, Outcome::Failed(_))).count();
    if failed > 0 {
        anyhow::bail!("{failed} exercise(s) failed");
    }
    Ok(())
}
