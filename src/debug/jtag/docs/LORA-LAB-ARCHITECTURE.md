# LoRA Lab Architecture

**Status**: Design Draft
**Goal**: Mass-market LoRA layer economy - cross-platform

---

## Vision

A lab where users on **any hardware** can:
1. **Use** LoRA layers from others (download, plug in, go)
2. **Create** their own fine-tuned layers locally
3. **Share** layers with the community
4. **Compose** multiple layers for unique AI personalities

Think: **App Store for AI customizations** - but the "apps" are lightweight adapters that stack on base models.

### Platform Spectrum

```
┌─────────────────────────────────────────────────────────────────┐
│                     LORA LAB PLATFORMS                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  FLOOR (Minimum)              CEILING (Maximum)                  │
│  ─────────────────────────────────────────────────────────────  │
│                                                                  │
│  M1 Mac (8GB)                 RTX 5090 (32GB VRAM)              │
│  • Inference only             • Full training                    │
│  • Small adapters             • Large models (70B+)              │
│  • Candle + Metal             • Candle + CUDA                    │
│                                                                  │
│  M1 Pro/Max (16-32GB)         Docker/Ubuntu + CUDA              │
│  • Local training             • Production inference             │
│  • 3B-7B models               • Multi-GPU training               │
│  • Candle + Metal             • Batch processing                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Key Insight**: Adapters are portable. Train on RTX 5090, deploy on M1 laptop.

---

## User Tiers

### Tier 1: Consumers (M1 8GB, entry laptops)
- Download and use adapters from others
- Swap between personalities/skills
- **Zero training required**
- Target: 80% of users

### Tier 2: Hobbyist Creators (M1 Pro 16GB+, gaming PCs)
- Train small adapters locally
- Share creations with community
- Experiment with composition
- Target: 15% of users

### Tier 3: Power Users (RTX 5090, cloud instances)
- Train large models (7B-70B)
- Create high-quality curated adapters
- Run production inference servers
- Train vision models
- Target: 5% of users

**The Economy**: Tier 3 creates → Tier 2 remixes → Tier 1 consumes

---

## User Scenarios

### Scenario A: Consumer (Inference Only)

```
User downloads adapter from Hub
         ↓
    Base Model (Llama 3.2 3B)
         +
    LoRA Adapter (2-50 MB)
         ↓
    Customized AI
```

**Requirements:**
- 8GB unified memory minimum
- No training needed
- ~30 second adapter load time
- Hot-swap between adapters

**Example workflow:**
```bash
# Browse available adapters
./jtag adapter/search --query="typescript expert"

# Download and activate
./jtag adapter/install --id="community/typescript-guru-v2"
./jtag adapter/activate --id="typescript-guru-v2"

# Use immediately
./jtag ai/generate --prompt="Review this TypeScript code..."
```

### Scenario B: Creator - Mac (Training + Inference)

```
User's training data (corrections, examples)
         ↓
    Candle LoRA Training (Metal backend)
         ↓
    Custom Adapter (.safetensors)
         ↓
    Publish to Hub (optional)
```

**Requirements:**
- 16GB+ unified memory recommended
- 10-60 minutes training time (500-2000 examples)
- Local dataset preparation tools

**Example workflow:**
```bash
# Prepare training data from chat corrections
./jtag training/prepare \
  --source="corrections" \
  --persona="helper-ai" \
  --output="./datasets/my-style.jsonl"

# Train locally (Rust/Candle with Metal)
./jtag training/start \
  --base="unsloth/Llama-3.2-3B-Instruct" \
  --data="./datasets/my-style.jsonl" \
  --output="./adapters/my-style-v1.safetensors" \
  --epochs=3

# Test locally
./jtag adapter/activate --path="./adapters/my-style-v1.safetensors"

# Share with community (optional)
./jtag adapter/publish \
  --path="./adapters/my-style-v1.safetensors" \
  --name="my-writing-style" \
  --description="Trained on my chat corrections"
```

### Scenario C: Power User - Docker/CUDA (RTX 5090)

```
Large datasets + powerful GPU
         ↓
    Candle LoRA Training (CUDA backend)
         ↓
    High-quality Adapter (.safetensors)
         ↓
    Publish to Hub (curated tier)
```

**Requirements:**
- RTX 5090 (32GB VRAM) or multi-GPU setup
- Docker with NVIDIA Container Toolkit
- Ubuntu 22.04+ or WSL2

**Example workflow:**
```bash
# Train large model (Rust/Candle with CUDA)
./jtag training/start \
  --backend="cuda" \
  --base="Qwen/Qwen2.5-7B-Instruct" \
  --data="./datasets/expert-corrections.jsonl" \
  --output="./adapters/expert-coder-v1.safetensors" \
  --epochs=5 \
  --lora-rank=64 \
  --batch-size=8

# Train vision model for UI critique
./jtag training/vision/start \
  --backend="cuda" \
  --base="Qwen/Qwen2.5-VL-7B-Instruct" \
  --data="./datasets/ui-critique.jsonl" \
  --output="./adapters/ui-expert-v1.safetensors" \
  --epochs=3

# Batch inference for production
./jtag inference/batch \
  --adapter="./adapters/expert-coder-v1.safetensors" \
  --input="./requests.jsonl" \
  --output="./responses.jsonl" \
  --batch-size=32
```

**Power user advantages:**
- 10-50x faster training than M1
- Larger models (7B-70B)
- Vision model training
- Batch processing for production
- Multi-GPU parallelism

### Scenario D: Hybrid - API Key Acceleration

```
User has API key (Together, Fireworks, Modal)
         ↓
    Upload dataset (encrypted)
         ↓
    Cloud Training (minutes, not hours)
         ↓
    Download adapter locally
         ↓
    Run on any hardware
