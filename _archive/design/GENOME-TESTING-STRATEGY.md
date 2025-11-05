# GENOME SYSTEM TESTING STRATEGY

**Date**: 2025-10-07
**Status**: Testing architecture for genome runtime implementation

---

## 🎯 Testing Philosophy

**Rust-Like Quality Standards**:
- ✅ Every entity CRUD operation tested
- ✅ Every command input/output verified
- ✅ Database persistence confirmed
- ✅ Widget/UI synchronization validated
- ✅ Integration flows end-to-end tested
- ✅ No mocking - test against real running system
- ✅ Repeatable, deterministic, fast

**Pattern**: Follow `tests/integration/crud-db-widget.test.ts` quality level

---

## 📋 Test Coverage Matrix

### Phase 1: Genome Entity CRUD Tests

**Test File**: `tests/integration/genome-crud.test.ts`

| Entity | CREATE | READ | UPDATE | DELETE | DB Persist | Cleanup |
|--------|--------|------|--------|--------|------------|---------|
| GenomeLayerEntity | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| GenomeEntity | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

**Test Structure** (per entity):
```typescript
describe('GenomeLayerEntity CRUD', () => {
  let testLayerIds: UUID[] = [];

  afterEach(async () => {
    // Clean up test data
    for (const id of testLayerIds) {
      await runJtagCommand('data/delete', {
        collection: 'genome_layers',
        id
      });
    }
    testLayerIds = [];
  });

  test('CREATE - should create genome layer with all fields', async () => {
    const layerData = {
      name: 'test-tone-layer',
      description: 'Test layer for tone adjustment',
      traitType: 'tone_and_voice',
      source: 'trained',
      modelPath: '/test/layers/tone.safetensors',
      sizeMB: 25,
      rank: 16,
      embedding: Array(768).fill(0.1),
      tags: ['test', 'tone'],
      fitness: {
        accuracy: 0,
        efficiency: 0,
        usageCount: 0,
        successRate: 0,
        averageLatency: 0,
        cacheHitRate: 0
      },
      generation: 0
    };

    const result = await runJtagCommand('data/create', {
      collection: 'genome_layers',
      data: layerData
    });

    expect(result.success).toBe(true);
    expect(result.id).toBeDefined();
    testLayerIds.push(result.id);

    // Verify database persistence
    const readResult = await runJtagCommand('data/read', {
      collection: 'genome_layers',
      id: result.id
    });

    expect(readResult.success).toBe(true);
    expect(readResult.entity.name).toBe('test-tone-layer');
    expect(readResult.entity.traitType).toBe('tone_and_voice');
    expect(readResult.entity.rank).toBe(16);
    expect(readResult.entity.embedding.length).toBe(768);
  });

  test('UPDATE - should update layer and increment version', async () => {
    // Create layer
    const createResult = await runJtagCommand('data/create', {
      collection: 'genome_layers',
      data: { /* ... */ }
    });
    testLayerIds.push(createResult.id);

    // Update layer
    const updateResult = await runJtagCommand('data/update', {
      collection: 'genome_layers',
      id: createResult.id,
      data: { sizeMB: 30 }
    });

    expect(updateResult.success).toBe(true);
    expect(updateResult.version).toBe(2);  // v1 → v2

    // Verify persistence
    const readResult = await runJtagCommand('data/read', {
      collection: 'genome_layers',
      id: createResult.id
    });

    expect(readResult.entity.sizeMB).toBe(30);
    expect(readResult.entity.version).toBe(2);
  });

  test('DELETE - should remove layer from database', async () => {
    const createResult = await runJtagCommand('data/create', {
      collection: 'genome_layers',
      data: { /* ... */ }
    });

    const deleteResult = await runJtagCommand('data/delete', {
      collection: 'genome_layers',
      id: createResult.id
    });

    expect(deleteResult.success).toBe(true);

    // Verify deletion
    const readResult = await runJtagCommand('data/read', {
      collection: 'genome_layers',
      id: createResult.id
    });

    expect(readResult.success).toBe(false);
  });
});
```

---

