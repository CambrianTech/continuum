# Fine-Tuning System Architecture

**Status**: Design Phase
**Created**: 2025-01-15
**Last Updated**: 2025-01-15

## Overview

This document defines the complete architecture for the fine-tuning system, including:
- Type definitions and enums (single source of truth)
- Entity schema design
- Command interfaces
- Provider adapter contracts
- Widget integration patterns

## Design Principles

### 1. Single Source of Truth for Types
All enums, constants, and type definitions live in **one place** and flow unidirectionally:

```
FineTuningTypes.ts
      ↓
FineTuningJobEntity.ts
      ↓
Command Schemas (genome/job-create)
      ↓
Provider Adapters
      ↓
Widget Configurations
```

**Why**: Prevents inconsistencies between entity storage, command validation, adapter translation, and UI rendering.

### 2. Universal Configuration at Boundaries
Commands and entities use a **provider-agnostic schema** that covers all common parameters across providers.

**Why**: Allows switching providers without changing stored configs, enables comparison across providers, simplifies widget logic.

### 3. Provider Adapters Handle Translation
Each provider adapter knows how to:
- Map universal schema → provider-specific API format
- Report capabilities (which params it supports)
- Validate parameter combinations for that provider

**Why**: Isolates provider-specific quirks, makes adding new providers straightforward.

### 4. Extensibility Through Metadata
Core parameters are typed and validated. Edge cases use `metadata` field.

**Why**: Type safety for common cases, flexibility for provider-specific features.

## Type Definitions

### Location
`daemons/data-daemon/shared/entities/FineTuningTypes.ts`

### Core Enums

```typescript
/**
 * Training method selection
 * Used by: Entity, Command, Widget dropdown
 */
export enum TrainingMethod {
  FULL = 'full',         // Full fine-tuning (all weights)
  LORA = 'lora',         // Low-Rank Adaptation
  QLORA = 'qlora'        // Quantized LoRA (4-bit/8-bit)
}

/**
 * Learning rate scheduler types
 * Used by: Entity, Command, Widget dropdown
 */
export enum LRSchedulerType {
  COSINE = 'cosine',
  LINEAR = 'linear',
  CONSTANT = 'constant',
  CONSTANT_WITH_WARMUP = 'constant_with_warmup',
  POLYNOMIAL = 'polynomial'
}

/**
 * Model precision for training
 * Used by: Entity, Command, Widget dropdown
 */
export enum ModelPrecision {
  FP32 = 'fp32',         // Full precision
  FP16 = 'fp16',         // Half precision
  BF16 = 'bf16',         // Brain float 16
  FP8 = 'fp8',           // 8-bit float (limited support)
  INT8 = 'int8',         // 8-bit integer (QLoRA)
  INT4 = 'int4'          // 4-bit integer (QLoRA)
}

/**
 * Memory/performance optimization features
 * Used by: Entity, Command, Widget toggles
 */
export enum OptimizationFeature {
  FLASH_ATTENTION = 'flash_attention',
  GRADIENT_CHECKPOINTING = 'gradient_checkpointing',
  OPTIMIZER_8BIT = 'optimizer_8bit',
  PAGED_OPTIMIZER = 'paged_optimizer',
  FUSED_KERNELS = 'fused_kernels',
  SEQUENCE_PACKING = 'sequence_packing',
  DYNAMIC_PADDING = 'dynamic_padding',
  ACTIVATION_OFFLOADING = 'activation_offloading'
}

/**
 * Training data source
 * Used by: Entity, Command, Widget radio buttons
 */
export enum TrainOnInputs {
  AUTO = 'auto',         // Provider decides
  ENABLED = 'enabled',   // Train on input tokens
  DISABLED = 'disabled'  // Only train on output tokens
}
```

## Configuration Schema

### Hierarchical Parameter Organization