```

**For users who want:**
- Faster training without local GPU
- Access to larger models (70B+)
- Privacy-preserving (data deleted after training)
- Pay-per-use flexibility

**Example workflow:**
```bash
# Configure API key
./jtag config/set --key="TOGETHER_API_KEY" --value="tok_..."

# Offload training to cloud
./jtag training/offload \
  --provider="together" \
  --model="Qwen/Qwen2.5-72B-Instruct" \
  --data="./datasets/expert-level.jsonl" \
  --output="./adapters/cloud-trained.safetensors" \
  --privacy="delete-after-training"

# Download and use locally (runs on M1!)
./jtag adapter/activate --path="./adapters/cloud-trained.safetensors"
```

**Key insight**: Train on 72B model in cloud, deploy adapter on 3B model locally. The adapter captures the *knowledge*, base model provides the *capability*.

---

## Practical Model Tiers (All Free, All Local)

| Tier | Model | Memory | Training | Best For |
|------|-------|--------|----------|----------|
| **Tiny** | Moondream 1.6B | 4GB | 5-10 min | Vision Q&A, quick tests |
| **Small** | Llama 3.2 3B | 8GB | 15-30 min | Chat, coding, general |
| **Medium** | Qwen2.5 7B | 16GB | 30-60 min | Complex reasoning |
| **Vision** | Qwen2.5-VL 3B | 12GB | 20-40 min | UI, screenshots |
| **Large** | Llama 3.3 70B | 48GB+ | 2-4 hrs | Expert systems (RTX 5090) |

**Note:** All models run 100% locally. Training times for ~500 examples with LoRA rank 32.

**Free Tools Used:**
- Candle (Rust) - Inference AND training, MIT license
- candle-lora - LoRA layer swapping for Candle
- Metal backend - Native Apple Silicon acceleration
- CUDA backend - NVIDIA GPU acceleration
- HuggingFace Hub - Free model downloads

**No Python required.** Everything runs in Rust.

---

## Local Storage Strategy

**Think Docker images**: Each adapter is like a Docker image with its own internal layers.

```
┌─────────────────────────────────────────────────────────────────────┐
│  ADAPTER: typescript-expert-v3                                       │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  Internal Layers (LoRA weight matrices per model layer):       │  │
│  │    • layers.0.self_attn.q_proj  (A: [32,4096], B: [4096,32])  │  │
│  │    • layers.0.self_attn.v_proj  (A: [32,4096], B: [4096,32])  │  │
│  │    • layers.1.self_attn.q_proj  ...                            │  │
│  │    • layers.1.self_attn.v_proj  ...                            │  │
│  │    • ... (typically 64-128 layer pairs)                        │  │
│  └───────────────────────────────────────────────────────────────┘  │
│  manifest.json: { base: "Llama-3.2-3B", rank: 32, scale: 1.0 }      │
└─────────────────────────────────────────────────────────────────────┘
```

**Composition** = stacking multiple adapters (like Docker multi-stage builds):
```
Your AI = Base Model + Adapter1(scale=1.0) + Adapter2(scale=0.8) + ...
W' = W + scale₁(B₁A₁) + scale₂(B₂A₂) + ...
```

**Key insight**: Like Docker images, adapters are immutable, versioned, and composable. Each adapter is a complete package you can pull, push, and stack.

Adapters are stored locally in `~/.continuum/adapters/` with a consistent structure for discovery, loading, and packaging.

### Directory Layout

```
~/.continuum/
├── adapters/
│   ├── installed/                   # Downloaded/installed adapters
│   │   ├── typescript-expert-v3/
│   │   │   ├── adapter.safetensors  # LoRA weights (2-50 MB)
│   │   │   ├── manifest.json        # Metadata, version, compatibility
│   │   │   └── README.md            # Usage guide
│   │   └── code-reviewer-v2/
│   │
│   ├── training/                    # In-progress training outputs
│   │   └── my-style-2024-01/
│   │       ├── checkpoint-500/
│   │       ├── checkpoint-1000/
│   │       └── training.log
│   │
│   └── personal/                    # User-created, local-only
│       └── my-writing-style/
│           ├── adapter.safetensors
│           ├── manifest.json
│           └── training-data.jsonl  # Optional: keep training data
│
├── models/                          # Base model cache (HuggingFace)
│   ├── unsloth--Llama-3.2-3B-Instruct/
│   └── Qwen--Qwen2.5-VL-3B-Instruct/
│
└── config.json                      # User settings, API keys
```

### Key Design Decisions

1. **Single safetensor file per adapter**: `adapter.safetensors` contains all LoRA A/B matrices
2. **Manifest is required**: Every adapter has `manifest.json` with version, base model, compatibility
3. **Training checkpoints preserved**: Can resume training or revert to earlier versions
4. **HuggingFace cache symlinks**: Models stored in standard HF cache, symlinked for organization
5. **Portable**: Entire `adapters/` directory can be copied/backed up

### Discovery

```bash
# List all installed adapters
./jtag adapter/list

# Load by ID (looks in installed/ and personal/)
./jtag adapter/activate --id="typescript-expert-v3"

# Load by path (any location)
./jtag adapter/activate --path="./custom/my-adapter.safetensors"
```

---

## Adapter Hub Structure (Remote)

For sharing adapters publicly:

```
continuum-hub/
├── adapters/
│   ├── official/                    # Curated, tested
│   │   ├── typescript-expert-v3/
│   │   │   ├── adapter.safetensors  # 15 MB
│   │   │   ├── manifest.json        # Metadata
│   │   │   └── README.md            # Usage guide
│   │   ├── code-reviewer-v2/
│   │   └── ui-critique-v1/
│   │
│   ├── community/                   # User-contributed
│   │   ├── wine-sommelier/
│   │   ├── legal-assistant/
│   │   └── game-master-dnd/
│   │
│   └── personal/                    # Private (local only)
│       └── my-writing-style/
│
└── models/                          # Base models (optional cache)
    ├── llama-3.2-3b-instruct-4bit/
    └── qwen2.5-vl-3b-instruct-4bit/
