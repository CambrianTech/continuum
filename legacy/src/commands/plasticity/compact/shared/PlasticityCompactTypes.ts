/**
 * Plasticity Compact Command - Shared Types
 *
 * Physically remove pruned heads from a model's safetensors. Reads gate_gradients.json from adapter directory, computes which heads to prune, then slices Q/K/V/O projection weights to remove dead heads. Produces a smaller model with fewer parameters. Handles both single-file and multi-shard models.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Plasticity Compact Command Parameters
 */
export interface PlasticityCompactParams extends CommandParams {
  // Path to adapter directory containing gate_gradients.json
  adapterPath: string;
  // Path to base model safetensors directory (single or multi-shard)
  modelPath: string;
  // Output path for compacted safetensors. Default: <adapterPath>/compacted/
  outputPath?: string;
  // Compaction config overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold }
  config?: object;
}

/**
 * Factory function for creating PlasticityCompactParams
 */
export const createPlasticityCompactParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to adapter directory containing gate_gradients.json
    adapterPath: string;
    // Path to base model safetensors directory (single or multi-shard)
    modelPath: string;
    // Output path for compacted safetensors. Default: <adapterPath>/compacted/
    outputPath?: string;
    // Compaction config overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold }
    config?: object;
  }
): PlasticityCompactParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  outputPath: data.outputPath ?? '',
  config: data.config ?? {},
  ...data
});

/**
 * Plasticity Compact Command Result
 */
export interface PlasticityCompactResult extends CommandResult {
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
 * Factory function for creating PlasticityCompactResult with defaults
 */
export const createPlasticityCompactResult = (
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
): PlasticityCompactResult => createPayload(context, sessionId, {
  modelPath: data.modelPath ?? '',
  topologyPath: data.topologyPath ?? '',
  originalSizeBytes: data.originalSizeBytes ?? 0,
  compactedSizeBytes: data.compactedSizeBytes ?? 0,
  ...data
});

/**
 * Smart Plasticity Compact-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPlasticityCompactResultFromParams = (
  params: PlasticityCompactParams,
  differences: Omit<PlasticityCompactResult, 'context' | 'sessionId' | 'userId'>
): PlasticityCompactResult => transformPayload(params, differences);

/**
 * Plasticity Compact — Type-safe command executor
 *
 * Usage:
 *   import { PlasticityCompact } from '...shared/PlasticityCompactTypes';
 *   const result = await PlasticityCompact.execute({ ... });
 */
export const PlasticityCompact = {
  execute(params: CommandInput<PlasticityCompactParams>): Promise<PlasticityCompactResult> {
    return Commands.execute<PlasticityCompactParams, PlasticityCompactResult>('plasticity/compact', params as Partial<PlasticityCompactParams>);
  },
  commandName: 'plasticity/compact' as const,
} as const;
