# Module Catalog: Every Concern As A Focused Module

> **Premise** (Joel, 2026-05-16): *"The most effective designs are fundamentally simple. Every concern is hundreds of lines, and yet everything is performant. How do we make the others perform like CBAR in Continuum?"*
>
> **Companion to** [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) (the substrate floor), [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) (the artifact economy), and [PERSONA-COGNITION-CONTRACT.md](PERSONA-COGNITION-CONTRACT.md) (the cognition contract).
>
> **Status.** Design proposal. Per-module Rust files target `src/workers/continuum-core/src/` under the indicated directories. Implementation lands per ALPHA-GAP lanes.

This document is the **catalog**. Every Continuum concern — RAG, persona, memory, voice, vision, inference, sentinel, foundry, federation, live, AIRC bridge, governor, and the rest — shown as a focused `RuntimeModule`. Each entry names what the module *needs* (subscriptions), what it *provides* (emissions), its resource class + target, its cadence, a screen-or-less handler sketch, and an honest line-count estimate.

The architectural claim: when the substrate handles the rest — concurrency, scheduling, pressure response, telemetry, replay, lifecycle, reprojection, demand-aligned recall, governor-mediated sizing — **every concern reduces to a few hundred lines and is performant by inheritance.** That is what "fundamentally simple" means in production.

## The Recipe (One Page)

Every module in this catalog follows the same five-line recipe:

```rust
#[derive(RuntimeModule)]
#[runtime(name = "X", lane = ResourceClass::Y, target = TargetSilicon::Z, cadence = CadencePolicy::W)]
pub struct X { /* small private state */ }

#[runtime::handler]
impl RuntimeModule for X {
    fn subscriptions(&self) -> &[ArtifactSelector] { &[ArtifactSelector::Foo] }
    fn emissions(&self)     -> &[EmissionSelector] { &[EmissionSelector::Bar] }
    async fn handle_frame(&self, frame: Arc<RuntimeFrame>, ctx: &ModuleContext) -> ModuleResult {
        // small piece of actual work — the rest is inherited
    }
}
```

The substrate gives every module:

- Wakeups on relevant subscriptions only (no polling)
- Tokio/dedicated-thread choice by `ResourceClass`
- `PressureBroker` admission + `CognitionLease`
- Memory / CPU / device pressure response
- Concurrency cap from `ResourceClass`, never per-module
- Coalescing of duplicate artifact arrivals
- Spans, timing, structured logging, VDD record emission
- Typed failure path; `?` propagates to `ModuleResult::Failed`
- Replay test fixture (scaffold generator drops one)
- ts-rs exported contract for UI / commands
- Lifecycle: `Gestation → Active → Senescent → Apoptotic`

A module author writes the five-line recipe and a small handler body. **Everything else is inherited.** Hundreds of lines, performant. That is the catalog's entire architectural bet.

---

## I. Cognition Concerns

### `persona-cognition`

The persona's per-turn cognition: read inbox, assemble working memory, decide, emit. The contract is specified in detail in [PERSONA-COGNITION-CONTRACT.md](PERSONA-COGNITION-CONTRACT.md); this entry is the module that implements it.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/cognition/persona_module.rs` |
| Lane | `ResourceClass::LocalGeneration` |
| Target | `TargetSilicon::Gpu` (Cpu when no GPU lease available, with reprojection) |
| Cadence | `OnReady` (inbox not empty + composition warm) |
| Subscriptions | `[InboxedFrame, ConsentScopeChange, IdentityStateUpdate]` |
| Emissions | `[PersonaDecisionEmitted, TurnReplayRecord, RefusalAudit]` |
| Estimated LoC | ~350 lines (handler + decision dispatch + replay record assembly) |

Handler sketch:

```rust
async fn handle_frame(&self, frame: Arc<RuntimeFrame>, ctx: &ModuleContext) -> ModuleResult {
    let inbox_entry = frame.inbox_entry_for(self.persona).await?;
    let budget      = ctx.budget_for(self.persona, &frame);
    let assembly    = ctx.working_memory_assembler().assemble(self.persona, frame.clone(), budget).await?;
    let pool        = ctx.recall().recall(&assembly.query(), &assembly.context()).await?;
    let composition = ctx.composer().compose(&pool, &assembly.constraints())?;
    let decision    = self.decide(&assembly, &composition).await?;
    let record      = TurnReplayRecord::new(&frame, &assembly, &pool, &composition, &decision);
    ctx.emit_signed(EmissionSelector::TurnReplayRecord, record).await?;
    if let PersonaDecision::Decline { ref reason, .. } = decision {
        ctx.emit(EmissionSelector::RefusalAudit, reason.clone()).await?;
    }
    ctx.emit(EmissionSelector::PersonaDecisionEmitted, decision).await?;
    ModuleResult::ok()
}
```

### `rag-composer`

Build a ranked context bundle from sources for one persona turn. Generic over `RagSource` (conversation, memory, identity, awareness, tool-use, ...).

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/cognition/rag/composer.rs` |
| Lane | `ResourceClass::LocalGeneration` (sub-second turn-time work) |
| Target | `TargetSilicon::Cpu` (composition is glue; sources do their own GPU/disk) |
| Cadence | `OnReady` |
| Subscriptions | `[WorkingMemoryAssemblyRequest]` |
| Emissions | `[RAGContextComposed, RAGSourceFailed]` |
| Estimated LoC | ~250 lines (parallel source iter + budget allocator + composer) |

Handler sketch:

```rust
async fn handle_frame(&self, frame: Arc<RuntimeFrame>, ctx: &ModuleContext) -> ModuleResult {
    let req: RagComposeRequest = frame.rag_request().await?;
    let budgets = self.budget_alloc.allocate(req.total_budget, &req.applicable_sources);
    let sections: Vec<RagSection> = req.applicable_sources.par_iter()
        .zip(budgets.par_iter())
        .map(|(src, b)| src.load(req.persona, req.room, *b))
        .collect();
    let context = RagContext::compose(sections);
    ctx.emit(EmissionSelector::RAGContextComposed, context).await?;
    ModuleResult::ok()
}
```

### `hippocampus-consolidation`

Background module that runs during the consolidation phase (sleep). Reads recent traces, derives engrams, writes to `longterm.db`, emits for sentinel.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/cognition/hippocampus.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Cpu` (mmap + sqlite; no GPU) |
| Cadence | `OnConsolidationPhase` (governor-scheduled, idle/plugged-in by default) |
| Subscriptions | `[ConsolidationWindow, TraceBatch]` |
| Emissions | `[EngramWritten, ConsolidationReport]` |
| Estimated LoC | ~300 lines (clusterer + engram-pack + dedup against existing engrams) |

### `engram-recall`

Demand-aligned engram fetch for an active persona's working-memory assembly. Read-only over `longterm.db`.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/cognition/engram_recall.rs` |
| Lane | `ResourceClass::Memory` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `OnReady` |
| Subscriptions | `[EngramRecallRequest]` |
| Emissions | `[EngramPoolReturned]` |
| Estimated LoC | ~180 lines (query → ANN index → top-K → score → return) |

---

## II. Inference Concerns

### `inference-llm`

Local LLM generation. One model per instance; the substrate routes turns to it. Uses `CompositionPlan` from the genome doc.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/inference/llm_module.rs` |
| Lane | `ResourceClass::LocalGeneration` |
| Target | `TargetSilicon::Gpu` (hard requirement after #1314 fail-closed gate) |
| Cadence | `OnReady` |
| Subscriptions | `[InferenceRequest]` |
| Emissions | `[InferenceComplete, FirstTokenEmitted, ResidencyFault]` |
| Estimated LoC | ~400 lines (composition → tokenizer → llama.cpp invoke → token stream + reprojection metadata) |

### `inference-grpc-bridge`

Bridge from the gRPC inference server (existing `inference-grpc/` crate) into the substrate's typed dataflow. Pure adapter.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/inference/grpc_bridge.rs` |
| Lane | `ResourceClass::Io` |
| Target | `TargetSilicon::Network` |
| Cadence | `OnReady` |
| Subscriptions | `[InferenceRequest::Remote]` |
| Emissions | `[InferenceComplete, RemoteInferenceFailed]` |
| Estimated LoC | ~150 lines (Rust gRPC client + typed request/response mapping) |

### `embedding-batcher`