```typescript
/**
 * Complete job configuration
 * Stored in FineTuningJobEntity
 * Accepted by genome/job-create command
 */
interface JobConfiguration {
  // ============================================
  // 1. MODEL SELECTION
  // ============================================
  model: {
    baseModel: string;              // Model identifier (e.g., 'llama-3.1-8b')
    precision?: ModelPrecision;     // Training precision
  };

  // ============================================
  // 2. DATASET CONFIGURATION
  // ============================================
  datasets: {
    trainingFileId: UUID;           // References FineTuningDatasetEntity.id
    validationFileId?: UUID | null; // Optional validation set
  };

  // ============================================
  // 3. TRAINING METHOD
  // ============================================
  method: {
    type: TrainingMethod;           // full, lora, or qlora
    loraConfig?: LoRAConfiguration; // Required if type is lora/qlora
  };

  // ============================================
  // 4. TRAINING SCHEDULE
  // ============================================
  schedule: {
    epochs: number;                 // Number of training epochs (range: 1-20)
    batchSize: number;              // Training batch size (range: 1-64)
    sequenceLength: number;         // Max tokens per sample (range: 512-33K)
    gradientAccumulation: number;   // Steps to accumulate gradients (range: 1-16)
    checkpoints: number;            // How many checkpoints to save (range: 1-10)
    evaluations: number;            // How often to evaluate (range: 0-20)
    trainOnInputs: TrainOnInputs;   // Whether to train on input tokens
  };

  // ============================================
  // 5. OPTIMIZATION
  // ============================================
  optimizer: {
    learningRate: number;           // Learning rate (range: 0-1, typically 1e-5)
    scheduler: LRSchedulerConfig;   // Learning rate schedule configuration
    weightDecay: number;            // Weight decay for regularization (range: 0-1)
    maxGradientNorm: number;        // Gradient clipping threshold (range: 0-10)
  };

  // ============================================
  // 6. MEMORY/PERFORMANCE OPTIMIZATIONS
  // ============================================
  optimizations: {
    enabled: OptimizationFeature[]; // Which optimizations to enable
    // Provider may enable additional optimizations automatically
  };

  // ============================================
  // 7. HARDWARE CONFIGURATION (optional)
  // ============================================
  hardware?: {
    gpuType?: string;               // GPU model (e.g., 'RTX 3060', 'A100')
    numGPUs?: number;               // Number of GPUs for parallel training
    // Only used by providers that allow hardware selection (e.g., DeepSeek)
  };

  // ============================================
  // 8. EXTERNAL INTEGRATIONS (optional)
  // ============================================
  integrations?: {
    wandb?: WandBIntegration;       // Weights & Biases logging
    huggingface?: HFIntegration;    // Hugging Face model upload
  };

  // ============================================
  // 9. OUTPUT CONFIGURATION
  // ============================================
  output: {
    suffix?: string;                // Model name suffix (e.g., 'v1', 'coding-expert')
  };

  // ============================================
  // 10. PROVIDER-SPECIFIC OVERRIDES
  // ============================================
  metadata: Record<string, unknown>; // Provider-specific parameters
}
```

### Nested Type Definitions

```typescript
/**
 * LoRA-specific configuration
 * Only used when method.type is 'lora' or 'qlora'
 */
interface LoRAConfiguration {
  rank: number;                     // LoRA rank (range: 1-256, typically 8-16)
  alpha: number;                    // LoRA alpha scaling (typically 2*rank)
  dropout: number;                  // LoRA dropout rate (range: 0-1)
  trainableModules: string;         // Which modules to apply LoRA to
                                    // Examples: 'all-linear', 'q_proj,v_proj', etc.
}

/**
 * Learning rate scheduler configuration
 */
interface LRSchedulerConfig {
  type: LRSchedulerType;            // Scheduler algorithm
  minLRRatio: number;               // Min LR as ratio of base LR (range: 0-1)
  warmupRatio: number;              // Warmup steps as ratio of total (range: 0-1)
  cycles?: number;                  // Cycles for cosine scheduler (range: 0.1-2.0)
}

/**
 * Weights & Biases integration
 */
interface WandBIntegration {
  project: string;                  // W&B project name
  runName?: string;                 // Optional run name
  apiKey?: string;                  // API key (or use env var)
  baseUrl?: string;                 // Custom W&B server URL
}

/**
 * Hugging Face integration
 */
interface HFIntegration {
  apiToken: string;                 // HF API token
  outputRepoName: string;           // Repository to push model to (e.g., 'user/model')
}
```

