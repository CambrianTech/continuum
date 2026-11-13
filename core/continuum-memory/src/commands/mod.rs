//! `memory/<verb>` — the persona's MEMORY hands as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one command per file.
//!
//! ## Why these are typed commands (not a `match` arm)
//!
//! The memory family once lived ONLY in `MemoryModule::handle_command`'s
//! stringly `match` — dispatchable, but with no descriptor in the registry, so a
//! persona was never OFFERED recall as a tool. As typed commands each gets a
//! descriptor (so it appears in the persona tool surface, the grid ACL, codegen,
//! `uu`) AND routes through the O(1) lock-free typed path. The wire name mirrors the
//! file path — `commands/memory/multi_layer_recall.rs` ⟺ `memory/multi-layer-recall`.
//!
//! ## Identity note
//!
//! These commands take `persona_id` on the WIRE (snake_case, the unchanged ORM
//! contract): `load-corpus`/`append-*` are infrastructure/consolidation writes that
//! name the target persona explicitly, so this is a faithful 1:1 port of the legacy
//! arms — not the `ctx.caller`-derived identity the git family uses. Tightening the
//! identity axis is deliberately out of scope for the registry collapse; the writes
//! are gated `Privileged` and reads `AiSafe` as the trust boundary.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::state::MemoryState;
use crate::sdk_codegen::DynCommand;

pub mod append_event;
pub mod append_memory;
pub mod consciousness_context;
pub mod load_corpus;
pub mod multi_layer_recall;
pub mod share;

use append_event::MemoryAppendEvent;
use append_memory::MemoryAppendMemory;
use consciousness_context::MemoryConsciousnessContext;
use load_corpus::MemoryLoadCorpus;
use multi_layer_recall::MemoryMultiLayerRecall;
use share::MemoryShare;

/// Result of an incremental append (`memory/append-memory`, `memory/append-event`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/AppendResult.ts"
)]
pub struct AppendResult {
    /// Always true on success (the call fails loud rather than returning false).
    pub appended: bool,
}

/// The dep-holding memory command objects `MemoryModule`
/// contributes to the kernel's typed object map, each sharing the module's
/// `Arc<MemoryState>`. The executor routes each name straight here, winning over the
/// (now-deleted) legacy `memory/` prefix arm.
pub fn command_objects(state: Arc<MemoryState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(MemoryLoadCorpus {
            state: state.clone(),
        }),
        Arc::new(MemoryMultiLayerRecall {
            state: state.clone(),
        }),
        Arc::new(MemoryShare {
            state: state.clone(),
        }),
        Arc::new(MemoryConsciousnessContext {
            state: state.clone(),
        }),
        Arc::new(MemoryAppendMemory {
            state: state.clone(),
        }),
        Arc::new(MemoryAppendEvent { state }),
    ]
}

// ─── Durable write-through + hydrate-on-miss (card aded8871) ────────────────
//
// The cache-only bug this closes: `memory/append-memory` mutated ONLY the
// in-process corpus, so every memory a persona wrote died with the core process — recall after a
// restart returned nothing. The durable truth is the persona's `longterm.db`
// (the data layer); the corpus is a derived cache of it. Writes go durable
// FIRST, then cache; a missing corpus hydrates FROM the store on first touch.

/// The collection memories live in inside the persona's `longterm.db` — the
/// same collection name the sentinel-escalation writer and the legacy TS ORM
/// use for `MemoryRecord` rows.
pub const MEMORIES_COLLECTION: &str = "memories";

/// Key the persisted row carries an optional embedding under, alongside the
/// flattened `MemoryRecord` fields. `MemoryRecord` deserialization tolerates
/// the extra key (no `deny_unknown_fields`), so rows with and without an
/// embedding — e.g. sentinel-written rows — both round-trip.
const EMBEDDING_KEY: &str = "embedding";

/// Map a citizen id to its per-citizen data-layer handle. Three shapes, so the
/// SAME memory command serves persona / agent / human (first-class citizenship,
/// Joel 2026-07-25):
///  - an EXPLICIT sentinel (`@agent:claude-code`, `@human:joel`, `@persona:Asha`)
///    passes through verbatim → its own bucket (`agents/<name>/`,
///    `humans/<name>/`, `personas/<name>/`);
///  - a UUID-shaped id passes through → the live `personas/<uuid>/` layout;
///  - a bare slug defaults to `@persona:<slug>` (back-compat — the unchanged
///    persona contract).
pub fn persona_db_handle(persona_id: &crate::identity::PersonaRef) -> String {
    let persona_id = persona_id.as_str();
    if persona_id.starts_with("@agent:")
        || persona_id.starts_with("@human:")
        || persona_id.starts_with("@persona:")
        || crate::identity::is_uuid_shape(persona_id)
    {
        persona_id.to_string()
    } else {
        format!("@persona:{persona_id}")
    }
}

