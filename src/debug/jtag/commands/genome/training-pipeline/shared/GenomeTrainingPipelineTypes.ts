/**
 * Genome Training Pipeline Command - Shared Types
 *
 * One-command entry point for full LoRA training workflow. Builds a Sentinel pipeline that prepares data, trains adapter, registers it, and activates it for a persona
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Training Pipeline Command Parameters
 */
export interface GenomeTrainingPipelineParams extends CommandParams {
  // Persona to train
  personaId: UUID;
  // Display name for the persona
  personaName: string;
  // Room to collect training data from
  roomId: UUID;
  // Trait type label (default: 'conversational')
  traitType?: string;
  // Base model to fine-tune (default: LOCAL_MODELS.DEFAULT)
  baseModel?: string;
  // LoRA rank (default: 32)
  rank?: number;
  // Training epochs (default: 3)
  epochs?: number;
  // Learning rate (default: 0.0001)
  learningRate?: number;
  // Batch size (default: 4)
  batchSize?: number;
}

/**
 * Factory function for creating GenomeTrainingPipelineParams
 */
export const createGenomeTrainingPipelineParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Persona to train
    personaId: UUID;
    // Display name for the persona
    personaName: string;
    // Room to collect training data from
    roomId: UUID;
    // Trait type label (default: 'conversational')
    traitType?: string;
    // Base model to fine-tune (default: LOCAL_MODELS.DEFAULT)
    baseModel?: string;
    // LoRA rank (default: 32)
    rank?: number;
    // Training epochs (default: 3)
    epochs?: number;
    // Learning rate (default: 0.0001)
    learningRate?: number;
    // Batch size (default: 4)
    batchSize?: number;
  }
): GenomeTrainingPipelineParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  traitType: data.traitType ?? '',
  baseModel: data.baseModel ?? '',
  rank: data.rank ?? 0,
  epochs: data.epochs ?? 0,
  learningRate: data.learningRate ?? 0,
  batchSize: data.batchSize ?? 0,
  ...data
});

/**
 * Genome Training Pipeline Command Result
 */
export interface GenomeTrainingPipelineResult extends CommandResult {
  success: boolean;
  // Sentinel pipeline handle for tracking progress via sentinel/status
  handle: string;
  // Name of the generated pipeline
  pipelineName: string;
  error?: string;
}

/**
 * Factory function for creating GenomeTrainingPipelineResult with defaults
 */
export const createGenomeTrainingPipelineResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Sentinel pipeline handle for tracking progress via sentinel/status
    handle?: string;
    // Name of the generated pipeline
    pipelineName?: string;
    error?: string;
  }
): GenomeTrainingPipelineResult => createPayload(context, sessionId, {
  handle: data.handle ?? '',
  pipelineName: data.pipelineName ?? '',
  ...data
});

/**
 * Smart Genome Training Pipeline-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeTrainingPipelineResultFromParams = (
  params: GenomeTrainingPipelineParams,
  differences: Omit<GenomeTrainingPipelineResult, 'context' | 'sessionId' | 'userId'>
): GenomeTrainingPipelineResult => transformPayload(params, differences);

/**
 * Genome Training Pipeline — Type-safe command executor
 *
 * Usage:
 *   import { GenomeTrainingPipeline } from '...shared/GenomeTrainingPipelineTypes';
 *   const result = await GenomeTrainingPipeline.execute({ ... });
 */
export const GenomeTrainingPipeline = {
  execute(params: CommandInput<GenomeTrainingPipelineParams>): Promise<GenomeTrainingPipelineResult> {
    return Commands.execute<GenomeTrainingPipelineParams, GenomeTrainingPipelineResult>('genome/training-pipeline', params as Partial<GenomeTrainingPipelineParams>);
  },
  commandName: 'genome/training-pipeline' as const,
} as const;
