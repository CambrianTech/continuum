/**
 * GenomeJobCreateTypes - Create fine-tuning jobs with comprehensive configuration
 *
 * Universal command for creating fine-tuning jobs across all providers (OpenAI, DeepSeek, Fireworks, Together).
 * Uses provider-agnostic JobConfiguration schema with automatic validation.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { JobConfiguration } from '../../../../daemons/data-daemon/shared/entities/FineTuningTypes';
import { Commands } from '../../../../system/core/shared/Commands';

/** Create a fine-tuning job on a cloud provider (OpenAI, DeepSeek, Fireworks, or Together) with a validated, provider-agnostic configuration. */
export interface GenomeJobCreateParams extends CommandParams {
  /**
   * PersonaUser ID that will own this fine-tuning job
   */
  personaId: UUID;

  /**
   * Provider to use for fine-tuning
   * Options: 'openai' | 'deepseek' | 'fireworks' | 'together'
   */
  provider: string;

  /**
   * Complete job configuration using universal schema
   * See JobConfiguration interface in FineTuningTypes.ts
   */
  configuration: JobConfiguration;

  /**
   * Optional: Pre-validated training file ID
   * If not provided, will be extracted from configuration.datasets.trainingFileId
   */
  trainingFileId?: string;

  /**
   * Optional: Pre-validated validation file ID
   * If not provided, will be extracted from configuration.datasets.validationFileId
   */
  validationFileId?: string | null;

  /**
   * Skip validation and create job immediately (dangerous, use for testing only)
   */
  skipValidation?: boolean;
}

/**
 * Result from genome/job-create command
 */
export interface GenomeJobCreateResult extends CommandResult {
  success: boolean;
  error?: string;

  /**
   * Created job details
   */
  job?: {
    /**
     * Internal job ID (UUID in our database)
     */
    jobId: UUID;

    /**
     * Provider's job ID (for status tracking)
     */
    providerJobId: string;

    /**
     * Provider name
     */
    provider: string;

    /**
     * Current status
     */
    status: 'validating_files' | 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

    /**
     * Base model being fine-tuned
     */
    baseModel: string;

    /**
     * Training file reference
     */
    trainingFileId: string;

    /**
     * Validation file reference (if provided)
     */
    validationFileId?: string | null;

    /**
     * Job creation timestamp
     */
    createdAt: number;

    /**
     * Estimated training time (milliseconds)
     * Based on dataset size and provider benchmarks
     */
    estimatedDuration?: number;

    /**
     * Configuration summary
     */
    configurationSummary: {
      method: string;           // 'full' | 'lora' | 'qlora'
      epochs: number;
      batchSize: number;
      learningRate: number;
      sequenceLength: number;
    };
  };

  /**
   * Validation warnings (non-fatal issues)
   */
  warnings?: string[];
}

/**
 * GenomeJobCreate — Type-safe command executor
 *
 * Usage:
 *   import { GenomeJobCreate } from '...shared/GenomeJobCreateTypes';
 *   const result = await GenomeJobCreate.execute({ ... });
 */
export const GenomeJobCreate = {
  execute(params: CommandInput<GenomeJobCreateParams>): Promise<GenomeJobCreateResult> {
    return Commands.execute<GenomeJobCreateParams, GenomeJobCreateResult>('genome/job-create', params as Partial<GenomeJobCreateParams>);
  },
  commandName: 'genome/job-create' as const,
} as const;
