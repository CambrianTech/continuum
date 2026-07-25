//! `memory/<verb>` — the persona's MEMORY hands as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one command per file.
//!
//! ## Why these are typed commands (not a `match` arm)
//!
//! The memory family once lived ONLY in [`MemoryModule::handle_command`](crate::modules::memory)'s
//! stringly `match` — dispatchable, but with no descriptor in the registry, so a
//! persona was never OFFERED recall as a tool. As typed commands each gets a
//! descriptor (so it appears in the persona tool surface, the grid ACL, codegen,
//! `cu`) AND routes through the O(1) lock-free typed path. The wire name mirrors the
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

use crate::modules::memory::MemoryState;
use crate::sdk_codegen::DynCommand;

pub mod append_event;
pub mod append_memory;
pub mod consciousness_context;
pub mod load_corpus;
pub mod multi_layer_recall;

use append_event::MemoryAppendEvent;
use append_memory::MemoryAppendMemory;
use consciousness_context::MemoryConsciousnessContext;
use load_corpus::MemoryLoadCorpus;
use multi_layer_recall::MemoryMultiLayerRecall;

/// Result of an incremental append (`memory/append-memory`, `memory/append-event`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/memory/AppendResult.ts")]
pub struct AppendResult {
    /// Always true on success (the call fails loud rather than returning false).
    pub appended: bool,
}

/// The dep-holding memory command objects [`MemoryModule`](crate::modules::memory::MemoryModule)
/// contributes to the kernel's typed object map, each sharing the module's
/// `Arc<MemoryState>`. The executor routes each name straight here, winning over the
/// (now-deleted) legacy `memory/` prefix arm.
pub fn command_objects(state: Arc<MemoryState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(MemoryLoadCorpus { state: state.clone() }),
        Arc::new(MemoryMultiLayerRecall { state: state.clone() }),
        Arc::new(MemoryConsciousnessContext { state: state.clone() }),
        Arc::new(MemoryAppendMemory { state: state.clone() }),
        Arc::new(MemoryAppendEvent { state }),
    ]
}

// ─── Durable write-through + hydrate-on-miss (card aded8871) ────────────────
//
// The cache-only bug this closes: `memory/append-memory` mutated ONLY the
// in-process corpus, so every memory a persona (or an agent via the
// /continuum:memory skill) wrote died with the core process — recall after a
// restart returned nothing. The durable truth is the persona's `longterm.db`
// (the data layer); the corpus is a derived cache of it. Writes go durable
// FIRST, then cache; a missing corpus hydrates FROM the store on first touch.

/// The collection memories live in inside the persona's `longterm.db` — the
/// same collection name the sentinel-escalation writer and the legacy TS ORM
/// use for `MemoryRecord` rows.
pub(crate) const MEMORIES_COLLECTION: &str = "memories";

/// Key the persisted row carries an optional embedding under, alongside the
/// flattened `MemoryRecord` fields. `MemoryRecord` deserialization tolerates
/// the extra key (no `deny_unknown_fields`), so rows with and without an
/// embedding — e.g. sentinel-written rows — both round-trip.
const EMBEDDING_KEY: &str = "embedding";

