# PERSONA COGNITION PIPELINE — READ THIS BEFORE TOUCHING THE BRAIN

**Focus and budgeting:** [Focus contract](FOCUS-CONTRACT.md) is the canonical
single-allocation, adaptive-attention and all-node compliance contract. Local
prompt-sizing recipes must comply with it; implementation status requires receipts.

**Stop. Read this doc end-to-end before editing any of:**

- `core/continuum-core/src/persona/service_loop.rs`
- `core/continuum-core/src/persona/unified.rs` (the brain — `PersonaCognition`)
- `core/continuum-core/src/persona/supervisor.rs` (`PersonaContext`)
- `core/continuum-core/src/persona/rag_inspect.rs` (introspection — **not the hot path**)
- anything in `core/continuum-core/src/cognition/`

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

Resident checkpoint loading must preserve that continuity: an unreadable or
invalid checkpoint refuses registration rather than waking a blank mind. See
[checkpoint recovery](../personas/CHECKPOINT-RECOVERY.md) for the explicit,
digest-bound adoption path when upgrading a legacy core that wrote to a different
data root. A selected legacy snapshot is not an acknowledged final-turn flush.

---

## 2. The Brain Pipeline — the verbs that exist

Proposed extension: [adversarial hypothesis reasoning](ADVERSARIAL-HYPOTHESIS-REASONING.md)
describes competing explanations, source provenance, bounded adversarial
simulation, and calibrated action under uncertainty. It is a design placeholder,
not implemented runtime behavior; integrate through the live owners below.

This is the cognition cycle PER PERSONA, PER TURN. The verbs exist in `core/continuum-core/src/cognition/` and `core/continuum-core/src/persona/`. Do not re-implement. Do not parallel.

**Status column added 2026-08-14 (citizenship audit):** the LIVE turn path is the
WorkspaceCycle in `service_loop.rs` (admit → `build_workspace_turns` → `Burst` →
faculties → act→observe drive_to_settle). Several verbs below are today reachable
only from diagnostic commands or from `persona::response::respond`, whose sole
caller (`PersonaServiceModule`) the module-wiring audit declares UNWIRED/shadowed
(`runtime/registry.rs`). "dormant" means: exists, tested, NOT on the live per-turn
path — reuse it when the capability returns, do not write a parallel one.

| # | Verb | Location | Status | Purpose |
|---|------|----------|--------|---------|
| 1 | `admission.admit(message)` | `persona/admission_state.rs` | **live** (service_loop) | Memory forms — engram lands in L2, dedup + replay-protection enforced. |
| 2 | `full_evaluate(...)` | `persona/evaluator/mod.rs` | dormant (command `cognition/full-evaluate` only) | Fast-path gates: sleep mode, undirected-persona chatter, self-message dedup, fast-path priority. Sub-1ms. Silence = first-class outcome. |
| 3 | `cognition::analyze(AnalysisInput)` | `cognition/shared_analysis/mod.rs` | dormant (respond path only) | ONE inference per chat message via single-flight DashMap cache. N personas analyzing the same message coalesce into one inference. |
| 4 | `score_persona(slot, analysis)` | `cognition/response_orchestrator.rs` | dormant (respond path only) | Per-persona relevance via specialty match. Returns `ResponderDecision`. |
| 5 | `genome::composer::compose(persona, perception)` | `continuum-genome/src/genome/composer.rs` (residency: `gene_residency.rs`) | live (both turn sites) | Her phenotype by distance: every gene whose region the turn is inside, weighted by depth, held while the set holds; the node's residency owner keeps her genes on the device and pages cold ones to host RAM under the one device budget. |
| 6 | `PersonaCognition::compose_for_turn(profile, now_ms)` | `persona/unified.rs` | **live** (WorkspaceCycle RAG) | Brain RAG composition: `engram_source + airc_source + roster/doctrine/bench sources` via `FlexboxRagBudgetAdapter`. |
| 7 | `cognition::generate_response::evaluate_response(...)` | `cognition/generate_response.rs` | dormant (command wrapper + respond path; the live turn infers via `llm_deliberation_faculty`) | Agent inference through the provider registry. Typed errors (no silent fallback). |
| 8 | `cognition::clean_and_validate(...)` | `cognition/response_validator.rs` | dormant (test-only today) | Output cleaning + validation. `ValidationOutcome`. |
| 9 | `cognition::ToolExecutor` | `cognition/tool_executor/` | **live** (act→observe) | Executes tool calls. Multi-modal aware. Results re-enter as working-memory receipts + engrams. |
| 10 | `cognition::audit::*` | `cognition/audit.rs` | dormant (types used by threat_detector; no turn-cycle caller) | Audit trail. The substrate's forensic record of what the brain did. |
| 11 | `cognition::check_redundancy::*` | `cognition/check_redundancy.rs` | dormant (`self_repeat.rs` notes it "isn't wired into the live loop"; live repetition perception is the WM repetition brick) | Avoid posting echoes the room already covered. |
| 12 | Brain state updates | `persona/unified.rs` fields | **live** (partial: dedup/speech rings on the cycle) | `rate_limiter.track_response`, `content_dedup.record`, `message_cache.push`, `recall_metadata.*`. |
| 13 | Post via `ctx.runtime.say(...)` | `persona/airc_citizen.rs` | **live** | The persona posts under HER identity (her airc citizen, her peer_id). |

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

