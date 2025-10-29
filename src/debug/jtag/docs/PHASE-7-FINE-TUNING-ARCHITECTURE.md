# Phase 7: LoRA Fine-Tuning Architecture

**Goal**: Enable PersonaUsers to learn from experience by fine-tuning LoRA adapters based on mistakes and successful patterns.

**Status**: Architecture designed, ready for implementation

---

## Overview

Phase 7 adds **actual ML training** to the LoRA adapter system. Phase 6 (PersonaGenome) provides the management layer (loading, caching, eviction), but adapters are currently mocked. Phase 7 closes the loop by integrating real LoRA fine-tuning.

### The Two Kinds of Adapters

1. **Behavioral Adapters (Pre-trained)**
   - Source: Downloaded or inherited from base models
   - Examples: `typescript-expert`, `rust-expert`, `conversational`
   - Training: Done offline via Academy or external tools
   - Usage: Task-specific skill activation via `genome.activateSkill(trait)`

2. **Memory Adapters (Experience-based)**
   - Source: Fine-tuned from persona's own mistakes/successes
   - Examples: `memory-room-{roomId}`, `memory-coding-style`, `memory-preferences`
   - Training: Continuous, online learning from interaction history
   - Usage: Automatically loaded alongside base model

---

## Architecture Components

### 1. Fine-Tuning Infrastructure (Proper Separation)

```
system/genome/fine-tuning/
├── shared/
│   ├── FineTuningTypes.ts          # Request/response types (environment-agnostic)
│   └── BaseLoRATrainer.ts          # Abstract base class (no Node.js imports!)
└── server/
    ├── TrainingDatasetBuilder.ts   # Build datasets from memories (server-only)
    ├── FineTuningOrchestrator.ts   # Manage training jobs (server-only)
    └── adapters/
        ├── OllamaLoRAAdapter.ts    # Ollama integration (llama.cpp)
        └── PyTorchLoRAAdapter.ts   # PyTorch/Transformers integration

daemons/ai-provider-daemon/
└── adapters/
    ├── ollama/server/              # Each adapter implements fine-tuning in server/
    │   └── OllamaAdapter.ts        # implements LoRATrainer interface
    ├── openai/server/
    │   └── OpenAIAdapter.ts        # implements LoRATrainer interface
    └── deepseek/server/
        └── DeepSeekAdapter.ts      # implements LoRATrainer interface
```

