# Model Endpoint Fabric — The Self-Healing Adapter Router For All Model Needs

**Status.** Canonical design for how the organism reaches *every* model capability — text, embedding, vision, audio, training — across *many* local and grid endpoints (continuum llama-server, unsloth, grid peers, cloud), through ONE adapter interface mediated by ONE stateful router. Crystallized 2026-06-24 with Joel: *"using and writing quality adapters, self healing and even negotiating across many grid unsloth/continuum endpoints to satisfy the needs of the system; will take stateful intelligent systems akin to advanced routers… develop an elegant adapter based architecture we can rely on for all our model needs. This is everything."*

This doc is the connective tissue **above** the local scheduler and **under** the dumb command surface. It is overwhelmingly *composition of primitives that already exist in tree* — its job is to name the layer, fix the invariants, and stop the recurring slop (reinventing the registry, owning model bytes, bolting policy onto commands, silent fallbacks).

**Read-first companions (this doc defers to them on their turf):**
- [AI-COMMAND-NAMESPACE.md](AI-COMMAND-NAMESPACE.md) — the dumb `ai/*` command surface. **Commands carry no policy knobs.** The fabric is the smart daemon those dumb commands route into.
- [INFERENCE-SCHEDULING-AND-SCARCITY.md](INFERENCE-SCHEDULING-AND-SCARCITY.md) — slot scheduling *inside one endpoint* (continuous batching, LoRA paging, adaptive quant). The fabric picks **which endpoint**; the scheduler runs the slots **within** the chosen one.
- [INFERENCE-LANES-REALISTIC.md](INFERENCE-LANES-REALISTIC.md) — the local lane/lease MVP. The fabric treats the local `InferenceCoordinator` as **one endpoint among many.**
- [GRID-ADDRESSING-AND-ROUTING.md](GRID-ADDRESSING-AND-ROUTING.md) — URI grammar, transport selection, the per-URI auth gate, `HandleRef` across the wire. The fabric uses these verbatim to reach remote endpoints; it does not invent a second addressing scheme.
- [ADAPTER-SYSTEM-ARCHITECTURE.md](ADAPTER-SYSTEM-ARCHITECTURE.md) — the *core↔client* boundary doctrine. Model adapters are a **specialization** of that same "one interface, many swappable impls, run anywhere" principle, pointed at model backends instead of language SDKs.
- [CONCURRENCY-STYLE-GUIDE.md](CONCURRENCY-STYLE-GUIDE.md) — the RTOS shape the fabric's health/discovery loop MUST conform to (own task + `interval` + `watch<Snapshot>` + atomic gate + `spawn_blocking` + quarantine).

---

## 1. The thesis in one paragraph

A model **need** (generate this text / embed this content / describe this image, for this persona, at this latency class, under this trust context) is a packet. An **endpoint** (a place that can serve some needs — local llama-server, local unsloth, a grid peer, a cloud provider) is a link, reached through an `AIProviderAdapter`. The **fabric** is the router: it holds the live table of endpoints with their state (health, latency, capacity, warm models, capabilities, trust), and for each need it **matches → scores → routes → heals**. The **custodian** (unsloth) owns the bytes underneath every local endpoint; the fabric never copies model bytes — it holds **handles by id** and asks the custodian to make a model or LoRA warm. This is an *advanced router*: stateful, self-describing endpoints; a control plane that probes and quarantines; routing decisions on capability + fit + cost + trust; and self-healing that reroutes around failure **to an equivalent-quality endpoint serving the same model** — never by silently degrading the answer.