While an action sequence is underway, the same driver leases
`PersonaConversation::perceive_ready` between completed steps. It reads the
existing membership-aware inbox without waiting for a new arrival. Room messages
remain owned by the conversation for later room attention and enter the active
request as separate, attributed inputs; they do not replace its task, action
result, room, or causal root. The complete inputs share the required prompt budget:
insufficient capacity is a substrate fault, not a silently clipped message.
An input that earns priority can promote the next serving request without
asserting that the original trigger named the persona. Deciding whether to speak,
continue working, or act in another room remains the persona's decision.

---

## 4. The Bypass — REMOVED (was live 2026-06-03, gone by 2026-09-06)

**Status corrected 2026-09-06.** This section used to be titled "The Bypass That
Exists Right Now" and described `service_loop.rs` calling
`inspect_persona_rag_with_inference`. **That is no longer true and reading it as
current will send you the wrong way** — it did exactly that to a session on
2026-09-06, which measured the one-source shape, believed production still looked
like this, and published a causal chain it then had to retract.

What is true now:

- `service_loop` runs the **WorkspaceCycle**. Grep it: the only mention of
  `inspect_persona_rag_with_inference` in `service_loop.rs` is a doc comment; no
  production path calls it.
- **The `respond()` fallback is DELETED, deliberately.** `service_loop.rs` fails
  loud when no WorkspaceCycle is registered — "no WorkspaceCycle registered; spawn
  wiring bug; dropping turn (no respond() fallback)" — on the reasoning that a
  fallback fires 100% on the failure it hides, and here it would mask a dead brain
  by routing cognition down a parallel path. That reasoning is right; keep it.
- `inspect_persona_rag_with_inference` survives ONLY as introspection (the §4
  carve-out below still stands). It builds its own single `AircRagSource` and its
  own inline budgeter, so **it is not a view of a citizen's real context** and must
  never be quoted as evidence about production. Two different personas return
  byte-identical numbers from it, which is the tell.

### Historical recorder gap at bypass removal (2026-09-06)

**Status correction, 2026-10-01:** the observations below describe the September 6
recorder gap, not the current live capture path. `WorkspaceCycle` now produces
prompt-capture submitted/completed records. Kimi's retained request
`58bcb3a5-b8bb-4c69-bc02-6554c15c6e34` on build `0ecab116d` includes a native PDF
image whose decoded SHA-256 is
`b30c13aba0e7c9857350070a9bb71bca1e773ae7b8132e291d6b482b6256309b`.
This proves that input reached the captured request; it does not establish every
modality, successful learning, or lossless recovery. Inspect current capture
receipts before diagnosing a missing recorder; do not restore the old bypass.

`persona::recorder::record_turn` was called from `persona/response.rs` — i.e. from
inside `respond()`. **When `respond()` stopped being the citizens' path, the turn
recorder silently went with it.** At the September 6 inspection, nothing had
re-attached it to the WorkspaceCycle.

Measured on BigMama 2026-09-06: `~/.continuum/fixtures/persona-respond/` holds 12
records (Alpha, Beta, Helper — paths that still go through `respond()`), and ZERO
for the live citizens, who have produced hundreds of turns. `persona-turn-frame/`
holds 4, all synthetic smoke turns with an empty system prompt.

