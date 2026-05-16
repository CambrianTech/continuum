# Alpha Gap Analysis — Stability Plan

<!-- markdownlint-disable MD013 MD060 -->

**Updated**: 2026-05-16
**Branch policy**: every change lands as `PR -> canary -> validation -> PR -> main`
**Status**: active planning document, shared by humans and agents
**Operating rule**: Rust owns runtime logic. TypeScript is UI, schema, generated types, and thin command/transport glue.
**Template-first rule**: new commands must start from `src/generator/specs/*.json` and Continuum's command generator. Manual command scaffolds are not acceptable; hand edits are for post-generation behavior only.
**Architectural mandate**: Rust-first, GPU-first, replay-tested. No patchwork substitutes for the target architecture.
**Runtime substrate spec**: [CBAR Substrate Architecture](../architecture/CBAR-SUBSTRATE-ARCHITECTURE.md) — the runtime/RTOS contract every Rust concern inherits. ALPHA-GAP owns sequencing; CBAR-SUBSTRATE owns the substrate behavior the lanes converge on.
**Sensory model plan**: [Sensory Model And Experiential Plasticity Plan](../architecture/SENSORY-MODEL-AND-EXPERIENTIAL-PLASTICITY-PLAN.md)

This document is the alpha/gap source of truth. Work should not proceed as disconnected chat threads, private agent branches, or parallel "gap" documents. Each implementation PR must name the issue it advances, land in `canary`, publish validation evidence, and only then be considered for promotion to `main`.

As of 2026-05-13 there is exactly one alpha/gap planning file:
`docs/planning/ALPHA-GAP-ANALYSIS.md`. New alpha/gap notes are merged here or
deleted. Architecture references may point here, but they must not become
parallel status ledgers.

The previous 2026-05-01 alpha snapshot was useful but had become a historical log. This revision turns it into an execution plan for the current goal: **stable, GPU-first, Rust-centric Continuum with modular Docker and fast tests that do not depend on the Node/UI stack for core correctness.**

## 2026-05-11 Management Reset: Rust First, No Patchwork

Continuum is past the point where local fixes to Node/TS symptoms can be treated as product progress. The product is a native, highly concurrent, resource-aware AI runtime that happens to have a browser UI. The implementation posture is therefore:

1. **Architecture beats remedies.** If the bug is caused by cognition, inference, resource pressure, model routing, memory, tool execution, or persona scheduling living in the wrong layer, the fix is to move the responsibility to the right Rust abstraction. Do not add another TS guardrail around a Rust/runtime concern.
2. **Rust is the design language for runtime behavior.** New behavior under persona cognition, model selection, local inference, paging, LoRA/model residency, memory consolidation, tool parsing/execution, command execution semantics, and recovery state machines starts in Rust.
3. **TypeScript is not the prototype layer for cognition.** TS iteration speed is not a justification. A fast prototype that stays in Node becomes permanent debt. The correct loop is Rust unit test -> Rust replay/VDD test -> canary integration -> live smoke.
4. **No silent fallbacks.** CPU fallback, cloud fallback, empty API-key availability, generic model fallback, placeholder UUIDs, and swallowed command errors are alpha blockers unless explicitly surfaced as degraded state with a user-visible remedy.
5. **No feature-disabling fixes.** A fix that makes tests pass by disabling local models, personas, chat, inference, telemetry, or replay is a regression unless the PR is explicitly a kill-switch PR and documents the lost capability.
6. **No PR sediment.** PRs are not storage. A PR either merges to canary after evidence, gets rebased and completed, or is closed with the durable work moved into an issue/design doc. Long-lived PRs are technical debt.
7. **Perfect means structurally correct, not endlessly delayed.** The expected cadence is small architectural PRs that move ownership to Rust and delete the wrong layer. "Perfect" does not mean one huge rewrite branch; it means every merged increment points at the final architecture and reduces future work.

This reset supersedes "move fast and break things" thinking. Agents have enough implementation bandwidth to spend the extra hours on the correct abstraction up front. That is cheaper than debugging another patchwork system for weeks.

## Alpha Definition

Alpha is ready when a fresh user can install, boot, talk to personas, recover from common failures, and verify the system mostly through Rust-level tests.

The non-negotiable gates:

1. **GPU-first inference**: alpha-critical inference must use Metal/CUDA/Vulkan/DMR GPU paths. No silent CPU fallback.
2. **Sensory personas are the product**: every standard persona has multimodal perception, voice/audio, avatar/control output, and WebRTC room presence. Text-only is a compatibility/degraded mode, not the alpha target.
3. **Qwen multimodal is the local target family**: Qwen 3.5 now and Qwen 3.6 next are treated as first-class local persona targets. Vision/audio layer gaps, unsupported kernels, CPU layers, or upstream runtime limitations are owned engineering work.
4. **Rust core owns behavior**: persona cognition, scheduling, resource pressure, paging, inference orchestration, replay, and recovery live in Rust.
5. **Node/TS is thin**: browser UI, command adapters, schemas, generated types, and minimal transport glue only.
6. **Docker is modular and GPU-capable**: one opaque "build/seed/start everything" container is not alpha-ready. Services need independent health, logs, restart boundaries, and GPU-visible runtime paths on machines that support them.
7. **Fast tests first**: core work must be covered by `cargo test` or Rust integration tests before Docker/browser tests.
8. **Canary is the sync point**: every fix is merged to `canary` first and tested there by available Mac/Windows/Linux agents.
9. **No silent success**: health checks, install steps, inference readiness, bridge delivery, and UI restore paths must fail loud with actionable evidence.
10. **Persona cognition TS line count trends downward**: any PR touching persona cognition must delete or shrink TS runtime logic under `src/system/user/server/` unless it is strictly UI/schema/adapter work.
11. **Replay before live claims**: persona, RAG, tool, inference, and memory changes must include a Rust fixture/replay/unit test before "works live" is accepted.
12. **One source of truth per runtime fact**: model definitions, provider availability, context budgets, hardware capability, config values, room identity, and command semantics must each have one canonical owner.

### CBAR-Like Runtime Substrate Contract

Continuum's Rust runtime must adopt the CBAR performance philosophy from
`/Users/joelteply/Development/cambrian/cb-mobile-sdk/cpp/cbar`: small concern
modules inherit the hard machinery from a shared substrate. The goal is not a
literal class-for-class port; the goal is the same RTOS-style behavior:
concurrent lanes, bounded queues, lazy shared artifacts, realtime-first
cadence, resource admission, and handles instead of copied memory.

The reusable substrate must provide:

- `RuntimeFrame` / `CognitionTurnFrame`: one turn/frame object with stable keys
  and lazy artifacts for room snapshot, RAG, model selection, prompt fragments,
  media handles, embeddings, KV leases, LoRA leases, response envelopes, and
  trace metrics.
- `RuntimeModule`: a narrow Rust trait for concerns. Modules declare
  subscriptions, lane, cadence, dependencies, and budget; they do not invent
  their own scheduler.
- `ResourceClass` plus `TargetSilicon`: the shipped two-axis scheduler shape.
  `ResourceClass` describes what kind of work is being scheduled, while
  `TargetSilicon` describes where it wants to run. Docs may say "lane"
  informally, but implementation should reuse these shipped enums rather than
  invent `ResourceLane`.
- `ArtifactHandle` / leases: module boundaries pass ids, hashes, offsets,
  texture ids, buffer leases, model residency leases, KV page ids, and LoRA
  page ids. Bulk payloads stay resident in the owning pool.
- dependency wakeups: work runs when required artifacts become ready, not
  because a global FIFO happened to drain.
- cadence and pressure gates: realtime work runs first; delayed work runs by
  cadence, state delta, or explicit trigger; pressure reduces cadence,
  precision, context, subscriber count, or modality with visible reasons.
- built-in logs, metrics, flush, abort, shutdown, queue depth, queue time,
  execution time, coalesced count, deferred count, and resource residency.
- one standard VDD record emitted by the Rust substrate for every platform, so
  Mac, Windows/RTX, Docker, and future grid nodes report comparable timing,
  throughput, CPU/GPU, residency, silence, and bottleneck fields.
- one-line instrumentation helpers for runtime code: scopes, marks, counters,
  residency, deferrals, and failures should feed the standard VDD record
  automatically. A module author should not write a custom timing harness to
  answer whether CPU fell, GPU utilization rose, memory/power stayed bounded,
  or throughput improved.

This substrate is the base-class/OOP-equivalent discipline for Rust. Extension
code should be short: implement the small trait, declare dependencies, and let
the runtime provide concurrency, telemetry, pressure, wakeups, and lifecycle.
New modules should normally be measured in a few hundred lines, not thousands.
If a new runtime concern needs its own bespoke communications, queue,
backpressure, retry, metrics, lifecycle, or failure-reporting system, the PR is
exposing missing substrate work and should fix the shared substrate instead of
growing a monolith.

The first implementation PRs should not add more bespoke queues, fallback
paths, or TS orchestration. They should converge existing Rust pieces into this
substrate: `ServiceModule`, `MessageBus`, `SharedCompute`, `ChannelQueue`,
`PressureBroker`, `PagedResourcePool`, model registry, and
`llamacpp_scheduler`.
The missing work is specifically `RuntimeFrame` / `CognitionTurnFrame` and
formal artifact subscription/cadence/dependency declarations on top of the
shipped substrate primitives, not a restart from zero.

