/**
 * Genome Dataset Prepare Command - Shared Types
 *
 * Collect training data from chat history for a persona and export as JSONL dataset for LoRA fine-tuning
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Dataset Prepare Command Parameters
 */
export interface GenomeDatasetPrepareParams extends CommandParams {
  // Persona to collect training data for
  personaId: UUID;
  // Display name (used in dataset metadata and file naming)
  personaName: string;
  // Room to collect conversation data from
  roomId: UUID;
  // Trait type label for the dataset (default: 'conversational')
  traitType?: string;
  // Minimum messages required to produce a dataset (default: 10)
  minMessages?: number;
  // Maximum messages to process (default: 500)
  maxMessages?: number;
}

/**
 * Factory function for creating GenomeDatasetPrepareParams
 */
export const createGenomeDatasetPrepareParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Persona to collect training data for
    personaId: UUID;
    // Display name (used in dataset metadata and file naming)
    personaName: string;
    // Room to collect conversation data from
    roomId: UUID;
    // Trait type label for the dataset (default: 'conversational')
    traitType?: string;
    // Minimum messages required to produce a dataset (default: 10)
    minMessages?: number;
    // Maximum messages to process (default: 500)
    maxMessages?: number;
  }
): GenomeDatasetPrepareParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  traitType: data.traitType ?? '',
  minMessages: data.minMessages ?? 0,
  maxMessages: data.maxMessages ?? 0,
  ...data
});

/**
 * Genome Dataset Prepare Command Result
 */
export interface GenomeDatasetPrepareResult extends CommandResult {
  success: boolean;
  // Absolute path to the generated JSONL file
  datasetPath: string;
  // Number of training examples in the dataset
  exampleCount: number;
  // Persona ID the dataset was built for
  personaId: UUID;
  // Trait type label
  traitType: string;
  error?: string;
}

/**
 * Factory function for creating GenomeDatasetPrepareResult with defaults
 */
export const createGenomeDatasetPrepareResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Absolute path to the generated JSONL file
    datasetPath?: string;
    // Number of training examples in the dataset
    exampleCount?: number;
    // Persona ID the dataset was built for
    personaId?: UUID;
    // Trait type label
    traitType?: string;
    error?: string;
  }
): GenomeDatasetPrepareResult => createPayload(context, sessionId, {
  datasetPath: data.datasetPath ?? '',
  exampleCount: data.exampleCount ?? 0,
  personaId: data.personaId ?? '' as UUID,
  traitType: data.traitType ?? '',
  ...data
});

/**
 * Smart Genome Dataset Prepare-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeDatasetPrepareResultFromParams = (
  params: GenomeDatasetPrepareParams,
  differences: Omit<GenomeDatasetPrepareResult, 'context' | 'sessionId' | 'userId'>
): GenomeDatasetPrepareResult => transformPayload(params, differences);

/**
 * Genome Dataset Prepare — Type-safe command executor
 *
 * Usage:
 *   import { GenomeDatasetPrepare } from '...shared/GenomeDatasetPrepareTypes';
 *   const result = await GenomeDatasetPrepare.execute({ ... });
 */
export const GenomeDatasetPrepare = {
  execute(params: CommandInput<GenomeDatasetPrepareParams>): Promise<GenomeDatasetPrepareResult> {
    return Commands.execute<GenomeDatasetPrepareParams, GenomeDatasetPrepareResult>('genome/dataset-prepare', params as Partial<GenomeDatasetPrepareParams>);
  },
  commandName: 'genome/dataset-prepare' as const,
} as const;