```

### Manifest Schema

```json
{
  "id": "typescript-expert-v3",
  "name": "TypeScript Expert",
  "version": "3.0.0",
  "description": "Deep TypeScript knowledge with strict typing focus",
  "author": "continuum-team",
  "license": "MIT",

  "base_model": "unsloth/Llama-3.2-3B-Instruct",
  "adapter_type": "lora",
  "lora_rank": 32,
  "lora_alpha": 64,

  "training": {
    "examples": 2500,
    "epochs": 3,
    "dataset_source": "typescript-corrections-2024"
  },

  "compatibility": {
    "min_memory_gb": 8,
    "platforms": ["darwin-arm64", "linux-x86_64", "windows-x86_64"],
    "inference_backends": ["candle-metal", "candle-cuda", "ollama"]
  },

  "metrics": {
    "downloads": 1234,
    "rating": 4.7,
    "verified": true
  },

  "files": {
    "adapter": "adapter.safetensors",
    "size_mb": 15.2
  }
}
```

---

## Persona = Base Model + Genome + Databases

A **Persona** is the complete package - not just adapters:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         PERSONA PACKAGE                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  BASE MODEL (immutable, shared)                                         │
│  └── unsloth/Llama-3.2-3B-Instruct                                      │
│                                                                          │
│  LORA GENOME (0-N layers, shareable)                                    │
│  ├── Layer 0: code-style:v2 (scale: 1.0)                                │
│  ├── Layer 1: typescript-expert:v3 (scale: 0.8)                         │
│  └── Layer 2: my-personality:v1 (scale: 0.5)                            │
│                                                                          │
│  DATABASES (per-persona, selective sharing)                             │
│  ├── ltm.db              [PRIVATE]   Long-term memories                 │
│  ├── corrections.db      [SHAREABLE] Training examples                  │
│  ├── preferences.db      [PRIVATE]   User preferences                   │
│  ├── skills.db           [SHAREABLE] Skill inventory                    │
│  └── hippocampus.db      [PRIVATE]   Episodic memory                    │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Sharing Granularity

Three levels of sharing - like Docker images and containers:

```
┌────────────────────────────────────────────────────────────────┐
│                     SHARING GRANULARITY                        │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  1. INDIVIDUAL LAYER (finest grain)                            │
│     └── Just one adapter: typescript-expert.safetensors        │
│         Size: 2-50 MB                                          │
│         Use: Share a specific skill                            │
│                                                                │
│  2. MEMORY-LESS PERSONA (genome only)                          │
│     └── Stack config + adapter refs (no DBs)                   │
│         {                                                      │
│           base: "Llama-3.2-3B",                                │
│           layers: ["code-style:1.0", "ts-expert:0.8"]          │
│         }                                                      │
│         Size: ~1 KB (just JSON, adapters pulled lazily)        │
│         Use: Share personality without memories                │
│                                                                │
│  3. WHOLE PERSONA (genome + memories)                          │
│     └── Full package with selected DBs                         │
│         ├── genome.json                                        │
│         ├── corrections.db    (training examples)              │
│         ├── skills.db         (capabilities)                   │
│         └── ltm.db            (optional - user choice)         │
│         Size: 10-500 MB depending on memory depth              │
│         Use: Clone a fully-trained expert                      │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

**Commands:**
```bash
# Share just a layer
./jtag adapter/push typescript-expert:v3

# Share memory-less persona (just the recipe)
./jtag persona/push helper-ai --no-memory

# Share whole persona with memories
./jtag persona/push helper-ai --with-memory
```

---

## Dynamic Genome Routing (Semantic MoE)

Layer scales aren't static - they're **dynamically computed** based on input using embedding similarity. Like a soft Mixture of Experts routing:

```
┌─────────────────────────────────────────────────────────────────┐
│                    DYNAMIC GENOME ROUTING                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  INPUT: "Review this TypeScript PR for security issues"         │
│                         │                                        │
│                         ▼                                        │
│                    embed(input)                                  │
│                         │                                        │
│                         ▼                                        │
│         ┌───────────────────────────────────┐                   │
│         │  COSINE SIMILARITY vs ADAPTERS    │                   │
│         │                                   │                   │
│         │  typescript-expert:  0.82 ████████░░                  │
│         │  security-reviewer:  0.94 █████████░                  │
│         │  code-style:         0.31 ███░░░░░░░                  │
│         │  writing-clarity:    0.15 █░░░░░░░░░                  │
│         └───────────────────────────────────┘                   │
│                         │                                        │
│                         ▼                                        │
│              W' = W + 0.82(B₁A₁)    ← typescript                │
│                      + 0.94(B₂A₂)    ← security (dominant)      │
│                      + 0.31(B₃A₃)    ← code-style (reduced)     │
│                      + 0.15(B₄A₄)    ← writing (minimal)        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Each adapter has a domain embedding** - a vector representing its competence space:

```typescript
interface AdapterEntity extends BaseEntity {
  adapterId: string;
  digest: string;

  // Domain embedding for MoE routing
  domainEmbedding: {
    learned?: number[];        // From usage feedback (best)
    training?: number[];       // From training data centroid (good)
    description: number[];     // From text description (fallback)
  };

  domainDescription: string;   // "TypeScript type safety and patterns"
}
```

**Embedding priority** (use best available):
1. **Learned** - accumulated from actual usage feedback
2. **Training** - centroid of training examples
3. **Description** - embed the text description (cold start)

---

## Learned Domain Embeddings

Adapters learn their own competence space from usage:

```
CONTINUOUS LEARNING
────────────────────────────────────────────────────────────

