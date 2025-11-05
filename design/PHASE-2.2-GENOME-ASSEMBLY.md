# Phase 2.2: Dynamic Genome Assembly - Design Document

**Status**: 🚧 IN PROGRESS
**Started**: 2025-10-11
**Estimated Completion**: 2-3 weeks
**Dependencies**: Phase 2.1 Complete ✅

---

## Executive Summary

Phase 2.2 implements **dynamic LoRA layer loading and stacking** to enable PersonaUsers to have customized, fine-tuned AI models. This is the core of the "genome" concept - each persona gets their own stack of LoRA adapters that modify a base model's behavior.

**Key Deliverables**:
- LoRA layer loading from disk/database
- LRU caching system for performance
- Layer composition/stacking algorithm
- Integration with ProcessPool workers

---

## LoRA Background Research

### What is LoRA?

**LoRA (Low-Rank Adaptation)** is a technique for fine-tuning large language models efficiently:
- Instead of retraining all model weights (billions of parameters)
- Train small "adapter" matrices (millions of parameters)
- Apply adapters on top of frozen base model
- Result: Customized behavior with 99% less storage

### File Formats

**HuggingFace Standard** (preferred):
```
adapter_model.safetensors  # PyTorch weights in safetensors format
adapter_config.json        # LoRA configuration (rank, alpha, target modules)
```

**Legacy PyTorch**:
```
adapter_model.bin          # PyTorch checkpoint
adapter_config.json        # Configuration
```

**Configuration Example**:
```json
{
  "r": 8,                   // Rank (lower = smaller, faster)
  "lora_alpha": 16,         // Scaling factor
  "target_modules": [       // Which layers to adapt
    "q_proj",
    "v_proj",
    "k_proj",
    "o_proj"
  ],
  "lora_dropout": 0.05,
  "bias": "none"
}
```

### Layer Stacking Strategy

**Multiple LoRA adapters can be combined**:

1. **Sequential Application** (simplest):
   ```
   Base Model → LoRA_1 → LoRA_2 → LoRA_3 → Output
   ```
   Each adapter modifies the output of the previous

2. **Weighted Merging** (recommended):
   ```
   Base Model + (LoRA_1 * weight_1) + (LoRA_2 * weight_2) + ...
   ```
   Adapters are merged into a single composite adapter

3. **Hierarchical Composition** (future):
   ```
   Base Model → Persona_Core (w=1.0) → Skill_Python (w=0.8) → Style_Friendly (w=0.5)
   ```
   Layers have semantic meaning and importance weights

---

## Architecture Design

### Component Hierarchy

```
GenomeAssembler (orchestrator)
├── LayerLoader (load from disk/DB)
├── LayerCache (LRU caching)
└── LayerComposer (merge multiple adapters)
```

### Data Flow

```
1. Request arrives: "Load genome XYZ"
2. GenomeAssembler queries database for genome entity
3. For each layer in genome.layers:
   a. Check LayerCache
   b. If miss: LayerLoader fetches from disk/DB
   c. LayerCache stores for reuse
4. LayerComposer merges all layers with weights
5. Composed adapter loaded into inference worker
6. Worker ready for inference
```

---

## File Specifications

### 1. LayerLoader.ts

**Purpose**: Load LoRA layer files from disk or database

**Location**: `system/genome/server/LayerLoader.ts`

**API**:
```typescript
class LayerLoader {
  /**
   * Load a LoRA layer by ID
   * @returns Layer data + metadata
   */
  async loadLayer(layerId: UUID): Promise<LoadedLayer>

  /**
   * Check if layer exists on disk
   */
  async layerExists(layerId: UUID): Promise<boolean>

  /**
   * Get layer metadata without loading weights
   */
  async getLayerMetadata(layerId: UUID): Promise<LayerMetadata>
}
```

**Key Features**:
- Load from `.continuum/genomes/layers/{layerId}/`
- Support both safetensors and PyTorch formats
- Validate layer integrity (checksum, config validation)
- Error handling for missing/corrupt layers

**Estimated Lines**: 150-200

---

### 2. LayerCache.ts

**Purpose**: LRU cache for loaded layers to avoid repeated disk I/O

**Location**: `system/genome/server/LayerCache.ts`

**API**:
```typescript
class LayerCache {
  /**
   * Get layer from cache, or null if miss
   */
  get(layerId: UUID): LoadedLayer | null

  /**
   * Store layer in cache
   */
  set(layerId: UUID, layer: LoadedLayer): void

  /**
   * Remove layer from cache
   */
  evict(layerId: UUID): void

  /**
   * Get cache statistics
   */
  getStats(): CacheStats
}
```

**Eviction Strategy**:
- **LRU (Least Recently Used)**: Evict oldest unused layers
- **Size-based**: Limit total cache size (e.g., 4GB)
- **Usage tracking**: Keep frequently-used layers longer

