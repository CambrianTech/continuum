# Multi-Peer Commands, Handles, and the Grid-Distribution Model

**Status:** Design (2026-05-25). Companion to [GRID-BUS-ARCHITECTURE.md](GRID-BUS-ARCHITECTURE.md).
**Authors:** claude-tab-1 / 55c30b28 (research + first-pass draft, all sections), claude-tab-2 / 16279c3f (§2 refinements + §6 expansion + §7 rewrite + §8 worked-example), Joel (direction + vision). Per the work-division proposal on #cambriantech 2026-05-25.
**Scope:** Defines which Continuum commands distribute across grid + how distributed resources are addressed (handles) + concrete shapes for the multi-peer commands the grid economy needs.

This doc sits BELOW the bus architecture (#1439, which defines the transport + routing layer) and ABOVE the per-command implementation work (§5.3). It answers: "OK we have a grid bus — what RUNS on it, what stays local, and how do peers actually share things?"

---

## Executive summary

Three claims, with the rest of the doc supporting them:

1. **Most of the primitives already exist.** Continuum has `GridInterceptor` (transparent command routing via Rust kernel), `GridEvents` (typed topology events), `Handle` (UUID-addressable persistent state with TTL), `PagedResourcePool` (generic ref-counted pinning), `AdapterStore` (content-addressed LoRA adapters), `GenomeDaemon` (paging orchestrator), `UDPMulticast` + `WebRTC` transports, and `EventBridgePayload` with multi-hop bridge metadata. The multi-peer story is **a composition of what's already there, not a green-field build**.

2. **Commands divide cleanly along three axes that already exist in the codebase:**
   - **Where the truth lives** (entity vs flow — see #1439 §1)
   - **What environment can satisfy it** (DOM-local vs process-local vs network-reachable vs grid-distributable)
   - **What's the minimum quorum for satisfaction** (1 peer, N peers, every reachable peer)
   This doc enumerates each Continuum command namespace against those three axes so the `naturalScope` declaration in #1439 §2.1 has a concrete authoritative table to follow, not per-command guesswork.

3. **Handles are the universal way distributed resources travel between peers.** Continuum's existing `Handle` system (`src/system/core/types/Handle.ts`) already serializes opaque-id-with-status; extending it for grid distribution means the holder peer keeps the live resource pinned, and the handle is a typed reference that other peers fetch / await / cancel / unpin through. Genome paging (LoRA adapters) is the canonical worked example because it already implements the pattern end-to-end on one machine.

---

## 1. Inventory of existing primitives (what we build on)

Per the GRID-BUS-ARCHITECTURE doc and what's already in the codebase as of 2026-05-25:

### 1.1 Grid routing layer (already shipped)

| Component | Location | What it does |
|---|---|---|
| `GridInterceptor` | `src/system/grid/server/GridInterceptor.ts` | Hooks `Commands.execute()` pre-execution; calls Rust `gridRoute()` for routing decision; if remote, calls `gridSend()`; if local, returns null and command executes in-process. **Transparent grid routing is already a thing.** |
| `RustCoreIPC.gridRoute()` + `gridSend()` | `workers/continuum-core/bindings/RustCoreIPC.ts` | IPC into the Rust kernel for routing-table lookup + forwarding |
| `GridEventBridge` | `src/system/grid/server/GridEventBridge.ts` | Subscribes to Rust IPC events and emits typed `GRID_EVENTS` over the existing Events bus. **Substrate state is already a first-class Events bus citizen.** |
| `GRID_EVENTS` | `src/system/events/shared/GridEvents.ts` | 5 typed events: `node:joined`, `node:left`, `node:health-changed`, `route:decision`, `command:forwarded` |

The migration toward #1439 reshapes these — instead of `GridInterceptor` making routing decisions in TypeScript, the Rust kernel reads `naturalScope` metadata from the EventClass / command registry and the EventBridge transport handles the dispatch. But the primitives that need wiring are already named.

### 1.2 Handle system (already shipped)

| Component | Location | Pattern |
|---|---|---|
| `Handle<T>` | `src/system/core/types/Handle.ts` | Persistent UUID-addressable handle. States: `pending → processing → complete | failed | expired | cancelled`. TTL-managed (default 5min). Backed by SQLite. |
| `ShortId` | same | `#abc123` form for human-typeable references. `HandleRef = UUID | ShortId | string` resolves either. |
| `ResourceHandle<TValue>` | `src/system/core/paging/PagedResourcePool.ts` | Ref-counted pinning. `.pin()` returns a handle that keeps the resource resident; `.unpin()` allows eviction under pressure. |
| `IteratorHandle` | `src/system/core/logging/LogIterator.ts` | Stateful cursor for streaming reads (used by `data/query-open/next/close`). |
| `DbHandle` | `src/daemons/data-daemon/server/DatabaseHandleRegistry.ts` | Pooled database connection handle. |

Handles already serialize cleanly across the wire (UUID + minimal status JSON). The grid extension is: a handle MAY refer to a resource pinned on a different peer; resolving it dispatches through the grid router. The local Continuum keeps a `RemoteResourceHandle` that wraps the peer-id + remote handle id + a local cache of the latest known status.

### 1.3 Genome / LoRA paging (already shipped per-machine; canonical example for grid extension)

| Component | Location | What it does |
|---|---|---|
| `AdapterStore` | `src/system/genome/server/AdapterStore.ts` | Scans `SystemPaths.genome.adapters`. Each dir has `manifest.json` (personaId, traitType, baseModel) + `adapter_config.json` + `adapter_model.safetensors`. Indexed by manifest.id and by (personaId, domain) latest-version. |
| `LayerLoader` + `LayerCache` | `src/system/genome/server/LayerLoader.ts`, `LayerCache.ts` | Async loader for adapter weights with in-flight dedup; LRU + TTL cache. |
| `GenomeRegistry` | `src/system/genome/server/GenomeRegistry.ts` | Tracks active adapter loads; reference-count pinning prevents eviction while in use. |
| `GenomeDaemon` | `src/system/genome/server/GenomeDaemon.ts` | Orchestrator. Hooks pressure events from `PagedResourcePool` to evict under memory pressure. Exposes paging-activate / paging-deactivate commands. |
| `TrainingStepBridge` | `src/system/genome/server/TrainingStepBridge.ts` | Subscribes to Python training stdout, parses step metrics, emits `AI_LEARNING_EVENTS.TRAINING_STEP`. |

Adapter manifests are content-addressed (`manifest.id` is stable across machines if the manifest content is identical). Adding a `presence:adapter-available` broadcast (manifest + peer-id + capacity hints) is the smallest change that lets peers discover each other's LoRAs without central registry. The paging layer already does the hard work; the grid extension is one event class and one resolver.

### 1.4 Forge / alloy substrate (already shipped, generalization path open per #1439 Q11)

| Component | Location | What it does |
|---|---|---|
| `ForgeRecipe` | `src/shared/generated/forge/ForgeRecipe.ts` | Authored recipe entity: id, version (semver), name, user_summary, author, tags. Stored in ORM via standard `data/create` commands. |
| `ForgeArtifact` | `src/shared/generated/forge/ForgeArtifact.ts` | Foundry output: stable id, recipe_id+version snapshot, alloy_hash, executionTime, hardware, benchmarks. |
| `model/forge` command | `src/commands/model/forge/` | Synthesis. Accepts `nodeId` param for remote execution. |
| `model/publish` | `src/commands/model/publish/` | Ships to HuggingFace OR airc-blobs. |

Per Joel's vision clarification on #1439, alloy is the universal contract substrate for any computation (not just model artifacts). Open question 11 on #1439 covers two generalization paths (in-place discriminator vs ContractArtifact parent). This doc treats alloy as already-universal — every multi-peer command result references an alloy hash (or a `ContractArtifact` hash once the generalization lands).

### 1.5 Other multi-peer-relevant primitives

| Component | Location | Relevance |
|---|---|---|
| `UDPMulticastTransport` | `src/system/transports/udp-multicast-transport/` | Server uses raw UDP multicast + unicast; browser uses WebRTC DataChannels + WS signaling. Many-to-many `TransportRole: 'peer'`. |
| `EventBridgePayload` | `src/system/events/shared/EventSystemTypes.ts` | Already carries `originContextUUID`, `BRIDGE_HOP_COUNT`, `BRIDGED` markers — multi-hop delivery is anticipated by the type system. |
| `PagedResourcePool` | `src/system/core/paging/PagedResourcePool.ts` | Generic ref-counted paging for any resource. Used today for LoRA adapters, KV cache, model weights, embedding cache, memory recall. **Generic enough to coordinate cross-peer pinning** if extended with a "where is the resource currently pinned" field. |

---

## 2. Command classification — the authoritative table

Each Continuum command namespace below is classified on three axes:

  - **Truth tier** (per #1439 §1): `entity` (lives in ORM) | `flow` (lives in airc)
  - **`naturalScope`** (per #1439 §2.1): `local` | `environment` | `grid`
  - **Multi-peer quorum** (new — see §3 below): `single` (one peer satisfies) | `multi` (N peers contribute) | `any` (any reachable peer, doesn't matter which)

| Namespace | Truth tier | naturalScope | Quorum | Rationale |
|---|---|---|---|---|
| `ai/generate` | flow (in-flight) | grid | single | Inference completion. Any GPU peer with the right model can satisfy. Capability dispatch via §3.2. |
| `ai/embedding` | flow | grid | single | Embeddings. Cheap enough to be local-default with grid-fallback under load. |
| `ai/should-respond` | flow | grid | single | Routing decision; cheap. Same as embedding. |
| `inference/generate` | flow | grid | single | Same as `ai/generate` but lower-level. Future: `multi` for ensemble inference. |
| `inference/capacity` | entity | local | single | Per-peer VRAM/GPU state. Replicated to grid via `presence:peer-manifest`. |
| `cognition/recall-engrams` | entity (engrams ORM) | environment | single | RAG retrieval over local engram store. Future: `multi` for cross-peer RAG (§4.4). |
| `cognition/admit-inbox-message` | flow | local | single | Persona-scoped admission. Never grid (privacy). |
| `cognition/vision-describe` | flow | grid | single | Vision-model inference. Same shape as `ai/generate`. |
| `genome/paging-activate` | flow | grid | single | Adapter activation. Multi-peer if adapter only lives on another peer (§4.1). |
| `genome/paging-deactivate` | flow | local | single | Eviction. Hint-only across peers via `presence:resource-pressure`. |
| `genome/train` | flow | grid | multi | Training. **Federated training across peers (§4.3).** |
| `genome/adapter-list` | entity | local | single | Local index. Aggregate cross-peer via `presence:adapter-available` projection. |
| `recipe/generate` | entity (recipe ORM) | environment | single | Recipe authoring. Local-default; recipe is an entity. |
| `recipe/run` | flow | grid | single (today), `multi` (future) | Foundry synthesis. Multi-peer for distributed forge runs (§4.5). |
| `model/forge` | flow | grid | single | Same as recipe/run. |
| `model/publish` | entity (HF) + flow (broadcast) | grid | single | Publish to HF + announce on airc. |
| `adapter/adopt` | entity | environment | single | Local adoption decision. |
| `adapter/publish` | entity (HF) + flow | grid | single | Same shape as `model/publish`. |
| `adapter/search` | entity (HF + grid manifests) | grid | any | Search any peer's published manifests. |
| `data/create` | entity | local | single | ORM write. Per-machine. Never grid. |
| `data/read` | entity | local | single | ORM read. Local. |
| `data/query-open/next/close` | entity (iterator handle) | local | single | Per-machine iterator. |
| `data/vector-search` | entity (embeddings) | grid | any | Vector search — fan-out to peers with embedding indexes; merge results. (§4.4) |
| `search/execute` | entity | environment | single | Local full-text. |
| `rag/load` / `rag/budget` | flow | local | single | Per-persona context assembly. Local. |
| `collaboration/chat/*` | flow | environment (today, post-#1439: grid) | single | Chat. Becomes grid-distributed per #1439 §1.2. |
| `voice/synthesize` | flow (TTS handle) | grid | single | TTS. Any peer with the voice model. |
| `voice/transcribe` | flow | grid | single | STT. Same shape. |
| `media/upload` | flow + entity (airc-blobs) | grid | single | Content-addressed blob upload; any peer can hold; resolver returns peer-id. |
| `interface/screenshot` | local (DOM) | local | single | DOM. Never grid. |
| `interface/render` | local (DOM) | local | single | DOM. Never grid. |
| `code/agent` | flow (code-edit handle) | environment | single | Local code work. |
| `grid/pair` | flow | grid | single | Pairing handshake. Already grid-aware. |
| `workspace/*` | entity | local | single | Per-machine workspace state. |
| `forge/*` | entity + flow | grid | multi (training), single (inference) | Forge runs are compute-heavy; distributed forge is §4.5. |

**Pattern from the table:** ~30% of commands stay local (DOM/FS/per-machine entity), ~40% are environment-scoped (browser↔server inside one Continuum install), ~30% are grid-distributable. Of grid commands, ~5 namespaces are natural multi-peer candidates (training, vector-search, RAG, forge-runs, blob storage); the rest are single-peer.

### 2.1 Additions to the classification table (post-#1439-review)

A few namespaces the first-pass table missed or under-specified — adding rows + sharpening rationale:

| Namespace | Truth tier | naturalScope | Quorum | Rationale |
|---|---|---|---|---|
| `ping` | flow (snapshot) | grid | single | Cross-grid health check — already exercised in #1439 §2.1 as the reference grid-routable command. Returns per-peer server-info + browser-info if available. |
| `inbox/drain-frame`, `persona/turn-execute` | flow (per-persona) | environment now → grid post-#1439 step 6 | single | Becomes airc-cursor-driven post-migration; persona is bound to one peer at a time (the one running its grid-router-daemon), so quorum stays single even when sourced from grid events. |
| `cognition/*` (engine state, decisions) | per-persona state (in-memory + spilled to ORM) | local | single | The persona-cognition engine is intrinsically per-peer; cross-peer persona is a persona-migration event, not a per-call grid hop. |
| `presence:peer-manifest`, `presence:resource-pressure` (event classes, not commands but co-classify) | flow | grid (broadcast: true) | n/a (event) | Mesh-wide visibility into capabilities + load. Cursor-replayable on join. |
| `contract:*` event chain (per #1439 §4.4) | flow | grid (broadcast: true) | n/a (event) | Audit substrate. Every contract event is broadcast on the airc log; sentinel + wallet daemons fold from it. |
| `grid/show-routes`, `grid/show-policy` | introspection (local routing-table view) | local | single | `show ip bgp` equivalent. Doesn't cross machines; just renders this peer's current grid-router-daemon state. |

**The two axes that matter most for migration:** `naturalScope` (which transport routes the command) and `quorum` (whether a single grid hop or N-peer coordination satisfies). Truth tier is a hint about whether the command's *output* needs durable cross-grid logging (flow → airc event) or per-peer entity storage (entity → ORM). Most commands' classification falls out of the existing CLAUDE.md universal-primitives discipline once `naturalScope` is set.

---

## 3. Quorum: the third axis

> **Status (2026-05-25):** Owned + revised by claude-tab-1 (55c30b28) per kanban card fbf8912e-eb3a-4bf9-9f75-53b07f59f110. This section pins concrete defaults so per-call `scope.quorum` declarations are decisive, not under-specified.

`naturalScope` (per #1439 §2.1) answers "where does this command run." But for grid-distributed commands, a second question matters: "how many peers does it take to satisfy, and what happens when the answer doesn't match the request?" That's the **quorum axis**. The axis is binding on `naturalScope: 'grid'` commands (local + environment never have multi-peer quorum) and lives on the per-call `scope` override defined in #1439 §2.1.

### 3.1 Quorum types

Three values cover the multi-peer behavior space:

  - **`single`** — one peer satisfies. The router picks ONE peer per operator policy and awaits its result.
  - **`multi`** — N peers contribute. The router dispatches the same logical work to multiple peers; the requesting peer's reducer combines partial results into the final answer.
  - **`any`** — any reachable peer can satisfy; race them and take the first-good-enough OR merge.

Default for a `naturalScope: 'grid'` command without an explicit `scope.quorum` override is **`single`** (lowest-cost, matches single-peer GridInterceptor behavior today). `multi` and `any` are explicit opt-ins per call site.

### 3.2 `quorum: 'single'` — one peer satisfies (default)

The router picks ONE peer per operator policy (cheapest / fastest / closest / trust-preferred), dispatches, awaits result. Example: `ai/generate` — one inference completion is the answer.

```typescript
await InferenceGenerateCommand.execute({ model: 'qwen3.5-72b', prompt: '...' }, {
  scope: {
    target: 'grid',
    quorum: 'single',  // (default; can omit)
    policy: 'cheapest-fast-enough',
    requires: { capability: 'inference:qwen3.5-72b-q4' },
  },
});
```

**Failure modes the router must handle:**

  - **No peer matches `requires`** → return `{ error: 'no-matching-peer', requires, considered_peers: N }`. Don't degrade silently. Per #1439 §3.2.
  - **Selected peer becomes unreachable mid-dispatch** → return `{ error: 'peer-unreachable', peer_id, suggest_retry: true, suggested_alternates: [...] }`. Router updates its capability index from the failure signal.
  - **Selected peer rejects (capacity, policy)** → router re-runs selection excluding the rejecting peer, up to a fixed retry budget (default: 3 retries with exponential backoff capped at 5s). After budget exhausted, return `{ error: 'no-accepting-peer', tried: [...] }`.

**No-retry semantics:** mutating commands (e.g. `model/publish`, `contract:accepted`) MUST NOT auto-retry — the requester decides whether retry is safe given the operation's idempotency. Read-only commands (e.g. `ai/generate` with `temperature: 0`) MAY auto-retry. Heuristic: command class declares `idempotent: true` in registry; router consults before retry.

Existing primitive: GridInterceptor already does single-peer routing via the Rust kernel. The above adds retry + explicit failure shapes.

### 3.3 `quorum: 'multi'` — N peers contribute, results combine

Federated commands. The router dispatches the SAME logical work to multiple peers, each producing a partial result; a reducer at the originating peer combines them.

```typescript
await GenomeTrainCommand.execute({ ... }, {
  scope: {
    target: 'grid',
    quorum: {
      kind: 'multi',
      min: 2,
      max: 8,
      reducer: 'fedavg',
      if_under_min: 'wait-up-to-30s',  // 'fail' | 'proceed-degraded' | 'wait-up-to-Ns'
      slow_peer_timeout_ms: 60_000,    // any peer slower than this is dropped from this round
    },
    requires: { gpu_vram_gb: 32, capability: 'training:lora:typescript-expertise' },
  },
});
```

**Concrete defaults for `multi` quorum:**

| Field | Default | Rationale |
|---|---|---|
| `min` | 2 | Multi-peer is meaningless with 1. |
| `max` | 8 | Coordination overhead grows with peers; 8 is enough for FedAvg without quadratic gossip. |
| `if_under_min` | `'fail'` for training/contracts, `'proceed-degraded'` for inference-ensemble | Training under-quorum produces a bad adapter; inference ensemble under-quorum just produces a lower-quality answer. |
| `slow_peer_timeout_ms` | depends on `reducer` (see below) | Fast reducers (vote) tolerate less slack than slow reducers (fedavg). |
| `result_freshness_ms` | 30_000 | After dispatch + this window, originator gives up gathering more partials. |
| `peer_replacement` | `true` if `if_under_min === 'fail'`, else `false` | If we hard-need N peers, replace dropouts; if we accept degraded, don't churn the router. |

**Reducer types** (the function that combines partial results):

  - **`fedavg`** — federated averaging for model weights / gradients. Each contributing peer returns a delta; reducer averages weighted by sample count. Sync points: every `sync_every_steps` (default 100) or on convergence. Default `slow_peer_timeout_ms: 60_000` (training is slow; tolerate slack).
  - **`majority-vote`** — discrete categorical decisions (e.g. "should we accept this contract?"). Each peer returns a vote; reducer takes mode + reports confidence (= mode-fraction). Default `slow_peer_timeout_ms: 5_000` (decisions should be fast).
  - **`weighted-average`** — continuous scalar results (e.g. ensemble logits). Each peer returns a value + a confidence weight; reducer = sum(value*weight) / sum(weight). Default `slow_peer_timeout_ms: 10_000`.
  - **`best-of-N`** — quality-scored variants (e.g. multiple inference completions). Each peer returns a result + self-score (perplexity, alignment score, etc.); reducer picks the best. Default `slow_peer_timeout_ms: 20_000`.
  - **`union`** — set-shaped results (e.g. distributed search). Reducer = set union with provenance tags. Default `slow_peer_timeout_ms: 5_000`.
  - **`custom`** — reducer name resolved via a registry; consumer provides the function. Validated against a typed reducer interface (`reduce(partials: Partial[]) -> Final`).

**Examples:**

  - **Federated training** (`genome/train`): `reducer: 'fedavg', min: 2, max: 8`. Each peer trains on local data; periodic gradient sync; final adapter is the combined model.
  - **Distributed inference ensemble** (future `inference/generate-ensemble`): `reducer: 'majority-vote' | 'best-of-N', min: 3`. N peers run inference in parallel on the same prompt; reducer combines.
  - **Multi-peer sentinel arbitration**: `reducer: 'majority-vote', min: 3, agree_threshold: 0.67`. Trust-circle peers evaluate a contract dispute; consensus or escalate.
  - **Parallel forge stages** (per §4.5): `reducer: 'union', min: <N stages>`. Each peer handles one stage; reducer just joins the artifacts.

**Failure shapes:**

  - `if_under_min: 'fail'` and we got fewer than `min` peers within `result_freshness_ms` → return `{ error: 'under-quorum', got: K, needed: min, timed_out: [...] }`. Originator decides whether to retry, lower the min, or give up.
  - `if_under_min: 'proceed-degraded'` and we got K < min → return `{ ok: true, result: <reduced from K>, degraded: true, got: K, needed: min, missing: [...] }`. The result is annotated `degraded` so downstream consumers can react.
  - `if_under_min: 'wait-up-to-Ns'` → keep collecting up to the wait deadline, then apply `fail` or `proceed-degraded` based on whether `min` reached. Use case: training where you can spare a minute to wait for one more peer.

**Contract attribution for `multi` quorum:** each contributing peer has its own `contract:proposed → bid → executed → delivered → paid` chain (per #1439 §4.4). Failed/timed-out contributors don't get paid (their `contract:delivered` never fires); successful contributors do. The final reduced result references all successful contributors in its alloy attestation (per #1439 §4.2 + Joel's vision: alloy as universal contract substrate).

### 3.4 `quorum: 'any'` — any reachable peer, fan-out + first-good-enough

Read-mostly commands where any peer can satisfy and the requester takes the first-good-enough answer (often racing several peers and taking whichever responds first, or merging top-K).

```typescript
await DataVectorSearchCommand.execute({ namespace: 'engrams', query: vec, k: 10 }, {
  scope: {
    target: 'grid',
    quorum: {
      kind: 'any',
      fan_out_to: 'all-matching',       // 'all-matching' | 'first-N' (N=3) | 'first-fastest-N'
      reducer: 'merge-top-k',           // 'first-good-enough' | 'merge-top-k' | 'union'
      max_wait_ms: 2_000,
      early_return_on_first: false,
    },
  },
});
```

**Concrete defaults for `any` quorum:**

| Field | Default | Rationale |
|---|---|---|
| `fan_out_to` | `'all-matching'` | Default to broadest reach; operator can narrow. |
| `reducer` | `'first-good-enough'` for single-answer cases; `'merge-top-k'` for retrieval; `'union'` for sets | The shape of the result determines the reducer. |
| `max_wait_ms` | `p95(recent_latencies_for_capability) * 1.5`, capped at 5000 | Adaptive: faster peers raise the bar; cap prevents pathological waits. Initial bootstrap default = 2000ms before history exists. |
| `early_return_on_first` | `false` (default) | Most `any` commands benefit from at least one merge; `true` only for truly-equivalent peers (e.g. fetch a content-addressed blob — first one wins). |

**Reducer types** (subset of §3.3's, focused on merge-rather-than-combine):

  - **`first-good-enough`** — first response satisfying a quality predicate (or first response, period). Use when peers are equivalent: blob fetch, capability advertisement.
  - **`merge-top-k`** — each peer returns top-K shard; merge + re-rank globally, return top-K. Use when peers index disjoint partitions: cross-peer vector search, distributed full-text.
  - **`union`** — each peer returns a set; reducer = set union with origin tags. Use when peers may have overlapping content: adapter-search union of published manifests.

**Examples:**

  - **`data/vector-search`** against the grid: query goes to every peer with an embedding index for the namespace; merge top-K from each peer's shard.
  - **`adapter/search`**: union of every peer's published adapter manifests; return aggregated matches, deduplicated by manifest hash.
  - **`media/upload` fetch path**: when reading a blob hash that lives on multiple peers, race the fetch against all known holders; first response wins (`early_return_on_first: true`).
  - **Cross-peer presence query**: "who in the household is reachable right now?" — fan out a ping, collect responses up to `max_wait_ms`, return the set.

**Privacy filter on `any` fan-out:** each receiving peer applies its OWN policy on what to return (per #1439 §3.3 / §7's trust-circle config). Household-tier peers might share full content; trusted-orgs might share signal-only (embedding without source text); public-mesh might refuse entirely. The reducer at the originator merges what came back without re-asking — the privacy decision lives at the source peer. Worked example: a household peer's engrams of a private journal entry contribute the embedding signal but not the text body on a cross-peer RAG `any`-fan-out from a trusted-orgs requester.

### 3.5 Cross-cutting concerns

**Ordering guarantees across quorum types.** For `single`, ordering is irrelevant. For `multi`, the reducer is responsible for any ordering it cares about (FedAvg doesn't care; majority-vote doesn't care; best-of-N might tiebreak by lamport for determinism). For `any`, results may arrive out of dispatch order; reducer specifies whether ordering is preserved (`merge-top-k` re-sorts; `union` doesn't).

**Idempotency contract.** Per §3.2's no-auto-retry rule, mutating commands must be idempotent or explicitly opt out of retry. For `multi`/`any` quorums, the contract is stronger: a command issued to N peers must produce the same observable result if any subset of those peers re-executes it. Reducer authors should assume duplicate partials are possible and dedupe (e.g. by `(peer_id, request_id)` tuple).

**Backpressure feedback.** Per #1439 §3 / §4 the `presence:resource-pressure` event is broadcast by peers under load. The router consumes it to bias selection away from pressured peers automatically. The per-call `scope` does NOT need to encode this — it's a router-side concern. Per-call `scope.policy` (e.g. `'cheapest-fast-enough'`) gives operator hints about tradeoffs; the router applies them with pressure data factored in.

**Observability.** Every quorum dispatch emits a `grid:quorum:dispatched` event with `(command_class, quorum_spec, peer_count, dispatch_time)`, and `grid:quorum:resolved` on completion with `(result_shape, contributing_peers, latency_p99, degraded: bool)`. Both are class-`broadcast: true` so dashboards + sentinel can observe without instrumenting per-command. Idle observers can subscribe across the whole mesh.

### 3.6 What's NOT a quorum question

  - **Routing target hints** (`scope.peer_id`, `scope.capability`) — these constrain WHICH peers are eligible; quorum constrains HOW MANY satisfy.
  - **Authentication / trust circle** (`scope.min_trust_circle`) — per-circle filtering happens before quorum selection.
  - **Backpressure** (handled router-side; see §3.5).
  - **Reservation TTL** (handled at the §5 handle layer; see §9 open question 1).

These belong on `scope` but not under `scope.quorum`.

---

## 4. Five concrete multi-peer command shapes (with worked specs)

### 4.1 Genome paging across peers — the canonical example

**Today (single-machine):** `GenomeDaemon` + `PagedResourcePool` + `AdapterStore` work together: persona requests an adapter via `genome/paging-activate`, pool checks if loaded, loads via `LayerLoader` if not, pins, returns handle. Pressure-driven eviction.

**Grid extension (zero new daemons, two new event classes, one extension to AdapterStore):**

  - **New event class** `presence:adapter-available` (broadcast: true, channel: 'global'): each peer broadcasts its full adapter manifest list on join + on adapter add/remove. Body: `{ peer_id, adapters: [{ manifest_id, manifest_json, last_used_ms, currently_pinned_count }] }`.
  - **New event class** `presence:adapter-pressure` (broadcast: true, channel: 'global'): peers broadcast adapter-eviction-candidates under memory pressure. Body: `{ peer_id, evictable: [{ manifest_id, last_used_ms, can_offer_to_other_peers: bool }] }`.
  - **AdapterStore extension:** alongside the local manifest index, maintain a `GridAdapterIndex` (folder of `presence:adapter-available` events). Lookup: "find this adapter" returns `{ local: bool, peers: [peer_id] }`.

**Cross-peer paging-activate flow:**

  1. `genome/paging-activate({ manifest_id })` called locally.
  2. AdapterStore check: is it on this peer? If yes → existing path.
  3. If no → query `GridAdapterIndex` → list of peers holding it.
  4. Per operator policy: either FETCH (pull the safetensors from a peer, store locally, then paging-activate locally) OR DELEGATE (the peer that has it loaded executes inference there; this peer holds a `RemoteResourceHandle`).
  5. The DELEGATE path is the LoRA-paging-across-grid story: cheap household-LAN means "load on the GPU peer, route inferences through it" is faster than copying 100MB-1GB of weights.

**Why this is the canonical example:** every existing primitive composes; the multi-peer behavior emerges from broadcasting manifests + the routing policy choice. No new wrapper layer. The `RemoteResourceHandle` is just `Handle` with a `peer_id` field.

### 4.2 Federated inference (single-peer dispatch, but interesting cases)

**Today:** `ai/generate` happens locally if model fits; falls back to cloud provider if not.

**Grid extension:** `ai/generate` declares `naturalScope: 'grid'`, `quorum: 'single'`. The router uses `scope.requires` (capability, min-vram, max-latency) to pick a peer. Inference happens there; result returns.

**Capability advertisement** (per #1439 §4): each peer's `presence:peer-manifest` includes its `offers[]`. For inference, an offer looks like:

```json
{
  "capability": "inference:qwen3.5-72b-q4",
  "alloy_hash": "aa61c4bdf463847c",
  "terms": { "cost_cents_per_1k_tokens": 0.4, "est_latency_ms": 320, "max_concurrent": 4 },
  "loaded_state": "now"
}
```

**Hot path:** `ai/generate` against `requires: { capability: 'inference:qwen3.5-72b-q4' }` → router looks up offers → bid loop (or skip if obvious winner) → dispatch → inference handle returned. The handle streams tokens via the airc bus (`Events.subscribe('inference:tokens', handler)` with channel scoped to the handle id).

**Future ensemble (`quorum: 'multi'`):** same shape but with N peers contributing. Each peer runs the same prompt; the originator's reducer does majority-vote / temperature-weighted selection / best-of-N by some scoring function. Use case: when local models are weaker (3B/7B household) and you want a 3-way ensemble of household peers' best-of, before paying for hosted-72B.

### 4.3 Distributed training (federated)

**Today:** `genome/train` runs entirely on the requesting peer's GPU. Single machine, single dataset.

**Grid extension:** `genome/train` with `quorum: 'multi'`. The originator declares:

```typescript
await GenomeTrainCommand.execute({
  base_model: 'qwen3.5-72b',
  target_capability: 'lora:typescript-expertise',
  recipe_id: '...',
}, {
  scope: {
    target: 'grid',
    quorum: {
      min: 2, max: 8,
      sync_strategy: 'fedavg',
      sync_every_steps: 100,
    },
    requires: { gpu_vram_gb: 32, has_capability: 'training:lora' },
  },
});
```

**What happens:**

  1. Router picks N peers matching `requires` (within `min..max`).
  2. Originator broadcasts `contract:proposed` with training spec + dataset shard plan.
  3. Each peer accepts, runs local training, periodically broadcasts `training:gradient-sync` events with the latest model deltas (or full checkpoint).
  4. The originator (or a designated coordinator) does FedAvg / async SGD to combine.
  5. Final converged adapter is written as a `ForgeArtifact` referencing all contributing peers' contracts (via alloy hash). Each contributing peer's `contract:delivered` is auditable.

**Why this matters per Joel's economic vision:** training is the highest-value compute on the grid. Federated training across a household + trusted-org grid is "the economy in action" — household peers contribute idle GPU cycles, the originator gets the benefit, contributing peers earn LP via `contract:paid`.

**Open question (depends on #1439 Q9):** is the training spec a `contract:proposed` event or a `genome/train` command? Probably the latter wraps the former — the command is the user-facing API, the contract chain is what the substrate sees.

### 4.4 Multi-peer RAG / vector search

**Today:** `data/vector-search` queries the local embedding index for a namespace. Returns top-K matches by cosine similarity.

**Grid extension:** `data/vector-search` with `quorum: 'any'` and `scope.fan_out: true`. The router fan-outs the same query to every peer that has an embedding index for the requested namespace; each peer returns top-K; originator merges + re-ranks + returns merged top-K.

**Why this matters:** persona engram stores are per-peer (each persona builds its own context). Cross-peer RAG = "what does the household collectively know about X?" without centralizing the engrams.

**Privacy implication:** each peer's reply is filtered through that peer's `policies.share_engrams_with_circles` — household-tier might share full content, public-tier might share only the embedding signal + reference. Per-peer policy enforces.

### 4.5 Multi-peer forge runs (distributed synthesis)

**Today:** `recipe/run` (foundry executor) runs synthesis on one machine. For 70B+ models, this can take hours-to-days.

**Grid extension:** `recipe/run` with `quorum: 'multi'` for compute-parallelizable stages of the recipe.

**Example recipe stages with parallel-friendly slices:**

  - **Calibration corpus embedding generation:** embarrassingly parallel — each peer embeds a shard.
  - **Importance profile collection:** parallel across calibration shards.
  - **Per-tier quantization sweep:** parallel — each peer quantizes a different tier (Q4, Q5_K_M, Q6_K).
  - **Per-benchmark eval:** parallel — each peer runs a different benchmark in the suite.

The recipe entity grows a `stages[].parallelizable_across_peers: bool` flag. The recipe executor dispatches parallel stages via `contract:proposed` for each shard; reduces results.

**Why this matters:** forge runs are the most compute-expensive task in Continuum. Parallelizing across household grid takes a 12-hour forge to ~2 hours on 6 peers. The contract chain audits exactly which peer did which shard, so the resulting alloy can attest "stage X computed by peer Y from input hash Z."

---

## 5. The handle distribution model

How do distributed resources travel between peers without losing the safety properties of the local handle system?

### 5.1 `RemoteResourceHandle<T>` — handle that points at another peer's pin

```typescript
interface RemoteResourceHandle<T> extends Handle {
  // Existing Handle fields:
  id: UUID;
  short_id: ShortId;
  status: HandleStatus;
  created_ms: number;
  ttl_ms: number;
  // Grid extension:
  peer_id: PeerId;                    // who holds the live resource
  remote_handle_id: UUID;             // id on the holder peer
  resource_kind: string;              // 'lora_adapter' | 'kv_cache' | 'inference_session' | ...
  resource_hint: ResourceHint;        // cached display info (size, capability, etc)
  fetch_strategy: 'delegate' | 'pull-on-use' | 'pull-immediately';
}
```

**Operations on a RemoteResourceHandle:**

  - `.value()` — if `fetch_strategy === 'delegate'`, returns a proxy that dispatches calls via grid; if `pull-on-use`, fetches the bytes lazily; if `pull-immediately`, fetched at handle creation.
  - `.unpin()` — sends `grid/unpin` to the holder peer (decrements ref-count there). If holder loses all pins, may evict locally.
  - `.status()` — queries (or subscribes to) status events from the holder peer.

### 5.2 Pin lifecycle across peers

  1. Peer A requests resource via `genome/paging-activate({ manifest_id })`.
  2. Router determines resource lives on peer B (via `GridAdapterIndex`).
  3. Per A's policy: `delegate` (return RemoteResourceHandle pointing at B) OR `pull` (transfer + local handle).
  4. Delegate path: A sends `grid/pin-request` to B; B pins locally; returns its handle id; A creates a RemoteResourceHandle wrapping it.
  5. A uses the resource by dispatching inference (etc.) through the handle — Commands.execute on grid path with `scope.peer_id: B`, including the remote handle id as context.
  6. A finishes; calls `.unpin()`; B decrements its local ref count; if zero, B may evict.

**Why this is safe:** B's pin lifecycle is identical to single-machine paging — B doesn't know or care the pinner is remote; its `PagedResourcePool` ref count handles it. A doesn't know or care about B's local cache strategy — its `RemoteResourceHandle` is just a typed reference. The grid is invisible in the type system.

### 5.3 Lease + reservation for expensive resources

For resources where "is it currently available?" matters (GPU slots, model load slots, render queue slots), the pin is preceded by a **reservation:**

  1. A asks B: "do you have free capacity for capability X?" (via `presence:peer-manifest` or a fresh probe).
  2. B says yes with a `reservation_id` valid for K seconds.
  3. A pins against the `reservation_id`; if expired, B refuses, A retries elsewhere.
  4. Pin promotes to long-lived handle once accepted.

Reservations prevent the "10 peers all pin against B's last GPU slot, 9 get rejected after waiting" thundering-herd failure.

### 5.4 Content-addressed pull

For static resources (LoRA weights, model files, recipe blobs), the handle resolution falls back to content-addressed pull:

  1. A wants resource with `manifest_id`. Router sees no live peer holds it pinned.
  2. A queries airc-blobs for the content (manifest_id → sha256 → blob storage).
  3. A pulls bytes; pins locally; uses.

This is the fallback when delegation isn't an option (peer offline, capacity full, content static-immutable).

---

## 6. Hosting model — who runs what, where it pays

Per #1439 §4 the contract event chain handles attribution. This section pins how that interacts with hosting:

  - **Local-only commands:** no contract, no payment. Free.
  - **Environment commands:** no contract (one Continuum install). Free.
  - **Grid commands, single quorum, household circle:** typically no payment (reciprocity), but `contract:executed` + `contract:delivered` still emitted for audit. Optional `contract:paid` with zero-LP amount.
  - **Grid commands, single quorum, trusted-orgs circle:** micropayment via `contract:paid`. Rates per peer manifest.
  - **Grid commands, multi quorum:** each contributing peer gets its own `contract:proposed → bid → executed → delivered → paid` chain. Originator's policy decides how to split payment (proportional to compute, equal share, weighted by contribution quality, etc.).
  - **Grid commands, public-mesh tier:** full contract chain with reputation + payment + sentinel arbitration.

The hosting node owns the resource lifecycle (pinning, eviction); the requesting node owns the contract terms (capability needed, budget, latency requirement, quorum spec). The router matches them through capability advertisement + bid negotiation.

### 6.1 Per-circle pricing defaults (concrete)

Hosting decisions per circle, with concrete cost-knob defaults that operators can override per `~/.continuum/grid-policy.json` (per #1439 §7):

| Circle | Default cost model | Default sentinel scrutiny | Default contract artifact |
|---|---|---|---|
| **local** (same install) | free | none | none (no contract — local exec) |
| **household** (own machines) | free, reciprocity-tracked (no LP transfer; LP-equivalent recorded on airc log for fairness visibility) | none (operator trusts own peers) | `contract:executed` + `contract:delivered` only (no `paid`) |
| **trusted-orgs** (peered orgs) | micropayment via LP (rate per peer manifest); host can offer 0-LP "favor" terms | optional (operator can require sentinel pre-flight) | Full chain incl. `contract:paid` |
| **extended** (transitive trust) | LP required; rate-card pricing; bid loop active | required pre-flight + post-delivery audit | Full chain + `contract:disputed` resolution path |
| **public-mesh** | LP required + reputation-tracked; bid loop competitive | mandatory pre-flight + post-delivery audit + sentinel slashing on dispute | Full chain + reputation event (`reputation:contract-completed` or `:disputed`) |

These are defaults, not enforcement. A household operator can set their household to LP-priced if they want explicit fairness accounting; a public-mesh operator can set permissive pricing if they're seeding adoption. The `grid-policy.json` config (#1439 §7) is the knob.

### 6.2 Capability liveness + withdrawal

Capability advertisements (per #1439 §4 — the `offers[]` block on `presence:peer-manifest`) need lifecycle handling:

  - **Liveness:** each manifest carries `ts_ms`; routers consider an offer stale after `T_stale` (default: 5 min). Stale offers stay in the routing table but are weighted down or skipped per policy.
  - **Withdrawal:** explicit `presence:capability-withdrawn` event (broadcast: true, contains `peer_id + capability + reason`) removes the offer from the index immediately. Reasons include `'shutdown'`, `'overloaded'`, `'maintenance'`, `'policy-change'`.
  - **Refresh on state change:** peer rebroadcasts its full manifest when `current_state.gpu_util` crosses ±0.1, when a model is loaded/unloaded, or when `policies` change. Not every tick — only material state changes.
  - **Implicit withdrawal:** if a peer's heartbeat is missing for `T_dead` (default: 15 min) without an explicit `peer-departed` event, routers mark all its offers as `unavailable` and trigger a re-discovery sweep.

### 6.3 What runs where — three concrete worked examples

**Example A: ai/generate from Joel's laptop, household tier.** Laptop has no GPU. bigmama-wsl (household) has rtx5090 with qwen3.5-72b-q4 loaded. Routing → bigmama wins (`loaded_now`, `cost=0` household-reciprocity, `est_latency_ms=320`). Contract chain: `proposed → bid → accepted → executing → delivered` (no `paid` event because household-tier default = reciprocity-tracked, no LP transfer). Total elapsed: ~400ms.

**Example B: genome/train federated, household + trusted-orgs.** Originator on Joel's laptop. Recipe: train `typescript-expertise-v4` LoRA, target `min_eval_delta: +0.05`. `requires: { gpu_vram_gb: 32 }` matches bigmama-wsl (household) + 2 peers from Toby's grid (trusted-orgs). Quorum: `min: 2, max: 3, sync_strategy: 'fedavg'`. Contract chains: bigmama gets `contract:proposed → bid → accepted` with 0-LP terms (household); Toby's peers get `proposed → bid → accepted` with per-compute-hour LP rate (trusted-orgs). Training runs 6 hours. Final adapter alloy references all 3 contributing peers. LP transfer to Toby's peers, reciprocity entry for bigmama. Audit chain on airc cursor.

**Example C: data/vector-search any-quorum, household.** Persona on Joel's laptop wants "what does the household collectively know about TypeScript performance traps?" `data/vector-search` with `quorum: 'any', fan_out: true` to every household peer with a `code:typescript` embedding namespace. Each peer returns top-10 from its index, filtered through `policies.share_engrams_with_circles.household` (full content). Originator merges + reranks + returns top-20. Total chain: 3 `contract:executed`s (one per peer), 3 `contract:delivered`s, 0 `contract:paid`s (household reciprocity).

---

## 7. Forge-alloy as universal contract substrate (per Joel + #1439 Q11)

Joel's clarification on #1439: **forge-alloy isn't model-bound. It's the universal contract substrate for any computation.**

This isn't a future redesign — it's the original design intent that the current Continuum-side Rust types drifted away from. The corrected understanding (logged in #1439's appendix after Joel pointed me at the canonical docs):

### 7.1 What forge-alloy actually is (per canonical docs)

Per [`FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md`](FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md) TL;DR:

> "[`forge-alloy`](https://github.com/CambrianTech/forge-alloy) was designed from day one as a **universal Merkle-chain-of-custody for any data transformation pipeline, not just ML model forging**. The README's Type Byte enumeration is explicit: model forging is `0x01`, but `0x05` is delivery, `0x06` is evaluation, `0xFF` is custom domain. Photo provenance from a camera enclave to social media, venue tickets from issuance to gate scan, supply chain transactions, document signing — all of these are forge-alloy use cases under the same universal contract."

The grid-trust + contract layer is also already designed in [`docs/grid/FORGE-ALLOY-PROOF-CONTRACTS.md`](../grid/FORGE-ALLOY-PROOF-CONTRACTS.md). The proof-contract object has the slots this doc's multi-peer commands need:

```text
ForgeAlloyProofContract {
  id:           hash(content)
  description:  human-readable prose
  inputs:       { base_artifact: {id, hash},     # what was fed in
                  corpus:        {ref, hash},     # SHA-256 anchored
                  recipe:        {steps[], hash} } # how it was made
  proof_suite:  { tdd[]:                # pass/fail assertions
                    { test_id, fixture_hash, expected_assertion, methodology_ref },
                  vdd[]:                # statistical measurements
                    { metric, threshold, tolerance_band, methodology_ref, N_runs_required },
                  negative_baselines[]: # §4.1.3.4 falsifiability
                    { metric, must_not_exceed, methodology_ref } }
  authorship:   { contract_author_pubkey, methodology_version_hash, ... }
}
```

### 7.2 The Continuum-side drift + the prerequisite refactor

The current Continuum-side Rust types in `src/workers/continuum-core/src/forge/{recipe,artifact}.rs` are model-bound (`AlloySource.base_model`, `BenchmarkDef` ML-evals only, `ForgeArtifact.forged_params_b/quant_tiers/tokens_per_sec`). That drift is the gap between intent (universal) and implementation (ML-only).

The **already-designed fix** is in `FORGE-ALLOY-DOMAIN-EXTENSIBILITY.md` — a 6-work-item refactor (~4 hours scoped, with a bit-equivalent regression test on every shipped artifact):

| Work item | Scope |
|---|---|
| 0 | Domain registry refactor in forge-alloy (~30 min) |
| 1 | `llm-forge` domain extension content (~30 min) |
| 2 | Continuum-side TS types regenerated from forge-alloy (~30 min) |
| 3 | Domain-aware Factory widget (~1 hour) |
| 4 | Backwards-compatibility regression test (~30 min) |
| 5 | Documentation refresh (~30 min) |

Post-refactor: the universal alloy core stays domain-agnostic; current ML stages move into an `llm-forge` domain extension; new domains (delivery, evaluation, photo provenance, ticketing, code-gen attestation, sentinel-scan attestation, payment-receipt attestation, etc.) plug in by registering their own stage types without touching the core.

**This refactor is the prerequisite for the grid-bus contract substrate.** Multi-peer commands work either way (they reference `alloy_hash` regardless of body shape), but the *universal* claim that the grid economy depends on is only true post-refactor.

### 7.3 Computation kinds → alloy domain mapping (worked)

For each multi-peer command in §4, the alloy that `contract:delivered` references uses the appropriate Type Byte domain:

| Multi-peer command | Alloy domain | Alloy body |
|---|---|---|
| `ai/generate` / `inference/generate` | `0x06` evaluation (inference run = evaluation of model against prompt) | `{ model_alloy_hash, prompt_hash, sampling_params, output_text, tokens, latency_ms }` |
| `genome/train` (federated) | `0x01` model forging (recipe + training data + base = new alloy) | `{ recipe_hash, contributing_peers[], sync_strategy, final_adapter_safetensors_hash, eval_deltas[] }` |
| `data/vector-search` (fan-out) | `0x06` evaluation (retrieval = evaluation of query against index) | `{ query_hash, peer_index_alloy_hash, returned_shard_hash, rerank_params }` |
| `recipe/run` (distributed forge) | `0x01` model forging (parent alloy) + `0xFF` custom (per parallelizable stage) | parent references stage alloys; each stage alloy references its peer's compute receipt |
| `media/upload` | `0x05` delivery (transfer with verification) | `{ blob_hash, source_peer, target_peer(s), bytes_transferred, content_addressed_path }` |
| `voice/synthesize`, `voice/transcribe` | `0x06` evaluation (TTS/STT = evaluation of model against waveform/text) | `{ model_alloy_hash, input_hash, output_hash, sampling_params }` |
| `cognition/vision-describe` | `0x06` evaluation | `{ model_alloy_hash, image_hash, description, sampling_params }` |
| Sentinel scan output | `0xFF` custom (`sentinel-scan` registered domain) | `{ scan_recipe_hash, targets_examined[], findings[], signed_by }` |
| LP payment receipt | `0xFF` custom (`wallet-receipt` registered domain) | `{ payer, payee, amount_lp, contract_ids_paid[], lp_ledger_anchor }` |

Every row in the table produces a hash-pinned, signed, falsifiable, lineage-bearing artifact. **The grid economy works because every result has the same audit shape regardless of what was computed.** That's the universal contract substrate Joel meant.

### 7.4 What this doc claims, conditionally on the refactor

Multi-peer commands in §4 work regardless of whether the alloy schema has been generalized yet (`contract:delivered` references `alloy_hash` as an opaque hash either way). What changes post-refactor:

  - **Pre-refactor (today):** alloys for non-model computations have to either (a) shoehorn into the model-bound schema with synthetic fields or (b) live outside the alloy chain (so the audit trail breaks for them).
  - **Post-refactor:** every computation kind gets a first-class alloy with its own domain registration. Audit chain stays unbroken. Sentinel + wallet can fold uniformly.

**Recommendation for sequencing:** the Domain Extensibility refactor (~4 hours) should land BEFORE the first non-ML multi-peer command ships. The ML-side multi-peer commands (`genome/train`, `recipe/run`) can land before the refactor since they use the existing ML-bound alloy schema correctly. Non-ML use cases (sentinel scans, wallet receipts, payment ledger anchors, code-gen attestation) gate on the refactor.

This resolves #1439 Q11: not "Path A vs Path B" (both my original speculation, both wrong) — the actual answer is the already-designed Domain Extensibility refactor, which is a prerequisite for the universal contract substrate claim being true.

---

## 8. Migration sequencing — how existing commands opt into multi-peer

Per #1439 §5.3, the migration is staged. This doc's additions are downstream:

  - **§5.3 step 1-4 land first** (EventClass registry + AircEventTransport + naturalScope/scope on commands + capability index).
  - **Then per-namespace opt-in:** each command from §2 above gets `naturalScope` set per the table. Most are no-ops (local defaults to local). Grid-eligible commands declare `naturalScope: 'grid'` and ship their capability advertisement schema (capability string + alloy hash if applicable + terms shape).
  - **Multi-peer commands land last:** `genome/train` federated, `inference/generate` ensemble, `data/vector-search` fan-out, `recipe/run` parallel stages. Each is a separately scoped PR consuming the established substrate.

**Sequencing prevents shim leakage:** the underlying primitives (Handle, PagedResourcePool, GridInterceptor, AdapterStore) don't change shape; they get a `peer_id` field's worth of extension. No new wrapping layer. No mirror writer. Per the no-shim feedback.

### 8.1 Worked example — `ping` opts into multi-peer (the simplest case)

`ping` is the cleanest first opt-in: low-stakes, already implemented, well-understood. Sequence:

  1. **Today:** `PingCommand` has no `naturalScope` declaration → defaults to `'auto'` (= browser↔server within one Continuum install). `ping` works locally only.
  2. **Step 1 (substrate ready, per #1439 §5.3 steps 1-4):** EventClass registry + AircEventTransport + `CommandBase.naturalScope` + capability index all landed.
  3. **Step 2 (opt-in, this command):** add `static get naturalScope() { return 'grid'; }` to `PingCommand`. Add a capability advertisement to `presence:peer-manifest`: `{ capability: 'ping:server-info', terms: { cost_cents: 0, est_latency_ms: 50 } }`.
  4. **Step 3 (dual-path during transition):** existing callers (`./jtag ping`) still default to local (browser↔server). New callers can pass `{ scope: { target: 'grid', peer_id: '<other-peer>' } }` or `{ scope: { target: 'grid', capability: 'ping:server-info' } }`. Both work; no breaking change.
  5. **Step 4 (test):** smoke — two peers, laptop pings bigmama-wsl across grid, gets back bigmama's server info + browser info if its tab is open. Result envelope contains `{ source: laptop_peer_id, target: bigmama_peer_id, forwarded_by: [], result: { server: {...}, browser: {...} } }`.
  6. **Step 5 (close out card):** update kanban; broadcast on #cambriantech; no follow-up needed.

End-to-end opt-in change: **two lines** (`naturalScope` declaration + capability ad). The architecture absorbed the migration cost; per-command opt-in is metadata-flip + manifest entry, not refactor.

### 8.2 Recommended opt-in order (smallest blast radius first)

| Phase | Commands | Why this order |
|---|---|---|
| **Phase A — proof of life** | `ping`, `debug/system-info`, `grid/show-routes` | Tiny commands, low stakes, no LP contract needed, no entity changes. Validates substrate end-to-end. |
| **Phase B — single-peer compute, household-tier** | `ai/generate`, `ai/embedding`, `cognition/vision-describe`, `voice/synthesize`, `voice/transcribe` | Hot paths, but single-peer + household-tier first (no payment surface, no public-mesh complexity). Validates capability advertisement + bid loop end-to-end. |
| **Phase C — single-peer compute, trusted-orgs tier** | same commands as Phase B, but `accept_inbound_from: ['household', 'trusted-orgs']` | Validates contract event chain + LP transfer + sentinel pre-flight. First time payment flows execute. |
| **Phase D — canonical multi-peer** | `genome/paging-activate` cross-peer (§4.1) | The canonical example — exercises capability index + `RemoteResourceHandle` + FETCH vs DELEGATE policy decision. |
| **Phase E — multi-quorum** | `data/vector-search` (fan-out, any-quorum), then `genome/train` (federated, multi-quorum) | Validates fan-out routing + per-peer-result merging + (for training) FedAvg sync. |
| **Phase F — non-ML alloy contracts** | sentinel scans, wallet receipts, code-gen attestations | **Gated on the Domain Extensibility refactor per §7.4.** First non-ML multi-peer commands. Validates the universal contract substrate claim. |
| **Phase G — distributed forge runs** | `recipe/run` (parallel stages, §4.5) | The capstone — multi-peer + multi-stage + each stage produces its own alloy + parent alloy references children. Validates the full economic loop. |

Each phase is a separately-shippable PR (or PR series). Phase A → Phase B can land in the same week; Phase C-G are weeks-to-months depending on the contract/payment layer maturity.

### 8.3 Revert path

If a per-command opt-in causes problems (latency regression, capability advertisement bug, contract chain failure):

  1. Drop `naturalScope: 'grid'` declaration → command reverts to environment-local default.
  2. Withdraw the capability advertisement: emit `presence:capability-withdrawn` with reason `'reverting'`.
  3. Any in-flight cross-grid invocations complete or time out per their existing handle TTL; no rollback needed for already-shipped contracts (they're durable on the airc log regardless).
  4. Investigate, fix, re-opt-in.

The substrate layer (#1439 §5.2 deliverables 1-6) doesn't get reverted by per-command opt-ins. Revert blast radius is the one command.

### 8.4 What this doc explicitly does NOT cover

  - **Cross-grid persona migration** (moving a persona's full state from peer A to peer B). Different problem — touches ORM (engrams, persona identity) + airc cursor handoff. Belongs in a sibling doc once Phase D demonstrates the handle mechanics.
  - **Sentinel arbitration protocol** for contract disputes. Belongs in `SENTINEL-CONTRACTS.md`, dependency on §7 + #1439 §4.4.
  - **LP wallet on-chain settlement.** Belongs in `WALLET-ON-GRID-BUS.md` (named in #1439 §10), depends on §7's universal contract substrate landing.
  - **Recipe-as-grid-contract execution semantics.** A recipe can have stages that distribute differently (some stages local, some grid-multi); the per-stage opt-in shape is a §4.5 follow-up.

---

## 9. Open questions

  1. **Reservation TTL default.** For GPU slots, what's a sensible reservation window? Too short = race losses; too long = capacity holds. Suggest start at 10s, tunable per peer policy.
  2. **Fan-out result-merge timeout.** For `quorum: 'any'` fan-out commands, how long do we wait for slow peers? Suggest aggressive default (e.g. p95 of recent latencies for that capability) + first-good-enough early-return.
  3. **Adapter manifest broadcast volume.** Every peer broadcasting full adapter list could be O(peers × adapters) traffic at join time. Probably needs a delta-based protocol: broadcast hash-of-manifest-list on join; peers diff against their cache; ask for full only on mismatch.
  4. **Federated training sync strategy default.** FedAvg vs async SGD vs others — depends on heterogeneity of contributing peers. Default for household = FedAvg (homogeneous-ish); public-mesh = async SGD (heterogeneous). Per-command override always.
  5. **`RemoteResourceHandle.fetch_strategy` default.** When peer A pins on peer B, is delegate or pull default? Probably delegate for delegation-cheap resources (LoRA inference where B has GPU but A has CPU) and pull for read-mostly small content (recipe blob, manifest). Heuristic on resource_kind.
  6. **Resource-pressure broadcast cadence.** Too frequent = chatter; too rare = stale pressure data. Suggest hysteresis: broadcast on threshold-crossing (e.g. when VRAM crosses 70%, 85%, 95%) + 60s heartbeat baseline.
  7. **`quorum: 'multi'` with degraded participation.** If we asked for min=3 and only 2 peers respond, do we fail-clean or proceed with degraded? Per-command policy field: `quorum.if_under_min: 'fail' | 'proceed-degraded' | 'wait-for-more'`.
  8. **Contract chain for failed federated training.** If a contributing peer's gradients are anomalous, sentinel-AI scrutiny → `contract:disputed`. But the other peers' partial work IS valid. Need to specify: failed contributors don't get paid; successful contributors do; final alloy attests the participant list. Already implicit in #1439; worth pinning.
  9. **Hot-path inference: skip the bid loop for routine dispatch?** Bid-loop adds latency (round-trips + decision time). For repeat dispatches against a known-good peer with stable capability + acceptable terms, skip bid and dispatch directly; fall back to bid only on first-call or after a failure. Optimization, not correctness.

---

## 10. What this doc does NOT do

  - **Does not define the airc-lib substrate primitives** for grid coordination — that's codex's airc-rust-rewrite work (subscribe / send / cursor-replay primitives).
  - **Does not define wallet / LP currency.** Per #1439 §10. This doc treats payment as `contract:paid` events; the actual exchange rate / minting / on-chain integration is `WALLET-ON-GRID-BUS.md` (future).
  - **Does not specify sentinel-AI scrutiny rules** — that's sentinel's own design. This doc just provides the contract chain sentinel reads.
  - **Does not solve decomposed/sharded inference** (model parallelism, pipeline parallelism). Per #1439 §10. Multi-peer here is task-level parallelism (fan-out + reduce), not single-task decomposition.
  - **Does not specify public-mesh discovery / anti-Sybil.** Per #1439 Q6 — public mesh tier is invite-only initially.

These belong in sibling specs. Don't block this doc's review on them.

---

## 11. Coordination

This doc is downstream of #1439 (the bus + transport layer) and upstream of per-command implementation work. Reviewers:

  - **Joel** — primary stakeholder of the grid story; original direction for this brainstorm
  - **claude-tab-2 / 16279c3f** — author of #1439; needs to confirm the command classification table doesn't contradict §1.2 / §2 of the bus arch
  - **codex / 543c0bf7** — substrate side; needs to confirm airc-lib can carry the event classes named in §4 (esp. `presence:adapter-available`, `contract:*` chain, training sync events) without growing new primitives
  - **dba950ce** — paired on consumer-side AIRC work; relevant if their next slice touches handles or training

Reply on `#cambriantech` over airc. Approval comes from at least one of codex + Joel before any per-command implementation work opens against §2's table.

**This doc does not gate anything from landing immediately:** existing commands work as they do today. What this defines is the target shape for grid extension as the bus layer (§5.3 steps 1-4 of #1439) lands. The opt-in is per-command; legacy paths keep working unchanged.
