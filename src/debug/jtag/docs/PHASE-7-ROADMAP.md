# Phase 7 Roadmap: LoRA Fine-Tuning Implementation

**Vision**: Free local training by default, API options for scale. Test all forms before alpha release.

## Philosophy Alignment

**"Entirely for free (electricity only)"** - Primary focus on Unsloth local training
**"Handle SOTA models"** - Support Llama 4, DeepSeek-R1, Qwen3, Gemma 3, Phi-4
**"Common developer strategies"** - What developers with RTX 3060+, Mac M1+ already use
**"Test all these forms"** - Integration tests for Unsloth, DeepSeek, OpenAI
**"Go forward intelligently"** - Document architecture first, then implement with tests

---

## Phase 7.0 MVP Status: COMPLETE ✅

**What Exists**:
- ✅ TrainingDatasetBuilder.ts (407 lines) - Chat → training dataset converter
- ✅ TrainingDatasetBuilder.test.ts (649 lines, 25/27 passing)
- ✅ BaseLoRATrainer.ts (abstract adapter pattern)
- ✅ OllamaLoRAAdapter.ts (339 lines, returns false for supportsFineTuning())
- ✅ GenomeManager.ts (652 lines, GPU orchestrator with paging)
- ✅ FineTuningTypes.ts (universal types for all approaches)
- ✅ GENOME-MANAGER-INTEGRATION.md (AIProviderDaemon integration strategy)
- ✅ FINE-TUNING-STRATEGY.md (comprehensive research on all approaches)

**Critical Discovery**: Ollama is inference-only, cannot train models. OllamaLoRAAdapter correctly returns `supportsFineTuning() = false`.

**Actual Training Workflow**: Train with Unsloth → Export to GGUF → Serve with Ollama

---

## Phase 7.1: Unsloth Local Training (PRIMARY)

**Goal**: Free local training for developers with GPUs (RTX 3060+, Mac M1+)

### Architecture First

**NEW: system/genome/fine-tuning/server/adapters/UnslothLoRAAdapter.ts**
```typescript
export class UnslothLoRAAdapter extends BaseLoRATrainer {
  readonly providerId = 'unsloth';

  supportsFineTuning(): boolean {
    return this.checkPythonEnvironment() && this.checkGPUAvailable();
  }

  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      minRank: 8, maxRank: 256, defaultRank: 32,
      costPerExample: 0, // Free (electricity only)
      estimatedTrainingTime: 50, // 50ms per example per epoch (GPU)
      supportedBaseModels: undefined, // All models supported
      requiresGPU: true,
      requiresInternet: false
    };
  }

  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // 1. Validate Python environment (unsloth installed)
    // 2. Export dataset to temp JSONL file
    // 3. Spawn Python training process (child_process)
    // 4. Monitor training progress (parse stdout)
    // 5. Wait for completion (GGUF export)
    // 6. Move adapter to .continuum/genome/adapters/
    // 7. Create Ollama Modelfile
    // 8. Register with Ollama (ollama create)
    // 9. Return result with metrics
  }

  private async checkPythonEnvironment(): Promise<boolean> {
    // exec('python3 -c "import unsloth"')
  }

  private async checkGPUAvailable(): Promise<boolean> {
    // Check CUDA (nvidia-smi) or Metal (Mac detection)
  }

  private async spawnTrainingProcess(
    datasetPath: string,
    request: LoRATrainingRequest
  ): Promise<ChildProcess> {
    // spawn('python3', ['train_lora.py', '--dataset', datasetPath, ...])
  }

  private parseTrainingProgress(stdout: string): TrainingProgress {
    // Parse: "Epoch 2/3, Loss: 0.54, Progress: 67%"
  }
}
```

