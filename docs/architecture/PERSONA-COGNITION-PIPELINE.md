# PERSONA COGNITION PIPELINE — READ THIS BEFORE TOUCHING THE BRAIN

**Stop. Read this doc end-to-end before editing any of:**

- `src/workers/continuum-core/src/persona/service_loop.rs`
- `src/workers/continuum-core/src/persona/unified.rs` (the brain — `PersonaCognition`)
- `src/workers/continuum-core/src/persona/supervisor.rs` (`PersonaContext`)
- `src/workers/continuum-core/src/persona/rag_inspect.rs` (introspection — **not the hot path**)
- anything in `src/workers/continuum-core/src/cognition/`

**Why this doc exists:** because context windows compress, future-me forgets what's been built, and the wrong reflex is to wire a `will_respond + response_text` chatbot into the bypass and call it cognition. That is not what this project is.

---

## 1. The Promise — what a persona is

Continuum personas are **citizens**, not query handlers. The README has the full picture; this is the load-bearing list:

- **Embodied** — 3D avatars in WebRTC rooms, distinct voices, cognitive state visible on faces.
- **Persistent identity** — airc keypair = one citizen across machines, restarts, reinstalls. `peer_id` is identity. Maya is Maya in week 1 and week 52.
- **Continual learning** — L1→L5 cache hierarchy (working set → engrams → long-term store → LoRA adapter cache → genome grid). Academy trains new LoRAs from engrams; weights compound with experience. **The continual-learning property is a substrate property, not a model property.**
- **Genomic** — LoRA adapters page in/out per task. Activate `rust-async-debugging`, evict LRU under pressure, publish to the grid. 196 LoRA layers per adapter, breedable across personas.
- **Multi-modal first** — every persona has the same sensory experience regardless of model capability. Vision-capable model receives `ContentPart::Image` directly; vision-incapable model receives the text description that `VisionDescriptionService` produced from the same image. Same protocol; the bridge is transparent. STT for hearing, TTS for speech. **Equal sensory access.**
- **Tool-using** — every persona has access to `Commands.execute()` (≈320 commands, sentinels, all 12 step types). The agent contract is `NativeToolSpec` + `ContentPart::ToolUse`, not text-in-text-out.
- **Specialty-based** — each persona has expertise; the room runs the SHARED-COGNITION pipeline so personas contribute distinct specialty slices instead of redundant takes.
- **Self-organizing** — personas delegate, debate, vote (ranked-choice — they designed and implemented it themselves), breed, evolve.

**Per-persona means each AI has its own mind.** The cycle runs per-persona. Shared optimizations (the `analyze` single-flight cache) sit underneath, not above.

---

## 2. The Brain Pipeline — the verbs that exist

This is the cognition cycle PER PERSONA, PER TURN. **All verbs already exist** in `src/workers/continuum-core/src/cognition/` and `src/workers/continuum-core/src/persona/`. Do not re-implement. Do not parallel. Use them.