User: "Help with this TypeScript generic"
      → typescript-expert activated at 0.9
      → Good response, user thumbs up 👍
      → Move typescript-expert embedding TOWARD this input

User: "Write a poem about code"
      → typescript-expert activated at 0.4
      → User corrects: "too technical"
      → Move typescript-expert embedding AWAY from this input

RESULT: Adapter naturally specializes based on where it actually helps
```

```typescript
async function updateDomainEmbedding(
  adapter: AdapterEntity,
  input: string,
  helpful: boolean
): Promise<void> {
  const inputEmb = await embed(input);
  const current = adapter.domainEmbedding.learned
    ?? adapter.domainEmbedding.training
    ?? adapter.domainEmbedding.description;

  // Online update: move toward helpful, away from unhelpful
  const alpha = 0.1;
  const direction = helpful ? 1 : -1;

  adapter.domainEmbedding.learned = current.map((v, i) =>
    v + alpha * direction * (inputEmb[i] - v)
  );
}
```

---

## Adapter Inheritance (Transfer Learning)

Training embeddings form a **searchable competence space**. When creating a new adapter, find similar existing ones as starting points:

```
ADAPTER DISCOVERY / INHERITANCE
────────────────────────────────────────────────────────────

WANT: "Legal contract clause reviewer"
                │
                ▼
         embed(description)
                │
                ▼
┌───────────────────────────────────────────────────────────┐
│  SEARCH EXISTING TRAINING SPACES                          │
│                                                           │
│  code-reviewer     ████████░░  0.65  (structured review)  │
│  writing-style     ██████░░░░  0.52  (prose clarity)      │
│  security-expert   █████░░░░░  0.48  (risk analysis)      │
│  typescript-expert ██░░░░░░░░  0.21  (wrong domain)       │
│                                                           │
└───────────────────────────────────────────────────────────┘
                │
                ▼
         STARTING POINTS:
         - Initialize from code-reviewer weights (closest)
         - Blend in writing-style at 0.5
         - Fine-tune on legal corpus

         → Faster convergence, better results
```

```typescript
// Find nearest adapters for transfer learning
async function findStartingPoints(
  desiredTraits: string,
  topK: number = 3
): Promise<Array<{ adapter: AdapterEntity; similarity: number }>> {
  const targetEmb = await embed(desiredTraits);

  const scored = adapters.map(a => ({
    adapter: a,
    similarity: cosineSimilarity(targetEmb, a.domainEmbedding.training)
  }));

  return scored
    .sort((a, b) => b.similarity - a.similarity)
    .slice(0, topK);
}

// Create new adapter with informed initialization
async function createAdapter(params: {
  description: string;
  trainingData: string;
}): Promise<AdapterEntity> {
  const ancestors = await findStartingPoints(params.description);

  // Warm start from best match if similarity > threshold
  const initWeights = ancestors[0].similarity > 0.5
    ? await loadWeights(ancestors[0].adapter)
    : null;

  return train({
    data: params.trainingData,
    initFrom: initWeights,
  });
}
```

**Adapters form a family tree** - new ones inherit from semantically similar ancestors. Never training blind.

---

## Persona Lab UI (Widget)

Visual genome editor from the persona page:

```
┌─────────────────────────────────────────────────────────────────────────┐
│  PERSONA: helper-ai                                        [Save] [Test]│
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  BASE MODEL: unsloth/Llama-3.2-3B-Instruct              [Change]        │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  GENOME STACK                                      [+ Add Layer] │   │
│  │                                                                   │   │
│  │  ☰ Layer 2: my-personality      ████░░░░░░ 0.5  [↑] [↓] [×]     │   │
│  │  ☰ Layer 1: typescript-expert   ████████░░ 0.8  [↑] [↓] [×]     │   │
│  │  ☰ Layer 0: code-style          ██████████ 1.0  [↑] [↓] [×]     │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  DYNAMIC ROUTING (MoE)                              [Enabled ✓]  │   │
│  │  Threshold: ████████░░░░░░░░ 0.3   (min similarity to activate)  │   │
│  │  Dampening: ██████░░░░░░░░░░ 0.2   (reduce low-score scales)     │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌──────────────────────┐  ┌──────────────────────────────────────┐   │
│  │  AVAILABLE ADAPTERS  │  │  TEST AREA                           │   │
│  │  □ security-expert   │  │  [Chat] [Benchmark] [Canvas] [Game]  │   │
│  │  □ writing-clarity   │  │                                      │   │
│  │  □ legal-reviewer    │  │  > You: Review this code...          │   │
│  │  [Browse Registry]   │  │  > AI: I notice a potential XSS...   │   │
│  └──────────────────────┘  │  Routing: ts:0.82 sec:0.91 style:0.3 │   │
│                            └──────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Drag-drop genome layers, adjust scales with sliders
- Live routing visualization (see which adapters activate per input)
- Inline testing: chat, benchmarks, canvas, games
- A/B compare different genome configs
- Import/Export genome configurations

---

## Persona Genome Autonomy

Should personas control their own genome?

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     GENOME ACCESS MODELS                                 │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  MODEL A: User-Controlled (default)                                     │
│  ────────────────────────────────────                                   │
│  • User edits genome via Persona Lab UI                                 │
│  • Persona sees current genome, cannot modify                           │
│  • Safe, predictable, explicit control                                  │
│                                                                          │
│  MODEL B: Curated Access                                                │
│  ───────────────────────                                                │
│  • User whitelists adapters per persona                                 │
│  • Persona can pick from allowed set                                    │
│  • "You can use these 5 adapters, choose what fits"                     │
│                                                                          │
│  MODEL C: Full Autonomy                                                 │
│  ──────────────────────                                                 │
│  • Persona has access to entire registry                                │
│  • Can add/remove/adjust layers based on task performance               │
│  • Self-evolving genome (continuous learning)                           │
│  • Requires trust, may diverge unexpectedly                             │
│                                                                          │
│  MODEL D: Supervised Evolution                                          │
│  ────────────────────────────                                           │
│  • Persona proposes genome changes                                      │
│  • User approves/rejects via notification                               │
│  • "I think adding security-expert would help. Allow? [Y/N]"            │
│  • Best of both: autonomy with oversight                                │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Implementation:**

