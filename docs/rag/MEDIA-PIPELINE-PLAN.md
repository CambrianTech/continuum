# Media & Perception Pipeline Architecture

## Vision: Unified Perception/Action Loop

AIs in Continuum can **see, hear, speak, and act** — each through the same adapter pattern.
The perception pipeline feeds RAG sources. The action pipeline provides tools. The recipe
controls which senses are active and which actions are available.

```
PERCEIVE (inputs → RAG sources)              ACT (tools → outputs)
─────────────────────────                    ─────────────────────
See:  video/screenshot/UI state              Speak: TTS → LiveKit
Hear: audio stream → STT                    Click:  interface/click, type
Read: chat, code, documents                  Draw:   canvas/stroke/add
                    │                                    │
                    ▼                                    ▲
              ┌──────────┐                         ┌──────────┐
              │   RAG    │──→ LLM Inference ──→    │  Tools   │
              │ Sources  │                         │ Executor │
              └──────────┘                         └──────────┘
                    │                                    │
              Recipe controls                    Recipe controls
              which sources                      which tools
              activate                           are available
```

### Tiered Perception (parallel to audio pipeline)

| Tier | Audio Pipeline | Visual Pipeline | Cost |
|------|---------------|-----------------|------|
| T0 (trigger) | VAD (voice activity) | YOLO / semantic segmentation | Cheap, every frame |
| T1 (transcribe) | STT transcription | VisionDescriptionService | Medium, on trigger |
| T2 (raw) | Audio-native models hear PCM | Vision models see raw frame | Full, by choice |

Each tier flows into the timeline. All consumers read from the timeline.
Processing is lazy, cached, and shared — one inference serves all consumers.

### Use Cases Enabled

- **Live chat**: AIs see shared images, describe them for text-only peers
- **Live video**: AIs see the room, each other, the human — by choice via recipe
- **Web browsing**: AIs screenshot pages, click buttons, navigate autonomously
- **Gaming**: AIs see the screen (YOLO for objects), click/type to play
- **Code review**: AIs see screenshots of UI, correlate with code changes
- **Whiteboard**: AIs see drawings, describe diagrams, suggest changes

All use the same pipeline: source → MediaFrame → lazy representation → RAG → LLM → tool action.

---

## Status

| Priority | Description | Status |
|----------|------------|--------|
| P1 | Fix artifact extraction race condition | DONE (artifacts=0.0ms cache hit) |
| P2 | MediaArtifactSource as proper RAGSource | Planned |
| P3 | Vision description cache + in-flight dedup | DONE (1 call, not 11) |
| P4 | MediaFrame interface (CBarFrame) | Designed below |
| P5 | Recipe-driven media config | Designed below |
| P6 | LivePerceptionSource for video/avatar | Designed below |

---

## Completed Fixes

### P1: Race Condition Fix (DONE)

Media artifacts (images, screenshots, drag-and-drop files) were broken in RAG context.

### Root Cause: Race Condition in ChatRAGBuilder

**Modular pipeline** (ChatRAGBuilder.ts lines 248-283):

```
extractArtifacts()  ─┐
loadRecipeContext()  ─┼─ Promise.all (parallel)
loadLearningConfig() ─┘
         │
         ▼
composer.compose()  ← ConversationHistorySource populates cache HERE
```

`extractArtifacts()` tries to read `ConversationHistorySource.getCachedRawMessages()` — but the
cache is empty because compose hasn't run yet. Falls through to a cold DB query that:
1. Duplicates work (same messages queried twice)
2. May miss messages (different query shape, no joins)
3. Has a separate 3s TTL cache that drifts from the 30s ConversationHistorySource cache

### Latency Budget

