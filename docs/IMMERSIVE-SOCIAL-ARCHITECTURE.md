# Immersive Social Architecture

## Vision

Shareable, interactive AI persona experiences across any device — VR headset, iPhone, Mac, browser. Users create characters with custom personalities, voices, and appearances. AI personas inhabit shared rooms where humans and AIs interact naturally. Content is instantly shareable to TikTok, Discord, Slack, or any social platform.

**Inspiration**: Viral VR content (e.g., AI presidents playing Pictionary — 133K likes, 22.8K shares) proves massive demand for interactive AI characters. Our system has the infrastructure to deliver this at scale.

## Architecture: Two Rendering Modes

### Mode A: Server-Rendered Streaming (Available Now)

```
┌─────────────────────────────────────────────────┐
│  Server (Mac / 5090)                            │
│  ┌─────────────┐  ┌──────────────┐              │
│  │ Bevy Renderer│  │ Persona Brains│             │
│  │ 16 VRM slots │  │ RAG + LLM    │             │
│  │ 640×360→HD   │  │ Voice Synth  │             │
│  └──────┬──────┘  └──────┬───────┘              │
│         │ RGBA frames    │ Audio                 │
│         └────────┬───────┘                       │
│           LiveKit WebRTC                         │
└─────────────────┬───────────────────────────────┘
                  │ Video + Audio streams
     ┌────────────┼────────────┐
     ▼            ▼            ▼
  iPhone       Browser      Oculus
  (thin)       (thin)       (thin)
```

- Server renders all avatars, streams pixels via LiveKit
- Clients are pure receivers — zero GPU, any device works
- Latency: ~50-100ms (acceptable for social, marginal for VR hand tracking)
- **This path works TODAY** with existing infrastructure

### Mode B: Hybrid Client Rendering (Target for VR)

```
┌──────────────────────────────┐
│  Server (Mac / 5090)         │
│  ┌──────────────┐            │
│  │ Persona Brains│           │
│  │ RAG + LLM    │           │
│  │ Voice Synth  │           │
│  │ Animation    │           │
│  │  Decisions   │           │
│  └──────┬───────┘           │
│         │ Bone transforms,  │
│         │ morph weights,    │
│         │ audio (~1KB/frame)│
└─────────┬───────────────────┘
          │ Data channel
     ┌────┴────────────┐
     ▼                 ▼
  Oculus Quest      iPhone
  ┌──────────┐   ┌──────────┐
  │ Local Bevy│   │ Local Bevy│
  │ VRM render│   │ VRM render│
  │ Vulkan    │   │ Metal     │
  └──────────┘   └──────────┘
```

- Server sends **decisions** (speak, emote, gesture), not pixels
- Client loads same VRM models, renders locally
- ~1KB/frame bone state vs ~900KB/frame video = 900x bandwidth reduction
- Bevy compiles natively: Quest (Android/Vulkan), iPhone (iOS/Metal), Mac (Metal)
- VR gets 6DoF head tracking, spatial audio, hand interaction
- The modular `bevy_renderer/` architecture makes this possible:
  - `types.rs` — shared animation state (compiles on any platform)
  - `animation.rs` — bone/morph computation (runs client-side in Mode B)
  - `vrm.rs` — model parsing (same VRM files everywhere)

## Compute Distribution

| Workload | Where | Notes |
|----------|-------|-------|
| LLM inference | Server (5090 / Mac) | Persona cognition, response generation |
| Voice synthesis | Server | TTS, voice cloning, audio processing |
| LoRA training | Server (5090 preferred) | Works on Mac (slower), 5090 for speed |
| Voice LoRA/QLoRA | Server (5090) | Custom voice training — planned feature |
| Avatar rendering | Server (Mode A) or Client (Mode B) | Depends on device capability |
| Audio I/O | Client | Microphone capture, speaker output |
| Spatial tracking | Client (VR only) | Head/hand pose from Oculus sensors |

**Shared compute**: 5090 handles heavy lifting (training, bulk inference). Mac handles real-time rendering and persona hosting. Mobile/VR clients handle local rendering (Mode B) or pure reception (Mode A).

## Shareability — The Social Layer

A "scene" is a room with loaded personas. Sharing is:

1. **Room link** — join the conversation, see the avatars
2. **Persona export** — personality config + LoRA adapter + VRM model + voice profile
3. **Recording** — screen capture of rendered scene → TikTok, YouTube, etc.
4. **Live bridge** — persona calls into Discord voice channel, Slack huddle, etc.
5. **Embed** — WebRTC widget embeddable in any webpage

The reticulum P2P mesh handles room discovery and peer routing. No central server required for room hosting — any node can host.

## Creator UX (Future)

```
"Make me a character that talks like X"
  → Personality shaping (system prompt + RAG context)
  → Voice cloning / QLoRA voice training
  → VRM model selection or generation
  → Academy session for skill training
  → Share to room → share to world
```

The AI team assists creation. PersonaUsers help shape the personality, test the voice, iterate on the character. The creator workflow is conversational.

## Platform Targets

| Platform | Rendering | Input | Status |
|----------|-----------|-------|--------|
| Mac (browser) | Server-rendered | Keyboard/mouse | **Working** |
| iPhone/Android | Mode A (stream) or Mode B (local Bevy) | Touch, voice | Planned |
| Oculus Quest | Mode B (local Bevy + spatial) | 6DoF, hand tracking | Planned |
| Desktop VR | Mode B (local Bevy + SteamVR) | 6DoF, controllers | Planned |
| Discord/Slack | Audio-only bridge | Voice | Planned (LiveKit bridge) |

## Implementation Order

1. **Stabilize current system** — memory management, render quality, persona reliability
2. **Custom voice QLoRA/LoRA training** — voice cloning pipeline
3. **Data channel bone streaming** — Mode B foundation (server sends transforms, not pixels)
4. **Mobile Bevy client** — iOS/Android app with local VRM rendering
5. **VR client** — Oculus Quest app with spatial tracking
6. **Creator UX** — conversational character creation workflow
7. **Social bridges** — Discord, Slack, TikTok export

## Key Files

| Component | Path | Role |
|-----------|------|------|
| Bevy renderer | `workers/continuum-core/src/live/video/bevy_renderer/` | Avatar rendering (5 modules) |
| Animation types | `bevy_renderer/types.rs` | Shared state — compiles on any platform |
| LiveKit transport | `workers/continuum-core/src/live/transport/` | WebRTC streaming |
| Persona system | `system/user/server/PersonaUser.ts` | AI personality + cognition |
| Genome/LoRA | `system/genome/` | Adapter training + management |
| Voice synthesis | `workers/continuum-core/src/live/audio/` | TTS + audio processing |