## Parameter Validation Ranges

### Hard Limits (Enforced by Entity Validation)

```typescript
export const PARAMETER_RANGES = {
  schedule: {
    epochs: { min: 1, max: 20, default: 3 },
    batchSize: { min: 1, max: 64, default: 4 },
    sequenceLength: { min: 512, max: 33000, default: 2048 },
    gradientAccumulation: { min: 1, max: 16, default: 1 },
    checkpoints: { min: 0, max: 10, default: 1 },
    evaluations: { min: 0, max: 20, default: 1 }
  },
  optimizer: {
    learningRate: { min: 0, max: 1, default: 0.00001 },
    weightDecay: { min: 0, max: 1, default: 0 },
    maxGradientNorm: { min: 0, max: 10, default: 1 },
    warmupRatio: { min: 0, max: 1, default: 0 },
    minLRRatio: { min: 0, max: 1, default: 0 }
  },
  lora: {
    rank: { min: 1, max: 256, default: 8 },
    alpha: { min: 1, max: 512, default: 16 },
    dropout: { min: 0, max: 1, default: 0 }
  }
} as const;
```

### Provider-Specific Limits

Providers may impose stricter limits. These are reported via `ProviderCapabilities` and validated by the adapter.

## Entity Schema Updates

### FineTuningJobEntity Changes

**Current problematic field:**
```typescript
hyperparameters!: JobHyperparameters;  // Too simplistic
```

**New approach:**
```typescript
configuration!: JobConfiguration;      // Comprehensive universal schema
```

**Migration strategy:**
1. Add `configuration` field alongside existing `hyperparameters`
2. Deprecate `hyperparameters` (mark with `@deprecated`)
3. Write migration helper to convert old format → new format
4. After migration period, remove `hyperparameters` field

## Provider Adapter Contract

### Required Interface

```typescript
interface FineTuningAdapter {
  /**
   * Report what this provider supports
   */
  getCapabilities(): ProviderCapabilities;

  /**
   * Validate configuration against provider capabilities
   */
  validateConfiguration(config: JobConfiguration): ValidationResult;

  /**
   * Translate universal config to provider-specific API format
   */
  translateConfiguration(config: JobConfiguration): ProviderAPIPayload;

  /**
   * Submit job to provider
   */
  startTraining(config: JobConfiguration): Promise<TrainingJobHandle>;

  /**
   * Query job status from provider
   */
  queryStatus(providerJobId: string): Promise<JobStatusUpdate>;
}
```

### Provider Capabilities Schema

```typescript
interface ProviderCapabilities {
  // Which training methods are supported
  supportedMethods: TrainingMethod[];

  // Which optimizations are supported
  supportedOptimizations: OptimizationFeature[];

  // Which precision types are supported
  supportedPrecisions: ModelPrecision[];

  // Parameter ranges (may be stricter than universal limits)
  parameterRanges: Partial<typeof PARAMETER_RANGES>;

  // Required parameters (beyond the universal required set)
  additionalRequiredParams?: string[];

  // Custom parameters specific to this provider
  customParameters?: Record<string, ParameterDefinition>;
}

interface ParameterDefinition {
  type: 'string' | 'number' | 'boolean' | 'enum';
  description: string;
  required: boolean;
  default?: unknown;
  enumValues?: string[];
  min?: number;
  max?: number;
}
```

## Command Architecture

### genome/job-create Command

**Input:**
```typescript
interface JobCreateParams {
  personaId: UUID;                  // Who owns this job
  provider: string;                 // Which provider to use
  configuration: JobConfiguration;  // Full job config
}
```