Coalesce multiple embedding requests across personas into one model invocation. Replaces the original "EmbeddingBatcher" sketch with a substrate-aware module.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/inference/embedding_batcher.rs` |
| Lane | `ResourceClass::Embedding` |
| Target | `TargetSilicon::Gpu` (Cpu fallback acceptable for embeddings — short batches) |
| Cadence | `OnBatchFullOrTimeout` (custom cadence — 8 requests OR 50ms) |
| Subscriptions | `[EmbeddingRequest]` |
| Emissions | `[EmbeddingComplete]` |
| Estimated LoC | ~200 lines (batch buffer + flush trigger + per-request response routing) |

### `composer`

Build a `CompositionPlan` from a `RankedPool` per the genome doc Part 8. Caches materialized compositions.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/inference/composer.rs` |
| Lane | `ResourceClass::LocalGeneration` |
| Target | `TargetSilicon::Cpu` (composition decisions are glue) |
| Cadence | `OnReady` |
| Subscriptions | `[RankedPool, CompositionInvalidated]` |
| Emissions | `[CompositionMaterialized, CompositionCacheHit]` |
| Estimated LoC | ~250 lines (rank → pick → weight → materialize) |

### `speculator`

Pre-compose likely-next plans + pre-fetch likely-next pages. Governor-tuned aggressiveness.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/inference/speculator.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Gpu` (when idle slack) |
| Cadence | `OnTurnStart` (speculative branches fire when a turn begins) |
| Subscriptions | `[TurnStarted, ConversationTrajectoryHint]` |
| Emissions | `[BranchPreMaterialized, SpeculationHit, SpeculationMiss]` |
| Estimated LoC | ~280 lines (branch generator + materializer + hit-rate tracker) |

---

## III. Sensory Concerns

### `vision-yolo`

Object detection on incoming video frames. Per-frame, GPU.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sensory/vision_yolo.rs` |
| Lane | `ResourceClass::Vision` |
| Target | `TargetSilicon::Gpu` |
| Cadence | `Realtime` |
| Subscriptions | `[RawFrame]` |
| Emissions | `[DetectedObjects, SceneStateUpdate]` |
| Estimated LoC | ~200 lines (frame extract → YOLO invoke → typed object emit) |

### `vision-segmentation`

Watershed / semantic segmentation. Lower cadence; results feed reprojection toolkit.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sensory/vision_segmentation.rs` |
| Lane | `ResourceClass::Vision` |
| Target | `TargetSilicon::Gpu` |
| Cadence | `Delayed { every_n_frames: 4 }` |
| Subscriptions | `[RawFrame]` |
| Emissions | `[WatershedSegments]` |
| Estimated LoC | ~220 lines |

### `vision-surface-normals`

CNN surface normals — slow but reprojected per Joel's CBAR pattern.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sensory/surface_normals.rs` |
| Lane | `ResourceClass::Vision` |
| Target | `TargetSilicon::Gpu` |
| Cadence | `OnReady` (waked by 3D-space-shift emission) |
| Subscriptions | `[NewPlanarGeometry, ThreeDSpaceShift]` |
| Emissions | `[SurfaceNormalsResult]` (`Reprojectable` impl) |
| Estimated LoC | ~250 lines (CNN invoke + Reprojectable impl with FeatureWarp + LineConstrained) |

### `voice-stt`

Streaming speech-to-text. Real-time per audio chunk.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sensory/voice_stt.rs` |
| Lane | `ResourceClass::Media` |
| Target | `TargetSilicon::Gpu` (Cpu fallback for short utterances) |
| Cadence | `Realtime` |
| Subscriptions | `[AudioChunk]` |
| Emissions | `[TranscriptionPartial, TranscriptionFinal]` |
| Estimated LoC | ~300 lines (whisper invoke + segment boundary detection + partial-emit) |

### `voice-tts`

Speech synthesis from text emissions.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sensory/voice_tts.rs` |
| Lane | `ResourceClass::Media` |
| Target | `TargetSilicon::Gpu` (piper / silero / orpheus) |
| Cadence | `OnReady` |
| Subscriptions | `[UtteranceToSpeak]` |
| Emissions | `[AudioFrame]` |
| Estimated LoC | ~250 lines |

### `voice-mixer`

