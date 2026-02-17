/**
 * Genome Train Command - Shared Types
 *
 * Execute LoRA fine-tuning on a JSONL dataset using PEFTLoRAAdapter. Wraps trainLoRA() as a command for Sentinel pipeline orchestration
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Train Command Parameters
 */
export interface GenomeTrainParams extends CommandParams {
  // Persona to train adapter for
  personaId: UUID;
  // Display name (used in adapter naming)
  personaName: string;
  // Trait type label for the adapter
  traitType: string;
  // Path to JSONL training dataset file
  datasetPath: string;
  // Base model to fine-tune (default: 'smollm2:135m')
  baseModel?: string;
  // LoRA rank (default: 32)
  rank?: number;
  // Number of training epochs (default: 3)
  epochs?: number;
  // Learning rate (default: 0.0001)
  learningRate?: number;
  // Batch size (default: 4)
  batchSize?: number;
}

/**
 * Factory function for creating GenomeTrainParams
 */
export const createGenomeTrainParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Persona to train adapter for
    personaId: UUID;
    // Display name (used in adapter naming)
    personaName: string;
    // Trait type label for the adapter
    traitType: string;
    // Path to JSONL training dataset file
    datasetPath: string;
    // Base model to fine-tune (default: 'smollm2:135m')
    baseModel?: string;
    // LoRA rank (default: 32)
    rank?: number;
    // Number of training epochs (default: 3)
    epochs?: number;
    // Learning rate (default: 0.0001)
    learningRate?: number;
    // Batch size (default: 4)
    batchSize?: number;
  }
): GenomeTrainParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  baseModel: data.baseModel ?? '',
  rank: data.rank ?? 0,
  epochs: data.epochs ?? 0,
  learningRate: data.learningRate ?? 0,
  batchSize: data.batchSize ?? 0,
  ...data
});

/**
 * Training metrics returned after successful LoRA fine-tuning
 */
export interface GenomeTrainMetrics {
  finalLoss: number;
  trainingTime: number;
  examplesProcessed: number;
  epochs: number;
}

/**
 * Genome Train Command Result
 */
export interface GenomeTrainResult extends CommandResult {
  success: boolean;
  // Path to the trained adapter files
  adapterPath: string;
  // Persisted GenomeLayerEntity ID (UUID) — used by downstream steps to reference the adapter
  layerId?: UUID;
  // Training metrics
  metrics: GenomeTrainMetrics;
  error?: string;
}

/**
 * Factory function for creating GenomeTrainResult with defaults
 */
export const createGenomeTrainResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Path to the trained adapter files
    adapterPath?: string;
    // Training metrics
    metrics?: GenomeTrainMetrics;
    error?: string;
  }
): GenomeTrainResult => createPayload(context, sessionId, {
  adapterPath: data.adapterPath ?? '',
  metrics: data.metrics ?? { finalLoss: 0, trainingTime: 0, examplesProcessed: 0, epochs: 0 },
  ...data
});

/**
 * Smart Genome Train-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeTrainResultFromParams = (
  params: GenomeTrainParams,
  differences: Omit<GenomeTrainResult, 'context' | 'sessionId' | 'userId'>
): GenomeTrainResult => transformPayload(params, differences);

/**
 * Genome Train — Type-safe command executor
 *
 * Usage:
 *   import { GenomeTrain } from '...shared/GenomeTrainTypes';
 *   const result = await GenomeTrain.execute({ ... });
 */
export const GenomeTrain = {
  execute(params: CommandInput<GenomeTrainParams>): Promise<GenomeTrainResult> {
    return Commands.execute<GenomeTrainParams, GenomeTrainResult>('genome/train', params as Partial<GenomeTrainParams>);
  },
  commandName: 'genome/train' as const,
} as const;