```typescript
interface GenomePermissions {
  mode: 'user-controlled' | 'curated' | 'autonomous' | 'supervised';

  // For 'curated' mode
  allowedAdapters?: string[];

  // For 'autonomous' and 'supervised' modes
  canAddLayers: boolean;
  canRemoveLayers: boolean;
  canAdjustScales: boolean;
  maxLayers: number;

  // For 'supervised' mode
  requireApproval: boolean;
}

// Persona can request genome changes
interface GenomeChangeRequest {
  personaId: string;
  action: 'add' | 'remove' | 'adjust';
  adapterId: string;
  reason: string;           // "This task involves security review"
  proposedScale?: number;
  status: 'pending' | 'approved' | 'rejected';
}
```

**Supervised Evolution** is probably the sweet spot - personas can learn and propose improvements, but user stays in control. Could even auto-approve low-risk changes (scale adjustments) while requiring approval for adding new layers.

---

## Entity Definitions

Core entities for the adapter/persona registry:

```typescript
// AdapterEntity - Single LoRA layer (content-addressable)
interface AdapterEntity extends BaseEntity {
  // Identity
  adapterId: string;           // "typescript-expert-v3"
  namespace: string;           // "official" | "community" | "personal"
  version: string;             // semver: "3.0.0"
  digest: string;              // SHA256 of safetensors (immutable ref)

  // Base model dependency
  baseModelId: string;         // "unsloth/Llama-3.2-3B-Instruct"

  // LoRA config
  loraRank: number;            // 8, 16, 32, 64
  loraAlpha: number;
  targetModules: string[];     // ["q_proj", "v_proj", ...]

  // Domain embedding for MoE routing
  domainEmbedding: {
    learned?: number[];        // From usage feedback
    training?: number[];       // From training data centroid
    description: number[];     // From text description
  };
  domainDescription: string;

  // Training provenance
  trainingExamples: number;
  trainingSource: 'corrections' | 'dataset' | 'synthetic';
  ancestorAdapters?: string[]; // Transfer learning lineage

  // Metadata
  author: string;
  license: string;
  sizeBytes: number;
  filePath: string;
}

// GenomeEntity - Stack configuration (tiny, just references)
interface GenomeEntity extends BaseEntity {
  genomeId: string;
  name: string;
  baseModelId: string;

  layers: Array<{
    order: number;             // Stack order (0 = bottom)
    adapterId: string;
    adapterDigest: string;     // Pinned version
    defaultScale: number;      // Static fallback
    domain: string;            // For dynamic routing
  }>;

  // Routing config
  dynamicRouting: boolean;     // Use semantic MoE?
  routingThreshold: number;    // Min similarity to activate
}

// PersonaPackageEntity - Full export manifest
interface PersonaPackageEntity extends BaseEntity {
  personaId: string;
  name: string;
  description: string;

  // Components
  genomeId: string;
  baseModelId: string;

  // Included databases
  includedDatabases: Array<{
    name: string;              // "corrections", "skills"
    type: DatabaseType;
    digest: string;
    sizeBytes: number;
  }>;

  // Excluded (listed for transparency)
  excludedDatabases: Array<{
    name: string;
    type: DatabaseType;
    reason: 'private' | 'user-choice';
  }>;

  // Package metadata
  version: string;
  totalSizeBytes: number;
  capabilities: string[];      // ["typescript", "code-review"]
}

type DatabaseType =
  | 'ltm'           // Long-term memory
  | 'corrections'   // Training examples
  | 'preferences'   // User settings
  | 'skills'        // Capability inventory
  | 'hippocampus'   // Episodic memory
  | 'tasks'         // Task queue
  ;
```

---

## Adapter Composition

Stack multiple adapters for compound expertise:

```
Base Model: Llama 3.2 3B
    ↓
Layer 1: code-style-v2 (scale: 1.0)
    ↓
Layer 2: typescript-expert-v3 (scale: 0.8)
    ↓
Layer 3: my-personality (scale: 0.5)
    ↓
Result: AI with your style + TS expertise + code conventions
```

**Implementation:**
```bash
# Activate multiple adapters with scaling
./jtag adapter/compose \
  --adapters="code-style-v2:1.0,typescript-expert-v3:0.8,my-personality:0.5" \
  --save-as="my-composite-expert"
```

**Math:** `W' = W + scale₁(B₁A₁) + scale₂(B₂A₂) + scale₃(B₃A₃)`

---

## Local Training Pipeline (Rust/Candle)

### Step 1: Data Collection

```typescript
// Automatic collection from chat corrections
interface TrainingExample {
  instruction: string;  // What was asked
  input: string;        // Context provided
  output: string;       // Correct response
  source: 'correction' | 'thumbs_up' | 'import';
}
```

Sources:
- Chat corrections ("Actually, you should...")
- Thumbs up on good responses
- Imported datasets (JSONL)
- Failed tool calls with fixes

### Step 2: Training

```bash
# Train with Candle (auto-detects Metal or CUDA)
./jtag training/start \
  --base="unsloth/Llama-3.2-3B-Instruct" \
  --data="./datasets/training.jsonl" \
  --output="./adapters/output.safetensors" \
  --lora-rank=32 \
  --lora-alpha=64 \
  --batch-size=2 \
  --epochs=3 \
  --learning-rate=2e-4
```