### Phase 2: Genome Command Tests (Real User Commands)

**Test File**: `tests/integration/genome-commands.test.ts`

**Commands to Test**:
1. `genome/layer/create` - Create new LoRA layer
2. `genome/layer/search` - Find similar layers via cosine similarity
3. `genome/assemble` - Assemble genome from layers
4. `genome/layer/refine` - Trigger Academy refinement
5. `lora/load` - Load LoRA adapter into process
6. `lora/unload` - Evict LoRA adapter from cache

**Tests**:

```typescript
test('genome/layer/create - should create LoRA layer with embedding', async () => {
  const result = await runJtagCommand('genome/layer/create', {
    name: 'test-tone-layer',
    description: 'Test layer for tone adjustment',
    traitType: 'tone_and_voice',
    modelPath: '/test/layers/tone.safetensors',
    sizeMB: 25,
    rank: 16,
    tags: ['test', 'tone']
  });

  expect(result.success).toBe(true);
  expect(result.layerId).toBeDefined();
  testLayerIds.push(result.layerId);

  // Verify it was persisted
  const layers = await runJtagCommand('data/list', {
    collection: 'genome_layers',
    filter: { id: result.layerId }
  });

  expect(layers.items.length).toBe(1);
  expect(layers.items[0].name).toBe('test-tone-layer');
  expect(layers.items[0].embedding.length).toBe(768);
});

test('genome/layer/search - should find similar layers by trait', async () => {
  // Create target layer
  const target = await runJtagCommand('genome/layer/create', {
    name: 'target-layer',
    traitType: 'tone_and_voice',
    modelPath: '/test/target.safetensors',
    sizeMB: 25,
    rank: 16,
    tags: ['formal', 'professional']
  });
  testLayerIds.push(target.layerId);

  // Create similar layer
  const similar = await runJtagCommand('genome/layer/create', {
    name: 'similar-layer',
    traitType: 'tone_and_voice',
    modelPath: '/test/similar.safetensors',
    sizeMB: 25,
    rank: 16,
    tags: ['formal', 'business']
  });
  testLayerIds.push(similar.layerId);

  // Search for similar layers
  const searchResult = await runJtagCommand('genome/layer/search', {
    targetLayerId: target.layerId,
    traitType: 'tone_and_voice',
    minSimilarity: 0.75
  });

  expect(searchResult.success).toBe(true);
  expect(searchResult.results.length).toBeGreaterThan(0);
  expect(searchResult.results[0]).toMatchObject({
    layerId: expect.any(String),
    similarity: expect.any(Number),
    recommendation: expect.stringMatching(/use-as-is|refine|fork|train/)
  });
});

test('genome/assemble - should create genome from layers', async () => {
  const persona = await createTestPersona();

  // Create two layers
  const layer1 = await runJtagCommand('genome/layer/create', {
    name: 'tone-layer',
    traitType: 'tone_and_voice',
    modelPath: '/test/tone.safetensors',
    sizeMB: 25,
    rank: 16
  });
  testLayerIds.push(layer1.layerId);

  const layer2 = await runJtagCommand('genome/layer/create', {
    name: 'domain-layer',
    traitType: 'domain_expertise',
    modelPath: '/test/domain.safetensors',
    sizeMB: 30,
    rank: 16
  });
  testLayerIds.push(layer2.layerId);

  // Assemble genome
  const genome = await runJtagCommand('genome/assemble', {
    personaId: persona.id,
    name: 'BiomechExpert Genome',
    baseModel: 'llama-3.1-8B',
    layers: [
      { layerId: layer1.layerId, weight: 1.0, enabled: true },
      { layerId: layer2.layerId, weight: 1.0, enabled: true }
    ]
  });

  expect(genome.success).toBe(true);
  expect(genome.genomeId).toBeDefined();
  testGenomeIds.push(genome.genomeId);

  // Verify genome was persisted with correct layer order
  const genomeData = await runJtagCommand('data/read', {
    collection: 'genomes',
    id: genome.genomeId
  });

  expect(genomeData.entity.layers.length).toBe(2);
  expect(genomeData.entity.layers[0].layerId).toBe(layer1.layerId);
  expect(genomeData.entity.compositeEmbedding.length).toBe(768);
});

test('lora/load - should load adapter into model process', async () => {
  // Create process
  const process = await runJtagCommand('runtime/spawn-process', {
    baseModel: 'llama-3.1-8B'
  });
  testProcessIds.push(process.processId);

  // Create layer
  const layer = await runJtagCommand('genome/layer/create', {
    name: 'test-adapter',
    traitType: 'tone_and_voice',
    modelPath: '/test/adapter.safetensors',
    sizeMB: 25,
    rank: 16
  });
  testLayerIds.push(layer.layerId);

  // Load adapter
  const loadResult = await runJtagCommand('lora/load', {
    processId: process.processId,
    layerId: layer.layerId
  });

  expect(loadResult.success).toBe(true);
  expect(loadResult.loaded).toBe(true);

  // Verify adapter is in cache
  const cacheEntries = await runJtagCommand('data/list', {
    collection: 'adapter_cache',
    filter: {
      modelProcessId: process.processId,
      layerId: layer.layerId,
      status: 'loaded'
    }
  });

  expect(cacheEntries.items.length).toBe(1);
  expect(cacheEntries.items[0].memoryMB).toBe(25);
});

test('lora/unload - should evict adapter from cache', async () => {
  const process = await createTestProcess();
  const layer = await createTestLayer();

  // Load adapter
  await runJtagCommand('lora/load', {
    processId: process.processId,
    layerId: layer.layerId
  });

  // Unload adapter
  const unloadResult = await runJtagCommand('lora/unload', {
    processId: process.processId,
    layerId: layer.layerId
  });

  expect(unloadResult.success).toBe(true);

  // Verify adapter is evicted
  const cacheEntries = await runJtagCommand('data/list', {
    collection: 'adapter_cache',
    filter: {
      modelProcessId: process.processId,
      layerId: layer.layerId,
      status: 'loaded'
    }
  });

  expect(cacheEntries.items.length).toBe(0);
});
```

