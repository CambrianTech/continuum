/**
 * Model List Published Command - Shared Types
 *
 * List all published models from the continuum-ai HuggingFace org.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * A published model on HuggingFace.
 */
export interface PublishedModelInfo {
  id: string;
  name: string;
  baseModel: string;
  domain: string;
  improvementPct: number;
  downloads: number;
  likes: number;
  sizeGb: number;
  variant: string;
  tags: string[];
  lastModified: string;
}

export interface ModelListPublishedParams extends CommandParams {
  domain?: string;
  includeGguf?: boolean;
}

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

export interface ModelListPublishedResult extends CommandResult {
  success: boolean;
  models: PublishedModelInfo[];
  totalDownloads: number;
  totalModels: number;
  error?: JTAGError;
}

export const createModelListPublishedResultFromParams = (
  params: ModelListPublishedParams,
  differences: Omit<ModelListPublishedResult, 'context' | 'sessionId' | 'userId'>
): ModelListPublishedResult => transformPayload(params, differences);

export const ModelListPublished = {
  execute(params: CommandInput<ModelListPublishedParams>): Promise<ModelListPublishedResult> {
    return Commands.execute<ModelListPublishedParams, ModelListPublishedResult>('model/list-published', params as Partial<ModelListPublishedParams>);
  },
  commandName: 'model/list-published' as const,
} as const;