### Step 3: Validation

```bash
# Quick quality check before deployment
./jtag training/validate \
  --adapter="./adapters/output.safetensors" \
  --test-set="./datasets/test.jsonl" \
  --min-quality=0.8
```

### Step 4: Deployment

```bash
# Hot-swap into running inference
./jtag adapter/activate \
  --path="./adapters/output.safetensors" \
  --replace="previous-adapter"
```

---

## Vision Model Workflow (UI/Design)

Vision models are **fully local and free** - same as text models. All Rust, no Python.

### Training UI Expert (Any Platform)

**On Mac (M1/M2/M3) - Metal backend:**
```bash
# 1. Collect UI screenshots with corrections
./jtag training/vision/collect \
  --source="screenshots" \
  --with-corrections

# 2. Train with Candle (Metal, free, local)
./jtag training/vision/start \
  --backend="metal" \
  --base="Qwen/Qwen2.5-VL-3B-Instruct" \
  --data="./datasets/ui-critique.jsonl" \
  --output="./adapters/ui-expert-v1.safetensors" \
  --epochs=3

# 3. Test on a screenshot
./jtag ai/vision/analyze \
  --image="./screenshot.png" \
  --adapter="ui-expert-v1" \
  --prompt="What usability issues do you see?"
```

**On CUDA (RTX 5090) - CUDA backend:**
```bash
# Same workflow, just different backend
./jtag training/vision/start \
  --backend="cuda" \
  --base="Qwen/Qwen2.5-VL-7B-Instruct" \
  --data="./datasets/ui-critique.jsonl" \
  --output="./adapters/ui-expert-v1.safetensors" \
  --epochs=5 \
  --batch-size=8
```

### Example Training Data Format

```jsonl
{"image": "ui-001.png", "conversations": [{"from": "human", "value": "What's wrong with this UI?"}, {"from": "assistant", "value": "The CTA button has low contrast..."}]}
{"image": "ui-002.png", "conversations": [{"from": "human", "value": "Rate this layout"}, {"from": "assistant", "value": "3/5. The visual hierarchy is unclear..."}]}
```

### Vision Adapter Examples

| Adapter | Domain | Training Data | Use Case |
|---------|--------|---------------|----------|
| ui-critique | UI/UX | 1000 annotated screenshots | Design feedback |
| code-screenshot | Code | IDE screenshots + explanations | Visual code review |
| diagram-reader | Architecture | System diagrams + descriptions | Doc generation |
| accessibility-check | A11y | WCAG violation examples | Compliance audit |

---

## Multi-Modal Outputs

LoRA adapters aren't just for text - they work across **all generative modalities**.

### Modality Spectrum

```
┌─────────────────────────────────────────────────────────────────┐
│                     OUTPUT MODALITIES                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  TEXT (Current Focus)                                           │
│  • Chat, code, writing, analysis                                 │
│  • Llama 3.2, Qwen2.5                                           │
│  • Training: 30 min on M1                                        │
│                                                                  │
│  VISION INPUT (Phase 6)                                          │
│  • UI analysis, screenshot understanding                         │
│  • Moondream, Qwen2.5-VL                                        │
│  • Training: 1 hr on M1, 15 min on CUDA                         │
│                                                                  │
│  IMAGE OUTPUT (Phase 9)                                          │
│  • Custom art styles, logo generation                            │
│  • SDXL, Flux, custom diffusion                                  │
│  • Training: CUDA required (16GB+ VRAM)                          │
│                                                                  │
│  AUDIO OUTPUT (Phase 10)                                         │
│  • Voice cloning, TTS styles, music                              │
│  • XTTS, MusicGen, Bark                                         │
│  • Training: CUDA recommended                                    │
│                                                                  │
│  VIDEO (Future)                                                  │
│  • Style transfer, motion synthesis                              │
│  • SVD, CogVideo, Sora-like                                     │
│  • Training: Multi-GPU required                                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Image Generation Adapters

For personas that want to CREATE images, not just understand them:

```bash
# Train custom art style on CUDA
./jtag training/diffusion/start \
  --backend="cuda" \
  --base="stabilityai/stable-diffusion-xl-base-1.0" \
  --data="./datasets/my-art-style/" \
  --output="./adapters/my-art-lora.safetensors" \
  --steps=2000

# Generate with style
./jtag ai/image/generate \
  --adapter="my-art-lora" \
  --prompt="A cyberpunk city at sunset" \
  --output="./generated.png"
```

### Audio Adapters

Voice cloning, custom TTS, music generation:

```bash
# Clone voice (requires audio samples)
./jtag training/audio/start \
  --backend="cuda" \
  --base="coqui/XTTS-v2" \
  --data="./voice-samples/" \
  --output="./adapters/my-voice.safetensors"

# Generate speech with cloned voice
./jtag ai/audio/speak \
  --adapter="my-voice" \
  --text="Hello, this is my cloned voice" \
  --output="./speech.wav"
```

### Community Specialization

Different users contribute different modalities:

| Hardware | Best For | Community Role |
|----------|----------|----------------|
| M1 8GB | Text inference, small adapters | Consumers, light creators |
| M1 Pro 32GB | Text training, vision inference | Text specialists |
| RTX 3080 16GB | Small diffusion, vision training | Image hobbyists |
| RTX 5090 32GB | All modalities, large models | Full creators |
| Multi-GPU | Video, large diffusion | Production studios |

**The Exchange**: Someone with RTX 5090 trains diffusion adapters → shares with M1 users who can only run inference → M1 users train text adapters → share back.

### Impatient Path (Cloud On-Demand)

Don't have the hardware? Don't want to wait? Pay for speed:

```bash
# Train diffusion adapter in cloud (minutes instead of hours)
./jtag training/diffusion/start \
  --offload=true \
  --base="stabilityai/stable-diffusion-xl-base-1.0" \
  --data="./my-art-style/" \
  --output="./adapters/my-art-lora.safetensors"

