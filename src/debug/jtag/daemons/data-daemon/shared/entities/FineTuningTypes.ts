/**
 * Fine-Tuning Type Definitions
 *
 * SINGLE SOURCE OF TRUTH for all fine-tuning enums, types, and constants.
 *
 * Flow:
 * FineTuningTypes.ts (this file)
 *       ↓
 * FineTuningJobEntity.ts
 *       ↓
 * Command Schemas (genome/job-create)
 *       ↓
 * Provider Adapters
 *       ↓
 * Widget Configurations
 *
 * Design Principle: Define once, use everywhere. Never duplicate these definitions.
 */

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

/**
 * LoRA-specific configuration
 * Only used when method.type is 'lora' or 'qlora'
 */
export interface LoRAConfiguration {
  rank: number;                     // LoRA rank (range: 1-256, typically 8-16)
  alpha: number;                    // LoRA alpha scaling (typically 2*rank)
  dropout: number;                  // LoRA dropout rate (range: 0-1)
  trainableModules: string;         // Which modules to apply LoRA to
                                    // Examples: 'all-linear', 'q_proj,v_proj', etc.
}

/**
 * Learning rate scheduler configuration
 */
export interface LRSchedulerConfig {
  type: LRSchedulerType;            // Scheduler algorithm
  minLRRatio: number;               // Min LR as ratio of base LR (range: 0-1)
  warmupRatio: number;              // Warmup steps as ratio of total (range: 0-1)
  cycles?: number;                  // Cycles for cosine scheduler (range: 0.1-2.0)
}

/**
 * Weights & Biases integration
 */
export interface WandBIntegration {
  project: string;                  // W&B project name
  runName?: string;                 // Optional run name
  apiKey?: string;                  // API key (or use env var)
  baseUrl?: string;                 // Custom W&B server URL
}

/**
 * Hugging Face integration
 */
export interface HFIntegration {
  apiToken: string;                 // HF API token
  outputRepoName: string;           // Repository to push model to (e.g., 'user/model')
}

/**
 * Complete job configuration
 * Stored in FineTuningJobEntity
 * Accepted by genome/job-create command
 */
export interface JobConfiguration {
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
    trainingFileId: string;         // References FineTuningDatasetEntity.id (UUID)
    validationFileId?: string | null; // Optional validation set
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

/**
 * Parameter validation ranges
 * Hard limits enforced by entity validation
 */
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

/**
 * Provider capability definition for dynamic parameters
 */
export interface ParameterDefinition {
  type: 'string' | 'number' | 'boolean' | 'enum';
  description: string;
  required: boolean;
  default?: unknown;
  enumValues?: string[];
  min?: number;
  max?: number;
}

/**
 * Provider capabilities schema
 * Reports what each provider supports
 */
export interface ProviderCapabilities {
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

/**
 * Validation result
 */
export interface ValidationResult {
  success: boolean;
  error?: string;
  warnings?: string[];
}

/**
 * Job status for tracking training lifecycle
 */
export type JobStatus =
  | 'validating_files'
  | 'queued'
  | 'running'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

/**
 * Event severity level
 */
export type EventLevel = 'info' | 'warning' | 'error';

/**
 * Training metrics data point
 */
export interface MetricPoint {
  readonly step: number;        // Training step number
  readonly value: number;       // Metric value
  readonly timestamp: number;   // Unix timestamp when recorded
}

/**
 * Training job metrics
 */
export interface JobMetrics {
  loss?: MetricPoint[];      // Training loss over time
  accuracy?: MetricPoint[];  // Training accuracy over time (if available)
  valLoss?: MetricPoint[];   // Validation loss (if validation set provided)
  valAccuracy?: MetricPoint[]; // Validation accuracy (if available)
}

/**
 * Training job event (for activity log)
 */
export interface JobEvent {
  readonly message: string;     // Event description
  readonly level: EventLevel;   // Severity level
  readonly createdAt: number;   // Unix timestamp
  readonly data?: Record<string, unknown>; // Additional event data
}

/**
 * Error information when job fails
 */
export interface JobError {
  readonly message: string;     // Error message
  readonly code?: string;       // Error code from provider
  readonly param?: string;      // Parameter that caused error (if applicable)
  readonly timestamp: number;   // When error occurred
}

/**
 * Job status query result
 */
export interface JobStatusResult {
  status: JobStatus;                // Universal status enum
  progress: number;                 // 0-1 (percentage complete)
  metrics: JobMetrics;              // Latest training metrics
  events: JobEvent[];               // Recent events
  estimatedTimeRemaining?: number;  // Seconds
}

/**
 * Job creation result
 */
export interface JobCreateResult {
  jobId: string;                    // Our internal entity ID (UUID)
  providerJobId: string;            // Provider's job ID
  estimatedCost?: number;           // If provider reports it
  estimatedDuration?: number;       // If provider reports it (seconds)
}
