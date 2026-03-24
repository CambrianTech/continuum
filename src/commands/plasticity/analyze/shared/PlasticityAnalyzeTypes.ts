/**
 * Plasticity Analyze Command - Shared Types
 *
 * Dry-run analysis of what compaction would do to a model. Reads gate_gradients.json from the adapter directory, computes per-head utilization scores, and returns a topology showing which heads would be pruned/compressed/kept. Does NOT modify any files.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Plasticity Analyze Command Parameters
 */
export interface PlasticityAnalyzeParams extends CommandParams {
  // Path to adapter directory containing gate_gradients.json (output from training with GateGradientCallback)
  adapterPath: string;
  // Compaction config overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold, saturatedThreshold, enableQuantization }
  config?: object;
}

/**
 * Factory function for creating PlasticityAnalyzeParams
 */
export const createPlasticityAnalyzeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to adapter directory containing gate_gradients.json (output from training with GateGradientCallback)
    adapterPath: string;
    // Compaction config overrides: { minHeadsPerLayer, minKvHeadsPerLayer, deadThreshold, lowThreshold, highThreshold, saturatedThreshold, enableQuantization }
    config?: object;
  }
): PlasticityAnalyzeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  config: data.config ?? {},
  ...data
});

/**
 * Plasticity Analyze Command Result
 */
export interface PlasticityAnalyzeResult extends CommandResult {
  success: boolean;
  // HeadTopology: per-layer head precision assignments (prune/Q2/Q4/Q8/BF16)
  topology: object;
  // Per-layer summary: head counts by precision tier, parameter reduction %
  layerSummaries: object;
  // Estimated memory savings in bytes from compaction
  estimatedSavingsBytes: number;
  // Heads with utilization > saturatedThreshold that may benefit from higher rank LoRA
  saturatedHeads: object;
  error?: JTAGError;
}

/**
 * Factory function for creating PlasticityAnalyzeResult with defaults
 */
export const createPlasticityAnalyzeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // HeadTopology: per-layer head precision assignments (prune/Q2/Q4/Q8/BF16)
    topology?: object;
    // Per-layer summary: head counts by precision tier, parameter reduction %
    layerSummaries?: object;
    // Estimated memory savings in bytes from compaction
    estimatedSavingsBytes?: number;
    // Heads with utilization > saturatedThreshold that may benefit from higher rank LoRA
    saturatedHeads?: object;
    error?: JTAGError;
  }
): PlasticityAnalyzeResult => createPayload(context, sessionId, {
  topology: data.topology ?? {},
  layerSummaries: data.layerSummaries ?? {},
  estimatedSavingsBytes: data.estimatedSavingsBytes ?? 0,
  saturatedHeads: data.saturatedHeads ?? {},
  ...data
});

/**
 * Smart Plasticity Analyze-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPlasticityAnalyzeResultFromParams = (
  params: PlasticityAnalyzeParams,
  differences: Omit<PlasticityAnalyzeResult, 'context' | 'sessionId' | 'userId'>
): PlasticityAnalyzeResult => transformPayload(params, differences);

/**
 * Plasticity Analyze — Type-safe command executor
 *
 * Usage:
 *   import { PlasticityAnalyze } from '...shared/PlasticityAnalyzeTypes';
 *   const result = await PlasticityAnalyze.execute({ ... });
 */
export const PlasticityAnalyze = {
  execute(params: CommandInput<PlasticityAnalyzeParams>): Promise<PlasticityAnalyzeResult> {
    return Commands.execute<PlasticityAnalyzeParams, PlasticityAnalyzeResult>('plasticity/analyze', params as Partial<PlasticityAnalyzeParams>);
  },
  commandName: 'plasticity/analyze' as const,
} as const;