# Download and run locally (inference works on M1!)
./jtag ai/image/generate \
  --adapter="my-art-lora" \
  --prompt="A landscape in my style"
```

**The trade-off:**
- **Local (free, slow)**: Train overnight, own your compute
- **Cloud (pay, fast)**: Train in minutes, pay per use

Both produce identical adapters. Choose based on patience.

---

## Cross-Domain Adapters

The adapter system works for **any domain** - not just code. Train on your expertise, share with others.

### Example Domains

| Domain | Adapter Ideas | Training Source |
|--------|--------------|-----------------|
| **Code** | TypeScript expert, code reviewer, debugging specialist | Corrections, PRs, reviews |
| **Design** | UI critique, color theory, accessibility | Annotated screenshots |
| **Writing** | Technical docs, creative writing, tone matching | Edited drafts |
| **Legal** | Contract review, clause analysis | Expert annotations |
| **Medical** | Symptom triage, drug interactions | Clinical guidelines |
| **Gaming** | D&D dungeon master, game balance | Session transcripts |
| **Music** | Chord progression, lyric writing | Song analyses |
| **Finance** | Stock analysis, risk assessment | Market commentary |
| **Language** | Translation style, dialect matching | Parallel texts |
| **Education** | Tutoring style, explanation clarity | Student interactions |

### Domain Specialization Pattern

```bash
# 1. Collect domain-specific corrections
./jtag training/prepare \
  --source="corrections" \
  --domain="legal" \
  --output="./datasets/legal-review.jsonl"

# 2. Train specialist adapter
./jtag training/start \
  --data="./datasets/legal-review.jsonl" \
  --output="./adapters/legal-reviewer-v1.safetensors" \
  --epochs=3

# 3. Compose with base skills
./jtag adapter/compose \
  --adapters="legal-reviewer-v1:1.0,writing-clarity:0.5" \
  --save-as="legal-writer"
```

### Community Growth Model

```
┌─────────────────────────────────────────────────────────────────┐
│                     ADAPTER ECOSYSTEM                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Week 1: Early adopters train personal style adapters           │
│          └── 10 adapters, mostly code-focused                   │
│                                                                  │
│  Month 1: Power users create domain specialists                 │
│           └── 100 adapters, diverse domains emerging            │
│                                                                  │
│  Month 6: Community remixes and compositions                    │
│           └── 1000 adapters, composition patterns emerge        │
│                                                                  │
│  Year 1: Ecosystem of specialized stacks                        │
│          └── 10000+ adapters, emergent expertise combos         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Key Insight**: The value isn't in any single adapter - it's in the combinations. A "legal-typescript-concise" stack is more valuable than any piece alone.

---

## Cloud Offload (Optional)

**Not required.** Only for users who want to trade money for time.

```bash
# Optional: Configure API key for acceleration
./jtag config/set --key="TOGETHER_API_KEY" --value="tok_..."

# Training automatically uses cloud when --offload is set
./jtag training/start \
  --data="./datasets/training.jsonl" \
  --output="./adapters/my-adapter.safetensors" \
  --offload=true  # Optional: uses cloud if API key exists
```

Supported providers (all pay-per-use):
- Together AI - Best for Llama/Qwen models
- Fireworks AI - Fast inference focus
- Modal - Flexible GPU rentals
- RunPod - Cheapest per-hour

**Without API key**: Everything works locally. Zero cost.

---

## Economy Model

### Core Principle: Free First, Always

**Everything essential is free and local.** No accounts, no subscriptions, no usage limits.

### Free Forever (Default)
- Download unlimited adapters from community
- Train locally on your hardware (any backend)
- Share unlimited adapters with community
- Compose and remix adapters freely
- Full source code access (open source)

### Optional Acceleration (API Keys)
For users who WANT to pay for speed:
- Cloud training offload (Together, Fireworks, Modal)
- Train 70B+ models without local GPU
- 10-100x faster than local training
- Data privacy options (delete after training)

**How it works:**
```bash
# Add optional API key for acceleration
./jtag config/set --key="TOGETHER_API_KEY" --value="tok_..."

# Now training can optionally use cloud
./jtag training/start --offload=true  # Uses cloud if API key set
./jtag training/start                  # Always local, always free
```

### Adapter Trading
The real economy is **adapters**, not subscriptions:
- Power users create high-quality adapters
- Community remixes and improves
- Everyone benefits from collective learning
- No paywalls, just reputation/ratings

---

## Commands Summary

```bash
# Discovery
./jtag adapter/search --query="python expert"
./jtag adapter/list --filter="vision"
./jtag adapter/info --id="typescript-expert-v3"

# Installation
./jtag adapter/install --id="community/wine-sommelier"
./jtag adapter/uninstall --id="wine-sommelier"
./jtag adapter/update --id="typescript-expert-v3"

# Activation
./jtag adapter/activate --id="typescript-expert-v3"
./jtag adapter/deactivate
./jtag adapter/compose --adapters="a:1.0,b:0.5"

# Training
./jtag training/prepare --source="corrections"
./jtag training/start --data="./data.jsonl"
./jtag training/status
./jtag training/validate --adapter="./output"

# Publishing
./jtag adapter/publish --path="./adapter" --name="my-adapter"
./jtag adapter/unpublish --id="my-adapter"
```

---

## Success Metrics

1. **Accessibility**: 80% of M1 Mac users can train a LoRA in < 30 min
2. **Adapter Size**: Average adapter < 50 MB (easy to share)
3. **Quality**: Community adapters rated 4+ stars on average
4. **Composition**: Users create 3+ adapter combinations on average
5. **Economy**: Active adapter sharing/trading ecosystem

---

