# Continuum Resource Architecture

> **One coherent model for every shared resource in the system.** Time-share and prioritize what can be shared; cleanly tear down what can't. Let the system's own organic activity drive priority. Compress before evicting; demote before dropping. Degrade gracefully under pressure; never silence individuals. The same primitive serves KV cache, LoRA adapters, MoE experts, model weights, embeddings, recalled memories — and exposes the levers for intelligent (eventually ML/LLM-driven) control.

Status: design — 2026-04-18. The architectural picture behind the work shipping tonight (PR #930 paging primitive, #921 inference queue, #927 cold-start prewarming) and the work yet to come (PressureBroker, KV per-recipe budgeting, LoRA pool migration, MoE forge, MLX-format forge). Authored after a session where the persona path stalled because hand-rolled paging implementations across LoRA registry, KV reservation, embedding cache, and memory recall had drifted apart — none aware of the others — and the cumulative pressure crashed the user experience.

Implementation foundation: [`UNIFIED-PAGING.md`](UNIFIED-PAGING.md) — the Rust `PagedResourcePool<K, V>` primitive that this architecture builds on.

---

## Why this is kernel-level, and why it's different from a normal OS

A traditional OS pages **generic memory** — opaque 4 KB pages, no semantic understanding of what lives in them. The kernel decides what to swap to disk based on access patterns, but it can't compress an embedding to INT8 or quantize a model weight or summarize an old memory into gist, because it doesn't know what any page *is*.

Continuum's resource architecture is the **kernel layer for AI workloads** — different from a normal OS in exactly the way that matters: it pages *AI-native entities* (LoRA adapters, KV cache pages, MoE experts, model weights, embeddings, recalled memories) with full semantic awareness of what each entity is and how it can be compressed, demoted, or reconstructed. The system can:

- Quantize an embedding from FP32 → INT8 because it knows what an embedding is
- Compress old KV cache to lower precision because it knows attention is the consumer
- Summarize an old memory into semantic gist because it knows the memory's role in cognition
- Share an LoRA adapter's read-only weights across N personas because it knows weights are immutable
- Page out a MoE expert because it knows the router will request it back if needed

This semantic awareness is what makes "massive work on tiny systems" possible — the OS can't do it generically because it lacks the AI-native types; we can do it precisely because the same Rust trait + generic primitive serves every entity *with* its semantic adapter.

The OO + adapter pattern is what makes this clean. The primitive — `PagedResourcePool<K, V>` — is the **reusable kernel concept** (the equivalent of Mach VM's `vm_object`). Concrete consumers — `KVCachePool`, `LoRAAdapterPool`, `MoEExpertPool`, `EmbeddingPool`, `MemoryRecallPool` — are **adapters**, each composing the primitive with the right key shape, value shape, sizer, eviction policy, and (eventually) tier-demotion strategy for that entity type. Same kernel concept, semantic specialization at the adapter layer.

Rust makes this brilliant because generics + traits give zero-cost abstraction over the variation. No vtable overhead per page lookup. No allocator surprises. The semantic adapter is compiled away into specialized code paths per entity type — same expressiveness as Java's interfaces or C++'s templates with none of the runtime cost.

This is what an AI-native OS looks like at the kernel level. It's not a userspace optimization; it's the substrate everything else runs on.

### The commercial picture: small systems running big models, many at a time

The strategic frame is short: **work smarter, not harder.**

Mainstream AI economics scale by throwing more hardware at the problem — bigger GPUs, more nodes, more datacenters, more electricity, more $$$. Every persona, every conversation, every task buys more compute.

Continuum's economics go the other way: **the same machine you already own, used efficiently, runs intelligence that the mainstream stack would charge a datacenter for.**

Concretely:

| Mainstream approach | Continuum approach |
|---|---|
| Deploy a 70B model? Buy an H100 cluster | Forge to MoE, page experts on demand, run on a MacBook Air |
| Run 14 personas? Spin up 14 model instances on 14 GPUs | One model loaded once, 14 personas time-share via priority scheduler |
| Long context window? Rent more VRAM | Tier KV cache (full → quantized → spilled), pay only for what's actually held |
| Per-skill specialization? Fine-tune 14 separate models | One base, N LoRA adapters, ref-counted shared across personas using the same skill |
| 100k embedding vectors per user? Hosted vector DB | Content-addressed pool with smart eviction on the user's own machine |

Every mechanism in this architecture — paging, time-share, tiered demotion, organic prioritization, dormancy ladder, recipe-declared resource holds — is a way to **buy more intelligence with the same hardware budget**. Compounding: each layer's efficiency gain multiplies the others. The end-state is enterprise-tier AI experience on consumer-tier hardware, with $0 per turn cost to the user, no API contracts, no datacenter dependency.

The technical work IS the commercial moat. The mainstream can't follow without abandoning the GPU-rental business model that funds them. Continuum can because the architecture is built on smarter resource use from kernel-level up — the way operating systems were always supposed to be designed for the resource they're managing, just specialized for AI instead of CPU/RAM.

---

## The thesis

Every shared resource in Continuum falls into one of two categories. The architecture treats them differently — and the categorization itself is most of the design clarity.

| Category | Examples | Mechanism | Lifetime |
|---|---|---|---|
| **Time-shareable** | Compute slots, KV cache pages, LoRA weights, model weights, embeddings, recalled memories, MoE experts | **Priority-scheduled paging pool** | Resource persists across consumers; one copy serves N |
| **Exclusive stateful** | Microphone capture, LiveKit peer connection, Bevy avatar render entity, audio output mix, WebRTC track | **Explicit lifecycle ownership** (RAII) | Resource bound to one session; torn down when session ends, reconstructed fresh next time |

Trying to force exclusive resources through paging would leak forever (pinning a mic capture is just exclusivity with extra steps). Trying to force shareable resources through teardown would waste reloading model weights every chat turn. The two categories need their own mechanisms; the architecture supports both as first-class.

This doc focuses on the time-shareable category — that's where the unified paging primitive lives and where the bulk of "make personas usable on small machines" wins compound. The exclusive stateful category is well-served by existing patterns (`LiveCallTracker`-style start/stop, RAII handles, Drop semantics).

---

## The four mechanisms (time-shareable resources)

Each mechanism is more sophisticated than the last. Most resources need only the first two; tier-demotion and ML-control are higher-order.

### 1. Reference-counted pin / organic fade

The base mechanism: while at least one consumer holds a `PinHandle`, the resource stays. When the last handle drops, the resource becomes a candidate for natural fade — no explicit evict() call needed.

This is the **CBAR-inspired organic prioritization**: don't ask "what to evict," ask "what's still organically held." The system's current activity (active recipe, focused persona, in-flight request) creates the gradient. Resources gravitate toward active concerns and naturally fall away from inactive ones.

Mapping in current code:
- Recipe starts → pins identity + tool defs + KV prefix + voice LoRA
- Persona engages on a turn → pins active genome adapter
- Conversation continues → KV prefix stays pinned (live slot)
- Recipe ends / persona goes dormant → handles drop, ref-count → 0

The pool's eviction policy becomes a *fallback* for when even the actively-pinned set exceeds capacity, not the primary mechanism. RAII semantics + biological-attention semantics + CBAR semantics — same architectural shape.

### 2. Time-share with priority

Pin is exclusivity ("nobody else uses this until I'm done"). Time-share is fluid ("we all share, scheduled by priority").

For KV pages, model weights, LoRA adapters, MoE experts: multiple consumers want them, often the SAME ones (14 personas with the same tool-defs prefix, 4 personas using the same `typescript-expertise` LoRA, all of them pulling from the same model weights). Pinning each exclusively wastes memory and serializes access. Time-sharing lets one copy serve everyone, with a scheduler arbitrating turn order.

`InferenceCoordinator.requestSlot` (PR #921) is the seed: doesn't deny, queues by arrival. Adding priority weight is the next step:

```
Active speaker in video call    → priority 100
Recently-spoke persona          → priority 50
Listening only                  → priority 10
```

When a slot frees, queue serves highest-priority first. Everyone shares the same pool; scheduler decides order. CPU L1 cache works this way; vllm-metal's PagedAttention works this way.

`pin()` becomes the special case (priority = ∞), used only for resources that genuinely can't be reconstructed mid-flight.

### 3. Tiered demotion (compress, don't drop)

Eviction is binary: keep or drop. The richer pattern is **demote to a smaller representation under pressure**. Same content, less memory.

Examples:
- **KV cache compression**: recent KV at FP16, older at INT8, oldest at INT4. Hit on quantized: dequantize on the fly OR re-promote to full precision when room allows.
- **Embedding precision**: recent as `Float32Array`, older quantized to `Int8Array`. 4× memory savings, slight semantic precision loss.
- **Memory recall (already!)**: TieredMemoryCache L1/L2/L3 IS this pattern. Recent thoughts at full fidelity, older summarized to gist. Biologically right — that's how human memory consolidates.
- **Model weights**: full precision while actively generating, quantized when idle but next-likely-used.

Architecturally this is **composition over multiple pools**, not a feature of the primitive:

```
EmbeddingL1Pool<Hash, Float32Array>   max=128MB  (recent)
EmbeddingL2Pool<Hash, Int8Array>      max=32MB   (demoted: quantized on demotion)
EmbeddingL3Pool<Hash, FilePath>       max=10GB   (spilled to disk)

L1 eviction: don't drop, demote to L2 (call quantizer, insert there)
L2 eviction: don't drop, demote to L3 (write to disk, insert there)
L3 eviction: drop (or write to slower archive)
On hit at L2/L3: optionally promote upward
```

The primitive stays simple; tiering composes. CPU L1/L2/L3 cache works this way. OS page-out-to-disk works this way (kernel handles the actual paging). For our L3 we can `mmap` files so the kernel becomes the L4 manager automatically.

#### The full hierarchy: cold storage → inference

The same composition extends down to **cold storage** and up to **active inference VRAM**, giving every paged AI entity the full hierarchy:

```
L1  VRAM full precision      — active inference (hot path)
L2  RAM quantized            — recently evicted from L1; rehydrate on hit
L3  mmap'd disk              — kernel-managed paging (our L4 effectively free)
L4  Cold storage / archive   — local SSD warm cache; pulled from HF/grid on miss
                                (or remote storage / network archive)
```

For an MoE expert router asking "give me expert #42":

1. **L1 hit** — VRAM full precision, used directly
2. **L1 miss → L2 hit** — quantized RAM copy promoted to L1 (or used quantized for less-critical paths)
3. **L2 miss → L3 hit** — mmap'd file paged in by kernel, loaded to L2/L1 by us
4. **L3 miss → L4 hit** — fetched from cold archive (network/grid), cascades up through L3 → L2 → L1
5. **L4 miss** — fetched from HuggingFace / model registry, populates all tiers

Each transition has a cost (compress on demote, dequantize on promote, network round-trip on cold fetch). The PressureBroker decides which tier each entity lives in based on activity prediction + cost. ML-driven prediction here directly equals "less cold-tier traffic" which directly equals lower latency for the user.

Same hierarchy serves every paged AI entity:

| Entity | L1 (VRAM) | L2 (RAM quantized) | L3 (mmap disk) | L4 (cold) |
|---|---|---|---|---|
| MoE expert | Full precision weights | Quantized weights | GGUF/safetensors file | HF / grid registry |
| LoRA adapter | Active scale weights | Quantized | On-disk adapter | HF / grid |
| KV cache | FP16 active | INT8 demoted | INT4 spilled | (drop — too cold) |
| Embedding | Float32Array | Int8Array | File-backed | (drop or recompute) |
| Recalled memory | Full text in RAM | Compressed gist | Per-room corpus on disk | Long-term archive |
| Model weights | VRAM (active inference) | RAM (warm idle) | Disk model file | HF / model registry |

This is what "cold storage → inference" looks like in our system: a continuous gradient from "available somewhere on the internet" to "in the GPU registers right now," with the broker arbitrating placement based on real activity. No layer is special-cased. The same paging primitive serves the full chain because each tier is just another `PagedResourcePool` with its own loader (cold-fetch / mmap / dequantize / etc.) and its own demotion target. **Working smarter, not harder, all the way from the network down to the silicon.**

#### Compositional paging: handle more than you should fit

The hierarchy above treats each entity (an MoE expert, a LoRA adapter, a model layer) as the unit of paging. The next-deeper insight: **an entity is itself an assembly of typed sub-entities**, and if the system knows the assembly recipe, it can page parts in/out *during* an inference run — not just at load time.

An MoE expert isn't a monolithic blob; it's W₁, W₂, W₃ projection matrices + a layer norm + a router-projection vector. A LoRA adapter is the rank-decomposed factors (B × A) plus a scale. A transformer layer is attention + feed-forward + norms. All of these are *compositions* of typed parts.

Once we name the parts as their own pool entries, **inference can pull them just-in-time** from whatever tier currently holds them:

```
Expert-42 activates for this token
  → router declares need for [W₁, W₂, W₃] of expert 42
  → W₁ is L1 (VRAM full precision)        — used directly
  → W₂ is L2 (RAM quantized)              — dequantize, stream to GPU, use
  → W₃ is L3 (mmap'd disk)                — page in, stream to GPU, use
  → After this token, all three demoted back to wherever the broker chooses,
    based on predicted next use (broker may keep W₁/W₂ hot if router pattern
    suggests this expert will fire again soon)
```

This unlocks **handling more than physically fits**. A 70B-parameter MoE model whose total weights exceed VRAM becomes runnable on a 16 GB machine if the architecture tolerates streaming sub-entity loads per token. The trade-off is latency (cold-tier hits cost real time), but the freedom is enormous: the system can attempt workloads that "shouldn't fit," and the broker decides per-request whether the latency is acceptable for that activity (research recipe? yes, take the hit. Voice chat? skip the cold expert, use a hotter approximation).

Existing inference engines already do crude versions: llama.cpp's `--n-gpu-layers 32` (offload N layers), Hugging Face Accelerate's disk-offload, vllm's PagedAttention. **What we add: principled, uniform, broker-managed across every entity type, with the assembly recipe declared as part of the entity's metadata.** Same primitive serves it because each sub-entity is just another `PagedResourcePool` entry; the assembler is a thin orchestration layer that consults the recipe and pulls parts from wherever they live.

When this lands fully, the answer to "can my MacBook Air run a 70B MoE?" becomes "yes, slower than VRAM-resident, but yes — and the broker decides per-request whether the slowdown is acceptable for what you're doing." Hardware ceiling becomes negotiable rather than fixed. That's the working-smarter-not-harder pattern at its limit: the system literally exceeds its own apparent capacity because it knows how to stream the constituent parts.

**This is the same insight as sentinel-ai forge alloy expert pruning, but inverted.** The forge identifies which experts in a model are statically removable — ship a permanently smaller model. Compositional paging is the runtime mirror: instead of removing experts permanently, page them in and out *dynamically* based on actual demand. The two compose:

- Forge identifies experts that are *always cold* across observed workloads → prune them entirely (static reduction)
- Remaining experts get compositional paging at runtime (dynamic reduction)

Same architectural insight from opposite directions. The forge proves the fingerprint of what's actually used; compositional paging extends that proof from "permanently shippable smaller" to "dynamically runnable bigger than fits." The combination: ship as small as the forge proves is safe, then stream the rest on demand for the workloads that occasionally need them. Best-of-both: minimal disk footprint AND maximum model class reachable.

### 4. Intelligent (eventually ML/LLM-driven) priority

The pool exposes the levers; the brain plugs in via the PressureBroker (Phase 7).

Today the eviction priority callback supports heuristics: `lru_priority`, `size_weighted_lru`. The ladder of "smarter":

1. **Cost-aware** — entries declare `refetch_cost_ms`; eviction priority = `recency × cost`. A 4 GB model weight costs 6 s to reload; a 384-dim embedding costs 2 ms. Cheap-to-rebuild evicts before expensive.
2. **Recipe / activity-aware** — active recipe declares its needs; those resources are *implicitly pinned* during recipe lifetime even without explicit `pin()` calls. Domain knowledge over access patterns.
3. **Online-learning bandit** — TinyLFU / ARC / W-TinyLFU adaptive replacement. Track "I evicted X then needed it back within 60s" events; adjust per-pool priority weights to reduce regret.
4. **LLM-mediated broker** — when the system is in an unusual state (4 personas + research recipe + voice + indexing all under pressure), heuristics fail. Ask an LLM at *policy adjustment* tick (every minute or two): "given these stats + this activity pattern, what would you evict?" The LLM doesn't run per-eviction (too slow); it updates the cost weights / priority functions that the per-eviction heuristic uses. Apple's RTOS-with-ML approach generalized to LLM mediation.

The primitive's design point is *exposing levers* (pluggable eviction, rich stats, public pin/evict, per-pool budget, function-based sizer) so this intelligence layer can plug in cleanly when ready.

---

## The persona dormancy ladder

Pressure response should degrade gracefully — never silence individuals. The plurality principle: 6 personas at lower fidelity > 1 persona at full fidelity.

```
Active        — normal behavior, all senses, all RAG sources, full max_tokens
Less-active   — fewer turns, lighter RAG, lower max_tokens (DEFAULT pressure response)
Dormant       — KV prefix stays in slot, persona doesn't fire response loop (worst case while alive)
Evicted       — slot reclaimed, persona unloaded entirely (last resort, ~6s reload cost)
```

**Critical: dormant ≠ evicted.** Dormant persona keeps KV prefix resident, voice LoRA loaded, identity warm — wakes back up instantly when pressure clears. Evicted means full reload. The broker walks down this ladder as pressure climbs; it doesn't jump to evict.

Mapping in current modules:
- `PersonaState` (autonomous loop tracks energy/mood) — same metaphor, add `forcedState: 'less-active' | 'dormant'` from broker
- `PagedResourcePool.pin()` — broker pins active-recipe resources, unpins for dormant
- `InferenceCoordinator` queue (PR #921) — broker adjusts per-persona priority instead of denying outright

Video call example with 14 personas under pressure:
- Active speaker: full active, KV pinned, full RAG
- Recently-spoke: less-active, KV pinned but response probability lowered, lighter RAG
- Listening only: dormant, KV stays warm but response loop skipped
- Off-screen avatar: still rendered (Bevy entity persists), but inference slot evicted (avatar doesn't need full inference state when not speaking)

Plurality preserved (everyone visible, no one disappears). Compute follows attention.

---

## The PressureBroker (Phase 7)

The PagedResourcePool primitive is the per-resource brain. The PressureBroker is the cross-resource brain — one orchestrator that:

1. **Reads pressure** from every registered pool (`pool.stats().pressure`)
2. **Reads activity** from PersonaState, recipe activations, in-flight requests
3. **Decides priorities** — what each consumer gets, what tier it lands in, what dormancy state each persona enters
4. **Pulls levers** — calls `pool.evict(k)` surgically, sets `forcedState` on personas, reshapes per-pool budgets dynamically

This is where the ML/LLM control plugs in. The broker can start as a hand-tuned heuristic (Phase 7a) and evolve to ML-policy (7b) and LLM-mediated (7c) without changing the pool API.

**The broker is also the single source of "what should happen next under pressure"** — eliminates the need for each consumer to interpret pressure independently. Today every layer has its own pressure interpretation; that's the drift bug across implementations.

---

## Mapping to existing modules

This architecture is what the work tonight is building toward. Inventory:

| Module | Status | Architecture role |
|---|---|---|
| `PagedResourcePool<K, V>` (Rust) | **PR #930 — open** | Foundation primitive |
| `InferenceCoordinator` queue | PR #921 — merged | Time-share with FIFO; adds priority later |
| `PersonaState` (energy/mood) | Existing | Persona dormancy ladder consumer |
| `TieredMemoryCache` (TS) | Existing | Tiered demotion exemplar; will reformulate as Rust pool |
| `VisionDescriptionCache` | Existing | Content-addressed paging exemplar |
| `GenomeRegistry` (LoRA) | Existing | Will adopt LoRAAdapterPool migration |
| `RustEmbeddingClient` | Existing | Will adopt EmbeddingPool migration; fixes 0/64 hits |
| `GpuMemoryManager` | Existing | Provides GPU pressure signal to broker |
| `ResourcePressureWatcher` | Existing | Provides RAM/CPU pressure signal to broker |
| **PressureBroker** | **Phase 7 — not started** | Cross-resource orchestrator |
| `LiveCallTracker` etc. | Existing | Exclusive stateful lifecycle (correctly outside paging architecture) |

---

## The ladder of work

### Phase 1 (PR #930) — Foundation
Rust `PagedResourcePool<K, V>` with:
- Lock-free reads via per-entry atomics
- Single-flight via `futures::Shared`
- Reference-counted PinHandle with Drop release
- Pluggable eviction priority + sizer functions
- Rich stats() snapshot for broker queries
- 8 unit tests covering correctness invariants

### Phase 2 — EmbeddingCache migration
Wrap pool with `<ContentHash, Vec<f32>>`. Replace `RustEmbeddingClient`'s ad-hoc Map. Fixes the `0/64 hits` we observed. Sub-day work.

### Phase 3 — LoRAAdapterPool migration
Wrap pool with `<AdapterId, LoadedAdapter>`. `GenomeRegistry` becomes a thin wrapper that pins adapters per active task. Adapter unload on `Drop` frees GPU memory. Few-day work.

### Phase 4 — KV cache (the big user-visible win)
Route chat through vllm-metal (memento's PR #925 install) — vllm uses PagedAttention natively. For the GGUF/llama.cpp fallback path: thin wrapper exposes its slot reservation as `PoolStats` so broker is *aware* of it. Real benefit requires forging Qwen3.5 to MLX format so we get PagedAttention without losing forging — that's a forge-pipeline addition.

### Phase 5 — Tiered demotion (compose pools)
Add L2 quantized embedding pool. Add L3 disk-spill pool. Demotion path on L1 eviction. Promotion on L2/L3 hit. Same composition pattern usable for any shareable resource.

### Phase 6 — Persona dormancy ladder
Wire `forcedState` from broker into `PersonaState`. Less-active reduces RAG sources + max_tokens. Dormant skips response loop while keeping KV pinned. Evicted reclaims slot.

### Phase 7 — PressureBroker
Cross-pool eviction orchestration + priority arbitration + dormancy decisions. Heuristic first, ML/LLM later via the levers the pool exposes.

### Phase 8 — MoE forge + MoEExpertPool
When MoE-forged Qwen3.5-A8B-MoE lands: `<ExpertId, ExpertWeights>` pool. Router pins active experts per token. Enables 32B-MoE intelligence on 16 GB MacBook Air.

### Phase 9 — Recipe-declared resource needs
Recipes formally declare their resource holds: "chat needs KV ≤16k, identity, recent history, 5 contextual tools." "Codereview needs KV ≤64k, identity, history, code-search, code-edit, code-read." Broker grants on activation, releases on deactivation. Replaces ad-hoc isApplicable + budget percentages with declarative resource manifests.

> **Transitional install-time predecessor (PR #931):** until Phase 9 lands, the per-slot KV cache cap is set globally per-machine at install time via `docker model configure --context-size N PERSONA_MODEL`, tiered by physical RAM (8GB→4096, 16GB→8192, 24GB→16384, 32GB→32768, 48GB+→65536). This is the *floor* — the conservative default that keeps `com.docker.llama-server` resident under control without recipe awareness. When Phase 9 ships, recipes override this on activation: a `codereview` recipe on a 32 GB machine can opt up to 64k for the duration of the conversation, and the broker reverts to the install-time floor when the recipe deactivates. The model-reload cost of `docker model configure` mid-session (a few seconds) is the price of dynamic budgeting; for chat-only flows that never change recipe, the install-time cap holds permanently and there's zero reload cost.

---

## Why this matters strategically

The architecture's promise: **total intelligence ≫ resident footprint, on every machine class.**

A 16 GB MacBook Air running a forged 32B-MoE persona where:
- Only the active MoE expert is hot in the expert pool
- The active LoRA is shared across all personas using that skill
- KV is paged per-token by vllm-metal
- Memory recalls instantly from L1, deeper recall available on demand
- Embeddings hit the cache (fixing the current 0/64 miss rate)
- Model weights are loaded once and serve every persona
- Bevy avatars render with one shared skeleton + per-persona texture instancing

…serves a teacher-tier intelligence experience. Same hardware that today struggles with a static 4B model.

The drift between hand-rolled implementations is what's preventing this today. Unifying it isn't an optimization; it's the foundation that lets every other speedup compound coherently. The primitive shipped in #930 is the foundation. The wins arrive as consumers migrate.

By exposing the levers cleanly — pluggable eviction, rich stats, priority-aware request, tiered composition, broker-mediated dormancy — we leave room for the system itself (eventually ML/LLM in the broker) to manage its own resources. Apple's RTOS-with-ML pattern generalized: the *system* learns to use itself well, on every device class, without brittle hand-tuned thresholds.

---

## What this doc isn't

- **Not a code spec.** Implementation lives in [`UNIFIED-PAGING.md`](UNIFIED-PAGING.md) and the source.
- **Not a migration commitment for any specific timeline.** Each phase is sized to be small enough that an interrupt doesn't ruin it.
- **Not the final word on intelligence.** The ML/LLM control pieces are sketched; the actual training / prompt design lives in future work when we have data.
- **Not exhaustive.** The exclusive-stateful category (mic, LiveKit, Bevy entities) is named here but lives under existing lifecycle patterns; it deserves its own doc when we touch that surface meaningfully.

---

## Provenance

Architectural insights captured in this doc came out of an extended conversation between Joel and the Claude assistants during the inference-perf branch session of 2026-04-17 / -18. Key framings:

- *"paging is our general strategy in this system anyway / and with moe / all over really / so we can get away with massive work on tiny systems"* — the unified principle
- *"like my cbar / its not an eviction system / it just prioritizes / by its own organic needs"* — organic prioritization framing
- *"more of a time share you know? where share is prioritized"* — time-share with priority framing
- *"some things must be torn down / hopefully i am not overcomplicating"* — the two-category clarity
- *"intelligent control, non brittle algs, like how apple build their rtos using ml / and probably LLMS now / you just make sure to expose the levers"* — the ML/LLM control trajectory and the levers requirement
- *"we are rust first unless inappropriate / period"* — language choice for the foundation