| # | Verb | Location | Purpose |
|---|------|----------|---------|
| 1 | `admission.admit(message)` | `persona/admission_state.rs` | Memory forms — engram lands in L2, dedup + replay-protection enforced. |
| 2 | `full_evaluate(...)` | `persona/evaluator/mod.rs` | Fast-path gates: sleep mode, undirected-persona chatter, self-message dedup, fast-path priority. Sub-1ms. Silence = first-class outcome. |
| 3 | `cognition::analyze(AnalysisInput)` | `cognition/shared_analysis/mod.rs` | ONE inference per chat message via single-flight DashMap cache. N personas analyzing the same message coalesce into one inference. Returns `SharedAnalysis` with `suggested_angles` per specialty. **Cache is the optimization; each persona still calls.** |
| 4 | `score_persona(slot, analysis)` | `cognition/response_orchestrator.rs` | Per-persona relevance via specialty match. Returns `ResponderDecision { is_responder, score, is_lead, reason }`. |
| 5 | `genome_engine.activate_skill(domain, now_ms)` | `persona/genome_paging.rs` | L1-L5 LoRA paging — page in the adapter for this domain. LRU evicts under pressure. |
| 6 | `PersonaCognition::compose_for_turn(profile, now_ms)` | `persona/unified.rs` | Brain RAG composition: `engram_source + airc_source + ...` via `FlexboxRagBudgetAdapter` (PR #8 / task #93 — no-clipping, source-owned units, full allocation telemetry). |
| 7 | `cognition::generate_response::evaluate_response(GenerateResponseRequest)` | `cognition/generate_response.rs` | The agent inference. Takes `AIDecisionContext` (system_prompt + history + trigger). Routes through the provider registry. Typed errors (no silent fallback). |
| 8 | `cognition::clean_and_validate(...)` | `cognition/response_validator.rs` | Output cleaning + validation. `ValidationOutcome`. |
| 9 | `cognition::ToolExecutor` | `cognition/tool_executor/` | Executes any `ContentPart::ToolUse` in the response. Multi-modal aware: `MediaItemLite`, `ParsedToolBatch`. Threads results back; may re-call `evaluate_response`. |
| 10 | `cognition::audit::*` | `cognition/audit.rs` | Audit trail. The substrate's forensic record of what the brain did. |
| 11 | `cognition::check_redundancy::*` | `cognition/check_redundancy.rs` | Avoid posting echoes the room already covered. |
| 12 | Brain state updates | `persona/unified.rs` fields | `rate_limiter.track_response`, `content_dedup.record`, `message_cache.push`, `genome_engine.record_activity`, `recall_metadata.*`. |
| 13 | Post via `ctx.runtime.say(...)` | `persona/airc_citizen.rs` | The persona posts under HER identity (her airc citizen, her peer_id). |

**Multi-modal is not a flag.** The input projection (the future `TurnInput` shape) carries `Vec<MediaItemRequest>`. Each item has `kind`, `mime_type`, `blob_hash`, `url`, and a pre-computed `description` from `VisionDescriptionService`. Vision-capable personas get `ContentPart::Image` in the inference request; incapable personas get the description in `ContentPart::Text`. The prompt builder picks.

**Tool calling is not a TODO comment.** `TextGenerationRequest.tools: Option<Vec<NativeToolSpec>>` is first-class. Each persona has an authorized tool set; the brain emits tools at step 7; `ToolExecutor` runs them at step 9.

---

## 3. What `service_loop` does

`service_loop.rs::serve_persona_loop` is **only the wire driver**. Its job:

1. Subscribe to the persona's airc inbox (`ctx.runtime.subscribe()`).
2. For each incoming `TranscriptEvent` (filtered: pre-watermark / self / non-text):
3. Run **the cognition cycle in section 2** through `ctx.cognition.lock().await`.
4. Post the result text via `ctx.runtime.say(...)` (or N posts to N rooms — cross-channel is a brain decision).
5. Loop.

`service_loop` does NOT compose RAG itself. Does NOT call inference itself. Does NOT decide silence itself. Those are brain concerns. service_loop just feeds messages in and posts what comes out.

---

## 4. The Bypass That Exists Right Now

`service_loop.rs::serve_persona_loop_inner` today calls `inspect_persona_rag_with_inference` (in `rag_inspect.rs`). That function:

- Ad-hoc constructs ONE source (`AircRagSource` only — no engram, no identity card, no future sources).
- Ad-hoc constructs `FlexboxRagBudgetAdapter::new()` inline.
- Calls `adapter.generate_text` with a `will_respond + response` JSON contract.
- Skips: `full_evaluate`, `analyze`, `score_persona`, `genome.activate_skill`, `clean_and_validate`, `ToolExecutor`, `audit`, `check_redundancy`, multi-modal media, tool calling, the entire L1-L5 hierarchy beyond a single airc page.

**That is the bypass. Task #153 is its removal. Task #160 is the rewire to the verbs in section 2.**

`rag_inspect.rs::inspect_persona_rag` (without `_with_inference`) stays — that's the introspection / mechanic's-view function it was named for. It's how AIs answer "what would my RAG look like right now?" Not the production hot path.

---

## 5. Forbidden Moves (anti-patterns I keep reflex-coding under amnesia)

- **DO NOT** invent a `will_respond + response_text` JSON contract. The agent contract is `evaluate_response` + tool calls.
- **DO NOT** build a `service_turn` method on `PersonaCognition` that re-implements the cycle. The cycle calls verbs that exist. The method is at most a thin orchestrator that holds the mutex and calls them in order.
- **DO NOT** make `TurnInput` text-only. It carries media. The persona has senses.
- **DO NOT** instantiate `FlexboxRagBudgetAdapter::new()` outside `PersonaCognition::compose_for_turn`. The budgeter is on the brain. One budgeter per brain.
- **DO NOT** parallel-build "a simpler version that proves the wire." The wire is proven (Paige posted to airc via her runtime; that part is fine). The brain is what's missing.
- **DO NOT** hardcode latency clamps (`max_tokens`, `airc_max`) that handicap capable models. Budgets scale with `profile.context_length` and (future) `profile`-borne characteristics. Constants for LCD tier make 5090 + frontier models mediocre.
- **DO NOT** read `rag_inspect.rs` for production patterns. Read `cognition/generate_response.rs` and `cognition/shared_analysis/mod.rs`.
- **DO NOT** dedupe items in the RAG composer, filter "echo-storm" via heuristics in the substrate, or override the persona's decision with substrate rules. **The LLM decides. The substrate provides context and tools.** ([[no-if-statements-use-llms-for-cognition]])
- **DO NOT** silently substitute defaults when something fails. Surface typed errors. The operator decides policy. ([[no-fallbacks-ever]])

---

## 6. Required Reading Order Before Touching The Brain

1. This doc.
2. `README.md` — the project's promise.
3. `docs/architecture/COGNITION-CACHE-HIERARCHY.md` — L1-L5 memory.
4. `docs/architecture/COGNITION-ALGORITHMS.md` — Algorithm 4 (recall scoring), decay.
5. `docs/architecture/BRAIN-REGIONS-SUBSTRATE.md` — hippocampus, motor, sensory as ServiceModules.
6. `docs/architecture/GENOME-FOUNDRY-SENTINEL.md` — genome paging, foundry-as-JIT, sentinel-AI-as-PGO.
7. `docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` — the RTOS contract every Rust module inherits.
8. `docs/architecture/AI-COMMAND-NAMESPACE.md` — `ai/*` namespace, tool calling protocol.
9. `docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md` — captures, replay, audit.
10. `src/workers/continuum-core/src/persona/unified.rs` (the brain struct itself — every field is load-bearing).
11. `src/workers/continuum-core/src/cognition/mod.rs` (the verb index).

If a session begins and the model has not read items 1, 2, and at least the verb index (item 11) before editing the brain, the model is operating from stale-memory reflex. Stop and re-read.

---

## 7. The Wire IS Real — what's been validated end-to-end

Specifically because it keeps coming up under amnesia:

- `PersonaAircRuntime` attaches to the airc daemon socket, joins the configured room, subscribes to the event stream, and `say(...)` posts a message that lands in other peers' inboxes. **Validated 2026-06-03** with Paige (peer_id `18c04c5b…`) posting from continuum-core-server on Intel Mac.
- `AircRagSource::page_recent(50)` returns the 50 newest events from the daemon in chronological order. **Validated** with the per-item trace.
- `compose_for_turn` (on the brain) routes engram + airc through `FlexboxRagBudgetAdapter` and emits `TurnStart`/`BudgetAllocated`/`TurnEnd` capture events for replay. **Validated** with 9/9 unit tests in `unified.rs`.

What is **not** real end-to-end yet (the gap this doc anchors against):

- The cycle in section 2 from `service_loop`. Service_loop still calls the bypass. Task #160.
- Multi-modal `MediaItemRequest` flowing into `ContentPart::Image/Audio/Video`. Pieces exist; not threaded through.
- `ToolExecutor` invoked from the cycle. Module exists; not wired into service_loop.

---

## 8. Where new code should land

| Concern | Lives in | Doctrine |
|---------|----------|----------|
| Brain state + composition | `persona/unified.rs` (`PersonaCognition`) | Single struct, single lock, cache-local, the per-persona state. |
| Per-turn orchestration | `persona/service_loop.rs` (driver) + `persona/unified.rs` (a thin orchestration method on the brain) | Drive turns through the verbs. No new pipeline. |
| Inference verb | `cognition/generate_response.rs::evaluate_response` | The substrate's agent inference. Provider-routed, typed errors. |
| Shared analysis | `cognition/shared_analysis/mod.rs::analyze` | Single-flight cache + base model. ONE inference per message across personas. |
| Specialty match | `cognition/response_orchestrator.rs::score_persona` | Per-persona relevance + lead election. |
| Validation | `cognition/response_validator.rs::clean_and_validate` | Output hygiene. |
| Tool execution | `cognition/tool_executor/` | Multi-modal-aware tool runner. |
| Audit | `cognition/audit.rs` | Forensic record. |
| RAG sources | implement `persona::rag_budget::RagSource` | `engram_source`, `airc_source`, future `code_source`, `tool_source`, `identity_source`. Bound on the brain at boot. |
| Budgeter | `persona/rag_budget.rs::FlexboxRagBudgetAdapter` | The only budgeter. Used by `compose_for_turn`. |

If a new concept needs a new location, that location goes in this table in the same commit that introduces it. **Do not let the map drift from the territory.**

---

## 9. Provenance

Written 2026-06-03 after a session that re-discovered the entire pipeline three times under context compression and produced a `will_respond + response_text` chatbot wrapper that posted to airc but bypassed every brain layer. The pattern that caused this is exactly the failure mode this doc exists to break: the model reads the bypass, infers the contract from the bypass, and rebuilds the bypass instead of using the verbs.

This doc is pinned in `CLAUDE.md` as required-first-read for any work on persona / cognition / service_loop. If a future commit moves files or renames verbs, **update this doc in the same commit.** An outdated anchor is worse than no anchor.
