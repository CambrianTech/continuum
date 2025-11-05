# GENOME SYSTEM COMMANDS SPECIFICATION

**Date**: 2025-10-07
**Status**: Command architecture for genome runtime

---

## 🎯 Command Design Principles

1. **User-Facing** - Commands users would actually invoke
2. **Self-Documenting** - Clear naming (genome/layer/create, not genome/cl)
3. **Composable** - Can be chained in recipes
4. **Testable** - Easy to write integration tests
5. **Type-Safe** - Strict TypeScript parameter/result types

---

## 📦 Command Hierarchy

```
commands/
├── genome/
│   ├── layer/
│   │   ├── create/          # genome/layer/create
│   │   ├── search/          # genome/layer/search
│   │   ├── refine/          # genome/layer/refine (Academy integration)
│   │   └── delete/          # genome/layer/delete
│   ├── assemble/            # genome/assemble
│   ├── mount/               # genome/mount (load genome into runtime)
│   └── unmount/             # genome/unmount
├── lora/
│   ├── load/                # lora/load (load adapter into process)
│   ├── unload/              # lora/unload (evict from cache)
│   └── status/              # lora/status (check cache state)
└── runtime/
    ├── spawn-process/       # runtime/spawn-process
    ├── kill-process/        # runtime/kill-process
    ├── execute-inference/   # runtime/execute-inference
    └── process-status/      # runtime/process-status
```

---

## 🧬 Genome Commands

### genome/layer/create

**Purpose**: Create new LoRA layer with metadata

**Parameters**:
```typescript
interface GenomeLayerCreateParams extends CommandParams {
  name: string;
  description: string;
  traitType: string;  // Free-entry (e.g., 'tone_and_voice')
  modelPath: string;  // Path to LoRA weights (.safetensors)
  sizeMB: number;
  rank: number;       // LoRA rank (8, 16, 32, 64)
  tags?: string[];
  source?: 'trained' | 'refined' | 'downloaded' | 'system';
}
```

**Result**:
```typescript
interface GenomeLayerCreateResult extends CommandResult {
  layerId: UUID;
  embedding: number[];  // 768-dim embedding (auto-generated)
  fitness: LayerFitness;
}
```

**Example**:
```bash
./jtag genome/layer/create \
  --name="Formal Tone Layer" \
  --traitType="tone_and_voice" \
  --modelPath="./genomes/bundled/formal-tone-r16.safetensors" \
  --sizeMB=25 \
  --rank=16 \
  --tags='["formal","professional"]'
```

---

### genome/layer/search

**Purpose**: Find similar layers via cosine similarity

**Parameters**:
```typescript
interface GenomeLayerSearchParams extends CommandParams {
  targetLayerId?: UUID;           // Search by existing layer
  targetEmbedding?: number[];     // Or search by raw embedding
  traitType?: string;             // Filter by trait
  minSimilarity?: number;         // Default 0.75
  maxResults?: number;            // Default 10
}
```

**Result**:
```typescript
interface GenomeLayerSearchResult extends CommandResult {
  results: Array<{
    layerId: UUID;
    name: string;
    similarity: number;  // 0-1
    recommendation: 'use-as-is' | 'refine' | 'fork' | 'train';
    traitType: string;
    sizeMB: number;
  }>;
}
```

**Example**:
```bash
./jtag genome/layer/search \
  --targetLayerId="layer-123" \
  --traitType="tone_and_voice" \
  --minSimilarity=0.85
```

---

### genome/layer/refine

**Purpose**: Trigger Academy refinement for existing layer

**Parameters**:
```typescript
interface GenomeLayerRefineParams extends CommandParams {
  layerId: UUID;
  trainingData: {
    curriculumId?: UUID;
    challenges: Array<{ prompt: string; expectedResponse: string }>;
  };
  epochs?: number;      // Default 3
  learningRate?: number;  // Default 1e-4
}
```

**Result**:
```typescript
interface GenomeLayerRefineResult extends CommandResult {
  refinedLayerId: UUID;  // New layer ID (v2)
  parentLayerId: UUID;   // Original layer ID
  trainingMetadata: {
    epochs: number;
    loss: number;
    performance: number;
    duration: number;
  };
}
```

**Example**:
```bash
./jtag genome/layer/refine \
  --layerId="layer-123" \
  --epochs=5 \
  --trainingData='{"challenges": [...]}'
```

---

### genome/assemble

