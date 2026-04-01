/**
 * Model Introspect Command - Shared Types
 *
 * Introspect a model — detect architecture, capabilities, and possible forge stages. Returns the model's current state as an alloy-compatible spec. Works from HF cache or API, no weight download needed.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Model Introspect Command Parameters
 */
export interface ModelIntrospectParams extends CommandParams {
  // HuggingFace model ID (e.g., 'Qwen/Qwen3.5-4B') or local path
  model: string;
}

/**
 * Factory function for creating ModelIntrospectParams
 */
export const createModelIntrospectParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // HuggingFace model ID (e.g., 'Qwen/Qwen3.5-4B') or local path
    model: string;
  }
): ModelIntrospectParams => createPayload(context, sessionId, {
  userId: '' as UUID,
  ...data
});

/**
 * Model Introspect Command Result
 */
export interface ModelIntrospectResult extends CommandResult {
  success: boolean;
  // AlloySource-compatible: baseModel, architecture, isMoE
  source: object;
  // Current model state: params, heads, context, modalities, hidden size
  currentCapabilities: object;
  // Which alloy stages can be applied (with availability and reasons)
  possibleStages: unknown[];
  // The model represented as a starting alloy recipe
  currentAlloy: object;
  error?: JTAGError;
}

/**
 * Factory function for creating ModelIntrospectResult with defaults
 */
export const createModelIntrospectResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // AlloySource-compatible: baseModel, architecture, isMoE
    source?: object;
    // Current model state: params, heads, context, modalities, hidden size
    currentCapabilities?: object;
    // Which alloy stages can be applied (with availability and reasons)
    possibleStages?: unknown[];
    // The model represented as a starting alloy recipe
    currentAlloy?: object;
    error?: JTAGError;
  }
): ModelIntrospectResult => createPayload(context, sessionId, {
  source: data.source ?? {},
  currentCapabilities: data.currentCapabilities ?? {},
  possibleStages: data.possibleStages ?? {} as unknown[],
  currentAlloy: data.currentAlloy ?? {},
  ...data
});

/**
 * Smart Model Introspect-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createModelIntrospectResultFromParams = (
  params: ModelIntrospectParams,
  differences: Omit<ModelIntrospectResult, 'context' | 'sessionId' | 'userId'>
): ModelIntrospectResult => transformPayload(params, differences);

/**
 * Model Introspect — Type-safe command executor
 *
 * Usage:
 *   import { ModelIntrospect } from '...shared/ModelIntrospectTypes';
 *   const result = await ModelIntrospect.execute({ ... });
 */
export const ModelIntrospect = {
  execute(params: CommandInput<ModelIntrospectParams>): Promise<ModelIntrospectResult> {
    return Commands.execute<ModelIntrospectParams, ModelIntrospectResult>('model/introspect', params as Partial<ModelIntrospectParams>);
  },
  commandName: 'model/introspect' as const,
} as const;
