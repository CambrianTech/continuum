# CBAR Substrate Architecture — The Pattern Continuum Will Adopt

**Status**: Architecture reference. The CBAR pattern from [react-home-ar](https://github.com/CambrianTech/react-home-ar) is the cleanest streaming-compute architecture in the Cambrian ecosystem. It should be the reference pattern for all streaming pipelines in continuum, and the basis for future responsiveness improvements.

**Rust implementation**: [open-eyes-core](https://github.com/CambrianTech/open-eyes) (`crates/open-eyes-core/src/frame.rs`)

---

## The Pattern

Three components, zero coupling:

### 1. Frame (the shared data bus)

A single immutable object that wraps a raw input (camera frame, audio chunk, inference request) with **lazy-computed derived outputs**. Each output is a `OnceLock<T>` that computes on first access and caches forever.

```rust
pub struct Frame {
    raw: image::RgbImage,
    timestamp: f64,
    
    // Lazy outputs — compute on first access, cache forever
    greyscale: OnceLock<GrayImage>,
    edges: OnceLock<EdgeMap>,
    features: OnceLock<Vec<FeaturePoint>>,
    normals: OnceLock<NormalMap>,
    semantic: OnceLock<SemanticMap>,
    optical_flow: OnceLock<FlowField>,
}

impl Frame {
    pub fn greyscale(&self) -> &GrayImage {
        self.greyscale.get_or_init(|| image::imageops::grayscale(&self.raw))
    }
    
    pub fn features(&self) -> &Vec<FeaturePoint> {
        self.features.get_or_init(|| {
            let grey = self.greyscale(); // chains — computes greyscale if not yet cached
            extract_features(grey)
        })
    }
}
```

**Key properties:**
- **Any concern can read any other concern's output** — the Frame IS the pub/sub bus
- **Compute cost is proportional to what's actually requested** — if nobody needs edges, edge detection never runs
- **Thread-safe via OnceLock** — share via `Arc<Frame>` across processing threads/tasks
- **Dependencies chain automatically** — `features()` calls `greyscale()` internally; greyscale computes once regardless of how many nodes need it
- **Resolution-agnostic** — each output can be at any resolution. A quarter-res flow field and a full-res edge map coexist on the same Frame. Consumers interpolate to what they need.
- **GPGPU-transparent** — the compute function inside each lazy getter can dispatch to wgpu/Metal/CUDA. The Frame doesn't care. Swapping CPU↔GPU is a per-getter decision invisible to consuming nodes.

### 2. ProcessNode (the subscriber)

An independent processing unit that receives Frames and pulls what it needs. Zero knowledge of other nodes.

```rust
pub trait ProcessNode: Send + Sync {
    fn name(&self) -> &str;
    fn enabled(&self) -> bool { true }
    fn update(&mut self, frame: &Frame) -> Vec<PipelineEvent>;
}
```

**Key properties:**
- **Nodes subscribe to inputs by calling lazy getters** — no explicit subscription registration. A node that needs features calls `frame.features()`. A node that needs normals calls `frame.normals()`. The dependency graph is implicit in the code.
- **Disabled nodes cost zero** — `enabled()` returns false, node is skipped entirely
- **Each node is a thread/task** — in the C++17 version, each node is a pthread with its own event loop. In Rust, each node is a tokio task or rayon work item. The Frame is the shared data bus passed between them.
- **Adding a node cannot break existing nodes** — zero coupling. New node, new file, register it with the pipeline, done.

### 3. Pipeline (the orchestrator)

Manages the node list and feeds Frames through. Thin — just a loop.

```rust
pub struct Pipeline {
    nodes: Vec<Box<dyn ProcessNode>>,
}

impl Pipeline {
    pub fn process_frame(&mut self, raw: RgbImage, ...) -> Vec<PipelineEvent> {
        let frame = Frame::new(raw, ...);
        let mut events = Vec::new();
        for node in &mut self.nodes {
            if node.enabled() {
                events.extend(node.update(&frame));
            }
        }
        events
    }
}
```

---

## The Two-Tier Compute Model

Not all outputs run at the same frequency. The architecture has two tiers:

**Tier 1: Synchronous (every frame, GPU, low-res)**
- Optical flow at quarter resolution
- This is the HEARTBEAT — if flow says nothing's moving, everything else sleeps
- Runs on GPU textures/framebuffers that already exist at the right size
- One synchronous process, full frame rate

**Tier 2: Lazy/Event-driven (on demand, CPU or GPU, any resolution)**
- Feature extraction (triggered by motion detection)
- Surface normals (CNN, runs every Nth frame or on scene change)
- Semantic segmentation (forged model, runs on demand)
- Edge detection (for plane estimation, runs rarely)
- Entity detection (YOLO variant, triggered by motion)

The tier 1 heartbeat drives tier 2 activation. If the flow field shows no motion, tier 2 nodes never wake up. If flow shows motion in region R, only nodes that care about region R activate. **Compute cost is proportional to what's actually happening in the scene.**

---

## Three Levels of Recycling

1. **Per-frame (Frame's OnceLock)** — within one frame, computed outputs are cached. Multiple nodes requesting greyscale get the same cached result.

2. **Cross-frame (Scene cache)** — the static scene model (planes, normals, semantic labels) is computed once and recycled across thousands of frames. Only dynamic elements (entities, motion) update per-frame.

3. **Cross-camera (Fusion engine)** — the shared world model is maintained across all cameras. Calibration is one-time (with self-regulating updates). Per-camera processing is independent; only the fusion layer merges outputs.

---

## Self-Regulating Calibration

Stationary cameras don't need per-frame pose estimation. The calibration is:
1. **One-time**: cross-camera feature matching → relative pose solve
2. **Self-regulating**: optical flow detects global drift (camera bumped) → recalibration triggers automatically
3. **The heartbeat IS the drift detector** — the same optical flow that detects scene motion also detects camera motion. If ALL features shift uniformly, the camera moved, not the scene.

No ARKit. No accelerometer. No external tracking. Just features and flow.

---

## Platform Adapters (not branches)

If the device provides capabilities natively (ARKit pose, ARCore depth, LiDAR point clouds), wrap them as adapters:

```rust
trait PoseProvider: Send + Sync {
    fn current_pose(&self) -> Option<Transform>;
}

struct ARKitPoseAdapter { /* wraps ARKit */ }
struct FeatureTrackingPoseAdapter { /* pure CV fallback */ }
```

Both implement `PoseProvider`. The pipeline doesn't care which one provides the data. Same "adapters not branches" principle as continuum's model family adapters.

---

## Where This Applies in Continuum

The CBAR pattern generalizes beyond cameras. Every streaming-compute pipeline in continuum could use this architecture:

| Domain | Raw Input | Lazy Outputs | Heartbeat |
|---|---|---|---|
| **Camera/Security** | RGB frame | greyscale, edges, features, normals, semantic, flow | optical flow |
| **Audio/Voice** | PCM chunk | spectrogram, VAD, transcription, speaker embedding | VAD energy |
| **AI Inference** | token sequence | attention weights, hidden states, logits, tool calls | token generation |
| **Persona Cognition** | inbox message | RAG context, tool relevance, priority score, response draft | inbox poll |
| **Live Call** | WebRTC frame | transcription, facial expression, gesture, speaking state | audio energy |

Each row is a Pipeline with domain-specific ProcessNodes pulling from a domain-specific Frame. The pattern is the same; only the types change.

**When continuum's responsiveness improves**: the CBAR substrate is the target architecture. Replace the current imperative persona-cognition cycle with a lazy-evaluated Frame-based pipeline, and the per-cycle compute cost drops to only what the current conversation actually requires — same way CBAR drops camera processing to only what motion requires.

---

## The open-eyes Implementation

[open-eyes-core](https://github.com/CambrianTech/open-eyes) is the first Rust implementation of this pattern:

- `frame.rs` — Frame + ProcessNode trait + Pipeline (the full pattern)
- `geometry/` — 3D math (projection, triangulation, RANSAC plane fitting)
- `features/` — two-tier feature architecture (flow heartbeat + lazy ORB)
- `fusion/` — N-camera fusion engine with self-regulating calibration

19 tests validate the core math and the lazy-evaluation semantics.

The same `open-eyes-core` crate will serve both security cameras AND mixed-reality devices (VR/AR headsets are just more camera sources feeding the same fusion engine). The on-device part is lightweight and fast; the grid part (AI, splats, persona reasoning) is heavy and distributed.

---

## References

- `react-home-ar/src/core/internal/pipeline/CBARPipeline.ts` — the original TypeScript pipeline
- `react-home-ar/src/core/internal/CBARFrame.ts` — the original lazy-evaluated Frame
- `react-home-ar/src/core/internal/pipeline/CBARProcessNode.ts` — the original subscriber interface
- `open-eyes/crates/open-eyes-core/src/frame.rs` — the Rust port (this is the reference implementation going forward)
- `docs/CONVERSATIONAL-CADENCE-ARCHITECTURE.md` — Alex's LoD primitive (same Gaussian attention-weighted summarization applied to conversation instead of vision)
- `docs/personas/AUTONOMOUS-PERSONA-ARCHITECTURE.md` — the persona cognition cycle that could adopt this pattern