**Purpose**: Assemble complete genome from layers

**Parameters**:
```typescript
interface GenomeAssembleParams extends CommandParams {
  personaId: UUID;
  name: string;
  description?: string;
  baseModel: string;  // e.g., "llama-3.1-8B"
  layers: Array<{
    layerId: UUID;
    orderIndex?: number;  // Auto-assigned if omitted
    weight?: number;      // Default 1.0
    enabled?: boolean;    // Default true
  }>;
  tags?: string[];
}
```

**Result**:
```typescript
interface GenomeAssembleResult extends CommandResult {
  genomeId: UUID;
  compositeEmbedding: number[];  // 768-dim average of all layers
  totalSizeMB: number;
  layerCount: number;
}
```

**Example**:
```bash
./jtag genome/assemble \
  --personaId="persona-456" \
  --name="BiomechExpert Genome" \
  --baseModel="llama-3.1-8B" \
  --layers='[
    {"layerId": "layer-tone", "weight": 1.0},
    {"layerId": "layer-domain", "weight": 1.0}
  ]'
```

---

## 🔧 LoRA Commands

### lora/load

**Purpose**: Load LoRA adapter into model process

**Parameters**:
```typescript
interface LoRALoadParams extends CommandParams {
  processId: UUID;
  layerId: UUID;
  priority?: number;  // For LRU cache decisions
}
```

**Result**:
```typescript
interface LoRALoadResult extends CommandResult {
  loaded: boolean;
  cacheHit: boolean;
  evictedLayerId?: UUID;  // If cache was full
  loadTime: number;       // Milliseconds
}
```

**Example**:
```bash
./jtag lora/load \
  --processId="process-789" \
  --layerId="layer-123" \
  --priority=10
```

---

### lora/unload

**Purpose**: Evict LoRA adapter from cache

**Parameters**:
```typescript
interface LoRAUnloadParams extends CommandParams {
  processId: UUID;
  layerId: UUID;
}
```

**Result**:
```typescript
interface LoRAUnloadResult extends CommandResult {
  unloaded: boolean;
  freedMemoryMB: number;
}
```

**Example**:
```bash
./jtag lora/unload \
  --processId="process-789" \
  --layerId="layer-123"
```

---

### lora/status

**Purpose**: Check cache state for process

**Parameters**:
```typescript
interface LoRAStatusParams extends CommandParams {
  processId: UUID;
}
```

**Result**:
```typescript
interface LoRAStatusResult extends CommandResult {
  processId: UUID;
  loadedAdapters: Array<{
    layerId: UUID;
    name: string;
    memoryMB: number;
    lastUsed: Date;
    usageCount: number;
  }>;
  totalMemoryMB: number;
  cacheUtilization: number;  // 0-1
}
```

**Example**:
```bash
./jtag lora/status --processId="process-789"
```

---

## ⚙️ Runtime Commands

### runtime/spawn-process

**Purpose**: Spawn new model process (worker/child process)

**Parameters**:
```typescript
interface RuntimeSpawnProcessParams extends CommandParams {
  baseModel: string;  // "llama-3.1-8B"
  processType?: 'worker' | 'child_process';  // Default 'worker'
  maxCacheSize?: number;  // Max LoRA adapters (default 5)
  memoryLimitMB?: number;  // Default 10000
}
```

**Result**:
```typescript
interface RuntimeSpawnProcessResult extends CommandResult {
  processId: UUID;
  pid?: number;  // OS process ID (if child_process)
  status: 'ready' | 'initializing';
  memoryMB: number;
}
```

**Example**:
```bash
./jtag runtime/spawn-process \
  --baseModel="llama-3.1-8B" \
  --maxCacheSize=10
```

---

### runtime/execute-inference

**Purpose**: Execute inference with persona genome

**Parameters**:
```typescript
interface RuntimeExecuteInferenceParams extends CommandParams {
  personaId: UUID;
  genomeId: UUID;
  prompt: string;
  priority?: 'immediate' | 'standard' | 'deferred';  // Default 'standard'
  maxTokens?: number;  // Default 2048
  temperature?: number;  // Default 0.7
}
```

**Result**:
```typescript
interface RuntimeExecuteInferenceResult extends CommandResult {
  requestId: UUID;
  response: string;
  latency: number;  // Milliseconds
  tokensGenerated: number;
  cacheHits: number;  // LoRA layers already loaded
  cacheMisses: number;  // Layers that needed loading
}
```

