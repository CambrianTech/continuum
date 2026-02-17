/**
 * GenomeBatchMicroTuneTypes - Lightweight in-recipe LoRA updates
 *
 * Performs fast micro-tuning during recipe execution using accumulated examples.
 * Soft updates in RAM, not persisted to disk (that happens during deep training).
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

/** Perform a fast, in-memory LoRA micro-tune on accumulated training examples during recipe execution, without persisting weights to disk. */
export interface GenomeBatchMicroTuneParams extends CommandParams {
  /**
   * Learning domain to train
   */
  domain: string;

  /**
   * Specific role to train (optional)
   */
  roleId?: string;

  /**
   * PersonaUser ID (if different from context)
   */
  personaId?: UUID;

  /**
   * LoRA adapter to update (defaults to domain-based)
   */
  loraAdapter?: string;

  /**
   * Force training even if batch threshold not reached
   */
  forceUpdate?: boolean;

  /**
   * Minimum quality threshold for training examples (0-1)
   */
  qualityThreshold?: number;

  /**
   * Maximum examples to use (if buffer is large)
   */
  maxExamples?: number;
}

/**
 * Result from genome/batch-micro-tune command
 */
export interface GenomeBatchMicroTuneResult extends CommandResult {
  success: boolean;
  error?: string;

  /**
   * Training summary
   */
  training?: {
    domain: string;
    loraAdapter: string;
    examplesUsed: number;
    examplesFiltered: number;     // Low quality examples excluded
    updateType: 'soft' | 'none';  // Soft = RAM only, none = skipped
    improvementEstimate?: number; // Estimated improvement (0-1)

    /**
     * Training metrics
     */
    metrics?: {
      trainingTime: number;       // Milliseconds
      averageQuality: number;     // Quality of examples used
      diversityScore: number;     // How diverse the examples were
    };
  };
}

/**
 * GenomeBatchMicroTune — Type-safe command executor
 *
 * Usage:
 *   import { GenomeBatchMicroTune } from '...shared/GenomeBatchMicroTuneTypes';
 *   const result = await GenomeBatchMicroTune.execute({ ... });
 */
export const GenomeBatchMicroTune = {
  execute(params: CommandInput<GenomeBatchMicroTuneParams>): Promise<GenomeBatchMicroTuneResult> {
    return Commands.execute<GenomeBatchMicroTuneParams, GenomeBatchMicroTuneResult>('genome/batch-micro-tune', params as Partial<GenomeBatchMicroTuneParams>);
  },
  commandName: 'genome/batch-micro-tune' as const,
} as const;
