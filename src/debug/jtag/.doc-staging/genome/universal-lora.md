# Universal LoRA Fine-Tuning Architecture

## Vision

Build a model-agnostic LoRA genome paging system where PersonaUsers can:
1. **Fine-tune** on custom datasets (git history, conversations, tasks)
2. **Page adapters** in/out dynamically based on task domain
3. **Work across ANY model type** through provider adapters

## The Four Provider Types

### 1. Local Models (MLX)
**Target**: qwen2.5-coder, llama3.2, deepseek-coder
**Fine-tuning**: MLX on Apple Silicon (blazing fast)
**Inference**: Ollama (local, zero cost)
**Use case**: Fast iteration, full control, privacy

### 2. SOTA Online Models (API-based)
**Target**: OpenAI GPT-4, GPT-3.5-turbo
**Fine-tuning**: OpenAI API (upload JSONL)
**Inference**: OpenAI API
**Use case**: Best quality, no local GPU needed

### 3. Sentinel Models (Hybrid)
**Target**: Any open-source model
**Fine-tuning**: Cloud GPUs (Modal, RunPod, AWS)
**Inference**: Download adapter, run locally via Ollama
**Use case**: Best quality + low latency

### 4. Multi-Model Swarm
**Target**: Mix of all above
**Example**: HelperAI uses local, CodeReviewAI uses OpenAI, TeacherAI uses Sentinel
**Use case**: Optimize cost/quality/latency per persona

---

## MLX Local Fine-Tuning Pipeline

### Phase 1: Data Preparation

**Input**: Our continuum-git JSONL (269MB, 1590 examples)

**MLX Format Requirements**:
```jsonl
{"text": "<|im_start|>system\nYou are...<|im_end|>\n<|im_start|>user\nWhat code changes...<|im_end|>\n<|im_start|>assistant\ndiff --git...<|im_end|>"}
```

**Conversion Script**: `scripts/convert-to-mlx-format.ts`
- Read continuum-git dataset
- Convert chat-completion format → single-text format with chat template
- Create train/valid/test splits (80/10/10)
- Output to `/datasets/prepared/continuum-git-mlx/`

### Phase 2: Model Preparation

**Pull HuggingFace Model**:
```bash
# qwen2.5-coder is already available from HF
# MLX requires HuggingFace format (not GGUF)
```

**Supported Base Models**:
- qwen2.5-coder (best for code)
- llama3.2 (general purpose)
- deepseek-coder (also excellent for code)

### Phase 3: Fine-Tuning

**MLX Fine-Tuning Command**:
```bash
python3 -m mlx_lm.lora \
  --model Qwen/Qwen2.5-Coder-1.5B \
  --train \
  --data /datasets/prepared/continuum-git-mlx/ \
  --iters 600 \
  --batch-size 2 \
  --learning-rate 1e-5 \
  --adapter-path /datasets/adapters/continuum-typescript-expertise
```

**Key Parameters**:
- `--iters 600`: Number of training steps
- `--batch-size 2`: Small for memory efficiency
- `--num-layers -1`: Fine-tune all layers (default: 16)
- `--adapter-path`: Where to save LoRA weights (~100MB)

**Training Time**: ~10-30 minutes on M1/M2/M3

### Phase 4: Export to Ollama

**Convert to GGUF**:
```python
# MLX export utilities (part of mlx-lm)
# Exports to Q8_0 quantization (8-bit)
# Generates Modelfile with correct chat template
```

**Create Ollama Model**:
```bash
ollama create continuum-typescript-expert -f Modelfile
```

**Modelfile Structure**:
```
FROM /path/to/base-model.gguf
ADAPTER /path/to/adapter.gguf
TEMPLATE """<|im_start|>system
{{ .System }}<|im_end|>
<|im_start|>user
{{ .Prompt }}<|im_end|>
<|im_start|>assistant
"""
PARAMETER temperature 0.7
```

### Phase 5: Genome Paging

**LoRAAdapter Tracks**:
- Adapter ID: `continuum-typescript-expert`
- Model name in Ollama: `continuum-typescript-expert`
- Domain: `typescript`, `code`, `continuum`
- Last used: timestamp for LRU eviction
- Loaded: boolean state

**Usage**:
```typescript
// PersonaUser detects TypeScript task
await this.genome.activateSkill('typescript-expertise');

// LoRAAdapter loads via Ollama
ollama run continuum-typescript-expert "What changes for: fix null pointer bug?"

// When memory pressure > 80%
await this.genome.evictLRU();  // Removes least-recently-used adapter
```

---

## Universal LoRA Provider Interface

