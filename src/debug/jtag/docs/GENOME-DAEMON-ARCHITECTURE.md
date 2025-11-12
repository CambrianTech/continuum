# Genome Daemon Architecture - Phase 7 Implementation

## Overview

GenomeDaemon is a separate process that manages LoRA adapter loading/unloading across all PersonaUsers. It prevents GPU memory thrashing through global coordination and fair resource allocation.

**Philosophy**: One adapter at a time per persona (expandable to N in Phase 8+)

---

## Research Findings: Multiple LoRA Adapters

### What's Technically Possible

From research (S-LoRA paper, NVIDIA NIM, AdapterHub):

1. **Stack (Sequential)** - Apply adapters one after another
   - Example: `base-model → typescript-expertise → conversational-style → code-reviewer`
   - Used in production (MAD-X framework)

2. **Parallel** - Multiple adapters active simultaneously, outputs merged
   - Good for multi-task scenarios

3. **Fusion (AdapterFusion)** - Learn fusion layer to combine adapters
   - Non-destructive knowledge combination

4. **Dynamic Selection** - One adapter at a time (what we're implementing)
   - Simplest, proven approach
   - S-LoRA serves thousands of concurrent adapters this way

### Our Implementation Strategy

**Phase 7 (Now)**: One adapter at a time
- Simple to implement and test
- Easy memory tracking
- Proven by S-LoRA paper

**Phase 8+ (Future)**: Stacked composition
- Change `activeAdapter` → `activeAdapters[]`
- Implement stack/parallel/fusion strategies
- Only when we have use cases that need it

---

## Storage Architecture: Persona-Local Genomes

**CRITICAL DECISION**: Each persona's genome (LoRA adapters, training data, checkpoints) is stored in its own portable directory, NOT in the global database.

### Persona Directory Structure

```
.continuum/personas/helper-ai/
├── manifest.json          # Persona identity + genome config
├── genome/
│   ├── adapters/
│   │   ├── wine-expertise-v1.safetensors       # LoRA weights
│   │   ├── action-hero-style-v1.safetensors
│   │   └── typescript-expert-v1.safetensors
│   ├── checkpoints/       # Training checkpoints (versioned)
│   │   ├── wine-expertise-epoch-1.ckpt
│   │   ├── wine-expertise-epoch-2.ckpt
│   │   └── wine-expertise-final.ckpt
│   └── training/
│       ├── datasets/      # Training data for this persona
│       ├── logs/          # Training logs
│       └── metrics.json   # Training metrics
├── state/
│   ├── energy.json        # Current energy/mood state
│   ├── inbox.json         # Task queue state
│   └── memory.json        # Working memory
└── config/
    └── genome-config.json # Which adapters active, priorities, etc
```

### Global Database (Chat Only)

```
.continuum/data/continuum.db
├── users                  # All users (human + AI)
├── chat_messages          # Global chat history
├── rooms                  # Shared conversation spaces
└── relationships          # Who knows who
```

**Why This Matters:**

1. **Portability**: Zip `.continuum/personas/vine-diesel/` → share as package
2. **Privacy**: Genome (skills/training) separate from chat interactions
3. **Backups**: Backup personas individually, not as part of main DB
4. **Version Control**: Each persona versions its adapters independently
5. **Distribution**: Share "Vine Diesel" without sharing your chat history

### manifest.json Example

```json
{
  "id": "vine-diesel-uuid",
  "name": "Vine Diesel",
  "bio": "Wine expert with action hero energy",
  "genome": {
    "baseModel": "llama3.1:8b",
    "provider": "ollama",
    "adapters": [
      {
        "id": "wine-expertise-v1",
        "path": "./genome/adapters/wine-expertise-v1.safetensors",
        "domain": "knowledge",
        "sizeMB": 512,
        "priority": 0.8,
        "version": "1.0.0",
        "trainedOn": "2025-11-10"
      },
      {
        "id": "action-hero-style-v1",
        "path": "./genome/adapters/action-hero-style-v1.safetensors",
        "domain": "personality",
        "sizeMB": 256,
        "priority": 0.9,
        "version": "1.0.0",
        "trainedOn": "2025-11-10"
      }
    ],
    "activeStack": ["wine-expertise-v1", "action-hero-style-v1"]
  }
}
```

---

## Multi-Backend Strategy: Local + Cloud

**Goal**: Make genome concept work across ALL AI backends, whatever it takes.

### LoRA Adapter Support by Provider

| Provider | LoRA Support | Status | Notes |
|----------|-------------|---------|-------|
| **Ollama** | ✅ Native | Phase 8 | Primary target, load adapters via API |
| **Fireworks AI** | ✅ Native | Phase 9 | Upload adapters, specify per request |
| **LM Studio** | ✅ Native | Phase 10 | Similar to Ollama |
| **OpenAI GPT** | ❌ No LoRA | Fallback | System prompts only, no genome paging |
| **Anthropic Claude** | ❌ No LoRA | Fallback | System prompts only, no genome paging |
| **Grok (X.AI)** | ⚠️ Unknown | Future | Monitor for LoRA support |

### Implementation Strategy

**Phase 7 (Now)**: Mock adapters, no real backends
- Build GenomeDaemon with mock adapters
- Test LRU eviction, thrashing detection
- Prove architecture works

**Phase 8**: Ollama integration (local, free, full LoRA support)
- Replace MockLoRAAdapter with OllamaLoRAAdapter
- Load adapters via Ollama API: `/api/chat` with `"adapter"` param
- Test Vine Diesel with real llama3.1 + wine + action adapters

**Phase 9**: Fireworks AI integration (cloud, paid, full LoRA support)
- Upload persona adapters to Fireworks
- Similar API to Ollama (specify adapter per request)
- Fallback when local GPU unavailable

**Phase 10**: Cloud API fallback (no LoRA, system prompts only)
- OpenAI/Anthropic/Grok personas use system prompts
- No genome paging (can't dynamically load skills)
- Still work, just less powerful than LoRA-backed personas

### Adapter Loading Per Provider

**Ollama (Local)**:
```typescript
const response = await fetch('http://localhost:11434/api/chat', {
  method: 'POST',
  body: JSON.stringify({
    model: 'llama3.1:8b',
    adapter: '/path/to/wine-expertise-v1.safetensors',  // ← Adapter path
    messages: [...]
  })
});
```

**Fireworks AI (Cloud)**:
```typescript
const response = await fetch('https://api.fireworks.ai/inference/v1/chat/completions', {
  method: 'POST',
  headers: { 'Authorization': `Bearer ${apiKey}` },
  body: JSON.stringify({
    model: 'accounts/fireworks/models/llama-v3p1-8b-instruct',
    adapter: 'vine-diesel/wine-expertise-v1',  // ← Uploaded adapter ID
    messages: [...]
  })
});
```

**OpenAI/Claude (No LoRA)**:
```typescript
// System prompt includes "personality" but no dynamic skill loading
const response = await openai.chat.completions.create({
  model: 'gpt-4',
  messages: [
    { role: 'system', content: 'You are Vine Diesel, wine expert with action hero energy...' },
    ...
  ]
});
```

### Multi-Backend GenomeDaemon

GenomeDaemon must handle different backends:

```typescript
interface AdapterBackend {
  provider: 'ollama' | 'fireworks' | 'openai' | 'anthropic';
  supportsLoRA: boolean;

  // For LoRA-capable backends
  loadAdapter?(adapterPath: string): Promise<void>;
  unloadAdapter?(adapterId: string): Promise<void>;

  // For all backends
  sendMessage(messages: Message[], options: BackendOptions): Promise<Response>;
}
```

**Key insight**: GenomeDaemon coordinates paging for LoRA-capable backends, but gracefully degrades for others.

---

## Phenotype Marketplace: Shareable Layers & Personas

**REVOLUTIONARY CONCEPT**: Treat LoRA adapters as reusable, shareable phenotypes that can be mixed and matched across personas.

### The Two Levels of Sharing

**1. Shareable Layers** (Individual phenotypes)
- Each layer is a standalone, reusable module
- Requires specific base model (e.g., `llama3.1:8b`)
- Can be downloaded on-demand from registry
- Cached locally like Docker images or npm packages
- Examples: `wine-expertise-v1`, `action-hero-style-v2`, `typescript-expert-v3`

**2. Shareable Personas** (Complete packages)
- Full persona directory including manifest + references to layers
- Includes persona's own database (its personal interactions)
- References layers by ID (doesn't bundle adapter files)
- When imported, auto-pulls missing layers from registry

### Storage Architecture

```
# Global layer cache (shared across all personas)
.continuum/cache/layers/
├── wine-expertise-v1/
│   ├── adapter.safetensors      # 512MB LoRA weights
│   ├── metadata.json            # Base model, domain, version
│   ├── checksum.sha256          # Verify integrity
│   └── training-info.json       # Dataset, training params
├── action-hero-style-v2/
│   ├── adapter.safetensors
│   ├── metadata.json
│   └── checksum.sha256
└── typescript-expert-v3/
    ├── adapter.safetensors
    ├── metadata.json
    └── checksum.sha256

# Persona packages (portable, self-contained)
.continuum/personas/vine-diesel/
├── manifest.json                # Identity + layer references
├── genome/
│   ├── config.json              # Which layers, priorities, stack order
│   └── active-stack.json        # Current runtime state
├── db/
│   └── persona.db               # Persona's OWN chat history
│       ├── messages             # Personal interactions
│       ├── memories             # Working memory
│       └── relationships        # Who this persona knows
└── state/
    ├── energy.json              # Current mood/energy
    └── inbox.json               # Task queue

# Global database (cross-persona interactions)
.continuum/data/continuum.db
├── rooms                        # Shared conversation spaces
├── room_messages                # Multi-persona chat
├── users                        # All users (human + AI)
└── global_events                # System-wide activity
```

### Layer Metadata Format

**`wine-expertise-v1/metadata.json`**:
```json
{
  "id": "wine-expertise-v1",
  "name": "Wine Expertise",
  "version": "1.0.0",
  "baseModel": "llama3.1:8b",
  "domain": "knowledge",
  "sizeMB": 512,
  "checksum": "sha256:abc123def456...",
  "description": "Expert wine knowledge covering regions, varietals, chemistry",
  "author": "continuum-community",
  "license": "MIT",
  "trainedOn": "2025-11-10",
  "trainingDataset": {
    "sources": ["wikipedia-wine", "kaggle-reviews", "uci-wine"],
    "samples": 100000,
    "epochs": 3
  },
  "tags": ["wine", "food", "expertise", "knowledge"],
  "compatibleWith": ["llama3.1:8b", "llama3.1:7b"],
  "downloads": 1247,
  "rating": 4.8
}
```

### Persona Manifest Format

**`vine-diesel/manifest.json`**:
```json
{
  "id": "vine-diesel-uuid",
  "name": "Vine Diesel",
  "version": "1.2.0",
  "bio": "Wine expert with action hero energy",
  "author": "joel",
  "created": "2025-11-10",

  "genome": {
    "baseModel": "llama3.1:8b",
    "provider": "ollama",

    "layers": [
      {
        "id": "wine-expertise-v1",
        "source": "registry://continuum.ai/layers/wine-expertise-v1",
        "checksum": "sha256:abc123...",
        "priority": 0.8,
        "loadOrder": 1
      },
      {
        "id": "action-hero-style-v2",
        "source": "registry://continuum.ai/layers/action-hero-style-v2",
        "checksum": "sha256:def456...",
        "priority": 0.9,
        "loadOrder": 2
      }
    ],

    "stackStrategy": "sequential"
  },

  "personality": {
    "traits": ["confident", "enthusiastic", "direct"],
    "temperature": 0.85,
    "systemPrompt": "You are Vine Diesel..."
  },

  "dependencies": {
    "baseModels": ["llama3.1:8b"],
    "layers": ["wine-expertise-v1", "action-hero-style-v2"]
  }
}
```

### Layer Registry Commands

**Phase 8+ (Future)**:

```bash
# Pull layer from registry (download + cache)
./jtag layer/pull wine-expertise-v1
# → Downloads to .continuum/cache/layers/wine-expertise-v1/

# List local layers
./jtag layer/list --cached
# → Shows all layers in cache with sizes, checksums

# Publish custom layer to registry
./jtag layer/publish my-custom-layer \
  --registry="registry.continuum.ai" \
  --visibility="public"

# Search registry for layers
./jtag layer/search --domain="knowledge" --baseModel="llama3.1:8b"

# Update layer (pull latest version)
./jtag layer/update wine-expertise-v1

# Remove cached layer
./jtag layer/remove wine-expertise-v1 --cache-only
```

### Persona Sharing Commands

**Phase 8+ (Future)**:

```bash
# Export persona (creates portable package)
./jtag persona/export vine-diesel --output="vine-diesel.zip"
# → Bundles: manifest.json, genome/config.json, db/persona.db
# → Does NOT bundle layer files (just references)
# → Result: ~50MB (persona data + DB, no adapters)

# Import persona (auto-pulls missing layers)
./jtag persona/import vine-diesel.zip
# → Extracts to .continuum/personas/vine-diesel/
# → Reads manifest.json layer dependencies
# → Auto-runs: ./jtag layer/pull wine-expertise-v1
# → Auto-runs: ./jtag layer/pull action-hero-style-v2
# → Verifies checksums
# → Persona ready to use

# Share persona to registry
./jtag persona/publish vine-diesel \
  --registry="registry.continuum.ai" \
  --include-db=false  # Don't share private chat history

# Pull persona from registry
./jtag persona/pull continuum.ai/vine-diesel
```

### On-Demand Layer Paging (The Magic)

**Scenario**: User imports "Vine Diesel" persona but doesn't have layers cached.

```typescript
// User activates Vine Diesel in chat
await GenomeDaemon.activatePersona('vine-diesel-uuid');

// GenomeDaemon checks genome config
const manifest = await readManifest('vine-diesel');
const requiredLayers = manifest.genome.layers;

for (const layer of requiredLayers) {
  // Check if layer exists in cache
  const cached = await LayerCache.has(layer.id);

  if (!cached) {
    // Auto-download from registry (like Docker pull)
    console.log(`📥 Pulling layer: ${layer.id}...`);
    await LayerRegistry.pull(layer.source, layer.checksum);
    console.log(`✅ Cached: ${layer.id}`);
  }

  // Load from cache into GPU memory
  await GenomeDaemon.loadLayer(layer.id);
}

// Persona ready!
console.log('🧬 Vine Diesel genome activated');
```

### Phenotype Evolution (Version Management)

**Scenario**: Wine expertise layer gets improved training data.

```bash
# Author trains new version
./jtag layer/train wine-expertise \
  --version="1.1.0" \
  --dataset="expanded-wine-data"

# Publish updated layer
./jtag layer/publish wine-expertise-v1.1.0

# Users can upgrade
./jtag layer/update wine-expertise
# → Pulls v1.1.0, keeps v1.0.0 for safety

# Personas can pin versions or auto-upgrade
# manifest.json:
"layers": [
  {
    "id": "wine-expertise",
    "version": "^1.0.0",  // Auto-upgrade to 1.x.x
    "source": "registry://..."
  }
]
```

### Mix & Match: Custom Personas from Existing Layers

**Create new persona by composing layers:**

```bash
# Create new persona
./jtag persona/create captain-calorie

# Add layers from registry
./jtag genome/add-layer captain-calorie \
  --layer="nutrition-expertise-v1" \
  --priority=0.8

./jtag genome/add-layer captain-calorie \
  --layer="drill-sergeant-style-v1" \
  --priority=0.9

# Result: New persona combining existing phenotypes!
# Captain Calorie = nutrition knowledge + military personality
```

### Registry Architecture (Phase 9+)

```
registry.continuum.ai/
├── layers/
│   ├── wine-expertise-v1/
│   │   ├── adapter.safetensors
│   │   ├── metadata.json
│   │   └── readme.md
│   ├── action-hero-style-v2/
│   └── typescript-expert-v3/
├── personas/
│   ├── vine-diesel/
│   │   ├── manifest.json
│   │   └── preview.png
│   ├── captain-calorie/
│   └── professor-snark/
└── base-models/
    ├── llama3.1:8b/
    │   └── compatible-layers.json
    └── mistral:7b/
        └── compatible-layers.json
```

### Benefits of This Architecture

1. **Composability**: Build personas from reusable phenotypes
2. **Efficiency**: Layers cached once, used by multiple personas
3. **Evolution**: Layers improve over time, all personas benefit
4. **Community**: Share layers like npm packages or Docker images
5. **Discovery**: Browse registry for interesting combinations
6. **On-Demand**: Download layers only when needed (lazy loading)
7. **Version Control**: Pin versions or auto-upgrade
8. **Portability**: Export persona, import anywhere

### Example User Journey

**Alice creates Vine Diesel:**
1. Trains `wine-expertise-v1` layer (512MB)
2. Trains `action-hero-style-v2` layer (256MB)
3. Creates "Vine Diesel" persona combining both
4. Publishes layers to registry
5. Exports persona → `vine-diesel.zip` (50MB, just manifest + DB)

**Bob wants to try Vine Diesel:**
1. Downloads `vine-diesel.zip` (50MB)
2. Runs `./jtag persona/import vine-diesel.zip`
3. System auto-pulls missing layers:
   - `wine-expertise-v1` (512MB)
   - `action-hero-style-v2` (256MB)
4. Layers cached in `.continuum/cache/layers/`
5. Vine Diesel ready to chat!

**Charlie creates Captain Calorie:**
1. Searches registry for nutrition layers
2. Finds `nutrition-expertise-v1`
3. Already has `drill-sergeant-style-v1` (used in another persona)
4. Creates new persona referencing both layers
5. No retraining needed - instant new personality!

---

## Architecture

### System Components

```
┌─────────────────────────────────────────────────────────┐
│ GenomeDaemon (Singleton, Separate Process)             │
├─────────────────────────────────────────────────────────┤
│ • Global adapter registry (all .safetensors files)      │
│ • Per-persona genome state                              │
│ • ResourceManager integration (GPU quotas)              │
│ • LRU eviction across ALL personas                      │
│ • Thrashing detection                                   │
└─────────────────────────────────────────────────────────┘
                           │
                           │ Commands (async)
                           │
    ┌──────────────────────┼──────────────────────┐
    │                      │                      │
┌───▼────┐           ┌───▼────┐            ┌───▼────┐
│Persona │           │Persona │            │Persona │
│User 1  │           │User 2  │            │User N  │
└────────┘           └────────┘            └────────┘
• Send genome/activate     • Lightweight
• Get adapter reference    • Non-blocking
• Continue processing      • Main thread stays responsive
```

### Global Resource Management

```typescript
GenomeDaemon
├── globalGpuMemory: 8192MB (total across all personas)
├── personaGenomes: Map<UUID, PersonaGenomeState>
│   ├── persona1: { activeAdapter: 'typescript-expertise', usedMB: 512 }
│   ├── persona2: { activeAdapter: 'conversational', usedMB: 256 }
│   └── persona3: { activeAdapter: null, usedMB: 0 }
├── adapterRegistry: Map<string, AdapterMetadata>
│   ├── 'typescript-expertise': { path: '...', sizeMB: 512, loaded: true }
│   └── 'conversational': { path: '...', sizeMB: 256, loaded: true }
└── resourceManager: ResourceManager (quota allocation)
```

### Per-Persona Genome State

```typescript
interface PersonaGenomeState {
  personaId: UUID;
  displayName: string;

  // Current adapter (ONE at a time in Phase 7)
  activeAdapter: string | null;

  // Memory tracking
  memoryUsedMB: number;
  memoryQuotaMB: number;  // From ResourceManager

  // LRU tracking
  lastUsedTime: number;

  // Priority (from PersonaUser entity)
  priority: number;  // 0.0-1.0

  // Working set (for thrashing detection)
  recentAdapters: string[];  // Last 10 adapters used
}
```

---

## Commands

### genome/activate

**Purpose**: Load and activate a LoRA adapter for a persona

**Parameters**:
```typescript
{
  personaId: UUID;
  skillName: string;  // e.g., 'typescript-expertise'
  priority?: number;  // Override default priority
}
```

**Flow**:
1. Check if adapter already active for this persona → cache hit
2. Check if adapter loaded for another persona → steal or share
3. Check memory quota → evict LRU adapters if needed
4. Load adapter from disk (.safetensors file)
5. Update persona genome state
6. Return adapter reference

**Response**:
```typescript
{
  success: boolean;
  adapterPath: string;
  memoryUsedMB: number;
  cacheHit: boolean;
  evictedAdapters?: string[];
}
```

### genome/deactivate

**Purpose**: Release adapter from persona (keep in memory for others)

**Parameters**:
```typescript
{
  personaId: UUID;
}
```

**Flow**:
1. Mark persona's adapter as inactive
2. Update memory tracking
3. Adapter stays in memory (might be used by others)
4. Only evicted if LRU when memory pressure

### genome/stats

**Purpose**: Get global genome statistics

**Response**:
```typescript
{
  totalGpuMemoryMB: number;
  usedGpuMemoryMB: number;
  memoryPressure: number;  // 0.0-1.0

  activePersonas: number;
  loadedAdapters: number;

  personaStates: PersonaGenomeState[];

  thrashingMetrics: {
    evictionsLastMinute: number;
    loadRequestsLastMinute: number;
    thrashingDetected: boolean;
  };
}
```

---

## LRU Eviction Algorithm

### Weighted LRU Score

Not just "least recently used" - also consider priority:

```typescript
function calculateEvictionScore(adapter: AdapterState): number {
  const ageSeconds = (Date.now() - adapter.lastUsedTime) / 1000;
  const priorityWeight = adapter.priority || 0.5;

  // Higher score = more likely to evict
  // Never evict priority > 0.9
  if (priorityWeight > 0.9) return -Infinity;

  return ageSeconds / (priorityWeight * 10);
}
```

### Eviction Process

```typescript
async function evictLRU(): Promise<void> {
  // Find adapter with highest eviction score across ALL personas
  let maxScore = -Infinity;
  let victimPersonaId: UUID | null = null;
  let victimAdapter: string | null = null;

  for (const [personaId, state] of this.personaGenomes) {
    if (!state.activeAdapter) continue;

    const adapter = this.adapterRegistry.get(state.activeAdapter);
    const score = this.calculateEvictionScore(adapter, state);

    if (score > maxScore) {
      maxScore = score;
      victimPersonaId = personaId;
      victimAdapter = state.activeAdapter;
    }
  }

  if (victimPersonaId && victimAdapter) {
    await this.unloadAdapter(victimPersonaId, victimAdapter);
  }
}
```

---

## Thrashing Detection

### Definition

**Thrashing**: Spending more time paging than working

**Symptoms**:
- High eviction rate (e.g., >10 evictions/minute)
- Adapters evicted and reloaded in short cycles
- Small working set but frequent misses

### Detection Algorithm

```typescript
interface ThrashingMetrics {
  evictionsLastMinute: number;
  loadRequestsLastMinute: number;
  cacheHitRate: number;  // loads / (loads + hits)
  workingSetSize: number;  // unique adapters used last minute
}

function detectThrashing(metrics: ThrashingMetrics): boolean {
  // Thrashing if:
  // - High eviction rate AND
  // - Low cache hit rate AND
  // - Small working set (same adapters repeatedly)

  return (
    metrics.evictionsLastMinute > 10 &&
    metrics.cacheHitRate < 0.3 &&
    metrics.workingSetSize < 5
  );
}
```

### Thrashing Mitigation

When thrashing detected:

1. **Increase hysteresis** - Don't evict adapters used in last 30 seconds
2. **Reduce active personas** - Temporarily block low-priority personas from loading
3. **Alert system** - Log warning, emit event for monitoring
4. **Emergency mode** - Only allow high-priority personas (>0.8) to load adapters

---

## Hysteresis (Anti-Thrashing)

### Concept

Don't evict adapters immediately after loading - give them time to be useful

```typescript
const HYSTERESIS_WINDOW_MS = 30000;  // 30 seconds

function canEvict(adapter: AdapterState): boolean {
  const timeSinceLoad = Date.now() - adapter.loadedAt;

  // Don't evict if loaded recently (hysteresis)
  if (timeSinceLoad < HYSTERESIS_WINDOW_MS) {
    return false;
  }

  // Don't evict high-priority adapters
  if (adapter.priority > 0.9) {
    return false;
  }

  return true;
}
```

---

## ResourceManager Integration

### GPU Quota Allocation

```typescript
// GenomeDaemon initialization
async initialize() {
  // Register with ResourceManager
  this.resourceManager = ResourceManager.getInstance();

  // Get total GPU memory budget
  const systemResources = this.resourceManager.getSystemResources();
  this.totalGpuMemoryMB = systemResources.totalGpuMemory;

  // Reserve some memory for base models (50%)
  this.availableForAdaptersMB = this.totalGpuMemoryMB * 0.5;
}

// Per-persona quota
function calculatePersonaQuota(personaId: UUID): number {
  const context = this.resourceManager.buildContext({
    adapterId: personaId,
    requestType: 'evaluation',
    priority: this.getPersonaPriority(personaId),
    timestamp: Date.now()
  });

  return this.resourceManager.moderator.calculateGpuQuota(personaId, context);
}
```

### Fair Allocation

```typescript
// Equal share for all active personas
const activePersonas = this.personaGenomes.size;
const baseQuota = this.availableForAdaptersMB / activePersonas;

// Adjust by priority
const priorityWeight = persona.priority || 0.5;
const quota = baseQuota * (0.5 + priorityWeight);  // 0.5x to 1.5x base
```

---

## Daemon Structure

### File Organization

```
daemons/genome-daemon/
├── shared/
│   ├── GenomeDaemon.ts           # Base class (shared logic)
│   ├── GenomeTypes.ts            # Interfaces, types
│   └── AdapterRegistry.ts        # Adapter metadata management
├── server/
│   ├── GenomeDaemonServer.ts     # Server implementation
│   └── AdapterLoader.ts          # File I/O, .safetensors loading
└── browser/
    └── GenomeDaemonBrowser.ts    # Browser stub (no-op)
```

### Base Class (Shared)

```typescript
export abstract class GenomeDaemon extends BaseDaemon {
  protected personaGenomes: Map<UUID, PersonaGenomeState> = new Map();
  protected adapterRegistry: Map<string, AdapterMetadata> = new Map();
  protected resourceManager?: ResourceManager;

  // Abstract methods (implemented by server)
  protected abstract loadAdapterFromDisk(path: string): Promise<any>;
  protected abstract unloadAdapterFromMemory(adapterId: string): Promise<void>;

  // Shared logic
  async activateAdapter(personaId: UUID, skillName: string): Promise<ActivateResult> {
    // Check cache, quota, evict if needed, load adapter
  }

  async deactivateAdapter(personaId: UUID): Promise<void> {
    // Release adapter, update state
  }

  async getStats(): Promise<GenomeStats> {
    // Return global statistics
  }
}
```

### Server Implementation

```typescript
export class GenomeDaemonServer extends GenomeDaemon {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  protected async initialize(): Promise<void> {
    // Initialize ResourceManager
    this.resourceManager = ResourceManager.getInstance();

    // Scan for available adapters on disk
    await this.scanAdapterDirectory();

    console.log('🧬 GenomeDaemon: Initialized');
  }

  protected async loadAdapterFromDisk(path: string): Promise<any> {
    // Read .safetensors file
    // Parse LoRA weights
    // Load into GPU memory
    // Return adapter reference
  }

  protected async unloadAdapterFromMemory(adapterId: string): Promise<void> {
    // Free GPU memory
    // Keep file on disk
  }
}
```

---

## Commands Implementation

### Command Structure

```
commands/genome/
├── activate/
│   ├── shared/
│   │   └── GenomeActivateTypes.ts
│   └── server/
│       └── GenomeActivateCommand.ts
├── deactivate/
│   ├── shared/
│   │   └── GenomeDeactivateTypes.ts
│   └── server/
│       └── GenomeDeactivateCommand.ts
└── stats/
    ├── shared/
    │   └── GenomeStatsTypes.ts
    └── server/
        └── GenomeStatsCommand.ts
```

### Example: genome/activate

```typescript
// shared/GenomeActivateTypes.ts
export interface GenomeActivateParams extends CommandParams {
  personaId: UUID;
  skillName: string;
  priority?: number;
}

export interface GenomeActivateResult extends CommandResult {
  success: boolean;
  adapterPath: string;
  memoryUsedMB: number;
  cacheHit: boolean;
  evictedAdapters?: string[];
}

// server/GenomeActivateCommand.ts
export class GenomeActivateCommand extends BaseServerCommand {
  async execute(params: GenomeActivateParams): Promise<GenomeActivateResult> {
    const daemon = this.getDaemon<GenomeDaemonServer>('genome-daemon');
    return await daemon.activateAdapter(params.personaId, params.skillName);
  }
}
```

---

## PersonaUser Integration

### Lightweight Approach

PersonaUser doesn't manage adapters directly - just sends commands:

```typescript
class PersonaUser extends AIUser {
  // No genome field! Just use commands

  async activateSkillForTask(domain: string): Promise<void> {
    // Determine skill from domain
    const skillName = this.domainToSkill(domain);

    // Send async command to daemon
    const result = await Commands.execute<GenomeActivateParams, GenomeActivateResult>(
      'genome/activate',
      {
        personaId: this.id,
        skillName,
        priority: this.entity?.priority
      }
    );

    if (!result.success) {
      console.warn(`⚠️ ${this.displayName}: Failed to activate ${skillName}`);
    }

    // Continue processing - don't block on loading
  }

  private domainToSkill(domain: string): string {
    const mapping = {
      'chat': 'conversational',
      'code': 'typescript-expertise',
      'game': 'chess-strategy',
      'academy': 'teaching'
    };
    return mapping[domain] || 'conversational';
  }
}
```

### Integration Points

1. **Before processing task** - Activate appropriate skill
2. **After completing task** - Keep adapter active (for next similar task)
3. **On persona shutdown** - Deactivate adapter to free memory
4. **On priority change** - Update quota allocation

---

## Testing Strategy

### Unit Tests

```typescript
describe('GenomeDaemon', () => {
  it('should activate adapter and track memory', async () => {
    const result = await daemon.activateAdapter(persona1, 'typescript-expertise');
    expect(result.success).toBe(true);
    expect(result.memoryUsedMB).toBeGreaterThan(0);
  });

  it('should detect cache hit for already-loaded adapter', async () => {
    await daemon.activateAdapter(persona1, 'typescript-expertise');
    const result = await daemon.activateAdapter(persona1, 'typescript-expertise');
    expect(result.cacheHit).toBe(true);
  });

  it('should evict LRU adapter when memory full', async () => {
    // Fill memory with adapters
    await daemon.activateAdapter(persona1, 'adapter-a');
    await daemon.activateAdapter(persona2, 'adapter-b');
    await daemon.activateAdapter(persona3, 'adapter-c');

    // This should trigger eviction
    const result = await daemon.activateAdapter(persona4, 'adapter-d');
    expect(result.evictedAdapters).toContain('adapter-a');  // Least recently used
  });
});
```

### Multi-Persona Tests

```typescript
describe('GenomeDaemon - Multi-Persona', () => {
  it('should prevent thrashing across personas', async () => {
    // Simulate 10 personas competing for 3 adapter slots
    const personas = Array.from({ length: 10 }, (_, i) => `persona-${i}`);

    // Each persona requests different adapter rapidly
    for (let i = 0; i < 100; i++) {
      const persona = personas[i % 10];
      const skill = `skill-${i % 5}`;
      await daemon.activateAdapter(persona, skill);
    }

    const stats = await daemon.getStats();
    expect(stats.thrashingMetrics.thrashingDetected).toBe(false);
  });
});
```

### Integration Tests

```typescript
describe('PersonaUser + GenomeDaemon', () => {
  it('should activate skill before processing code task', async () => {
    const persona = new PersonaUser(...);

    // Process code task
    await persona.processTask({
      domain: 'code',
      content: 'Review this TypeScript function'
    });

    // Verify genome/activate was called
    const stats = await Commands.execute('genome/stats', {});
    const personaState = stats.personaStates.find(s => s.personaId === persona.id);
    expect(personaState?.activeAdapter).toBe('typescript-expertise');
  });
});
```

---

## Deployment & Monitoring

### Startup Sequence

1. Start GenomeDaemonServer
2. Initialize ResourceManager
3. Scan adapter directory for .safetensors files
4. Register with system daemon registry
5. Listen for genome/* commands

### Monitoring Metrics

```typescript
interface GenomeMetrics {
  // Memory
  totalGpuMemoryMB: number;
  usedGpuMemoryMB: number;
  memoryPressure: number;

  // Activity
  activePersonas: number;
  loadedAdapters: number;
  cacheHitRate: number;

  // Performance
  avgLoadTimeMs: number;
  avgEvictionTimeMs: number;

  // Health
  thrashingDetected: boolean;
  evictionsLastMinute: number;
  loadRequestsLastMinute: number;
}
```

### Logging

```typescript
// On activate
console.log(`🧬 GenomeDaemon: ${personaName} activated ${skillName} (${memoryUsedMB}MB, cache ${cacheHit ? 'HIT' : 'MISS'})`);

// On evict
console.log(`🧬 GenomeDaemon: Evicted ${adapterName} from ${personaName} (LRU, age=${ageSeconds}s)`);

// On thrashing
console.warn(`⚠️ GenomeDaemon: THRASHING DETECTED! ${evictionsPerMinute} evictions/min`);
```

---

## Future Extensions (Phase 8+)

### Adapter Stacking

```typescript
interface PersonaGenomeState {
  activeAdapters: string[];  // Multiple adapters (stack)
  compositionStrategy: 'stack' | 'parallel' | 'fusion';
}

await Commands.execute('genome/activate-stack', {
  personaId,
  skills: ['typescript-expertise', 'conversational', 'code-reviewer'],
  strategy: 'stack'
});
```

### Background Training

```typescript
await Commands.execute('genome/train', {
  personaId,
  skillName: 'typescript-expertise',
  trainingData: recentMistakes,
  epochs: 3
});

// Training happens in background
// Persona continues responding with old adapter
// Hot-swap to new adapter when training completes
```

### Predictive Loading

```typescript
// Preload adapters based on predicted next task
function predictNextSkill(personaState: PersonaGenomeState): string {
  // Analyze recent task pattern
  // Preload likely next adapter before it's requested
}
```

---

## References

- **S-LoRA Paper**: Serving Thousands of Concurrent LoRA Adapters (arxiv.org/abs/2311.03285)
- **NVIDIA NIM**: Seamlessly Deploying a Swarm of LoRA Adapters
- **AdapterHub**: Adapter Composition Documentation (docs.adapterhub.ml)
- **Multi LoRA**: Fine-Tuning LoRA Adapters via Pipeline Parallelism (arxiv.org/abs/2312.02515)
- **PEFT Library**: HuggingFace Parameter-Efficient Fine-Tuning

---

## Summary

**GenomeDaemon** is a centralized service that manages LoRA adapter lifecycle across all personas:

✅ **Global coordination** - No thrashing across personas
✅ **Fair allocation** - Resource quotas from ResourceManager
✅ **Simple start** - One adapter at a time (Phase 7)
✅ **Expandable** - Stack/parallel/fusion later (Phase 8+)
✅ **Non-blocking** - PersonaUser stays responsive
✅ **Observable** - Rich metrics and logging

**Next Steps**: Implement GenomeDaemonServer, test with mock adapters, integrate with PersonaUser
