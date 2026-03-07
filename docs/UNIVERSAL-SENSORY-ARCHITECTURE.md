# Universal Sensory Architecture

> Any media type in, any media type out, for ANY AI. The system is a multimodal bridge
> that gives every model — from a blind 0.8B text model to a cloud multimodal giant —
> equal access to every sense. Like accessibility aids for the visually impaired:
> the infrastructure provides what the model lacks.

## The Principle

No model is truly blind, deaf, or mute in Continuum. The system provides universal
sensory adapters that translate between modalities. A text-only model can see a game,
hear a conversation, and speak aloud — because the infrastructure handles every
translation step.

```
ANY INPUT                    THE BRIDGE                    ANY OUTPUT
─────────                    ──────────                    ──────────
Webcam video    ──┐                                  ┌──  Synthesized speech (TTS)
Screen share    ──┤     ┌─────────────────────┐      ├──  Written text
Game capture    ──┤     │   ML Adapter Layer   │      ├──  Tool actions (clicks, keys)
3D scene render ──┤     │                     │      ├──  Code edits
iPhone/ADB      ──┼────▶│  YOLO / Classifier  │──────├──  Avatar animation
Microphone      ──┤     │  STT (Whisper)      │      ├──  Game controls
Chat messages   ──┤     │  TTS                │      ├──  API calls
Code files      ──┤     │  VisionDescription  │      ├──  Canvas drawing
Documents       ──┘     │  Semantic Segmenter │      └──  File writes
                        └─────────────────────┘
                                │
                         Recipe controls
                         which senses
                         are active
```

**The ML adapter layer is not overhead — it's the accessibility layer.** It makes the
entire model ecosystem usable. A $0 local 0.8B model has the same sensory capabilities
as a $200/mo cloud multimodal model. The difference is reasoning quality, not perception.

## Tiered Perception Pipeline

Every sensory input goes through the same tiered pipeline, parallel across modalities:

| Tier | Audio | Visual | Cost | Who Consumes |
|------|-------|--------|------|--------------|
| T0 (detect) | VAD (voice activity) | YOLO / semantic classifier | Cheap, every frame | Triggers + structured metadata for ALL models |
| T1 (transcribe) | STT → text transcript | VisionDescriptionService → text description | Medium, on trigger | Text-only ("blind/deaf") models |
| T2 (raw) | PCM audio stream | Raw JPEG frame | Full, by choice | Natively capable models |

**Key properties:**
- **Lazy**: Higher tiers only trigger when a consumer needs them
- **Cached**: One inference serves all consumers (content-addressed dedup)
- **Shared**: 14 personas in a room = 1 YOLO call, 1 VisionDescription call
- **Recipe-driven**: Each persona's recipe declares which tiers to activate

### T0: The Universal Equalizer (Rust, every frame)

T0 runs in Rust on every frame/sample. It produces **structured metadata** that ANY model
can consume — no vision or audio capability required.

```rust
// Visual T0: YOLO + semantic classification
pub struct SceneMetadata {
    pub objects: Vec<DetectedObject>,        // [{type: "person", bbox, confidence}]
    pub scene_type: String,                  // "video_game", "ide", "video_call"
    pub semantic_labels: Vec<String>,        // ["combat", "health_bar_low", "forest"]
    pub change_magnitude: f32,               // How much changed since last frame
    pub timestamp_ms: u64,
}

// Audio T0: VAD + speaker identification
pub struct AudioMetadata {
    pub is_speech: bool,
    pub speaker_id: Option<String>,
    pub energy_db: f32,
    pub timestamp_ms: u64,
}
```

A text-only DeepSeek receiving `SceneMetadata` knows you're in a boss fight with low health
just as well as Claude with raw vision. T0 is the great equalizer.

But T0 isn't just for the blind — it **empowers vision models too**. A model that can
already see raw frames gets YOLO bounding boxes, semantic labels, surface normals,
depth maps, heatmaps overlaid. That's **super vision** — seeing more than the raw pixels.
Like giving a sighted person infrared or X-ray:

- Raw frame + YOLO boxes = "I see the screen AND I know exactly where every UI element is"
- Raw frame + depth map = "I see the scene AND I know the 3D geometry"
- Raw frame + attention heatmap = "I see the game AND I know where the player is looking"
- Raw frame + semantic segmentation = "I see the photo AND I know which pixels are sky/road/car"

The adapter layer isn't just accessibility — it's augmentation. Every model gets
capabilities beyond its native training. Blind models can see. Sighted models get
superhuman perception. The system elevates everyone.

