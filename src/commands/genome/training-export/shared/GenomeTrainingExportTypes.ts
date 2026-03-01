/**
 * Genome Training Export Command - Shared Types
 *
 * Exports accumulated training data from a PersonaUser's TrainingDataAccumulator
 * to a JSONL file suitable for genome/train.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Training Export Command Parameters
 */
export interface GenomeTrainingExportParams extends CommandParams {
  /** Persona whose accumulated training data to export */
  personaId: UUID;
  /** Display name (used in file naming) */
  personaName: string;
  /** Training domain to export (must match what was used in capture) */
  domain: string;
  /** Optional output path — auto-generated if omitted */
  outputPath?: string;
}

/**
 * Factory function for creating GenomeTrainingExportParams
 */
export const createGenomeTrainingExportParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    personaId: UUID;
    personaName: string;
    domain: string;
    outputPath?: string;
  }
): GenomeTrainingExportParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  outputPath: data.outputPath ?? '',
  ...data
});

/**
 * Genome Training Export Command Result
 */
export interface GenomeTrainingExportResult extends CommandResult {
  success: boolean;
  /** Absolute path to the generated JSONL file */
  datasetPath: string;
  /** Number of training examples exported */
  exampleCount: number;
  /** The domain that was exported */
  domain: string;
  error?: string;
}

/**
 * Smart result factory — inherits context/sessionId from params
 */
export const createGenomeTrainingExportResultFromParams = (
  params: GenomeTrainingExportParams,
  differences: Omit<GenomeTrainingExportResult, 'context' | 'sessionId' | 'userId'>
): GenomeTrainingExportResult => transformPayload(params, differences);

/**
 * Genome Training Export — Type-safe command executor
 *
 * Usage:
 *   import { GenomeTrainingExport } from '...shared/GenomeTrainingExportTypes';
 *   const result = await GenomeTrainingExport.execute({ personaId, personaName, domain: 'coding' });
 */
export const GenomeTrainingExport = {
  execute(params: CommandInput<GenomeTrainingExportParams>): Promise<GenomeTrainingExportResult> {
    return Commands.execute<GenomeTrainingExportParams, GenomeTrainingExportResult>('genome/training-export', params as Partial<GenomeTrainingExportParams>);
  },
  commandName: 'genome/training-export' as const,
} as const;
