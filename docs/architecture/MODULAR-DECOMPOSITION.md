# Modular Decomposition — Composable Service Profiles ("a good AWS template")

**Status:** Design (worked out); implementation in slices. Consolidates the
live findings #25 (decompose the monolith), #26 (faculties degrade-not-panic),
#27 (intra-machine addressing).
**Parent docs:** [MODULE-ARCHITECTURE](MODULE-ARCHITECTURE.md) ·
[CBAR-SUBSTRATE-ARCHITECTURE](CBAR-SUBSTRATE-ARCHITECTURE.md) ·
[CONCURRENCY-STYLE-GUIDE](CONCURRENCY-STYLE-GUIDE.md) ·
[ORGANISM-OS-ARCHITECTURE](ORGANISM-OS-ARCHITECTURE.md) ·
[UNSLOTH-INTEGRATION](UNSLOTH-INTEGRATION.md) · [ADAPTER-SYSTEM-ARCHITECTURE](ADAPTER-SYSTEM-ARCHITECTURE.md)

---

## 1. Principle — refine, don't rewrite

The monolith is the *opposite* of our adapter-based dynamic design. But the
modular machinery already exists; the monolith is only that **one process
registers all ~40 modules**. Decomposition reuses every primitive:

- **`ServiceModule`** — the one trait every module implements (own task/tick/
  health/commands). Unchanged.
- **`Commands.execute` / `Events`** — the two universal primitives. Unchanged.
- **`route_command` + the airc command protocol** — already route a command to
  whatever process hosts the handling module (local walk, or over the bus to a
  peer). This is what makes "module not in my process" just work.
- **`BootMode` + `ModuleCategory` + `required_modules(discovery, mode)`** — the
  composition seam. A process boots a *profile* (a set of categories); modules
  outside it are reached over the bus.

So this is **a good AWS template**: declare which service-groups a node runs;
each is independently health-checked, restartable, and testable; they compose
over the bus. We refine the category taxonomy and add profiles — we do not
rewrite the modules.

## 2. Today's seam (and why it's coarse)

`runtime.rs` already has `MODULES: &[(&str, ModuleCategory)]` with two
categories: `Core` (≈38 modules — everything) and `PersonaHosting` (2).
`required_modules` filters by `(AircDiscovery, BootMode)`; `BootMode` is
`FullCitizen | InferenceOnly | FailFast`. The mechanism is real — it's just that
`Core` lumps inference + forge + live + grid + data + cognition into one blob,
so the smallest bootable process is still ~the whole substrate (live finding:
serving `mcp/list-tools` booted ONNX/Bevy/persona/serving). The fix is a finer
taxonomy.

## 3. The service taxonomy (refined `ModuleCategory`)

Group modules by *concern* — each group is a candidate process/container:

| Service group | Modules (from `MODULES`) | Notes |
|---|---|---|
| **RuntimeShell** | health, auth, system, events, logger, runtime, mcp, data | Every node. The minimal addressable substrate — commands/events/data/health. A node is *nothing* without this. |
| **ResourceGov** | gpu, resource-broker, pressure-broker | Per-node hardware governance; co-resident with whatever it governs. |
| **Inference** | inference, inference-coordinator, ai-inference-handle, inference-llm, ai_provider, embedding, search, tool-parsing, vision, models | The engine. Increasingly **external (unsloth)** behind the `AIProviderAdapter` (see UNSLOTH-INTEGRATION); the in-tree modules become a fallback/local-lane. |
| **Cognition** | cognition, channel, persona_allocator, agent, memory, rag, persona_instance_manager, persona-rag-inspect | The organism's brain. Per-persona hosts (the Docker-citizen model). |
| **Forge** | forge, sentinel, plasticity, dataset, vdd, cargo, code | Training / dev / self-improvement. Bursty, GPU-heavy, schedulable separately. |
| **GridTransport** | airc, grid | The bus itself. |
| **Live** | live, avatar | **Bevy + LiveKit + GPU — CO-LOCATED (see §4).** |

These become finer `ModuleCategory` variants; `required_modules` filters by the
profile's category set. `FullCitizen` = all groups (current behavior preserved —
nothing breaks); new slimmer profiles host subsets.

## 4. Co-location constraints (placement groups)

Not every group is independently containerizable — some MUST share a VM, exactly
like an AWS placement group / k8s pod affinity. The hard one:

- **Live = Bevy (headless GPU render) + LiveKit (WebRTC SFU) + the GPU must be in
  ONE VM.** Avatar frames flow GPU readback (`Readback` → RGBA framebuffer) →
  encode → WebRTC video track. That's a **GPU-transfer / framebuffer** path; a
  process or network boundary between Bevy and LiveKit would force a frame copy
  off the GPU every frame (latency + bandwidth death). They co-locate, sharing
  the GPU. (`Cargo.toml`: bevy 0.18 headless + livekit both in-core, GPU
  readback → LiveKit tracks — this is why.)
- **ResourceGov co-resides with the hardware it governs** (the GPU node runs its
  own gpu/pressure modules).
