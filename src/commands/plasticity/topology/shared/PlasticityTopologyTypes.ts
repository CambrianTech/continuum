/**
 * Plasticity Topology Command - Shared Types
 *
 * Read the head topology of a compacted model. Returns per-layer head precision assignments showing which heads were pruned, quantized to different levels, or kept at full precision. Use this to inspect what compaction did to a model.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Plasticity Topology Command Parameters
 */
export interface PlasticityTopologyParams extends CommandParams {
  // Path to head_topology.json file (produced by plasticity/compact or plasticity/pipeline)
  topologyPath: string;
}

/**
 * Factory function for creating PlasticityTopologyParams
 */
export const createPlasticityTopologyParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to head_topology.json file (produced by plasticity/compact or plasticity/pipeline)
    topologyPath: string;
  }
): PlasticityTopologyParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Plasticity Topology Command Result
 */
export interface PlasticityTopologyResult extends CommandResult {
  success: boolean;
  // Per-layer topology: head precision assignments
  layers: object;
  // Overall parameter reduction ratio (0-1)
  parameterReduction: number;
  // Count of heads at each precision tier: { removed, ternary, q2, q4, q8, bf16 }
  precisionProfile: object;
  error?: JTAGError;
}

/**
 * Factory function for creating PlasticityTopologyResult with defaults
 */
export const createPlasticityTopologyResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Per-layer topology: head precision assignments
    layers?: object;
    // Overall parameter reduction ratio (0-1)
    parameterReduction?: number;
    // Count of heads at each precision tier: { removed, ternary, q2, q4, q8, bf16 }
    precisionProfile?: object;
    error?: JTAGError;
  }
): PlasticityTopologyResult => createPayload(context, sessionId, {
  layers: data.layers ?? {},
  parameterReduction: data.parameterReduction ?? 0,
  precisionProfile: data.precisionProfile ?? {},
  ...data
});

/**
 * Smart Plasticity Topology-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPlasticityTopologyResultFromParams = (
  params: PlasticityTopologyParams,
  differences: Omit<PlasticityTopologyResult, 'context' | 'sessionId' | 'userId'>
): PlasticityTopologyResult => transformPayload(params, differences);

/**
 * Plasticity Topology — Type-safe command executor
 *
 * Usage:
 *   import { PlasticityTopology } from '...shared/PlasticityTopologyTypes';
 *   const result = await PlasticityTopology.execute({ ... });
 */
export const PlasticityTopology = {
  execute(params: CommandInput<PlasticityTopologyParams>): Promise<PlasticityTopologyResult> {
    return Commands.execute<PlasticityTopologyParams, PlasticityTopologyResult>('plasticity/topology', params as Partial<PlasticityTopologyParams>);
  },
  commandName: 'plasticity/topology' as const,
} as const;
