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

/**
 * A single active forge job on a grid node.
 */
export interface ForgeJobStatus {
  nodeId: string;
  nodeName: string;
  phase: 'loading' | 'training' | 'pruning' | 'defrag' | 'saving' | 'generating_samples' | 'idle';
  detail: string;
  model: string;
  domain: string;
  step: number;
  totalSteps: number;
  loss: number;
  vramGb: number;
  vramTotalGb: number;
  itPerSec: number;
  etaSeconds: number;
  cycle: number;
  totalCycles: number;
  timestamp: string;
}

export interface ModelForgeStatusParams extends CommandParams {
  nodeId?: string;
}

export interface ModelForgeStatusResult extends CommandResult {
  success: boolean;
  forges: ForgeJobStatus[];
  error?: JTAGError;
}

export const createModelForgeStatusResultFromParams = (
  params: ModelForgeStatusParams,
  differences: Omit<ModelForgeStatusResult, 'context' | 'sessionId' | 'userId'>
): ModelForgeStatusResult => transformPayload(params, differences);

export const ModelForgeStatus = {
  execute(params: CommandInput<ModelForgeStatusParams>): Promise<ModelForgeStatusResult> {
    return Commands.execute<ModelForgeStatusParams, ModelForgeStatusResult>('model/forge-status', params as Partial<ModelForgeStatusParams>);
  },
  commandName: 'model/forge-status' as const,
} as const;
