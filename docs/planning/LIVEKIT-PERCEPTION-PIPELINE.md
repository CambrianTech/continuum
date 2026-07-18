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
Pull `Qwen3-VL-7B-Instruct` GGUF + its `mmproj`, bring up in llama-server, register the
`Vision` capability + the mmproj path on the Model row. Bounded and validatable ALONE —
nothing perceives without a describer.
*Gate:* `cu ai/generate` (or `cognition/vision-describe`) with an image → a real
description returns from the local model.

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