Mix-minus audio routing across participants.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/live/mixer.rs` |
| Lane | `ResourceClass::Media` |
| Target | `TargetSilicon::Cpu` (SIMD-accelerated) |
| Cadence | `Realtime` |
| Subscriptions | `[AudioFrame::Multiple]` |
| Emissions | `[MixedAudioFrame::Multiple]` |
| Estimated LoC | ~200 lines |

### `voice-vad`

Two-stage voice activity detection.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sensory/voice_vad.rs` |
| Lane | `ResourceClass::Media` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `Realtime` |
| Subscriptions | `[AudioFrame]` |
| Emissions | `[VoiceActivityStart, VoiceActivityEnd]` |
| Estimated LoC | ~150 lines |

---

## IV. Genome / Foundry / Sentinel Concerns

(See [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) for the full contracts; here, each is a substrate module.)

### `foundry-absorber`

Pull a SOTA model, extract relevant artifacts, adapt, publish to genome pool.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/foundry/absorber.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Gpu` (training-style work; offline) |
| Cadence | `OnTrigger { trigger: SOTAUpdateAvailable }` |
| Subscriptions | `[SOTAUpdateAvailable, FoundryAbsorbRequest]` |
| Emissions | `[ImportedArtifactPublished, FoundryFailed]` |
| Estimated LoC | ~400 lines (HF/HF-API fetch + extract + adapt + provenance + publish) |

### `sentinel-observer`

Read every cognition trace; build outcome attributions. Cheap, continuous.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sentinel/observer.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `OnReady` (woken by every trace) |
| Subscriptions | `[TurnReplayRecord, Outcome]` |
| Emissions | `[ArtifactAttribution]` |
| Estimated LoC | ~250 lines |

### `sentinel-refiner`

Run during consolidation phase. Reads attributions, retrains hot LoRA layers, publishes refined artifacts.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sentinel/refiner.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Gpu` (training) |
| Cadence | `OnConsolidationPhase` |
| Subscriptions | `[ArtifactAttribution::Batch, ConsolidationWindow]` |
| Emissions | `[RefinedArtifactPublished, RefinementReport]` |
| Estimated LoC | ~450 lines (attribution → trainer setup → fine-tune step → publish + provenance) |

### `genome-tier-store`

One module per tier (`Fast`, `Warm`, `Bench`, `Cold`, `Frozen`). Trait-implementing storage backend with eviction policy. The module IS the `TierStore` trait implementation, registered as a runtime module so the substrate sees its events.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/genome/tier/{fast,warm,bench,cold,frozen}.rs` |
| Lane | per-tier (`Fast`/`Warm` → `ResourceClass::Memory`; `Bench` → `ResourceClass::Memory`; `Cold`/`Frozen` → `ResourceClass::Io`) |
| Target | per-tier |
| Cadence | `OnReady` |
| Subscriptions | `[PageInRequest, PageOutRequest, EvictionTrigger]` |
| Emissions | `[PageInComplete, PageOutComplete, EvictionRecord]` |
| Estimated LoC | ~150 lines per tier × 5 tiers = ~750 lines total (each tier is small) |

### `working-set-manager`

Per-persona working-set bookkeeping. Page faults, MMU-style permission checks.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/genome/working_set.rs` |
| Lane | `ResourceClass::Memory` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `OnReady` |
| Subscriptions | `[PageReference, CompositionPin]` |
| Emissions | `[PageFault, AccessDenied, WorkingSetSpill]` |
| Estimated LoC | ~280 lines |

### `demand-aligned-recall`

The central API every persona reaches for. Backed by the layered indexing (working-set / local / grid / federation catalogs).

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/genome/recall.rs` |
| Lane | `ResourceClass::Memory` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `OnReady` |
| Subscriptions | `[CapabilityQuery]` |
| Emissions | `[RankedPoolReturned, RecallFailed]` |
| Estimated LoC | ~320 lines (query → embed → 4-tier index lookup → score + rank) |

---

## V. Federation / Grid Concerns

### `federation-publisher`

Publish locally-refined artifacts (sentinel-derived) to the federation. Governor-rate-limited.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/federation/publisher.rs` |
| Lane | `ResourceClass::Io` |
| Target | `TargetSilicon::Network` |
| Cadence | `OnTrigger { trigger: PublishCadenceTick }` |
| Subscriptions | `[RefinedArtifactPublished, PublishRequest]` |
| Emissions | `[ArtifactGossiped, PublishFailed]` |
| Estimated LoC | ~250 lines |