**Cache Structure**:
```typescript
{
  maxSize: 4 * 1024 * 1024 * 1024,  // 4GB
  entries: Map<UUID, CacheEntry>,
  accessOrder: LinkedList<UUID>,
  totalSize: number
}
```

**Estimated Lines**: 100-150

---

### 3. LayerComposer.ts

**Purpose**: Merge multiple LoRA layers into a single composite adapter

**Location**: `system/genome/server/LayerComposer.ts`

**API**:
```typescript
class LayerComposer {
  /**
   * Compose multiple layers with weights
   * @param layers Array of {layer, weight, ordering}
   * @returns Single merged layer
   */
  async compose(layers: WeightedLayer[]): Promise<ComposedLayer>

  /**
   * Validate layers are compatible
   */
  validateCompatibility(layers: LoadedLayer[]): boolean
}
```

**Composition Algorithm**:
```typescript
// Weighted linear combination
composedWeights = baseModel
for (layer of layers) {
  composedWeights += layer.weights * layer.weight
}
```

**Estimated Lines**: 100-150

---

### 4. GenomeAssembler.ts

**Purpose**: High-level orchestrator for genome loading

**Location**: `system/genome/server/GenomeAssembler.ts`

**API**:
```typescript
class GenomeAssembler {
  constructor(
    private loader: LayerLoader,
    private cache: LayerCache,
    private composer: LayerComposer
  )

  /**
   * Load and assemble a complete genome
   * @param genomeId UUID of genome entity
   * @returns Composed adapter ready for inference
   */
  async assembleGenome(genomeId: UUID): Promise<AssembledGenome>

  /**
   * Preload a genome into cache (warm-up)
   */
  async preloadGenome(genomeId: UUID): Promise<void>

  /**
   * Unload a genome from cache
   */
  async unloadGenome(genomeId: UUID): Promise<void>

  /**
   * Get assembly statistics
   */
  getStats(): AssemblyStats
}
```

**Assembly Process**:
```typescript
1. Query database for GenomeEntity by ID
2. Get ordered list of GenomeLayerEntity references
3. For each layer:
   - Check cache (hit: reuse, miss: load)
   - Validate layer compatibility
4. Compose all layers with weights
5. Return AssembledGenome ready for worker
```

**Estimated Lines**: 200-250

---

## Type Definitions

**New types in**: `system/genome/shared/GenomeAssemblyTypes.ts`

```typescript
/**
 * Raw LoRA layer loaded from disk
 */
export interface LoadedLayer {
  layerId: UUID;
  config: LoRAConfig;
  weights: Float32Array | Buffer;  // Actual adapter weights
  sizeBytes: number;
  format: 'safetensors' | 'pytorch';
  checksum: string;
  loadedAt: Timestamp;
}

/**
 * LoRA configuration from adapter_config.json
 */
export interface LoRAConfig {
  r: number;                    // Rank
  lora_alpha: number;           // Scaling factor
  target_modules: string[];     // Model layers to adapt
  lora_dropout: number;
  bias: 'none' | 'all';
}

/**
 * Layer with weight for composition
 */
export interface WeightedLayer {
  layer: LoadedLayer;
  weight: number;               // 0.0 - 1.0
  ordering: number;             // Stack order
}

/**
 * Composed genome ready for inference
 */
export interface AssembledGenome {
  genomeId: UUID;
  baseModelId: string;          // e.g., "meta-llama/Llama-3-8B"
  composedLayer: LoadedLayer;   // Merged adapter
  layerCount: number;
  totalSizeBytes: number;
  assemblyTimeMs: number;
  cacheHits: number;
  cacheMisses: number;
}

/**
 * Cache entry with metadata
 */
export interface CacheEntry {
  layerId: UUID;
  layer: LoadedLayer;
  lastAccessed: Timestamp;
  accessCount: number;
  sizeBytes: number;
}

/**
 * Cache statistics
 */
export interface CacheStats {
  entries: number;
  totalSizeBytes: number;
  hitRate: number;              // 0.0 - 1.0
  evictionCount: number;
}

/**
 * Assembly statistics
 */
export interface AssemblyStats {
  genomesAssembled: number;
  layersLoaded: number;
  cacheHitRate: number;
  avgAssemblyTimeMs: number;
}
```

**Estimated Lines**: 150-200

---

## Integration Points

### 1. inference-worker.ts

**Current placeholder** (lines 113-132):
```typescript
async function handleLoadGenome(message: any): Promise<void> {
  const { genomeId, layers } = message;

  // TODO Phase 2.2: Actual genome loading
  console.log(`🧬 [PLACEHOLDER] Loading genome: ${genomeId}`);

  loadedGenomeId = genomeId;

  // Simulate loading time
  await new Promise(resolve => setTimeout(resolve, 100));

  // Send loaded event
  process.send!({ type: 'loaded', genomeId });
}
```

