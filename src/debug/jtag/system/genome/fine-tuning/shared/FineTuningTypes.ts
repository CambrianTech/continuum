/**
 * Fine-Tuning Types - Environment-Agnostic
 *
 * These types work in both browser and server environments.
 * NO Node.js imports allowed here!
 *
 * Philosophy: "general knowledge can understand the base only"
 * - Shared types define the contract
 * - Server implementations provide Node.js-specific logic
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { TraitType } from '../../../genome/entities/GenomeLayerEntity';

/**
 * Training dataset format for fine-tuning
 * Uses standard chat completions format (compatible with OpenAI, Anthropic, etc.)
 */
export interface TrainingExample {
  messages: TrainingMessage[];
  metadata?: {
    timestamp?: number;
    roomId?: UUID;
    correctionId?: UUID;
    confidence?: number;
  };
}

export interface TrainingMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

/**
 * Training dataset with examples
 */
export interface TrainingDataset {
  examples: TrainingExample[];
  metadata: {
    personaId: UUID;
    personaName: string;
    traitType: TraitType;
    createdAt: number;
    source: 'corrections' | 'conversations' | 'exercises';
    totalExamples: number;
  };
}

/**
 * Fine-tuning request configuration
 */
export interface LoRATrainingRequest {
  // Identity
  personaId: UUID;
  personaName: string;
  traitType: TraitType;

  // Model configuration
  baseModel: string;  // 'llama3.2', 'gpt-3.5-turbo', 'deepseek-chat', etc.

  // Training data
  dataset: TrainingDataset;

  // LoRA hyperparameters
  rank?: number;              // LoRA rank (default: 16)
  alpha?: number;             // LoRA alpha (default: 32)
  epochs?: number;            // Training epochs (default: 3)
  learningRate?: number;      // Learning rate (default: 0.0001)
  batchSize?: number;         // Batch size (default: 4)

  // QLoRA quantization — quantize base model to fit largest model on hardware.
  // LoRA weights stay full precision. A 3B model in 4-bit fits ~2GB VRAM.
  quantize?: boolean;         // Enable QLoRA quantization (default: true)
  quantizeBits?: 4 | 8;      // Quantization bits (default: 4 for NF4)

  // Output configuration
  outputPath?: string;        // Where to save adapter (default: system-generated)

  // Optional validation dataset
  validationDataset?: TrainingDataset;
}

/**
 * Fine-tuning result with metrics
 */
export interface LoRATrainingResult {
  success: boolean;

  // Adapter location
  modelPath?: string;         // Local path to .safetensors file (local training)
  modelId?: string;           // Remote model ID (API training)
  trainedModelName?: string;  // Trained model identifier for inference

  // Training metrics
  metrics?: {
    trainLoss?: number;
    validationLoss?: number;
    finalLoss?: number;       // Final loss value (may be same as trainLoss)
    accuracy?: number;
    epochs: number;
    trainingTime: number;     // milliseconds
    examplesProcessed: number;
  };

  // Adapter package manifest (written to adapter directory)
  manifest?: import('../../shared/AdapterPackageTypes').AdapterPackageManifest;

  // Sentinel handle — references the Rust-managed process that ran training.
  // Use sentinel/status or sentinel/logs/read to inspect.
  sentinelHandle?: string;

  // Error information
  error?: string;
  errorDetails?: unknown;
}

/**
 * Training job status (for async training)
 */
export type TrainingJobStatus =
  | 'pending'      // Queued for training
  | 'preparing'    // Preparing dataset
  | 'training'     // Currently training
  | 'validating'   // Running validation
  | 'completed'    // Successfully completed
  | 'failed'       // Training failed
  | 'cancelled';   // User cancelled

/**
 * Training job (tracks async fine-tuning)
 */
export interface TrainingJob {
  id: UUID;
  personaId: UUID;
  personaName: string;
  traitType: TraitType;
  baseModel: string;

  status: TrainingJobStatus;
  progress: number;           // 0.0 - 1.0

  startedAt?: number;
  completedAt?: number;

  result?: LoRATrainingResult;

  // For API-based training
  providerJobId?: string;     // OpenAI job ID, DeepSeek job ID, etc.
}

/**
 * Fine-tuning strategy (how to train)
 */
export type FineTuningStrategy =
  | 'local-llama-cpp'    // Local training via llama.cpp
  | 'local-pytorch'      // Local training via PyTorch + Transformers
  | 'remote-api';        // Remote training via provider API (OpenAI, DeepSeek, etc.)

/**
 * Provider capabilities for fine-tuning
 */
export interface FineTuningCapabilities {
  supportsFineTuning: boolean;
  strategy: FineTuningStrategy;

  // Model restrictions
  supportedBaseModels?: string[];      // Which models can be fine-tuned

  // Hyperparameter constraints
  minRank?: number;
  maxRank?: number;
  defaultRank?: number;               // Default LoRA rank
  minAlpha?: number;
  maxAlpha?: number;
  defaultAlpha?: number;              // Default LoRA alpha
  minEpochs?: number;
  maxEpochs?: number;
  defaultEpochs?: number;             // Default training epochs
  minLearningRate?: number;
  maxLearningRate?: number;
  defaultLearningRate?: number;       // Default learning rate
  minBatchSize?: number;
  maxBatchSize?: number;
  defaultBatchSize?: number;          // Default batch size

  // Genome capabilities (adapter composition)
  maxActiveLayers?: number;           // Max LoRA layers active simultaneously (undefined = unlimited)
  supportsDownload?: boolean;         // Can download trained adapter weights
  supportsLocalComposition?: boolean;  // Can compose locally with PEFT
  compositionMethods?: Array<'stack' | 'weighted' | 'TIES' | 'DARE'>;  // Supported composition methods

  // System requirements
  requiresGPU?: boolean;              // Does this strategy require a GPU?
  requiresInternet?: boolean;         // Does this strategy require internet?

  // Pricing (for API-based training)
  costPerExample?: number;             // USD per training example
  estimatedTrainingTime?: number;      // Milliseconds per example
}

/**
 * Training handle returned by _startTraining()
 * Contains whatever identifier(s) needed to track this training job
 */
export interface TrainingHandle {
  /** Primary identifier (jobId, processId, etc.) */
  jobId: string;

  /** Optional secondary identifiers */
  fileId?: string;        // For cleanup (OpenAI file uploads)
  datasetName?: string;   // Fireworks-style dataset references
  processId?: number;     // Local training process IDs

  /** Provider-specific metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Training status returned by _queryStatus()
 */
export interface TrainingStatus {
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress?: number;      // 0-1 if available
  modelId?: string;       // When completed
  error?: string;         // If failed

  /** Provider-specific data */
  metadata?: Record<string, unknown>;
}