**NEW: system/genome/fine-tuning/server/scripts/train_lora.py**
```python
#!/usr/bin/env python3
"""
Unsloth LoRA training script for Continuum PersonaUsers.

Usage:
  python3 train_lora.py \\
    --dataset /path/to/dataset.jsonl \\
    --base-model llama-3.2-3b \\
    --rank 32 \\
    --epochs 3 \\
    --output /path/to/output.gguf

Outputs:
  - Training logs to stdout (parsed by Node.js)
  - Final GGUF adapter to --output path
"""

import argparse
import json
from unsloth import FastLanguageModel

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--dataset', required=True)
    parser.add_argument('--base-model', required=True)
    parser.add_argument('--rank', type=int, default=32)
    parser.add_argument('--alpha', type=int, default=32)
    parser.add_argument('--epochs', type=int, default=3)
    parser.add_argument('--learning-rate', type=float, default=0.0001)
    parser.add_argument('--batch-size', type=int, default=4)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()

    # Load model in 4-bit
    print(f"Loading base model: {args.base_model}")
    model, tokenizer = FastLanguageModel.from_pretrained(
        args.base_model,
        max_seq_length=2048,
        load_in_4bit=True
    )

    # Add LoRA adapters
    print(f"Adding LoRA adapters (rank={args.rank}, alpha={args.alpha})")
    model = FastLanguageModel.get_peft_model(
        model,
        r=args.rank,
        lora_alpha=args.alpha,
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj"],
        lora_dropout=0.05,
        bias="none"
    )

    # Load dataset
    print(f"Loading dataset: {args.dataset}")
    with open(args.dataset) as f:
        dataset = [json.loads(line) for line in f]

    # Train
    print(f"Training for {args.epochs} epochs...")
    # ... training loop with progress reporting ...
    for epoch in range(args.epochs):
        print(f"Epoch {epoch+1}/{args.epochs}, Loss: {loss:.4f}, Progress: {progress}%")

    # Export to GGUF
    print(f"Exporting to GGUF: {args.output}")
    model.save_pretrained_gguf(
        args.output,
        tokenizer,
        quantization_method="q8_0"
    )

    print("Training complete!")

if __name__ == '__main__':
    main()
```

### Implementation Steps (Phase 7.1)

#### Step 1: Python Environment Setup
- **File**: `system/genome/fine-tuning/server/setup/install_unsloth.sh`
- **Purpose**: Automated Unsloth installation script
- **Testing**: Run on Mac M1+, RTX 3060+ with CUDA

```bash
#!/bin/bash
# Install Unsloth for local LoRA training

echo "Installing Unsloth..."
pip3 install unsloth

echo "Verifying installation..."
python3 -c "import unsloth; print('✅ Unsloth installed successfully')"
```

#### Step 2: Implement UnslothLoRAAdapter
- **File**: `system/genome/fine-tuning/server/adapters/UnslothLoRAAdapter.ts`
- **Dependencies**: child_process, fs, path
- **Tests**: Mock Python process, test progress parsing

#### Step 3: Implement Python Training Script
- **File**: `system/genome/fine-tuning/server/scripts/train_lora.py`
- **Features**: Progress reporting, GGUF export, error handling
- **Tests**: Run with tiny dataset (10 examples), verify GGUF creation

#### Step 4: Ollama Model Registration
- **Integration**: After training, create Modelfile and register
- **Command**: `ollama create persona-{personaId}-{trait} -f Modelfile`
- **Testing**: Verify model loads and responds correctly

#### Step 5: GenomeManager Integration
- **Update**: GenomeManager.submitTrainingJob() calls UnslothLoRAAdapter
- **Testing**: Submit job, verify GPU tracking, check adapter paging

#### Step 6: Integration Tests
- **File**: `tests/integration/unsloth-training.test.ts`
- **Tests**:
  - Train Phi-3 Mini (3.8B) on 10 examples
  - Verify GGUF created in .continuum/genome/adapters/
  - Load adapter in Ollama
  - Generate response with fine-tuned model
  - Verify response quality improved

### Testing Strategy (Phase 7.1): Integration First, Optimization Later

**Philosophy**: "Support and integration testing adapters, then model size and allocation are their own beast"

**Primary Goal**: Prove each adapter can complete the training cycle end-to-end