**Key Architecture Rules**:
- ✅ **shared/** = Environment-agnostic types and interfaces ONLY
- ✅ **server/** = Node.js-specific implementations (child_process, fs, etc.)
- ✅ **Static imports** at top of file, NO dynamic imports
- ✅ **Each adapter** implements its own fine-tuning in its server/ directory
- ✅ **BaseLoRATrainer** knows general concepts, not provider names

### 2. Command: `ai/fine-tune`

**Purpose**: Train LoRA adapter from dataset

**Params**:
```typescript
interface FineTuneParams {
  personaId: UUID;
  traitType: TraitType;  // What skill to train
  trainingData: TrainingExample[];  // Input/output pairs
  baseModel: string;  // 'llama3.2:1b', 'phi3:mini', etc.

  // Training hyperparameters
  rank?: number;  // LoRA rank (default: 16)
  alpha?: number;  // LoRA alpha (default: 32)
  epochs?: number;  // Training epochs (default: 3)
  learningRate?: number;  // Learning rate (default: 2e-4)
  batchSize?: number;  // Batch size (default: 4)

  // Output
  outputPath?: string;  // Where to save adapter (default: .continuum/adapters/{personaId}/{traitType}.safetensors)
}

interface TrainingExample {
  input: string;  // User message or context
  output: string;  // Desired response
  weight?: number;  // Training weight (default: 1.0, mistakes get 2.0)
}

interface FineTuneResult {
  success: boolean;
  layerId?: UUID;  // New GenomeLayerEntity ID
  modelPath?: string;  // Path to .safetensors file
  trainingMetrics?: {
    loss: number;
    accuracy?: number;
    epochs: number;
    samples: number;
    trainingTime: number;
  };
  error?: string;
}
```

**Implementation**:
```typescript
export class AIFineTuneServerCommand extends AIFineTuneCommand {
  async execute(params: FineTuneParams): Promise<FineTuneResult> {
    // 1. Validate params and select trainer
    const trainer = FineTuningOrchestrator.getTrainer(params.baseModel);

    // 2. Prepare dataset (convert examples to prompt format)
    const dataset = TrainingDatasetBuilder.build(params.trainingData, params.baseModel);

    // 3. Train LoRA adapter (delegates to OllamaLoRAAdapter or PyTorchLoRAAdapter)
    const result = await trainer.train({
      baseModel: params.baseModel,
      dataset,
      rank: params.rank ?? 16,
      alpha: params.alpha ?? 32,
      epochs: params.epochs ?? 3,
      learningRate: params.learningRate ?? 2e-4,
      batchSize: params.batchSize ?? 4,
      outputPath: params.outputPath || this.getDefaultOutputPath(params)
    });

    // 4. Create GenomeLayerEntity and register with PersonaGenome
    const layer = await this.createGenomeLayer(params, result);

    // 5. Update persona's genome to include new layer
    await this.updatePersonaGenome(params.personaId, layer);

    return {
      success: true,
      layerId: layer.id,
      modelPath: result.modelPath,
      trainingMetrics: result.metrics
    };
  }
}
```

### 3. Training Data Collection

**Source**: Mistakes and corrections stored in database

**Entities**:
```typescript
// database/entities/TrainingCorrectionEntity.ts
export class TrainingCorrectionEntity extends BaseEntity {
  static readonly collection = 'training_corrections';

  personaId!: UUID;
  roomId?: UUID;  // Context where mistake happened
  messageId?: UUID;  // Original message

  // Mistake data
  originalResponse!: string;  // What persona said
  expectedResponse!: string;  // What should have been said
  correctionReason!: string;  // Why it was wrong

  // Classification
  traitType!: TraitType;  // Which skill this trains
  severity!: number;  // 0-1, weight in training

  // Metadata
  correctedBy?: UUID;  // User who provided correction
  correctedAt!: Date;
  isUsedInTraining!: boolean;
}
```

**Collection Methods**:
1. **Explicit correction**: User marks message as incorrect + provides correction
2. **Implicit correction**: User rephrases + persona learns from context
3. **Batch review**: Periodic review of responses, mark good/bad examples

**Example Flow**:
```typescript
// User corrects AI response in chat
const correction = await TrainingCorrectionEntity.create({
  personaId: helperAI.id,
  roomId: generalRoom.id,
  messageId: badResponse.id,
  originalResponse: "I don't know",
  expectedResponse: "TypeScript interfaces define object shapes with property types",
  correctionReason: "Gave up too easily on easy question",
  traitType: 'typescript-expert',
  severity: 0.8,  // Important correction
  correctedBy: user.id,
  correctedAt: new Date(),
  isUsedInTraining: false
});
await correction.save();
```

### 4. Provider-Specific Fine-Tuning Strategies

**CRITICAL INSIGHT**: Fine-tuning works differently for local vs remote models.

#### Strategy A: Local Models (Ollama, Grok-like, llama.cpp)

**Approach**: Train LoRA adapters locally using llama.cpp/PyTorch

**Supported Models**: llama3.2:1b, phi3:mini, qwen2.5:1.5b, grok-like open-source models

**Implementation**:
```typescript
// daemons/ai-provider-daemon/adapters/ollama/shared/OllamaLoRAAdapter.ts
export class OllamaLoRAAdapter {
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // 1. Export training data to JSONL
    const jsonlPath = await this.exportToJSONL(request.dataset);

    // 2. Call llama.cpp training (Ollama uses llama.cpp internally)
    const adapterPath = await this.runLlamaCppFineTune({
      baseModel: request.baseModel,
      trainFile: jsonlPath,
      outputPath: request.outputPath,
      rank: request.rank,
      alpha: request.alpha,
      epochs: request.epochs,
      learningRate: request.learningRate
    });

    // 3. Create Modelfile and register with Ollama
    const modelName = `${request.personaName.toLowerCase()}-${request.traitType}`;
    await this.createOllamaModel(modelName, request.baseModel, adapterPath);

    return {
      success: true,
      modelPath: adapterPath,
      ollamaModelName: modelName,
      metrics: { /* ... */ }
    };
  }

  private async runLlamaCppFineTune(config: FineTuneConfig): Promise<string> {
    // Use llama.cpp's fine-tuning tools
    // https://github.com/ggerganov/llama.cpp/blob/master/examples/finetune/README.md

    const { execSync } = await import('child_process');

    const command = `
      llama-finetune \\
        --model-base ${config.baseModel} \\
        --train-file ${config.trainFile} \\
        --output-file ${config.outputPath} \\
        --lora-rank ${config.rank} \\
        --lora-alpha ${config.alpha} \\
        --epochs ${config.epochs} \\
        --learning-rate ${config.learningRate}
    `.trim();

    execSync(command, { stdio: 'inherit' });

    return config.outputPath;
  }

  private async createOllamaModel(name: string, baseModel: string, adapterPath: string): Promise<void> {
    // Create Modelfile
    const modelfile = `
FROM ${baseModel}
ADAPTER ${adapterPath}
PARAMETER temperature 0.7
    `.trim();

    const modelfilePath = `/tmp/Modelfile-${name}`;
    await fs.writeFile(modelfilePath, modelfile);

    // Register with Ollama
    const { execSync } = await import('child_process');
    execSync(`ollama create ${name} -f ${modelfilePath}`);

    console.log(`✅ Ollama: Created fine-tuned model "${name}"`);
  }
}
```

#### Strategy B: Remote API Models (OpenAI, Anthropic, DeepSeek, etc.)

**Approach**: Use provider's fine-tuning API (upload dataset, monitor training job, use fine-tuned model ID)

**Supported Providers**: OpenAI (gpt-4, gpt-3.5-turbo), Anthropic (Claude fine-tuning API when available), DeepSeek, Groq, Fireworks, Together, XAI

**Key Difference**: We don't train locally - we send training data to provider's API

**Implementation**:
```typescript
// daemons/ai-provider-daemon/adapters/openai/shared/OpenAILoRAAdapter.ts
export class OpenAILoRAAdapter {
  private readonly apiKey: string;
  private readonly baseUrl = 'https://api.openai.com/v1';

  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // 1. Upload training dataset to OpenAI
    const fileId = await this.uploadTrainingFile(request.dataset);

