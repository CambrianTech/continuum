# Sensory-Transport Separation Architecture

## Problem

`continuum-core-server` is a monolithic binary containing both `ort` (ONNX Runtime) and
`webrtc-sys` (LiveKit). These C++ libraries statically link **different versions of protobuf**,
causing intermittent runtime deadlocks. The process freezes 30-60s after startup when both
protobuf runtimes interact. No amount of link ordering, dynamic loading, or feature flags
fixes this — they must be in separate address spaces.

## Architecture: Three Binaries, Three Concerns

```
                    ┌─────────────────────────┐
                    │    continuum-core        │
                    │    (Brain)               │
                    │                          │
                    │  Cognition, Data, RAG    │
                    │  Personas, Embeddings    │
                    │  Genome, Sentinel        │
                    │                          │
                    │  NO ort, NO webrtc-sys   │
                    └────────────┬─────────────┘
                                 │ Unix Socket IPC
                    ┌────────────▼─────────────┐
                    │    sensory-server         │
                    │    (Senses)               │
                    │                          │
                    │  STT (whisper.cpp)        │
                    │  TTS (Piper via ort)      │
                    │  VAD (Silero via ort)     │
                    │  Vision Description       │
                    │  Capability Router        │
                    │                          │
                    │  HAS ort, NO webrtc-sys   │
                    └────────────┬─────────────┘
                                 │ Unix Socket IPC
                    ┌────────────▼─────────────┐
                    │    livekit-bridge         │
                    │    (Transport)            │
                    │                          │
                    │  LiveKit room mgmt       │
                    │  WebRTC audio/video       │
                    │  Token generation         │
                    │  Track publish/subscribe  │
                    │                          │
                    │  NO ort, HAS webrtc-sys   │
                    └──────────────────────────┘
```

## Capability Routing (The Key Insight)

Not all models need the sensory pipeline. Audio-native models (Qwen3-Omni, Gemini Live)
process raw audio directly. Vision-native models (GPT-4V) see images directly. The
sensory server is a **router**, not a fixed pipeline:

| Model Type | Hearing | Speaking | Seeing |
|-----------|---------|----------|--------|
| **Audio-native** (Qwen3-Omni) | Raw audio → model | Model → raw audio | Needs vision bridge |
| **Text + TTS/STT** (Helper AI) | Audio → STT → text | Text → TTS → audio | Image → description → text |
| **Vision-native** (GPT-4V) | Needs STT | Needs TTS | Raw image → model |
| **Full multimodal** (future forged) | Raw audio → model | Model → raw audio | Raw image → model |

Models trained in the Factory can **graduate** from text-only to audio-native. When a LoRA
adapter adds audio capabilities, the sensory router adapts — same interface, different path.
This is genome paging applied to senses.

## Audio Flow Examples

### Text-only model (Helper AI) hearing a human speak:
```
Human mic → LiveKit room → livekit-bridge (receives audio track)
  → IPC → sensory-server (VAD → STT → text)
  → IPC → continuum-core (utterance routing → persona generates text response)
  → IPC → sensory-server (TTS → PCM audio)
  → IPC → livekit-bridge (publishes audio track)
  → LiveKit room → Human speaker
```

### Audio-native model (Qwen3-Omni) hearing a human speak:
```
Human mic → LiveKit room → livekit-bridge (receives audio track)
  → IPC → continuum-core (raw audio → Qwen3-Omni → raw audio response)
  → IPC → livekit-bridge (publishes audio track)
  → LiveKit room → Human speaker
```

The sensory-server is **bypassed** for audio-native models. Zero latency penalty.

### Game/3D mode (no LiveKit):
```
Local mic → sensory-server (VAD → STT → text)
  → IPC → continuum-core (persona generates response)
  → IPC → sensory-server (TTS → PCM audio)
  → Local speaker
```

The livekit-bridge is **not running**. Same sensory pipeline, different transport.

## IPC Protocol

All three binaries communicate over Unix sockets with length-prefixed binary frames:

```
[4 bytes: frame length (u32 LE)][JSON header][0x00 separator][binary payload]
```

### Bridge ↔ Sensory Messages

**Bridge → Sensory:**
- `audio_frame` — raw PCM i16 from human participant + speaker metadata
- `participant_joined/left` — room membership changes

**Sensory → Bridge:**
- `speak` — TTS PCM audio to publish for a persona
- `inject_audio` — raw audio to publish
- `video_frame` — RGBA avatar frame to publish
- `join_room/leave_room` — agent lifecycle
- `publish_transcription` — subtitle text

### Sensory ↔ Core Messages

**Sensory → Core:**
- `utterance` — transcribed text from human speech (after VAD + STT)
- `raw_audio` — for audio-native models, bypass STT

**Core → Sensory:**
- `synthesize` — text to speak (core decides what to say, sensory decides how)
- `raw_audio_response` — from audio-native model, bypass TTS

## Shared Protocol Crate

`livekit-protocol` (name may change to `sensory-protocol`):
- Message enums and types
- Frame codec (serialize/deserialize)
- Shared constants (sample rate, frame size)
- Zero heavy dependencies (serde only)

## Docker Compose

```yaml
services:
  continuum-core:
    # Brain — no ort, no webrtc-sys
    depends_on: [postgres, sensory-server]

  sensory-server:
    # Senses — has ort, no webrtc-sys
    depends_on: [livekit-bridge]
    volumes:
      - voice-models:/app/models:ro
      - ipc-sockets:/root/.continuum/sockets

  livekit-bridge:
    # Transport — has webrtc-sys, no ort
    depends_on: [livekit]
    volumes:
      - ipc-sockets:/root/.continuum/sockets
    environment:
      - LIVEKIT_URL=${LIVEKIT_URL:-ws://livekit:7880}
```

## Implementation Order

1. **`sensory-protocol` crate** — message types, codec (~100 lines)
2. **`livekit-bridge` binary** — move LiveKitAgent/Manager from livekit_agent.rs (~800 lines)
3. **`sensory-server` binary** — move VAD/STT/TTS, add capability router (~600 lines)
4. **Core changes** — remove ort/webrtc deps, add bridge clients (~400 lines)
5. **Docker** — new Dockerfiles, compose update
6. **Test** — end-to-end voice pipeline through all three processes

## Why Three, Not Two

Two binaries (core + bridge) would still require ort in the core for VAD/STT/TTS.
That works but misses the deeper insight: **senses are independent from cognition**.

A game client renders avatars locally and connects directly to the sensory server —
no LiveKit bridge needed. A phone integration connects via SIP bridge — different
transport, same senses. The three-way split makes each layer independently swappable
and testable.

It also enables the Factory vision: forged models graduate from text-only to multimodal,
and the sensory router adapts without changing core or transport code.
