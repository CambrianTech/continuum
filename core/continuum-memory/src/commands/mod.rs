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
mod persistence;
pub use persistence::{shared_lessons, SharedLessonSelection};
#[cfg(test)]
mod persistence_tests;

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

/// Write one typed memory and its native payload through the shared storage owner.
/// Fails when the storage provider isn't installed or the write fails — a memory
/// that only landed in cache is the exact lie this seam exists to kill.
pub(crate) async fn persist_memory(
    state: &MemoryState,
    persona_id: &crate::identity::PersonaRef,
    memory: &crate::memory::CorpusMemory,
) -> Result<(), crate::sdk_codegen::CommandError> {
    use crate::sdk_codegen::CommandError;
    use crate::memory::persistent::PersistentMemory;
    use crate::orm::store::{NativeEntityWrite, OrmStore};

    if memory.record.persona_id != persona_id.as_str() {
        return Err(CommandError::Invalid("memory owner differs from the destination persona".into()));
    }
    let id = uuid::Uuid::parse_str(&memory.record.id)
        .map_err(|error| CommandError::Invalid(error.to_string()))?;
    let adapter = state.storage().map_err(CommandError::Internal)?
        .adapter(&persona_db_handle(persona_id)).await.map_err(CommandError::Internal)?;
    let store = OrmStore::<PersistentMemory>::new(adapter).await
        .map_err(|error| CommandError::Internal(error.to_string()))?;
    let mut payloads = std::collections::BTreeMap::new();
    if let Some(embedding) = &memory.embedding {
        let payload = persistence::native_embedding(embedding)?;
        payloads.insert(EMBEDDING_KEY.to_owned(), payload);
    }
    let entity = PersistentMemory::from_record(memory.record.clone());
    store.save_native(NativeEntityWrite { id, entity: &entity, payloads }).await
        .map_err(|error| CommandError::Internal(error.to_string()))
}
/// Hydrate a persona's corpus from its durable store when no corpus is cached
/// (i.e. first touch after a core restart). The data owner admits the declared
/// schema before reading; schema/read failures remain errors. Each request is
/// bounded and keyset traversal visits matching identities within one pinned
/// snapshot shared with native payloads. Returns how many memories were
/// loaded, or `None` when the corpus was already cached.
pub(crate) async fn hydrate_corpus_if_missing(
    state: &MemoryState,
    persona_id: &crate::identity::PersonaRef,
) -> Result<Option<usize>, crate::sdk_codegen::CommandError> {
    use crate::sdk_codegen::CommandError;
    if state.memory_manager.has_corpus(persona_id) {
        return Ok(None);
    }
    let adapter = state.storage().map_err(CommandError::Internal)?
        .adapter(&persona_db_handle(persona_id)).await.map_err(CommandError::Internal)?;
    let memories = persistence::load(adapter, persona_id).await?;
    let count = memories.len();
    state
        .memory_manager
        .load_corpus(persona_id, memories, Vec::new());
    Ok(Some(count))
}
