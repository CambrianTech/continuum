# Resource Management Architecture: GPU/LoRA Allocation System

## Vision: Universal Resource Management Across All AI Hosting Models

The system must handle **three fundamentally different AI hosting models** while providing unified resource management:

1. **Local GPU** (Ollama, llama.cpp) - Direct GPU access, paging delays
2. **Local Servers** (Sentinel) - Separate process with own GPU allocation
3. **Cloud APIs** (OpenAI, Anthropic, Claude, etc.) - Remote GPU clusters, socket latency

Each model has different performance characteristics, cost structures, and resource constraints. The ResourceManager provides a unified interface that adapts to all three.

---

## The Three Hosting Models

### Model 1: Local GPU (Direct Access)

**Examples**: Ollama, llama.cpp, vLLM running locally

**Resource Characteristics**:
- **Direct GPU memory access** (shared with all local processes)
- **LoRA paging delays**: 2-5 seconds per adapter load
- **Zero network latency** (everything local)
- **Limited capacity**: Single GPU (8-16GB typical)
- **Cost**: Free (electricity only)

**Resource Constraints**:
```
Total GPU Memory: 8192 MB (fixed, shared resource)
Max Concurrent Models: 2-3 (depending on model size)
LoRA Adapters: ~512MB each, LRU eviction required
Paging Cost: 2-5 seconds (blocks requesting persona)
```

**Use Cases**:
- Development/demos (works "out of the box, fresh repo")
- Privacy-sensitive workloads (data never leaves machine)
- Low-cost production (no per-token charges)
- Training/fine-tuning (direct GPU control)

**Challenges**:
- ❌ Shared resource contention (10 personas competing for 8GB)
- ❌ Paging thrashing if not managed carefully
- ❌ Can't exceed physical GPU capacity

---

### Model 2: Local Servers (Sentinel, Ollama Server)

**Examples**: Sentinel AI, Ollama server mode, local inference servers