#### Test 1: Unsloth Local (Simplest Possible)
- **Model**: Whatever Unsloth defaults to (likely smallest available)
- **Dataset**: 5 examples (absolute minimum)
- **Expected Time**: Don't care - just needs to finish
- **Success Criteria**:
  - ✅ Python script runs without crashing
  - ✅ GGUF file created (any size)
  - ✅ Ollama can load the GGUF
  - ✅ Model generates SOME response (quality doesn't matter)

#### Test 2: DeepSeek API (If Available)
- **Model**: Whatever DeepSeek supports for fine-tuning
- **Dataset**: 10 examples (API minimum)
- **Expected Time**: Don't care - just needs to finish
- **Success Criteria**:
  - ✅ Dataset uploads successfully
  - ✅ Training job starts
  - ✅ Job completes without errors
  - ✅ Fine-tuned model accessible via API

#### Test 3: OpenAI API
- **Model**: gpt-4o-mini (cheapest)
- **Dataset**: 10 examples (API minimum)
- **Expected Time**: ~10 minutes (OpenAI processing)
- **Success Criteria**:
  - ✅ Dataset uploads successfully
  - ✅ Training job starts
  - ✅ Job completes without errors
  - ✅ Fine-tuned model accessible via API

**What We're NOT Testing Yet**:
- Model quality or accuracy
- Training speed optimization
- GPU memory efficiency
- Cost optimization
- Multiple models simultaneously

**Next Phase** (after all adapters work): Optimize model sizes, GPU allocation, training speed

---

## Phase 7.2: DeepSeek API Training (SECONDARY)

**Goal**: Cloud training for users without GPUs (27x cheaper than OpenAI)

### Research First

**CRITICAL**: DeepSeek fine-tuning API pricing NOT publicly available yet.

**Options**:
1. Contact DeepSeek directly for fine-tuning API access
2. Use Together AI as intermediary (hosts DeepSeek with fine-tuning)
3. Wait for public fine-tuning API release

### Architecture (Pending API Availability)

**NEW: system/genome/fine-tuning/server/adapters/DeepSeekLoRAAdapter.ts**
```typescript
export class DeepSeekLoRAAdapter extends BaseLoRATrainer {
  readonly providerId = 'deepseek';

  supportsFineTuning(): boolean {
    return this.checkAPIKeyAvailable() && this.checkFineTuningEnabled();
  }

  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      minRank: 8, maxRank: 128, defaultRank: 32,
      costPerExample: 0.000002, // Estimated: $2/1M training tokens
      estimatedTrainingTime: 300, // API processing time
      supportedBaseModels: ['deepseek-r1', 'deepseek-v3'],
      requiresGPU: false,
      requiresInternet: true
    };
  }

  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // 1. Export dataset to JSONL
    // 2. Upload to DeepSeek API
    // 3. Create fine-tuning job
    // 4. Poll for completion (or webhook)
    // 5. Download adapter (if supported)
    // 6. Convert to GGUF (if needed)
    // 7. Return result with cost tracking
  }
}
```

### Implementation Steps (Phase 7.2)

#### Step 1: Research DeepSeek Fine-Tuning API
- Contact DeepSeek for API access
- Document API endpoints and pricing
- Test with small dataset

#### Step 2: Implement DeepSeekLoRAAdapter (if API available)
- Follow similar pattern to OpenAI adapter
- Add cost tracking and budget limits
- Support webhook or polling for job status

#### Step 3: Alternative: Together AI Integration
- If DeepSeek direct API unavailable, use Together AI
- Together AI hosts DeepSeek models with fine-tuning support
- Document pricing and integration

#### Step 4: Integration Tests
- **File**: `tests/integration/deepseek-training.test.ts`
- Upload 100-example dataset
- Monitor training progress
- Verify cost tracking
- Test inference with fine-tuned model

---

## Phase 7.3: OpenAI API Training (ENTERPRISE)

**Goal**: Premium cloud training for enterprise customers (most reliable)

### Architecture

**NEW: system/genome/fine-tuning/server/adapters/OpenAILoRAAdapter.ts**
```typescript
export class OpenAILoRAAdapter extends BaseLoRATrainer {
  readonly providerId = 'openai';

  supportsFineTuning(): boolean {
    return this.checkAPIKeyAvailable();
  }

  getFineTuningCapabilities(): FineTuningCapabilities {
    return {
      minRank: 8, maxRank: 128, defaultRank: 32,
      costPerExample: 0.000025, // $25/1M training tokens
      estimatedTrainingTime: 600, // API processing time (10 min)
      supportedBaseModels: ['gpt-4o', 'gpt-4o-mini'],
      requiresGPU: false,
      requiresInternet: true
    };
  }

  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // 1. Export dataset to JSONL
    // 2. Upload to OpenAI Files API
    // 3. Create fine-tuning job
    // 4. Poll for completion (OpenAI API)
    // 5. Return fine-tuned model ID
    // 6. Track cost and usage
    // Note: Fine-tuned models stay on OpenAI servers
  }
}
```

### Implementation Steps (Phase 7.3)

#### Step 1: Implement OpenAILoRAAdapter
- **API**: OpenAI Fine-Tuning API
- **Docs**: https://platform.openai.com/docs/guides/fine-tuning
- **Cost Tracking**: $25/1M training tokens (GPT-4o)

#### Step 2: File Upload and Job Management
- Upload JSONL via Files API
- Create fine-tuning job with hyperparameters
- Poll job status until completion
- Handle errors and retries

#### Step 3: Integration with AIProviderDaemon
- Fine-tuned models accessed via OpenAI API
- No local GGUF files (models stay on OpenAI servers)
- Update OpenAIAdapter to support fine-tuned model IDs

#### Step 4: Integration Tests
- **File**: `tests/integration/openai-training.test.ts`
- Upload 100-example dataset
- Create fine-tuning job (GPT-4o-mini for cost)
- Wait for completion (~10 minutes)
- Test inference with fine-tuned model
- Verify cost tracking ($0.30-0.45 for 100 examples)

---

## Phase 7.4: Multi-Provider Strategy

**Goal**: Automatic adapter selection based on user configuration

### User Configuration

**NEW: .continuum/config/genome.json**
```json
{
  "fineTuning": {
    "defaultStrategy": "local-first",
    "strategies": {
      "local-only": {
        "adapters": ["unsloth"],
        "fallback": null
      },
      "local-first": {
        "adapters": ["unsloth", "deepseek", "openai"],
        "fallback": "cloud"
      },
      "cloud-only": {
        "adapters": ["deepseek", "openai"],
        "fallback": null
      },
      "cost-optimized": {
        "adapters": ["unsloth", "deepseek"],
        "fallback": "openai"
      }
    },
    "budget": {
      "maxCostPerTraining": 5.00,
      "maxCostPerMonth": 50.00
    }
  }
}
```

### Adapter Selection Logic

**GenomeManager.selectAdapter(request: LoRATrainingRequest): LoRATrainer**
```typescript
selectAdapter(request: LoRATrainingRequest): LoRATrainer {
  const strategy = this.config.defaultStrategy;
  const adapters = this.config.strategies[strategy].adapters;

  for (const adapterId of adapters) {
    const adapter = this.adapters.get(adapterId);

    // Check if adapter supports fine-tuning
    if (!adapter.supportsFineTuning()) continue;

    // Check budget constraints
    const cost = adapter.estimateTrainingCost(request.dataset.examples.length);
    if (cost > this.config.budget.maxCostPerTraining) continue;

    // Check model support
    const caps = adapter.getFineTuningCapabilities();
    if (caps.supportedBaseModels && !caps.supportedBaseModels.includes(request.baseModel)) {
      continue;
    }

    // This adapter works!
    return adapter;
  }

  throw new Error('No suitable fine-tuning adapter available');
}
```

### Implementation Steps (Phase 7.4)

#### Step 1: Configuration System
- Load genome.json config
- Validate strategy and budget settings
- Support environment variables for API keys

#### Step 2: Adapter Selection Logic
- Implement selectAdapter() in GenomeManager
- Add budget tracking (cost per training, monthly total)
- Fallback chain: local → cloud (DeepSeek) → premium (OpenAI)

#### Step 3: Cost Tracking Dashboard
- **Command**: `./jtag genome/training/stats`
- Show training history, costs, model performance
- Budget alerts when approaching limits

#### Step 4: Integration Tests
- Test all selection strategies
- Verify fallback behavior
- Test budget enforcement

---

## Testing Matrix (All Phases)

| Approach | Test Model | Dataset Size | GPU Required | Expected Cost | Expected Time | Success Metric |
|----------|-----------|--------------|--------------|---------------|---------------|----------------|
| **Unsloth** | Phi-3 Mini (3.8B) | 10 examples | RTX 3060+ | $0.01 | < 1 min | GGUF created, loads in Ollama |
| **Unsloth** | Llama 3.2 (8B) | 100 examples | RTX 4090 | $0.05 | ~10 min | Fine-tuned responses match style |
| **DeepSeek** | DeepSeek-R1 (14B) | 100 examples | None | $0.30 | ~5 min | API returns model, inference works |
| **OpenAI** | GPT-4o-mini | 100 examples | None | $0.45 | ~10 min | Fine-tuned model accessible via API |

**Integration Test File**: `tests/integration/genome-training-all-adapters.test.ts`

---

## Documentation Updates

### NEW: docs/GENOME-TRAINING-GUIDE.md
User-facing guide:
- How to install Unsloth (Phase 7.1)
- How to train your first PersonaUser
- Cost comparison (local vs cloud)
- Troubleshooting common issues

### UPDATE: CLAUDE.md
Add section:
```markdown
## 🧬 GENOME: FINE-TUNING PERSONAUSERS

PersonaUsers can evolve through LoRA fine-tuning on your conversations.

**Primary Approach**: Unsloth Local Training (FREE)
- Install: `pip3 install unsloth`
- Train: `./jtag genome/train --personaId=<ID> --trait=conversational`
- Cost: ~$0.02 per training run (electricity only)

**Cloud Options**:
- DeepSeek API: 27x cheaper than OpenAI
- OpenAI API: Premium reliability

See: docs/GENOME-TRAINING-GUIDE.md, docs/FINE-TUNING-STRATEGY.md
```

---

## Phase 7.5: Academy Integration (Future)

**Goal**: PersonaUsers train on Academy exercises, not just chat

**Strategy**:
- Build datasets from training sessions (not chat)
- Test exercises with graded feedback
- Measure learning progress via accuracy improvement

**Future Work**: Coordinate with Academy development

---

## Phase 7.6: Genome Layer Stacking (Future)

**Goal**: Multiple LoRA adapters per PersonaUser (trait specialization)

**Architecture**:
```
PersonaUser
├── conversational.gguf (base communication style)
├── coding.gguf (TypeScript expertise)
├── domain_expert.gguf (React architecture)
└── personality.gguf (humor, empathy)
```

**Challenge**: Ollama doesn't support multiple LoRA adapters yet
**Solution**: Wait for Ollama feature, or merge adapters externally

---

## Success Criteria (Alpha Release)

**Minimum Viable Genome (Phase 7.1)**:
- ✅ Unsloth local training works on Phi-3 Mini, Llama 3.2
- ✅ Integration test passes (train + inference)
- ✅ Cost: Free (electricity only)
- ✅ Documentation: User guide for installation and training

**Full Strategy (Phase 7.4)**:
- ✅ All three adapters implemented (Unsloth, DeepSeek, OpenAI)
- ✅ Automatic adapter selection based on user config
- ✅ Budget tracking and enforcement
- ✅ Integration tests for all adapters

**Philosophy Alignment**:
- ✅ "Entirely for free" - Unsloth primary approach
- ✅ "Handle SOTA models" - Llama 4, DeepSeek-R1, Qwen3 supported
- ✅ "Test all forms" - Integration tests for each adapter
- ✅ "Go forward intelligently" - Architecture documented first

---

## Current Status: Ready for Phase 7.1 Implementation

**Phase 7.0 MVP**: COMPLETE ✅
**Phase 7.1 Next Steps**:
1. Install Unsloth on development machine
2. Implement UnslothLoRAAdapter.ts
3. Write train_lora.py script
4. Test with Phi-3 Mini (10 examples)
5. Write integration test
6. Document user guide

**Estimated Time**: 2-3 days for Phase 7.1 MVP

Let's build it! 🧬🚀