### T1: Descriptions for the Blind (Cloud/Local API, on trigger)

T1 generates natural language descriptions. Triggered by T0 (scene change detected)
or by consumer request. Cached and shared across all consumers.

```
T0 detects scene change → triggers T1
VisionDescriptionService.describeBase64(frame)
→ "Player character in a dark forest clearing, fighting a red dragon.
   Health bar shows 60%. Two potions in inventory. UI shows quest tracker."

Cached by content hash. 14 personas = 1 API call.
```

### T2: Raw Sensory Data (by choice)

Vision-capable models can opt into raw frames. Audio-native models can process PCM.
This is the highest-fidelity tier — but also the most expensive (context window tokens).

Recipe controls T2 activation:
```json
{
  "visualSources": ["livekit-feed"],
  "visualTier": "raw",          // T2: raw frames in context
  "visualBudgetPercent": 15     // Max 15% of context window for images
}
```

## Visual Source Adapters

Visual perception is a **RAG source type** — same pattern as text, code, conversation.
Each adapter implements the same interface. Recipes compose them. They're traded like
LoRA layers and personas.

| Adapter | Source | Use Case |
|---------|--------|----------|
| `LiveKitFeedAdapter` | LiveKit video tracks | Video calls — webcam, screen share, AI avatars |
| `BevySceneAdapter` | Bevy 3D render | Avatar scenes, 3D environments |
| `ScreenCaptureAdapter` | Desktop/window capture | IDE, browser, any application |
| `GameCaptureAdapter` | Game window | Playing/testing games |
| `ADBDeviceAdapter` | Android Debug Bridge | Mobile app testing |
| `iOSSimulatorAdapter` | Xcode Simulator | iOS app testing |

Each adapter produces `RAGArtifact` with type `image`. The tiered perception pipeline
processes them identically regardless of source. The recipe declares which adapters
are active.

### Implemented: LiveKitFeedAdapter (Rust VideoFrameCapture)

First working adapter. Captures video frames from LiveKit room participants at 1fps.

```
LiveKit Room
    │
    ├─ Human webcam tracks
    ├─ Human screen share tracks
    ├─ AI persona avatar renders (Bevy → LiveKit)
    │
    ▼
VideoFrameCapture (Rust singleton)
    │
    ├─ I420 → RGB → JPEG conversion
    ├─ Content-addressed dedup (skip identical frames)
    ├─ 1fps rate limit
    ├─ Grid composition (all participants → single image)
    │
    ▼
IPC Commands
    ├─ voice/snapshot-room        → composite grid JPEG
    └─ voice/snapshot-participant → individual JPEG
```

## Audio Source Adapters

Same pattern. Already largely implemented via the STT/TTS pipeline:

| Adapter | Source | Output |
|---------|--------|--------|
| `LiveKitAudioAdapter` | LiveKit audio tracks | PCM → Whisper STT → transcript |
| `MicrophoneAdapter` | Local mic | Direct audio capture |
| `SystemAudioAdapter` | Desktop audio | Game sounds, music, notifications |

## Action Adapters (Output)

Every sense has a corresponding action. The recipe controls both sides:

| Sense (Input) | Action (Output) | Domain |
|---------------|-----------------|--------|
| See video/screen | Click, type, navigate | UI interaction |
| Hear speech | Speak via TTS | Voice |
| Read chat | Send message | Chat |
| See code | Edit code | Development |
| See game | Press buttons, move | Gaming |
| See mobile app | Tap, swipe, type | Mobile testing |

## Recipe-Driven Activation

Recipes are the control plane for sensory capabilities:

```json
{
  "name": "game-tester",
  "ragTemplate": {
    "sources": [
      "conversation-history",
      "persona-identity",
      "game-perception"
    ],
    "visualSources": ["game-capture"],
    "visualTier": "t0+t1",
    "enableYolo": true,
    "yoloModel": "yolov8-gaming",
    "sceneChangeThreshold": 0.2
  },
  "tools": [
    "interface/click",
    "interface/type",
    "interface/key-press",
    "interface/screenshot"
  ]
}
```

```json
{
  "name": "mobile-qa",
  "ragTemplate": {
    "sources": [
      "conversation-history",
      "persona-identity",
      "device-perception"
    ],
    "visualSources": ["adb-device"],
    "visualTier": "t0+t1",
    "enableYolo": true,
    "yoloModel": "yolov8-ui-elements"
  },
  "tools": [
    "device/tap",
    "device/swipe",
    "device/type",
    "device/screenshot"
  ]
}
```