    // 2. Create fine-tuning job
    const job = await this.createFineTuneJob({
      trainingFile: fileId,
      model: request.baseModel,  // 'gpt-3.5-turbo', 'gpt-4', etc.
      hyperparameters: {
        nEpochs: request.epochs || 3,
        learningRateMultiplier: request.learningRate || 'auto',
        batchSize: request.batchSize || 'auto'
      },
      suffix: `${request.personaName}-${request.traitType}`  // Appears in model name
    });

    // 3. Poll job status until complete (can take 10-60 minutes)
    const finishedJob = await this.waitForJobCompletion(job.id);

    // 4. Return fine-tuned model ID (e.g., "ft:gpt-3.5-turbo:my-org:custom_suffix:id")
    return {
      success: true,
      modelId: finishedJob.fineTunedModel,  // Use THIS model ID in future requests
      trainingMetrics: {
        loss: finishedJob.resultFiles.trainLoss,
        epochs: finishedJob.hyperparameters.nEpochs,
        samples: request.dataset.length,
        trainingTime: finishedJob.finishedAt - finishedJob.createdAt
      }
    };
  }

  private async uploadTrainingFile(dataset: TrainingExample[]): Promise<string> {
    // Convert to OpenAI's JSONL format
    const jsonl = dataset.map(ex => JSON.stringify({
      messages: [
        { role: 'system', content: 'You are a helpful assistant.' },
        { role: 'user', content: ex.input },
        { role: 'assistant', content: ex.output }
      ]
    })).join('\n');

    // Upload file
    const response = await fetch(`${this.baseUrl}/files`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        purpose: 'fine-tune',
        file: Buffer.from(jsonl)
      })
    });

    const file = await response.json();
    return file.id;  // file-abc123
  }

  private async createFineTuneJob(config: FineTuneJobConfig): Promise<FineTuneJob> {
    const response = await fetch(`${this.baseUrl}/fine_tuning/jobs`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify(config)
    });

    return await response.json();  // { id: 'ftjob-abc123', status: 'queued', ... }
  }

  private async waitForJobCompletion(jobId: string): Promise<FineTuneJob> {
    console.log(`⏳ OpenAI: Waiting for fine-tune job ${jobId} to complete...`);

    while (true) {
      const job = await this.getFineTuneJob(jobId);

      if (job.status === 'succeeded') {
        console.log(`✅ OpenAI: Fine-tune job completed! Model: ${job.fineTunedModel}`);
        return job;
      }

      if (job.status === 'failed' || job.status === 'cancelled') {
        throw new Error(`Fine-tune job ${jobId} ${job.status}: ${job.error?.message}`);
      }

      // Still running, wait 30 seconds
      await new Promise(resolve => setTimeout(resolve, 30000));
    }
  }

  private async getFineTuneJob(jobId: string): Promise<FineTuneJob> {
    const response = await fetch(`${this.baseUrl}/fine_tuning/jobs/${jobId}`, {
      headers: { 'Authorization': `Bearer ${this.apiKey}` }
    });

    return await response.json();
  }
}
```

**Usage After Fine-Tuning**:
```typescript
// Instead of using base model:
const response = await AIProviderDaemon.generateText({
  model: 'gpt-3.5-turbo',  // Base model
  messages: [...]
});