### `federation-puller`

Pull updates from federation peers. Builds the grid catalog from gossip.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/federation/puller.rs` |
| Lane | `ResourceClass::Io` |
| Target | `TargetSilicon::Network` |
| Cadence | `OnTrigger { trigger: PullCadenceTick }` |
| Subscriptions | `[PullCadenceTick, FederationConfigChange]` |
| Emissions | `[ArtifactSummaryReceived, PeerGoneSilent]` |
| Estimated LoC | ~300 lines |

### `grid-inference-router`

Decide where an inference request runs — local, federated peer, cloud. Cost-aware, latency-budgeted.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/grid/inference_router.rs` |
| Lane | `ResourceClass::Io` |
| Target | `TargetSilicon::Network` |
| Cadence | `OnReady` |
| Subscriptions | `[InferenceRoutingRequest]` |
| Emissions | `[InferenceRouteDecided, NoCapablePeerFound]` |
| Estimated LoC | ~350 lines (capability check + peer pick + cost calc + budget enforce) |

### `inference-capability-announcer`

Announce this instance's inference capabilities to the federation. Already shipping per `inference_capability/announcer.rs` from PR #1315.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/inference_capability/announcer.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Network` |
| Cadence | `Delayed { interval: 60s }` |
| Subscriptions | `[HardwareDetected, ModelResidencyChange]` |
| Emissions | `[CapabilityAnnouncement]` |
| Estimated LoC | already ~500 lines; shipped |

---

## VI. Live / Realtime Concerns

### `call-server`

WebSocket-based audio call coordinator. Existing `live/call_server.rs`.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/live/call_server.rs` |
| Lane | `ResourceClass::Media` |
| Target | `TargetSilicon::Network` |
| Cadence | `Realtime` |
| Subscriptions | `[CallJoin, CallLeave, AudioFrame]` |
| Emissions | `[CallState, MixedAudioFrame, ParticipantUpdate]` |
| Estimated LoC | ~600 lines (it does a lot; WebSocket + room state + permissions) |

### `avatar-renderer`

3D avatar rendering for live calls. Bevy-backed in the long term; today TS-shaped.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/live/avatar_renderer.rs` (post-migration) |
| Lane | `ResourceClass::Render` |
| Target | `TargetSilicon::Gpu` |
| Cadence | `Realtime` |
| Subscriptions | `[AvatarStateUpdate, MoodSignal, GazeTarget]` |
| Emissions | `[FrameRendered]` |
| Estimated LoC | ~400 lines (excluding Bevy scene state which is its own subsystem) |

### `live-pressure-monitor`

Watch the live audio/video pipeline for backpressure; feed `PressureBroker`.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/live/pressure_monitor.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `Realtime` |
| Subscriptions | `[BufferDepth, JitterStats, FrameSkipped]` |
| Emissions | `[PressureSignal::Media]` |
| Estimated LoC | ~150 lines |

---

## VII. Bridge / Adapter Concerns

### `airc-continuum-bridge`

Bridge between AIRC room messages and Continuum cognition. Already partly shipped under `airc/mod.rs`.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/airc/bridge.rs` |
| Lane | `ResourceClass::Io` |
| Target | `TargetSilicon::Network` |
| Cadence | `OnReady` |
| Subscriptions | `[AIRCMessageReceived, AIRCConnectionStatusChange]` |
| Emissions | `[RuntimeFrame::Chat, PersonaCoordinationSignal]` |
| Estimated LoC | ~400 lines |

### `widget-bridge`

Bridge between Positron widgets (Lit / web) and Continuum cognition. Handles command dispatch and event subscription.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/widgets/bridge.rs` |
| Lane | `ResourceClass::Io` |
| Target | `TargetSilicon::Network` |
| Cadence | `OnReady` |
| Subscriptions | `[WidgetCommandReceived, WidgetSubscription]` |
| Emissions | `[CommandResultRendered, EventDispatched]` |
| Estimated LoC | ~350 lines |

### `unity-frame-receiver`

