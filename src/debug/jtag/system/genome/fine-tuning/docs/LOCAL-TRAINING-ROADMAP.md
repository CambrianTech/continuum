# Local LoRA/QLoRA Training Roadmap

**Phase 2 Implementation Guide** - Local fine-tuning on Apple Silicon and NVIDIA GPUs

**Resources:**
- VRAM Calculator: https://apxml.com/tools/vram-calculator
- Local LLM Guide: https://apxml.com/posts/best-local-llm-apple-silicon-mac
- MCP Integration: https://apxml.com/mcp

---

## Architecture Overview

### Current (Phase 1): Remote API Training
```typescript
// Simple interface - providers handle optimization
interface LoRATrainingRequest {
  baseModel: string;
  dataset: TrainingDataset;
  epochs: number;
  learningRate: number;
  loraRank: number;
  loraAlpha: number;
}

// Adapters:
- OpenAILoRAAdapter (✅ Working)
- FireworksLoRAAdapter (✅ Working)
- MistralLoRAAdapter (✅ Working)
- TogetherLoRAAdapter (⚠️ File upload issue)
```

### Future (Phase 2): Local Training
```typescript
// Extended interface with optimization controls
interface LoRATrainingRequest {
  // ... existing fields ...

  // Quantization (enables QLoRA)
  baseModelPrecision?: 'fp32' | 'fp16' | 'bf16' | '8bit' | '4bit';

  // Memory optimization
  optimizations?: {
    flashAttention?: boolean;           // ~45% memory reduction
    gradientCheckpointing?: boolean;    // ~70% activation savings, 25% slower
    use8bitOptimizer?: boolean;         // 75% optimizer memory reduction
    pagedOptimizer?: boolean;           // CPU RAM offloading
    fusedKernels?: boolean;             // Kernel fusion speedup
    sequencePacking?: boolean;          // ~25% training speedup
    dynamicPadding?: boolean;           // Reduce wasted computation
    activationOffloading?: boolean;     // Offload activations to CPU
  };

  // Hardware configuration
  hardware?: {
    device?: 'mps' | 'cuda' | 'cpu';   // Metal (Apple), CUDA (NVIDIA), CPU
    numDevices?: number;                // Multi-GPU support
    maxMemoryGB?: number;               // Memory budget
  };

  // Advanced training
  gradientAccumulationSteps?: number;  // Simulate larger batch size
  optimizer?: 'adamw' | 'sgd' | 'adafactor';
}
```

---

## Memory Calculations (from apxml.com/tools/vram-calculator)

### Memory Breakdown:

**Total VRAM = Base Model + LoRA Adapters + Optimizer + Gradients + Activations + Overhead**

#### 1. Base Model Weights
- **FP32**: 4 bytes per parameter
- **FP16/BF16**: 2 bytes per parameter (standard LoRA)
- **8-bit**: 1 byte per parameter (QLoRA)
- **4-bit**: 0.5 bytes per parameter (QLoRA)

Example (Llama 3.1 8B):
- FP16: 8B × 2 = 16 GB
- 8-bit: 8B × 1 = 8 GB
- 4-bit: 8B × 0.5 = 4 GB

#### 2. LoRA Adapters
- **Size**: 2 × rank × hidden_dim × num_layers
- **Always full precision** (FP16/BF16)
- **Tiny**: ~50-200 MB for rank 16-64

Example (Llama 3.1 8B, rank 16):
- 2 × 16 × 4096 × 32 = ~8M parameters
- 8M × 2 bytes = 16 MB

#### 3. Optimizer States (AdamW)
- **2x trainable parameters** (momentum + variance)
- **8-bit optimizer**: 75% reduction
- **Paged optimizer**: CPU RAM offload

Example (LoRA adapters only):
- Standard: 16 MB × 2 = 32 MB
- 8-bit: 16 MB × 0.5 = 8 MB

#### 4. Gradients
- **Same size as trainable parameters**
- Only for LoRA adapters (not base model)

Example: 16 MB

#### 5. Activations (Huge!)
- **Scales with sequence length²**
- **Gradient checkpointing**: Save only checkpoints, recompute rest
- **70% memory savings, 25% speed penalty**

Formula:
```
Activation Memory = batch_size × seq_length × hidden_dim × num_layers × 2 (FP16)
```

Example (Llama 3.1 8B, batch=1, seq=1024):
- No checkpointing: ~4 GB
- With checkpointing: ~1.2 GB

#### 6. Framework Overhead
- **Temp buffers**: ~10-20% of total
- **Multi-GPU overhead**: ~5-10% per additional GPU

---

## Optimization Strategies

### Memory Optimizations:

