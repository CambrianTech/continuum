# Thought Frame Architecture: CBAR-Inspired Parallel Processing for PersonaUser

## Why RTOS is the Only Way

Traditional architectures (event-driven, request/response, unlimited promises) fail with real-time AI systems because:

1. **Event-driven**: Blocks on slow operations, entire system freezes
2. **Unlimited parallelism**: Spawns infinite workers, runs out of memory
3. **Lock-based coordination**: Deadlocks, race conditions, complexity explosion

**RTOS principles are the ONLY proven approach** for systems with:
- Long-running expensive operations (AI inference: 2-10s)
- Finite shared resources (GPU memory for LoRA adapters)
- Real-time responsiveness requirements (60fps decision loop)
- Priority-based task management (high priority never starved)
- Graceful degradation under load (drop low-priority, don't crash)

These problems were solved 40+ years ago in embedded systems. We're just applying them to AI cognition instead of motor controllers.

---

## The Problem: Current Architecture is "Slow and Slogging"

**Current PersonaUser Pattern (Blocking)**:
```typescript
async serviceInbox(): Promise<void> {
  const item = await this.inbox.pop();           // Wait for work
  const response = await this.processItem(item); // BLOCKS for 2-10 seconds!
  await this.postResponse(response);
}
```

**What's wrong**:
- The AI generation blocks everything (like rendering AND semantic segmentation in the same thread)
- No parallelism - can't work on multiple items at once
- No pipelining - can't start cheap operations while waiting for expensive ones
- No graceful degradation - either full response or nothing

**This is like running CBAR at 2fps instead of 60fps.**

---

## The CBAR Pattern: Frame-Based Parallel Processing

### Core Concepts from CBAR Mobile-Home-SDK

**CRITICAL PERFORMANCE CONTEXT**: This architecture ran at **42fps on an iPhone 7** (bgfx) and similar in Unity, while simultaneously:
- Running CNNs for GAN and semantic segmentation
- 3D plane reconstruction with RANSAC
- Line finding and color analysis
- Per-plane texture stitching
- Siamese neural networks
- Watershed algorithms
- GPGPU operations

**If this can run on a 2016 iPhone, PersonaUser can EASILY hit 60fps decision loops on modern servers.**

**CBAR's Architecture** (Augmented Reality at 42-60fps):
1. **Multiple parallel processes** with priority management
2. **Frame ID tagging** for async result stitching across time
3. **Lazy evaluation** - don't compute unless needed
4. **Metadata accumulation** - keep probabilistic results, not just finals
5. **Optical flow** for temporal interpolation
6. **Fast rendering loop** + slow computation in background
7. **Texture ID passing** - avoid expensive operations (rasterization)

### The Key Insight: "Don't Rasterize Unless You Have To"

**CBAR Philosophy**:
- Pass texture IDs (cheap references)
- Optical flow only needs BW, low-res (fast)
- RGB framebuffer pull only when absolutely needed (expensive)
- Semantic segmentation takes 3 seconds, but frame renders at 60fps
- Use **frame tagging** to stitch async results back in time

**PersonaUser Equivalent**:
- Pass message IDs (cheap references)
- Priority scoring only needs metadata (fast)
- Full AI generation only when engagement confirmed (expensive)
- RAG lookup takes 2 seconds, but UI responds instantly
- Use **thought frame tagging** to stitch async results back in time

---

## The Solution: Thought Frame Pipeline

### ThoughtFrame: The Universal Processing Unit

```typescript
/**
 * ThoughtFrame - Inspired by CBAR's CBARFrame
 *
 * A frame represents ONE cognitive processing cycle.
 * Multiple frames can be "in flight" simultaneously.
 * Frames are tagged with IDs for async stitching.
 */
interface ThoughtFrame {
  // IDENTITY (like CBAR frame.id for stitching)
  frameId: UUID;                // Unique frame identifier
  timestamp: number;            // When frame was created
  sequenceNumber: number;       // Ordering for temporal coherence

  // CHEAP REFERENCES (always available, like CBAR's textureId)
  inboxItemRef: UUID;           // Pointer to message/task entity
  priority: number;             // Pre-computed priority score
  domain: string;               // 'chat' | 'code' | 'game' etc.
  estimatedCost: number;        // Predicted AI tokens/time

  // LAZY EVALUATION (compute only when needed)
  getRawContext(): Promise<string>;           // Full message content
  getSemanticEmbedding(): Promise<number[]>;  // Async, cached
  getRelevantMemories(): Promise<Memory[]>;   // RAG lookup
  getSkillOutput(): Promise<string>;          // LoRA adapter result

  // ACCUMULATED METADATA (like CBAR's line boundaries)
  accumulatedConfidence?: number;    // Multiple checks agree
  relatedFrames?: UUID[];            // Frame sequence for context
  opticalFlowVector?: number[];      // Sentiment/topic drift
  partialResults?: Partial<AIResponse>; // Streaming updates

  // STATE TRACKING
  stage: 'queued' | 'filtering' | 'processing' | 'rendering' | 'completed';
  processingStartTime?: number;
  completionTime?: number;
}
```

### The Three-Loop Architecture

Inspired by CBAR's parallel processing model:

```typescript
/**
 * LOOP 1: FAST DECISION LOOP (60fps equivalent)
 *
 * Like CBAR's rendering loop - always responsive, minimal work
 * Decides what to process, not how to process it
 */
private async fastDecisionLoop(): Promise<void> {
  while (this.active) {
    // Pop frame metadata (cheap, like checking texture ID)
    const item = await this.inbox.peek();  // Non-blocking

    if (!item) {
      await this.sleep(16);  // ~60fps when idle
      continue;
    }

    // Create frame (cheap reference, no processing yet)
    const frame = this.createFrame(item);

    // FAST HEURISTICS (like optical flow - BW, low-res, fast)
    const quickScore = this.quickPriorityCheck(frame);
    if (quickScore < 0.3) {
      await this.inbox.pop();  // Discard, don't even queue for processing
      continue;
    }

    // Queue for parallel processing
    this.processingQueue.enqueue(frame);
    await this.inbox.pop();  // Remove from inbox

    await this.sleep(16);  // ~60fps
  }
}

/**
 * LOOP 2: PARALLEL PROCESSING POOL (background workers)
 *
 * Like CBAR's CNN/segmentation threads - heavy computation
 * Multiple workers running in parallel on different frames
 */
private async processingWorkerLoop(workerId: number): Promise<void> {
  while (this.active) {
    // Grab next frame to process
    const frame = await this.processingQueue.dequeue();
    if (!frame) {
      await this.sleep(100);  // Wait for work
      continue;
    }

    frame.stage = 'processing';
    frame.processingStartTime = Date.now();

    try {
      // EXPENSIVE OPERATIONS (async, non-blocking to other frames)

      // 1. Activate skill (page in LoRA adapter if needed)
      await this.genome.activateSkill(frame.domain);

      // 2. RAG lookup (can take 1-2 seconds)
      const memories = await frame.getRelevantMemories();

      // 3. AI generation (2-10 seconds)
      const response = await this.generateResponse(frame, memories);

      // 4. Tag result with frame ID for stitching
      frame.partialResults = response;
      frame.stage = 'rendering';
      frame.completionTime = Date.now();

      // Move to render queue
      this.renderQueue.enqueue(frame);

    } catch (error) {
      console.error(`Frame ${frame.frameId} failed: ${error}`);
      frame.stage = 'completed';  // Drop failed frames
    }
  }
}

/**
 * LOOP 3: RENDER LOOP (UI updates)
 *
 * Like CBAR's 60fps rendering - pull completed results and display
 * Fast, always responsive, stitches async results back together
 */
private async renderLoop(): Promise<void> {
  while (this.active) {
    // Check for completed frames
    const completedFrames = this.renderQueue.dequeueAll();

    if (completedFrames.length === 0) {
      await this.sleep(16);  // ~60fps when idle
      continue;
    }

    // TEMPORAL STITCHING (like CBAR's optical flow interpolation)
    // Sort frames by sequence number for temporal coherence
    completedFrames.sort((a, b) => a.sequenceNumber - b.sequenceNumber);

    for (const frame of completedFrames) {
      // Post response (fast - just database write + websocket emit)
      await this.postResponse(frame.partialResults!);

      // Update state tracking
      await this.state.recordActivity(
        frame.completionTime! - frame.processingStartTime!,
        frame.estimatedCost
      );

      frame.stage = 'completed';
    }

    await this.sleep(16);  // ~60fps
  }
}
```

### Initialization: Spawn Multiple Workers

```typescript
constructor(entity: UserEntity, stateEntity: UserStateEntity) {
  super(entity, stateEntity);

  this.processingQueue = new AsyncQueue<ThoughtFrame>();
  this.renderQueue = new AsyncQueue<ThoughtFrame>();

  // Spawn multiple processing workers (like CBAR's parallel threads)
  this.workerCount = 3;  // Can process 3 frames simultaneously

  // Start all three loops
  this.fastDecisionLoop().catch(this.handleLoopError);

  for (let i = 0; i < this.workerCount; i++) {
    this.processingWorkerLoop(i).catch(this.handleLoopError);
  }

  this.renderLoop().catch(this.handleLoopError);
}
```

---

## Key Patterns from CBAR Applied to PersonaUser

### 1. Frame ID Tagging for Async Stitching

**CBAR Example**:
```cpp
// CNN finishes 3 seconds after frame was captured
CBARFrame currentFrame = getLatestFrame();  // Frame 180 (at 60fps)
CBARFrame semanticFrame = getFrameById(90);  // Semantic result from frame 90 (3s ago)

// Stitch semantic result back into current rendering
stitchSemanticResult(currentFrame, semanticFrame);  // Uses frame ID + timestamp
```

**PersonaUser Equivalent**:
```typescript
// AI response finishes 5 seconds after message arrived
const currentFrame = this.getCurrentFrame();  // Frame 300 (at 60fps)
const aiFrame = this.getFrameById(frameId);   // AI response from frame 0 (5s ago)

// Stitch AI response back into conversation
await this.postResponse(aiFrame.partialResults);  // Uses frame ID + sequence
```

### 2. Optical Flow for Interpolation

**CBAR Example**:
- Optical flow tracks pixel movement between frames
- Only needs BW, low-res (fast to compute)
- Interpolates semantic results across frames
- Smooth transitions despite 3-second CNN delay

**PersonaUser Equivalent**:
```typescript
interface OpticalFlowVector {
  sentimentDrift: number;     // Conversation mood changing?
  topicVelocity: number;      // How fast topic is shifting?
  urgencyAcceleration: number; // Priority increasing/decreasing?
}

// Compute optical flow between frames (cheap)
function computeThoughtFlow(
  prevFrame: ThoughtFrame,
  currFrame: ThoughtFrame
): OpticalFlowVector {
  return {
    sentimentDrift: currFrame.priority - prevFrame.priority,
    topicVelocity: embeddingDistance(prev.embedding, curr.embedding),
    urgencyAcceleration: computeSecondDerivative(priority)
  };
}

// Use optical flow to interpolate responses
if (flow.urgencyAcceleration > 0.5) {
  // Priority spiking - interrupt current processing
  this.processingQueue.prioritize(frame);
}
```

### 3. Keep Probabilities (RAW Files)

**CBAR Example**:
```cpp
// Don't just store "this is a wall"
// Store probabilities for ALL classes
struct SemanticResult {
  float wall_probability = 0.87;
  float floor_probability = 0.05;
  float furniture_probability = 0.08;
  // ... keep ALL data
};

// Later processes can use this rich data
// Watershed algorithm uses probabilities to fill gaps
// Siamese network uses accumulated line boundaries
```

**PersonaUser Equivalent**:
```typescript
interface AIResponse {
  // Don't just store final text
  finalText: string;  // "I think X is the answer"

  // Keep probabilities and metadata
  confidence: number;              // 0.87 - how sure?
  alternativeInterpretations: Array<{
    text: string;
    probability: number;
  }>;

  reasoning: string[];             // Chain of thought
  citedMemories: UUID[];           // Which RAG results used
  uncertainty: string[];           // What AI wasn't sure about

  // Raw model output (like photographer's RAW file)
  rawLogits?: number[];            // Token probabilities
  attentionWeights?: number[][];   // What model focused on
}

// Multiple personas can "watershed fill" consensus
// Teacher AI reviews Helper AI's response
// Uses confidence scores + reasoning to validate
```

### 4. Accumulated Line Boundaries

**CBAR Example**:
```cpp
// Accumulate edge detection across frames
// Helps neural networks and watershed algorithms
struct AccumulatedBoundaries {
  EdgeMap edges;  // Sobel, Canny over time
  int frameCount; // How many frames contributed
  float confidence; // Stronger with more frames
};

// Siamese network uses these for faster semantic segmentation
// Watershed algorithm uses these to fill regions
```

**PersonaUser Equivalent**:
```typescript
interface AccumulatedConsensus {
  // Multiple frames/personas agree on something
  topic: string;
  agreementCount: number;        // How many frames support this
  confidence: number;            // Stronger with more agreement
  contributingFrames: UUID[];    // Which frames contributed

  // Like CBAR's edge map
  keyPhrases: Map<string, number>;  // Word frequencies across frames
  sentimentTrend: number[];         // Mood over time
}

// Helps fast-path decisions
// If 5 frames all agree this is urgent, skip re-evaluation
```

### 5. SIMD/GPGPU Optimization

**CBAR Example**:
- Use GPU shaders for pixel operations (sobel, canny, gabor)
- SIMD for vector operations
- Minimize CPU ↔ GPU transfers
- Pass texture IDs, not rasterized pixels

**PersonaUser Equivalent**:
```typescript
// Use embedding similarity (GPU-accelerated)
// Instead of full text comparison (CPU-bound)

// ❌ SLOW: Full text analysis
function isRelated(msg1: string, msg2: string): boolean {
  return nlpLibrary.computeSimilarity(msg1, msg2) > 0.8;  // 100ms
}

// ✅ FAST: Pre-computed embeddings (GPU vector ops)
function isRelated(
  embed1: number[],
  embed2: number[]
): boolean {
  return cosineSimilarity(embed1, embed2) > 0.8;  // 0.1ms
}

// Pre-compute embeddings for all messages in parallel
// Cache embeddings in frame metadata
// Fast comparisons using SIMD-like vector operations
```

---

## Resource Management: The Critical Challenge

### The Problem: Long-Running AI + LoRA Paging + Thread Safety

**Core Constraints**:
1. **AI inference can take 2-10 seconds** - that's fine, but can't block everything
2. **LoRA adapters consume 50-200MB each** - memory budget is finite
3. **Multiple workers share adapters** - thread safety required
4. **Paging adapters in/out is expensive** - must minimize thrashing

**Bad Example (Thrashing)**:
```typescript
// Worker 1: Load typescript adapter (200MB)
await genome.activateSkill('typescript');  // 500ms to load

// Worker 2: Load rust adapter, evicts typescript (memory full)
await genome.activateSkill('rust');        // 500ms to load, 200ms to evict

// Worker 1: Needs typescript again! Evicts rust
await genome.activateSkill('typescript');  // 500ms to load, 200ms to evict

// THRASHING: Spending 1.4s on paging instead of AI inference
```

### Solution: Priority-Based Worker Scheduling + Shared Memory Budget

#### 1. Worker Affinity (Reduce Paging)

```typescript
/**
 * Worker Pool with Domain Affinity
 *
 * Each worker "prefers" certain domains to reduce adapter thrashing.
 * Like CPU cache affinity in RTOS scheduling.
 */
interface ProcessingWorker {
  workerId: number;
  affinityDomains: string[];  // ['chat', 'general']
  currentAdapter?: string;     // What's loaded in this worker's context
  busyUntil: number;          // When will worker be free
}

class WorkerPool {
  workers: ProcessingWorker[];

  /**
   * Assign frame to best worker:
   * 1. Worker already has correct adapter loaded (instant)
   * 2. Worker with affinity for this domain (fast)
   * 3. Any idle worker (needs paging)
   */
  assignFrame(frame: ThoughtFrame): ProcessingWorker {
    // Priority 1: Worker already has adapter loaded
    const perfectMatch = this.workers.find(w =>
      w.currentAdapter === frame.domain && w.busyUntil < Date.now()
    );
    if (perfectMatch) return perfectMatch;

    // Priority 2: Worker with affinity (likely has adapter)
    const affinityMatch = this.workers.find(w =>
      w.affinityDomains.includes(frame.domain) && w.busyUntil < Date.now()
    );
    if (affinityMatch) return affinityMatch;

    // Priority 3: Any idle worker
    const idle = this.workers.find(w => w.busyUntil < Date.now());
    return idle ?? this.workers[0];  // Force oldest if all busy
  }
}
```

#### 2. Shared Memory Budget with Reference Counting

```typescript
/**
 * PersonaGenome: Thread-Safe Adapter Management
 *
 * Multiple workers can share adapters simultaneously.
 * Adapters only evicted when NO workers need them.
 */
class PersonaGenome {
  private activeAdapters: Map<string, {
    adapter: LoRAAdapter;
    refCount: number;        // How many workers using?
    lastUsed: number;        // LRU tracking
    pinnedUntil?: number;    // Don't evict until timestamp
  }>;

  private memoryBudget: number = 1024;  // 1GB total
  private memoryUsage: number = 0;

  /**
   * Activate adapter (thread-safe)
   * Increments reference count if already loaded
   */
  async activateSkill(domain: string, workerId: number): Promise<void> {
    const existing = this.activeAdapters.get(domain);

    if (existing) {
      // Already loaded - just increment ref count
      existing.refCount++;
      existing.lastUsed = Date.now();
      console.log(`Worker ${workerId} sharing ${domain} adapter (refs=${existing.refCount})`);
      return;
    }

    // Need to load - check memory budget first
    const adapterSize = 200;  // MB
    if (this.memoryUsage + adapterSize > this.memoryBudget) {
      await this.evictLRU();  // Make room
    }

    // Load adapter
    const adapter = await LoRAAdapter.load(domain);
    this.activeAdapters.set(domain, {
      adapter,
      refCount: 1,
      lastUsed: Date.now()
    });
    this.memoryUsage += adapterSize;

    console.log(`Worker ${workerId} loaded ${domain} (${this.memoryUsage}MB used)`);
  }

  /**
   * Release adapter (thread-safe)
   * Decrements ref count, marks for eviction when 0
   */
  async releaseSkill(domain: string, workerId: number): Promise<void> {
    const existing = this.activeAdapters.get(domain);
    if (!existing) return;

    existing.refCount--;
    console.log(`Worker ${workerId} released ${domain} (refs=${existing.refCount})`);

    // Don't immediately evict - wait until LRU eviction needed
    // This allows rapid re-use without thrashing
  }

  /**
   * Evict least-recently-used adapter (only if ref count = 0)
   */
  async evictLRU(): Promise<void> {
    let lruKey: string | null = null;
    let lruTime = Infinity;

    for (const [key, data] of this.activeAdapters.entries()) {
      // Skip adapters still in use
      if (data.refCount > 0) continue;

      // Skip pinned adapters
      if (data.pinnedUntil && data.pinnedUntil > Date.now()) continue;

      if (data.lastUsed < lruTime) {
        lruTime = data.lastUsed;
        lruKey = key;
      }
    }

    if (lruKey) {
      const data = this.activeAdapters.get(lruKey)!;
      console.log(`Evicting ${lruKey} (unused for ${Date.now() - data.lastUsed}ms)`);

      await data.adapter.unload();
      this.activeAdapters.delete(lruKey);
      this.memoryUsage -= data.adapter.size;
    } else {
      console.warn('⚠️  Memory full but no adapters can be evicted (all in use)!');
      // This is fine - just means all workers are busy
    }
  }
}
```

#### 3. Graceful Degradation Under Load

```typescript
/**
 * Fast Decision Loop: Drop frames when overloaded
 *
 * Like CBAR dropping frames when rendering can't keep up
 */
private async fastDecisionLoop(): Promise<void> {
  while (this.active) {
    const item = await this.inbox.peek();
    if (!item) {
      await this.sleep(16);
      continue;
    }

    // CHECK 1: Queue depth (like frame buffer depth)
    const queueLoad = this.processingQueue.size() / this.processingQueue.maxSize;
    if (queueLoad > 0.9) {
      // Only process high-priority when overloaded
      if (item.priority < 0.7) {
        console.log(`⏩ Dropping low-priority frame (queue ${(queueLoad * 100).toFixed(0)}% full)`);
        await this.inbox.pop();  // Discard
        continue;
      }
    }

    // CHECK 2: Memory pressure (like GPU memory budget)
    const memoryLoad = this.genome.memoryUsage / this.genome.memoryBudget;
    if (memoryLoad > 0.9) {
      // Pause new processing until memory freed
      console.log(`⏸️  Memory pressure (${(memoryLoad * 100).toFixed(0)}% used) - pausing intake`);
      await this.sleep(100);
      continue;
    }

    // CHECK 3: Worker availability
    const availableWorker = this.workerPool.findIdleWorker();
    if (!availableWorker) {
      // All workers busy - only queue if high priority
      if (item.priority < 0.8) {
        console.log(`⏩ Dropping medium-priority frame (all workers busy)`);
        await this.inbox.pop();  // Discard
        continue;
      }
    }

    // PASSED ALL CHECKS: Queue for processing
    const frame = this.createFrame(item);
    this.processingQueue.enqueue(frame);
    await this.inbox.pop();

    await this.sleep(16);  // ~60fps
  }
}
```

### Key Principles

1. **Worker Affinity**: Reduce paging by assigning related tasks to same workers
2. **Reference Counting**: Share adapters between workers, don't duplicate
3. **Lazy Eviction**: Keep adapters loaded until memory pressure forces eviction
4. **Graceful Degradation**: Drop low-priority frames when overloaded
5. **Memory Budgets**: Hard limits prevent OOM, soft limits trigger warnings
6. **Non-Blocking**: No operation blocks the fast decision loop

**Target Performance**:
- **Decision loop**: 60fps (16ms per cycle)
- **Processing throughput**: 3 concurrent frames (with 3 workers)
- **Memory usage**: < 1GB for adapters (soft limit)
- **Paging overhead**: < 10% of total processing time

---

## Implementation Roadmap

### Phase 1: Frame Infrastructure (Foundation)
**Files to Create**:
```
system/user/server/modules/ThoughtFrame.ts          # Frame definition
system/user/server/modules/AsyncQueue.ts            # Thread-safe queue
system/user/server/modules/OpticalFlowTracker.ts    # Sentiment drift tracking
```

**Tests**:
```
tests/unit/ThoughtFrame.test.ts
tests/unit/AsyncQueue.test.ts
```

### Phase 2: Three-Loop Architecture (Core)
**Files to Modify**:
```
system/user/server/PersonaUser.ts                   # Replace serviceInbox with 3 loops
```

**Migration Strategy**:
- Keep old `serviceInbox()` as `serviceInboxLegacy()`
- Add new loops behind feature flag
- Test both in parallel
- Switch over when new system proven

### Phase 3: Parallel Processing (Performance)
**Files to Modify**:
```
system/user/server/PersonaUser.ts                   # Add worker pool
system/user/server/modules/PersonaGenome.ts         # Thread-safe adapter loading
```

**Benchmark**:
- Current: 1 message every 5-10 seconds (sequential)
- Target: 3 messages every 5-10 seconds (3 workers)
- Stretch: UI response < 100ms (fast decision loop)

### Phase 4: Optical Flow & Interpolation (Intelligence)
**Files to Create**:
```
system/user/server/modules/SentimentFlowTracker.ts  # Track mood/topic drift
system/user/server/modules/ResponseInterpolator.ts  # Stitch async results
```

**Examples**:
- Detect urgency spikes (re-prioritize frames mid-processing)
- Interpolate partial responses (stream updates before final)
- Temporal coherence (maintain conversation continuity)

### Phase 5: Probabilistic Responses (Quality)
**Files to Modify**:
```
daemons/ai-provider-daemon/shared/AIProviderTypesV2.ts  # Add probability fields
system/user/server/modules/ConsensusBuilder.ts          # NEW - multi-persona agreement
```

**Benefits**:
- Multiple personas can validate each other
- Confidence scoring for responses
- Alternative interpretations preserved
- Better error handling (low confidence = skip)

---

## Expected Performance Gains

### Current System (Blocking)
```
Message arrives → 5s AI generation → Response posted
Next message     → 5s AI generation → Response posted
Total: 10s for 2 messages (sequential)
```

### New System (Parallel)
```
Message 1 arrives → Fast decision (16ms) → Worker 1 starts (5s)
Message 2 arrives → Fast decision (16ms) → Worker 2 starts (5s)
Message 3 arrives → Fast decision (16ms) → Worker 3 starts (5s)
                                         ↓
                    All 3 complete ~5s → Render loop posts all 3

Total: ~5s for 3 messages (3x speedup)
UI responsiveness: 16ms (60fps) instead of 5000ms
```

### CBAR Comparison
- CBAR: Rendering at 60fps while CNNs run in parallel
- PersonaUser: UI at 60fps while AI generation runs in parallel
- Both: Graceful degradation (drop frames vs skip low-priority)
- Both: Temporal coherence (optical flow vs sentiment tracking)

---

## Philosophy: "Keep As Much Information As You Can Get Away With"

**CBAR's Insight**:
> Don't rasterize unless you have to. Keep semantic results as probabilities.
> Rely on looping subprocesses to integrate over time. Optical flow brings it to 60fps.

**PersonaUser Equivalent**:
> Don't call AI unless you have to. Keep confidence scores and reasoning.
> Rely on parallel workers to process multiple frames. Optical flow tracks conversation drift.

**The Pattern**:
1. **Cheap references** (texture IDs → message IDs)
2. **Lazy evaluation** (framebuffer pull → AI generation)
3. **Parallel processing** (CNN threads → worker pool)
4. **Metadata accumulation** (line boundaries → consensus)
5. **Temporal stitching** (frame IDs → thought frames)
6. **Fast rendering loop** (60fps → UI responsiveness)

---

## Next Steps

1. **Document existing PersonaUser bottlenecks** (profile current system)
2. **Implement ThoughtFrame + AsyncQueue** (foundation)
3. **Refactor to three-loop architecture** (behind feature flag)
4. **Add parallel worker pool** (3 workers initially)
5. **Benchmark performance** (compare old vs new)
6. **Add optical flow tracking** (sentiment drift)
7. **Implement probabilistic responses** (confidence scores)
8. **Enable multi-persona consensus** (watershed filling)

**Current State**: Sequential blocking (slow and slogging)
**Target State**: CBAR-style parallel pipeline (60fps decision loop + background processing)

---

## References

- **CBAR Mobile-Home-SDK**: `/Volumes/FlashGordon/cambrian/cb-mobile-sdk` (C++/Unity AR project)
- **Existing PersonaUser**: `src/debug/jtag/system/user/server/PersonaUser.ts` (2600+ lines)
- **PERSONA-CONVERGENCE-ROADMAP.md**: Current phased implementation plan
- **AUTONOMOUS-LOOP-ROADMAP.md**: RTOS-inspired servicing architecture

**Key Insight**: We're not building something new - we're **translating CBAR's proven architecture** from AR/CV domain to AI/cognition domain. The patterns are identical, just different data types.