**Validation Flow:**
1. Validate `configuration` structure against `JobConfiguration` schema
2. Validate parameter ranges against `PARAMETER_RANGES`
3. Load provider adapter based on `provider` field
4. Call `adapter.validateConfiguration(configuration)` for provider-specific validation
5. If valid, create `FineTuningJobEntity` with full config
6. Call `adapter.startTraining(configuration)` to submit to provider
7. Update entity with `providerJobId`
8. Return `{ jobId: entity.id, providerJobId: entity.providerJobId }`

**Output:**
```typescript
interface JobCreateResult {
  jobId: UUID;                      // Our internal entity ID
  providerJobId: string;            // Provider's job ID
  estimatedCost?: number;           // If provider reports it
  estimatedDuration?: number;       // If provider reports it (seconds)
}
```

## Widget Integration

### Widget Responsibilities

1. **Query provider capabilities** when user selects a provider
2. **Render form fields** based on capabilities (hide unsupported options)
3. **Validate input** against `PARAMETER_RANGES` before submission
4. **Build `JobConfiguration`** object from form values
5. **Submit** via `genome/job-create` command

### Dynamic Form Generation Pattern

```typescript
// 1. User selects provider
const capabilities = await Commands.execute('genome/provider-capabilities', {
  provider: 'together'
});

// 2. Widget conditionally renders form fields
{capabilities.supportedMethods.includes(TrainingMethod.LORA) && (
  <LoRAConfigSection />
)}

// 3. Dropdowns are generated from enums
{Object.values(LRSchedulerType).map(scheduler => (
  <option value={scheduler}>{scheduler}</option>
))}

// 4. Ranges are validated against capabilities
const maxEpochs = capabilities.parameterRanges.schedule?.epochs.max ?? PARAMETER_RANGES.schedule.epochs.max;
```

## Status Monitoring

### genome/job-status Command

**Input:**
```typescript
interface JobStatusParams {
  jobId: UUID;                      // Our internal entity ID
}
```

**Flow:**
1. Load `FineTuningJobEntity` by `jobId`
2. Load provider adapter based on `entity.provider`
3. Call `adapter.queryStatus(entity.providerJobId)`
4. Update entity with latest status/metrics
5. Return universal status format

**Output:**
```typescript
interface JobStatusResult {
  status: JobStatus;                // Universal status enum
  progress: number;                 // 0-1 (percentage complete)
  metrics: JobMetrics;              // Latest training metrics
  events: JobEvent[];               // Recent events
  estimatedTimeRemaining?: number;  // Seconds
}
```

## Implementation Status

### ✅ Phase 1: Type Definitions - COMPLETE
- [x] Create `FineTuningTypes.ts` with all enums (359 lines)
- [x] Define `JobConfiguration` interface and nested types
- [x] Define `PARAMETER_RANGES` constants
- [x] Update `FineTuningJobEntity` to use new schema with backward compatibility
- [x] Fix readonly property errors in `JobMetrics` interface
- [x] All TypeScript compilation passing (0 errors)
- [x] All entity validation tests passing (22 tests in `tests/unit/FineTuningJobEntity.test.ts`)

**Deliverables:**
- `daemons/data-daemon/shared/entities/FineTuningTypes.ts` - Single source of truth for all types
- `daemons/data-daemon/shared/entities/FineTuningJobEntity.ts` - Updated entity with new schema
- `tests/unit/FineTuningJobEntity.test.ts` - Comprehensive test coverage
- TypeScript compilation: 0 errors
- All CRUD validation tests passing

**Integration with Existing System:**
The new type system coexists with the existing `genome/train` command infrastructure:

- **Existing Commands** (currently in production):
  - `genome/train` - Uses simpler parameter model (`GenomeTrainParams` with individual fields like `rank`, `alpha`, `epochs`)
  - `genome/train-status` - Queries async job status using `sessionId`

