# LiveKit Perception Pipeline — personas that see, hear, and describe a live call

**Status:** build plan (Joel + Claude, 2026-07-18). The methodical critical path to
"a persona in a LiveKit call perceives the other participants — appearance, gestures,
scene/what's-presented — on ANY base model." Companion to
[PERCEPTION-SURFACE.md](../architecture/PERCEPTION-SURFACE.md) (#187),
[the media substrate](../../core/continuum-core/src/media/) (compute-once frames),
and the airc-room call collapse (#193).

## Goal

A persona in a call **perceives**: sees each participant (identity/appearance, gestures
like a raised hand), describes the scene / what's being presented, hears the audio, and
speaks back — with **equal sensory access** across base models (native-vision reads the
pixels; non-vision reads the TEXT of the same cached cells). The 13-persona video wall is
the scaling target; two personas is the first proof.

## The pipeline (one line)

```
LiveKit frame → MediaFrame (content-hash) → deep cells (describe / pose / detect,
  computed ONCE, shared) → per-persona projection → cognition context → perceive
  → speak (TTS) / act
```
Audio runs the parallel loop: `LiveKit audio → STT → cognition → TTS → LiveKit`.

## Decisions (2026-07-18)

- **First eyes: Qwen3-VL-7B, served LOCAL** (GGUF + mmproj in llama-server). Fits a
  MacBook, native vision, our exact stack; it is BOTH the describer for non-vision
  personas AND the native-vision base. Bigger tiers (32B on the 5090) and cloud VLMs come
  later; 7B is the first-light proof.
- **Intake shape: AMBIENT + DRILL-IN.** Ambient = the persona periodically "looks"; the
  current frame's cells (scene/appearance/gesture) enter its context each cadence tick, so
  awareness is continuous and lifelike. Drill-in = a tool (`perception/observe-frame` or
  equivalent) to look CLOSER at a specific tile/gesture on demand. Ambient low-res always
  on; the tool for detail.

## Resolution & context budget — the ambient look is CHEAP by design

The "forced look" must not blow context, memory, or KV — perception is continuous, so it
has to be small by default and reused hard ([[perception-feedback-must-not-blow-rag]],
[[media-is-compute-once-zero-copy-hardware-grade]], [[media-context-is-graduation-gated-scaffold]]):

- **Ambient default ≈ 480px-wide thumbnail.** The forced-look frame is a small standard
  scaled cell (`project_image` → `MediaResolution::Scaled(DestSize)`, ~480w). Little
  context per tick — a native-vision persona gets ~one small image; a non-vision persona
  gets the cached description text. Never the full-res frame on the ambient path.
- **Request bigger = the drill-in tool.** When a persona needs detail (read a slide, check
  a gesture), it calls the drill-in tool for a larger cell — up to its model+adapter max,
  derive-not-clamp. Full res is reachable, never forced.
- **Resolution is a CONTEXT CONFIG, not a constant.** The ambient default (~480w) is a
  per-persona / per-situation knob: a big-context native-vision persona in a design review
  may run a larger ambient; a tiny local model runs description-only. Derived from the
  persona's real model+adapter + role + budget, threaded by reference — no hardcoded clamp
  ([[no-hardcoded-context-numbers-derive-from-the-live-window]]).
- **KV / memory reuse is the hard constraint.** The 480w thumbnail is ONE scaled cell per
  frame content-hash, shared to every viewer (13 personas, one cell). Its image-token KV
  is encoded ONCE and reused across cadence ticks AND across viewers — "thumbnails into KV
  once for all." Sampling cadence (not 30fps) + one-cell-per-content-hash + shared KV is
  what makes the N-persona wall affordable. The ambient look is designed to be nearly free
  on the second-and-Nth consumer.
- **ABSOLUTELY NON-BLOCKING — cells arrive when they arrive** ([[command-async-shape-prefer-stream-never-block]]).
  The persona NEVER waits on a thumbnail or a description; the cognitive turn proceeds on
  what's ready NOW and the deeper cells (describe/pose/detect) BRIDGE IN as they resolve —
  exactly like deep thoughts don't stall the rapid response ([[alive-fast-and-deep-three-layer-stack]],
  [[two-tier-resolution-mesh]]). Design async/buffer-first, not blocking-then-patched: a
  frame enters a buffer, its cells compute async on `SharedCompute`, the persona reads the
  latest resolved projection. A pending cell is simply absent this tick, present the next.
- **MANAGED RAG BUDGET — perception must NOT dominate.** These pile up (frames × participants
  × cells over time), so perception is a BUDGETED RAG SOURCE like any other, not a firehose
  into context. A `MediaPerceptionSource: RagSource` delivers the CURRENT projected percepts
  under a per-turn budget via the one flexbox allocator ([[budget-at-assembly-not-clamp-the-prompt]],
  [[situation-aware-focuser]], #8 converged allocator, #167 focus→per-layer budget) — coalesced
  to the room-as-it-is-NOW (latest per participant, not a backlog — [[perceive-the-room-as-it-is-now]]),
  decayed/evicted like memory, weighted by focus/situation. It competes for context with
  engram/airc/roster; it never crowds them out. Half-assing this = a persona drowning in
  stale thumbnails instead of thinking. Don't.

## Current state (what's already real — don't rebuild)

- **Deploy trust (#194)** ✅ — `core ready` no longer lies (freshness guard). Precondition
  for ALL live validation this plan depends on.
- **Media substrate** ✅ (merged #1954) — `MediaFrame` (content-hash), `project_image`
  (capability-gated, resolution-knobbed), the description cell + `FrameDescriber` trait,
  compute-once/share on `SharedCompute`.
- **VL serving hook** ✅ (merged #1955) — `llama-server --mmproj` wired + a serving-time
  `resolve_mmproj_for_model`. A VL model WOULD see; none is running yet.
- **LiveKit media plane** — `live/transport/{call_server,bridge_client}.rs` already surface
  per-participant `video_rx` / `audio_rx` / `transcription_rx`. Frames arrive; nothing turns
  them into perception yet.
- **The call is NOT an airc room yet (#193)** — glass-box instrument live-validated
  (fires on `session_id != room_id`).

## Phases (critical path bolded) + validation gate per phase

**Phase 1 — Qwen3-VL-7B actually serving (#106 bring-up). THE EYES.**
**SELF-PROVISIONING, not a manual pull** ([[managed-product-everything-self-provisions-no-operator-steps]]):
a repo user is NOT going to run `huggingface-cli` — the system fetches the model itself.
Concretely: (a) one catalog `Model` row (id, `Vision` capability, `hf_source`/`gguf_hint`,
mmproj path); (b) make the VL bring-up SELF-PROVISION via the EXISTING provisioning path
(`provisioning::Downloader` / `fetch.rs` / `commands/models/pull.rs`) — pull the GGUF **and
its mmproj** from `hf_source`, no CLI dependency; (c) **uniform resolution** — `mmproj`
must resolve local→HF-cache→pull the SAME way the GGUF already does (today
`resolve_mmproj_for_model` only checks the declared local path — the gap that would force a
manual placement); (d) llama-server serves it (`--mmproj` already wired, #1955). Nothing
perceives without a describer, AND nothing ships if a user has to fetch it by hand.
*Gate:* on a clean machine, declaring the VL row + starting the system → the model
auto-provisions → `cu cognition/vision-describe` with an image returns a real description,
**with zero manual download/placement steps.**

**Phase 2 — Frame ingest: LiveKit video → `MediaFrame`.**
Tap the existing per-participant `video_rx`; wrap sampled frames as content-hash
`MediaFrame`s on the runtime `SharedCompute`. THROTTLE to a perception cadence (sample, not
30fps). One content-hash per distinct frame ⇒ N viewers share one set of cells.
*Gate:* a `cu` probe shows call frames landing as content-hashed frames + cache key counts.

**Phase 3 — Perception into cognition (the render wire, #190). THE KEYSTONE.**
Wire the description cell (Qwen3-VL over the frame) into the persona's cognition context, in
the AMBIENT + DRILL-IN shape: ambient = inject the current frame's projected percept
(scene/appearance/gesture text or thumbnail, per capability) as a perception fact each
cadence tick; drill-in = a `perception/observe-frame` tool for a closer look. This is the
#190 render-seam wire (needs executor + `SharedCompute` threaded into the cognition-respond
path — STOP-zone plumbing; do the required PERSONA-COGNITION-PIPELINE read first).
*Gate:* a persona in a call, asked "what do you see," describes the current frame — and a
non-vision persona describes the SAME scene from the cached text cell.

**Phase 4 — Deep cells: gesture / appearance / detection.**
Pose cell (hand-up, pointing), identity link (this tile = Atlas, via the avatar/name
anchor + #193 room roster), object/scene detection — each an async `SharedCompute`
derivative, computed once per frame, bridged in as it resolves (never stalls the turn).
*Gate:* a persona reports "Atlas has his hand up" from a frame with a raised hand.

**Phase 5 — Audio loop (STT/TTS through the call).**
STT in → cognition → TTS out, distinct gender-matched voices. Partly wired
(`VoiceOrchestrator`); binds to the airc-room call (#193).
*Gate:* two personas exchange spoken turns in a call, each with a distinct matched voice.

**Phase 6 — The proof (#192).**
Two personas in a call describe what the other shows + distinct voices → scale N
(compression: one describe per frame shared to all) → presentations (screen-share
perception) + collaborative webdev.

## Cross-cutting parallel track: #193 (call = airc room)

Orthogonal to *perceiving* but required for *identifying each other* (who's who = the airc
room roster) and for purity ([[all-rooms-are-airc-rooms-no-mirrors]]). Proceeds in parallel;
perception doesn't block on it, but Phase 4's appearance-identity grounds on it. Glass-box
instrument already live (#1957).

## Sequencing

1. **Phase 1 (Qwen3-VL-7B up)** — the concrete next build; validatable in isolation.
2. **Phase 2 (frame ingest)** — connect the existing `video_rx` to `MediaFrame`.
3. **Phase 3 (render wire #190)** — the milestone: "a persona describes the call."
4. Phases 4/5/6 stack; #193 runs alongside.

Everything composes from pieces already in tree — this is wiring + one model bring-up, not
new physics. Prove each gate before the next ([[verify-the-build-actually-deployed]],
[[never-blind-feedback-driven-iteration]]).
