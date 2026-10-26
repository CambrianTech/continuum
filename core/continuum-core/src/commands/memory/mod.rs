//! `memory/<verb>` in core: the `memory/consolidate` command (it submits training, so it
//! needs the genome and persona and cannot leave) plus the end-to-end tests that drive the
//! whole family through the data module. Every other `memory/*` command, the durable
//! write-through helpers and `MemoryState` live in `continuum-memory` and are re-exported
//! here under their old paths.

use std::sync::Arc;

use crate::modules::memory::MemoryState;
use crate::sdk_codegen::DynCommand;

pub use continuum_memory::commands::*;

pub mod consolidate;

use consolidate::MemoryConsolidate;

/// The family's command objects: the crate's, plus `memory/consolidate`; the kernel keys them
/// by name, so their order does not matter.
pub fn command_objects(state: Arc<MemoryState>) -> Vec<Arc<dyn DynCommand>> {
    let mut objects = continuum_memory::commands::command_objects(state.clone());
    objects.push(Arc::new(MemoryConsolidate { state }));
    objects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;
    use continuum_memory::commands::{
        append_event::MemoryAppendEvent, append_memory::MemoryAppendMemory,
        consciousness_context::MemoryConsciousnessContext, load_corpus::MemoryLoadCorpus,
        multi_layer_recall::MemoryMultiLayerRecall, share::MemoryShare,
    };

    /// The one crate-wide home lock (`crate::paths::test_home`, #4082) — pins HOME, USERPROFILE,
    /// HF_HOME and CONTINUUM_HOME together and restores them on drop.
    use crate::paths::test_home::HomeGuard;

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

    // What this catches: resolving a short persona ID is insufficient if the
    // handler still queries @persona:<prefix> or filters rows by that prefix.
    #[tokio::test]
    async fn consolidate_short_persona_reads_the_canonical_corpus() {
        use crate::runtime::module_harness::ModuleHarness;
        let home = tempfile::tempdir().expect("test: temporary memory home");
        let _home = HomeGuard::set(home.path()).await;
        let persona = uuid::Uuid::new_v4();
        let canonical = persona.to_string();
        // The registry is process-global; remove only this test's unique card,
        // including when an assertion unwinds. No shared registry reset.
        struct RegisteredCard(String);
        impl Drop for RegisteredCard {
            fn drop(&mut self) {
                crate::persona::card::remove(&self.0);
            }
        }
        let _card = RegisteredCard(canonical.clone());
        crate::persona::card::register(crate::persona::card::PersonaCard::genesis(
            persona,
            "received-lesson-fixture",
            1,
            None,
        ));
        let h = ModuleHarness::with_modules([
            fresh_memory_module(),
            Arc::new(crate::modules::data_wiring::data_module())
                as Arc<dyn crate::runtime::ServiceModule>,
        ])
        .await;
        let mut memory = test_memory(&canonical, "shared-canonical", "A received lesson.");
        memory["record"]["memory_type"] = serde_json::json!("shared");
        memory["record"]["context"] = serde_json::json!({"scope": "rust", "shared_by": "teacher"});
        let appended: AppendResult = h
            .execute(
                "memory/append-memory",
                serde_json::json!({
                    "persona_id": canonical,
                    "memory": memory,
                }),
            )
            .await
            .expect("test: append shared lesson to full UUID corpus");
        assert!(appended.appended);
        let result: consolidate::ConsolidateResult = h
            .execute(
                "memory/consolidate",
                serde_json::json!({
                    "persona_id": &canonical[..8],
                    "base_model": "synthetic",
                }),
            )
            .await
            .expect("test: consolidate resolves short persona");
        assert_eq!(
            result.shared_lessons, 1,
            "the canonical stored lesson must be found"
        );
        // No training module exists in this harness: the found lesson is refused,
        // so this test exercises the real read path without dispatching training.
        assert_eq!(result.consolidated, 0);
        assert_eq!(result.latest_consolidated_ts, None);
    }

    // what this catches: the cache-only amnesia bug (card aded8871) — a memory
    // appended via memory/append-memory must survive a core restart. Session 1
    // appends (durable write-through to the persona's longterm.db via data/*);
    // session 2 boots a FRESH manager (cold cache) and multi-layer-recall must
    // hydrate from the store and return the memory. Regression: before the fix,
    // append mutated only the in-process corpus and session 2 recalled nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn append_memory_survives_a_core_restart() {
        roundtrip_survives_restart("roundtrip-test-persona", "the grid password is tangerine")
            .await;
    }

    // what this catches (#224): the SAME durable-survival must hold for AGENT and HUMAN
    // citizens, not just personas — "both agents (me + BigMama) and humans (Joel) get the same
    // ORM-backed engram; no one re-boots amnesiac." `persona_db_handle` routes the `@agent:` /
    // `@human:` sentinels to their OWN store buckets (agents/, humans/); if that routing or a
    // bucket's hydrate-on-miss were wrong, an agent would forget across a restart while
    // personas remembered. Regression guard for the citizenship-homes work.
    #[tokio::test(flavor = "multi_thread")]
    async fn agent_citizen_memory_survives_a_core_restart() {
        roundtrip_survives_restart("@agent:claude-code", "seam-2 lands in genome eviction").await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn human_citizen_memory_survives_a_core_restart() {
        roundtrip_survives_restart("@human:operator", "the raid drives were 420 at microcenter")
            .await;
    }

    /// Shared body: append a memory for `persona_id` through the real dispatch chain, prove the
    /// durable row lands in THAT citizen's bucket, then boot a FRESH manager (the restart) and
    /// assert recall hydrates it from the store. Parameterized over the citizen id so
    /// persona / `@agent:` / `@human:` all exercise the identical write-through + hydrate path.
    async fn roundtrip_survives_restart(persona_id: &str, secret: &str) {
        use crate::runtime::module_harness::ModuleHarness;

        let safe = persona_id.replace(|c: char| matches!(c, ':' | '/' | '@'), "_");
        let home = std::env::temp_dir().join(format!(
            "memory-roundtrip-{}-{}-{:?}",
            safe,
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&home).expect("create temp HOME");
        let _home = HomeGuard::set(&home).await;

        // Session 1: append through the real dispatch chain (memory + data).
        {
            let h = ModuleHarness::with_modules([
                fresh_memory_module(),
                Arc::new(crate::modules::data_wiring::data_module())
                    as Arc<dyn crate::runtime::ServiceModule>,
            ])
            .await;
            let appended: AppendResult = h
                .execute(
                    "memory/append-memory",
                    serde_json::json!({
                        "persona_id": persona_id,
                        "memory": test_memory(persona_id, "m-roundtrip-1", secret),
                    }),
                )
                .await
                .expect("append-memory");
            assert!(appended.appended);

            // The durable row exists NOW — not merely the cache — in THIS citizen's bucket.
            let listed = h
                .execute_json(
                    "data/list",
                    serde_json::json!({
                        "collection": MEMORIES_COLLECTION,
                        "dbPath": persona_db_handle(&persona_id.into()),
                    }),
                )
                .await
                .expect("data/list");
            assert_eq!(
                listed["total"], 1,
                "append must write through to {persona_id}'s longterm.db"
            );
        }

        // Session 2: FRESH memory manager (the restart). Recall must hydrate.
        {
            let h = ModuleHarness::with_modules([
                fresh_memory_module(),
                Arc::new(crate::modules::data_wiring::data_module())
                    as Arc<dyn crate::runtime::ServiceModule>,
            ])
            .await;
            let recalled: crate::memory::MemoryRecallResponse = h
                .execute(
                    "memory/multi-layer-recall",
                    serde_json::json!({
                        "persona_id": persona_id,
                        "room_id": "general",
                        "max_results": 10,
                    }),
                )
                .await
                .expect("multi-layer-recall after restart");
            assert!(
                recalled.memories.iter().any(|m| m.content.contains(secret)),
                "recall after restart must hydrate {persona_id}'s memory from longterm.db; got {:?}",
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
            persona_db_handle(&"90e758b2-3cf3-45c1-b100-de7c4ab5a549".into()),
            "90e758b2-3cf3-45c1-b100-de7c4ab5a549"
        );
        assert_eq!(persona_db_handle(&"helper".into()), "@persona:helper");
        // First-class citizenship: an explicit kind sentinel passes through to
        // its OWN bucket, so a Claude Code / Codex agent's /continuum:memory
        // writes land in agents/<name>/ (durable, own-dir — the amnesia fix),
        // and a human's in humans/<name>/.
        assert_eq!(
            persona_db_handle(&"@agent:claude-code".into()),
            "@agent:claude-code"
        );
        assert_eq!(
            persona_db_handle(&"@human:operator".into()),
            "@human:operator"
        );
        assert_eq!(persona_db_handle(&"@persona:Asha".into()), "@persona:Asha");
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
        assert_eq!(MemoryShare::NAME, "memory/share");
    }
}