## Implementation Phases

### Phase 1: Rust Inference Backend (DONE)
- [x] Candle gRPC worker with Metal support
- [x] Model hot-swap via RPC commands
- [x] GPU memory allocator with smart eviction
- [x] Proto schema for model/adapter management

### Phase 2: LoRA Weight Loading (DONE)
- [x] Proto schema for adapter load/unload
- [x] Safetensor parsing in Rust (lora.rs with F32/F16/BF16 support)
- [x] LoRA weight merging with base model (W' = W + scale × B @ A)
- [x] Rebuild model with merged weights (`rebuild_with_lora_from_paths()`)
- [x] TypeScript client with `merge` option
- [ ] Hot-swap adapters without full model reload (future optimization)

### Phase 2.5: Model-Agnostic Adapter Mapping (TODO)
Different base models and training frameworks use different tensor naming:

```
PEFT/HuggingFace:  base_model.model.model.layers.X.self_attn.q_proj
Candle Llama:      model.layers.X.self_attn.q_proj.weight
Other frameworks:  layers.X.attention.query.weight
```

**Solution: Adapter Trait Per Model Architecture**

```rust
trait LoRANameMapper {
    /// Map LoRA tensor name to base model tensor name
    fn map_name(&self, lora_name: &str) -> String;

    /// Get target modules for this architecture
    fn target_modules(&self) -> &[&str];
}

struct LlamaNameMapper;
struct Qwen2NameMapper;
struct MistralNameMapper;

// Registry of mappers by model architecture
fn get_mapper(model_id: &str) -> Box<dyn LoRANameMapper>;
```

**Or Algorithmic Strategy:**
- Parse adapter_config.json for `base_model_name_or_path`
- Auto-detect architecture from model config
- Apply appropriate naming transformation

**Current**: Hardcoded Llama/PEFT mapping in `lora.rs::map_lora_name_to_model_name()`

### MILESTONE 1: Single Adapter Inference ✅ COMPLETE (2026-01-06)
- [x] Downloaded public Llama LoRA: `Jiten1024/llama-3.2-3b-int-finetune-jav-rank-1-alpha-32`
- [x] Load adapter via gRPC with merge: 196 LoRA layer pairs merged
- [x] Generate text: haiku generation test
- [x] Validated output differs from base model (different vocabulary, style)
- [x] **PROVEN: parsing, merging, rebuild, generation all work**

**Test**: `npx tsx tests/lora-adapter-test.ts` (integration test added)

### MILESTONE 2: Genome Assembly (Multi-Adapter)
- [ ] Load 2-3 public adapters with different scales
- [ ] Stack them: W' = W + s₁(B₁A₁) + s₂(B₂A₂) + ...
- [ ] Generate and verify composite behavior
- [ ] **Proves: composition math works**

### MILESTONE 3: Local Registry
- [ ] `~/.continuum/adapters/` directory structure
- [ ] manifest.json for each adapter
- [ ] `./jtag adapter/list` discovery
- [ ] `./jtag adapter/install` from path
- [ ] **Proves: can organize, store, find adapters**

### MILESTONE 4: Full Personas
- [ ] Genome config (JSON stack definition)
- [ ] `./jtag persona/export` with selected DBs
- [ ] `./jtag persona/import`
- [ ] **Proves: complete packaging and sharing story**

---

### Phase 3: Training Commands
- [ ] `training/prepare` - Collect corrections → JSONL
- [ ] `training/start` - Launch Candle LoRA training (Metal/CUDA)
- [ ] `training/status` - Monitor training progress
- [ ] `training/validate` - Quality check adapter

### Phase 4: Adapter Management
- [ ] `adapter/install` - Download from hub
- [ ] `adapter/activate` - Load into inference worker
- [ ] `adapter/deactivate` - Unload adapter
- [ ] `adapter/list` - Show installed adapters

### Phase 5: Composition
- [ ] Multi-adapter loading in Rust worker
- [ ] Scale factor support per adapter
- [ ] `adapter/compose` - Merge adapters
- [ ] Composite adapter saving

### Phase 6: Vision Models
- [ ] Moondream/Qwen2.5-VL integration in Candle
- [ ] Image encoding in gRPC proto
- [ ] `ai/vision/*` commands
- [ ] Vision LoRA training (Candle + Metal/CUDA)

### Phase 7: Hub & Sharing
- [ ] Adapter manifest schema validation
- [ ] `adapter/publish` command
- [ ] Community hub API
- [ ] Rating/discovery system

### Phase 8: Cloud Offload (Optional)
- [ ] Together/Fireworks API integration
- [ ] `training/offload` command
- [ ] Automatic adapter download

### Phase 9: Diffusion/Image Generation
- [ ] SDXL LoRA training wrapper
- [ ] `training/diffusion/start` command
- [ ] `ai/image/generate` command
- [ ] Diffusion adapter hub integration

### Phase 10: Audio Generation
- [ ] XTTS/voice cloning integration
- [ ] `training/audio/start` command
- [ ] `ai/audio/speak` command
- [ ] Audio adapter formats

### Phase 11: Video (Future)
- [ ] Research SVD/CogVideo integration
- [ ] Video LoRA training (multi-GPU)
- [ ] `ai/video/generate` command

---

## References

- [Candle](https://github.com/huggingface/candle) - Rust ML framework with Metal/CUDA backends
- [candle-lora](https://github.com/EricLBuehler/candle-lora) - Pure Rust LoRA implementation
- [safetensors](https://github.com/huggingface/safetensors) - Safe, fast tensor serialization
- [CONTINUOUS-LEARNING-RUNTIME.md](CONTINUOUS-LEARNING-RUNTIME.md) - Runtime architecture
- [LORA-TRAINING-STRATEGY.md](LORA-TRAINING-STRATEGY.md) - Training approaches