// Use fine-tuned model:
const response = await AIProviderDaemon.generateText({
  model: 'ft:gpt-3.5-turbo:my-org:helper-ai-typescript-expert:7p4lURogABC123',  // Fine-tuned model ID
  messages: [...]
});
```

#### Strategy C: Hybrid Approach (Best of Both Worlds)

**Approach**: Use local models for privacy-sensitive data, remote models for SOTA performance

**Example**:
```typescript
// PersonaUser decides which approach based on data sensitivity
if (corrections.containsSensitiveData()) {
  // Use local Ollama fine-tuning (data never leaves machine)
  await this.fineTuneLocally(corrections);
} else {
  // Use OpenAI fine-tuning (better performance, but data sent to API)
  await this.fineTuneRemotely(corrections);
}
```

#### Adapter Selection Logic (Elegant Pattern)

**CRITICAL**: Use the existing AIProviderDaemon adapter registry, don't hard-code provider names!

```typescript
// system/genome/fine-tuning/server/FineTuningOrchestrator.ts
export class FineTuningOrchestrator {
  /**
   * Get fine-tuning trainer for a model
   * Uses AIProviderDaemon's adapter registry - each adapter knows if it supports fine-tuning
   */
  static async getTrainer(model: string): Promise<LoRATrainer> {
    // Ask AIProviderDaemon which provider handles this model
    const provider = await AIProviderDaemon.getProviderForModel(model);

    if (!provider) {
      throw new Error(`No provider found for model: ${model}`);
    }

    // Check if provider's adapter supports fine-tuning
    const adapter = provider.adapter;
    if (adapter.supportsFineTuning && adapter.supportsFineTuning()) {
      // Provider handles its own fine-tuning (e.g., OpenAI, DeepSeek APIs)
      return adapter as LoRATrainer;
    }

    // Fallback: Use local fine-tuning if model is llama.cpp compatible
    if (provider.providerId === 'ollama') {
      return new OllamaLoRAAdapter();
    }

    throw new Error(`Provider ${provider.providerId} does not support fine-tuning for model: ${model}`);
  }
}
```

**How This Works**:

1. **Each adapter declares its capabilities**:
```typescript
// daemons/ai-provider-daemon/adapters/deepseek/shared/DeepSeekAdapter.ts
export class DeepSeekAdapter extends BaseAIProviderAdapter implements LoRATrainer {
  readonly providerId = 'deepseek';
  readonly providerName = 'DeepSeek';
  readonly supportedCapabilities = ['text-generation', 'chat', 'fine-tuning'];  // NEW

  supportsFineTuning(): boolean {
    return true;  // DeepSeek has fine-tuning API
  }

  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // DeepSeek-specific fine-tuning implementation
    // Similar to OpenAI approach (upload dataset, create job, poll status)
    return this.fineTuneViaAPI(request);
  }
}
```

2. **OllamaAdapter declares local fine-tuning**:
```typescript
// daemons/ai-provider-daemon/adapters/ollama/shared/OllamaAdapter.ts
export class OllamaAdapter extends BaseAIProviderAdapter implements LoRATrainer {
  readonly providerId = 'ollama';
  readonly providerName = 'Ollama';
  readonly supportedCapabilities = ['text-generation', 'chat', 'fine-tuning'];

