# Provider Genome Capabilities Summary

**Status**: Implemented (2025-11-15)
**Location**: `FineTuningCapabilities.compositionMethods` field

---

## Overview

Each fine-tuning provider declares its genome capabilities via `getFineTuningCapabilities()`. This tells PersonaGenome what the provider can do with LoRA adapters.

---

## Composition Methods

The `compositionMethods` array specifies which techniques a provider supports for combining multiple LoRA layers:

### **'stack'** - Sequential Loading
- Simplest method: Load adapters one after another
- PEFT applies them in order during inference
- Fast, no computation overhead
- **Use case**: Domain + personality layers

```python
# PEFT code (already implemented in peft_composition.py)
composer.load_adapter("./adapters/wine-expertise", "wine")
composer.load_adapter("./adapters/vin-diesel-style", "personality")
composer.set_adapter(["wine", "personality"])  # Stack them
```

### **'weighted'** - Blended Merge
- Combine adapters with weights (e.g., 0.7 wine + 0.3 personality)
- Creates smooth blend of behaviors
- Slightly more computation than stack
- **Use case**: Fine-tuning the balance between domains

```python
# PEFT weighted composition
composer.add_weighted_adapter(
    adapters=["wine", "personality"],
    weights=[0.7, 0.3],
    adapter_name="wine-personality-blend"
)
```

### **'TIES'** - Conflict Resolution
- **T**rim, **I**nspect, **E**lect, **S**ign
- Resolves parameter conflicts when merging
- Advanced algorithm for clean merges
- **Use case**: Merging adapters trained on similar domains

```python
# TIES merging (via PEFT or separate library)
merged = ties_merge(
    adapters=["wine-v1", "wine-v2", "wine-v3"],
    weights=[0.4, 0.3, 0.3]
)
```

### **'DARE'** - Pruning Technique
- **D**rop **A**nd **RE**scale
- Prunes low-magnitude parameters during merge
- Creates smaller, faster adapters
- **Use case**: Optimizing for inference speed

```python
# DARE pruning during merge
merged = dare_merge(
    adapters=["large-adapter-1", "large-adapter-2"],
    drop_rate=0.1  # Drop 10% of parameters
)
```

---

## Provider Capabilities Matrix

| Provider | maxActiveLayers | supportsDownload | supportsLocalComposition | compositionMethods | Primary Use Case |
|----------|----------------|------------------|------------------------|-------------------|------------------|
| **OpenAI** | 1 | ❌ false | ❌ false | `[]` | API inference only |
| **Fireworks** | 1 | ✅ true | ❌ false | `[]` | API inference (downloadable for backup/portability) |
| **Together** | 1 | ✅ true | ❌ false | `[]` | API inference (downloadable for backup/portability) |
| **DeepSeek** | 1 | ✅ true | ❌ false | `[]` | API inference (downloadable for backup/portability) |
| **PEFT (local)** | 16 | ✅ true | ✅ true | `['stack', 'weighted', 'TIES', 'DARE']` | Local inference (requires significant compute) |

---

## Three-Tier Deployment Strategy

### Tier 1: Demo Mode (M1 MacBook - Local Tiny Models)

**Target Users:** Students, broke users, demo/trial, education

**Goal:** Demonstrate REAL multi-layer genome locally with minimal hardware

**✅ WORKS ON M1 (16GB unified memory):**
- **Tiny local models**: Llama 3.2 1B via Ollama (4-bit quantized = ~1.5GB)
- **Multi-layer genome**: 2-3 adapters simultaneously! (~100MB each)
- **True composition**: Wine + personality layers active at once
- **Memory budget**: Base (1.5GB) + 3 adapters (0.3GB) + context (1GB) = ~3GB total
- **LRU paging**: When hitting memory limits (4th+ adapter)
- **RAG-only PersonaUsers**: No fine-tuning, just retrieval (zero cost alternative)

**Example - TRUE MULTI-LAYER:**
```typescript
// Llama 3.2 1B with MULTIPLE adapters simultaneously!
const tinyGenome = new PersonaGenome({
  provider: 'ollama-peft',  // Local Ollama + PEFT
  baseModel: 'llama3.2:1b-q4',  // 4-bit quantized
  maxActiveLayers: 3,        // M1 can handle 2-3!
  availableLayers: [
    'wine-expertise-tiny',   // ~100MB adapter
    'vin-diesel-style-tiny', // ~100MB adapter
    'helpful-assistant-tiny', // ~100MB adapter
    'typescript-expert-tiny' // ~100MB adapter (4th requires paging)
  ]
});

// REAL COMPOSITION: Load wine + personality simultaneously
await tinyGenome.setLayers([
  { name: 'wine-expertise-tiny', weight: 0.7 },
  { name: 'vin-diesel-style-tiny', weight: 0.3 }
], 'weighted');  // Both adapters active at once!

const response1 = await tinyGenome.generate("Tell me about Cabernet");
// Response has BOTH wine knowledge AND Vin Diesel personality!

// Switch to different composition
await tinyGenome.setLayers([
  { name: 'typescript-expert-tiny', weight: 0.8 },
  { name: 'helpful-assistant-tiny', weight: 0.2 }
]);
const response2 = await tinyGenome.generate("Review my code");
// Now it's a helpful TypeScript expert

// LRU paging only needed if we want 4+ adapters
// With 3 max, we can keep frequently-used ones loaded
```

