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
| P2 | MediaArtifactSource as proper RAGSource | DONE (DB query, 50-msg scan, 10s vision timeout) |
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

---

## P7: Optimization, Modularity & Rust Migration

Analysis of the current media/vision pipeline after P1-P3 completion.
Identifies what works, what's fragile, and what should move to Rust.

### Current Architecture (Post P1-P3)

```
chat/send (upload)
    │
    ├─ Store message with media[] in SQLite
    └─ Fire-and-forget: media/prewarm → VisionDescriptionService.describeBase64()
                                              │
                                              ▼
                                    LLaVA via AIProviderDaemon (60-70s CPU)
                                    Content-addressed cache (in-memory, 5min TTL)
                                    In-flight dedup (concurrent callers coalesce)

persona RAG build (5-10s later, per message)
    │
    ├─ RAGComposer.compose() → 17 sources in parallel
    │   ├─ ConversationHistorySource → DB query + event-driven cache (30s TTL)
    │   │   └─ Single-flight coalescing (16 personas = 1 DB query)
    │   └─ MediaArtifactSource → reads ConversationHistorySource cache
    │       ├─ Extract media[] from messages
    │       ├─ Budget: maxArtifacts = allocatedBudget / tokensPerArtifact
    │       └─ Preprocess for model capabilities:
    │           ├─ Vision models: raw base64 → ContentPart[]
    │           └─ Text-only: VisionDescriptionService → cached description
    │               ├─ Adaptive timeout: 90s (in-flight) / 10s (new)
    │               └─ Parallel: all images fire at once, single timeout
    │
    └─ PersonaResponseGenerator → vision-aware rendering
        ├─ AICapabilityRegistry.hasCapability(provider, model, 'image-input')
        ├─ Vision: base64 ContentPart[]
        └─ Text-only: [Image "file": description] text annotation
```

### Known Weaknesses

| Issue | Impact | Root Cause |
|-------|--------|------------|
| **In-memory cache dies on restart** | All LLaVA descriptions lost, 60-70s re-inference | VisionDescriptionService uses JS Map, no persistence |
| **No `has_media` index** | Full message scan to find images | SQLite schema has no media boolean column |
| **Base64 over IPC** | ~1MB per image × 10+ personas = IPC saturation | Messages carry inline base64, no external storage |
| **50-message scan window** | Images pushed past window in chatty rooms | Linear scan, no indexed media lookup |
| **LLaVA on CPU = 60-70s** | First image description blocks all text-only models | No GPU acceleration, no model quantization |
| **Cache TTL = 5 min** | Descriptions evicted while conversation still active | Fixed TTL, not conversation-scoped |
| **ConversationHistorySource cache timing** | MediaArtifactSource may check before cache populated | Race between source load order in compose phase |

---

### Optimization Strategy

#### O1: Persistent Vision Description Cache (Rust SQLite)

**Problem**: VisionDescriptionService's in-memory Map dies on restart. 60-70s of LLaVA inference wasted.

**Solution**: New `vision_descriptions` table in the default SQLite database, managed by a Rust `VisionModule`.

```sql
CREATE TABLE vision_descriptions (
    content_key TEXT PRIMARY KEY,     -- SHA-256(first 4KB + length)
    description TEXT NOT NULL,
    model_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    confidence REAL DEFAULT 0.85,
    processing_time_ms INTEGER,
    created_at INTEGER NOT NULL,      -- Unix timestamp ms
    last_accessed_at INTEGER NOT NULL  -- For LRU eviction
);

CREATE INDEX idx_vision_last_accessed ON vision_descriptions(last_accessed_at);
```

**Rust module**: `modules/vision.rs`

Commands:
- `vision/description-get` — lookup by content_key (< 1ms SQLite read)
- `vision/description-put` — store description after inference
- `vision/description-stats` — cache hit rate, total entries
- `vision/description-evict` — LRU eviction (keep last 2000)

**TS change**: VisionDescriptionService checks Rust cache BEFORE triggering inference.
In-memory Map becomes L1 (process lifetime), SQLite becomes L2 (persistent).

```
L1: JS Map (500 entries, 5min TTL, instant)     ← current
L2: SQLite via Rust IPC (2000 entries, no TTL)   ← NEW
L3: LLaVA inference (60-70s)                     ← current
```

**Impact**: Restart = warm cache. Descriptions survive across deploys.

#### O2: Media Index Column

**Problem**: Finding messages with media requires scanning all messages and checking `content.media.length > 0`.

