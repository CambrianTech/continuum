/**
 * Plasticity Pipeline Command - Shared Types
 *
 * End-to-end plasticity pipeline: gate_gradients.json → analysis → compaction. The 'wake up to a compacted model' command. Given a gate capture directory and a model path, runs the full pipeline: load gradients, compute optimization plan, build topology, compact model (multi-shard aware), write compacted model + topology + analysis.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Plasticity Pipeline Command Parameters
 */
export interface PlasticityPipelineParams extends CommandParams {
  // Gate capture directory containing gate_gradients.json (from PEFT training with GateGradientCallback)
  capturePath: string;
  // Base model path — directory for multi-shard, file for single safetensors
  modelPath: string;
  // Output directory for compacted model. Default: <capturePath>/compacted/
  outputPath?: string;
  // CompactionConfig overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold, saturatedThreshold, enableQuantization, targetSizeGb }
  config?: object;
}

/**
 * Factory function for creating PlasticityPipelineParams
 */
export const createPlasticityPipelineParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Gate capture directory containing gate_gradients.json (from PEFT training with GateGradientCallback)
    capturePath: string;
    // Base model path — directory for multi-shard, file for single safetensors
    modelPath: string;
    // Output directory for compacted model. Default: <capturePath>/compacted/
    outputPath?: string;
    // CompactionConfig overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold, saturatedThreshold, enableQuantization, targetSizeGb }
    config?: object;
  }
): PlasticityPipelineParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  outputPath: data.outputPath ?? '',
  config: data.config ?? {},
  ...data
});

/**
 * Plasticity Pipeline Command Result
 */
export interface PlasticityPipelineResult extends CommandResult {
  success: boolean;
  // Path to the compacted model safetensors
  modelPath: string;
  // Path to the head_topology.json file
  topologyPath: string;
  // Original model size in bytes
  originalSizeBytes: number;
  // Compacted model size in bytes
  compactedSizeBytes: number;
  error?: JTAGError;
}

/**
 * Factory function for creating PlasticityPipelineResult with defaults
 */
export const createPlasticityPipelineResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Path to the compacted model safetensors
    modelPath?: string;
    // Path to the head_topology.json file
    topologyPath?: string;
    // Original model size in bytes
    originalSizeBytes?: number;
    // Compacted model size in bytes
    compactedSizeBytes?: number;
    error?: JTAGError;
  }
): PlasticityPipelineResult => createPayload(context, sessionId, {
  modelPath: data.modelPath ?? '',
  topologyPath: data.topologyPath ?? '',
  originalSizeBytes: data.originalSizeBytes ?? 0,
  compactedSizeBytes: data.compactedSizeBytes ?? 0,
  ...data
});

/**
 * Smart Plasticity Pipeline-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPlasticityPipelineResultFromParams = (
  params: PlasticityPipelineParams,
  differences: Omit<PlasticityPipelineResult, 'context' | 'sessionId' | 'userId'>
): PlasticityPipelineResult => transformPayload(params, differences);

/**
 * Plasticity Pipeline — Type-safe command executor
 *
 * Usage:
 *   import { PlasticityPipeline } from '...shared/PlasticityPipelineTypes';
 *   const result = await PlasticityPipeline.execute({ ... });
 */
export const PlasticityPipeline = {
  execute(params: CommandInput<PlasticityPipelineParams>): Promise<PlasticityPipelineResult> {
    return Commands.execute<PlasticityPipelineParams, PlasticityPipelineResult>('plasticity/pipeline', params as Partial<PlasticityPipelineParams>);
  },
  commandName: 'plasticity/pipeline' as const,
} as const;