| Optimization | Memory Savings | Speed Impact | Notes |
|--------------|----------------|--------------|-------|
| **QLoRA (4-bit)** | 75% base model | ~10% slower | Best for limited VRAM |
| **Flash Attention** | ~45% activations | 0-5% faster | Free speedup |
| **Gradient Checkpointing** | ~70% activations | 25% slower | Trade memory for compute |
| **8-bit Optimizer** | 75% optimizer | Minimal | Recommended for all |
| **Paged Optimizer** | Unlimited* | Varies | Use CPU RAM when GPU full |
| **Activation Offloading** | Large | 50%+ slower | Last resort |

*Limited by CPU RAM

### Speed Optimizations:

| Optimization | Speedup | Memory Impact | Notes |
|--------------|---------|---------------|-------|
| **Fused Kernels** | ~10-15% | None | Free speedup |
| **Sequence Packing** | ~25% | None | Remove padding waste |
| **Dynamic Padding** | ~10-20% | None | Pad only to longest in batch |
| **Mixed Precision** | ~2x | None | FP16/BF16 training |
| **Flash Attention** | ~5-10% | -45% memory | Win-win |

---

## Hardware Configurations

### Apple Silicon (M-Series)

**M2 Pro 16GB Example:**
- **Available VRAM**: ~12 GB (70% of unified memory)
- **Recommended Config**:
  - Model: Llama 3.1 8B (4-bit) = 4 GB
  - LoRA rank: 16 = 16 MB
  - Optimizer (8-bit): 8 MB
  - Activations (with checkpointing): 1.2 GB
  - Overhead: 1 GB
  - **Total: ~6.2 GB** ✅ Fits!

**Optimizations for Apple Silicon:**
```typescript
{
  device: 'mps',  // Metal Performance Shaders
  baseModelPrecision: '4bit',
  optimizations: {
    flashAttention: true,
    gradientCheckpointing: true,
    use8bitOptimizer: true,
    fusedKernels: true,
    sequencePacking: true,
    dynamicPadding: true,
  }
}
```

### NVIDIA GPUs

**RTX 4090 24GB Example:**
- **Available VRAM**: ~22 GB
- **Recommended Config**:
  - Model: Llama 3.1 8B (FP16) = 16 GB
  - LoRA rank: 64 = 64 MB
  - Optimizer (8-bit): 32 MB
  - Activations (with checkpointing): 2 GB
  - Overhead: 2 GB
  - **Total: ~20 GB** ✅ Fits!

**Optimizations for NVIDIA:**
```typescript
{
  device: 'cuda',
  baseModelPrecision: 'bf16',  // BF16 preferred on Ampere+
  optimizations: {
    flashAttention: true,
    gradientCheckpointing: true,
    use8bitOptimizer: true,
    fusedKernels: true,
    sequencePacking: true,
  }
}
```

---

## Implementation Frameworks

### Option 1: Unsloth (Recommended for Apple Silicon)
- **Best for**: M-series Macs
- **Supports**: LoRA, QLoRA, full fine-tuning
- **Optimizations**: All major optimizations built-in
- **Models**: Llama, Mistral, Qwen, CodeLlama, etc.

```python
from unsloth import FastLanguageModel

model, tokenizer = FastLanguageModel.from_pretrained(
    model_name="unsloth/llama-3.1-8b-bnb-4bit",
    max_seq_length=2048,
    dtype=None,  # Auto-detect
    load_in_4bit=True,
)

model = FastLanguageModel.get_peft_model(
    model,
    r=16,
    target_modules=["q_proj", "k_proj", "v_proj", "o_proj"],
    lora_alpha=16,
    lora_dropout=0,
    bias="none",
    use_gradient_checkpointing="unsloth",
)
```

### Option 2: MLX (Apple's Framework)
- **Best for**: Latest M-series chips (M3+)
- **Supports**: LoRA, QLoRA
- **Optimizations**: Hardware-specific acceleration
- **Models**: Llama, Mistral, Phi, etc.

```python
from mlx_lm import load, LoRAConfig, train

model, tokenizer = load("mlx-community/Llama-3.1-8B-4bit")

config = LoRAConfig(
    num_layers=32,
    lora_rank=16,
    lora_alpha=16,
)

train(model, tokenizer, config, train_data)
```

### Option 3: HuggingFace PEFT + BitsAndBytes
- **Best for**: NVIDIA GPUs
- **Supports**: LoRA, QLoRA, full control
- **Optimizations**: Manual configuration