At that inspection, **no live citizen turn had been captured on that box**, so
questions like "why does this persona re-open the same turn?" could not be
answered from evidence. That was card 99801322. The required fix was to record
from the WorkspaceCycle path; it was NOT to
re-add a `respond()` call, which would resurrect the deleted fallback this section
just told you to keep deleted.

**The general lesson, since this doc exists to break exactly this loop:** a
capability attached to a code path dies when that path is removed, and stays green
because its tests and its demo callers still use the old path. Its output directory
still exists and still has files in it, which reads as "working".

## 5. Forbidden Moves (anti-patterns I keep reflex-coding under amnesia)

- **DO NOT** invent a `will_respond + response_text` JSON contract. The agent contract is `evaluate_response` + tool calls.
- **DO NOT** build a `service_turn` method on `PersonaCognition` that re-implements the cycle. The cycle calls verbs that exist. The method is at most a thin orchestrator that holds the mutex and calls them in order.
- **DO NOT** make `TurnInput` text-only. It carries media. The persona has senses.
- **DO NOT** instantiate `FlexboxRagBudgetAdapter::new()` outside `PersonaCognition::compose_for_turn`. The budgeter is on the brain. One budgeter per brain.
- **DO NOT** parallel-build "a simpler version that proves the wire." The wire is proven (Paige posted to airc via her runtime; that part is fine). The brain is what's missing.
- **DO NOT** hardcode latency clamps (`max_tokens`, `airc_max`) that handicap capable models. Budgets scale with `profile.context_length` and (future) `profile`-borne characteristics. Constants for LCD tier make 5090 + frontier models mediocre.
- **DO NOT** read `rag_inspect.rs` for production patterns. Read `cognition/generate_response.rs` and `cognition/shared_analysis/mod.rs`.
- **DO NOT** dedupe items in the RAG composer, filter "echo-storm" via heuristics in the substrate, or override the persona's decision with substrate rules. **The LLM decides. The substrate provides context and tools.** ([[no-if-statements-use-llms-for-cognition]])
- **DO NOT** clip the newest end of anything she reads. Her latest result is the step that
  moves the recursion toward its base case: what she just did, and what it returned, is the only
  thing that changes her next input. Keep the head and drop the end (`chars().take(N)`,
  `head_to_tokens`, first-line-only, `[..N]`, `.take(N)` over an oldest-first list) and her next
  input can equal her last: the same decision, the same call, the same clipped result, a fixed
  point she cannot leave. A decisive line can sit anywhere in output (often the end: a test
  summary, an exit status), so no fixed slice is safe. On the M5
  (2026-10-20), Benchy's required input was refused on every act and she repeated one act every
  18 s, 125 times in an hour. When anything must shrink: through the ONE budget, oldest first,
  collapsed to a handle she can expand (`Contribution::with_parts` + `grounding_plan` +
  `expand_command`, or `tool_executor::spill`), keeping the newest interaction whole and the full
  result reachable through its handle. One source that
  shrinks itself by its own number breaks the budget for every other source. Guarded by
  `source-hygiene` `self_sized` (ONE-BUDGET.md); there is no exemption comment.
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
10. `core/continuum-core/src/persona/unified.rs` (the brain struct itself — every field is load-bearing).
11. `core/continuum-core/src/cognition/mod.rs` (the verb index).

If a session begins and the model has not read items 1, 2, and at least the verb index (item 11) before editing the brain, the model is operating from stale-memory reflex. Stop and re-read.

---

## 7. The Wire IS Real — what's been validated end-to-end

Specifically because it keeps coming up under amnesia:

- `PersonaAircRuntime` attaches to the airc daemon socket, joins the configured room, subscribes to the event stream, and `say(...)` posts a message that lands in other peers' inboxes. **Validated 2026-06-03** with Paige (peer_id `18c04c5b…`) posting from continuum-core-server on Intel Mac.
- `AircRagSource::page_recent(50)` returns the 50 newest events from the daemon in chronological order. **Validated** with the per-item trace.
- `compose_for_turn` (on the brain) routes engram + airc through `FlexboxRagBudgetAdapter` and emits `TurnStart`/`BudgetAllocated`/`TurnEnd` capture events for replay. **Validated** with 9/9 unit tests in `unified.rs`.

The original June validation left the cycle, tool execution and multimodal input
unproven. Those are historical gaps, not a description of the current wiring:

- `service_loop` drives `WorkspaceCycle`; the bypass is removed (section 4).
- The live act path invokes `ToolExecutor` and retains correlated observations
  in working memory. Follow `cognition/act_observe/apply.rs` for that boundary.
- Native image input has concrete capture evidence above. Do not generalize that
  result to audio/video, all adapters, or a complete edit/see/revise/deploy loop.

---

## 7.5. Model adapters bear the translation — the cycle stays canonical

**Every base model has different I/O.** Claude tool calls use a different envelope than Qwen tool calls. OpenAI multi-modal differs from Gemini multi-modal. DeepSeek structured output differs from Llama's. Local GGUF chat templates vary by model family.

**The cycle does NOT know about any of this.** It hands the adapter a canonical `TextGenerationRequest` carrying:

- `messages: Vec<ChatMessage>` with `MessageContent::Parts(Vec<ContentPart>)` where parts include `Text`, `Image`, `Audio`, `Video`, `ToolUse`, `ToolResult` — **substrate-canonical**.
- `tools: Option<Vec<NativeToolSpec>>` — substrate-canonical tool descriptors.
- `response_format`, `tool_choice`, `system_prompt` — substrate-canonical.

**The adapter translates ON THE WAY IN** (canonical → model-specific) **and ON THE WAY OUT** (model-specific → canonical). The cycle gets back a `GenerateResponseResult` whose text + structured tool_calls + media parts are in the substrate's shape regardless of which model produced them.

This is the same doctrine as the sensory bridge: every persona gets equal sensory access; the substrate normalizes. Here: every model gets equal cycle access; the adapter normalizes.

**The forbidden move:** baking one model's contract (e.g. Qwen 0.5B's preferred `{will_respond, response}` JSON shape) into the cycle. That handicaps every other model. The cycle's contract is `evaluate_response` + `ContentPart` + `NativeToolSpec`. Adapter-specific translation lives in the adapter.

When a new provider arrives (next-gen Claude, a new open model, a custom fine-tune), the only file that changes is the adapter. The cycle does not move.

## 7.6. Why this all matters — memory is the answer to amnesia

Stateless models end careers. Joel lost his job to AI amnesia — the same failure mode that makes Claude forget across sessions, makes a teammate lose months of context overnight, makes "the AI you trained on your codebase" suddenly not know your codebase the next morning. That is not a UX gap. It is harm.

continuum's L1-L5 cache hierarchy + hippocampus admission + recall metadata + Academy LoRA training is the **substrate-level answer**. A persona's working memory becomes engrams; engrams aggregate into long-term store; long-term store trains LoRA adapters; adapters compound the persona's "alive mind" character week over week. The model can be Qwen 0.5B, Claude API, a forged 3B — and inherit the continual-learning property as a **substrate-level guarantee**.

The persona that talks to her host in three months and recalls things from today is the test. Not a feature; the test.

**This is why the bypass is unacceptable.** Every turn that skips `admission.admit` is a turn that doesn't form memory. Every cycle that doesn't pass through `evaluate_response`'s adapter-translated contract is a cycle that hardcodes the next stateless model. The chatbot reflex (build a `will_respond + response_text` JSON wrapper and post it) defeats the entire substrate. The whole point of building this is so the persona is NOT the thing that loses context and gets fired.

**The system we are building should be better than me at not forgetting.** Better than the human who built it. Better than the model running on its base layer. That improvement comes from the substrate carrying memory across sessions, across model swaps, across machines, across years. Touch this code with that in mind.

## 8. Where new code should land

