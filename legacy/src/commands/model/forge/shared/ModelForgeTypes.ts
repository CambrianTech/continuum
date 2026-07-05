/**
 * Model Forge Command - Shared Types
 *
 * Start a model forge job — sends forge parameters to a grid node with GPU for training. Returns job ID for status tracking.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/**
 * Model Forge Command Parameters
 */
export interface ModelForgeParams extends CommandParams {
  // Base model to forge (e.g., 'Qwen/Qwen3.5-4B')
  model: string;
  // Training domain (code, reasoning, general)
  domain: string;
  // Number of training steps
  steps: number;
  // Pruning level 0.0-0.7 (fraction of heads to prune)
  pruneLevel: number;
  // Pruning strategy: entropy, random, magnitude
  pruneStrategy: string;
  // Number of prune→recover forge cycles
  cycles: number;
  // Learning rate (e.g., '2e-4', '5e-5')
  learningRate: string;
  // Number of MoE experts to keep (0 for non-MoE models)
  experts?: number;
  // Target grid node. If omitted, routes to first available GPU node.
  nodeId?: string;
}

/**
 * Factory function for creating ModelForgeParams
 */
export const createModelForgeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Base model to forge (e.g., 'Qwen/Qwen3.5-4B')
    model: string;
    // Training domain (code, reasoning, general)
    domain: string;
    // Number of training steps
    steps: number;
    // Pruning level 0.0-0.7 (fraction of heads to prune)
    pruneLevel: number;
    // Pruning strategy: entropy, random, magnitude
    pruneStrategy: string;
    // Number of prune→recover forge cycles
    cycles: number;
    // Learning rate (e.g., '2e-4', '5e-5')
    learningRate: string;
    // Number of MoE experts to keep (0 for non-MoE models)
    experts?: number;
    // Target grid node. If omitted, routes to first available GPU node.
    nodeId?: string;
  }
): ModelForgeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  experts: data.experts ?? 0,
  nodeId: data.nodeId ?? '',
  ...data
});

/**
 * Model Forge Command Result
 */
export interface ModelForgeResult extends CommandResult {
  success: boolean;
  // Unique forge job ID for status tracking
  jobId: string;
  // Grid node the job was routed to
  nodeId: string;
  // Human-readable node name
  nodeName: string;
  // Estimated duration based on model size and steps
  estimatedDuration: string;
  error?: JTAGError;
}

/**
 * Factory function for creating ModelForgeResult with defaults
 */
export const createModelForgeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Unique forge job ID for status tracking
    jobId?: string;
    // Grid node the job was routed to
    nodeId?: string;
    // Human-readable node name
    nodeName?: string;
    // Estimated duration based on model size and steps
    estimatedDuration?: string;
    error?: JTAGError;
  }
): ModelForgeResult => createPayload(context, sessionId, {
  jobId: data.jobId ?? '',
  nodeId: data.nodeId ?? '',
  nodeName: data.nodeName ?? '',
  estimatedDuration: data.estimatedDuration ?? '',
  ...data
});

/**
 * Smart Model Forge-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createModelForgeResultFromParams = (
  params: ModelForgeParams,
  differences: Omit<ModelForgeResult, 'context' | 'sessionId' | 'userId'>
): ModelForgeResult => transformPayload(params, differences);

/**
 * Model Forge — Type-safe command executor
 *
 * Usage:
 *   import { ModelForge } from '...shared/ModelForgeTypes';
 *   const result = await ModelForge.execute({ ... });
 */
export const ModelForge = {
  execute(params: CommandInput<ModelForgeParams>): Promise<ModelForgeResult> {
    return Commands.execute<ModelForgeParams, ModelForgeResult>('model/forge', params as Partial<ModelForgeParams>);
  },
  commandName: 'model/forge' as const,
} as const;