  supportsFineTuning(): boolean {
    return true;  // Ollama supports local fine-tuning via llama.cpp
  }

  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    // Local llama.cpp fine-tuning
    return this.fineTuneLocally(request);
  }
}
```

3. **Adapters that don't support fine-tuning**:
```typescript
// daemons/ai-provider-daemon/adapters/anthropic/shared/AnthropicAdapter.ts
export class AnthropicAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'anthropic';
  readonly providerName = 'Anthropic';
  readonly supportedCapabilities = ['text-generation', 'chat'];  // NO fine-tuning yet

  supportsFineTuning(): boolean {
    return false;  // Anthropic doesn't have public fine-tuning API yet
  }

  // No trainLoRA() method
}
```

**Benefits**:
- ✅ **No hard-coded provider names** in orchestrator
- ✅ **Each adapter declares its own capabilities**
- ✅ **DeepSeek knows about "deepseek", orchestrator doesn't**
- ✅ **Easy to add new providers** - just implement LoRATrainer interface
- ✅ **Consistent with existing AIProviderDaemon architecture**

**Model-to-Provider Resolution**:
```typescript
// AIProviderDaemon already has this logic
export class AIProviderDaemon {
  static async getProviderForModel(model: string): Promise<ProviderRegistration | null> {
    // Check each registered adapter's supported models
    for (const [providerId, registration] of this.adapters.entries()) {
      const models = await registration.adapter.getAvailableModels();
      if (models.some(m => m.id === model || m.name === model)) {
        return registration;
      }
    }

    return null;
  }
}
```

### 5. Continuous Learning Loop

**Trigger**: Periodic background task (every 24 hours or N corrections)

```typescript
// system/user/server/PersonaUser.ts
private async startFineTuningMonitor(): Promise<void> {
  setInterval(async () => {
    // Check if enough corrections accumulated
    const corrections = await this.getUnusedCorrections();

    if (corrections.length >= 10) {  // Min batch size
      console.log(`🧬 ${this.displayName}: ${corrections.length} corrections accumulated, starting fine-tuning...`);

      // Group by traitType
      const byTrait = this.groupCorrectionsByTrait(corrections);

      for (const [traitType, examples] of byTrait.entries()) {
        await this.fineTuneAdapter(traitType, examples);
      }
    }
  }, 24 * 60 * 60 * 1000);  // Check every 24 hours
}