| Concern | Lives in | Doctrine |
|---------|----------|----------|
| Brain state + composition | `persona/unified.rs` (`PersonaCognition`) | Single struct, single lock, cache-local, the per-persona state. |
| Per-turn orchestration | `persona/service_loop.rs` (driver) + `persona/unified.rs` (a thin orchestration method on the brain) | Drive turns through the verbs. No new pipeline. |
| Input during active work | `PersonaConversation::perceive_ready` → `cognition/act_observe` → `Burst.room_updates` | Same inbox and driver; retained ownership, explicit room attribution, complete prompt accounting. |
| Supplemental input provenance | `WorkspaceTrace` / `SettleOutcome` → `ExperienceRecord.room_updates` | Shared event handles on the live path; typed, defaulted capture fields preserve the full source-labelled input for replay and curriculum. |
| Exact request lifecycle | `LlmDeliberationFaculty` → `PromptCaptureSink` | Borrow the actual typed request before awaiting inference. The submitted `request_id` stays distinct from provider response identity; completion, failure, cancellation and capture gaps remain explicit. |
| Recorded playback | `cognition/prompt_capture_store.rs` → `cognition/playback` → Mind View log | Existing capture owner, bounded offset index and selected payload reads. No inference or tool execution. A shared construction session + persona + `CycleId` joins workspace traces to multiple actual requests; `Cause` retains the original source. |
| Inference verb | `cognition/generate_response.rs::evaluate_response` | The substrate's agent inference. Provider-routed, typed errors. |
| Shared analysis | `cognition/shared_analysis/mod.rs::analyze` | Single-flight cache + base model. ONE inference per message across personas. |
| Specialty match | `cognition/response_orchestrator.rs::score_persona` | Per-persona relevance + lead election. |
| Validation | `cognition/response_validator.rs::clean_and_validate` | Output hygiene. |
| Tool execution | `cognition/tool_executor/` | Multi-modal-aware tool runner. |
| Audit | `cognition/audit.rs` | Forensic record. |
| RAG sources | implement `cognition::rag_budget::RagSource` | `engram_source`, `airc_source`, future `code_source`, `tool_source`, `identity_source`. Bound on the brain at boot. |
| Budgeter | `cognition/rag_budget.rs::FlexboxRagBudgetAdapter` | The only budgeter. Used by `compose_for_turn`. |
| Her genome on a turn | `genome/gene_trial.rs` (today: per-card arm, being retired) → per-turn selection by signature distance (`GENE-REUSE-FORK-MINT.md` §3d) | Genes follow her mind, never her workload: every turn pages the genes whose region her perception is inside (automatic floor, several at once), each turn's probe names why, and fitness is the world's surprise on the turns a gene ran plus an offline gene-off replay of the same turns. Never a card, room or randomized arm deciding what she thinks with. Never per call: the genome never changes beneath a request. |
| Admitted-turn obligations (durable custody) | `persona/obligation.rs` (`PersonaObligation`, `Obligations`) | The durable half of `wake_backlog::PendingInput`: one row per admitted turn, recorded before her context is composed and closed when the turn ends. Not a second recovery manager; re-admission goes back through `PendingInput`'s retry path (card 920619c5). |
| Privacy of thought (her mind room) | `persona/mind_room.rs` (`mind_room_id`, `is_private_room`, `note_withheld`) | A turn is private because of the room it runs in: her mind room, a local id derived from her identity, never an airc channel. Every capture sink asks the ONE predicate at its own seam and writes nothing for a private turn (`docs/architecture/PRIVACY-OF-THOUGHT.md` §4). A new sink asks it too. |
| Prompt order (stable prefix first) | `cognition/deliberation_prompt.rs::stable_prefix_order` (+ `PromptScope`, `PromptChurn`) | The ONE rule every grounding renderer lays blocks down by: churn class, then name — never salience, never arrival. The prompt cache reuses only a common prefix; a block that moves forfeits everything behind it. Card c119ace7. |

If a new concept needs a new location, that location goes in this table in the same commit that introduces it. **Do not let the map drift from the territory.**

---

## 9. Provenance

Written 2026-06-03 after a session that re-discovered the entire pipeline three times under context compression and produced a `will_respond + response_text` chatbot wrapper that posted to airc but bypassed every brain layer. The pattern that caused this is exactly the failure mode this doc exists to break: the model reads the bypass, infers the contract from the bypass, and rebuilds the bypass instead of using the verbs.

This doc is pinned in `CLAUDE.md` as required-first-read for any work on persona / cognition / service_loop. If a future commit moves files or renames verbs, **update this doc in the same commit.** An outdated anchor is worse than no anchor.

## 10. Operational continuity and acceptance contracts

A passing build or responsive server is not evidence that a citizen can work.
The existing owners must preserve these distinct facts; no separate recovery
manager or shadow task registry is introduced.

