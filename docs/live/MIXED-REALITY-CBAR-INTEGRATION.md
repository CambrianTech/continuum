# Mixed Reality — CBAR Integration for Immersive Continuum

**Status**: Architecture direction. The CBAR substrate from [open-eyes](https://github.com/CambrianTech/open-eyes) is the sensory layer that mixed reality needs. This doc describes how VR/AR devices integrate into continuum as camera sources feeding the same 3D scene pipeline that powers security cameras — same engine, different devices, shared world model.

---

## The Insight: Every Device Is Just a Camera Source

A cheap $20 wireless security camera, an iPhone, a Vision Pro, a Meta Quest, and a Ring doorbell are all the same thing to the CBAR pipeline: **a source of timestamped image frames with known or discoverable intrinsics.** The pipeline doesn't care about the device — it cares about the Frame.

| Device | Intrinsics | Pose Source | Frame Rate | Special Capabilities |
|---|---|---|---|---|
| Cheap wireless camera | Fixed (calibrate once) | Static (self-regulating drift detection) | 15-30fps | Cheap, numerous, wireless |
| iPhone (ARKit) | Known from Apple | ARKit adapter (IMU + visual) | 30-60fps | Depth (LiDAR on Pro), mobile |
| Vision Pro | Known from Apple | Head tracking (sub-mm precision) | 90fps | Depth, eye tracking, hand tracking |
| Meta Quest | Known from Meta | Inside-out tracking (SLAM) | 72-90fps | Passthrough, hand tracking |
| USB webcam | Calibrate via checkerboard | Static (mounted) or ORB flow | 30fps | Cheap, common |
| Ring / Wyze | Reverse-engineered | Static | 15fps | Already deployed everywhere |

**Each device gets a `CameraSource` adapter** (same "adapters not branches" principle). The adapter handles device-specific setup (RTSP stream, ARKit session, WebXR API). What comes out is always a `CameraFrame` with intrinsics + pose + timestamp + image. The pipeline is identical downstream.

---

## What Mixed Reality Adds to the Security Pipeline

The open-eyes security pipeline already builds a 3D scene from stationary cameras. Mixed reality devices add:

1. **A moving viewpoint** — the human wearing a headset walks through the scene that static cameras mapped. The headset's pose (from ARKit/Quest tracking) places the human in the shared 3D model. The static cameras continue to update the world around them.

2. **Depth sensing** — LiDAR (Vision Pro, iPhone Pro) and structured-light (Quest) provide direct depth measurements that improve the 3D reconstruction. Static cameras estimate depth from monocular cues and cross-camera stereo; depth-sensing devices provide ground truth that anchors the estimates.

3. **Close-range detail** — static cameras see the big picture (property perimeter, room layout). A human with a headset walking through adds close-range detail (furniture surfaces, object textures, face details). The fusion engine merges both scales into one model.

4. **Interaction** — the human in the headset can interact with the 3D scene: select an entity, ask a persona about it, set alert boundaries, adjust camera views. The headset is both a sensor (contributing frames) and a display (consuming the fused 3D scene).

---

## The Continuum Integration

```
┌─────────────────────────────────────────────────────────┐
│                    CONTINUUM GRID                        │
│                                                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐             │
│  │ Camera   │  │ Camera   │  │ Vision   │             │
│  │ Node 1   │  │ Node 2   │  │ Pro Node │             │
│  │ (static) │  │ (static) │  │ (mobile) │             │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘             │
│       │              │              │                    │
│       ▼              ▼              ▼                    │
│  ┌─────────────────────────────────────────┐            │
│  │         FUSION ENGINE (grid node)        │            │
│  │  Merges all camera sources into one      │            │
│  │  3D world model. Static cameras provide  │            │
│  │  the skeleton; mobile devices add detail. │            │
│  └────────────────────┬────────────────────┘            │
│                       │                                  │
│       ┌───────────────┼───────────────┐                 │
│       ▼               ▼               ▼                 │
│  ┌─────────┐  ┌──────────────┐  ┌──────────┐          │
│  │ Splat   │  │ Security     │  │ Alex     │          │
│  │ Renderer│  │ Personas     │  │ Cadence  │          │
│  │ (3D     │  │ (threat      │  │ Mediator │          │
│  │  view)  │  │  assessment) │  │          │          │
│  └─────────┘  └──────────────┘  └──────────┘          │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**Every component is a grid participant.** The camera nodes run the lightweight CBAR pipeline on-device. The fusion engine runs on the strongest grid node. The splat renderer runs where the display is (headset GPU, or streamed from the grid). Security personas run on any node with inference capacity. Alex mediates the conversation between personas and the human at their preferred cadence.

---

## The CBAR Pipeline on Mixed Reality Devices

The same `open-eyes-core` crate runs on every device. On mixed reality devices:

**Tier 1 (synchronous heartbeat):**
- Optical flow at quarter-res → motion detection → "is anything changing?"
- On VR/AR: this detects BOTH scene motion and head motion
- Head motion is filtered out using the platform's IMU/tracking data (adapter)
- Scene motion (someone walking into the room) triggers tier 2

**Tier 2 (lazy, on-demand):**
- Feature extraction → cross-camera matching with static cameras
- Surface normals → lighting estimation for realistic AR overlay
- Semantic segmentation → "this is a table, this is a person, this is a wall"
- Entity detection → "person detected, tracking"

**Platform adapters provide for free:**
- ARKit: pose, depth, plane detection, face tracking, hand tracking
- Quest: pose, passthrough, hand tracking, room mesh
- Open-eyes pure-CV fallback: pose from features, depth from stereo/mono CNN

The adapter wraps whatever the platform gives. The pipeline consumes it uniformly.

---

## Connection to the Immersive Vision

This is the sensory layer that the continuum 3D immersive world (docs/IMMERSIVE-SOCIAL-ARCHITECTURE.md, docs/3D-IMMERSIVE-VISION.md) needs:

- **The Tron room** from CONVERSATIONAL-CADENCE-ARCHITECTURE.md becomes a REAL room when mixed reality devices contribute their view to the shared 3D model. You're standing in a real room; the AI personas appear in it via AR overlay; the static cameras provide the security context; the splat renderer provides the visual quality.

- **Alex the cadence mediator** from CONVERSATIONAL-CADENCE-ARCHITECTURE.md operates in the same fused 3D scene — mediating the conversation between personas and the human, with spatial audio located at the personas' positions in the real/virtual room.

- **The Gaussian LoD primitive** applies to the 3D scene itself: high fidelity near the human's gaze (from the headset's depth sensor + close-range cameras), smooth falloff to coarse representation at distance (from static cameras with lower resolution). Same continuous Gaussian attention-weighted summarization, applied to spatial rendering instead of conversation.

---

## Future: Continuum Core Rework

The CBAR pattern (Frame + ProcessNode + Pipeline) will eventually become the substrate for continuum's core daemon architecture:

- **Better daemons** — each daemon becomes a Pipeline with domain-specific ProcessNodes, replacing the current imperative loop with a lazy-evaluated reactive model
- **Better IPC** — the Frame is the message format between daemons. One daemon's output Frames feed another daemon's input. The existing Unix socket IPC carries Frames instead of ad-hoc JSON messages.
- **Better responsiveness** — lazy evaluation means compute cost is proportional to what the current request actually needs. A persona responding to a simple "hi" doesn't compute RAG context, tool relevance, and genome paging — those lazy getters don't fire unless the response pipeline pulls them.
- **Existing base adapters stay** — the adapter pattern is already the right pattern. The rework is in the SUBSTRATE (how data flows between adapters), not in the adapters themselves.

The rework is NOT urgent — continuum works today. But when responsiveness and efficiency become the priority, the CBAR pattern is the target architecture, and open-eyes-core is the reference implementation to follow.

---

## See also

- `docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` — the full pattern description with code examples
- `docs/CONVERSATIONAL-CADENCE-ARCHITECTURE.md` — Alex + LoD (same attention-weighted primitives)
- `docs/IMMERSIVE-SOCIAL-ARCHITECTURE.md` — the 3D immersive world these sensors feed
- [open-eyes](https://github.com/CambrianTech/open-eyes) — the Rust implementation
- [react-home-ar](https://github.com/CambrianTech/react-home-ar) — the original TypeScript/C++ implementation
