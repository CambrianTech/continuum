/**
 * Genome Train Command Types
 *
 * JTAG command that integrates fine-tuning into the live system.
 * Philosophy: "Move past simple integration tests into real utility"
 *
 * Usage: ./jtag genome/train --personaId=<uuid> --provider=unsloth --roomId=<uuid>
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { TraitType } from '../../../../system/genome/entities/GenomeLayerEntity';

/**
 * Genome Train Parameters
 */
export interface GenomeTrainParams extends CommandParams {
  /**
   * PersonaUser ID to train
   */
  personaId: UUID;

  /**
   * Training provider: 'unsloth' | 'deepseek' | 'openai' | 'anthropic'
   */
  provider: 'unsloth' | 'deepseek' | 'openai' | 'anthropic';

  /**
   * Room ID to extract training data from
   * If not provided, uses all rooms the PersonaUser participates in
   */
  roomId?: UUID;

  /**
   * Trait type to train (default: 'conversational')
   */
  traitType?: TraitType;

  /**
   * Base model to fine-tune (provider-specific)
   * - Unsloth: 'unsloth/Llama-4-8b', 'unsloth/DeepSeek-R1-7b', etc.
   * - DeepSeek: 'deepseek-chat', 'deepseek-coder', etc.
   * - OpenAI: 'gpt-3.5-turbo', 'gpt-4', etc.
   * - Anthropic: 'claude-3-haiku', 'claude-3-sonnet', etc.
   */
  baseModel?: string;

  /**
   * LoRA rank (8-256, default: 32)
   */
  rank?: number;

  /**
   * LoRA alpha (8-256, default: 32)
   */
  alpha?: number;

  /**
   * Training epochs (1-100, default: 3)
   */
  epochs?: number;

  /**
   * Learning rate (0.00001-0.001, default: 0.0001)
   */
  learningRate?: number;

  /**
   * Batch size (1-32, default: 4)
   */
  batchSize?: number;

  /**
   * Maximum messages to extract from chat history (default: 50)
   */
  maxMessages?: number;

  /**
   * Minimum messages required for training (default: 10)
   */
  minMessages?: number;

  /**
   * Show cost/time estimates before training (default: true)
   */
  showEstimates?: boolean;

  /**
   * Dry run: build dataset and show estimates, but don't train (default: false)
   */
  dryRun?: boolean;
}

/**
 * Genome Train Result
 */
export interface GenomeTrainResult extends CommandResult {
  /**
   * Training completed successfully
   */
  success: boolean;

  /**
   * Trained model ID or adapter path
   */
  modelId?: string;

  /**
   * Path to adapter files (for local training)
   */
  adapterPath?: string;

  /**
   * Training metrics
   */
  metrics?: {
    /**
     * Training time in milliseconds
     */
    trainingTime: number;

    /**
     * Final loss value
     */
    finalLoss: number;

    /**
     * Number of examples processed
     */
    examplesProcessed: number;

    /**
     * Number of epochs completed
     */
    epochs: number;
  };

  /**
   * Cost estimates (shown before training)
   */
  estimates?: {
    /**
     * Estimated cost in USD
     */
    cost: number;

    /**
     * Estimated time in milliseconds
     */
    time: number;

    /**
     * Number of training examples
     */
    exampleCount: number;
  };

  /**
   * Dataset statistics
   */
  dataset?: {
    /**
     * Total messages processed
     */
    totalMessages: number;

    /**
     * Training examples created
     */
    exampleCount: number;

    /**
     * Messages filtered out
     */
    messagesFiltered: number;
  };

  /**
   * Error message if training failed
   */
  error?: string;
}