---

### Phase 3: Runtime Process Management Tests

**Test File**: `tests/integration/genome-runtime-processes.test.ts`

**Tests**:
1. **ModelProcessEntity Lifecycle**
   - Create process entity
   - Update status (ready → busy → ready)
   - Track metrics (queueDepth, latency)
   - Verify crash detection
   - Test restart logic

2. **AdapterCacheEntity LRU**
   - Load 5 adapters into cache
   - Access adapters (track lastUsed)
   - Load 6th adapter (triggers eviction)
   - Verify LRU eviction (oldest adapter removed)

```typescript
test('ModelProcessEntity - should track process lifecycle', async () => {
  // Create process
  const process = await runJtagCommand('data/create', {
    collection: 'model_processes',
    data: {
      baseModel: 'llama-3.1-8B',
      processType: 'worker',
      status: 'ready',
      loadedAdapters: [],
      queueDepth: 0,
      totalRequests: 0,
      avgLatency: 0,
      memoryUsageMB: 8192,
      cpuUsage: 0,
      lastHealthCheck: new Date()
    }
  });

  testProcessIds.push(process.id);

  // Update to busy
  await runJtagCommand('data/update', {
    collection: 'model_processes',
    id: process.id,
    data: { status: 'busy', queueDepth: 1 }
  });

  // Verify persistence
  const readResult = await runJtagCommand('data/read', {
    collection: 'model_processes',
    id: process.id
  });

  expect(readResult.entity.status).toBe('busy');
  expect(readResult.entity.queueDepth).toBe(1);
});

test('AdapterCacheEntity - should implement LRU eviction', async () => {
  const maxCacheSize = 5;
  const processId = await createTestProcess();
  const layerIds = await createTestLayers(6);

  // Load 5 adapters
  for (let i = 0; i < 5; i++) {
    await runJtagCommand('data/create', {
      collection: 'adapter_cache',
      data: {
        layerId: layerIds[i],
        modelProcessId: processId,
        loadedAt: new Date(),
        lastUsed: new Date(),
        usageCount: 0,
        memoryMB: 25,
        status: 'loaded',
        priority: i
      }
    });
  }

  // Load 6th adapter (should trigger eviction)
  await runJtagCommand('runtime/load-adapter', {
    processId,
    layerId: layerIds[5],
    maxCacheSize
  });

  // Verify oldest adapter (layerIds[0]) was evicted
  const cacheEntries = await runJtagCommand('data/list', {
    collection: 'adapter_cache',
    filter: { modelProcessId: processId, status: 'loaded' }
  });

  expect(cacheEntries.items.length).toBe(maxCacheSize);
  expect(cacheEntries.items.find(e => e.layerId === layerIds[0])).toBeUndefined();
  expect(cacheEntries.items.find(e => e.layerId === layerIds[5])).toBeDefined();
});
```