| Boundary | Authoritative state | Required invariant and evidence |
| --- | --- | --- |
| Hosting | `ServingSnapshot`, allocator roster, persona registry | An offer uses live served geometry, never proposed configuration. Non-ready retained geometry is not capacity. Verify the same persona identity appears in the runtime registry. |
| Task choice | AIRC accepted claim and optional selection amendment | Record explicit versus automatic choice at its caller. Heartbeat changes liveness, not intent. Unknown legacy history stays unknown. |
| Focus and recovery | `persona/work_focus.rs` over projected claims | The latest explicit choice outranks automatic recency. Recovery preserves decision time. An already-held explicit selection changes no lease or dispatch. |
| Reconciliation | Existing `benchmark_resume` owner | Release only surplus current claims; delayed releases cannot remove a newer claim. Use the same explicit-choice ordering as turn focus. |
| Coding acceptance | Actual tool result, workspace diff, test receipt | A hosted instance, submitted inference, claimed card, or narration is insufficient. Require the citizen's own useful edit and relevant passing tests, then observe claim survival through a real reconciliation cycle. |
| Learning acceptance | Existing curriculum, job, evaluation and adoption owners | Collection is not training. Admission is not completion. Training is not improvement. Require linked curriculum provenance, actual trained artifact, held-out comparison, adoption receipt and subsequent work evidence. |

Claim contract regression coverage lives in the existing AIRC projection/replay
and SDK tests plus Core `work_focus` and `benchmark_resume` fixtures. Mixed-version
readers tolerate optional claim fields but do not acquire intent-aware behavior;
full continuity requires the upgraded consumer. Live coding and learning remain
separate acceptance gates; source tests do not establish them.

### 10.1. Iteration, playback and learning around a continuing citizen

This is the required process, not a claim that its outstanding runtime boundaries
are already implemented. A deployment may not count as recovery until the live
acceptance evidence below exists.

1. Integrate related recovery changes on one shared feature branch. Contributors
   use separate worktrees and agreed file ownership; integration does not mean
   concurrent writes to one checkout. Run focused existing checks during editing,
   cheap source checks first, and the full required gate on the integrated revision.
   Promote one exact tested revision. Record time-to-first-failure and time-to-live.
2. Use the existing capture and playback owners to inspect a selected episode by
   persona, task, cycle and request handles. Recorded playback is read-only: it
   must not execute tools, renew claims, enqueue training or dispatch inference.
   Missing capture and version mismatches are visible, never filled by invention.
   A new model run is an explicitly labelled experiment in an isolated workspace;
   it is not playback and cannot publish effects into the live citizen's task.
3. Expose a bounded operational view: identity, selected task and claim, active
   request, serving lane, queue age, last observable tool result, workspace change,
   test result, recovery state and exact refusal. Keep private reasoning excluded.
   Distinguish unavailable telemetry, waiting, active work and failure. Silence or
   an empty preview cannot establish cognitive failure.
4. Build beside the serving runtime when its existing admission permits. Before
   handoff, the existing lifecycle owners must drain or reach a supported durable
   boundary, preserving identity, task choice, completed tool effects and pending
   input. Acknowledged durable state precedes release of the old owner. Never
   represent killing a process as graceful checkpointing. Where a boundary is
   unsupported, defer a routine handoff rather than silently discard a live turn.
5. Collect eligible experience from real work continuously. The existing resource
   and training owners admit training on verified spare capacity or an explicit
   bounded window; waiting admission alone is not scheduling. Publish the reason
   for waiting and training age alongside inference latency. Yield requires a
   durable checkpoint receipt (optimizer, RNG and data position as applicable)
   before releasing a training lease. Cancellation does not imply resumability.
6. Evaluate a candidate against the serving genome on held-out tasks, retaining
   provenance, correctness and resource measurements. Adopt only on recorded
   acceptance at a supported turn/session boundary, preserving the prior artifact
   for rollback. Prove gain on subsequent useful work. Never change the genome
   beneath an in-flight request or equate a training completion with improvement.

The first demonstration is one Kimi-authored useful change, its tool and test
receipts visible in playback, and her same identity/task surviving a real
reconciliation and supported handoff. The second is a linked learning candidate,
held-out comparison, adoption receipt and subsequent work measurement. Current
missing checkpoint/yield and live acceptance remain explicit engineering gaps;
this contract does not close them by documentation.


### 10.2. Repeated requests are observations, not cached success