The entire RAG build is on the hot path between "human speaks" and "AI responds".
Current breakdown (from logs):
- Recipe + Learning: ~5-15ms (parallel)
- Compose (all RAG sources): ~30-80ms
- Artifact extraction: ~10-40ms (DB query that shouldn't happen)
- Vision preprocessing: ~200-2000ms per image (vision API call)

**Goal**: Artifact extraction = ~0ms (cache hit). Vision processing = lazy, cached, off hot path.

---

## Priority 1: Fix the Race Condition (Immediate)

**Change**: Move `extractArtifacts()` to run AFTER `composer.compose()`.

The compose phase populates ConversationHistorySource's cache. Artifact extraction then
reads from cache (~0ms) instead of hitting DB (~10-40ms). Net result: faster AND correct.

**File**: `src/system/rag/builders/ChatRAGBuilder.ts`

Before:
```typescript
const [extractedArtifacts, recipe, learning] = await Promise.all([
  extractArtifacts(),   // ← cache miss, hits DB
  loadRecipe(),
  loadLearning()
]);
const composition = await composer.compose(sourceContext);
```

After:
```typescript
const [recipe, learning] = await Promise.all([
  loadRecipe(),
  loadLearning()
]);
const composition = await composer.compose(sourceContext);  // populates cache
const extractedArtifacts = includeArtifacts
  ? await this.extractArtifacts(contextId, maxMessages)    // cache hit, ~0ms
  : [];
```

**Risk**: Low. Sequential but faster (cache hit vs DB query).
**Verification**: Add timing log, confirm extractArtifacts shows ~0ms after compose.

---

## Priority 2: MediaArtifactSource (Proper RAG Source)

**Why**: extractArtifacts + preprocessArtifactsForModel are 200 lines of media logic
inlined in ChatRAGBuilder. They should be a proper RAGSource that:
- Participates in the compose phase
- Respects recipe activation (`activeSources`)
- Has its own budget allocation
- Returns artifacts in RAGSection (the interface already supports it: `RAGSection.artifacts`)

**New file**: `src/system/rag/sources/MediaArtifactSource.ts`

```typescript
export class MediaArtifactSource implements RAGSource {
  readonly name = 'media-artifacts';
  readonly priority = 65;  // Medium — enriches but not critical
  readonly defaultBudgetPercent = 5;

  isApplicable(context: RAGSourceContext): boolean {
    // Active when recipe includes media, or always in chat domain
    return true;
  }

  async load(context: RAGSourceContext, budget: number): Promise<RAGSection> {
    // Read from ConversationHistorySource cache (populated by composer before us)
    // Extract media items from messages
    // Preprocess for model capabilities (vision vs text-only)
    // Return artifacts in RAGSection
  }
}
```

**Key design decisions**:
- Source reads from ConversationHistorySource cache (same compose phase = populated)
- Vision preprocessing is LAZY — only describe images when a non-vision model requests
- Descriptions are CACHED on the artifact object — process once, serve all consumers
- Budget controls how many artifacts to include (not all 50 images in history)

---

## P3: Vision Description Cache (DONE)

**Problem**: 11 personas sharing one image = 11 independent LLaVA calls (100-150s each).

**Fix**: Content-addressed cache + in-flight deduplication in `VisionDescriptionService`.
- Key: SHA-256 of first 4KB + total length (~0ms to compute)
- In-flight dedup: concurrent callers await the same promise
- LRU cache: 500 entries, 5 min TTL
- **Result**: 1 `Selected model` + 9 `Coalescing` in production logs

---

## Priority 4: CBarFrame — Unified Media Representation (Future)

The AR-inspired "one frame, many representations" pattern. A single media item
lazily populates representations as consumers with different capabilities request them.

```typescript
interface MediaFrame {
  id: string;
  sourceType: 'image' | 'audio' | 'video' | 'screen';
  timestamp: number;

  // Lazily populated representations
  representations: Map<MediaRepresentationType, MediaRepresentation>;

  // Request a specific representation — processes on first access, caches after
  getRepresentation(type: MediaRepresentationType): Promise<MediaRepresentation>;
}

type MediaRepresentationType =
  | 'image/base64'          // Raw image data
  | 'image/description'     // Text description (for text-only models)
  | 'image/objects'         // YOLO detection results
  | 'audio/pcm'            // Raw audio
  | 'audio/transcript'     // STT result
  | 'video/frames'         // Key frames
  | 'video/summary';       // Text summary
```

**Key principle**: Processing is triggered by consumer capability, not eagerly.
- Vision model requests `image/base64` → instant (already have it)
- Text model requests `image/description` → triggers vision API (once), caches
- Audio model requests `audio/pcm` → instant
- Text model requests `audio/transcript` → triggers STT (once), caches

**"So the blind can see"**: Text-only models get descriptions. Audio-only models
get transcripts. Everyone gets the representation their capabilities support.
AIs naturally help each other — a vision model's description enters the timeline,
text-only models "see" through that description.

---

## Priority 5: Recipe-Driven Media Activation

Recipes should control which media sources activate:

```json
{
  "ragTemplate": {
    "sources": ["conversation-history", "persona-identity", "media-artifacts"],
    "mediaConfig": {
      "maxImages": 3,
      "maxAudioClips": 1,
      "preprocessForNonVision": true,
      "preferredVisionModel": "candle"
    }
  }
}
```

A code review recipe might activate screenshots but not audio.
A voice call recipe activates audio transcripts but not images.
A gaming recipe activates screenshots + object detection.

---

## Latency Strategy

### Hot Path (must be <100ms total RAG build)
- Artifact extraction from cache: ~0ms
- Artifact metadata assembly: ~1-2ms
- Selecting which artifacts to include (budget): ~0ms

### Warm Path (can be 100-500ms, overlapped with other work)
- Vision description lookup (cached): ~0ms
- Vision description generation (first time): ~200-2000ms — but ONLY for non-vision models

### Cold Path (async, fire-and-forget)
- Proactive vision description on upload (before any AI asks)
- Audio transcription of shared clips
- Video key frame extraction

### Memory Strategy
- Base64 images are LARGE (500KB-5MB each)
- Don't hold more than `maxImages` in RAG context
- Description cache holds strings (~500 bytes each), not images
- LRU eviction on description cache (1000 entries max)
- Recipe budget controls total media token allocation

---

## Implementation Order

```
P1 (race condition fix)     ← TODAY, ~20 min
P2 (MediaArtifactSource)    ← Next, ~2 hours
P3 (vision description cache) ← After P2, ~1 hour
P4 (CBarFrame)              ← Future sprint
P5 (recipe media config)    ← After P4
```

P1 is a reorder of 3 lines. P2 extracts existing code into proper RAGSource.
P3 adds caching. P4-P6 are architecture that builds on P1-P3.

---

## Priority 6: LivePerceptionSource — Video/Avatar Awareness

RAGSource that gives AIs visual awareness of live video calls and avatar scenes.
Same pattern as VoiceConversationSource (which gives AIs audio awareness via timeline).

### Architecture

```
Live Video Feed (Bevy/LiveKit)
        │
        ▼
  SceneChangeDetector (Rust, cheap)
  - Frame diff threshold (% pixels changed)
  - YOLO object detection (optional, Rust ONNX)
  - Semantic segmentation (optional, Rust ONNX)
  - Emits: scene_metadata events with structured data
        │
        ▼
  MediaFrame (cached, content-addressed)
  - Lazily populated representations
  - image/base64, image/description, image/objects
  - One VisionDescriptionService call per unique frame
        │
        ▼
  LivePerceptionSource (RAGSource)
  - Subscribes to scene_metadata events
  - Provides system prompt section: "Current scene: ..."
  - Budget-aware: drops old frames, keeps N most recent
  - Recipe-activated: only fires when recipe includes 'live-perception'
```

### SceneChangeDetector (Rust module)

Runs in the Bevy render pipeline or as a post-processing step.
Cheap enough to run every frame or every Nth frame.

```rust
pub struct SceneChangeDetector {
    last_frame_hash: u64,       // Perceptual hash of last processed frame
    change_threshold: f32,       // 0.0-1.0, how much change triggers event
    detection_interval: u32,     // Process every Nth frame (e.g., every 15 = 1/sec at 15fps)
    yolo_model: Option<OrtSession>,  // Optional YOLO for object detection
}

// Emits via MessageBus:
pub struct SceneMetadata {
    pub call_id: String,
    pub timestamp_ms: u64,
    pub change_type: SceneChangeType,  // PersonMoved, ObjectAppeared, SceneChanged, etc.
    pub objects: Vec<DetectedObject>,   // YOLO results if model loaded
    pub frame_hash: u64,               // For dedup
}
```

### LivePerceptionSource (TypeScript RAGSource)

```typescript
export class LivePerceptionSource implements RAGSource {
  readonly name = 'live-perception';
  readonly priority = 70;
  readonly defaultBudgetPercent = 8;

  isApplicable(context: RAGSourceContext): boolean {
    // Only active during live calls with visual component
    return !!context.options.voiceSessionId;
  }

  async load(context: RAGSourceContext, budget: number): Promise<RAGSection> {
    // Get latest scene metadata from event buffer
    // If scene changed recently, get/cache description via VisionDescriptionService
    // Return as system prompt section: "Visual context: ..."
    // Include structured YOLO data if available
  }
}
```

### Recipe Integration

```json
{
  "name": "live-call-with-vision",
  "ragTemplate": {
    "sources": [
      "conversation-history",
      "persona-identity",
      "voice-conversation",
      "live-perception",
      "media-artifacts"
    ],
    "mediaConfig": {
      "maxFrameSnapshots": 3,
      "sceneChangeThreshold": 0.3,
      "enableYolo": true,
      "preferredVisionModel": "candle"
    }
  },
  "tools": ["voice/synthesize", "interface/screenshot", "canvas/vision"]
}
```

### Command Interface

All perception capabilities are command-callable:

| Command | Purpose |
|---------|---------|
| `interface/screenshot` | Capture current browser/UI state |
| `avatar/snapshot` | Capture avatar scene for a slot |
| `canvas/vision` | Describe canvas content |
| `media/process` | Process media through perception tiers |

Future additions:
| Command | Purpose |
|---------|---------|
| `live/snapshot` | Capture live call room view |
| `live/scene-metadata` | Get latest YOLO/segmentation results |
| `live/perception-config` | Adjust detection thresholds at runtime |

---

## The Perception/Action Symmetry

Every sense has a corresponding action. Every input adapter has an output adapter.
The recipe controls both sides:

| Sense (RAG Source) | Action (Tool) | Domain |
|-------------------|---------------|--------|
| VoiceConversationSource | voice/synthesize (TTS) | Audio |
| LivePerceptionSource | avatar/snapshot, live/snapshot | Video |
| ConversationHistorySource | collaboration/chat/send | Chat |
| WidgetContextSource | interface/click, type, navigate | UI |
| CodebaseSearchSource | code/edit, code/write | Code |
| MediaArtifactSource | media/process | Media |

AIs are citizens with senses and capabilities. Recipes define their role.
The same AI with a "web researcher" recipe browses websites.
With a "game player" recipe it plays games.
With a "voice call" recipe it talks.
With a "live observer" recipe it watches and describes.

The infrastructure is the same. Only the recipe changes.