private async fineTuneAdapter(traitType: TraitType, corrections: TrainingCorrectionEntity[]): Promise<void> {
  // Build training dataset
  const trainingData = corrections.map(c => ({
    input: c.originalContext || c.originalResponse,  // What led to mistake
    output: c.expectedResponse,  // What should have been said
    weight: c.severity * 2  // Mistakes weighted higher
  }));

  // Call ai/fine-tune command
  const result = await Commands.execute<FineTuneParams, FineTuneResult>('ai/fine-tune', {
    personaId: this.id,
    traitType,
    trainingData,
    baseModel: this.genome?.baseModel || 'llama3.2:1b',
    rank: 16,
    alpha: 32,
    epochs: 3,
    learningRate: 2e-4
  });

  if (result.success) {
    console.log(`✅ ${this.displayName}: Fine-tuned ${traitType} adapter (${result.trainingMetrics?.loss} loss)`);

    // Mark corrections as used
    await this.markCorrectionsUsed(corrections);

    // Reload genome to use new adapter
    await this.genome.reloadLayers();
  }
}
```

---

## Testing Strategy

### Unit Tests

**File**: `tests/unit/LoRAFineTuning.test.ts`

```typescript
describe('LoRA Fine-Tuning', () => {
  describe('TrainingDatasetBuilder', () => {
    it('should convert corrections to training examples');
    it('should apply severity weights');
    it('should format for llama.cpp');
    it('should handle empty datasets');
  });

  describe('OllamaLoRAAdapter', () => {
    it('should export dataset to JSONL');
    it('should call llama-finetune with correct params');
    it('should create Ollama Modelfile');
    it('should register model with Ollama');
    it('should handle training failures gracefully');
  });

  describe('FineTuningOrchestrator', () => {
    it('should select correct trainer for model');
    it('should create GenomeLayerEntity after training');
    it('should update PersonaGenome with new layer');
    it('should track training metrics');
  });
});
```

### Integration Tests

**File**: `tests/integration/fine-tuning-e2e.test.ts`

```typescript
describe('LoRA Fine-Tuning End-to-End', () => {
  it('should train adapter from corrections', async () => {
    // 1. Create persona
    const persona = await createTestPersona();

    // 2. Add training corrections
    const corrections = await addTestCorrections(persona.id, 'typescript-expert', 10);

    // 3. Trigger fine-tuning
    const result = await jtag.commands['ai/fine-tune']({
      personaId: persona.id,
      traitType: 'typescript-expert',
      trainingData: corrections.map(c => ({
        input: c.originalResponse,
        output: c.expectedResponse,
        weight: c.severity
      })),
      baseModel: 'llama3.2:1b',
      rank: 8,  // Small for fast test
      epochs: 1  // Single epoch for speed
    });

    expect(result.success).toBe(true);
    expect(result.layerId).toBeDefined();
    expect(result.modelPath).toContain('.safetensors');

    // 4. Verify adapter is loaded in genome
    const genome = await PersonaGenome.findByPersonaId(persona.id);
    const layer = genome.layers.find(l => l.traitType === 'typescript-expert');
    expect(layer).toBeDefined();
    expect(layer.source).toBe('trained');

    // 5. Verify adapter improves responses (qualitative)
    const response = await persona.generateResponse({
      prompt: 'What are TypeScript interfaces?',
      useAdapter: 'typescript-expert'
    });

    expect(response).toContain('object shapes');  // From training data
  });

  it('should handle continuous learning loop', async () => {
    // Test 24-hour background task
    // Mock timer to speed up test
  });

  it('should prevent duplicate training on same corrections', async () => {
    // Verify isUsedInTraining flag works
  });
});
```

---

## Implementation Phases

### Phase 7.0: Ollama LoRA Integration (MVP)
- ✅ TrainingDatasetBuilder
- ✅ OllamaLoRAAdapter (llama.cpp)
- ✅ ai/fine-tune command
- ✅ Unit tests for training pipeline
- ✅ Integration test for end-to-end training

### Phase 7.1: Continuous Learning
- ✅ TrainingCorrectionEntity
- ✅ Background monitoring loop in PersonaUser
- ✅ Automatic fine-tuning when corrections accumulated
- ✅ Integration with PersonaGenome

### Phase 7.2: Advanced Features
- ✅ PyTorchLoRAAdapter (distributed training)
- ✅ Multi-GPU support
- ✅ Hyperparameter optimization
- ✅ A/B testing of adapters
- ✅ Genome merging (combine multiple LoRA adapters)

---

## Key Decisions

1. **llama.cpp for MVP**: Ollama uses llama.cpp internally, so we can leverage existing tooling
2. **Safetensors format**: Industry standard for LoRA weights, portable across tools
3. **Corrections-based training**: User feedback is the gold standard for improvement
4. **Continuous learning**: Fine-tune in background, don't block user interactions
5. **Trait-specific adapters**: Train separate adapters per skill domain
6. **Low rank (16)**: Balance between quality and memory footprint

---

## Open Questions

1. **How to handle model updates?** If Ollama updates llama3.2:1b, do we retrain all adapters?
2. **Adapter versioning?** Track which base model version adapter was trained on?
3. **Negative examples?** Include good responses (don't change these) alongside corrections?
4. **Transfer learning?** Can adapters trained on one persona transfer to another?
5. **Catastrophic forgetting?** Does new training erase old knowledge?

---

## References

- llama.cpp fine-tuning: https://github.com/ggerganov/llama.cpp/tree/master/examples/finetune
- LoRA paper: https://arxiv.org/abs/2106.09685
- Ollama Modelfile: https://github.com/ollama/ollama/blob/main/docs/modelfile.md
- Safetensors: https://huggingface.co/docs/safetensors/index
