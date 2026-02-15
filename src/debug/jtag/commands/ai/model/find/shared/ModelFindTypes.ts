/**
 * Model Find Command - Types
 *
 * Find the best AI model matching capability requirements
 * Like AVCaptureDevice.default(for:position:) - pick best matching device
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import type { ModelCapabilities, ModelInfo } from '../../list/shared/ModelListTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Find the best available AI model matching a set of capability requirements, with optional fallback to the closest match.
 */
export interface ModelFindParams extends CommandParams {
  // Capability requirements
  capabilities: ModelCapabilities;

  // Selection strategy
  preferSmallest?: boolean;          // Prefer smallest/fastest model (default for gating)
  preferLargest?: boolean;           // Prefer largest/best model (default for reasoning)
  preferFastest?: boolean;           // Optimize for latency

  // Fallback behavior
  allowFallback?: boolean;           // Return closest match if exact match not found (default: true)
}

/**
 * Model find result
 */
export interface ModelFindResult extends CommandResult {
  success: boolean;
  model?: ModelInfo;                 // Best matching model (undefined if none found)
  alternates?: ModelInfo[];          // Alternative models that also match
  matchScore?: number;               // How well does this match requirements (0-100)
  fallbackUsed?: boolean;            // Did we have to use fallback logic?
  error?: string;
}

/**
 * ModelFind — Type-safe command executor
 *
 * Usage:
 *   import { ModelFind } from '...shared/ModelFindTypes';
 *   const result = await ModelFind.execute({ ... });
 */
export const ModelFind = {
  execute(params: CommandInput<ModelFindParams>): Promise<ModelFindResult> {
    return Commands.execute<ModelFindParams, ModelFindResult>('ai/model/find', params as Partial<ModelFindParams>);
  },
  commandName: 'ai/model/find' as const,
} as const;