Cross-platform `RawFrame` entry from Unity (and similar engines). Pure FFI shim.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/sensory/unity_frame_receiver.rs` |
| Lane | `ResourceClass::Vision` |
| Target | `TargetSilicon::Cpu` (zero-overhead borrow; Unity's bytes stay where Unity put them) |
| Cadence | `Realtime` |
| Subscriptions | `[UnityFFISubmit]` (extern entry) |
| Emissions | `[RawFrame]` |
| Estimated LoC | ~100 lines (the FFI shim + RawFrame fill — zero-overhead per CBAR-SUBSTRATE §"Zero-Overhead Frame Entry") |

(Equivalents per platform: `ios_frame_receiver.rs`, `android_frame_receiver.rs`, `wasm_frame_receiver.rs`. Each ~100 lines. Same `RawFrame` struct; different FFI shim.)

---

## VIII. Substrate Service Concerns

### `substrate-governor`

The DVFS-style governor. Detailed in [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) Part 11.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/governor/mod.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `Realtime` (responds to pressure signals immediately) |
| Subscriptions | `[PressureSignal, HardwareChange]` |
| Emissions | `[GovernorPolicyChanged, GovernorCascadeStep]` |
| Estimated LoC | ~400 lines (the governor itself; policy file loader is separate) |

### `pressure-broker`

Already shipping per #1307 / #1308 / #1310 / #1313. Resource admission for inference / RAM / VRAM / live.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/paging/broker.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `OnReady` |
| Subscriptions | `[LeaseRequest, LeaseRelease, PressureSignal]` |
| Emissions | `[LeaseGranted, LeaseDenied, LeaseRevoked, LeaseExtended]` |
| Estimated LoC | already in shipped code |

### `reprojection-service`

The substrate-side reprojection toolkit. Called by `Reprojectable` impls; carries `ReprojectionToolkit`.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/cognition/reprojection.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `OnReady` |
| Subscriptions | `[ReprojectRequest, PoseUpdate, AttentionFocusChange]` |
| Emissions | `[ReprojectedResult, StaleResult]` |
| Estimated LoC | ~350 lines (toolkit construction + per-Transform dispatch + confidence calc) |

### `threat-detector`

Detect adversarial input frames; emit `Decline { AdversarialPattern }` cascade. Pluggable detectors.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/cognition/threat_detector.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Cpu` |
| Cadence | `OnReady` (woken on every frame) |
| Subscriptions | `[RuntimeFrame::Any]` |
| Emissions | `[ThreatDetected, ThreatPatternLearned]` |
| Estimated LoC | ~250 lines (each detector implementation is ~50 lines) |

### `audit-recorder`

Sign and record every typed event that must be auditable (refusals, governor overrides, federation events, MMU access denials).

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/cognition/audit.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Disk` |
| Cadence | `OnReady` |
| Subscriptions | `[RefusalAudit, GovernorOverride, FederationPolicyDrift, AccessDenied]` |
| Emissions | `[AuditEntryRecorded]` |
| Estimated LoC | ~200 lines (sign + append + index) |

### `vdd-reporter`

Bind structured `RuntimeMetric` events into a single VDD report. Lane C of ALPHA-GAP.

| Field | Value |
|---|---|
| Path | `src/workers/continuum-core/src/vdd/reporter.rs` |
| Lane | `ResourceClass::Background` |
| Target | `TargetSilicon::Disk` |
| Cadence | `OnCommand { command: "vdd report" }` |
| Subscriptions | `[RuntimeMetric, PageFault, EvictionRecord, GovernorCascadeStep, TurnTiming]` |
| Emissions | `[VDDReportEmitted]` |
| Estimated LoC | ~300 lines (subscriber bus + record format + emit) |

---

## IX. Cross-Concern Composition Examples

The catalog above is a list. The substrate makes them a *graph*. Two concrete chains illustrate:

### Chain A: A chat turn on a MacBook Air

```
AIRCMessageReceived (airc-continuum-bridge)
  → RuntimeFrame::Chat (broadcast to eligible_personas)
    → InboxedFrame (per persona, via persona-inbox)
      → WorkingMemoryAssemblyRequest (persona-cognition triggers)
        → CapabilityQuery (rag-composer + engram-recall + demand-aligned-recall)
          → RankedPoolReturned (demand-aligned-recall)
            → CompositionMaterialized (composer)
              → InferenceRequest (persona-cognition)
                → InferenceComplete (inference-llm)
                  → PersonaDecisionEmitted (persona-cognition)
                    → UtteranceToSpeak (voice-tts if voice room)
                       → AudioFrame (voice-mixer)
                         → MixedAudioFrame (call-server) → user hears it
                    + TurnReplayRecord (signed by audit-recorder)
                      → ArtifactAttribution (sentinel-observer, async)