**Example**:
```bash
./jtag runtime/execute-inference \
  --personaId="persona-456" \
  --genomeId="genome-789" \
  --prompt="Explain biomechanical principles" \
  --priority="standard"
```

---

### runtime/process-status

**Purpose**: Get status of all model processes

**Parameters**:
```typescript
interface RuntimeProcessStatusParams extends CommandParams {
  processId?: UUID;  // Optional: filter by process
}
```

**Result**:
```typescript
interface RuntimeProcessStatusResult extends CommandResult {
  processes: Array<{
    processId: UUID;
    baseModel: string;
    status: 'ready' | 'busy' | 'crashed';
    queueDepth: number;
    totalRequests: number;
    avgLatency: number;
    memoryUsageMB: number;
    cpuUsage: number;
    loadedAdapters: number;
  }>;
}
```

**Example**:
```bash
./jtag runtime/process-status
```

---

## 🎯 Type-Safe Command Execution

**Commands.execute() Pattern** (from `system/core/shared/Commands.ts`):

```typescript
// ✅ CORRECT: Type-safe with explicit generics
const result = await Commands.execute<
  GenomeLayerCreateParams,
  GenomeLayerCreateResult
>('genome/layer/create', {
  name: 'test-layer',
  traitType: 'tone_and_voice',
  modelPath: '/test/layer.safetensors',
  sizeMB: 25,
  rank: 16
});

// result is typed as GenomeLayerCreateResult
expect(result.layerId).toBeDefined();  // TypeScript knows this exists
```

**Commands.execute() Signature**:
```typescript
// From Commands.ts - Two overloads for type safety
static execute<TCommand extends CommandName>(
  command: TCommand,
  params?: CommandInputFor<TCommand>
): Promise<CommandResultFor<TCommand>>;

static execute<T extends CommandParams, U extends CommandResult>(
  command: string,
  params?: Omit<T, 'context' | 'sessionId'>
): Promise<U>;
```

**Usage in Genome Commands**:
- Use `GENOME_COMMANDS.LAYER.CREATE` constant (no magic strings)
- Provide explicit type parameters for params and result
- Omit `context` and `sessionId` (auto-injected by Commands.execute)

---

## 🧪 Testing Commands

All commands should be testable via:

```bash
npm test -- tests/integration/genome-commands.test.ts
```

**Test Pattern**:
```typescript
import { GENOME_COMMANDS } from 'system/genome/shared/GenomeCommandConstants';
import type { GenomeLayerCreateParams, GenomeLayerCreateResult } from '../shared/GenomeTypes';

test('genome/layer/create - should create layer', async () => {
  const result = await runJtagCommand<
    GenomeLayerCreateParams,
    GenomeLayerCreateResult
  >(GENOME_COMMANDS.LAYER.CREATE, {
    name: 'test-layer',
    traitType: 'tone_and_voice',
    modelPath: '/test/layer.safetensors',
    sizeMB: 25,
    rank: 16
  });

  expect(result.success).toBe(true);
  expect(result.layerId).toBeDefined();

  // Clean up
  await runJtagCommand(GENOME_COMMANDS.LAYER.DELETE, {
    layerId: result.layerId
  });
});
```

---

## 📊 Command Implementation Priority

**Phase 1** (MVP - Core Operations):
1. ✅ `genome/layer/create`
2. ✅ `genome/assemble`
3. ✅ `runtime/spawn-process`
4. ✅ `lora/load`
5. ✅ `runtime/execute-inference`

**Phase 2** (Search & Optimization):
6. ✅ `genome/layer/search`
7. ✅ `lora/status`
8. ✅ `runtime/process-status`

**Phase 3** (Advanced Features):
9. ✅ `genome/layer/refine` (Academy integration)
10. ✅ `lora/unload` (cache management)

---

## ✅ Success Criteria

Each command must:
1. ✅ Have TypeScript types (Params + Result)
2. ✅ Follow shared/browser/server pattern
3. ✅ Include README.md with examples
4. ✅ Have integration tests
5. ✅ Update database entities
6. ✅ Emit appropriate events
7. ✅ Handle errors gracefully
8. ✅ Work via CLI (`./jtag command`)

---

**Status**: Command specification complete, ready for implementation
**Next**: Implement Phase 1 commands + tests

---

*"Commands as the API for PersonaOS"* 🎯✨