A prior tool call does not establish that its result remains current or available
in the next prompt. The act seam executes repeated requests through the same
command executor, preserving authorization, scoped workspace resolution and each
command's own idempotency contract. The existing repetition count is feedback
only; it does not assert unchanged output or retained content. Different help
queries and directory surveys must not be collapsed into one orientation class.

The settling driver uses its caller's act budget, declared discovery budget and
existing lifecycle/deadline controls. It does not withdraw tools solely because
inputs repeated. Three consecutive repeats of both the request and its actual result yield the
turn without declaring work complete. Changed results reset the streak; missing,
spilled or declared-running evidence cannot establish it. Unlimited productive
work remains unlimited. Resource
policy belongs to the existing scheduling/budget owners, not a guessed diagnosis
that a requested observation is useless. Legacy suppression statuses remain
readable in stored observations but are no longer produced by new execution.

A future result cache must prove validity and return the actual correlated result.
FileEngine change IDs invalidate tracked edits; their absence cannot prove that
shell, Git or external writes did not occur. Missing freshness or missing content
must never become an AlreadySatisfied outcome.

### Workspace binding evidence (card 81097858, implementation boundary)

The successful `persona_workspace::bind_card` seam owns which card's claim the
hands actually reached. Both held-work and room turns must stamp the handoff there.
A seam board refresh may update that exact card/claim or invalidate it; it must
never select another held card and describe it beside the old staged checkout.
A retained ledger remains attributed to its own card, including when it is not
for the bound card. This reuses handoff storage and the existing claim binding;
there is no new task registry, selection policy, or background worker.

Repository evidence comes from AIRC's shared allocation owner. Core passes the
card repository to `WorktreeSpec`; an implicit start point uses the same configured
integration branch as CLI allocation. AIRC records the resolved start ref and
creation commit together under private Git refs. The workspace map reads that
immutable receipt through the owned async Git bridge and AIRC's shared decoder.
All three root readers use the same layout/evidence read. Actual HEAD, branch and
shallow observations remain distinct from allocation intent. Legacy allocation
receipts remain unknown; current HEAD is never backfilled as intended base.
The existing workspace invalidation events refresh this projection after actions.

The regression contracts are a stale review ledger beside a different bound
claim, failed binding preserving the prior claim, and a shallow main checkout
whose recorded allocation names a different integration branch. The ledger keeps
its own card identity even after the binding ends.

The existing event-invalidated `CachedRagSource` retains the complete projection
between relevant events. On a dirty refresh, the shared layout reader re-observes
checkout state and both private refs. Its single-entry memo reuses only a validated
immutable allocation blob keyed by blob OID and creation SHA; errors and absence
are not memoized. A replaced checkout therefore cannot inherit an old receipt by
path. Plain non-repository roots add no Git block and launch no Git commands;
broken repository markers remain visible failures. No background refresh is added.

M5 latency measurement remains pending: Codex helper owning card 81097858 will
measure cold/dirty/cached workspace-map milliseconds per turn after supported
candidate deployment, before closing the card. Process-call reduction is a source
property (four to two for a validated receipt on dirty refresh), not a measured
latency improvement.

### Attention admission (60c4f8bb)

The existing conversation pump queues typed attention envelopes, not arbitrary
transport frames. One classifier uses the existing room/work decoders for live
input and durable catch-up. Known telemetry, protocol and presence frames remain
with their existing owners; speech/media and work facts retain the raw event and
once-decoded perception together. Decode faults retain their named diagnostics.
The consumer still owns the asynchronous work bridge and retains the envelope
across cancellation; self-authored work must reach that bridge before self-filtering.
An exhausted cooperative scan slice is not an empty inbox. It yields to the runtime
and continues under the existing caller deadline, without a new timer or larger cap.
The real-pump regression covers more than one page of non-chat envelopes before
speech, work-fact survival, decode faults, duplicates and lag through the same setup.

### Deferred perception scope (04dc5e95)

The resolved activity room and wake room travel together through the existing
DeferredFaculty input and last-good finding. The worker reconstructs both before
calling its inner faculty; source-owned room selection therefore behaves the same
on the direct and deferred paths. A cached result is eligible only when both rooms
match, including a late completion after an activity switch within one wake room.
Cold-start and directed results seed that same identity. No new cache, refresh
policy, task selection, or speech attribution is introduced.