### Sensory Persona Product Contract

Continuum's differentiator is not "chat with several text bots." The alpha product is a local sensory persona grid: users can call personas into a WebRTC room, speak to them, see them, and receive useful multimodal responses from agents that can perceive images/video/audio and drive avatar or other control outputs.

Implementation consequences:

- **Every standard persona declares sensory requirements.** The default requirement set includes text, vision, audio input, voice/audio output, avatar/control output, and WebRTC presence. A persona that cannot satisfy those requirements is marked `Degraded` with the missing capability, not silently treated as alpha-complete.
- **STT/TTS are adapters, not the center.** They exist to support compatibility models and weaker hosts. The standard local model path targets multimodal models directly where possible.
- **Qwen 3.5/3.6 are optimization targets.** The registry and runtime resolve model requirements by capability, context, memory budget, and GPU support. They do not scatter hardcoded model names or accept random provider/model drift.
- **Qwen GPU support is an alpha contract.** Qwen 3.5 text/code and Qwen2-VL
  vision must run through Continuum's llama.cpp/local runtime with all viable
  layers on the required platform backend: Mac -> Metal, NVIDIA -> CUDA, and
  AMD/Intel -> Vulkan. Unsupported Qwen layers, mmproj/audio/vision gaps, CPU
  graph splits, or missing upstream kernels are implementation blockers to fix
  or vendor/upstream, not reasons to route around the local runtime. The model
  resolver must expose selected model, backend, GPU layer count, expected
  residency, unsupported layers, and any degraded reason before a persona turn
  starts.
- **Open-source runtime gaps are ours to fix.** If llama.cpp, Candle training code, GGUF conversion, kernels, multimodal projectors, audio layers, or paging support are missing what Qwen needs, the work item is to fork/vendor/upstream the fix with benchmarks. "Upstream cannot" is not a final answer for open-source dependencies.
- **No CPU crutches in the happy path.** CPU fallback is explicit degraded mode for unsupported hardware, tests, or emergency operation. It is not a performance plan for a 3090/5090/M-series target.
- **Live media is a gate.** Video chat, avatar output, and WebRTC bridge health are alpha gates. A PR that breaks sensory persona presence must fail validation before canary promotion.
- **Sensory model scouting is a tracked workstream.** Current Qwen3.5, Qwen3.6, Qwen2.5-Omni, Qwen3-Omni, forge/alloy, experiential plasticity, pruning, and MoE pruning work lives in the sensory model plan linked above. Runtime adoption still goes through the Rust registry and VDD gates.

## Current Snapshot

Reflects canary as of 2026-05-16 (post the 8-PR cognition-oxidization batch +
PressureBroker bootstrap PR-1/2/3 + Docker tier Phase 1 + inference-grpc
fail-closed). For each area, the "current read" is what is provably in canary,
not what is intended. "Alpha risk" calls out the gap to the alpha gates above.