---

### Phase 4: Inference Queue Tests

**Test File**: `tests/integration/genome-inference-queue.test.ts`

**Tests**:
1. **Queue Priority**
   - Create 3 requests (immediate, standard, deferred)
   - Verify execution order
   - Confirm priority scheduling

2. **Process Assignment**
   - Create 2 model processes
   - Queue 5 requests
   - Verify load balancing
   - Test cache-aware routing

```typescript
test('InferenceRequestEntity - should respect priority order', async () => {
  const personaId = await createTestPersona();
  const genomeId = await createTestGenome();

  // Create requests with different priorities
  const deferred = await runJtagCommand('data/create', {
    collection: 'inference_requests',
    data: {
      personaId,
      genomeId,
      requiredLayers: [],
      prompt: 'Deferred task',
      priority: 'deferred',
      status: 'queued',
      queuedAt: new Date()
    }
  });

  const standard = await runJtagCommand('data/create', {
    collection: 'inference_requests',
    data: {
      personaId,
      genomeId,
      requiredLayers: [],
      prompt: 'Standard task',
      priority: 'standard',
      status: 'queued',
      queuedAt: new Date()
    }
  });

  const immediate = await runJtagCommand('data/create', {
    collection: 'inference_requests',
    data: {
      personaId,
      genomeId,
      requiredLayers: [],
      prompt: 'Immediate task',
      priority: 'immediate',
      status: 'queued',
      queuedAt: new Date()
    }
  });

  // Get queue (should be sorted by priority)
  const queue = await runJtagCommand('data/list', {
    collection: 'inference_requests',
    filter: { status: 'queued' },
    orderBy: [{ field: 'priority', direction: 'desc' }]
  });

  expect(queue.items[0].id).toBe(immediate.id);
  expect(queue.items[1].id).toBe(standard.id);
  expect(queue.items[2].id).toBe(deferred.id);
});
```

---

### Phase 5: Integration Tests (End-to-End)

**Test File**: `tests/integration/genome-complete-workflow.test.ts`

**Tests**:
1. **Persona Creation with Genome Assembly**
   - Create PersonaUser
   - Create 3 genome layers
   - Assemble genome (link layers)
   - Verify database relationships

2. **Inference Execution Flow**
   - Create persona + genome
   - Spawn model process
   - Queue inference request
   - Load required LoRA layers
   - Execute inference
   - Update fitness metrics
   - Verify all database updates

```typescript
test('Complete workflow - persona creation to inference', async () => {
  // 1. Create PersonaUser
  const persona = await runJtagCommand('data/create', {
    collection: 'users',
    data: {
      displayName: 'BiomechExpert',
      userType: 'persona'
    }
  });

  // 2. Create genome layers
  const toneLayer = await createTestLayer({ traitType: 'tone_and_voice' });
  const domainLayer = await createTestLayer({ traitType: 'domain_expertise' });

  // 3. Assemble genome
  const genome = await runJtagCommand('genome/assemble', {
    personaId: persona.id,
    name: 'BiomechExpert Genome',
    baseModel: 'llama-3.1-8B',
    layerIds: [toneLayer.id, domainLayer.id]
  });

  expect(genome.success).toBe(true);

  // 4. Spawn model process
  const process = await runJtagCommand('runtime/spawn-process', {
    baseModel: 'llama-3.1-8B'
  });

  expect(process.success).toBe(true);

  // 5. Queue inference request
  const request = await runJtagCommand('runtime/execute-inference', {
    personaId: persona.id,
    genomeId: genome.id,
    prompt: 'Explain biomechanical principles'
  });

  expect(request.success).toBe(true);
  expect(request.status).toBe('completed');

  // 6. Verify fitness updates
  const updatedLayer = await runJtagCommand('data/read', {
    collection: 'genome_layers',
    id: toneLayer.id
  });

  expect(updatedLayer.entity.fitness.usageCount).toBe(1);
  expect(updatedLayer.entity.fitness.successRate).toBeGreaterThan(0);
});
```