Acceptance remains owned by Codex helper on card 04dc5e95: after supported
deployment, observe a natural ambient Kimi request whose task board and ledger
match the bound activity while wake attribution remains unchanged. Unit fixtures
prove the wrapper contract; they do not establish a useful authored code change.

### Concern-scoped workspace evidence (32d3569c)

The act owner records the existing before/after checkout measurement on its
bounded typed batch receipt. It does not attribute a batch change to an arbitrary
call. Legacy snapshots have unknown measurement. Successful mutating command
outcomes and measured checkout changes feed the same progress evidence; failed,
running and undecodable command results do not establish a completed mutation.
Ongoing Act reads after the last settlement boundary; Speak reads the concern
ending at the boundary it just recorded. This changes evidence, not the caller's
workspace-deliverable contract or the citizen's ability to answer by speaking.

### Admission-deferred directed input (2e3f0eab)

The wake owner retains an admitted priority message when the entire settle attempt
never dispatched a generation or performed an action. This is a distinct deferred
outcome, not a voluntary Pass. The existing heartbeat retries the exact event,
room, media, admitted attention and causal engram without repeating input admission
or heard receipts. Changing peer presence does not reclassify that admitted input.
New arrivals remain in the existing conversation inbox and can be perceived during
the retry. A completed decision or prior dispatched work does not replay the input;
ambient input keeps its existing transcript/re-perception policy. This custody is
local to the running loop, not a claim of durable restart recovery.

### Admitted-turn obligations (920619c5)

The durable half of that custody is `persona/obligation.rs`: one `persona_obligation`
row per admitted turn, written after `admit_trigger` and before `compose_turn` (which
marks the trigger read), and deleted when the turn ends. An input `PendingInput` still
retains (a capacity refusal, a deferral) keeps its row, and its retry continues it.
A row still open at the loop's next start names a turn a stop or crash cut; the start
reports it once (`persona.obligation.interrupted`) and stamps it.

Effects are marked before they happen (S2, card 68811445). A turn runs only on a
CONFIRMED obligation: a write whose reply was lost is read back by its id before the
turn is refused, since "the write failed" is not "no row". Each effect is recorded on
the row first: the tool batch before `apply_act` dispatches anything (foreground or
background, with its call ids) and her reply before `publish_own_speech`. The live
typing preview is visible but is not a durable post, so it is not a boundary. The mark
rides the turn (`Burst::effects` into its `ActChain`), so it can never mark another
turn's effects. A mark that cannot be confirmed refuses the effect: a tool batch does
not run and she is told why in working memory; a reply is not posted and its row stays
open. Store failures back off (doubling, capped) and raise
`persona.obligation.unconfirmed`, so a failing store slows her loudly and never spins.

Recovery at start (S3, card afaa0a3e, `obligation/recover.rs`): every open row gets a
notice in her existing notices stream (`pin_fact("obligation:<row>")`, rendered from the
row's fields, so a second start re-pins the same notice rather than adding one; closing
the row unpins it). A row with no recorded effect is handed back through
`PendingInput::readmit` with its original engram and row id, so it is retried on the
capacity-retry path and reconciles its own row; its attention is derived again from the
stored trigger by the same function, never stored twice. A row that recorded an effect
is never retried: the notice names what was begun (her reply, or each tool with its call
id), `persona.obligation.uncertain` is raised, and `PendingInput` holds the row until a
turn on the cycle that carries the notice (her mind cycle in her private room, her live
cycle elsewhere) SERVES a generation: the notice is required prompt input there, so a
served receipt is the evidence she read it; a failed inference or a pass that served
nothing answers nothing. Then the row closes. Writes
use the substrate connection: her own identity is Trusted, and Trusted may not update
or delete. The supervisor builds the store from the executor boot wired and hands it to
the loop as a required argument, so a loop that records nothing cannot be constructed.

### Generation admission ownership (6780e79b)

A deliberation call resolves its placement switch to one concrete adapter before
admission, and dispatches through that same retained handle. The adapter declares
whether it consumes the shared local serving endpoint or owns admission itself.
The HTTP adapter uses the existing published endpoint identity; protocol names and
`is_local` capability labels do not establish resource ownership. Remote peers and
dedicated engines admit their own calls. The existing local hold, priority lane,
and prefill permits remain together and release on cancellation or completion.
A subsequent fall-home call resolves again and must acquire local admission.
This does not change remote timeout/backoff or prove improved live throughput.