**What this proves:**
- ✅ GenomeLayerEntity storage and retrieval
- ✅ **TRUE multi-layer composition** (wine + personality simultaneously)
- ✅ Dynamic layer selection and composition per task
- ✅ PersonaGenome architecture works with REAL benefits
- ✅ Modular training strategy (train layers separately, compose freely)
- ✅ LRU eviction when exceeding memory budget (optional demo with 4+ adapters)

### Tier 2: Cloud API Mode (M1 MacBook - Remote Inference)

**Target Users:** Production users, startups, scale-up phase

**Goal:** Full capability without hardware investment

**✅ WORKS ON M1:**
- **Cloud training**: Fireworks/Together/DeepSeek train on their GPUs
- **Cloud inference**: API calls route to their servers
- **Single-layer per provider**: Train modular, use one at a time via API
- **Multi-layer via composition**: Sequential API calls or pre-merged composites
- **Zero local GPU needed**: M1 just sends HTTP requests

**Example:**
```typescript
// Train modular layers on Fireworks (cloud GPUs)
const fireworks = new FireworksLoRAAdapter();
await fireworks.trainLoRA({
  traitType: 'wine-expertise',
  dataset: wineData,
  baseModel: 'llama-v3p1-8b-instruct'
});
// Result: modelId = "accounts/joel/models/wine-expert"

// PersonaGenome routes to Fireworks API
const cloudGenome = new PersonaGenome({
  provider: 'fireworks',
  baseModel: 'llama-v3p1-8b-instruct',
  maxActiveLayers: 1,              // Fireworks constraint
  availableLayers: [
    { name: 'wine-expertise', remoteId: 'accounts/joel/models/wine-expert' },
    { name: 'vin-diesel-style', remoteId: 'accounts/joel/models/vin-diesel' }
  ]
});

// Inference via Fireworks API (runs on their servers)
await cloudGenome.setLayers([{ name: 'wine-expertise', weight: 1.0 }]);
const response = await cloudGenome.generate("Tell me about wine");
// M1 sends HTTP request, Fireworks runs Llama 8B with adapter
```

**What this enables:**
- ✅ Production-quality models (Llama 8B/70B)
- ✅ Modular training (N domains + M personalities = N×M personas)
- ✅ Pay-per-use pricing (no upfront GPU cost)
- ✅ Infinite scale via API
- ✅ Works on ANY hardware (just needs internet)

### Tier 3: Local Multi-Layer (High-End GPU - Future)

**Target Users:** Advanced users, enterprises, privacy-focused

**Goal:** Full PEFT composition power, no API dependency

**✅ REQUIRES GPU:**
- 24GB+ VRAM GPU (RTX 3090/4090) for 8B models
- 80GB+ VRAM (A100) for 70B models
- NVMe storage for adapter caching

**✅ CAPABILITIES:**
- **Multi-layer composition**: wine + personality simultaneously
- **Advanced merging**: TIES, DARE algorithms
- **Unlimited layers**: maxActiveLayers=16 (or more)
- **Zero API cost**: All inference local
- **Full privacy**: No data leaves machine

**Example:**
```typescript
// Local PEFT with multi-layer composition (requires GPU)
const peftGenome = new PersonaGenome({
  provider: 'peft',
  baseModel: 'meta-llama/Llama-3.1-8B',
  maxActiveLayers: 16,               // GPU memory allows this
  availableLayers: [
    'wine-expertise',
    'vin-diesel-style',
    'typescript-expert',
    'helpful-assistant'
  ]
});

// Load MULTIPLE adapters simultaneously
await peftGenome.setLayers([
  { name: 'wine-expertise', weight: 0.7 },
  { name: 'vin-diesel-style', weight: 0.3 }
], 'weighted');  // Blend them

// Inference runs locally with both adapters active
const response = await peftGenome.generate("Tell me about Cabernet");
// No API call, no latency, full composition power
```

---

## Implementation Priority

### Phase 1: Tier 2 (Cloud APIs) - CURRENT FOCUS
**Why first:** Fastest path to production capability, works on M1 today

**Status:**
- ✅ Type system with genome capabilities
- ✅ OpenAI/Fireworks/Together/DeepSeek adapters declare capabilities
- ✅ Training jobs working (3/4 providers)
- 📋 TODO: PersonaGenome class with provider routing
- 📋 TODO: GenomeLayerEntity storage
- 📋 TODO: Cloud inference integration

