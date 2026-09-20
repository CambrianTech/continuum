/**
 * Model Download Command - Shared Types
 *
 * Download a base model from HuggingFace to a local or remote grid node. Routes to GPU-capable node if needed. Wraps huggingface_hub snapshot_download with progress reporting via chat.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Model Download Command Parameters
 */
export interface ModelDownloadParams extends CommandParams {
  // HuggingFace model ID (e.g., 'Qwen/Qwen3.5-27B')
  modelId: string;
  // Target grid node IP or name. Default: local machine, or GPU node if model requires GPU.
  node?: string;
  // Specific revision/branch/tag to download. Default: main.
  revision?: string;
}

/**
 * Factory function for creating ModelDownloadParams
 */
export const createModelDownloadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // HuggingFace model ID (e.g., 'Qwen/Qwen3.5-27B')
    modelId: string;
    // Target grid node IP or name. Default: local machine, or GPU node if model requires GPU.
    node?: string;
    // Specific revision/branch/tag to download. Default: main.
    revision?: string;
  }
): ModelDownloadParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  node: data.node ?? '',
  revision: data.revision ?? '',
  ...data
});

/**
 * Model Download Command Result
 */
export interface ModelDownloadResult extends CommandResult {
  success: boolean;
  // Local path where the model was downloaded
  downloadPath: string;
  // Total download size in GB
  sizeGb: number;
  // Which grid node the model was downloaded to
  nodeId: string;
  error?: JTAGError;
}

/**
 * Factory function for creating ModelDownloadResult with defaults
 */
export const createModelDownloadResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Local path where the model was downloaded
    downloadPath?: string;
    // Total download size in GB
    sizeGb?: number;
    // Which grid node the model was downloaded to
    nodeId?: string;
    error?: JTAGError;
  }
): ModelDownloadResult => createPayload(context, sessionId, {
  downloadPath: data.downloadPath ?? '',
  sizeGb: data.sizeGb ?? 0,
  nodeId: data.nodeId ?? '',
  ...data
});

/**
 * Smart Model Download-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createModelDownloadResultFromParams = (
  params: ModelDownloadParams,
  differences: Omit<ModelDownloadResult, 'context' | 'sessionId' | 'userId'>
): ModelDownloadResult => transformPayload(params, differences);

/**
 * Model Download — Type-safe command executor
 *
 * Usage:
 *   import { ModelDownload } from '...shared/ModelDownloadTypes';
 *   const result = await ModelDownload.execute({ ... });
 */
export const ModelDownload = {
  execute(params: CommandInput<ModelDownloadParams>): Promise<ModelDownloadResult> {
    return Commands.execute<ModelDownloadParams, ModelDownloadResult>('model/download', params as Partial<ModelDownloadParams>);
  },
  commandName: 'model/download' as const,
} as const;
