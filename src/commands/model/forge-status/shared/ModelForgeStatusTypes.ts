/**
 * Model Forge Status Command - Shared Types
 *
 * Get the current status of active model forges — phase, step, loss, VRAM usage, ETA.
 * Polls status.json from forge nodes on the grid.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/**
 * Status of a single active forge job
 */
export interface ForgeJobStatus {
  /** Grid node running this forge */
  nodeId: string;
  /** Human-readable node name */
  nodeName: string;
  /** Current phase: loading, training, pruning, defrag, saving, generating_samples */
  phase: string;
  /** Human-readable detail string */
  detail: string;
  /** Base model being forged (e.g., Qwen/Qwen3.5-4B) */
  model: string;
  /** Training domain (code, reasoning, etc.) */
  domain: string;
  /** Current training step */
  step: number;
  /** Total training steps */
  totalSteps: number;
  /** Current training loss */
  loss: number;
  /** GPU VRAM used in GB */
  vramGb: number;
  /** GPU VRAM total in GB */
  vramTotalGb: number;
  /** Training iterations per second */
  itPerSec: number;
  /** Estimated seconds remaining */
  etaSeconds: number;
  /** Current forge cycle */
  cycle: number;
  /** Total forge cycles */
  totalCycles: number;
  /** ISO timestamp of last status update */
  timestamp: string;
}

/**
 * Model Forge Status Command Parameters
 */
export interface ModelForgeStatusParams extends CommandParams {
  /** Optional grid node ID to query. If omitted, returns status from all nodes. */
  nodeId?: string;
}

/**
 * Factory function for creating ModelForgeStatusParams
 */
export const createModelForgeStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    nodeId?: string;
  }
): ModelForgeStatusParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  nodeId: data.nodeId ?? '',
  ...data
});

/**
 * Model Forge Status Command Result
 */
export interface ModelForgeStatusResult extends CommandResult {
  success: boolean;
  /** List of active forge jobs */
  forges: ForgeJobStatus[];
  error?: JTAGError;
}

/**
 * Factory function for creating ModelForgeStatusResult with defaults
 */
export const createModelForgeStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    forges?: ForgeJobStatus[];
    error?: JTAGError;
  }
): ModelForgeStatusResult => createPayload(context, sessionId, {
  forges: data.forges ?? [],
  ...data
});

/**
 * Smart Model Forge Status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createModelForgeStatusResultFromParams = (
  params: ModelForgeStatusParams,
  differences: Omit<ModelForgeStatusResult, 'context' | 'sessionId' | 'userId'>
): ModelForgeStatusResult => transformPayload(params, differences);

/**
 * Model Forge Status — Type-safe command executor
 *
 * Usage:
 *   import { ModelForgeStatus } from '...shared/ModelForgeStatusTypes';
 *   const result = await ModelForgeStatus.execute({ ... });
 */
export const ModelForgeStatus = {
  execute(params: CommandInput<ModelForgeStatusParams>): Promise<ModelForgeStatusResult> {
    return Commands.execute<ModelForgeStatusParams, ModelForgeStatusResult>('model/forge-status', params as Partial<ModelForgeStatusParams>);
  },
  commandName: 'model/forge-status' as const,
} as const;