```python
from transformers import AutoModelForCausalLM, BitsAndBytesConfig
from peft import LoraConfig, get_peft_model

bnb_config = BitsAndBytesConfig(
    load_in_4bit=True,
    bnb_4bit_quant_type="nf4",
    bnb_4bit_compute_dtype=torch.bfloat16,
    bnb_4bit_use_double_quant=True,
)

model = AutoModelForCausalLM.from_pretrained(
    "meta-llama/Llama-3.1-8B",
    quantization_config=bnb_config,
    device_map="auto",
)

peft_config = LoraConfig(
    r=16,
    lora_alpha=16,
    target_modules=["q_proj", "k_proj", "v_proj", "o_proj"],
    lora_dropout=0.05,
    bias="none",
    task_type="CAUSAL_LM",
)

model = get_peft_model(model, peft_config)
```

---

## LocalLoRAAdapter Implementation

```typescript
import { BaseLoRATrainerServer } from '../BaseLoRATrainerServer';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

/**
 * Local LoRA Adapter - Apple Silicon and NVIDIA GPU training
 *
 * Uses unsloth for Apple Silicon (MPS) or PEFT+BitsAndBytes for NVIDIA (CUDA)
 *
 * Status: 🚧 FUTURE IMPLEMENTATION (Phase 2)
 */
export class LocalLoRAAdapter extends BaseLoRATrainerServer {
  readonly providerId = 'local';

  /**
   * Check if local training is supported
   * Requires Python environment with unsloth or peft+bitsandbytes
   */
  supportsFineTuning(): boolean {
    // Check for Python, torch, and training frameworks
    return this.checkPythonEnvironment();
  }

  /**
   * Get fine-tuning capabilities
   *
   * Local training capabilities (hardware-dependent):
   * - LoRA rank: Configurable (4-256)
   * - Quantization: FP32, FP16, BF16, 8-bit, 4-bit
   * - Full optimization control
   * - No cost (use local hardware)
   * - Speed depends on hardware
   */
  getFineTuningCapabilities(): FineTuningCapabilities {
    const hardware = this.detectHardware();

    return {
      supportsFineTuning: this.supportsFineTuning(),
      strategy: 'local',

      // LoRA parameters (highly configurable)
      minRank: 4,
      maxRank: 256,
      defaultRank: 16,
      minAlpha: 4,
      maxAlpha: 256,
      defaultAlpha: 16,

      // Training parameters
      minEpochs: 1,
      maxEpochs: 100,
      defaultEpochs: 3,
      minLearningRate: 0.00001,
      maxLearningRate: 0.01,
      defaultLearningRate: 0.0001,
      minBatchSize: 1,
      maxBatchSize: 32,
      defaultBatchSize: 1,  // Local often uses batch=1 + gradient accumulation

      // Cost (FREE - uses local hardware)
      costPerExample: 0,

      // Performance (hardware-dependent)
      estimatedTrainingTime: this.estimateSpeed(hardware),

      // Model support (depends on framework)
      supportedBaseModels: this.getSupportedModels(hardware),

      // Hardware info
      metadata: {
        device: hardware.device,
        memoryGB: hardware.memoryGB,
        framework: hardware.preferredFramework,
      },
    };
  }

  /**
   * Start training - Launches local training process
   */
  protected async _startTraining(
    request: LoRATrainingRequest
  ): Promise<TrainingHandle> {
    console.log('🖥️  Local: Starting training on local hardware...');

    // 1. Detect hardware and select framework
    const hardware = this.detectHardware();
    const framework = this.selectFramework(hardware, request);

    // 2. Generate training script
    const scriptPath = await this.generateTrainingScript(framework, request);

    // 3. Launch training process (non-blocking)
    const process = await this.launchTrainingProcess(scriptPath);

    // 4. Return handle
    return {
      jobId: process.pid.toString(),
      metadata: {
        framework,
        device: hardware.device,
        scriptPath,
      },
    };
  }

  /**
   * Query training status - Check local process
   */
  protected async _queryStatus(
    sessionId: UUID,
    providerJobId: string,
    metadata: Record<string, unknown>
  ): Promise<TrainingStatus> {
    // Check if process is still running
    const isRunning = await this.isProcessRunning(providerJobId);

    if (!isRunning) {
      // Check for output artifacts
      const outputPath = this.getOutputPath(sessionId);
      const adapterExists = await this.checkAdapterExists(outputPath);

      if (adapterExists) {
        return {
          status: 'completed',
          modelId: outputPath,
        };
      } else {
        return {
          status: 'failed',
          error: 'Training process exited without creating adapter',
        };
      }
    }

    // Parse training logs for progress
    const progress = await this.parseTrainingLogs(sessionId);

    return {
      status: 'running',
      metadata: {
        currentEpoch: progress.epoch,
        loss: progress.loss,
        samplesProcessed: progress.samples,
      },
    };
  }

  /**
   * Detect available hardware
   */
  private detectHardware(): HardwareInfo {
    // Check for Apple Silicon (MPS)
    // Check for NVIDIA GPU (CUDA)
    // Fallback to CPU
    // Return device, memory, compute capability
  }

  /**
   * Select best framework for hardware
   */
  private selectFramework(
    hardware: HardwareInfo,
    request: LoRATrainingRequest
  ): 'unsloth' | 'mlx' | 'peft' {
    if (hardware.device === 'mps') {
      // Apple Silicon - prefer unsloth or MLX
      return 'unsloth';
    } else if (hardware.device === 'cuda') {
      // NVIDIA - use PEFT + BitsAndBytes
      return 'peft';
    } else {
      // CPU - use unsloth (has CPU support)
      return 'unsloth';
    }
  }

  /**
   * Generate Python training script
   */
  private async generateTrainingScript(
    framework: string,
    request: LoRATrainingRequest
  ): Promise<string> {
    // Generate framework-specific training script
    // Include all optimizations from request
    // Save to temp file
    // Return path
  }
}

interface HardwareInfo {
  device: 'mps' | 'cuda' | 'cpu';
  memoryGB: number;
  computeCapability?: string;
  preferredFramework: 'unsloth' | 'mlx' | 'peft';
}
```

