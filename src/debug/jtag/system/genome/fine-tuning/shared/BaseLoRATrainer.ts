/**
 * BaseLoRATrainer - Abstract base class for LoRA fine-tuning
 *
 * Environment-agnostic interface - NO Node.js imports allowed!
 *
 * Philosophy: "general knowledge can understand the base only"
 * - Base class defines the contract (what fine-tuning looks like)
 * - Subclasses implement provider-specific logic (how to actually train)
 * - Orchestrator uses base class interface (doesn't know about providers)
 *
 * Architecture:
 * - Each AI provider adapter CAN implement LoRATrainer interface
 * - Only DeepSeek knows about "deepseek", not the orchestrator
 * - Proper adapter pattern: registry-based, not if/else chains
 */

import type {
  LoRATrainingRequest,
  LoRATrainingResult,
  FineTuningCapabilities,
  FineTuningStrategy,
  TrainingHandle,
  TrainingStatus
} from './FineTuningTypes';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { Logger } from '../../../core/logging/Logger';

/**
 * LoRA Trainer Interface
 *
 * AI provider adapters implement this interface to support fine-tuning.
 * Each adapter knows its own capabilities and strategy.
 */
export interface LoRATrainer {
  /**
   * Check if this provider supports fine-tuning
   */
  supportsFineTuning(): boolean;

  /**
   * Get fine-tuning capabilities (constraints, pricing, etc.)
   */
  getFineTuningCapabilities(): FineTuningCapabilities;

  /**
   * Train a LoRA adapter - Returns immediately with session ID
   *
   * ASYNC ARCHITECTURE:
   * - This method returns immediately (no blocking!)
   * - Returns a session ID for tracking
   * - Use checkStatus(sessionId) to query progress
   * - Training continues on provider's servers/processes
   *
   * Implementation is provider-specific:
   * - Ollama: Call llama.cpp locally, return process handle
   * - OpenAI: Upload dataset to API, create fine-tuning job, return job ID
   * - DeepSeek: Upload dataset to API, create fine-tuning job, return job ID
   *
   * @param request Training configuration
   * @returns Training result with session ID and handle (returns immediately!)
   */
  trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult>;

  /**
   * Check training status for a session
   *
   * Fast query that returns current training status.
   * Can be called anytime, even days after training started.
   *
   * @param sessionId Session ID returned by trainLoRA()
   * @returns Current training status
   */
  checkStatus(sessionId: UUID): Promise<TrainingStatus>;

  /**
   * Get training strategy (local vs remote API)
   */
  getFineTuningStrategy(): FineTuningStrategy;

  /**
   * Estimate training cost (for API-based training)
   * Returns USD cost, or 0 for local training
   */
  estimateTrainingCost(exampleCount: number): number;

  /**
   * Estimate training time (milliseconds)
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number;
}

/**
 * Base implementation with common utility methods
 *
 * Subclasses override abstract methods for provider-specific logic.
 */
export abstract class BaseLoRATrainer implements LoRATrainer {
  /**
   * Provider identifier (e.g., 'ollama', 'openai', 'deepseek')
   * Used for logging and metrics only, NOT for if/else chains
   */
  abstract readonly providerId: string;

  /**
   * Helper to log fine-tuning operations
   * Writes to genome log file
   */
  protected log(level: 'info' | 'debug' | 'warn' | 'error', message: string, ...args: any[]): void {
    const logger = Logger.create('FineTuningAdapter', 'genome');
    logger[level](message, ...args);
  }

  /**
   * Check if this provider supports fine-tuning
   */
  abstract supportsFineTuning(): boolean;

  /**
   * Get fine-tuning capabilities
   */
  abstract getFineTuningCapabilities(): FineTuningCapabilities;

  /**
   * Train LoRA adapter (provider-specific implementation)
   */
  abstract trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult>;

  /**
   * Check training status for a session
   */
  abstract checkStatus(sessionId: UUID): Promise<TrainingStatus>;

  /**
   * Get training strategy
   */
  abstract getFineTuningStrategy(): FineTuningStrategy;

  /**
   * Estimate training cost
   * Default: Free for local training
   */
  estimateTrainingCost(exampleCount: number): number {
    const capabilities = this.getFineTuningCapabilities();

    if (capabilities.costPerExample) {
      return exampleCount * capabilities.costPerExample;
    }

    // Local training is free (ignoring electricity costs)
    return 0;
  }

  /**
   * Estimate training time
   * Default: Use capability metadata if available
   */
  estimateTrainingTime(exampleCount: number, epochs: number): number {
    const capabilities = this.getFineTuningCapabilities();

    if (capabilities.estimatedTrainingTime) {
      return exampleCount * epochs * capabilities.estimatedTrainingTime;
    }

    // Fallback: Conservative estimate (10ms per example per epoch for local)
    const strategy = this.getFineTuningStrategy();
    if (strategy === 'local-llama-cpp' || strategy === 'local-pytorch') {
      return exampleCount * epochs * 10; // 10ms per example (local GPU)
    }

    // API training is usually slower (network overhead, queuing)
    return exampleCount * epochs * 100; // 100ms per example (API)
  }

  /**
   * Validate training request
   * Checks hyperparameters against provider capabilities
   */
  protected validateRequest(request: LoRATrainingRequest): void {
    const capabilities = this.getFineTuningCapabilities();

    // Check rank bounds
    if (request.rank && capabilities.minRank && request.rank < capabilities.minRank) {
      throw new Error(`LoRA rank ${request.rank} below minimum ${capabilities.minRank}`);
    }
    if (request.rank && capabilities.maxRank && request.rank > capabilities.maxRank) {
      throw new Error(`LoRA rank ${request.rank} exceeds maximum ${capabilities.maxRank}`);
    }

    // Check epoch bounds
    if (request.epochs && capabilities.minEpochs && request.epochs < capabilities.minEpochs) {
      throw new Error(`Epochs ${request.epochs} below minimum ${capabilities.minEpochs}`);
    }
    if (request.epochs && capabilities.maxEpochs && request.epochs > capabilities.maxEpochs) {
      throw new Error(`Epochs ${request.epochs} exceeds maximum ${capabilities.maxEpochs}`);
    }

    // Check dataset not empty
    if (!request.dataset.examples || request.dataset.examples.length === 0) {
      throw new Error('Training dataset is empty');
    }

    // Check model support (if provider restricts base models)
    if (capabilities.supportedBaseModels && !capabilities.supportedBaseModels.includes(request.baseModel)) {
      throw new Error(
        `Model ${request.baseModel} not supported for fine-tuning. ` +
        `Supported models: ${capabilities.supportedBaseModels.join(', ')}`
      );
    }
  }
}
