/**
 * Inference Capacity Command - Shared Types
 *
 * Report local-inference concurrency cap. How many parallel generate requests the hardware can handle simultaneously — matches the BatchScheduler's n_seq_max and the InferenceCoordinator's admission slots. Scaled by RAM: 48GB+ → 3, 16GB+ → 2, else 1. Single source of truth across the TS admission layer and the Rust scheduler (see issue #887).
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Inference Capacity Command Parameters
 */
export interface InferenceCapacityParams extends CommandParams {
  _noParams?: never; // Marker to avoid empty interface
}

/**
 * Factory function for creating InferenceCapacityParams
 */
export const createInferenceCapacityParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Record<string, unknown> = {}
): InferenceCapacityParams => createPayload(context, sessionId, {
  ...data
}) as unknown as InferenceCapacityParams;

/**
 * Inference Capacity Command Result
 */
export interface InferenceCapacityResult extends CommandResult {
  success: boolean;
  // Number of concurrent local-inference slots available on this host. Always >= 1.
  capacity: number;
  error?: JTAGError;
}

/**
 * Factory function for creating InferenceCapacityResult with defaults
 */
export const createInferenceCapacityResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Number of concurrent local-inference slots available on this host. Always >= 1.
    capacity?: number;
    error?: JTAGError;
  }
): InferenceCapacityResult => createPayload(context, sessionId, {
  capacity: data.capacity ?? 0,
  ...data
});

/**
 * Smart Inference Capacity-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInferenceCapacityResultFromParams = (
  params: InferenceCapacityParams,
  differences: Omit<InferenceCapacityResult, 'context' | 'sessionId' | 'userId'>
): InferenceCapacityResult => transformPayload(params, differences);

/**
 * Inference Capacity — Type-safe command executor
 *
 * Usage:
 *   import { InferenceCapacity } from '...shared/InferenceCapacityTypes';
 *   const result = await InferenceCapacity.execute({ ... });
 */
export const InferenceCapacity = {
  execute(params: CommandInput<InferenceCapacityParams>): Promise<InferenceCapacityResult> {
    return Commands.execute<InferenceCapacityParams, InferenceCapacityResult>('inference/capacity', params as Partial<InferenceCapacityParams>);
  },
  commandName: 'inference/capacity' as const,
} as const;
