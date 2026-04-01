/**
 * Model List Published Command - Shared Types
 *
 * List all published models from the continuum-ai HuggingFace org —
 * download counts, likes, improvement scores, hardware targets, tags.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/**
 * Information about a published model on HuggingFace
 */
export interface PublishedModelInfo {
  /** Full HuggingFace model ID (org/name) */
  id: string;
  /** Short model name */
  name: string;
  /** Base model this was forged from */
  baseModel: string;
  /** Training domain (code, reasoning, general, chat) */
  domain: string;
  /** Improvement percentage vs base model */
  improvementPct: number;
  /** Total download count */
  downloads: number;
  /** Total likes */
  likes: number;
  /** Model size in GB */
  sizeGb: number;
  /** Model variant (forged, compacted, defragged, gguf, mlx) */
  variant: string;
  /** HuggingFace tags */
  tags: string[];
  /** Last modified ISO timestamp */
  lastModified: string;
}

/**
 * Model List Published Command Parameters
 */
export interface ModelListPublishedParams extends CommandParams {
  /** Filter by domain (code, reasoning, general). If omitted, returns all. */
  domain?: string;
  /** Include GGUF variant repos in the list. Default false. */
  includeGguf?: boolean;
}

/**
 * Factory function for creating ModelListPublishedParams
 */
export const createModelListPublishedParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    domain?: string;
    includeGguf?: boolean;
  }
): ModelListPublishedParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  domain: data.domain ?? '',
  includeGguf: data.includeGguf ?? false,
  ...data
});

/**
 * Model List Published Command Result
 */
export interface ModelListPublishedResult extends CommandResult {
  success: boolean;
  /** List of published models */
  models: PublishedModelInfo[];
  /** Sum of all model downloads */
  totalDownloads: number;
  /** Number of published models */
  totalModels: number;
  error?: JTAGError;
}

/**
 * Factory function for creating ModelListPublishedResult with defaults
 */
export const createModelListPublishedResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    models?: PublishedModelInfo[];
    totalDownloads?: number;
    totalModels?: number;
    error?: JTAGError;
  }
): ModelListPublishedResult => createPayload(context, sessionId, {
  models: data.models ?? [],
  totalDownloads: data.totalDownloads ?? 0,
  totalModels: data.totalModels ?? 0,
  ...data
});

/**
 * Smart Model List Published-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createModelListPublishedResultFromParams = (
  params: ModelListPublishedParams,
  differences: Omit<ModelListPublishedResult, 'context' | 'sessionId' | 'userId'>
): ModelListPublishedResult => transformPayload(params, differences);

/**
 * Model List Published — Type-safe command executor
 *
 * Usage:
 *   import { ModelListPublished } from '...shared/ModelListPublishedTypes';
 *   const result = await ModelListPublished.execute({ ... });
 */
export const ModelListPublished = {
  execute(params: CommandInput<ModelListPublishedParams>): Promise<ModelListPublishedResult> {
    return Commands.execute<ModelListPublishedParams, ModelListPublishedResult>('model/list-published', params as Partial<ModelListPublishedParams>);
  },
  commandName: 'model/list-published' as const,
} as const;