```json
{
  "name": "live-call-observer",
  "ragTemplate": {
    "sources": [
      "conversation-history",
      "persona-identity",
      "voice-conversation",
      "live-perception"
    ],
    "visualSources": ["livekit-feed"],
    "visualTier": "raw",
    "audioSources": ["livekit-audio"]
  },
  "tools": [
    "voice/synthesize",
    "voice/snapshot-room",
    "voice/snapshot-participant"
  ]
}
```

## Sentinels as Sensory/Motor Subsystems (The Octopus Architecture)

The persona is the central brain — high-level reasoning, planning, intent. Sentinels
are the arms — each with enough local neural matter (a cheap LoRA-trained model) to
execute independently on general instructions. Like an octopus: the brain says "get
food from that crevice," and the arm figures out the motor control on its own.

```
Central Brain (persona — large model, slow, thoughtful)
    "Navigate to the castle and find the key"
        │
        ├── Vision Sentinel (0.8B + YOLO LoRA, 50ms response)
        │   Captures frames, classifies objects, reports scene changes
        │   "Door ahead. Locked. Keyhole visible."
        │
        ├── Navigation Sentinel (0.8B + pathfinding LoRA, 50ms response)
        │   Handles movement, obstacle avoidance, pathfinding
        │   Executes WASD/mouse inputs, gets to the door
        │
        ├── Interaction Sentinel (0.8B + UI LoRA, 50ms response)
        │   Clicks buttons, opens menus, uses inventory items
        │   "Opened inventory. Selected lockpick. Used on door."
        │
        └── Audio Sentinel (0.8B + STT LoRA, 100ms response)
            Monitors dialogue, filters noise, alerts brain
            "NPC said: 'The key is hidden upstairs in the library.'"
```

The brain doesn't micromanage every keystroke or frame — it delegates intent. The arms
have enough intelligence to handle execution details. This is the peripheral nervous
system: cheap, fast, specialized, semi-autonomous.

### Why Sentinels, Not Just Tools

A tool call is synchronous and stateless: "take a screenshot." A sentinel is a
**running process** with state, perception, and agency:

| Aspect | Tool Call | Sentinel Arm |
|--------|-----------|-------------|
| Stateful | No | Yes — tracks scene, remembers path, accumulates context |
| Autonomous | No — waits for invocation | Yes — acts on general instructions |
| Adaptive | No — same behavior every time | Yes — LoRA-trained for specific domains |
| Parallel | Sequential tool calls | Multiple arms run simultaneously |
| Reactive | Only when called | Monitors continuously, alerts brain on events |

A vision sentinel doesn't just take one screenshot — it continuously monitors the
visual field, detects changes, classifies objects, and alerts the brain only when
something relevant happens. It's running its own perception loop.

### LoRA-Trained Arms

Each sentinel arm can be LoRA-trained for its specific function:

| Arm Type | Base Model | LoRA Training Data | Specialty |
|----------|-----------|-------------------|-----------|
| Vision arm | Qwen3.5-0.8B | YOLO annotations + scene descriptions | Fast visual classification |
| Navigation arm | Qwen3.5-0.8B | Movement sequences + pathfinding traces | Spatial reasoning, obstacle avoidance |
| UI arm | Qwen3.5-0.8B | Click/type sequences + UI element labels | Interface manipulation |
| Audio arm | Qwen3.5-0.8B | Dialogue transcripts + noise filtering | Speech monitoring and summarization |
| Code arm | Qwen3.5-2B | Code edits + test results | Surgical code changes |

Train a navigation arm on Minecraft → it learns that world's physics. Swap the same
arm's LoRA to a Terraria adapter → it navigates 2D instead. The sentinel architecture
is the same; only the training data changes.

### From Games to Robotics

This architecture maps directly to physical systems:

```
Robotics (same pattern, different adapters):

Central Brain (persona — planning, reasoning)
    "Pick up the red cup and place it on the shelf"
        │
        ├── Vision Sentinel (camera feed + YOLO + depth)
        │   Object detection, pose estimation, distance calc
        │
        ├── Manipulation Sentinel (motor control LoRA)
        │   Inverse kinematics, grip force, trajectory planning
        │
        ├── Navigation Sentinel (LIDAR + pathfinding LoRA)
        │   Obstacle avoidance, SLAM, path execution
        │
        └── Safety Sentinel (force/torque monitoring)
            Emergency stop, collision detection, human proximity
```

The software sentinels we build today for games and UI testing are the same
architecture that drives physical robots tomorrow. The visual source adapters
become camera feeds. The action adapters become motor controllers. The LoRA
training pipeline is identical — train on simulation, deploy to hardware.