- **New Entity Types** (Phase 1, ready for use):
  - `FineTuningJobEntity` with comprehensive `JobConfiguration` schema
  - Can be adopted incrementally without breaking existing commands
  - Provides foundation for future command refactoring

- **Existing Provider Adapters** (working):
  - `OpenAILoRAAdapter`, `DeepSeekLoRAAdapter`, `TogetherLoRAAdapter`, `FireworksLoRAAdapter`, `MistralLoRAAdapter`, etc.
  - Located in `daemons/ai-provider-daemon/adapters/*/server/*FineTuningAdapter.ts`
  - Currently accept simple parameters, can be enhanced to use `JobConfiguration` when needed

### Phase 2: Command Integration (Future - When Needed)
This phase involves REFACTORING existing commands to use the new comprehensive type system. **NOT currently required** - existing system works well.

**Option A: Refactor Existing Commands** (breaking change)
- [ ] Migrate `genome/train` to accept `JobConfiguration` instead of flat parameters
- [ ] Update all callers (TrainingDataAccumulator, PersonaUser, tests)
- [ ] Maintain backward compatibility with parameter mapping

**Option B: Create New Commands** (non-breaking)
- [ ] Implement `genome/job-create` command alongside existing `genome/train`
- [ ] Implement `genome/job-status` command alongside existing `genome/train-status`
- [ ] Implement `genome/provider-capabilities` command for dynamic UI
- [ ] Gradually migrate callers to new commands

**Decision Required:** User feedback needed on whether to refactor or add new commands.

### Phase 3: Provider Adapter Enhancement (Future)
- [ ] Define `FineTuningAdapter` interface with `getCapabilities()` and `validateConfiguration()`
- [ ] Enhance adapters to report capabilities (supported methods, optimizations, precisions)
- [ ] Add provider-specific validation beyond universal schema

### Phase 4: Widget Integration (Future)
- [ ] Build fine-tuning configuration widget
- [ ] Implement dynamic form generation based on provider capabilities
- [ ] Add real-time validation
- [ ] Add training monitor widget

## Open Questions

### 1. Parameter Validation Location
**Question**: Should parameter validation happen in the command or adapter?

**Proposal**:
- Command validates universal schema structure + ranges
- Adapter validates provider-specific capabilities + combinations

**Rationale**: Separation of concerns - command ensures data integrity, adapter ensures provider compatibility.

### 2. Unsupported Parameters
**Question**: How do we handle parameters that only SOME providers support?

**Proposal**:
- All parameters in `JobConfiguration` are optional (except required core params)
- Adapter reports capabilities via `getCapabilities()`
- Widget conditionally shows/hides fields based on capabilities
- Adapter ignores unsupported parameters with a warning log

**Rationale**: Allows configuration portability across providers without breaking.

### 3. Entity Storage Format
**Question**: Should entity store BOTH universal config AND provider-specific payload?

**Proposal**:
- Store only `configuration: JobConfiguration` (universal format)
- Use `metadata` field for provider-specific overrides
- Adapter can reconstruct provider payload from universal config on-demand

**Rationale**: Minimizes storage redundancy, allows re-submission with different provider.

### 4. Widget Capabilities Query
**Question**: Should widgets query provider capabilities dynamically?

**Proposal**:
- Yes - implement `genome/provider-capabilities` command
- Widget calls it when user selects a provider
- Response determines which form fields to show/hide
- Caches capabilities per provider (they rarely change)

**Rationale**: Ensures UI accurately reflects what each provider supports.

## Next Steps

1. **Validate this architecture** with user/team feedback
2. **Create `FineTuningTypes.ts`** with all enums and interfaces
3. **Update `FineTuningJobEntity.ts`** to use new schema
4. **Implement Phase 2** (commands) with proper validation
5. **Document provider adapter implementation guide**

## References

- Together AI Fine-Tuning UI (screenshots provided)
- DeepSeek Fine-Tuning UI (screenshots provided)
- OpenAI Fine-Tuning API Documentation
- Existing `FineTuningJobEntity` implementation
