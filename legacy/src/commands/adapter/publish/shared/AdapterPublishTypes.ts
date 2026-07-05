/**
 * Adapter Publish Command - Shared Types
 *
 * Publish a trained LoRA adapter to HuggingFace with auto-generated model card and continuum:* tags. The adapter manifest metadata (role, skill, scores, base model) becomes discoverable via adapter/search. Every published adapter is an advertisement for the Continuum ecosystem.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Adapter Publish Command Parameters
 */
export interface AdapterPublishParams extends CommandParams {
  // Path to the adapter directory containing adapter_model.safetensors and manifest.json
  adapterPath: string;
  // HuggingFace repo ID to publish to (e.g., 'continuum-ai/sprite-artist-pixel-games-qwen14b')
  repoId: string;
  // Project type tag (e.g., 'game-development', 'web-app', 'music-production')
  projectType?: string;
  // Academy session ID to pull exam scores and before/after data for the model card
  academySessionId?: string;
  // Team project ID to pull project context and role grades for the model card
  teamProjectId?: string;
  // Publish as private repo (default: false)
  private?: boolean;
  // Update existing repo instead of creating new (default: false). Pushes new weights + regenerates model card with latest training data.
  update?: boolean;
}

/**
 * Factory function for creating AdapterPublishParams
 */
export const createAdapterPublishParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to the adapter directory containing adapter_model.safetensors and manifest.json
    adapterPath: string;
    // HuggingFace repo ID to publish to (e.g., 'continuum-ai/sprite-artist-pixel-games-qwen14b')
    repoId: string;
    // Project type tag (e.g., 'game-development', 'web-app', 'music-production')
    projectType?: string;
    // Academy session ID to pull exam scores and before/after data for the model card
    academySessionId?: string;
    // Team project ID to pull project context and role grades for the model card
    teamProjectId?: string;
    // Publish as private repo (default: false)
    private?: boolean;
    // Update existing repo instead of creating new (default: false). Pushes new weights + regenerates model card with latest training data.
    update?: boolean;
  }
): AdapterPublishParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  projectType: data.projectType ?? '',
  academySessionId: data.academySessionId ?? '',
  teamProjectId: data.teamProjectId ?? '',
  private: data.private ?? false,
  update: data.update ?? false,
  ...data
});

/**
 * Adapter Publish Command Result
 */
export interface AdapterPublishResult extends CommandResult {
  success: boolean;
  // Full HuggingFace URL to the published adapter
  repoUrl: string;
  // Array of continuum:* tags applied to the repo
  tags: object;
  // Whether a model card with training data was auto-generated
  modelCardGenerated: boolean;
  error?: JTAGError;
}

/**
 * Factory function for creating AdapterPublishResult with defaults
 */
export const createAdapterPublishResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Full HuggingFace URL to the published adapter
    repoUrl?: string;
    // Array of continuum:* tags applied to the repo
    tags?: object;
    // Whether a model card with training data was auto-generated
    modelCardGenerated?: boolean;
    error?: JTAGError;
  }
): AdapterPublishResult => createPayload(context, sessionId, {
  repoUrl: data.repoUrl ?? '',
  tags: data.tags ?? {},
  modelCardGenerated: data.modelCardGenerated ?? false,
  ...data
});

/**
 * Smart Adapter Publish-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAdapterPublishResultFromParams = (
  params: AdapterPublishParams,
  differences: Omit<AdapterPublishResult, 'context' | 'sessionId' | 'userId'>
): AdapterPublishResult => transformPayload(params, differences);

/**
 * Adapter Publish — Type-safe command executor
 *
 * Usage:
 *   import { AdapterPublish } from '...shared/AdapterPublishTypes';
 *   const result = await AdapterPublish.execute({ ... });
 */
export const AdapterPublish = {
  execute(params: CommandInput<AdapterPublishParams>): Promise<AdapterPublishResult> {
    return Commands.execute<AdapterPublishParams, AdapterPublishResult>('adapter/publish', params as Partial<AdapterPublishParams>);
  },
  commandName: 'adapter/publish' as const,
} as const;