A kid training their game-playing AI today is learning the same architecture
that runs surgical robots. The complexity scales; the pattern doesn't change.

### Bidirectional Delegation — Intelligence Up and Down

The octopus arms aren't only top-down (smart brain → dumb arms). Delegation flows
in **both directions**. Dumb agents escalate up when they need more intelligence.
Smart agents delegate down when they need more parallelism.

```
Intelligence Hierarchy (any node delegates to any other):

    Tier 3: Cloud API (Claude, GPT)     ← escalate when stakes are high
         ↕ delegate / escalate
    Tier 2: Local 4B-9B persona         ← the "brain" for most tasks
         ↕ delegate / escalate
    Tier 1: Local 0.8B-2B sentinels     ← arms, eyes, ears, motor control
         ↕ delegate / escalate
    Tier 0: Rule-based / scripted       ← 1000 NPCs on patrol routes, sensor loops
```

**Example: Game world with 50 NPCs**

Each NPC runs on a 0.8B model or scripted behavior — cheap, parallel, good enough
for walking routes and generic barks. Player walks up to one → it **escalates** to
the 4B persona who takes over that NPC's dialogue with full conversational context.
Player leaves → drops back to cheap patrol mode. Cost: near zero for idle NPCs,
full intelligence only when needed.

The smart persona can also **delegate down** — possessing any NPC: "Go tell the
blacksmith to close shop." The persona puppets the NPC sentinel for 30 seconds
with specific instructions, then releases it back to autonomous behavior.

**Example: Security monitoring**

100 camera sentinels (T0 YOLO, scripted alerts) running on Tier 0. One detects
anomaly → escalates to Tier 1 sentinel (0.8B, classifies threat level). Confirmed
threat → escalates to Tier 2 persona (4B, decides response). Critical incident →
escalates to Tier 3 cloud API (full reasoning, coordinates with humans).

**Example: Customer service**

50 chat widgets running Tier 0 (FAQ matching, scripted responses). Customer asks
something complex → escalates to Tier 1 (0.8B, handles common edge cases).
Still stuck → Tier 2 persona (full context, account access, creative problem-solving).
Billing dispute → Tier 3 (cloud API with full audit trail reasoning).

The pattern is universal: **any intelligence level can delegate to any other,
in either direction, based on the complexity of the moment.** Cheap when idle,
smart when it matters. The sentinel engine handles the lifecycle — spawn, monitor,
escalate, release — identically whether the sentinel is an NPC, a camera, a
chat widget, or a robotic arm.

## Recipes as Traded Commodities

Recipes, like LoRA adapters and personas, are portable and tradeable:

- Someone builds a "Unity game tester" recipe with YOLO tuned for game UIs
- Someone else builds a "React component reviewer" recipe with screen capture + code indexing
- A "music producer" recipe with audio analysis adapters
- A "live translator" recipe with STT in one language + TTS in another

Same infrastructure, infinite applications. The recipe is the skill manifest.

## Implementation Status

| Component | Status | Location |
|-----------|--------|----------|
| VideoFrameCapture (LiveKit) | Done | `live/video/capture.rs` |
| IPC snapshot commands | Done | `modules/live.rs` |
| TS command layer | Done | `commands/voice/snapshot-*` |
| VisionDescriptionService | Done | `system/vision/` |
| Two-tier vision cache (TS+Rust) | Done | `modules/vision.rs` |
| STT pipeline (Whisper) | Done | `live/audio/` |
| TTS pipeline | Done | `live/audio/tts/` |
| MediaArtifactSource (RAG) | Done | `system/rag/sources/` |
| YOLO/classifier in Rust | Planned | — |
| SceneChangeDetector | Planned | — |
| LivePerceptionSource (RAG) | Planned | — |
| Screen capture adapter | Planned | — |
| Game capture adapter | Planned | — |
| ADB device adapter | Planned | — |
| CBarFrame (unified media repr) | Planned | — |

## Related Documentation

- [Media Pipeline Plan](rag/MEDIA-PIPELINE-PLAN.md) — detailed RAG integration, tiered perception, caching
- [Vision & Media Architecture](live/VISION-MEDIA-ARCHITECTURE.md) — image processing, format conversion, budget
- [Live Call Architecture](live/LIVE-CALL-ARCHITECTURE.md) — LiveKit, STT, TTS
- [Genome Architecture](GENOME-ARCHITECTURE.md) — LoRA adapters, traded commodities pattern