```
            ai/inference/{open,generate,close}   ai/generate   ai/embedding/*   vision/*   (DUMB commands — no policy knobs)
                                   │
                                   ▼
          ┌───────────────────────────────────────────────────────────┐
          │                   MODEL ENDPOINT FABRIC                     │   ← this doc
          │  endpoint table (watch<FabricSnapshot>) · match→score→route │
          │  health probe loop (RTOS shape) · self-heal · capability    │
          │  negotiation · trust gate · custodian calls (handles by id) │
          └───────────────────────────────────────────────────────────┘
             │ local                 │ local                 │ remote                │ cloud
             ▼                       ▼                       ▼                       ▼
     ┌───────────────┐      ┌───────────────┐      ┌────────────────────┐   ┌──────────────┐
     │ llama-server  │      │ unsloth /v1   │      │ AircRemoteInference │   │ OpenAI/Anthr │
     │ endpoint      │      │ endpoint      │      │ (grid peer, TEXT)   │   │ endpoint     │
     │ (Coordinator: │      │ (custodian-   │      │ compute-lease only  │   │              │
     │  lanes/slots) │      │  served)      │      └────────────────────┘   └──────────────┘
     └──────┬────────┘      └──────┬────────┘
            └──────────────────────┘
                     │
                     ▼
        ┌─────────────────────────────┐
        │  CUSTODIAN (unsloth)         │  owns: catalog · storage (~/.unsloth) · downloads ·
        │  bytes live here, not here ──┼─ train · fuse/quant/GGUF · serve · share. Continuum
        │  in continuum               │  holds HANDLES by id; never copies bytes.
        └─────────────────────────────┘
```

---

## 2. The domain cut: custodian vs organism (non-negotiable)

Joel, 2026-06-24: *"unsloth is the custodian of our models… It manages the model catalog and the storage and probably sharing of some model info. The downloads for sure. You gotta work with it. See what domain separates concerns."*

| Concern | Owner | Why |
|---|---|---|
| Model catalog, storage of bytes (`~/.unsloth`), downloads (HF snapshot) | **Custodian (unsloth)** | One authority for model bytes. Bytes are heavy, shared, content-addressed. |
| LoRA **training**, fuse, quant, GGUF convert | **Custodian** | These are byte-ops on model weights. Apple → `mlx_lm.lora`/`mlx_lm.fuse`; NVIDIA → unsloth. |
| Serving (`/v1`, `/v1/embeddings`, load/unload, `/lora-adapters`) | **Custodian** | The thing that holds weights in VRAM serves them. |
| Persona / memory / tools / grounding | **Organism (continuum)** | The being. |
| Genome-loop **policy** (when to train, what to measure, adopt-or-reject) | **Organism** | `forge/decide` is pure logic — a cognition decision, not a byte-op. |
| Dataset production (turns → ShareGPT JSONL) | **Organism** | "The work is the data." Produces *training data*, not model bytes. |
| **Choosing which endpoint serves a need + healing around failure** | **Organism (this fabric)** | Routing is cognition over live endpoint state. |

**Continuum holds handles, not bytes.** A trained LoRA is referenced by `(name, path-or-id, scale)` — a handle. The custodian registers it with the server and owns the bytes. The fabric routes a generation that *references* the handle; the byte resolution happens custodian-side.