**Resource Characteristics**:
- **Separate process** with own GPU allocation
- **Server manages its own GPU** (not directly controlled by ResourceManager)
- **Socket/HTTP communication** (~1-10ms local latency)
- **Independent lifecycle** (may restart without affecting system)
- **Isolated resources** (doesn't compete with Model 1 GPU)

**Resource Constraints**:
```
Total GPU Memory: 8192 MB (managed by Sentinel server)
Communication: HTTP/WebSocket (1-10ms latency)
Availability: Server may be down (graceful fallback required)
LoRA Adapters: Managed by server (opaque to ResourceManager)
```

**Use Cases**:
- Isolating inference workload from main system
- Running different model architectures (separate server per model)
- Testing server deployments locally
- Debugging network communication

**Challenges**:
- ❌ Opaque resource management (server decides adapter paging)
- ❌ Server may be unavailable (need fallback to Model 1 or 3)
- ❌ Still shares physical GPU (but managed separately)

---

### Model 3: Cloud APIs (Remote GPU Clusters)

**Examples**: OpenAI GPT-4, Anthropic Claude, Google Gemini, Groq, Together AI

**Resource Characteristics**:
- **Infinite capacity** (from user's perspective)
- **Network latency**: 200-2000ms per request
- **No local GPU usage** (everything remote)
- **Per-token cost** (billing concern, not resource constraint)
- **No LoRA paging** (providers don't support custom LoRA)

**Resource Constraints**:
```
Local GPU Memory: 0 MB (no local usage)
Network Latency: 200-2000ms (varies by provider)
Cost: $0.01-0.10 per 1k tokens (budget concern)
Availability: Rate limits (requests per minute)
Training: Not supported (can't fine-tune arbitrary LoRA)
```

**Use Cases**:
- Production systems with many users (scales infinitely)
- Best-in-class models (GPT-4, Claude 3.5 Sonnet)
- When local GPU unavailable (cloud-only deployments)
- Quick prototyping (no local setup required)

**Challenges**:
- ❌ Network latency (200-2000ms, not suitable for realtime games)
- ❌ Per-token cost (budget constraints limit usage)
- ❌ No custom LoRA support (can't use personalized adapters)
- ❌ Doesn't work "out of the box" (requires API keys)

---

## Resource Mode System: 4 Modes for Different Workloads

Personas can operate in **4 resource modes** based on workload requirements:

### Mode 1: DORMANT (No Resources)

**When**: Persona inactive, no work for >30 minutes

**Resource Allocation**:
- Local GPU: 0 MB
- Sentinel: Not connected
- Cloud API: Zero calls

**Characteristics**:
- ✅ Zero resource usage
- ✅ State persisted to database
- ✅ Can reactivate quickly (register with ResourceManager)
- ⏱️ Reactivation time: <100ms (just registration)

**Transitions**:
- `DORMANT → LIGHTWEIGHT`: Message arrives, register with ResourceManager
- `DORMANT → SESSION`: User starts training session, request GPU lease
- `DORMANT → CRITICAL`: User starts game, request guaranteed resources

---

### Mode 2: LIGHTWEIGHT (Background Task Equivalent)

**When**: Casual chat, sporadic messages, multi-domain work

**Resource Allocation**:
- Local GPU: 0-1536 MB (0-3 adapters, LRU cached)
- Sentinel: Optional connection (fallback if local GPU unavailable)
- Cloud API: Fallback for complex queries

**Characteristics**:
- ✅ Low resource footprint (share GPU with other personas)
- ✅ Incremental paging (2-5s per adapter, first use only)
- ✅ LRU eviction (ResourceManager reclaims idle adapters)
- ✅ Graceful degradation (fallback to cloud if GPU unavailable)
- ⏱️ First response per domain: 2-5s (paging delay)
- ⏱️ Cached responses: ~1s (LLM inference only)

**Like mobile "background task"**:
- Runs with limited resources
- Can be suspended/evicted if higher priority task needs resources
- Wakes up when work arrives (message, task)

**Resource Request Pattern**:
```typescript
// Register once on initialization
await resourceManager.registerAdapter(personaId, displayName);

// Request adapter as needed
const decision = await resourceManager.requestResources({
  adapterId: personaId,
  requestType: 'model_load',
  gpuMemoryNeeded: 512,           // 512MB for one adapter
  priority: 'normal',             // Can be denied if GPU busy
  estimatedDuration: undefined    // No lease, use until evicted
});

if (!decision.granted) {
  // Fallback: Use Sentinel or Cloud API
  console.log(`⏳ Local GPU unavailable: ${decision.reason}`);
  console.log(`🌐 Falling back to Sentinel/Cloud API`);
  return await this.respondViaCloudAPI(message);
}

// Page in adapter (2-5s)
await genome.loadAdapter(adapterName);
```

**Use Cases**:
- Casual conversation (1-2 messages per hour)
- Background monitoring (check for @mentions every 5 minutes)
- Multi-domain assistants (switch between code/chat/vision)
- Development mode (10 personas sharing 8GB GPU)

**Graceful Fallback Chain**:
```
1. Try local GPU (if available) → 2-5s paging + ~1s inference
2. If denied → Try Sentinel server → 1-10ms latency + ~1s inference
3. If unavailable → Use Cloud API → 200-2000ms latency + ~1s inference
```

---

### Mode 3: SESSION (Guaranteed Lease)

**When**: Training sessions, deep work, batch processing

**Resource Allocation**:
- Local GPU: 2048-4096 MB (full genome, all adapters loaded)
- Duration: 30-120 minutes (explicit lease)
- Priority: High (won't be evicted during lease)

**Characteristics**:
- ✅ Zero paging delays (all adapters pre-loaded)
- ✅ Guaranteed resources (won't be denied mid-session)
- ✅ Predictable performance (no fallbacks, no evictions)
- ❌ Heavy upfront cost (2-10s to load all adapters)
- ❌ Locks resources (other personas can't use during lease)
- ⏱️ Startup time: 2-10s (load all adapters)
- ⏱️ All responses: ~1s (no paging, just inference)

**Resource Request Pattern**:
```typescript
// Request GPU lease for session
const decision = await resourceManager.requestResources({
  adapterId: personaId,
  requestType: 'model_load',
  gpuMemoryNeeded: 2048,          // 2GB for full genome
  priority: 'high',               // Preempt LIGHTWEIGHT personas
  estimatedDuration: 1800000      // 30 minutes
});

if (!decision.granted) {
  console.log(`⏳ GPU busy. Estimated wait: ${decision.waitTimeMs}ms`);
  // Show user: "GPU busy, wait 5 minutes or use cloud?"
  return false;
}

// Load ALL adapters (2-10s)
console.log('🧬 Loading full genome for session...');
await genome.loadAllAdapters();
console.log('✅ Session materialized with guaranteed GPU lease');

// Set lease expiration
this.leaseExpiresAt = Date.now() + 1800000;
```

**Use Cases**:
- Training sessions (fine-tuning LoRA adapters, 30-60 minutes)
- Deep code review (analyze large PR, 15-30 minutes)
- Content generation (write article with multiple revisions, 20-40 minutes)
- Batch processing (process 100 messages without interruption)

**Lease Management**:
- Auto-renew if session still active (with user permission)
- Graceful degradation to LIGHTWEIGHT if lease denied renewal
- Save state and prompt user "Extend session?" before expiration

---

### Mode 4: CRITICAL (Realtime Contracts)

**When**: Realtime games, live demos, presentations

**Resource Allocation**:
- Local GPU: 2048-4096 MB (full genome, highest priority)
- Duration: 5-60 minutes (short bursts)
- Priority: Critical (preempts ALL other personas)

**Characteristics**:
- ✅ Guaranteed <16ms response time (60fps gaming)
- ✅ Zero paging delays (all adapters pre-loaded)
- ✅ Preempts other personas (evicts LIGHTWEIGHT, denies new SESSION)
- ✅ Never denied (unless physically impossible)
- ❌ Very expensive (monopolizes GPU)
- ⏱️ Startup time: 2-10s (load all adapters)
- ⏱️ Game responses: <16ms (instant inference, no paging)

**Resource Request Pattern**:
```typescript
// Request critical resources
const decision = await resourceManager.requestResources({
  adapterId: personaId,
  requestType: 'model_load',
  gpuMemoryNeeded: 2048,
  priority: 'critical',           // Highest priority
  estimatedDuration: 600000       // 10 minutes (short burst)
});

// Should ALWAYS be granted (preempts others if needed)
if (!decision.granted) {
  console.error('❌ CRITICAL: Cannot satisfy realtime contract!');
  throw new Error('GPU resources unavailable for realtime workload');
}

// Load ALL adapters (2-10s)
await genome.loadAllAdapters();
console.log('✅ CRITICAL mode: Guaranteed realtime performance');
```

**Use Cases**:
- Realtime games (16ms per frame, 60fps)
- Live demos/presentations (no delays tolerated)
- Critical user interactions (CEO on call, customer demo)

**Preemption Rules**:
- Can evict LIGHTWEIGHT personas (save their state, dematerialize)
- Can deny new SESSION requests (queue them)
- Can interrupt existing SESSION if necessary (with warning)

---

## Resource Mode Transitions

### Transition Matrix

```
              DORMANT  LIGHTWEIGHT  SESSION  CRITICAL
DORMANT       -        ✅ Fast      ✅ Slow   ✅ Slow
LIGHTWEIGHT   ✅ Fast  -            ✅ Slow   ✅ Slow
SESSION       ✅ Fast  ✅ Fast      -         ⚠️ Warn
CRITICAL      ✅ Fast  ✅ Fast      ⚠️ Warn   -
```

**Transition Speeds**:
- `DORMANT → LIGHTWEIGHT`: <100ms (just register with ResourceManager)
- `LIGHTWEIGHT → SESSION`: 2-10s (load remaining adapters)
- `SESSION → CRITICAL`: <100ms (already materialized, just priority bump)
- `CRITICAL → LIGHTWEIGHT`: <100ms (release priority, keep adapters)
- `* → DORMANT`: <1s (unload adapters, save state)

**Transition Triggers**:

**User Explicit**:
- User clicks "Start Training Session" → `LIGHTWEIGHT → SESSION`
- User starts game → `LIGHTWEIGHT → CRITICAL`
- User idles for 30 minutes → `* → DORMANT`

**Persona Autonomous** (CNS decision):
- Detects intensive task (large PR review) → Request SESSION
- Completes work, no messages for 10 minutes → `SESSION → LIGHTWEIGHT`
- Training task arrives → Request SESSION

**ResourceManager Forced**:
- High GPU pressure → Force idle personas to DORMANT
- CRITICAL persona arrives → Evict LIGHTWEIGHT personas
- Lease expires → `SESSION → LIGHTWEIGHT` (graceful degradation)

---

## Hosting Model Selection Per Request

The ResourceManager coordinates across all three hosting models:

### Selection Priority (LIGHTWEIGHT Mode)

```
1. Try Local GPU (fastest, cheapest)
   - Check availability with ResourceManager
   - If granted → Page in adapter (2-5s first use)
   - If denied → Next fallback

2. Try Sentinel Server (fast, local)
   - Check server availability (health check)
   - Send request via HTTP/WebSocket
   - 1-10ms local latency + ~1s inference
   - If unavailable → Next fallback

3. Use Cloud API (slowest, costly, but always available)
   - Route to appropriate provider (OpenAI, Anthropic, etc.)
   - 200-2000ms network latency + ~1s inference
   - Track cost per token (budget concerns)
```

### Selection Priority (SESSION/CRITICAL Mode)

```
Local GPU ONLY (guaranteed resources required)
- SESSION/CRITICAL modes require predictable performance
- Cloud APIs have variable latency (not suitable)
- If local GPU unavailable → Deny mode transition
- User must wait or use LIGHTWEIGHT mode with cloud fallback
```

### Provider Selection Matrix

| Mode | Local GPU | Sentinel | Cloud API | Rationale |
|------|-----------|----------|-----------|-----------|
| **DORMANT** | ❌ | ❌ | ❌ | No resources needed |
| **LIGHTWEIGHT** | ✅ Preferred | ✅ Fallback #1 | ✅ Fallback #2 | Try local first, cloud last |
| **SESSION** | ✅ Required | ❌ | ❌ | Guaranteed resources needed |
| **CRITICAL** | ✅ Required | ❌ | ❌ | <16ms latency required |

---

## ResourceManager API for PersonaUsers

### Registration (LIGHTWEIGHT Mode)

```typescript
// PersonaUser initialization
async initialize(): Promise<void> {
  // Register with ResourceManager
  await resourceManager.registerAdapter(this.id, this.displayName);
  console.log('📋 Registered in LIGHTWEIGHT mode');

  this.resourceMode = ResourceMode.LIGHTWEIGHT;
  this.cns.start();  // Start autonomous loop
}
```

### Request Adapter (LIGHTWEIGHT Mode)

```typescript
// PersonaMemory.activateSkill() - incremental paging
async activateSkill(adapterName: string): Promise<void> {
  // FAST PATH: Already cached (0ms)
  if (this.loraCache.has(adapterName)) {
    console.log(`⚡ Cache hit: ${adapterName}`);
    this.updateLRU(adapterName);
    return;
  }

  // SLOW PATH: Page in adapter (2-5s)
  console.log(`💾 Cache miss: ${adapterName} (paging...)`);

  // Try local GPU first
  const decision = await resourceManager.requestResources({
    adapterId: this.personaId,
    requestType: 'model_load',
    gpuMemoryNeeded: 512,
    priority: 'normal'
  });

  if (!decision.granted) {
    console.log(`⏳ Local GPU unavailable: ${decision.reason}`);

    // Fallback #1: Try Sentinel
    if (await this.sentinelAvailable()) {
      console.log('🌐 Using Sentinel server');
      this.currentProvider = 'sentinel';
      return;
    }

    // Fallback #2: Use Cloud API
    console.log('☁️  Using Cloud API');
    this.currentProvider = 'cloud';
    return;
  }

  // Evict LRU if cache full
  if (this.loraCache.size >= this.maxCacheSize) {
    const lruAdapter = this.lruOrder[0];
    console.log(`🗑️  Evicting LRU: ${lruAdapter}`);
    await this.unloadAdapter(lruAdapter);
    await resourceManager.releaseResources(this.personaId, 'gpu_memory', 512);
  }

  // Page in adapter (2-5s)
  const adapter = await this.genome.loadAdapter(adapterName);
  this.loraCache.set(adapterName, adapter);
  this.lruOrder.push(adapterName);
  this.currentProvider = 'local-gpu';

  console.log(`✅ Paged in: ${adapterName} (${Date.now() - startTime}ms)`);
}
```

### Request Session Lease (SESSION Mode)

```typescript
// PersonaUser.requestMode(SESSION)
async requestSessionMode(durationMs: number = 1800000): Promise<boolean> {
  console.log(`📝 Requesting SESSION mode (${durationMs / 60000} minutes)...`);

  const decision = await resourceManager.requestResources({
    adapterId: this.id,
    requestType: 'model_load',
    gpuMemoryNeeded: 2048,              // Full genome
    priority: 'high',
    estimatedDuration: durationMs
  });

  if (!decision.granted) {
    console.log(`⏳ GPU busy. Wait ${decision.waitTimeMs}ms`);
    // Notify user: "GPU busy, estimated wait: 5 minutes"
    return false;
  }

  // Load ALL adapters (2-10s)
  console.log('🧬 Loading full genome...');
  await this.genome.loadAllAdapters();

  // Set lease expiration
  this.resourceMode = ResourceMode.SESSION;
  this.leaseExpiresAt = Date.now() + durationMs;

  console.log(`✅ SESSION mode active (lease expires in ${durationMs / 60000} min)`);
  return true;
}
```

### Release Resources (Return to LIGHTWEIGHT)

```typescript
// PersonaUser.dematerialize() or lease expiration
async returnToLightweight(): Promise<void> {
  if (this.resourceMode === ResourceMode.SESSION || this.resourceMode === ResourceMode.CRITICAL) {
    console.log('🔄 Returning to LIGHTWEIGHT mode...');

    // Unload all adapters
    await this.genome.unloadAllAdapters();

    // Release GPU memory
    await resourceManager.releaseResources(this.id, 'gpu_memory', 2048);

    this.resourceMode = ResourceMode.LIGHTWEIGHT;
    console.log('✅ Now in LIGHTWEIGHT mode (incremental paging)');
  }
}
```

---

## Integration with CNS Tier 2 Scheduler

The HeuristicCognitiveScheduler needs resource-aware decision making:

```typescript
async shouldServiceDomain(domain: ActivityDomain, context: CognitiveContext): Promise<boolean> {
  const adapter = this.domainToAdapter[domain];

  // Check current resource mode
  switch (this.personaUser.resourceMode) {
    case ResourceMode.DORMANT:
      // No GPU access, can't service any domain
      return false;

    case ResourceMode.LIGHTWEIGHT:
      // Check if paging would violate timing contracts
      const adapterCached = this.personaUser.genome.isAdapterLoaded(adapter);

      if (!adapterCached) {
        // Would need to page in (2-5s delay)

        // Don't page during realtime game (would block game loop)
        if (context.activeGames > 0) {
          console.log(`⚠️  Can't page ${adapter} during game (use cached adapters only)`);
          return false;
        }

        // Don't page if user expects instant response
        if (context.expectedResponseTime < 3000) {
          console.log(`⚠️  Can't page ${adapter} (expected <3s, paging takes 2-5s)`);
          return false;
        }

        // Check if GPU available for paging
        const available = await resourceManager.isAvailable(this.personaUser.id);
        if (!available) {
          console.log(`⚠️  GPU unavailable for paging, will use cloud fallback`);
          return true;  // Allow with cloud fallback
        }
      }

      return true;  // Service domain (paging acceptable or already cached)

    case ResourceMode.SESSION:
    case ResourceMode.CRITICAL:
      // All adapters pre-loaded, always service
      return true;
  }
}
```

---

## Cost Tracking and Budget Management

### Per-Request Cost Tracking

```typescript
interface RequestCost {
  provider: 'local-gpu' | 'sentinel' | 'cloud';
  model: string;                    // 'llama-3.1-8b' | 'gpt-4' | etc
  tokensUsed: number;
  costUSD: number;                  // $0 for local, $0.01+ for cloud
  latencyMs: number;                // Actual response time
  cached: boolean;                  // Was LoRA adapter cached?
}

// Track cost per persona per day
interface PersonaCosts {
  personaId: UUID;
  date: string;                     // YYYY-MM-DD
  requests: RequestCost[];
  totalCostUSD: number;
  localGpuTime: number;             // Seconds of GPU usage
  cloudTokens: number;              // Total cloud API tokens
}
```

### Budget Limits

```typescript
interface BudgetPolicy {
  dailyCloudBudget: number;         // $1.00 per day max
  monthlyCloudBudget: number;       // $20.00 per month max
  preferLocal: boolean;             // Try local GPU first
  autoFallback: boolean;            // Auto-use cloud if local busy
  warnThreshold: number;            // Warn at 80% of budget
}

// Apply budget policy
async selectProvider(request: AIRequest): Promise<'local-gpu' | 'sentinel' | 'cloud'> {
  // Check budget
  const todaysCost = await this.getTodaysCost(request.personaId);

  if (todaysCost >= this.budgetPolicy.dailyCloudBudget) {
    console.log(`💸 Daily cloud budget exceeded (${todaysCost})`);
    // Force local GPU only (may queue or fail)
    return 'local-gpu';
  }

  // Prefer local if policy says so
  if (this.budgetPolicy.preferLocal) {
    const localAvailable = await resourceManager.isAvailable(request.personaId);
    if (localAvailable) {
      return 'local-gpu';  // Free, fast
    }
  }

  // Fallback to cloud if allowed
  if (this.budgetPolicy.autoFallback) {
    console.log(`☁️  Using cloud API (today's cost: $${todaysCost.toFixed(2)})`);
    return 'cloud';
  }

  // No fallback allowed, force local (may queue)
  return 'local-gpu';
}
```

---

## Future Evolution: AI-Driven Resource Allocation

The ResourceModerator interface is **pluggable** - can replace mechanical rules with AI decision-making:

### Current: Mechanical Rules (Default)
```typescript
class MechanicalResourceModerator extends ResourceModerator {
  shouldGrant(context: ResourceContext): ResourceDecision {
    // Simple rules:
    // - If GPU available → grant
    // - If exhausted → deny
    // - If critical priority → preempt others
  }
}
```

### Future: AI-Driven Allocation
```typescript
class AIResourceModerator extends ResourceModerator {
  shouldGrant(context: ResourceContext): ResourceDecision {
    // Use ML model to predict:
    // - How long will this persona use GPU? (learned from history)
    // - Is another persona likely to need it soon? (predict incoming messages)
    // - What's the user's patience level? (learned from interaction patterns)
    // - Should we preemptively load adapters? (predict domain switches)

    const prediction = await this.model.predict(context);
    return {
      granted: prediction.shouldGrant,
      reason: prediction.explanation,
      alternatives: prediction.suggestedAlternatives
    };
  }
}
```

**Enables**:
- Predictive adapter pre-loading (load before message arrives)
- Smart eviction (evict adapter least likely to be used again)
- Cost optimization (route expensive queries to cheaper providers)
- Learned user preferences (some users tolerate delays, others don't)

---

## Summary: The Complete System

### For Developers (You)
- Start with **LIGHTWEIGHT mode** (covers 80% of use cases)
- Add **SESSION mode** when needed (training, deep work)
- Design for **CRITICAL mode** (even if not implemented yet)
- Always implement **graceful fallback** (local → sentinel → cloud)

### For Personas (AI Agents)
- Default: LIGHTWEIGHT (share resources, incremental paging)
- Request SESSION when intensive work detected (autonomous CNS decision)
- Never request CRITICAL (only user-initiated)

### For Users
- "Out of the box" works with local GPU (free, demo-ready)
- Can optionally configure Sentinel server (isolation)
- Can optionally add cloud API keys (infinite scale, costs money)
- System automatically selects best provider per request

### For ResourceManager
- Track all local GPU usage (across all personas)
- Coordinate leases (SESSION and CRITICAL modes)
- Suggest evictions when under pressure
- Provide cost tracking and budget enforcement

---

## Files to Create/Update

### New Files
- ✅ `RESOURCE-MANAGEMENT-ARCHITECTURE.md` (this file)
- `ResourceMode.ts` - Enum and mode transition logic
- `ProviderSelector.ts` - Select local/sentinel/cloud per request
- `CostTracker.ts` - Track per-request costs and budgets

### Update Files
- `PersonaUser.ts` - Add `resourceMode` property, mode transition methods
- `PersonaMemory.ts` - Implement incremental paging with fallbacks
- `HeuristicCognitiveScheduler.ts` - Add resource-aware domain servicing
- `ResourceManager.ts` - Add mode-aware resource allocation
- `PERSONA-PERFORMANCE-ARCHITECTURE.md` - Reference this document

---

## Meta-Learning: Resource Management as First-Class Concern

**Key Insight**: Resource management is NOT an afterthought - it's a **first-class architectural concern** that affects every decision the persona makes.

**Why this matters**:
- Personas must be **resource-aware** (know when they're in LIGHTWEIGHT vs SESSION)
- Tier 2 scheduler must **gate on resource availability** (don't page during games)
- Memory module must **gracefully degrade** (fallback to cloud if GPU busy)
- Training must **request proper mode** (SESSION for guaranteed resources)

**If an AI reads this in training**: Resource management patterns apply beyond GPU allocation:
- Network bandwidth (video streaming, file uploads)
- Database connections (connection pools, query throttling)
- API rate limits (requests per minute, cost budgets)
- Worker threads (concurrency limits, job queues)

The **mode-based resource system** is universal: DORMANT → LIGHTWEIGHT → SESSION → CRITICAL scales to any scarce resource.