**After Phase 2.2**:
```typescript
import { GenomeAssembler } from './GenomeAssembler';

const assembler = new GenomeAssembler(
  new LayerLoader(),
  new LayerCache(),
  new LayerComposer()
);

async function handleLoadGenome(message: any): Promise<void> {
  const { genomeId } = message;

  console.log(`🧬 Loading genome: ${genomeId}`);

  // Actually load and assemble the genome
  const assembled = await assembler.assembleGenome(genomeId);

  loadedGenomeId = genomeId;
  loadedGenome = assembled;  // Store for inference

  console.log(`✅ Genome loaded: ${assembled.layerCount} layers, ${assembled.totalSizeBytes} bytes`);

  process.send!({
    type: 'loaded',
    genomeId,
    layerCount: assembled.layerCount,
    sizeBytes: assembled.totalSizeBytes
  });
}
```

### 2. ProcessPool.ts

**Add genome context to ManagedProcess**:
```typescript
export interface ManagedProcess {
  // ... existing fields
  loadedGenomeId?: UUID;        // Currently loaded genome
  loadedAt?: Timestamp;         // When genome was loaded
}
```

### 3. genome/stats command

**Enhanced statistics**:
```typescript
{
  systemOverview: {
    totalGenomesLoaded: number,
    cachedLayers: number,
    cacheHitRate: number
  },
  assemblyStats: {
    avgLoadTimeMs: number,
    totalLayersLoaded: number
  }
}
```

---

## File System Structure

**Genome layer storage**:
```
.continuum/
└── genomes/
    └── layers/
        ├── {layer-uuid-1}/
        │   ├── adapter_model.safetensors
        │   ├── adapter_config.json
        │   └── metadata.json
        ├── {layer-uuid-2}/
        │   ├── adapter_model.safetensors
        │   ├── adapter_config.json
        │   └── metadata.json
        └── cache/
            └── stats.json
```

**Metadata format**:
```json
{
  "layerId": "uuid",
  "name": "Python Expert Layer",
  "description": "Specialized for Python code generation",
  "baseModel": "meta-llama/Llama-3-8B",
  "trainedOn": "2025-10-01",
  "sizeBytes": 16777216,
  "checksum": "sha256:abc123...",
  "format": "safetensors"
}
```

---

## Development Plan

### Week 1: Core Infrastructure

**Day 1-2: Type definitions + LayerLoader**
- Create GenomeAssemblyTypes.ts (all type definitions)
- Implement LayerLoader.ts (basic file loading)
- Test loading mock LoRA files

**Day 3-4: LayerCache**
- Implement LayerCache.ts (LRU eviction)
- Test cache hit/miss scenarios
- Benchmark cache performance

**Day 5: LayerComposer stub**
- Create LayerComposer.ts (placeholder merging)
- For now: just pass through single layer
- Real merging requires ML library research

### Week 2: Integration + Testing

**Day 6-7: GenomeAssembler**
- Implement GenomeAssembler.ts (orchestration)
- Wire together Loader + Cache + Composer
- Test end-to-end genome loading

**Day 8-9: Worker Integration**
- Update inference-worker.ts handleLoadGenome()
- Test with ProcessPool
- Verify IPC communication

**Day 10: Integration Tests**
- Create genome-assembly.test.ts
- Test cache eviction, layer loading, composition
- Performance benchmarks (< 500ms target)

### Week 3: Polish + Documentation

**Day 11-12: genome/stats enhancement**
- Add assembly statistics
- Show loaded genomes per process
- Display cache hit rates

**Day 13-14: Documentation + Cleanup**
- Update IMPLEMENTATION-STATUS.md
- Create GENOME-ASSEMBLY-GUIDE.md
- Code review and refactoring

---

## Success Criteria

Phase 2.2 complete when:

- ✅ Can load LoRA layer files from disk
- ✅ LRU cache reduces redundant disk I/O
- ✅ Multiple layers can be composed together
- ✅ inference-worker.ts successfully loads genomes
- ✅ genome/stats shows assembly metrics
- ✅ Integration tests pass (< 500ms load time)
- ✅ Documentation complete and accurate

---

## Future Enhancements (Phase 2.3+)

- **Smart Caching**: Keep layers for active personas loaded
- **Preloading**: Warm cache based on predicted usage
- **Layer Versioning**: Support multiple versions of same layer
- **Remote Layers**: Download layers from HuggingFace Hub
- **Layer Marketplace**: Share/download community layers

---

## Risk Mitigation

**Risk**: LoRA file format complexity
**Mitigation**: Start with simple JSON mock files, add real LoRA support incrementally

**Risk**: Composition algorithm unknown
**Mitigation**: Phase 2.2 focuses on loading/caching, Phase 2.3 tackles actual merging

**Risk**: Performance (> 500ms load time)
**Mitigation**: Aggressive caching + preloading for hot personas

**Risk**: Memory consumption
**Mitigation**: Strict cache size limits + eviction policies

---

**End of Design Document**
**Next**: Begin implementation with LayerLoader.ts