- **Inference co-resides with its accelerator** — but since it's becoming
  external (unsloth's own GPU container), the boundary is the OpenAI-compatible
  `/v1` wire, not a continuum process split.

A profile declares its placement constraints; the container layout (§6) honors
them. Placement-flexible groups (RuntimeShell, Cognition, Forge) can land
anywhere and reach the rest over the bus.

## 5. Per-service health, restart, independent test

The `ServiceModule` trait already carries `config()` (name, priority, cadence)
and the runtime quarantines a module after 3 consecutive tick failures
(CONCURRENCY-STYLE-GUIDE). Decomposition surfaces this at the *service* boundary:

- **Health** — each profile exposes `health` for its hosted modules; a
  supervisor (or the container orchestrator) probes it. A degraded module marks
  itself unavailable (#26: faculties degrade, never panic) without taking the
  process down.
- **Restart** — a crashed/quarantined service restarts independently (supervisor
  / `restart: unless-stopped`), and rejoins the bus on boot. The grid heals
  (grid-node-resilience) because losing one service ≠ losing the node.
- **Independent test** — a profile boots in isolation with only its modules + a
  `MockTransport`/`InProcessTransport` for the rest, so each service is
  integration-tested without the monolith (the #22 pain: booting everything per
  test). This is the direct testability win.

## 6. Container layout mirrors the profiles

Containers follow the same grouping (your "containers can follow similar
layout"):

```
┌─ node: light citizen (no GPU) ────────────┐   ┌─ node: GPU engine ───────────┐
│  RuntimeShell + Cognition + GridTransport  │   │  unsloth (inference+train)   │
│  (+ continuum-mcp sidecar)                 │   │  OpenAI-compatible /v1        │
│  leases ai/generate over the grid ─────────┼──▶│  (its own AGPL app/container) │
└────────────────────────────────────────────┘   └───────────────────────────────┘
┌─ node: Live (GPU) — CO-LOCATED ───────────┐   ┌─ node: Forge (GPU, bursty) ──┐
│  Bevy render + LiveKit SFU + GPU           │   │  forge/sentinel/plasticity    │
│  (framebuffer transfer stays on-GPU)       │   │  training runs                │
└────────────────────────────────────────────┘   └───────────────────────────────┘
```

N light citizen nodes, shared GPU engine/live/forge nodes — composed over airc,
each independently health-checked + restartable. The light citizen container is
the common case (no CUDA, fast to spin up); GPU-bound groups are shared.

## 7. Cross-process command routing (already exists)

When a process's profile doesn't host a module, `route_command` resolves the
target over the bus: local-walk if present, else the airc command protocol to
the peer/service that hosts it (`GridAddressing`). The caller writes the same
`Commands.execute` — locality is the transport's concern, not the caller's. (The
locality tiers — InProcessTransport / CoreIpcTransport / AircIpcTransport — are
#27's unification: one `Connection`, transport picked by where the target lives.)

## 8. Build plan (slices)

1. **Refine `ModuleCategory`** into the §3 groups; `required_modules` filters by
   profile category-set. `FullCitizen` stays = all (no behavior change); add the
   taxonomy + the `category_dispatch_consistency` drift test coverage. *(First
   slice — contained, test-covered.)*
2. **Slim boot profiles** — new `BootMode`s (e.g. `RuntimeShell`, `Cognition`,
   `Live`) that host a subset; prove a minimal process boots + routes the rest
   over the bus.
3. **Faculty degrade-not-panic** (#26) — audit subsystem init (ONNX/Bevy/Metal)
   to disable-on-missing-dep, announce capability set.
4. **Per-service health + supervised restart** (§5).
5. **Split Live into the co-located Bevy+LiveKit sidecar** (the code already
   anticipates "Slice B' splits live into renderer + voice sidecars").
6. **Containerize per profile** (§6) honoring placement constraints; validate via
   compose-up + the #22 integration smoke inside containers.

## 9. This IS the grid (not merely grid-friendly)

Decomposition and grid distribution are the *same mechanism* — which is the
payoff of designing everything on one consistent set of primitives:

- **The profiles ARE grid roles.** "Inference node," "Live node," "light citizen
  node" aren't new code — they're the same binary booting a different profile.
  (The "same Rust binary, different `SubstrateGovernor` policy" theme of
  GENOME-FOUNDRY-SENTINEL, extended to "same binary, different service
  composition.")
- **Compute-leasing falls out for free.** A laptop leasing `ai/generate` from a
  GPU node is *literally* the Inference service group living on a different node,
  reached over the bus via `route_command`. The monolith can't express that — it
  is all-or-nothing per machine.
- **Heterogeneous hardware just works.** A node hosts only the profiles its
  hardware supports; missing-faculty degrade-not-panic (#26) is the per-faculty
  version of the same idea.
- **Healing is per-service.** A down profile re-routes around (the resilience
  lesson at the service layer) instead of a whole peer vanishing.
- **Scale each group independently** — N light citizens, M shared GPU engines —
  like auto-scaling groups, declaratively.

Because the substrate is one fluent design (concurrency + separation of concerns
applied uniformly), the grid is not a layer bolted on top — it is what the module
graph looks like when its nodes are placed across machines. The unsloth move
already proved the first decomposition: inference pulled out to its own GPU
process behind `/v1`.

## 10. Guardrails (per CONCURRENCY-STYLE-GUIDE)
- No parallel manager/coordinator hierarchy — profiles reuse `Runtime` +
  `ModuleRegistry`, not a new orchestrator.
- Each service still own-task + `interval` tick + `watch` snapshot; no sleep-loops.
- Cross-process must not add locks-across-await or hot-path serde beyond the
  existing command envelope.
- Decomposition must not handicap capable hardware (no LCD clamps); a fat node
  can still host every profile in one process (`FullCitizen`) — decomposition is
  *optional placement*, not forced fragmentation.