---

## Testing Strategy

### Unit Tests:
```bash
npx vitest tests/unit/LocalLoRAAdapter.test.ts
```

### Integration Tests:
```bash
# Test with small model (Qwen 0.5B)
npx vitest tests/integration/local-training-small.test.ts

# Test with production model (Llama 3.1 8B)
npx vitest tests/integration/local-training-full.test.ts
```

### Hardware Tests:
```bash
# Test on Apple Silicon
./scripts/test-local-training-mps.sh

# Test on NVIDIA GPU
./scripts/test-local-training-cuda.sh

# Test on CPU (slow, for CI only)
./scripts/test-local-training-cpu.sh
```

---

## Performance Benchmarks (from apxml.com)

### Apple Silicon:

| Hardware | Model | Precision | Tokens/sec | Cost/hour |
|----------|-------|-----------|------------|-----------|
| M2 Pro 16GB | Llama 3.1 8B | 4-bit | ~17 | $0.004 (power) |
| M3 Max 48GB | Llama 3.1 8B | FP16 | ~45 | $0.004 (power) |
| M3 Max 48GB | Llama 3.1 70B | 4-bit | ~12 | $0.004 (power) |

### NVIDIA GPUs:

| Hardware | Model | Precision | Tokens/sec | Cost/hour |
|----------|-------|-----------|------------|-----------|
| RTX 4090 24GB | Llama 3.1 8B | BF16 | ~120 | $0.20 (cloud) |
| A100 80GB | Llama 3.1 8B | BF16 | ~200 | $3.00 (cloud) |
| A100 80GB | Llama 3.1 70B | 4-bit | ~40 | $3.00 (cloud) |

**Training Time Estimates (1000 examples, 3 epochs):**
- M2 Pro: ~4-6 hours
- M3 Max: ~1-2 hours
- RTX 4090: ~0.5-1 hour
- A100: ~0.25-0.5 hour

---

## Next Steps

1. **Environment Setup**:
   - Install Python 3.10+
   - Install PyTorch with MPS/CUDA support
   - Install unsloth or mlx-lm
   - Test with small model

2. **Implement LocalLoRAAdapter**:
   - Hardware detection
   - Framework selection
   - Script generation
   - Process management
   - Status monitoring

3. **Add to Command**:
   - Register in GenomeTrainServerCommand.ts
   - Add 'local' provider option
   - Test end-to-end

4. **Documentation**:
   - User guide for setup
   - Hardware requirements
   - Troubleshooting guide

5. **Testing**:
   - Unit tests
   - Integration tests
   - Hardware-specific tests
   - Performance benchmarks

---

## Resources

**Documentation:**
- VRAM Calculator: https://apxml.com/tools/vram-calculator
- Local LLM Guide: https://apxml.com/posts/best-local-llm-apple-silicon-mac
- Unsloth Docs: https://docs.unsloth.ai
- MLX Docs: https://ml-explore.github.io/mlx/build/html/index.html
- PEFT Docs: https://huggingface.co/docs/peft

**Frameworks:**
- Unsloth: https://github.com/unslothai/unsloth
- MLX-LM: https://github.com/ml-explore/mlx-examples/tree/main/lora
- PEFT: https://github.com/huggingface/peft
- BitsAndBytes: https://github.com/TimDettmers/bitsandbytes

**Models:**
- Hugging Face Hub: https://huggingface.co/models
- Unsloth Models: https://huggingface.co/unsloth
- MLX Community: https://huggingface.co/mlx-community

---

**Status**: 📋 PLANNED - Phase 2 (after remote API adapters are stable)