---

## 🏗️ Test Infrastructure

### Test Utilities

**Create**: `tests/test-utils/GenomeTestUtils.ts`

```typescript
import { runJtagCommand } from './CRUDTestUtils';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

export async function createTestLayer(overrides: Partial<GenomeLayerData> = {}) {
  const defaultData = {
    name: `test-layer-${Date.now()}`,
    description: 'Test layer',
    traitType: 'test',
    source: 'trained',
    modelPath: '/test/layers/test.safetensors',
    sizeMB: 25,
    rank: 16,
    embedding: Array(768).fill(0.5),
    tags: ['test'],
    fitness: {
      accuracy: 0,
      efficiency: 0,
      usageCount: 0,
      successRate: 0,
      averageLatency: 0,
      cacheHitRate: 0
    },
    generation: 0
  };

  const result = await runJtagCommand('data/create', {
    collection: 'genome_layers',
    data: { ...defaultData, ...overrides }
  });

  return result;
}

export async function createTestGenome(personaId: UUID, layerIds: UUID[]) {
  const result = await runJtagCommand('genome/assemble', {
    personaId,
    name: `test-genome-${Date.now()}`,
    baseModel: 'llama-3.1-8B',
    layerIds
  });

  return result;
}

export async function createTestProcess() {
  const result = await runJtagCommand('runtime/spawn-process', {
    baseModel: 'llama-3.1-8B'
  });

  return result.processId;
}

export function generateEmbedding(similarity: number, baseEmbedding: number[]): number[] {
  // Generate embedding with specified similarity to base
  const dimension = baseEmbedding.length;
  const matchCount = Math.floor(dimension * similarity);

  return baseEmbedding.map((v, i) =>
    i < matchCount ? v : Math.random()
  );
}
```

---

## 📊 Test Execution

### Run All Genome Tests
```bash
npm test -- --grep "genome"
```

### Run Specific Test Suites
```bash
npm test -- tests/integration/genome-crud.test.ts
npm test -- tests/integration/genome-similarity.test.ts
npm test -- tests/integration/genome-runtime-processes.test.ts
npm test -- tests/integration/genome-inference-queue.test.ts
npm test -- tests/integration/genome-complete-workflow.test.ts
```

### Pre-Commit Validation
```bash
npm run test:genome  # All genome tests must pass
```

---

## ✅ Success Criteria

**Every test must**:
1. ✅ Create test data with unique identifiers
2. ✅ Verify database persistence via `data/read`
3. ✅ Clean up all test data in `afterEach`
4. ✅ Use proper TypeScript types (no `any`)
5. ✅ Include descriptive console output
6. ✅ Run reliably in CI/CD (no flakiness)
7. ✅ Complete in <30 seconds per test
8. ✅ Use real running system (no mocks)

---

## 🚀 Implementation Order

1. **Week 1**: Genome entity CRUD tests ✅
2. **Week 2**: Similarity calculation tests
3. **Week 3**: Runtime process management tests
4. **Week 4**: Inference queue tests
5. **Week 5**: End-to-end integration tests

**All tests must pass before merging any genome runtime code!**

---

**Status**: Testing strategy complete, ready for implementation
**Next**: Create Phase 2 entities + corresponding tests

---

*"Test-driven development for PersonaOS - no compromises."* 🧪✨