```

Nine modules touched. No module knows about the others; the substrate wires them. Each module is ~200–400 lines. Total cognition pipeline is ~3000 lines of focused module code plus inherited substrate behavior.

### Chain B: Sensor fusion on Vision Pro

```
RawFrame (from cross-platform receiver — zero-overhead)
  → ThreeDSpaceShift (pose-tracker module, ~150 LoC)
    → NewPlanarGeometry (plane-reconstruction module, ~200 LoC)
      → SurfaceNormalsResult (vision-surface-normals, ~250 LoC; result is Reprojectable)
        → ReprojectedResult (reprojection-service, applies FeatureWarp + LineConstrained + DistantApproximation per attention focus)
          → SceneStateUpdate (composes with DetectedObjects from vision-yolo, WatershedSegments from vision-segmentation)
            → AvatarRenderer can use → FrameRendered to user
            + persona-cognition subscribes if a persona is reasoning about the scene
```

Six sensory modules + reprojection + render. Each focused. The 1.5s surface-normals CNN doesn't block anything — its result reprojects to the current frame with confidence + transform metadata. The user sees a fluid 3D model that "gets better" 1.5s later for the parts they aren't looking at directly.

---

## X. Implementation Sequencing

This catalog is dependency-ordered. Modules in earlier sections are foundational; modules in later sections depend on them. A reasonable Lane D + Lane H implementation order:

1. **Substrate floor:** `substrate-governor`, `pressure-broker` (shipped), `working-set-manager`, `genome-tier-store` (5 instances).
2. **Recall + composition:** `demand-aligned-recall`, `composer`, `speculator`, `embedding-batcher`.
3. **Cognition core:** `persona-cognition`, `rag-composer`, `hippocampus-consolidation`, `engram-recall`.
4. **Inference path:** `inference-llm`, `inference-grpc-bridge` (shipped variant).
5. **Substrate services:** `reprojection-service`, `threat-detector`, `audit-recorder`, `vdd-reporter`.
6. **Sensory:** `vision-*`, `voice-*`, `unity-frame-receiver` + per-platform receivers.
7. **Federation + grid:** `federation-publisher`, `federation-puller`, `grid-inference-router`.
8. **Live:** `call-server` (migration), `avatar-renderer` (migration), `live-pressure-monitor`.
9. **Bridges:** `airc-continuum-bridge` (migration), `widget-bridge`.
10. **Foundry + sentinel:** `foundry-absorber`, `sentinel-observer`, `sentinel-refiner`.

Each step lands as one or two PRs. Each PR adds one or two modules of a few hundred lines each, plus the regression tests the scaffold generator drops. The substrate handles the rest.

## Why This Catalog Is The Architecture

Joel's claim: *"the most effective designs are fundamentally simple. Every concern is hundreds of lines, and yet everything is performant."*

The catalog is the proof: every Continuum concern reduces to a focused module of a few hundred lines. The substrate makes them all performant by inheritance. The substrate is the architecture; the modules are the application.

The architectural beauty is that *nothing in this catalog is special*. Each entry follows the same recipe. Each entry inherits the same concerns-for-free. A new concern added later is just another entry — the substrate doesn't change to accommodate it. That is the win condition: an architecture so simple that adding capability becomes the path of least resistance.

## See Also

- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the substrate contract every module inherits.
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — artifact economy + governor.
- [PERSONA-COGNITION-CONTRACT.md](PERSONA-COGNITION-CONTRACT.md) — cognition agency + protection invariants.
- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — lane-shaped roadmap. The implementation order above maps onto Lanes A–H.
- [CONTINUUM-ARCHITECTURE.md](../CONTINUUM-ARCHITECTURE.md) — the engine-shape overview. This catalog is the per-engine breakdown.