**The standing trespass to repair** (`modules/forge.rs`): it custodies bytes under `~/.continuum/forge/{lora,export}` (forge.rs:211), shells `mlx_lm.fuse` (forge.rs:692–826), and runs llama.cpp GGUF convert/quantize itself. That is custodian work living in the organism. It is the bug class behind GGUF-gibberish and the adapter no-op (see `[[mlx-serving-gguf-breaks-hybrid-adapter-path-noop]]`). The fix: `forge/train` and `forge/export` become **custodian calls** that return a handle; the bytes land under `~/.unsloth`. `forge/decide` stays in the organism (it's policy). Tracked by task #32.

---

## 3. What already exists (the build-ON ledger — do NOT reinvent)

This is the most important section. The fabric is ~80% composition. Reinventing any of these is the slop this doc exists to prevent.

| You need… | It already exists | Where |
|---|---|---|
| One model-backend interface | `AIProviderAdapter` trait (text, embeddings, vision, health, models, **lora**) | `ai/adapter.rs:299` |
| Self-describing endpoint capabilities | `AdapterCapabilities` (modalities, tool protocol, ctx window, lora caps) | `ai/adapter.rs` |
| Endpoint health signal | `health_check() -> HealthStatus` | `ai/adapter.rs` (defined; **selector ignores it today**) |
| LoRA hooks on the adapter | `lora_capabilities()`, `apply_lora()`, `remove_lora()`, `list_lora_adapters()` | `ai/adapter.rs` (default no-op) |
| Adapter table + device-aware selection + hot-swap | `AdapterRegistry` (`register`/`deregister`/`select`) | `ai/adapter.rs:605` |
| Concrete endpoints (the two outliers, §6) | `OpenAICompatibleAdapter` (llama-server/unsloth/DMR/cloud), `AircRemoteInferenceAdapter` (grid peer, TEXT-only), `AnthropicAdapter`, `HeuristicInferenceAdapter` (fixture) | `ai/openai_adapter.rs`, `inference/airc_remote/`, `ai/anthropic_adapter.rs`, `ai/heuristic_adapter.rs` |
| Local slot scheduling within an endpoint | `InferenceCoordinator` + `Lane` + `ThroughputLease` + `FootprintRegistry` + `AdaptiveThroughputPlanner` + `PressureBroker` | `inference/coordinator.rs`, `inference/lane.rs`, et al |
| The dumb command surface + handle lifecycle | `ai/inference/{open,generate,close,inspect}`, `OpenParams.active_adapters` | `inference/handle_module.rs:100` |
| Per-request adapter passthrough | `TextGenerationRequest.active_adapters` reaches `generate_text` | `ai/types.rs:259`, `inference/handle_store.rs:294` |
| Grid peer table + addressing | `NodeRegistry` (address-keyed), `GridRouter` (`RouteDecision::{Local,Remote}`) | `modules/grid/registry.rs`, `modules/grid/router.rs` |
| Per-peer trust gate (remote capped at Trusted, Blocked denied) | `GridTrustAuthPolicy` + `PeerTrustSource` | `routing/grid_trust_policy.rs` |
| Custodian control surface | `UnslothHttp` (`status`/`list_models`/`load_model`/`ensure_model_active`) | `inference/unsloth_control.rs` |
| Serving plan (which base, how many lanes, residency) | `plan_serving()` + `ServingDaemonModule` (5s tick) | `cognition/serving_plan.rs`, `modules/serving_daemon.rs` |
| Hardware policy (DVFS) | `SubstrateGovernor` | `runtime/substrate_governor.rs` |
| RTOS monitor template | `MemoryPressureMonitor` (own task + interval + watch + quarantine) | `system_resources/memory_pressure.rs` |

**The fabric is the layer that fuses these.** Today they are islands: `AdapterRegistry.select()` is static and ignores `health_check()`; `GridRouter` routes *commands* but isn't wired to *adapter selection*; `unsloth_control` is called ad hoc, not as a custodian seam behind a routed local endpoint; capability advertisement across peers does not exist. The fabric is the supervisor that makes them one organism.

---

## 4. Invariants (the fabric must hold these or it is wrong)

1. **Dumb commands, smart fabric.** No `max_latency_ms`, `prefer_local`, `min_quality_tier` on any command. The need is derived from metadata that *already* rides the request — `persona_id`, `task`/`LaneClass`, `purpose`, `request_id`, caller trust. The fabric interprets; the command carries nothing to negotiate. (AI-COMMAND-NAMESPACE.md.)

2. **Compute-lease boundary: remote does TEXT only.** A grid peer leases *token generation*. The brain, tool execution, vision/audio bridging, and memory stay local. `AircRemoteInferenceAdapter` declares `modalities = text-only` for exactly this reason. (`[[compute-lease-boundary]]`.)

3. **Fail-loud, no fallback — but reroute-to-same-model is healing, not fallback.** This is the subtle, load-bearing distinction:
   - **Healing (allowed):** local llama-server endpoint dies mid-need → reroute the *same model, same quality* to a grid peer that serves it, or retry after the custodian reloads it. The answer is unchanged; only the *where* changed. The video-player analogy from INFERENCE-SCHEDULING-AND-SCARCITY (drop 4K→1080p without the app noticing) applies **only to dimensions the caller already declared don't-care** (which machine, batch window) — never to model identity or answer quality.
   - **Fallback (forbidden):** no endpoint can serve the requested model → **do NOT** silently substitute a lesser model, a heuristic stand-in, or a degraded answer. **Fail loud, name the cause** ("no endpoint serves `qwen3-coder`; nearest is peer X which is Blocked, peer Y which is offline"). (`[[fallbacks-are-illegal-fail-loud]]`.)
   - The test: *did the answer's fidelity change?* If no → healing. If yes → fallback → illegal.

4. **Trust gate is hard.** Remote callers capped at `Trusted`, never `Owner`; `Blocked` denied. Owner-gated ops stay local. The fabric routes through `GridTrustAuthPolicy`; it does not re-implement trust. (GRID-ADDRESSING-AND-ROUTING.md §RBAC.)

5. **Custodian owns bytes; fabric holds handles by id.** The fabric never writes model weights, never runs fuse/quant. It calls the custodian to make something warm and routes generation that references it. (§2.)

6. **RTOS concurrency.** The endpoint health/discovery loop is one owned task with its own `interval`, publishing a `watch<FabricSnapshot>`, reading an atomic gate on the hot path, probing endpoints via `spawn_blocking` with a 100ms timeout, quarantining an endpoint after 3 consecutive probe failures. No synchronous probe on the routing hot path; no `Arc<Mutex>` across await; no env-tuned thresholds. (CONCURRENCY-STYLE-GUIDE.md.)

7. **Observability is first-class.** Every routing decision emits a capture event (why this endpoint? what were the candidates and scores? what healed?) through the existing `CaptureSink` pattern, Noop by default at zero hot-path cost, replayable. A route choice you can't replay is a guess. (OBSERVABILITY-AS-SUBSTRATE.md.)

8. **Capability is DISCOVERED, not declared — the organism figures itself out.** The fabric never carries a hand-maintained "provider X supports LoRA / vision / N context, provider Y does not" table. Each endpoint *describes itself* and the fabric *stays in sync with it via its API* — **the adapter call is the sync point**. Concretely: whether an endpoint can page a LoRA is learned by *asking it* (`GET /lora-adapters` → 200 = supported + the name→id catalog; 404 = unsupported, cached), exactly as warm models are learned from `/v1/models` (`runtime_models`). The instant the endpoint changes (custodian loads a new adapter, a model is pulled), the next probe reflects it — no code edit, no human picking. A hardcoded capability list is the anti-pattern this invariant forbids; it drifts from what the endpoint actually serves the moment either side changes. (Joel, 2026-06-24: *"the organism figures itself out, not some human picking. It's self organized"* / *"we stay in sync via APIs, and therefore the adapter call."*)

---

## 5. The Endpoint abstraction

An **Endpoint** is an `AIProviderAdapter` plus the *live state the router needs to route well*. The adapter already self-describes its static capabilities; the fabric wraps it with dynamic state it maintains:

```
Endpoint {
    adapter: Arc<dyn AIProviderAdapter>,   // the reach mechanism (existing)
    kind:    EndpointKind,                  // LocalServed | LocalCoordinator | GridPeer | Cloud
    trust:   TrustLevel,                    // from GridTrustAuthPolicy; Local = Owner
    // --- dynamic state, maintained by the health/discovery loop ---
    health:        HealthStatus,            // from adapter.health_check(), polled
    latency_ewma:  Millis,                  // observed, decayed
    capacity:      CapacitySnapshot,        // free slots / lease headroom (local: from Coordinator)
    warm_models:   Set<ModelId>,            // what this endpoint can serve NOW without a load
    capabilities:  AdapterCapabilities,     // modalities, ctx window, lora caps, tool protocol
    quarantined:   bool,                    // 3 consecutive probe failures → out of rotation
}
```

**Key discipline:** none of this is new *invention* — `health_check()`, `AdapterCapabilities`, and the coordinator's lane/lease accounting all exist. The Endpoint is the *struct that finally consumes them together*.

`EndpointKind` distinguishes the four routing realities:
- **`LocalServed`** — a custodian-served `/v1` endpoint reached via `OpenAICompatibleAdapter` (llama-server, unsloth). The fabric asks the custodian (`unsloth_control`) to ensure the model/LoRA is warm before routing.
- **`LocalCoordinator`** — the in-process `InferenceCoordinator` with lanes/leases (the realistic local MVP). Capacity = lease headroom.
- **`GridPeer`** — `AircRemoteInferenceAdapter` over `AircLiveTransport`. TEXT-only. Trust-gated. Capacity/warm-models learned via negotiation (§7).
- **`Cloud`** — `OpenAICompatibleAdapter`/`AnthropicAdapter`. Effectively infinite capacity, metered cost, never local-secret-bearing.

---

## 6. Outlier validation — the interface is already proven

The methodical process (CLAUDE.md) says: prove an interface against the two *most different* implementations, not exhaustively. For model endpoints, the two outliers **already ship**:

- **Outlier A (maximally local):** in-process / `LocalServed` llama-server — same machine, GPU-direct, single-slot, custodian holds the bytes.
- **Outlier B (maximally remote):** `AircRemoteInferenceAdapter` — a grid peer reached over airc, TEXT-only, trust-gated, model chosen by the peer, transport-abstracted (`AircLiveTransport`/`LocalAdapterTransport`/`StubInferenceTransport`).

Both implement the *same* `AIProviderAdapter` trait and return the *same* `TextGenerationResponse`. The transparency rule already holds: `adapter.generate_text(request).await?` is identical at both extremes. **The endpoint interface is therefore validated; the fabric is the router over already-proven endpoints**, not a new abstraction needing its own outlier pass. New endpoint kinds (a new cloud provider, a TPU box) slot in as new adapters — the rails are laid.

---

## 7. Capability negotiation across the grid (the genuinely-new part)

Today each endpoint knows only its *own* warm models (DMR's `runtime_models`, unsloth's `/v1/models`). There is no cross-peer picture. The fabric needs one, and it must be event-driven, trust-gated, and cheap.

**Mechanism (compose existing pieces, don't build a new gossip stack):**
- Each node periodically publishes an **endpoint advertisement** over airc — its serveable surface: `{ warm_models, capabilities (modalities/ctx/lora), capacity headroom, trust-origin }`. This is the airc analog of `cognition/personas` roster, for model endpoints. It rides the existing airc event substrate; no new transport.
- A receiving fabric **merges remote endpoints into its table**, trust-gated: a peer's advertisement is admitted only up to `Trusted` (GridTrustAuthPolicy), `Blocked` peers dropped, unknown → `Provisional` until the airc↔grid trust bridge (task #38) resolves real trust.
- Remote endpoints are admitted **TEXT-only** (compute-lease boundary). A peer advertising vision does not make the local fabric route vision over the wire — that violates invariant #2.
- Discovery is the same RTOS loop as health: advertisements update the `watch<FabricSnapshot>`; staleness (no advertisement in N intervals) quarantines the remote endpoint exactly like a failed probe.

**Open questions (carried, do not pretend they're solved):** advertisement cadence vs. churn, capacity-headroom honesty across a trust boundary, and warm-model freshness windows. These belong in [AI-LANE-OPEN-QUESTIONS.md](../planning/AI-LANE-OPEN-QUESTIONS.md); the fabric ships single-node first and lights grid negotiation behind the trust bridge.

---

## 8. The routing decision: match → score → route → heal

**The catalog is a capability search, not a name lookup** (Joel, 2026-06-24: *"our model catalog is like a search for a camera resolution, frame rate, features"*). A buyer doesn't ask for a camera by SKU; they search the spec space — resolution, frame rate, sensor, features — and pick the best one that fits the budget. A `ModelNeed` is the same query over the model spec space: **modality** (text/vision/audio/embedding) ≈ what it can capture, **context window** ≈ resolution, **latency/throughput class** ≈ frame rate, **quality/params + available LoRA features + tool protocol** ≈ the feature list, all bounded by **hardware fit** ≈ budget. The custodian's `/v1/models` ∪ the trust-scoped genome market (`[[ask-anything-assemble-best-self-or-train]]`, `[[model-fit-is-the-priority-single-machine-first]]`) is that searchable catalog; selecting a model = ranking candidates by those dimensions and taking the best that fits. Requesting a model by exact name is just the degenerate one-row query.

A `ModelNeed` (derived from request metadata — never from command knobs) flows:

1. **Match (the camera search).** Query the endpoint × catalog space for entries that *can* serve the need's spec: required modality ∈ capabilities, context window ≥ need, model present in warm_models (or custodian/market can warm it), required LoRA/tool features available, trust sufficient, fits local hardware, not quarantined. **Empty match set → fail loud** (invariant #3), naming the nearest misses ("closest is `qwen3-coder` but ctx 32k < 64k needed"). Capabilities come from invariant #8 discovery, never a static spec sheet.
2. **Score.** Among matches, rank by fit: local before remote before cloud (latency + the compute-lease preference to keep work local); capacity headroom; observed latency EWMA; for a LoRA need, endpoints with the adapter already registered score above those needing a load. Scoring is *policy in the daemon*, not knobs on the wire.
3. **Route.** For a `LocalServed`/`LocalCoordinator` choice: ensure the model/LoRA is warm via the custodian, open/reuse the handle through `ai/inference/open` (lanes/leases apply), generate. For a `GridPeer`: dispatch via `AircRemoteInferenceAdapter`. The choice is invisible to the caller.
4. **Heal.** On endpoint failure mid-need: mark the endpoint degraded, **reroute the same model/quality** to the next-best match, retry with bounded backoff, circuit-break a peer that fails repeatedly (quarantine). If healing exhausts the match set without a same-quality option → fail loud (never downgrade the answer).

**Concurrency note Joel pinned (2026-06-24):** *"If persona cognition layers are written correctly they should run in parallel to inference… the total latency shouldn't change much… not fifo/waiting."* The fabric adds **zero serial stage** to a turn: endpoint state is read lock-free from the `watch` snapshot / atomic gate (no probe on the hot path); cognition faculties overlap generation and reconcile via turn-history. Routing is a snapshot lookup, not a negotiation round-trip.

---

## 9. Worked example — the LoRA page-in (slice 1, the proof of mechanism)

This is the smallest end-to-end slice that exercises every layer of the fabric, and it's the load-bearing increment for the genome loop (#32/#35). It is *located and exact*:

**The gap (was).** `TextGenerationRequest.active_adapters` (ai/types.rs:259) carries `Vec<ActiveAdapterRequest{ name, path, domain, scale }>`. It flows through `handle_store.rs:294` and reaches `OpenAICompatibleAdapter::generate_text`. But the body builder **never read it** — the LoRA was dropped at the last inch. The page-in was a no-op; this is why a "live" adapter measured as LIFT=0 in the no-op A/B.

**Why it's the custodian boundary in miniature.** llama-server's per-request field is `"lora":[{"id":N,"scale":S}]`, where `id` is the **integer load-index** the *server* assigns — visible via `GET /lora-adapters`. That id exists only after the **custodian** registers the adapter (server launch `--lora`, or a registration call). So the wire has two halves, on the right sides of the boundary:
- **Custodian half** (not yet built; needs the llama.cpp `llama-server` serving path stood up — `[[llama-server-serves-v1-direct-python-gateway-optional]]`): register the safetensors LoRA with the serving endpoint; it gets an integer id. (Belongs to unsloth/serving, not forge.rs writing bytes.)
- **Fabric/adapter half** (✅ landed, openai_adapter.rs): per invariant #8 the adapter **discovers** the capability by *asking the endpoint* — `probe_lora_catalog()` calls `GET /lora-adapters` and caches `LoraSupport::{Supported(catalog)|Unsupported}` (200 vs 404), exactly as `runtime_models` is learned from `/v1/models`. It then resolves `ActiveAdapterRequest.name` → server id via the pure `match_lora_index()` and injects `"lora":[{id,scale}]` — *exactly* the shape of the existing `repeat_penalty` extension. No provider-id allow-list: a backend with no `/lora-adapters` reports `Unsupported` and a page-in request **fails loud** (`lora_miss_error`), never a silent drop. `lora_capabilities()` reports the discovered truth so the future health-aware `select()` reads it.

**Acceptance:** with a real trained LoRA registered (custodian half), an `ai/inference/open { active_adapters:[…], persona_id }` then `generate` produces output that *differs from base* (adapter genuinely live), measured through `cognition/eval` (the full organism — memory/tools/grounding), and `forge/decide` adopts-or-rejects on the lift. No bash A/B against a bare server — that measures the wrong subject (`[[asha-coder-baseline-and-spawn-race]]`). The pure name→id matcher + miss paths are unit-tested (`mod lora_page_in`); the probe/HTTP path is exercised by the organism eval, not a unit test (RULE 1).

---

## 10. Build slices (each validated through the organism, not a side-channel)

1. **Page-in wire (proof of mechanism).** ✅ *Adapter half landed* (openai_adapter.rs): `probe_lora_catalog` discovers capability via `GET /lora-adapters` (invariant #8), `match_lora_index` resolves name→server-id, `"lora":[{id,scale}]` injected into the body, fail-loud on unsupported/unregistered, `lora_capabilities()` reports discovered truth, unit-tested. *Remaining:* custodian half — stand up the llama.cpp `llama-server` serving path and register the LoRA so it gets an id (`[[llama-server-serves-v1-direct-python-gateway-optional]]`); then validate output differs from base via `cognition/eval`. (Unblocks #32/#35.)
2. **Endpoint struct + health loop.** Wrap `AdapterRegistry` entries as `Endpoint`s; add the RTOS health/discovery loop publishing `watch<FabricSnapshot>`; `select()` becomes health-aware (skip quarantined). Outliers already proven (§6).
3. **Custodian boundary repair.** Move forge.rs byte-custody (paths/fuse/quant/GGUF) behind custodian calls returning handles; bytes land under `~/.unsloth`; `forge/decide` stays organism-side. (Task #32 trespass.)
4. **Match→score→route→heal** over the local endpoints (llama-server + coordinator). Self-healing reroute *within local* first; capture every decision.
5. **Grid negotiation.** Endpoint advertisement over airc + trust-gated merge (§7), behind the airc↔grid trust bridge (#38). Remote = TEXT-only. RouteSelector picks local-vs-peer (composes `GridRouter`).
6. **Modality breadth.** Bring embedding (already adapter-routed via `/v1/embeddings`), then vision, then audio into the same match→route path so "all model needs" is literal, not text-only.

---

## 11. Forbidden moves (the amnesia guardrails)

The model keeps reflex-coding these under amnesia. Each is a violation of a section above.

- ❌ **A second adapter registry / parallel selector.** There is ONE `AdapterRegistry` (ai/adapter.rs:605). Extend it; never write `MockAdapterRouter` or a parallel `EndpointManager` beside it.
- ❌ **Owning model bytes in continuum.** No writing weights, no `mlx_lm.fuse`, no GGUF convert in the organism. That's custodian work (§2). forge.rs doing it is the trespass to *remove*, not to copy.
- ❌ **Policy knobs on commands.** No `prefer_local`/`max_latency_ms` on `ai/*`. Derive the need from existing metadata (§4.1).
- ❌ **Silent fallback to a lesser model/heuristic.** Reroute same-model = healing; substitute a worse answer = forbidden. Fail loud and name the cause (§4.3).
- ❌ **Routing vision/audio over the wire.** Remote leases TEXT only (§4.2). A peer advertising vision does not change that.
- ❌ **Synchronous endpoint probe on the routing hot path.** Read the `watch` snapshot / atomic gate; the health loop is the only thing that probes (§4.6).
- ❌ **Re-implementing trust.** Route through `GridTrustAuthPolicy`; remote capped at Trusted, Blocked denied (§4.4).
- ❌ **A bash A/B against a bare server to "measure the adapter."** That bypasses the organism — wrong subject. Measure through `cognition/eval` / the full persona (§9 acceptance).
- ❌ **Inventing a new addressing/transport scheme for remote endpoints.** Use the URI grammar + transport selection in GRID-ADDRESSING-AND-ROUTING.md.

---

**The cost of skipping this doc** is the model rebuilding the registry as a parallel router, re-owning model bytes in forge.rs, bolting latency knobs onto the dumb commands, and "healing" by silently swapping in a worse model — i.e. turning an advanced router back into a pile of ad-hoc `if endpoint_down { use_other }` branches with no state, no trust, and no truth about what it answered with. Don't.