**Solution**: Add `has_media BOOLEAN DEFAULT 0` column to chat_messages schema.

```sql
ALTER TABLE chat_messages ADD COLUMN has_media BOOLEAN DEFAULT 0;
CREATE INDEX idx_chat_messages_has_media ON chat_messages(room_id, has_media, timestamp);
```

Set on insert in the data module when `content.media` array is non-empty.
Query becomes: `WHERE room_id = ? AND has_media = 1 ORDER BY timestamp DESC LIMIT 10`.

**Impact**: O(1) indexed lookup instead of O(N) scan. MediaArtifactSource drops from 50-message scan to direct indexed query for media messages only.

#### O3: Externalize Base64 Storage

**Problem**: Base64 images stored inline in chat_messages JSON. Every message query loads every image. 10 personas × 1MB image = 10MB IPC traffic.

**Solution**: Store base64 in separate blobs table, reference by content-addressed key.

```sql
CREATE TABLE media_blobs (
    content_key TEXT PRIMARY KEY,    -- Same SHA-256 as vision cache
    base64_data TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
```

Chat messages store `{ "blobKey": "abc123", "mimeType": "image/png" }` instead of inline base64.
MediaArtifactSource fetches blobs only when needed (vision models) or skips entirely (text-only models that have cached descriptions).

**Impact**: Message queries go from ~1MB to ~1KB per media message. IPC congestion eliminated.

#### O4: Extend Cache TTL to Conversation-Scoped

**Problem**: 5-minute TTL evicts descriptions while conversation is still active.

**Solution**: Replace fixed TTL with `last_accessed_at` tracking. Descriptions stay cached as long as they're being accessed. LRU eviction only when cache exceeds max size.

In-memory L1 cache: bump `cachedAt` on every access (effectively infinite TTL while active).
SQLite L2 cache: `UPDATE vision_descriptions SET last_accessed_at = ? WHERE content_key = ?` on read.

**Impact**: No re-inference during active conversations. Cold descriptions evicted naturally.

---

### Modularity Refinement

#### M1: Split VisionDescriptionService into Cache + Inference

Current VisionDescriptionService does too much: cache management, in-flight dedup, model selection, prompt building, response parsing.

**Refactor into**:

```
VisionDescriptionService (facade, unchanged API)
    ├─ VisionDescriptionCache (L1 in-memory + L2 Rust SQLite)
    │   ├─ get(contentKey): cached | inflight | miss
    │   ├─ put(contentKey, description)
    │   └─ status(contentKey): 'cached' | 'inflight' | 'none'
    │
    └─ VisionInferenceProvider (model selection + inference)
        ├─ selectModel(options): { provider, modelId }
        ├─ describe(base64, mimeType, options): VisionDescription
        └─ buildPrompt(options): string
```

**Why**: Cache layer is reusable (screenshots, canvas, video frames all need it).
Inference provider is swappable (LLaVA today, Candle native tomorrow, cloud fallback).

#### M2: MediaArtifactSource Batching Support

MediaArtifactSource currently runs as a TypeScript-only source (no batching).
The media extraction (scan messages for `content.media`) could move to Rust.

Add `supportsBatching = true` and implement `getBatchRequest()`:

```typescript
// New RagSourceRequest variant
#[serde(rename = "media")]
Media {
    budget_tokens: usize,
    params: MediaSourceParams,
}

pub struct MediaSourceParams {
    pub room_id: String,
    pub max_messages: usize,
    pub include_base64: bool,  // false for text-only models
}
```

Rust does the DB query (with `has_media` index), returns media metadata + optional base64.
TypeScript handles vision preprocessing (still needs VisionDescriptionService).

**Impact**: Media extraction joins the batched Rust IPC call. One less TypeScript-side DB query.

#### M3: Decouple Vision Preprocessing from RAG Load

Currently, vision preprocessing (describing images for text-only models) happens inside `MediaArtifactSource.load()`. This blocks the RAG compose phase.

**Better**: MediaArtifactSource returns raw artifacts. Vision preprocessing happens in PersonaResponseGenerator (render time), which already knows model capabilities.

```
Current:  MediaArtifactSource.load() → preprocess → return described artifacts
Better:   MediaArtifactSource.load() → return raw artifacts
          PersonaResponseGenerator   → preprocess on demand (lazy, cached)
```

**Why**: RAG compose completes faster. Preprocessing only happens if artifacts actually make it into the prompt (budget may exclude them). Vision descriptions still cached — just triggered later.

**Risk**: Adds latency to render phase. Mitigated by pre-warm (descriptions already cached from upload).