```typescript
/**
 * Universal interface for LoRA fine-tuning across any model type
 */
export interface LoRAProvider {
  /** Provider identification */
  getProviderType(): 'local' | 'cloud' | 'hybrid';
  getName(): string;  // 'mlx', 'openai', 'sentinel'

  /** Fine-tuning */
  fineTune(config: FinetuneConfig): Promise<FinetuneJob>;
  checkFinetuneStatus(jobId: string): Promise<FinetuneStatus>;

  /** Adapter management */
  listAdapters(): Promise<AdapterInfo[]>;
  loadAdapter(adapterId: string): Promise<void>;
  unloadAdapter(adapterId: string): Promise<void>;
  deleteAdapter(adapterId: string): Promise<void>;

  /** Inference */
  generate(prompt: string, options?: GenerateOptions): Promise<string>;

  /** Metadata */
  getSupportedModels(): string[];
  getMaxContextLength(): number;
}

/**
 * Fine-tuning configuration
 */
export interface FinetuneConfig {
  datasetPath: string;         // Path to training data
  baseModel: string;           // 'qwen2.5-coder', 'gpt-3.5-turbo', etc
  adapterId: string;           // 'continuum-typescript-expert'
  targetDomains: string[];     // ['typescript', 'code', 'continuum']

  // Training hyperparameters
  epochs?: number;             // Default: 3
  batchSize?: number;          // Default: 4 (MLX), 1 (OpenAI)
  learningRate?: number;       // Default: 1e-5
  maxTokens?: number;          // Max sequence length

  // Provider-specific
  providerConfig?: Record<string, any>;
}

/**
 * Fine-tuning job tracking
 */
export interface FinetuneJob {
  jobId: string;               // UUID
  provider: string;            // 'mlx', 'openai', 'sentinel'
  status: 'queued' | 'running' | 'completed' | 'failed';
  progress: number;            // 0.0-1.0

  startedAt?: number;
  completedAt?: number;
  estimatedCompletion?: number;

  metrics?: {
    loss: number;
    tokensProcessed: number;
    stepsCompleted: number;
  };

  error?: string;
}
```

---

## Implementation Roadmap

### Phase 1: MLX Local (NEXT - This Week)
- [x] Install MLX tools
- [ ] Convert dataset to MLX format
- [ ] Create test dataset (100 examples)
- [ ] Fine-tune qwen2.5-coder locally
- [ ] Export to Ollama
- [ ] Test genome paging
- [ ] Create `ai/adapter/train` command

### Phase 2: OpenAI Cloud (Next Week)
- [ ] Implement OpenAILoRAProvider
- [ ] Convert dataset to OpenAI format
- [ ] Upload dataset via API
- [ ] Fine-tune gpt-3.5-turbo
- [ ] Test quality vs local

### Phase 3: Sentinel Hybrid (Week 3)
- [ ] Set up Modal/RunPod account
- [ ] Create cloud fine-tuning script
- [ ] Fine-tune on cloud GPUs
- [ ] Download adapter
- [ ] Run locally via Ollama

### Phase 4: Multi-Model Swarm (Week 4)
- [ ] Assign different providers to different PersonaUsers
- [ ] Load balancing across providers
- [ ] Cost/quality/latency optimization
- [ ] Provider failover

---

## File Structure

```
src/debug/jtag/
├── system/
│   └── user/
│       └── server/
│           └── modules/
│               ├── LoRAAdapter.ts                    # Existing (add provider interface)
│               ├── PersonaGenome.ts                  # Existing (genome paging logic)
│               └── providers/
│                   ├── LoRAProvider.ts               # Universal interface
│                   ├── MLXLoRAProvider.ts            # Apple Silicon local
│                   ├── OpenAILoRAProvider.ts         # Cloud API
│                   └── SentinelLoRAProvider.ts       # Hybrid
│
├── commands/
│   └── ai/
│       └── adapter/
│           └── train/
│               ├── shared/
│               │   └── AdapterTrainTypes.ts
│               ├── browser/
│               │   └── AdapterTrainBrowserCommand.ts
│               └── server/
│                   └── AdapterTrainServerCommand.ts  # Async, UUID-tracked
│
├── scripts/
│   ├── convert-to-mlx-format.ts                      # Dataset conversion
│   └── train-mlx-adapter.py                          # MLX training script
│
└── tests/
    ├── unit/
    │   └── lora-providers.test.ts
    └── integration/
        └── mlx-fine-tuning.test.ts
```

---

## Critical Success Factors

### 1. Chat Template Consistency
**Problem**: Most Ollama failures come from mismatched chat templates
**Solution**: Save template with adapter, enforce identical format during inference

### 2. Token Limits
**Problem**: Training examples > 3500 tokens cause OOM
**Solution**: Filter/truncate during dataset preparation

### 3. Quality Metrics
**Problem**: How do we know if fine-tuning improved the model?
**Solution**:
- Perplexity scores (automatic)
- Manual evaluation (sample 20 outputs)
- A/B testing (base vs fine-tuned)

### 4. Adapter Storage
**Problem**: LoRA adapters are ~100MB each, can accumulate quickly
**Solution**:
- LRU eviction in PersonaGenome
- Compress old adapters
- Archive to cold storage after 30 days unused

---

## Next Steps

1. **Convert dataset to MLX format** (scripts/convert-to-mlx-format.ts)
2. **Create 100-example test dataset** for fast iteration
3. **Fine-tune qwen2.5-coder** on test dataset (~5 minutes)
4. **Export to Ollama** and test inference
5. **Validate genome paging** works end-to-end
6. **Scale to full dataset** (1590 examples, ~30 minutes)
7. **Create ai/adapter/train command** with async tracking

**Goal**: End-to-end working system in 2-3 days, then iterate on quality.
