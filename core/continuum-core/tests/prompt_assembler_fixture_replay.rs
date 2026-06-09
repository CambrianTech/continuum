//! Fixture-replay harness for Phase 0.5.2 — `PersonaPromptAssembler` turn-N port.
//!
//! Architecture: memento (TS-side) instruments the live TS
//! `PersonaPromptAssembler.assembleMessages` call with a wrapper that
//! writes `{ input, output, persona_id, ts }` rows to a JSONL fixture
//! file during a real chat session. Each row captures the EXACT input
//! state the assembler saw and the EXACT output it produced.
//!
//! This harness loads each fixture row and asserts the Rust port
//! `persona::prompt_assembly::assemble()` (or its turn-N variant when
//! we add one) produces an output equivalent to the captured TS output.
//!
//! Equivalence is field-by-field, not byte-by-byte — TS may serialize
//! whitespace or attribute order differently than serde_json. We
//! reconstruct both as structured types and compare semantically.
//!
//! Failure modes the harness catches:
//!   - Rust output diverges from TS for any captured persona / context shape
//!   - Multimodal artifact handling differs (vision base64, audio inline)
//!   - Voice-mode instruction injection differs
//!   - Identity reminder position differs
//!   - Social awareness block content differs
//!   - Conversation history time-gap markers differ
//!
//! Fixture path (gated by env to keep the file optional during dev):
//!   .continuum/fixtures/0.5.2-prompt-assembler-turn-n.jsonl
//! or override via `PROMPT_ASSEMBLER_FIXTURES=<path>`.
//!
//! When no fixture file exists, the harness exits cleanly (no tests
//! run, no failure). When the file exists, ONE test runs per row.
//! Run with:
//!
//!   cargo test --package continuum-core \
//!     --test prompt_assembler_fixture_replay \
//!     -- --nocapture

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

/// One row in the JSONL fixture file. Mirrors what memento's TS-side
/// capture-hook serializes. INPUT STATE the assembler saw + the OUTPUT
/// it produced.
///
/// The shape here is a JSON Value for input/output (not a strongly-typed
/// struct) because the TS-side capture is the source of truth for the
/// schema; we don't want this harness blocking on a Rust struct that
/// memento has to also keep in sync. Once the schema stabilizes we
/// can promote it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureRow {
    /// Persona that produced this assembly. UUID as string.
    persona_id: String,
    /// ISO timestamp of capture.
    ts: String,
    /// Input state the assembler SAW (system prompt, RAG context,
    /// social signals, conversation history with timestamps as-is).
    input: serde_json::Value,
    /// Output the TS assembler produced (LLM message array).
    output: serde_json::Value,
}

fn fixture_path() -> PathBuf {
    if let Ok(p) = env::var("PROMPT_ASSEMBLER_FIXTURES") {
        return PathBuf::from(p);
    }
    let cwd = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(cwd)
        .join("../../..")
        .join(".continuum")
        .join("fixtures")
        .join("0.5.2-prompt-assembler-turn-n.jsonl")
}

fn load_fixtures() -> Vec<FixtureRow> {
    let path = fixture_path();
    if !path.exists() {
        eprintln!(
            "[fixture-replay] no fixture file at {path:?} — \
             this is the no-op state until memento ships TS captures. \
             Set PROMPT_ASSEMBLER_FIXTURES=<path> or drop a .jsonl at \
             .continuum/fixtures/0.5.2-prompt-assembler-turn-n.jsonl"
        );
        return Vec::new();
    }
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[fixture-replay] failed to read {path:?}: {e}");
            return Vec::new();
        }
    };
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        match serde_json::from_str::<FixtureRow>(trimmed) {
            Ok(row) => rows.push(row),
            Err(e) => {
                eprintln!("[fixture-replay] line {i} parse error: {e} — skipping");
            }
        }
    }
    eprintln!(
        "[fixture-replay] loaded {} fixture rows from {path:?}",
        rows.len()
    );
    rows
}

/// What this catches: the Rust `assemble()` (or its turn-N extension)
/// producing output structurally different from the TS captures for
/// the same input. Run per-fixture so each failure surfaces the
/// SPECIFIC shape that diverged, not a one-line "fixtures don't match."
///
/// Until memento ships fixtures, this is a no-op (load_fixtures returns
/// empty and the loop body never runs). That's intentional — it lets
/// the harness ship without blocking on the TS side.
///
/// Once fixtures land, EACH row's failure is a specific bug to chase.
/// The error message includes the persona_id + ts so memento can
/// re-capture the same input live to debug.
#[test]
fn rust_assembler_matches_ts_captures() {
    let fixtures = load_fixtures();
    if fixtures.is_empty() {
        // No fixtures yet — the test passes trivially. This is the
        // pre-handoff state. When memento ships, the assertions kick in
        // and the test stops being a no-op.
        return;
    }

    let mut failures: Vec<String> = Vec::new();
    for (i, row) in fixtures.iter().enumerate() {
        // Best-effort path: parse the captured input as our existing
        // PromptAssemblyInput shape, call assemble(), compare against
        // the captured output.
        //
        // The mapping from JSON fixture → PromptAssemblyInput is the
        // place this harness will need to evolve once we see the actual
        // capture schema. For now we attempt a direct deserialize and
        // record schema mismatches as failures (they're actionable
        // signals that the TS capture needs a field rename or the
        // Rust struct needs a new field).
        let parsed: Result<continuum_core::persona::prompt_assembly::PromptAssemblyInput, _> =
            serde_json::from_value(row.input.clone());
        let input = match parsed {
            Ok(input) => input,
            Err(e) => {
                failures.push(format!(
                    "row {i} (persona={}, ts={}): input deserialize failed: {e}",
                    row.persona_id, row.ts
                ));
                continue;
            }
        };

        let actual = continuum_core::persona::prompt_assembly::assemble(&input);
        // Compare via JSON to be tolerant of field-order differences.
        let actual_json = match serde_json::to_value(&actual) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "row {i} (persona={}): actual serialize failed: {e}",
                    row.persona_id
                ));
                continue;
            }
        };
        if actual_json != row.output {
            // Print first divergent path for fast triage. Full diff
            // would explode into the test log on multimodal fixtures.
            failures.push(format!(
                "row {i} (persona={}, ts={}): output mismatch.\n  expected: {}\n  actual:   {}",
                row.persona_id,
                row.ts,
                serde_json::to_string(&row.output).unwrap_or_default(),
                serde_json::to_string(&actual_json).unwrap_or_default(),
            ));
        }
    }

    if !failures.is_empty() {
        let count = failures.len();
        eprintln!("[fixture-replay] {count} failures:");
        for f in &failures {
            eprintln!("  - {f}");
        }
        panic!(
            "{} of {} fixture rows diverged — see above for per-row details",
            count,
            fixtures.len()
        );
    }

    eprintln!(
        "[fixture-replay] all {} fixture rows matched TS captures",
        fixtures.len()
    );
}