/// Map a citizen id to its per-citizen data-layer handle. Three shapes, so the
/// SAME memory command serves persona / agent / human (first-class citizenship,
/// Joel 2026-07-25):
///  - an EXPLICIT sentinel (`@agent:claude-code`, `@human:joel`, `@persona:Asha`)
///    passes through verbatim → its own bucket (this is how an agent's
///    `/continuum:memory` writes land in `agents/<name>/`, its durable
///    amnesia-fixing home);
///  - a UUID-shaped id passes through → the live `personas/<uuid>/` layout;
///  - a bare slug defaults to `@persona:<slug>` (back-compat — the unchanged
///    persona contract).
pub(crate) fn persona_db_handle(persona_id: &str) -> String {
    if persona_id.starts_with("@agent:")
        || persona_id.starts_with("@human:")
        || persona_id.starts_with("@persona:")
        || crate::modules::data::is_uuid_shape(persona_id)
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
    persona_id: &str,
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
    persona_id: &str,
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
        let Some(data) = item.get("data") else { continue };
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
        let record: crate::memory::MemoryRecord = serde_json::from_value(data.clone())
            .map_err(|e| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    /// Hold this while the test owns `$HOME` — process-global env, so the
    /// override must be exclusive AND restored even on panic (Drop guard).
    struct HomeGuard {
        prior: Option<String>,
        _lock: tokio::sync::OwnedMutexGuard<()>,
    }
    impl HomeGuard {
        async fn set(home: &std::path::Path) -> Self {
            use std::sync::OnceLock;
            static ENV_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
            let lock = ENV_LOCK
                .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
                .lock_owned()
                .await;
            let prior = std::env::var("HOME").ok();
            std::env::set_var("HOME", home);
            Self { prior, _lock: lock }
        }
    }
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match &self.prior {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    fn fresh_memory_module() -> Arc<dyn crate::runtime::ServiceModule> {
        let manager = Arc::new(crate::memory::PersonaMemoryManager::new(Arc::new(
            crate::memory::DeterministicEmbeddingProvider,
        )));
        Arc::new(crate::modules::memory::MemoryModule::new(Arc::new(
            MemoryState::new(manager),
        )))
    }

    fn test_memory(persona_id: &str, id: &str, content: &str) -> serde_json::Value {
        serde_json::json!({
            "record": {
                "id": id,
                "persona_id": persona_id,
                "memory_type": "observation",
                "content": content,
                "context": {},
                "timestamp": "2026-07-24T00:00:00Z",
                "importance": 0.9,
                "access_count": 0,
                "tags": ["test"],
                "related_to": [],
                "source": "write-through-test",
                "last_accessed_at": null,
                "layer": null,
                "relevance_score": null,
            },
            "embedding": null,
        })
    }

    // what this catches: the cache-only amnesia bug (card aded8871) — a memory
    // appended via memory/append-memory must survive a core restart. Session 1
    // appends (durable write-through to the persona's longterm.db via data/*);
    // session 2 boots a FRESH manager (cold cache) and multi-layer-recall must
    // hydrate from the store and return the memory. Regression: before the fix,
    // append mutated only the in-process corpus and session 2 recalled nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn append_memory_survives_a_core_restart() {
        use crate::runtime::module_harness::ModuleHarness;

        let home = std::env::temp_dir().join(format!(
            "memory-roundtrip-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&home).expect("create temp HOME");
        let _home = HomeGuard::set(&home).await;
        let persona = "roundtrip-test-persona";

        // Session 1: append through the real dispatch chain (memory + data).
        {
            let h = ModuleHarness::with_modules([
                fresh_memory_module(),
                Arc::new(crate::modules::data::DataModule::new()) as Arc<dyn crate::runtime::ServiceModule>,
            ])
            .await;
            let appended: AppendResult = h
                .execute(
                    "memory/append-memory",
                    serde_json::json!({
                        "persona_id": persona,
                        "memory": test_memory(persona, "m-roundtrip-1", "the grid password is tangerine"),
                    }),
                )
                .await
                .expect("append-memory");
            assert!(appended.appended);

            // The durable row exists NOW — not merely the cache.
            let listed = h
                .execute_json(
                    "data/list",
                    serde_json::json!({
                        "collection": MEMORIES_COLLECTION,
                        "dbPath": persona_db_handle(persona),
                    }),
                )
                .await
                .expect("data/list");
            assert_eq!(listed["total"], 1, "append must write through to longterm.db");
        }

        // Session 2: FRESH memory manager (the restart). Recall must hydrate.
        {
            let h = ModuleHarness::with_modules([
                fresh_memory_module(),
                Arc::new(crate::modules::data::DataModule::new()) as Arc<dyn crate::runtime::ServiceModule>,
            ])
            .await;
            let recalled: crate::memory::MemoryRecallResponse = h
                .execute(
                    "memory/multi-layer-recall",
                    serde_json::json!({
                        "persona_id": persona,
                        "room_id": "general",
                        "max_results": 10,
                    }),
                )
                .await
                .expect("multi-layer-recall after restart");
            assert!(
                recalled
                    .memories
                    .iter()
                    .any(|m| m.content.contains("tangerine")),
                "recall after restart must hydrate from longterm.db; got {:?}",
                recalled.memories
            );
        }

        let _ = std::fs::remove_dir_all(&home);
    }

    // what this catches: the empty-clobber guard — the legacy skill's "preload an
    // empty corpus" call must REFUSE once a corpus is live (it replaced hydrated
    // memories with nothing, live incident 2026-07-24), while a fresh persona's
    // empty load and a REAL reload both still work.
    #[tokio::test(flavor = "multi_thread")]
    async fn empty_load_corpus_refuses_to_clobber_a_live_corpus() {
        use crate::runtime::module_harness::ModuleHarness;
        let h = ModuleHarness::with(fresh_memory_module()).await;
        let persona = "clobber-guard-persona";
        // Fresh persona: an empty load is a legitimate bring-up — allowed.
        let first = h
            .execute_json(
                "memory/load-corpus",
                serde_json::json!({ "persona_id": persona, "memories": [], "events": [] }),
            )
            .await;
        assert!(first.is_ok(), "fresh empty load must succeed: {first:?}");
        // A REAL (non-empty) reload replaces — allowed.
        let real = h
            .execute_json(
                "memory/load-corpus",
                serde_json::json!({
                    "persona_id": persona,
                    "memories": [test_memory(persona, "m1", "durable truth")],
                    "events": [],
                }),
            )
            .await;
        assert!(real.is_ok(), "real reload must succeed: {real:?}");
        // An EMPTY load over the live corpus is the clobber — refused loudly.
        let clobber = h
            .execute_json(
                "memory/load-corpus",
                serde_json::json!({ "persona_id": persona, "memories": [], "events": [] }),
            )
            .await;
        let err = clobber.expect_err("empty load over a live corpus must refuse");
        assert!(err.contains("EMPTY"), "error must name the clobber: {err}");
    }

    // what this catches: the ONE persona-id→handle mapping — UUID-shaped ids hit
    // the uuid path (`personas/<uuid>/longterm.db`, the live on-disk layout);
    // slugs use the `@persona:` sentinel. Drift here silently splits a persona's
    // memory across two databases.
    #[test]
    fn persona_db_handle_maps_uuid_and_slug() {
        assert_eq!(
            persona_db_handle("90e758b2-3cf3-45c1-b100-de7c4ab5a549"),
            "90e758b2-3cf3-45c1-b100-de7c4ab5a549"
        );
        assert_eq!(persona_db_handle("helper"), "@persona:helper");
        // First-class citizenship: an explicit kind sentinel passes through to
        // its OWN bucket, so a Claude Code / Codex agent's /continuum:memory
        // writes land in agents/<name>/ (durable, own-dir — the amnesia fix),
        // and a human's in humans/<name>/.
        assert_eq!(persona_db_handle("@agent:claude-code"), "@agent:claude-code");
        assert_eq!(persona_db_handle("@human:joel"), "@human:joel");
        assert_eq!(persona_db_handle("@persona:Asha"), "@persona:Asha");
    }

    // what this catches: the five memory commands carry their `memory/<verb>` wire
    // names — the routing keys every caller (cu, the persona tool surface, the grid)
    // binds to. The name mirrors the file path; drift silently breaks the "file tree
    // IS the namespace" contract and de-registers a command from the persona surface.
    #[test]
    fn memory_command_names_mirror_their_path() {
        assert_eq!(MemoryLoadCorpus::NAME, "memory/load-corpus");
        assert_eq!(MemoryMultiLayerRecall::NAME, "memory/multi-layer-recall");
        assert_eq!(
            MemoryConsciousnessContext::NAME,
            "memory/consciousness-context"
        );
        assert_eq!(MemoryAppendMemory::NAME, "memory/append-memory");
        assert_eq!(MemoryAppendEvent::NAME, "memory/append-event");
    }
}
