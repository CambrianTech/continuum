# Media Pipeline Fix Plan

## The Problem

Media artifacts (images, screenshots, drag-and-drop files) are broken in RAG context.
AIs used to see images shared in chat — now they don't.

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

## Priority 3: Lazy Vision Description Cache

**Problem**: `VisionDescriptionService.describeBase64()` is called per-image per-non-vision-model.
With 3 text-only personas and 5 images, that's 15 vision API calls for the same 5 descriptions.

**Fix**: Content-addressed description cache.

```typescript
class VisionDescriptionCache {
  // Key = hash of base64 content (first 1KB + length for speed)
  // Value = { description, timestamp, model }
  private cache: Map<string, CachedDescription> = new Map();

  async getOrDescribe(base64: string, mimeType: string): Promise<string | null> {
    const key = this.contentKey(base64);
    const cached = this.cache.get(key);
    if (cached && Date.now() - cached.timestamp < TTL) return cached.description;

    const desc = await visionService.describeBase64(base64, mimeType);
    if (desc) this.cache.set(key, { description: desc.description, timestamp: Date.now() });
    return desc?.description ?? null;
  }
}
```

**Performance**: Each image described exactly once regardless of how many personas need it.

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
P3 adds caching. P4-P5 are architecture that builds on P1-P3.