---

### Rust Migration Plan

#### R1: VisionModule (New Rust Module)

**File**: `workers/continuum-core/src/modules/vision.rs`
**Command prefix**: `vision/`

```rust
pub struct VisionModule {
    description_cache: Arc<RwLock<HashMap<String, CachedVisionDescription>>>,
    db_adapter: Arc<dyn StorageAdapter>,
}

// Commands:
// vision/description-get    → L1 HashMap, fallback L2 SQLite
// vision/description-put    → Write to both L1 + L2
// vision/description-stats  → Hit rates, entry count
// vision/description-evict  → LRU cleanup
// vision/describe           → Future: native Candle LLaVA inference
```

**IPC mixin**: `bindings/modules/vision.ts`

```typescript
export function VisionMixin<T extends ...>(Base: T) {
  return class extends Base {
    async visionDescriptionGet(contentKey: string): Promise<VisionDescription | null> { ... }
    async visionDescriptionPut(contentKey: string, desc: VisionDescription): Promise<void> { ... }
    async visionDescriptionStats(): Promise<VisionCacheStats> { ... }
  };
}
```

#### R2: Media Source in RagModule

Add `Media` variant to `RagSourceRequest` enum in `rag.rs`:

```rust
#[serde(rename = "media")]
Media {
    budget_tokens: usize,
    params: MediaSourceParams,
},
```

Rust handles the indexed query (`has_media = 1`), returns metadata.
Base64 data only fetched when `include_base64 = true` (vision models).

#### R3: Native LLaVA in CandleAdapter (Future)

Currently `supports_vision: false` in CandleAdapter. Candle supports LLaVA models.

**Steps**:
1. Add LLaVA model loading to `inference/model.rs` (GGUF format, same as text models)
2. Implement image preprocessing in Rust (resize, normalize, base64 decode)
3. Set `supports_vision: true` in CandleAdapter capabilities
4. Route `vision/describe` through CandleAdapter instead of TypeScript AIProviderDaemon

**Impact**: LLaVA runs on Rust tokio thread pool. No IPC for inference. GPU-acceleratable when 5090 available.

#### R4: Content-Addressed Blob Storage

Add `media_blobs` table management to DataModule:

```rust
// New commands in data module:
// data/blob-store   → Store base64 by content key
// data/blob-get     → Retrieve base64 by content key
// data/blob-exists  → Check existence without loading
// data/blob-delete  → Remove blob (LRU eviction)
```

Chat messages reference blobs by key. MessageEntity schema change:
`content.media[].base64` → `content.media[].blobKey` (base64 field deprecated but still accepted for backward compat).

---

### Implementation Priority

```
Phase A (Quick Wins — days)
├── O4: Extend cache TTL to access-based          (TS only, ~30 min)
├── M1: Split VisionDescriptionService             (TS refactor, ~2 hours)
└── O2: has_media column + index                   (Rust migration, ~1 day)
    └── data.rs schema change + DataModule insert hook

Phase B (Persistent Cache — days)
├── R1: VisionModule in Rust                       (~1 day)
│   ├── vision_descriptions SQLite table
│   ├── vision/description-get, -put, -stats, -evict
│   └── VisionMixin for IPC
└── O1: Wire VisionDescriptionService → Rust L2    (~half day)
    └── L1 Map → L2 SQLite fallback chain

Phase C (IPC Optimization — week)
├── R4: Blob storage (media_blobs table)           (~2 days)
│   ├── data/blob-store, -get, -exists
│   └── ChatSendServerCommand stores blobs on upload
├── O3: Messages reference blobKey, not inline b64 (~1 day)
└── R2: Media source batching in rag.rs            (~1 day)

Phase D (Native Vision — future)
├── R3: CandleAdapter LLaVA support                (~3-5 days)
│   ├── Image preprocessing in Rust
│   ├── LLaVA GGUF model loading
│   └── supports_vision: true
├── M3: Lazy preprocessing in render phase         (~1 day)
└── P4-P6: CBarFrame, recipe config, live perception (per original plan)
```

### Verification Criteria

| Phase | Test |
|-------|------|
| A | `npm run build:ts` clean. VisionDescriptionService tests pass. `has_media` index created on deploy. |
| B | Restart server → vision descriptions survive. `vision/description-stats` shows persistent entries. |
| C | Chat with image: message query returns ~1KB not ~1MB. IPC timing logs show <50ms for media source. |
| D | `./jtag ai/report` shows Candle with `supports_vision: true`. LLaVA inference in Rust process logs. |
