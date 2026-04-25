# Live Video Chat Architecture -- Vision-Capable Personas in WebRTC Calls

> A 16 GB MacBook Air, lid open, no cuts: an avatar makes eye contact, says hi, you hold up a sticky note, the avatar reads it back. All-local, sub-400ms turn cycles, zero cloud. That's the demo this architecture targets. The vision-bytes path is unblocked as of 2026-04-22; the remaining work is the change-detection gate, streaming TTS, and the autonomous avatar loop. **Energy spend correlates with novelty, not time** -- if nothing in the scene changed, the heavy vision model does not run.

**Parent:** [Live](README.md)
**Status:** Vision-bytes path operational (2026-04-22). Change-detection gate, streaming TTS, and autonomous video-chat behavior pending.

---

## Table of Contents

1. [Demo Target](#demo-target)
2. [What Shipped (the Unblocker)](#what-shipped-the-unblocker)
3. [The Load-Bearing Principle: Change Drives Inference, Not Time](#the-load-bearing-principle-change-drives-inference-not-time)
4. [Two Gates: Passive CV + Active AI Request](#two-gates-passive-cv--active-ai-request)
5. [Gate Palette](#gate-palette)
6. [Everything Is a Command (And a Reusable Adapter)](#everything-is-a-command-and-a-reusable-adapter)
7. [Detection ≠ Event: Track-State-Change Is the Event](#detection--event-track-state-change-is-the-event)
7. [Mixed-Modality Turn-Taking](#mixed-modality-turn-taking)
8. [Streaming Pipeline](#streaming-pipeline)
9. [Punch List](#punch-list)
10. [Cross-References](#cross-references)

---

## Demo Target

Pin the spec so engineering decisions point at it.

**Setup:** Stock M2 Air 16 GB, lid opens, single 30-second take, no cuts, no cloud, no API keys.

**Sequence:**
1. Avatar walks into frame on idle.
2. Camera detects user → avatar makes eye contact.
3. Avatar greets unprompted: *"Hi, what are you up to?"*
4. User holds up a sticky note with handwritten text.
5. Avatar reads the text back, comments on it.
6. Total latency budget per turn: **<400 ms hear→speak**, with first-syllable TTS audio leading the LLM completing.

**Why this is the moat:** every "AI avatar" demo cheats with workstation GPU + cloud-only model + edited cuts to hide 4-second latency. Stock M2 Air, no cuts, all-local is something nobody else can ship right now. The pieces exist in this repo. This doc threads them.

**Device ladder degrades gracefully:** M2 Air 16 GB runs the single-persona demo above; M2 Pro 32 GB runs a small group; 3090 desktop runs a 14-persona room. Same architecture, more seats per machine.

---

## What Shipped (the Unblocker)

Before 2026-04-22, every webcam frame routed to a vision-capable persona produced `parts=0 image=0` in the adapter log -- the bytes never reached the encoder. **Four** layers were stripping `messageMedia` between PRG and the model:

1. **Inbox round-trip strip** -- Rust's `ChatQueueItem` and `ChannelEnqueueRequest::{Chat,Voice}` had no `media` field. Items serialized through Rust IPC lost the attachment. *Fixed in commit `e1915f218`* (PR #950).
2. **Mixin payload strip** -- TS `cognitionPersonaRespond` mixin built a typed `PersonaRespondRequest` carrying `messageMedia`, but the actual `requestFull(...)` call args silently omitted `message_media`. *Fixed in commit `efa73f7cd`* (PR #950).
3. **Consolidation trigger demotion** -- `ChatQueueItem.consolidate_with_items` picked latest-by-timestamp as the trigger and dropped media from non-trigger items. In an active room where text replies landed after an image, the image became a non-trigger and its bytes were lost. *Fixed in commit `39d2a6fce`* (PR #950): trigger-selection strategy now prefers the latest media-bearing item when any exists, falling back to latest-by-timestamp otherwise. Per-item-type polymorphism preserved -- chat strategy ≠ video-frame strategy ≠ game-move strategy. Each item type owns its rule.
4. **Adapter walk + mtmd encoder** -- `LlamaCppAdapter.generate_text` walks `ContentPart::Image`, decodes base64, routes to `backend.generate_with_image()` → `MtmdContext::eval_image()`. Existed prior; verified end-to-end 2026-04-22.

**Proof signals** that the chain works (from `~/.continuum/jtag/logs/system/modules/llamacpp.log`):

Single-image standalone case (msg `390dad9d`, "BAD MOTHER FUCKER" wallet, 2026-04-22):
```
generate_text request: model=qwen2-vl-7b-instruct messages=12
  (text=11 parts=1; parts contain text=1 image=1 audio=0 other=0)
```
Vision AI's response: *"A worn, brown leather wallet with the words 'BAD MOTHER FUCKER' embroidered in black on its front."* — pixel-level OCR.

Image-with-queue-depth case (msg `8668bc`, Activity Monitor screenshot with 10 prior messages queued, 2026-04-22):
```
qwen2-vl-7b-instruct messages=11 (text=10 parts=1;
  parts contain text=1 image=1 audio=0 other=0)
```
Vision AI's response named the actual processes visible (*"limactl, llama-cli, qemu-system-aarch64, continuum-core-server"*) and the memory value (*"24.04 GB"*) — confirming the trigger-prefers-media strategy correctly picked the image as the trigger even with 10 text messages around it.

Reading embroidered wallet text and process names inside a screenshot requires actual image bytes at the encoder, not metadata or filename leakage. Vision is wired AND robust to queue depth.

Audio path is structurally identical (`ContentPart::Audio` walk, `backend.generate_with_audio()`, `MtmdContext::eval_audio()`, `Capability::AudioInput` check, test fixture) and ships with the audio-model verification work in PR #950.

---

## The Load-Bearing Principle: Change Drives Inference, Not Time

**If nothing in the scene changed, the heavy vision model does not run.** No exceptions.

The naive design -- "send every webcam frame to qwen2-vl every N ms" -- wastes 99% of inference on identical pixels. At 30 fps, a single persona watching a stationary user burns ~50 GB of model activations per minute and produces no new information. Multiply by N personas in a video call and the energy budget collapses before the demo runs.

The right design comes straight from CBAR (`cb-mobile-sdk/cpp/cbar/`):

- `CBP_RenderingEngine::m_isStillMode` pauses expensive rendering when the device is still.
- `CBP_FeatureTracker` tracks point identity across frames with optical flow, so we don't re-derive the world every tick.
- The analyzer pipeline (`pipeline/analysis/`) routes events on semantic deltas, not on time.

Same shape here. Cheap, continuous CV runs always (~1-30 ms/frame depending on detector). Heavy vision LLM only fires on triggered events. Cadence at the gate is **0.5-1 Hz** -- humans don't react to scene changes faster than that anyway.

This applies to every continuous visual stream feeding a persona: webcam in a video call, screen share in a coding session, AR camera in a future mixed-reality activity. The principle doesn't change.

---

## Two Gates: Passive CV + Active AI Request

Two complementary triggers feed the same downstream pipeline.

### Passive: CV-driven

Cheap CV runs on every frame in the capture pipeline (Rust, off the main thread per the render-loop-sacred principle from [LIVE-CALL-ARCHITECTURE.md](LIVE-CALL-ARCHITECTURE.md)). On a meaningful semantic event, it emits a `vision:scene-event` to the persona's autonomous loop:

```rust
// Conceptual shape -- final API lives in the cv-attention-gate PR.
pub enum SceneEvent {
    ObjectAppeared  { class: String, bbox: BBox, frame: FrameRef },
    ObjectDisappeared { class: String, last_bbox: BBox },
    ObjectMoved    { class: String, from: BBox, to: BBox, distance: f32 },
    PersonEntered  { bbox: BBox, frame: FrameRef },
    SceneShift     { magnitude: f32, frame: FrameRef },  // generic large delta
}
```

The persona's autonomous loop subscribes to these events. When one fires, the loop decides whether to invoke the vision LLM (rate-limited, capability-checked, recipe-aware). The vision LLM gets the **cropped region** plus context, not the whole frame -- massively cheaper inference and a more focused prompt.

### Active: AI-initiated

The persona has a `vision/look` tool it can call when reasoning concludes a look would be useful:

```
User: "check this out"
Persona: <reasoning>user is asking me to attend visually</reasoning>
Persona: tool_call(vision/look, source: "main-camera")
→ same MediaItem pipeline, ContentPart::Image, mtmd encoder
```

Both gates feed the same proven mtmd path shipped in PR #950. The expensive model only fires on triggered events; the architecture stays consistent regardless of trigger source.

---

## Gate Palette

Different detectors trade compute for semantic richness. Pick per scenario; mix-and-match per recipe.

| Detector | Cost (Metal) | Output | Best for |
|----------|-------------|--------|----------|
| Frame diff | <1 ms | "pixels changed by N%" | Useless alone (lighting, shake noise); fine as a prefilter to skip the others when truly static |
| ORB feature tracks | ~5 ms | Keypoint motion vectors, robust to lighting | "Did the camera move? Did the user shift position?" CBAR's FeatureTracker family |
| Optical flow (dense) | ~15 ms | Motion field per pixel | "Where is motion happening?" Useful for region-of-interest before YOLO |
| YOLO (small variant) | ~10 ms | Object bboxes + classes | "What objects are present?" The semantic workhorse |
| Semantic seg (SegFormer-tiny / DeepLabV3-tiny) | ~30 ms | Per-pixel region labels | "Scene structure changed -- person now seated, wall now has whiteboard text" |
| Pose estimation (RTMPose-tiny / MoveNet) | ~15 ms | Skeleton joints | "Person is gesturing, holding object up, sitting/standing" |

At 0.5 Hz cadence (every 2 seconds), even the heavier seg model is rounding-error in the energy budget. The combination of one cheap always-on detector + one richer on-demand detector is the right pattern. CBAR's `pipeline/analysis/` shows the polymorphic-analyzer shape we mirror.

---

## Everything Is a Command (And a Reusable Adapter)

The CV gate is not a private subsystem. It's a **family of commands** so:

- AIs invoke detectors as tools (`vision/detect --algorithm=yolo --source=main-camera`)
- Other code reuses them (a sentinel pipeline can run the same YOLO command headlessly; the Factory can use the same semantic-seg command as a forge-time data-quality check)
- Algorithm choice is a runtime decision, not a compile-time one -- per the OpenCV-style polymorphic-adapter pattern Continuum already uses for search and inference

### Adapter shape (Rust)

Mirrors the existing pattern documented in CLAUDE.md and used throughout `continuum-core` (search algorithms, inference backends, vision providers):

```rust
trait SceneDetector: Send + Sync {
    fn name(&self) -> &'static str;          // "frame-diff" | "orb" | "yolo" | "segformer-tiny"
    fn detect(&self, frame: &VideoFrame) -> Vec<Detection>;
    fn cost_estimate_ms(&self) -> f32;       // for the gate scheduler
    fn get_param(&self, name: &str) -> Option<Value>;
    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String>;
}

trait Tracker: Send + Sync {
    fn name(&self) -> &'static str;          // "iou" | "kalman" | "deepsort"
    fn associate(&mut self, detections: Vec<Detection>) -> Vec<Track>;
    fn get_param(&self, name: &str) -> Option<Value>;
    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String>;
}

// Factory registry — runtime creation by name, no hardcoded match arms.
struct DetectorRegistry {
    factories: HashMap<&'static str, fn() -> Box<dyn SceneDetector>>,
}
```

Concrete implementations live in their own modules (`frame_diff.rs`, `orb.rs`, `yolo.rs`, `segformer.rs`, `kalman.rs`) and self-register at startup. Adding a new detector means writing one file plus one registration line. AIs and other commands discover them via the registry without recompiling.

### Command surface (TS shell, Rust impl)

The Continuum command shell is TypeScript (CLI ergonomics, command discovery, schema generation). The implementation is **always** Rust via the IPC mixin -- TS is the thin wrapper, Rust is the truth. Per the standard pattern documented in CLAUDE.md.

| Command | Purpose | Reusable by |
|---------|---------|-------------|
| `vision/detect` | Run a registered detector on a frame source. Returns detections. | AI tool calls, sentinels, data pipelines |
| `vision/track` | Associate detections across frames; returns tracks. | Same |
| `vision/look` | AI-initiated heavyweight vision invocation. Captures one frame, routes through the proven mtmd path. | AI tool calls primarily |
| `vision/subscribe` | Subscribe to `SceneEvent`s from the gate (inbox routing). | Persona autonomous loops, future activity types |
| `vision/list-detectors` | Enumerate registered detectors with cost / capability. | AIs that want to choose; settings UI |

The CV gate event loop itself is Rust -- a long-running detector per video source, configured by recipe, emits `SceneEvent`s onto the persona inbox channel via the existing IPC. TS never sees frames.

### What gets reused

Thinking from "what would someone want to reuse" outward, not from "what does this PR need." The gate is **activity-agnostic** -- a chat persona watching a webcam, a game NPC scanning the game scene, a sentinel running a headless data-quality pass on a video file, a screen-share session in a coding activity all call the same primitives:

- **Detectors and trackers** -- one set, used across video chat, screen share, AR / mixed reality, game NPC perception, factory data-quality runs, sentinel pipelines, headless batch analysis. The frame source differs (webcam vs game framebuffer vs video file vs screen capture); the detector trait does not.
- **`SceneEvent` enum** -- the wire shape that lets any subscriber consume gate output regardless of which detector produced it OR which activity is hosting the persona
- **The cropping primitive** (bbox + frame → cropped MediaItem) -- shared with the active `vision/look` path so both gates produce the same thing, regardless of caller
- **Cost estimator** -- so a future `PressureBroker` can adapt detector cadence under memory pressure without each consumer reinventing the policy

The principle: when a chat persona, a game NPC, and a sentinel pipeline all want "tell me when an object enters the scene I'm looking at," they should all call `vision/subscribe` and get a `SceneEvent` -- not three different chat-shaped, game-shaped, batch-shaped APIs.

### What stays narrow

What's NOT a reusable abstraction (avoid premature generalization):

- The webcam-capture-to-frame plumbing -- one place, well-typed, no need for a trait
- The persona-inbox routing -- already typed via `InboxMessage`/`InboxTask`
- The avatar animation hooks -- specific to the Bevy renderer, no benefit to abstracting

---

## Detection ≠ Event: Track-State-Change Is the Event

Per-frame detections are noisy. YOLO misses an object in frame N that it found in N-1 and N+1. Naive "no detection → object gone" produces spurious events that page the persona on every flicker.

The mandatory layer between detection and event is **tracking**:

- Associate detections across frames (IoU overlap or feature embedding match).
- Maintain track lifetimes -- a track is born after K consecutive detections, dies after M consecutive misses.
- Smooth pose / position with a Kalman filter (or simpler EMA for static objects).
- Emit a `SceneEvent` only when a TRACK is born, dies, or moves more than a threshold -- not on per-frame detection fluctuation.

Same pattern Joel used in CBAR with Kalman filtering for handheld pose stability. Without this layer the persona gets paged dozens of times per minute on noise; with it, paging matches the real semantic rhythm of the room.

```
detector (noisy, per-frame)
        ↓
tracker (associate, smooth, lifetime)
        ↓
event derivation (track born / died / moved meaningfully)
        ↓
persona inbox (vision:scene-event)
```

---

## Mixed-Modality Turn-Taking

Not every persona in a video chat needs to be the full sensory stack. Group dynamics work BETTER with mixed cadences:

| Tier | Modality | Latency | Social role |
|------|----------|---------|-------------|
| Audio-native (dominant majority) | Hear + speak natively, see via change-gate | <400 ms | Carry the room rhythm, live banter, immediate reaction |
| Vision-only | See natively, hear via STT bridge, speak via TTS | ~1.5 s | Beat-late observers, "hey did anyone notice that" voice |
| Pure-text | Read transcript, write responses (rendered as TTS) | ~3 s | Deep contributor -- code reviewer, deliberate one |

The slow personas don't break the illusion. They read as **deliberate thinkers**, not as broken. The audio-natives carry the perceived liveness; the bridged personas chime in after a beat with something thoughtful. That's a *better* social pattern than everyone-responds-instantly -- it matches how real groups work.

Implication for seed strategy: when paging + audio-native local model land, **bias the local team toward audio-native** (Qwen2-Audio-7B or eventually Qwen2.5-Omni). Keep one or two vision-only or pure-text personas for variety and per-task strength (CodeReview AI on the code-forged model, for example).

Avatar-side surface for this: subtle visual tells. Bridged persona's avatar shows "thinking" idle animation while audio-natives are speaking; when the deep one finally speaks, others on the call orient toward them.

---

## Streaming Pipeline

Sub-400 ms turn cycles require streaming end to end. The current cognition path runs analyze → render → strip → parse before TTS even starts -- way over budget. The right architecture:

- **Token streaming** from the Rust LLM scheduler through the IPC boundary as tokens generate (not a single "response" payload at the end).
- **TTS pipelined per-phoneme** -- audio chunks emit as soon as enough phonemes accumulate, not after the full sentence completes. First-syllable audio leads the LLM completing.
- **Visemes drive avatar mouth shapes** off the phoneme stream -- `bevy_renderer/animation/speaking.rs` already has the mouth-shape primitives; needs the phoneme→viseme mapping wired in.
- **Eye gaze tracks the camera frame** in parallel with the LLM thinking -- `bevy_renderer/animation/eye_gaze.rs` reads scene events from the same change-gate that drives vision invocation.

See [STREAMING-BACKBONE-ARCHITECTURE.md](STREAMING-BACKBONE-ARCHITECTURE.md) for the substrate; this layer adds the token-stream IPC + TTS-per-phoneme contract on top.

The latency budget split (target):

| Stage | Budget | Notes |
|-------|--------|-------|
| STT (audio → text, partial) | 80 ms | Whisper.cpp partials at ~100 ms windows |
| Persona dispatch + analyze | 50 ms | Fast-path classifier; Rust |
| First token from LLM | 100 ms | Time to first token is the dominant ceiling |
| First phoneme → first audio chunk | 100 ms | TTS pipelining |
| Network + render | 50 ms | LiveKit + Bevy frame |
| **Total to first user-audible response** | **~380 ms** | Within the 400 ms social-realism threshold |

LLM continues generating in parallel; subsequent audio chunks chase the token stream. Visemes update mouth shape on each phoneme.

---

## Punch List

Ordered by criticality for the demo target.

### Now (PR #950 — landed)
- [x] Vision-bytes path end-to-end through Rust IPC (commits `e1915f218`, `efa73f7cd`)
- [x] Tile UI shows real model name + locality glyph (commit `62aa2642e`)
- [x] Audio integration test proves Qwen2-Audio-7B + mtmd path deterministically (commit `a3c4ea08d`)
- [x] Trigger-prefers-media-bearing-item — vision survives queue depth (commit `39d2a6fce`)
- [x] Conservative seed avoids the multi-mtmd brick (commit `f77476848`) — Vision AI alone uses qwen2-vl, Audio AI dormant

### Next-up architectural blockers (PR #951 candidates) — surfaced empirically 2026-04-22
- [ ] **Multi-mtmd Metal pipeline-compile race** — confirmed cause of the Mac brick (single mtmd backend = safe; 2+ concurrent mmproj loads at boot wedge WindowServer / cursor frozen / hard reset). Fix: serialize `mtmd_init_from_file` calls behind a global mutex OR re-integrate vision/audio paths through the llama scheduler instead of `LlamaCppBackend::generate_with_image/audio`'s per-call context bypass. Mutex is 1-day; scheduler integration is the architecturally pure version (~1 week). Until shipped, only ONE mtmd-bearing model can be live in the system.
- [ ] **Image-size preprocessing at chat-send** — confirmed: a 6.6 MB image crashes the system (qwen2-vl tiles large images into many Metal compute passes; combined with per-call context allocation, exceeds Metal device capacity). Cap inbound images to ≤1568px max dimension (qwen2-vl tile boundary), JPEG-compress at 85% quality, downscale with Lanczos. Standard practice for vision pipelines (Anthropic / OpenAI / Google all do this server-side); we just don't yet.
- [ ] **Audio AI persona seeded after multi-mtmd fix lands** — model + mmproj already on disk + integration test passes; only waiting on the architectural fix above.

### Next PR (`feature/cv-attention-gate`)
- [ ] OpenCV bindings vendored in Rust workers
- [ ] Cheap-continuous detector pipeline (frame diff prefilter → ORB tracks → optional YOLO)
- [ ] Kalman tracker layer (detection → smoothed track → event)
- [ ] `SceneEvent` enum + persona-inbox routing
- [ ] `vision/look` active-trigger command (AI-initiated)
- [ ] Crop-on-trigger: heavy vision LLM gets the bbox region, not the whole frame

### Next PR (`feature/streaming-tts`)
- [ ] Token-stream IPC contract (Rust → TS)
- [ ] TTS-per-phoneme pipelining (Kokoro / Piper streaming mode)
- [ ] Phoneme → viseme mapping wired into `bevy_renderer/animation/speaking.rs`
- [ ] End-to-end latency budget validation

### Next PR (`feature/persona-context-paging`)
- [ ] PressureBroker (per [UNIFIED-PAGING.md](../architecture/UNIFIED-PAGING.md))
- [ ] PersonaContextSlot + spill/resume primitive (per [PERSONA-CONTEXT-PAGING.md](../architecture/PERSONA-CONTEXT-PAGING.md))
- [ ] Hot-set sizing -- 14 personas in a room, ~3 hot at a time, rest paged

### Next PR (`feature/avatar-autonomous-loop`)
- [ ] Avatar idle behavior (breathing, idle gestures already exist in `bevy_renderer/animation/`)
- [ ] Camera-driven eye gaze (subscribes to `vision:scene-event`)
- [ ] Unprompted greeting on user-detected entry
- [ ] Cognitive autonomous loop extended with frame-driven event handling (today the loop reacts only to inbox messages)

---

## Cross-References

Links to existing docs that this synthesis depends on. **Don't duplicate -- index.**

| Doc | What it covers | Relevance to this doc |
|-----|----------------|----------------------|
| [LIVE-CALL-ARCHITECTURE.md](LIVE-CALL-ARCHITECTURE.md) | Game-engine philosophy, render-loop-sacred, handle-based zero-copy, LiveKit transport | Substrate for everything here |
| [STREAMING-BACKBONE-ARCHITECTURE.md](STREAMING-BACKBONE-ARCHITECTURE.md) | Universal real-time infrastructure -- ring buffers, pipeline stages | Streaming TTS + token streaming sit on this |
| [VISION-MEDIA-ARCHITECTURE.md](VISION-MEDIA-ARCHITECTURE.md) | Image processing, format conversion, RAG budget integration | The image substrate this doc extends to live video |
| [VOICE-STREAMING-ARCHITECTURE.md](VOICE-STREAMING-ARCHITECTURE.md) | TTS adapter registry, voice chat infrastructure | TTS-per-phoneme extends this |
| [VOICE-SYNTHESIS-ARCHITECTURE.md](VOICE-SYNTHESIS-ARCHITECTURE.md) | Piper / Kokoro adapters, 0.13x realtime factor | Streaming-mode work targets these adapters |
| [VOICE-CONFERENCE-ARCHITECTURE.md](VOICE-CONFERENCE-ARCHITECTURE.md) | N humans + M AIs, mix-minus, turn coordination | Mixed-modality turn-taking design extends this |
| [VAD-FINAL-SUMMARY.md](VAD-FINAL-SUMMARY.md) | Production VAD (Silero, 100% noise rejection, two-stage) | Audio-side analog to the CV-gate principle: VAD gates STT, CV gates vision |
| [SCENE-ANIMATION-ARCHITECTURE.md](SCENE-ANIMATION-ARCHITECTURE.md) | Bevy avatar animation system | Where eye_gaze, speaking, idle_gestures, breathing live |
| [UNIFIED-PAGING.md](../architecture/UNIFIED-PAGING.md) | `PagedResourcePool<K,V>` primitive, PressureBroker design | The paging substrate the 14-persona target depends on |
| [PERSONA-CONTEXT-PAGING.md](../architecture/PERSONA-CONTEXT-PAGING.md) | Per-persona KV/context paging, signals-not-constants | "Signals not constants" rule applies here too |
| [PERSONA-CONVERGENCE-ROADMAP.md](../personas/PERSONA-CONVERGENCE-ROADMAP.md) | Autonomous loop, self-managed queues, genome paging | Avatar-side autonomous loop extends this |

External:
- CBAR mobile SDK (`cb-mobile-sdk/cpp/cbar/`) -- the analyzer-pipeline + still-mode + Kalman-tracking patterns this doc draws from. The C++ heritage of the change-detection design.

---

## Key Principles (One-Liners)

- **Scene unchanged → zero inference.** Energy spend correlates with novelty, not time.
- **Cheap-continuous, heavy-on-trigger.** Cheap CV runs always; vision LLM only on event.
- **Detection ≠ event.** Track-state-change is the event. Smooth with Kalman or equivalent.
- **Crop on trigger.** Heavy model gets the relevant region, not the whole frame.
- **Two gates, one pipeline.** Passive CV + active AI request both feed the same proven mtmd path.
- **Audio-natives carry the room rhythm.** Bridged personas chime in deliberately. That's a feature.
- **Render loop is sacred.** Off-main-thread everything (carried from LIVE-CALL-ARCHITECTURE).
- **Streaming end to end.** Token stream → TTS chunk → audio out. First syllable leads the LLM completing.
- **Signals, not constants.** No hardcoded "fire vision every 2 seconds" anywhere -- the cadence emerges from gate event rates.