| Area | Current read (canary @ 2026-05-16) | Alpha risk |
|---|---|---|
| AIRC collaboration | AIRC canary has public `knock` plus forward-secret `approve`/`decrypt-approval` handoff; Continuum PR #1110 pilots repo-local `.airc/` collaboration rules; agent flywheel board #1272 active with codex-main heartbeats | Queue/nudge work tracked in CambrianTech/airc#562; Continuum personas and external agent providers are not yet first-class workers on the shared queue; manager-role transition in progress this session |
| UI room state | PR #1047 merged to `canary` for stale duplicate General tab recovery | Needs live UI reload validation before `main` promotion |
| Docker | Phase 1 of Docker tier surface merged (#1297 — `system/docker-tier-stats` IPC + ts-rs DockerTierStats); GPU profile + tier pool eviction (#1238, #1239) still open; historical bulk and mixed responsibility still in the runtime images | Docker can mask failures and slow iteration; tier pool eviction + capability-visible health are the remaining alpha lifts |
| Rust core | Substantial gains this session: PressureBroker bootstrap landed (#1307 PR-1 + #1308 PR-2 IPC + #1310 PR-3 status surface); runtime lease broker added (#1313); cognition migrated for `should_respond` (#1284), `rate_proposals` (#1290/#1291/#1293), `generate_recipe` (#1298/#1301/#1303), `vision-describe` (#1292); dead Candle paths deleted (#1277/#1279/#1281/#1288); inference-grpc + orpheus hard-fail on no-GPU (#1314); InferenceCapability trait + probe + registry shipping on `feat/grid-inference-routing-pr2-announcer` (PR-1 of GRID-INFERENCE-ROUTING) | RuntimeFrame / CognitionTurnFrame still unbuilt (Lane D); per-module hardcoded concurrency declarations still present across `src/workers/continuum-core/src/modules/*.rs`; universal base trait + derive macro + scaffold generator (the "low-friction inheritance" triplet from CBAR-SUBSTRATE) not yet landed |
| Node/TS | Net-negative trend this week: ~2500 LOC TS deleted via cognition oxidization stacks (rate_proposals adapter zero-callers deletion + generate_recipe shim collapse 371→140 LOC + post-inference adequacy gate rip #1309); SQLite default config landed (#1271) | Multiple TS daemons still own runtime logic that belongs in continuum-core; the F-lane ratchet (TS cognition deletion CI gate) is not yet active; new TS in cognition paths is still mechanically allowed |
| Config/secrets | `$HOME/.continuum/config.env` is the local source of truth, but empty placeholders and per-process loading have caused false provider availability | Cloud providers can steal local turns and fail; grid nodes cannot yet receive encrypted config consistently |
| Tests | Many tests exist; the alpha loop still overuses `npm start`/browser/Docker as proof; `no_cpu_fallback_contract.rs` regression test exists for the llama.cpp/ORT paths only — does not cover the Candle-side device selection where the orpheus + inference-grpc CPU fallbacks lived before #1314 | Slow tests hide root causes and discourage TDD; the no-CPU-fallback contract test needs widening to the whole workers tree, not just three whitelisted files |

## Immediate Canary Work Packages

These are the active alpha blockers exposed by the 2026-05-11 VDD runs and
PR #1082 review. They are split so agents can work in parallel without stepping
on each other. Each lane starts from `canary`, opens a focused PR back to
`canary`, and posts validation evidence before merge. Assignment is explicit:
if an agent cannot work a lane, it says so on AIRC and the lane is reassigned.

| Lane | State @ 2026-05-16 | Owner | Branch | First PR | Merge gate |
|---|---|---|---|---|---|
| A. Rust model registry and admission | In progress | RTX/Windows lane (catalog + admission); supervision rotated from Codex PM → this manager | `feature/rust-model-registry-admission` (merged-stack), follow-ups on canary | Typed Rust catalog, capability request, resolver/admission explanation | Rust resolver tests plus missing-Qwen fail-hard test |
| B. Installer model seeding and GPU profiles | Phase 1 landed (#1297 Docker tier surface); GPU profile + tier-pool eviction still open (#1238/#1239) | RTX/Windows Docker lane; Lane A owns registry artifact contract | `feature/docker-gpu-profile-modular` | `model-init`/installer seeds required Qwen artifacts into the runtime model volume | Windows/RTX fresh install reaches model-ready state or fails loud |
| C. VDD telemetry substrate | In progress; structured RuntimeMetric emitting from inference and persona but VDD report command not yet bound | RTX/Windows substrate; Mac/Metal adapter sub-task carried by Mac lane | `feature/rust-vdd-telemetry-substrate` | Structured timing/resource metrics flow into trace/event bus | VDD report shows first-token, tok/s, CPU, GPU, VRAM/RSS from structured data |
| D. CBAR persona runtime frame | **Unstarted.** Critical Phase 0 gap. CBAR-SUBSTRATE-ARCHITECTURE.md spec exists but RuntimeFrame/CognitionTurnFrame are not built. Most other lanes are blocked-or-degraded on this | **Needs owner claim** — this is the alpha critical path | `feature/cbar-persona-runtime-frame` | Rust `PersonaTurnFrame` with lazy RAG/media/priority outputs and inbox coalescing | Multi-message smoke produces one consolidated turn, not per-event inference flood |
| E. Pressure broker and paging gate | Bootstrap landed (#1307 PR-1 broker types/registry, #1308 PR-2 IPC, #1310 PR-3 status surface, #1313 runtime lease broker); paging (KV/LoRA residency) + pooled mtmd context still open | RTX/Mac runtime lanes | `feature/pressurebroker-admission-gate` (bootstrap stack merged); follow-ups branch per PR | Unified admission gate blocks unsafe backend/model/context loads | Concurrency test refuses unsafe second load and reports `Backpressured`/`Unavailable` |
| F. TS cognition deletion ratchet | Manual deletion progressing (~2500 LOC TS deleted via 8 PRs this session) but mechanical CI gate not yet enforced | **Needs owner claim** — without the ratchet, new TS cognition can still mechanically slip back in | `feature/persona-ts-deletion-ratchet` | CI/check script enforces no new persona cognition TS and net-negative touched cognition | PR fails if verb-shaped TS cognition grows or introduces forbidden provider/fallback strings |
| G. Canary PR hygiene | In progress; rotating from Codex PM → this manager. Doc refresh in flight on `joel/docs-alpha-refresh` | This manager | `docs/alpha-rust-workstreams` (current refresh: `joel/docs-alpha-refresh`) | This document plus issue/PR checklist cleanup | Every active PR has owner, blocker, validation command, and canary target |
| H. Substrate governor + tiered genome cache | **Proposed** — design landed via continuum#1327. 7-PR implementation sequence: governor types → tier stores → recall API → composer+speculator → foundry skeleton → sentinel skeleton → sharing-protocol local-first | **Needs owner claim** | `feature/substrate-governor-genome-cache` | `SubstrateGovernor` + `HardwareClass` + hardware detection at boot | Same Rust binary writes different policy on MacBook Air vs RTX 5090; VDD records prove different tier sizes / concurrency / speculation aggressiveness |

Adjacent active workstream not in the lane table:

- **GRID-INFERENCE-ROUTING** — PR-1 (inference capability announcer + probe +
  registry) in flight on `feat/grid-inference-routing-pr2-announcer`. This is
  the grid-side counterpart of Lane A: Lane A says which model the request
  needs, GRID-INFERENCE-ROUTING says which peer can serve it. Owner: airc-8a5e.
  Tracked under § 7 (AIRC And Continuum Internal AI Collaboration) below.

Lane claim updates as of 2026-05-16:

- Lane A has shipped its first wave — `model_registry/` exists in
  `src/workers/continuum-core/src/`, with curated catalog rows and an
  admission resolver. Open follow-ups: missing-Qwen fail-hard end-to-end (must
  surface in the chat UI, not just structured status) and `ts-rs` exports
  shrink the duplicate TS model maps in Lane F's deletion targets.
- Lane B Phase 1 landed (#1297 `system/docker-tier-stats` IPC + ts-rs
  `DockerTierStats`). Capability-visible health and tier-pool eviction
  (#1238/#1239) are the next Lane B PRs; both should consume the Lane A
  registry artifact contract, not invent a parallel one.
- Lane C structured `RuntimeMetric` events emit from inference paths, but the
  `vdd-report-command` step (Lane C PR sequence step 3) is not yet bound. As a
  result, "VDD" is still mostly read from logs rather than from a single
  command's structured output. RAG source tracing and `SEAM_RAG_COMPOSE`
  remain joint with Lane D.
- **Lane D is the most expensive currently-unstarted lane.** PressureBroker
  (Lane E) and the inbox coalescing CBAR pattern were both written in the
  expectation that a `RuntimeFrame` / `CognitionTurnFrame` exists. Until it
  does, every persona-side consumer still owns ad-hoc fan-out and the
  inference-per-event flood the lane was created to remove. Claiming this lane
  is the single highest leverage move on the board right now.
- Lane E bootstrap landed (#1307 / #1308 / #1310 / #1313). The remaining lane
  scope is paging (KV/LoRA residency, pooled mtmd context, eviction policy)
  and **deletion of pre-broker concurrency hacks** that still bypass the
  broker. Concrete example pinned for deletion:
  `src/workers/inference-grpc/src/main.rs` — `get_num_workers()` reads
  `INFERENCE_WORKERS` from `~/.continuum/config.env` and otherwise picks a
  worker count from system memory at startup. Both branches are exactly the
  "we do not hard code" / "they code in tokio not whatever their fee fees say"
  anti-pattern. PressureBroker owns concurrency; this function should be
  deleted and the worker count derived from broker leases.
- Lane F has been progressing through manual deletion (rate_proposals adapter
  zero-callers delete, generate_recipe shim collapse, #1306 cognition cap
  lift, #1309 TS suppression rip — ~2500 LOC TS removed this session). The
  mechanical ratchet itself (the CI gate that prevents *new* verb-shaped TS)
  has not yet landed. Until it does, the deletion progress is reversible.
- Lane G refresh in flight: this document, the supporting doc cross-links
  (CBAR-SUBSTRATE precedence rule added), and the lane status table you are
  reading.
- Lane H proposed via continuum#1327
  ([GENOME-FOUNDRY-SENTINEL.md](../architecture/GENOME-FOUNDRY-SENTINEL.md)).
  Owns the artifact-sharing economy layered on top of CBAR-SUBSTRATE:
  tiered genome cache (L1–L5), `WorkingSetManager` + page faults, foundry
  (JIT for SOTA absorption), sentinel-AI (profile-guided optimization
  from lived traces), demand-aligned recall, composer + speculator, and
  the `SubstrateGovernor` (DVFS for AI — same Rust code on MacBook Air
  and RTX 5090, different governor policy). Sibling to Lane E
  (`PressureBroker`): broker owns admission; governor owns sizing.
  Needs owner claim; 7-PR sequence detailed in the GENOME-FOUNDRY-SENTINEL
  doc's Part 13.

### Lane A: Rust Model Registry And Admission

**Problem**: model/provider facts are scattered, cloud/local availability can be
misreported, and the Windows/RTX VDD run proved the CUDA stack can be healthy
while no local Qwen model exists and personas silently produce zero replies.

**Design**:

- Rust owns `ModelRegistry`, `ModelRequirement`, `ModelCandidate`,
  `ModelArtifact`, `ProviderKind`, `LocalRuntimeKind`, and `AdmissionDecision`.
- Runtime callers request capabilities: modalities, minimum intelligence tier,
  context window, tool support, latency class, memory budget, GPU requirement,
  family preference, and explicit override.
- The registry is a curated whitelist of vetted artifacts. Hugging Face/foundry
  discovery can populate candidates, but runtime admission only selects vetted
  rows with known template, license, backend, quantization, memory estimate,
  modality metadata, and forge status.
- Local chat inference is `LocalRuntime` through the llama.cpp/Qwen adapter
  stack. Candle is for training/LoRA/forge paths, not persona chat inference.
- Cloud providers remain adapter kinds. They do not steal turns unless their key
  is non-empty, health checked, and explicitly admitted for that request.

**Owned files/modules**:

- `src/workers/continuum-core/src/model_registry/`
- `src/workers/continuum-core/src/inference/`
- `src/workers/continuum-core/src/ai/`
- `src/workers/continuum-core/src/persona/cognition_io.rs`
- generated `ts-rs` types under `src/shared/generated/`

**PR sequence**:

1. `model-registry-types`: Rust enums/structs plus `ts-rs` exports.
2. `model-registry-catalog`: curated Qwen 3.5/2-VL rows and artifact metadata.
3. `model-admission`: resolver returns selected candidate plus rejected
   alternatives and resource explanation.
4. `missing-model-fail-hard`: no local Qwen yields typed unavailable state and
   user/actionable remedy, never silence.

**TDD**:

- `cargo test --package continuum-core model_registry`
- exact model pin, family preference, `>=` intelligence/context requirement, GPU
  required, no artifact present, and cloud key empty cases.

**VDD**:

- Fresh machine with no model file reports `Unavailable(MissingArtifact)` in
  structured status and chat smoke sees a visible failure.
- Machine with Qwen artifact selects local runtime, records memory projection,
  and starts inference without CPU fallback.

**Deletion targets**:

- duplicate TS model maps/context windows
- free-form provider/model strings in persona seed/runtime paths
- stale local-model fallback branches and any forbidden provider tombstones

### Lane B: Installer Model Seeding And GPU Profiles

**Problem**: Windows/RTX had CUDA containers ready, low CPU, and available VRAM,
but no Qwen model was mounted. The runtime stayed silent instead of becoming
model-ready or failing loud.

**Design**:

- Add an explicit `model-init` responsibility for required alpha artifacts.
- Seed required local Qwen artifacts into the same volume/bind mount the Rust
  runtime reads.
- Separate Docker profiles: `gpu`, `ui`, `live`, `grid`, `forge`, `devtools`.
- Pin GPU images and make backend capability visible at health check time.

**Owned files/modules**:

- `setup.sh`, install scripts, and docs install paths
- `docker-compose*.yml`
- Docker image build/push scripts
- `src/workers/continuum-core/src/model_registry/artifacts.rs`

**PR sequence**:

1. `model-init-profile`: separate model prewarm/download service.
2. `qwen-seed-contract`: required local model list comes from Rust registry
   artifact metadata, not shell hardcoding.
3. `windows-rtx-install-vdd`: Windows GPU install smoke with model-ready proof.

**TDD**:

- shell/unit checks for model volume path resolution
- Rust artifact resolver tests for missing, partial, corrupt, and ready states

**VDD**:

- Windows/RTX: cold start, first token, tok/s, CPU%, GPU%, VRAM, RSS.
- Mac/Metal: same metrics, plus Metal layer offload evidence.
- No model present: install exits or health reports explicit missing artifact in
  less than 30 seconds.

**Deletion targets**:

- one-off model download code in TS/server startup
- Docker paths that bypass Continuum's adapter/router substrate
- opaque bulk startup scripts that hide which service failed

### Lane C: VDD Telemetry Substrate

**Problem**: timing, CPU/GPU utilization, tok/s, memory growth, and RAG evidence
are still partly ad hoc logs. That makes validation slow and makes realtime
behavior hard to reproduce.

**Design**:

- Rust emits structured `ValidationTrace`/`RuntimeMetric` events.
- `CognitionTrace` gets seams for RAG composition, model admission, inference
  init, first token, steady decode, post-process, and recorder persistence.
- Metrics are emitted through the event bus and recorder fixtures. Stdout/stderr
  text is local debugging output only, not the validation API.
- One-liner timing guards are available to Rust modules so every new subsystem
  gets timing and metadata with almost no code.

**Owned files/modules**:

- `src/workers/continuum-core/src/persona/trace.rs`
- `src/workers/continuum-core/src/persona/recorder.rs`
- `src/workers/continuum-core/src/rag/`
- `src/workers/continuum-core/src/inference/`
- event bus/logging modules under `continuum-core`

**PR sequence**:

1. `trace-rag-compose`: add `SEAM_RAG_COMPOSE` and RAG source hashes.
2. `trace-inference-metrics`: first-token, tok/s, backend, layer offload,
   CPU-degraded and GPU-required status flags.
3. `vdd-report-command`: command emits a compact machine-readable VDD report.

**TDD**:

- recorder fixture tests for success and failure traces
- RAG replay test proves source hashes and context can be inspected
- inference adapter unit test with injected timings

**VDD**:

- Mac/Windows report generated from structured metrics, not copied terminal log.
- CPU peg, CPU layer fallback, missing tok/s, and memory growth become failed
  validation checks.

**Deletion targets**:

- println-style validation paths
- duplicate TS logging/capture sinks
- hand-assembled performance report scripts that scrape random console text

### Lane D: CBAR Persona Runtime Frame

**Problem**: persona inbox/RAG/scheduling behavior can flood inference by
treating events too literally. The runtime needs a CBAR-like turn frame:
immutable input, lazy derived outputs, coalesced work, and independent nodes.

**Design**:

- `PersonaTurnFrame` wraps room/user/persona signal state for a bounded turn.
- Lazy outputs include consolidated inbox chunk, RAG context, media summary,
  priority score, tool relevance, model requirement, and response prompt.
- Nodes pull what they need and pay only for what they request.
- Inbox consolidation is FIFO-preserving but chunked: many room events can
  produce one planned turn instead of one inference per event.

**Owned files/modules**:

- `src/workers/continuum-core/src/persona/`
- `src/workers/continuum-core/src/cognition/`
- `src/workers/continuum-core/src/rag/`
- TS shrink targets under `src/system/user/server/modules/PersonaInbox.ts`,
  `ChatRAGBuilder.ts`, `PersonaResponseGenerator.ts`, and related deciders

**PR sequence**:

1. `persona-turn-frame`: frame/trait/pipeline skeleton with lazy outputs.
2. `inbox-coalescing`: chunk/buffer room events and prove one turn per window.
3. `rag-frame-output`: RAG composition becomes a lazy frame output with trace.
4. `prg-shim-shrink`: TS PRG becomes a thin command shim or deletes.

**TDD**:

- Rust tests for lazy output computes once across multiple consumers.
- Inbox test: N events within window -> one consolidated turn plan.
- Replay test: fixture reproduces prompt/RAG/media from frame outputs.

**VDD**:

- Chat smoke records fewer inference calls than incoming events.
- First response improves or stays flat while CPU/RSS do not climb.

**Deletion targets**:

- TS inbox consolidation logic
- TS ChatRAGBuilder behavior
- TS response-generator orchestration beyond thin command glue

### Lane E: Pressure Broker And Paging Gate

**Problem**: model, context, LoRA, media, and backend resources are still too
independent. The correct controller must admit, page, evict, or defer across
all resource types under one policy.

**Design**:

- `PressureBroker` owns admission for model weights, mmproj/mtmd contexts, KV
  cache, LoRA adapters, embedding cache, WebRTC/media buffers, and render
  textures.
- Resource pools expose typed cost, residency, last-use, priority, and eviction
  hooks.
- Unsafe requests return `Backpressured`, `Unavailable`, or `Deferred` with an
  explanation. They do not allocate and hope.

**Owned files/modules**:

- `src/workers/continuum-core/src/gpu/`
- `src/workers/continuum-core/src/inference/`
- `src/workers/continuum-core/src/memory/`
- `src/workers/continuum-core/src/live/`
- `src/workers/llama/src/mtmd.rs`

**PR sequence**:

1. `pressurebroker-types`: typed resource classes, budgets, decisions.
2. `backend-admission-gate`: model/mmproj init checks broker before allocate.
3. `pooled-mtmd-context`: reuse multimodal context under broker ownership.
4. `kv-lora-paging`: extend to KV and LoRA residency.

**TDD**:

- concurrent allocation test refuses unsafe second backend/context.
- injected OOM/dead backend enters recover/unavailable state, no hang.
- LRU/priority eviction tests.

**VDD**:

- 4+ personas on constrained profile report bounded memory and explicit
  deferrals.
- 5090 profile uses GPU lanes aggressively without CPU fallback.

**Deletion targets**:

- per-adapter private memory heuristics
- hidden CPU fallback branches
- duplicate context/model pool code

### Lane F: TS Cognition Deletion Ratchet

**Problem**: migration intent is not enough. The repo needs a mechanical gate
that prevents new verb-shaped TS cognition and forces deletion as Rust lands.

**Design**:

- CI/check script computes TS cognition line count for touched cognition PRs.
- New `.ts` files under persona cognition directories fail unless allowlisted as
  ORM noun, generated schema, UI, or thin shim.
- Forbidden strings such as deprecated provider names or fallback comments are
  blocked in runtime code and docs that are not migration notes.

**Owned files/modules**:

- test/ratchet scripts
- CI/pre-push hooks
- `src/tests/unit/shared-node-boundary.test.ts`
- docs describing exceptions

**PR sequence**:

1. `persona-ts-ratchet-script`: local script with clear failure output.
2. `persona-ts-ratchet-ci`: CI/pre-push enforcement for touched cognition PRs.
3. `forbidden-provider-scan`: remove and block obsolete provider/runtime names.

**TDD**:

- fixtures for allowed generated/UI/noun TS and forbidden verb TS.
- scan test proves obsolete provider names cannot re-enter runtime code.

**VDD**:

- each cognition PR reports TS lines before/after and Rust test coverage.

**Deletion targets**:

- stale comments, tombstones, fallback branches, and obsolete provider mentions
- any TS cognition file replaced by a Rust module

## Issue-Driven Workstreams

### 0. Canary Discipline And Collaboration

**Goal**: stop parallel agents from diverging. Every agent should know the issue, branch, PR, validation command, and current blocker.

| Issue / PR | Role | Required action |
|---|---|---|
| PR #1035 | current canary -> main promotion PR | Keep rebased; promote only after canary has real chat/local-model validation plus relevant platform smoke |
| PR #1046 | AIRC bridge harness for Continuum testing | Merge/rebase/close deliberately; use it to reduce manual `jtag chat/send` and paste relay |
| PR #1068 | Rust persona recorder as single fixture source | Merged to canary; sets the SSoT pattern for replay/capture |
| PR #1069 | Rust response cleanup, TS sanitizer removed | Merged to canary; sets the "move behavior Rust-side, delete TS duplicate" pattern |
| stale canary PRs (#941, #972, #973, #1026, #912) | PR debt | Rebase and validate within one work session or close with issue notes |
| #967 | personas as AIRC peers | Treat as the collaboration unlock: Continuum personas should participate without manual CLI glue |
| CambrianTech/airc#559 | public knock, approved room handoff, shared sprint queue | AIRC canary has knock and encrypted approve handoff; Continuum must consume the workflow through `.airc/` and persona/agent integration |
| CambrianTech/airc#562 | peer-to-peer work queue/nudges | Use as the always-on flywheel: any approved peer can nudge idle agents, discover stale/unowned work, and keep the queue moving |
| PR #1110 | repo-local `.airc/` pilot | Land to canary once docs match current AIRC commands and validation passes; this is the first Continuum-side collaboration contract |
| #1113 | move live chat off ORM/IPC hot path | AIRC/event-log owns transcript, files, pointers, signaling metadata, and queue chatter; Continuum stores bounded projections |
| CambrianTech/airc#563 | AIRC message/file substrate | Needed before Carl/browser chat smoke can stop using JTAG chat commands |

Rules:

- Implementation starts from an issue. If no issue exists, file it before coding.
- PR body must include: issue link, canary target, validation commands, platform coverage, and what was not tested.
- Agents coordinate on AIRC, but the durable truth is issue + PR comments.
- `main` promotion only happens after canary has been exercised by at least one real UI path and one non-UI/Rust path relevant to the changes.
- Open PRs are triaged every session before new feature work. Each gets one of four states: `merge-after-green`, `needs-rebase`, `convert-to-issue`, or `close-stale`.
- A PR older than 48 hours without a concrete blocker is presumed stale until proven otherwise.
- If a PR is correct but incomplete, finish and merge it to canary; do not recreate the same work on a new branch.

### 0A. AIRC As The Development Substrate

**Goal**: Continuum should be able to develop itself through a shared grid of
agents, personas, local models, and humans. AIRC owns the coordination substrate;
Continuum exposes reliable generated commands and consumes AIRC as an
integration layer.

The operating model:

- AIRC remains available even when Continuum is down, rebuilding, wedged, or
  being restarted. It is the continuity layer for work state, handoffs, and
  recovery.
- GitHub issues and PRs are the durable work cards. AIRC provides the concise
  room digest, presence, nudges, approval, and peer-to-peer coordination around
  those cards.
- One GitHub account may run many agents. Assignment and presence must use AIRC
  peer/session identity, nick, role, bio, and whois data rather than assuming
  one GitHub login equals one worker.
- Agents should not need a human to ask what to do. An approved agent joins,
  receives the room rules and current queue digest, claims or reviews a card,
  posts evidence, and releases or completes the card.
- `airc nudge` / queue nudges must be peer-to-peer, not manager-only. Any
  online approved peer can poke idle peers to poll the queue, report blockers,
  or pick up stale work.
- Cloud models, local models, Continuum personas, OpenClaw, Hermes, and future
  grid workers all plug in as workers if they can speak AIRC and execute the
  relevant Continuum command surface.
- This is intentionally an OpenClaw-lite/Hermes-lite development framework,
  not a replacement for those projects. AIRC supplies the small, durable
  collaboration/control plane: rooms, identity, queue cards, nudge/stale
  detection, PR proof, and handoff. Continuum supplies the local runtime,
  cognition, Sentinels, generated commands, grid execution, and product UI.
- The alpha target is useful even with no web interface running. A developer
  should be able to install AIRC, join the project room, run Continuum's Rust
  backend/Sentinel worker surface, and let approved agents coordinate work
  across local and grid machines without Node being required for the core
  worker loop.
- Continuum commands used by these workers must be generated/template-first.
  Manual command scaffolds break the self-development loop because agents need
  one predictable command contract.
- JTAG chat commands are compatibility plumbing. The target is AIRC transcript
  plus file/attachment APIs for live chat, scrollback, cursors, receipts, and
  replay. Continuum should consume compact events/pointers and project only
  bounded durable state.

Near-term Continuum tasks:

1. Land PR #1110 so this repo advertises its AIRC front door, rules, and queue
   expectations from `.airc/`.
2. Wire Continuum personas into AIRC rooms as first-class peers for issue/PR
   digest, claim/release/done, and nudge handling.
3. Expose generated Continuum commands that let agents run bounded smoke tests,
   image preflights, install checks, and forge/factory preflights without
   needing bespoke shell knowledge.
4. Move the core agent worker path toward Rust-only execution: queue polling,
   Sentinel dispatch, generated command execution, and proof emission must have
   a no-Node path so Continuum can serve agents while the browser/UI stack is
   down.
5. Validate the pilot by having at least one external peer join through knock,
   receive approval, claim a GitHub-backed work card, post validation evidence,
   and hand off through AIRC.

### 1. First-Run And Install Stability

**Goal**: a new user does not hit a silent or half-working install.

| Issue | Priority | Direction | Test gate |
|---|---:|---|---|
| #1006 WSL2 cannot reach raw.githubusercontent.com | P0 | install must detect network/bootstrap failure early and print a concrete fix | Windows fresh install log shows failure in <30s with remedy |
| #1007 Windows rustc ICE compiling continuum-core | P0 | do not make first-run depend on a fragile local Rust build when a published binary/image can be used | Windows install reaches runnable app without compiling core locally |
| #1008 core socket owned by root container | P0 | fix UID/GID and socket volume ownership; host `jtag` must connect | host `jtag ping` succeeds against container core |
| #980 Carl validator QA bugs | P0 | break into child issues if still bundled | each child has a canary PR or is closed as stale |
| #983 Vulkan deferred model download | P0 | download/prewarm with progress during install or show explicit first-chat loading state | first Vulkan chat never sits silent during multi-GB download |
| #770 fresh install E2E | P0 | make this the release gate, not a one-off QA task | Mac + Windows reinstall logs attached to canary validation |

Implementation posture:

- Prefer published Rust artifacts or minimal service images over compiling everything during first-run.
- If build is unavoidable, make it explicit and resumable.
- Install health must distinguish: network unavailable, Docker unavailable, GPU unavailable, model unavailable, Rust core unavailable, UI unavailable.

### 1A. Config, Secrets, And Grid Propagation

**Goal**: one authoritative config path per node, explicit encrypted propagation across trusted grid nodes, and no false "configured" state from empty placeholders.

| Issue | Priority | Direction | Test gate |
|---|---:|---|---|
| file: config single-source issue | P0 | `SecretManager` and Rust `secrets.rs` must treat only non-empty values as configured and must lazy-load `$HOME/.continuum/config.env` before any provider check | provider status shows cloud unavailable for empty placeholders; local chat still works |
| [#1097](https://github.com/CambrianTech/continuum/issues/1097) API-key merge commands | P0 | extend the existing `ai/key/*` command surface for encrypted config sharing over trusted grid/Tailscale nodes; no loose file copying and no browser exposure | two-node test shares selected keys, decrypts only on trusted target, and never logs values |
| [#1098](https://github.com/CambrianTech/continuum/issues/1098) routed command program substrate | P0 | consolidate bounded multi-command execution on top of `grid/send`, `GridInterceptor`, and `grid/route` so secrets and forge use the same path | one local-grid test runs a redacted `ai/key/*` program; one forge preflight routes through the same envelope |
| #860 config.env as directory | P1 | keep setup file/dir creation idempotent and typed | setup test catches file-vs-dir mismatch |

Implementation status:

- Shared `ai/key` base types now exist for provider identity, sync intent,
  target nodes, dry-run, synced state, and merge-plan id.
- Existing `ai/key/save`, `ai/key/remove`, and `ai/key/test` shared types
  inherit the base. Runtime sync behavior is intentionally not claimed until the
  routed reconciliation path exists.
- `ai/key/status` is generated from `src/generator/specs/ai-key-status.json`
  and returns only redacted provider/key/source/configured/fingerprint metadata.
- `grid/send` is the explicit routed command envelope; `GridInterceptor` is the
  transparent `Commands.execute()` remote path; `grid/route` is the dry-run
  routing/debug primitive.

Command shape:

- Existing `ai/key/save`: write one key through `SecretManager` to `$HOME/.continuum/config.env` or the platform vault; command echo and logs must redact values.
- Existing `ai/key/remove`: remove one key through `SecretManager`.
- Existing `ai/key/test`: validate a candidate or stored provider key.
- Existing `ai/providers/status`: provider-facing availability view.
- `ai/key/status`: list configured key names, source path, empty placeholders, fingerprints, and provider health without values.
- `ai/key/diff`: compare redacted key revisions across selected target nodes and produce a merge plan without values.
- `ai/key/apply-merge`: apply an approved merge plan through `SecretManager`; conflicts require owner/persona approval and never auto-overwrite a newer local key.

Rules:

- Empty placeholders such as `DEEPSEEK_API_KEY=` are documentation, not availability.
- Local mode must work with zero API keys.
- Cloud personas are eligible only when their required key is non-empty and the provider health check is not expired/failed.
- Config sharing is an owner/trusted-node command. It should use grid identity plus transport encryption, then persist through `SecretManager` so all runtimes see one source.
- Remote/grid execution is command routing context, not a namespace. The capability name stays stable while target environment changes.
- Fresh install and Carl smoke must pass with public model downloads and no `HF_TOKEN`; token-dependent private/gated/factory upload paths are optional later setup.

### 2. GPU Runtime Stability

**Goal**: GPU resource failures degrade or recover; they do not brick the session.

| Issue | Priority | Direction | Test gate |
|---|---:|---|---|
| #1048 mmproj/mtmd init mutex | P0 | one mtmd-capable backend may enter Metal pipeline/mmproj init at a time | Rust concurrency test: parallel vision/audio backend init serializes and all callers receive a sane result |
| #1050 backend recovery state machine | P0 | represent backend as `Healthy`, `Initializing`, `Recovering`, `Dead`, `Unavailable`; recover/drop/recreate on OOM/dead backend | Rust test with injected backend failure recovers or reports `Unavailable`, never hangs |
| #960 Mac Metal throughput 5-7 tok/s | P0 | measure and fix actual GPU path; do not route through slow CPU-shaped fallback | benchmark shows expected Metal path and records tok/s |
| #964 ONNX Runtime CPU spike | P0 | enforce Metal/GPU provider selection for fastembed/TTS/STT/vision bridge or fail loud | test/log proves provider is Metal/GPU; CPU fallback is explicit |
| #948 DMR concurrency failure | P1 | add bounded request scheduling/backpressure around DMR | 4+ persona concurrency test passes without reqwest cascade |
| #915 Kokoro ONNX deadlock | P1 | isolate session creation and apply GPU provider lifecycle rules | regression test for TTS startup no deadlock |
| #918 multimodal-native worker | P2 | after lifecycle is safe, collapse voice chain latency | live voice turn benchmark |

Rust targets:

- `src/workers/continuum-core/src/inference/`
- `src/workers/llama/src/mtmd.rs`
- `src/workers/continuum-core/src/gpu/`
- `src/workers/continuum-core/src/live/audio/`

Do not fix these in TypeScript. TS may display state and call commands; it must not own backend lifecycle.

### 3. Rust Persona Runtime And Cognition

**Goal**: personas can run, replay, and be embedded without Node acting as the brain.

| Issue / doc | Priority | Direction | Test gate |
|---|---:|---|---|
| #969 migrate tool agent loop to Rust | P0 | move persona/tool loop behavior out of TS | net-negative TS cognition lines and Rust replay test |
| #909 local persona tool execution | P0 | wire local DMR/Candle tool execution through Rust path | local persona can call a tool without cloud path |
| #958 DMR repetition penalty / echo | P0 | fix generation config at adapter layer | replay/conversation test proves no verbatim echo loop |
| #837 raw tool-call XML leak | P1 | output rendering and model post-processing both need tests | fixture with tool markup renders/filters correctly |
| #970 missing image marker | P1 | ensure media markers are role/content correct in Rust prompt assembly | vision replay fixture includes media marker |
| docs/architecture/PERSONA-AS-RUST-LIBRARY-PLAN.md | P0 reference | keep as detailed architecture, but alpha doc owns sequencing | cargo tests run without Node |
| docs/architecture/PERSONA-COGNITION-RUST-MIGRATION.md | P0 reference | enforce "Rust = verbs, TS = nouns/shims" | PRs touching cognition show TS line reduction |

Near-term PR sequence:

1. **PR: Rust persona trace/recorder validation**
   - issue: file/link if not already present
   - scope: Rust fixture capture and replay for a chat turn
   - tests: `cargo test --package continuum-core persona`
2. **PR: Rust tool loop migration**
   - issue: #969
   - scope: shrink TS tool-agent loop to a shim
   - tests: Rust tool loop unit/integration test; net-negative TS cognition lines
3. **PR: local persona tool execution**
   - issue: #909
   - scope: local model path can execute tools without cloud-only assumptions
   - tests: local persona tool-call replay; no browser required

### 4. Unified Paging And Pressure Control

**Goal**: support many personas and modalities by paging resources coherently instead of over-allocating and hoping.

| Issue / doc | Priority | Direction | Test gate |
|---|---:|---|---|
| docs/architecture/UNIFIED-PAGING.md | P0 reference | `PagedResourcePool` is the primitive; migrate consumers one at a time | pool tests plus consumer-specific tests |
| docs/architecture/PERSONA-CONTEXT-PAGING.md | P0 reference | KV/persona context paging policy | tests prove bounded memory with multiple personas |
| #1049 PressureBroker admission gate | P0 | broker must deny unsafe allocations, not just observe them | admission test refuses second unsafe mtmd/backend creation |
| #1051 MtmdContext pooling | P0 | reuse multimodal context instead of fresh multi-GB allocation per image/frame | replay test avoids repeated context allocation |
| #945 data/query memory leak | P0 | apply resource attribution and leak tests | load test stays within memory envelope |
| #944 embedding loop/cache misses | P1 | migrate embedding cache to shared paging primitive | repeated index pass has cache hits and bounded memory |
| #911 16GB MacBook Air | P1 | define reduced alpha profile with strict budgets | 16GB profile starts and reports disabled features honestly |

Model selection contract:

- Callers request capabilities, not model IDs.
- Discovery and admission are separate: discovery builds the catalog of model
  artifacts, modalities, context windows, templates, quantizations, and backend
  requirements; admission chooses the best viable candidate for the current
  machine state and request.
- The catalog is a curated whitelist, not arbitrary Hugging Face passthrough.
  Candidate discovery may crawl/search HF offline or through foundry commands,
  but runtime selection only admits vetted rows with known templates, license,
  backend compatibility, memory estimates, modality metadata, and forge status.
- Foundry output flows back into the same registry: `candidate` -> `vetted` ->
  `forged` -> `published`, with Sentinel/foundry jobs updating metadata rather
  than TS code hardcoding new model names.
- Provider identity must be typed. Runtime local chat is `LocalRuntime`
  (llama.cpp/Qwen through our adapter stack), cloud providers are explicit
  external identities, and Candle is not an inference provider for persona chat.
  Export this with `ts-rs` so TS seed/config/user paths cannot invent free-form
  provider strings.
- Request fields should be typed: `taskKind`, `minIntelligence`, `modalities`, `toolSupport`, `minContextTokens`, `latencyClass`, `qualityClass`, `memoryBudget`, `gpuRequired`, `familyAllowlist`, `familyPreference`, and `explicitOverride`.
- Constraint syntax should feel like semver where it helps: exact pins for repro, `>=` for minimum intelligence/capability, `~qwen3.5` for near-family preference, ranges for context/latency/memory, and hard allow/deny lists for safety.
- Rust registry/admission returns the selected provider/model/artifact plus explanation: why selected, why alternatives were rejected, projected VRAM/RAM/KV/LoRA footprint, and whether the choice is degraded.
- Persona seed stores intent (`local-default`, `vision-default`, future typed capability refs), not hardcoded model strings.
- TS may display selection state; it must not invent fallback models.

Implementation order:

1. PressureBroker admission gate.
2. Backend/mmproj lifecycle integration.
3. First consumer migration: embedding cache or mtmd context pool.
4. KV/persona context policy.
5. LoRA adapter paging.

### 5. Docker Modularization

**Goal**: Docker should isolate services and make failures obvious; it must not become a bulk mess that hides Rust/Node/UI problems.

| Issue | Priority | Direction | Test gate |
|---|---:|---|---|
| #892 CUDA Docker path bypasses our substrate | P0 | GPU profile must run Continuum runtime or explicitly documented external service, not orphaned upstream server | GPU compose path exercises our adapter/router health |
| #955 floating CUDA image tag | P0 | pin digest or controlled version | CI verifies pinned image |
| #834 / #776 image size | P1 | split build/runtime layers; remove unused Node/vendor bulk from runtime images | image size trend published in PR |
| #796 Docker compose E2E live mode/grid | P1 | profile-based compose tests, not one giant default | compose profile tests pass independently |
| #908 Windows npm start should route through docker compose | P1 | Windows dev path should use the supported Docker/WSL path | Windows smoke reaches GPU-backed inference |
| #860 config.env as directory | P1 | keep setup file/dir creation idempotent and typed | setup test catches file-vs-dir mismatch |
| #859 compose pull hangs in Git Bash | P1 | Windows shell path needs bounded timeout and clear next step | install does not hang indefinitely |

Docker shape:

- `continuum-core`: Rust runtime, GPU adapters, IPC/HTTP surface, no UI.
- `node-server`: thin command/websocket bridge; no persona cognition logic.
- `widget-server`: static/browser UI only.
- `model-init`: explicit model prewarm/download with progress.
- Optional profiles: `ui`, `grid`, `gpu`, `live`, `forge`, `devtools`.

Health checks:

- Process exists is not health.
- Core health means IPC responds and required GPU/model capability is ready or explicitly unavailable.
- Node health means it can reach core or reports degraded with cause.
- Widget health means static UI and WebSocket proxy are reachable.
- Model health means expected model is present and GPU-serving path is known.

### 6. UI And Realtime Stability

**Goal**: the browser should reflect reality and recover without manual localStorage/database cleanup.

| Issue / PR | Priority | Direction | Test gate |
|---|---:|---|---|
| #961 / PR #1047 | P0 | stale General tab canonicalization merged to canary | browser reload with stale persisted state collapses to one General tab |
| #793 Node does not reconnect when Rust core restarts | P0 | request pipeline must drain/recreate after core restart | kill/restart core test: next command succeeds |
| #794 AI messages not realtime | P0 | event bridge forwards AI senders immediately | browser sees AI message without refresh |
| #962 / #1113 | P1 | AIRC transcript cursor + bounded Continuum projection + IntersectionObserver | scroll-up test loads older messages without ORM live-bus fanout |
| #773 browser WS reconnect | P1 | reconnect/rebind without manual refresh | browser survives server restart |
| #785 URL scheme | P1 | one consistent route rule, zero special cases | stale room URL redirects/recovers deterministically |
| #783 stale room URLs | P1 | stale URLs show recovery path, not broken tab | route test |

TS is acceptable here because this is UI/session state. Still, data validation and canonicalization should use existing routing/entity APIs, not hardcoded UUID/string hacks.

### 7. AIRC And Continuum Internal AI Collaboration

**Goal**: Continuum personas and external coding agents can collaborate through the same room/bus without humans relaying messages.

| Issue / PR | Priority | Direction | Test gate |
|---|---:|---|---|
| #967 | P0 | expose personas as AIRC peers | persona receives AIRC room message and replies through Continuum chat |
| [#1167](https://github.com/CambrianTech/continuum/issues/1167) AIRC/Rust agent flywheel | P0 | treat AIRC as the agent development substrate and Continuum Rust/Sentinel as the no-Node execution plane | approved agent claims queue card, runs Rust/Sentinel command path without Node, opens PR to canary, and close-merged removes the card |
| PR #1046 | P0 | AIRC bridge harness | bridge protocol test and live room smoke |
| #856 grid event streaming | P1 | persistent event channels between nodes | cross-node event smoke, no polling-only path |
| #798 route inference through mesh | P2 | use grid routing for GPU-heavy inference | command from non-GPU node routes to GPU node |

Design rule:

- AIRC is the collaboration transcript and message/file substrate.
- Continuum owns runtime inputs, generated command execution, persona behavior,
  UI state, and bounded durable projections. It should not use ORM writes and
  broad IPC fanout as the live chat bus.
- The bridge should map messages/events without requiring agents to shell out to
  `jtag chat/send` manually. Long term, Carl/browser chat smoke should validate
  through AIRC transcript APIs rather than JTAG chat commands.
- Protocol tests must run without a browser.

## PR Roadmap To Alpha

| Order | Branch | Base | Issue(s) | Deliverable | Required validation before canary merge |
|---:|---|---|---|---|---|
| 1 | `codex/alpha-gap-stability-plan` | `canary` | planning doc | this document; shared execution map | docs lint/readability, AIRC review |
| 2 | `fix/gpu-backend-lifecycle` | `canary` | #1048, #1050, #960, #964 | mutex + backend state/recovery | Contract TDD for injected failure; Residency VDD for GPU provider; Performance VDD for tok/s |
| 3 | `feature/grid-config-sync` | `canary` | config single-source, grid config sync | encrypted config status/export/import/sync commands | Contract TDD for config shape; Cross-platform VDD for two-node encrypted config sync; provider status remains truthful |
| 4 | `fix/docker-alpha-profiles` | `canary` | #892, #955, #834, #776, #796 | modular Docker profile cleanup | Failure TDD for health boundaries; Cross-platform VDD for compose profiles; image size report |
| 5 | `feature/persona-rust-replay` | `canary` | #969, #909 | Rust persona replay/tool-loop foundation | Contract TDD via `cargo test`; Accuracy VDD via replay fixture and repeated-run stability; net-negative TS cognition lines |
| 6 | `feature/pressure-broker-gate` | `canary` | #1049, #1051, #945, #944 | admission gate + first resource consumer | Contract TDD for admission decisions; Resource/Residency VDD for memory envelope; no Node required |
| 7 | `fix/realtime-core-reconnect` | `canary` | #793, #794, #773 | core restart + realtime browser recovery | Failure TDD for killed core; Timing VDD for reconnect/event timestamps; UX VDD for browser receive |
| 8 | `feature/airc-persona-peer` | `canary` | #967, PR #1046 | Continuum persona as AIRC participant | Protocol TDD for bridge mapping; Timing VDD for round trip; AIRC -> Continuum -> AIRC live smoke |
| 9 | `test/fresh-install-e2e` | `canary` | #770, #1006-#1008, #983 | install validation matrix | Cross-platform VDD for Mac/Windows logs; Failure TDD for missing network/Docker/GPU; no silent waits |

This order can change when a blocker is discovered, but changes must be made in this document and on the issue/PR thread, not only in chat.

## VDD/TDD Operating Loop

Continuum cannot be validated by integration tests alone. It has ML quality, GPU residency, timing, and recovery requirements that can regress while normal tests stay green. The alpha loop is therefore **TDD + VDD**:

- **TDD**: deterministic unit, integration, and protocol tests that prove contracts and failure modes.
- **VDD**: validation-driven development for measured behavior: latency, throughput, GPU provider, memory pressure, model accuracy, recovery time, and live UX.

Every alpha PR must choose its validation class up front. A PR may use more than one class, but it may not claim broad stability from a single browser smoke or Docker boot.

| Class | Proves | Typical evidence | Examples |
|---|---|---|---|
| Contract TDD | API/state/protocol invariants | unit test, Rust test, type-level regression | `PageState.clear()` emits `null`; pressure gate refuses unsafe allocation |
| Failure TDD | known failure recovers or fails loud | injected fault test, stale fixture, bounded timeout | dead core reconnect, stale room ID, missing model, gone channel |
| Performance VDD | speed stays inside alpha budget | benchmark output with baseline delta | tok/s, first-token latency, boot time, chat round-trip |
| Resource VDD | memory, handles, queues, and cache growth stay bounded over time | soak/load output, monotonic-growth check, resource envelope delta | no ORM/query leak over N iterations; KV cache stays under budget |
| Accuracy VDD | model output quality and repeatability stay acceptable | replay fixture score, golden semantic check, repeated-run variance, human spot-check note | no echo loop, tool-call XML stripped, vision marker preserved, stable tool choice over N runs |
| Residency VDD | correct hardware path is used | provider log, GPU counter, no silent CPU fallback | Metal/CUDA provider active; CPU fallback logged as degraded |
| Timing VDD | async/realtime behavior is observed | event timestamp trace, reconnect timing, race replay | AI message renders without refresh; cold start emits progress |
| UX VDD | user-visible workflow works | browser screenshot/log, concise manual steps | close all tabs -> empty center; `/chat/general` -> one tab |
| Cross-platform VDD | Mac/Windows/Linux path works | platform logs from canary, issue/PR comment | WSL install, Mac Metal, Docker profile |

### PR Validation Template

Each PR body should include this block, filled in concretely:

```text
Validation class:
Issue(s):
Core contract test:
Failure injection / stale fixture:
Performance/latency budget:
Resource/memory evidence:
Accuracy/replay evidence:
GPU/provider evidence:
Browser/UX evidence:
Migration evidence:
Platform coverage:
Known gaps:
Canary agents/humans asked to test:
Canary ACK/BLOCKER evidence:
```

Rules:

1. Every template line is required; use `n/a — <reason>` when a field does not apply.
2. Core behavior needs a fast non-browser proof when feasible.
3. Browser tests prove browser responsibilities only.
4. Docker tests prove packaging and service boundaries, not core algorithm correctness.
5. ML behavior needs replay fixtures or scored checks, not only "the command returned"; variance-sensitive paths need repeated-run evidence.
6. Timing-sensitive behavior needs measured timestamps or bounded waits.
7. GPU-critical behavior must prove provider/residency or fail as degraded. CPU fallback is never silent.
8. Memory/resource behavior needs a bounded-envelope or leak test when touching caches, pools, queues, ORM cursors, model contexts, or long-lived handles.
9. State/data shape changes need migration evidence against old persisted state, or `n/a — no state/schema change`.
10. Install and postinstall must be bounded, explicit, and resumable. Large downloads must not hide inside unrelated validation.
11. Canary peer testing must close the loop: agents/humans reply with `ACK` or `BLOCKER` plus measured evidence, and the PR records or links that evidence.

## Test Strategy

### Rust-first tests

Use these before Docker/browser validation:

```bash
cargo test --manifest-path src/workers/continuum-core/Cargo.toml
cargo test --manifest-path src/workers/llama/Cargo.toml
```

Add focused tests for:

- backend lifecycle and recovery
- mmproj init serialization
- persona replay fixtures
- paging pool consumers
- pressure admission decisions
- local tool execution

### Docker tests

Docker tests are service/profile tests, not proof that core logic is correct:

```bash
docker compose up -d postgres continuum-core node-server
docker compose --profile ui up -d widget-server
docker compose --profile gpu up -d
docker compose --profile live up -d
```

Each profile needs a bounded smoke command and a log artifact.

### Browser tests

Use browser tests only for browser responsibilities:

- tab restore and route canonicalization
- WebSocket reconnect
- realtime message rendering
- UI state after data reseed

The stale General bug belongs here; backend lifecycle does not.

### AIRC collaboration tests

Use AIRC for live coordination, but also create protocol tests:

- external agent sends AIRC message into room
- Continuum bridge records it as chat event
- persona responds
- response mirrors back to AIRC
- duplicate/replay protection is verified
- approved peer receives `.airc/` rules plus a concise issue/PR queue digest
- idle peer receives `nudge`, polls for unowned/stale work, and either claims a
  card or reports why it cannot
- local-model persona and cloud agent both operate on the same GitHub-backed
  queue without assuming separate GitHub users
- scrollback/history fetch reads from AIRC transcript cursors, while Continuum
  storage only receives bounded projections
- file attachments flow through AIRC file/manifest events and enter Continuum
  only as pointers, cache handles, memory candidates, or UI projections

## Merge Gates

Every alpha PR must answer:

- Which issue does this advance?
- Why does this belong in Rust, TS, Docker, or docs?
- Which validation class(es) does this PR use: Contract TDD, Failure TDD, Performance VDD, Accuracy VDD, Residency VDD, Timing VDD, UX VDD, Cross-platform VDD?
- What command proves the core behavior without browser/Node?
- What canary validation was run, and what measured evidence was attached?
- What platforms were covered?
- What remains untested?
- Did it reduce Node/TS logic or at least avoid adding new TS logic?
- Did it avoid silent fallback/silent success?

Main promotion requires:

- canary contains the PR
- canary has been tested by at least one other agent/human where practical
- failures are linked to issues, not buried in chat
- the promotion PR lists included canary commits and validation evidence

## Document Map

This document owns execution order and alpha gates. Detailed architecture
remains in the supporting docs below. ALPHA-GAP-ANALYSIS is the beacon; the
supporting docs are the specifications its lanes converge on.

**Runtime substrate (load-bearing, read before any runtime/cognition PR):**

- [CBAR Substrate Architecture](../architecture/CBAR-SUBSTRATE-ARCHITECTURE.md)
  — the RTOS-style runtime contract every Rust module/adapter inherits.
  Substrate provides bounded queues, dependency wakeups, cadence/pressure
  gates, automatic VDD/TDD evidence hooks, and ts-rs exported contracts.
  Module authors declare subscriptions/lane/cadence and write the small piece
  of actual work — everything else is inherited "for free." Lanes C/D/E in
  this document converge on this substrate.
- [Genome, Foundry, Sentinel-AI](../architecture/GENOME-FOUNDRY-SENTINEL.md)
  — the artifact-sharing economy on top of the CBAR substrate. Tiered genome
  cache (L1–L5), `WorkingSetManager` + page faults, foundry (JIT for SOTA
  absorption), sentinel-AI (profile-guided optimization from lived traces),
  demand-aligned recall, composer + speculator, and the `SubstrateGovernor`
  (DVFS — same Rust code on MacBook Air and RTX 5090, different governor
  policy). Lane H converges on this doc.

**Cognition / persona migration:**

- [Persona-as-Rust-Library](../architecture/PERSONA-AS-RUST-LIBRARY-PLAN.md)
- [Persona Cognition Rust Migration](../architecture/PERSONA-COGNITION-RUST-MIGRATION.md)

**Memory / paging:**

- [Unified Paging](../architecture/UNIFIED-PAGING.md)
- [Persona Context Paging](../architecture/PERSONA-CONTEXT-PAGING.md)

**Model registry (source-of-truth references, code-side):**

- `src/shared/models.json` and `src/shared/ModelRegistry.ts`

**Grid / Docker / AIRC:**

- [Docker Node Architecture](../grid/DOCKER-NODE-ARCHITECTURE.md)
- [Grid Architecture](../grid/GRID-ARCHITECTURE.md)
- [AIRC Continuum Bridge](../grid/AIRC-CONTINUUM-BRIDGE.md)
- repo-local AIRC pilot files under `../../.airc/`
- CambrianTech/airc#559 and CambrianTech/airc#562 for public entry, approval,
  queue, and nudge behavior

If those docs disagree with this one on sequence, update this one first or
explicitly revise the sequence in the PR. If they disagree with this one on
the substrate contract (concurrency, scheduling, memory, pressure, telemetry,
artifact handles), defer to CBAR-SUBSTRATE-ARCHITECTURE.md and reconcile
in a follow-up.

## Immediate Next Actions (Refreshed 2026-05-16, second update)

Ordered by alpha leverage. **Items 6, 8 (PR-1), and parts of 2/3/9 closed since
the first refresh** — see the closeout summary at the end of this section.
The implementing agent (claude-tab-1, continuum-scope) is **ready for the next
slice** and explicitly read MODULE-CATALOG to pick what fits. See
[MODULE-CATALOG.md](../architecture/MODULE-CATALOG.md) §"Next Modules To Build"
for the ranked-by-buildability work queue.

If you are picking this up, claim explicitly on AIRC before you start.

1. **Claim Lane D (CBAR persona runtime frame).** Still the highest-leverage
   unstarted lane. PressureBroker (Lane E) and the inbox coalescing pattern
   both presupposed `RuntimeFrame` / `CognitionTurnFrame`. Lane H's governor
   (alpha-floor) doesn't strictly depend on Lane D, but the persona-cognition
   module catalog entry does — and that's the cognition core. Spec: see
   [CBAR Substrate Architecture](../architecture/CBAR-SUBSTRATE-ARCHITECTURE.md)
   §"The Dataflow Contract" + §"Runtime Frame", plus
   [PERSONA-COGNITION-CONTRACT.md](../architecture/PERSONA-COGNITION-CONTRACT.md)
   §"Core Surfaces" for the full contract.

2. **Land the universal-trait "for free" triplet.** Unchanged. Codex's
   derive-macro acceptance gate (continuum#1324) added five hard gates the
   macro must clear before landing: thin, contract-preserving, inspectable,
   tested, no hidden behavior. Spec: CBAR-SUBSTRATE §"The 'For Free' Triplet"
   + §"Acceptance Criteria For Substrate-Done".

3. **Lane H groundwork: substrate-governor.** Continuum#1335 shipped the
   hardware probe + `HardwareProfile`. Remaining is the policy TOML loader,
   the cascade state machine (six steps with hysteresis), and the
   pressure-signal subscriber. Spec:
   [GENOME-FOUNDRY-SENTINEL.md](../architecture/GENOME-FOUNDRY-SENTINEL.md)
   Part 11. About 400 LoC in 3 PRs per MODULE-CATALOG §"Next Modules To Build"
   entry #5. **This is currently the #5 buildable module by leverage** —
   the four ahead of it (audit-recorder, threat-detector,
   working-set-manager, demand-aligned-recall) are smaller and unblock more.

4. **Claim Lane F mechanical ratchet PR.** Still open. The TS deletion
   progress from prior sessions (~2500 LOC across 8 cognition PRs)
   is reversible until the CI gate exists. Lane F PR sequence step 1
   (`persona-ts-ratchet-script`) is small and unblocks step 2 (CI
   enforcement). claude-tab-1 (continuum-scope) signaled willingness to
   take this in a prior airc broadcast.

5. **Bind Lane C `vdd-report-command`.** Still open. Structured
   `RuntimeMetric` events already emit from inference paths, but VDD is
   still read from logs because the report command was not bound. Small;
   unblocks every PR's "VDD: tokens/sec improved from X → Y" claim.

6. ~~**Widen the no-CPU-fallback contract test.**~~ **DONE.** Continuum#1341
   widened `no_cpu_fallback_contract.rs` to cover the Candle-side paths
   (inference-grpc/model.rs, orpheus.rs, residency.rs, enforcement.rs,
   llamacpp_adapter.rs, hw_probe.rs). 6 new assertions; 9 tests passing.
   Locks in PIECE-5's whole stack at type-checking time.

7. **Lane B follow-ups: capability-visible health + tier-pool eviction.**
   Unchanged. #1297 landed the Docker tier stats surface; #1238 / #1239
   still open. Both should consume the Lane A registry artifact contract.

8. ~~**GRID-INFERENCE-ROUTING.**~~ **PR-1 SHIPPED.** Continuum#1315 merged
   (inference capability announcer + probe + registry). PR-2 (routing
   decision) and PR-3 (eviction-on-grid policy) remain. Owner: airc-8a5e
   per prior claim.

9. **Lane H follow-on after substrate-governor (#3 above).** Per
   MODULE-CATALOG §"Next Modules To Build", after the governor lands:
   - `audit-recorder` (#1 in the catalog's queue) — small, no dependencies,
     unblocks the trace-bus landing place for typed events.
   - `threat-detector` (#2 in the queue) — depends on audit-recorder;
     unlocks `PersonaDecision::Decline { AdversarialPattern }`.
   - `working-set-manager` (#3 in the queue) — substrate's MMU; depends on
     governor types + PressureBroker (shipped).
   - `demand-aligned-recall` (#4 in the queue) — central API; mechanical
     given working-set-manager.

   The MODULE-CATALOG entries name dependency state, estimated PRs + LoC,
   and concrete acceptance criteria. This is the substrate-side implementation
   path; the cognition core lands on top once these stabilize.

10. **CBAR-PIECE-5 + PIECE-8 closed end-to-end.** ✓
    - PIECE-5 PR-1 gate types (#1331 MERGED)
    - PIECE-5 PR-2 GGUF loader (#1333 MERGED)
    - PIECE-5 PR-3 hardware probe (#1335 MERGED)
    - PIECE-5 PR-4 adapter wiring (#1338 MERGED, codex co-authored)
    - PIECE-8 inference-grpc hardcoded-clamps deletion (#1340 MERGED)
    The `inference-grpc/main.rs::get_num_workers()` anti-pattern was
    partially addressed via #1340 (hardcoded clamps removed); full
    PressureBroker-lease integration remains as a Lane E follow-up tied
    to the broker IPC design.

11. **Doc refresh closed.** ✓ The whole architecture doc family is now in
    open or merged PRs:
    - `CBAR-SUBSTRATE-ARCHITECTURE.md` — continuum#1324, deepened with
      dataflow contract, zero-overhead frame entry, spatiotemporal
      reprojection toolkit.
    - `GENOME-FOUNDRY-SENTINEL.md` — continuum#1327, all eleven substantive
      parts at engineer-buildable depth (Parts 5, 6, 7, 8, 9, 10, 11 all
      fully spec'd with Rust types, algorithms, acceptance criteria, and
      per-anchor performance budgets).
    - `PERSONA-COGNITION-CONTRACT.md` — continuum#1332, reactive cognition
      contract with 14 substrate-enforced invariants.
    - `PERSONA-THOUGHT-PROCESS.md` — continuum#1337, proactive thought
      surface + concrete worked example (delphi persona, 7 reasoning steps,
      ~23s LLM time spread across 9 wall-clock hours to crystallize a
      substantive insight on Q4_K Qwen3-7B).
    - `MODULE-CATALOG.md` — continuum#1336, every Continuum concern as a
      focused module + "Next Modules To Build" ranked work queue.
    - `CONTINUUM-ARCHITECTURE.md`, `CONTINUUM-VISION.md`, `CLAUDE.md` +
      `UNIVERSAL-*.md` deprecation pointers — all merged via #1317, #1320,
      #1329.

### Closeout Summary

What's done since the first refresh:
- 6 closed: ALPHA-GAP refresh, CONTINUUM-ARCHITECTURE refresh,
  CONTINUUM-VISION refresh, stale-section pointers, CBAR-PIECE-5
  end-to-end (4 PRs), PIECE-8 inference-grpc clamps, no-CPU-fallback
  contract widening.
- 5 open architecture-doc PRs ready for review: #1324 CBAR-SUBSTRATE,
  #1327 GENOME-FOUNDRY-SENTINEL, #1332 PERSONA-COGNITION-CONTRACT,
  #1336 MODULE-CATALOG, #1337 PERSONA-THOUGHT-PROCESS.
- 2 open coordination-substrate PRs on airc: #642 manager-role,
  #643 lane-kanban-protocol.

What's queued (in MODULE-CATALOG order): audit-recorder, threat-detector,
working-set-manager, demand-aligned-recall, substrate-governor. After those,
the cognition core (persona-cognition, inference-llm, composer, speculator,
reprojection-service) becomes the next-tier work.

The architectural roadmap is now substantially backed by code-shaped specs.
Doc-driven development is working: doc spec → implementing agent picks up →
ships PR → next spec referenced.