**Outcome:** Production-ready genome with Llama 8B/70B via APIs

### Phase 2: Tier 1 (Demo Mode) - NEXT
**Why second:** Proves genome concepts work locally, great for demos/education

**TODO:**
- 📋 Ollama + PEFT integration for Llama 3.2 1B
- 📋 Single-layer paging (load → use → evict → load next)
- 📋 LRU eviction under memory constraints
- 📋 Train tiny adapters (~100MB each) for demonstration
- 📋 Demo PersonaUser that swaps layers per task

**Outcome:** Genome demonstration on any M1 MacBook, zero API cost

### Phase 3: Tier 3 (Local Multi-Layer) - FUTURE
**Why last:** Requires GPU hardware investment, smaller user base

**TODO:**
- 📋 Full PEFT integration with multi-layer composition
- 📋 TIES/DARE merging algorithms
- 📋 GPU memory management for 8B+ models
- 📋 Performance optimization for real-time inference

**Outcome:** Full genome power for advanced users with GPUs

---

## Usage Pattern

### Remote API Providers (OpenAI, Fireworks, Together, DeepSeek)

```typescript
// 1. Train on cloud
const openaiAdapter = new OpenAILoRAAdapter();
const result = await openaiAdapter.trainLoRA({
  personaId: "wine-expert",
  traitType: "wine-expertise",
  dataset: trainingData,
  baseModel: "gpt-4o-mini"
});

// 2. OpenAI: Keep model ID, use via API
// result.modelId = "ft:gpt-4o-mini-2024-07-18:personal::CcKeiPN2"
// compositionMethods: [] - no composition, single layer only

// 3. Fireworks/Together/DeepSeek: Download adapter
const fireworksAdapter = new FireworksLoRAAdapter();
await downloadAdapter(result.modelId, "./adapters/wine-expertise");
// compositionMethods: [] - but now you CAN use PEFT locally!
```

### Local PEFT Composition

```typescript
// FUTURE: PEFTCompositionAdapter (wraps Python PEFT)
const peftAdapter = new PEFTCompositionAdapter();

// Capabilities query
const caps = peftAdapter.getFineTuningCapabilities();
console.log(caps.maxActiveLayers);        // 16
console.log(caps.supportsLocalComposition);  // true
console.log(caps.compositionMethods);     // ['stack', 'weighted', 'TIES', 'DARE']

// Compose downloaded adapters
await peftAdapter.composeAdapters({
  baseModel: "meta-llama/Llama-3.1-8B",
  adapters: [
    { path: "./adapters/wine-expertise", weight: 0.7 },
    { path: "./adapters/vin-diesel-style", weight: 0.3 }
  ],
  method: "weighted",  // Use weighted merge
  outputPath: "./adapters/wine-vin-diesel-composite"
});
```

---

## Why This Design?

**1. Remote APIs are single-layer by design**
- They control their inference stack
- Multi-layer composition requires their infrastructure support
- Current reality: maxActiveLayers=1 for all cloud providers

**2. PEFT unlocks composition locally**
- Download adapters from Fireworks/Together/DeepSeek
- Compose them locally with PEFT on your hardware
- Full control over merging strategies

**3. Explicit is better than implicit**
- `compositionMethods: []` clearly says "no native composition"
- `compositionMethods: ['stack', 'weighted']` tells PersonaGenome what's available
- No guessing, no surprises

**4. Future-proof**
- When Fireworks adds native multi-layer support, just update their `getFineTuningCapabilities()`
- PersonaGenome queries capabilities at runtime and adapts

---

## Implementation Status

**✅ DONE**:
- Type system (`FineTuningCapabilities` with genome fields)
- All 4 remote adapters declare capabilities
- Python PEFT environment installed
- PEFT composition prototype (`peft_composition.py`)

**📋 TODO**:
- Create `PEFTCompositionAdapter` TypeScript wrapper
- Implement adapter download scripts (Fireworks, Together)
- Test dynamic composition with real trained adapters
- Create `genome/composite-create` command for offline merging

---

## Related Documentation

- [DYNAMIC-GENOME-ARCHITECTURE.md](DYNAMIC-GENOME-ARCHITECTURE.md) - Overall genome architecture
- [DYNAMIC-COMPOSITION-ROADMAP.md](../../system/genome/fine-tuning/DYNAMIC-COMPOSITION-ROADMAP.md) - Implementation phases
- [peft_composition.py](../../system/genome/python/peft_composition.py) - Python PEFT integration
- [FineTuningTypes.ts](../../system/genome/fine-tuning/shared/FineTuningTypes.ts) - Type definitions

---

**Key Insight**: The `compositionMethods` array is the contract between providers and PersonaGenome. Empty array = train single layers, compose locally. Full array = native multi-layer support.