/// Write one memory through to the persona's durable store via `data/create`.
/// Fails LOUD when the executor isn't installed or the write fails — a memory
/// that only landed in cache is the exact lie this seam exists to kill.
pub(crate) async fn persist_memory(
    state: &MemoryState,
    persona_id: &crate::identity::PersonaRef,
    memory: &crate::memory::CorpusMemory,
) -> Result<(), crate::sdk_codegen::CommandError> {
    use crate::sdk_codegen::CommandError;
    let executor = state.executor().map_err(CommandError::Internal)?;
    let mut data = serde_json::to_value(&memory.record)
        .map_err(|e| CommandError::Internal(format!("serialize memory record: {e}")))?;
    if let Some(embedding) = &memory.embedding {
        data[EMBEDDING_KEY] = serde_json::json!(embedding);
    }
    executor
        .execute_json(
            "data/create",
            serde_json::json!({
                "collection": MEMORIES_COLLECTION,
                "dbPath": persona_db_handle(persona_id),
                "id": memory.record.id,
                "data": data,
            }),
        )
        .await
        .map_err(|e| {
            CommandError::Internal(format!(
                "memory/append-memory: durable write to {} failed: {e}",
                persona_db_handle(persona_id)
            ))
        })?;
    Ok(())
}

/// Hydrate a persona's corpus from its durable store when no corpus is cached
/// (i.e. first touch after a core restart). A fresh persona with no store yet
/// hydrates to an EMPTY corpus (the sqlite adapter treats a missing table as
/// zero rows) — an honest empty, not a fallback. Returns how many memories
/// were loaded, or `None` when the corpus was already cached.
pub(crate) async fn hydrate_corpus_if_missing(
    state: &MemoryState,
    persona_id: &crate::identity::PersonaRef,
) -> Result<Option<usize>, crate::sdk_codegen::CommandError> {
    use crate::sdk_codegen::CommandError;
    if state.memory_manager.has_corpus(persona_id) {
        return Ok(None);
    }
    let executor = state.executor().map_err(CommandError::Internal)?;
    let listed = executor
        .execute_json(
            "data/list",
            serde_json::json!({
                "collection": MEMORIES_COLLECTION,
                "dbPath": persona_db_handle(persona_id),
                "filter": { "persona_id": persona_id },
            }),
        )
        .await
        .map_err(|e| {
            CommandError::Internal(format!(
                "memory hydrate: data/list on {} failed: {e}",
                persona_db_handle(persona_id)
            ))
        })?;
    let items = listed
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut memories: Vec<crate::memory::CorpusMemory> = Vec::with_capacity(items.len());
    for item in &items {
        // Each item is a DataRecord envelope; the memory row is its `data`.
        let Some(data) = item.get("data") else {
            continue;
        };
        // The ORM returns row keys camelCased (TS compatibility); MemoryRecord
        // is snake_case on the wire. Fold TOP-LEVEL keys back — nested objects
        // (`context`) keep their own keys untouched.
        let data = match data {
            serde_json::Value::Object(obj) => serde_json::Value::Object(
                obj.iter()
                    .map(|(k, v)| (crate::orm::adapter::naming::to_snake_case(k), v.clone()))
                    .collect(),
            ),
            other => other.clone(),
        };
        let record: crate::memory::MemoryRecord =
            serde_json::from_value(data.clone()).map_err(|e| {
                CommandError::Internal(format!(
                    "memory hydrate: row in '{MEMORIES_COLLECTION}' is not a MemoryRecord: {e}"
                ))
            })?;
        let embedding: Option<Vec<f32>> = data
            .get(EMBEDDING_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        memories.push(crate::memory::CorpusMemory { record, embedding });
    }
    let count = memories.len();
    state
        .memory_manager
        .load_corpus(persona_id, memories, Vec::new());
    Ok(Some(count))
}

