/**
 * Model Publish Command - Shared Types
 *
 * Publish a forged model to HuggingFace — safetensors, config, tokenizer, model card, and alloy provenance. This is the Factory's shipping department: the forge produces the artifact on a grid node, this command pushes it to HuggingFace where anyone can download it. Supports publishing from a local forged directory (bigmama-style) or from a grid node's finished/ station via grid/send.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Model Publish Command Parameters
 */
export interface ModelPublishParams extends CommandParams {
  // Path to the forged model directory containing safetensors, config.json, tokenizer files, and optionally the .alloy.json recipe
  forgedDir: string;
  // HuggingFace repo name (e.g., 'mixtral-8x7b-instruct-compacted-conservative'). Published under the org.
  repoName: string;
  // HuggingFace organization (default: 'continuum-ai')
  org?: string;
  // Path to a custom README.md model card. If omitted, a card is auto-generated from the alloy's results + metadata.
  cardPath?: string;
  // Path to the .alloy.json recipe file. If omitted, searches forgedDir for *.alloy.json. Included in the published repo as provenance.
  alloyPath?: string;
  // Include GGUF quantized files if present in the forged dir (default: true)
  includeGguf?: boolean;
  // Publish as private repo (default: false)
  private?: boolean;
  // Mark the model card as 'eval in progress' with placeholder benchmark fields. Use when publishing before eval completes. Card will be updated via model/update-card when eval finishes.
  evalPending?: boolean;
  // If forgedDir is on a remote grid node, specify the node ID. The command will use grid/send to execute the publish on the remote node.
  nodeId?: string;
  // Additional HuggingFace tags beyond the auto-generated ones
  tags?: string[];
}

/**
 * Factory function for creating ModelPublishParams
 */
export const createModelPublishParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to the forged model directory containing safetensors, config.json, tokenizer files, and optionally the .alloy.json recipe
    forgedDir: string;
    // HuggingFace repo name (e.g., 'mixtral-8x7b-instruct-compacted-conservative'). Published under the org.
    repoName: string;
    // HuggingFace organization (default: 'continuum-ai')
    org?: string;
    // Path to a custom README.md model card. If omitted, a card is auto-generated from the alloy's results + metadata.
    cardPath?: string;
    // Path to the .alloy.json recipe file. If omitted, searches forgedDir for *.alloy.json. Included in the published repo as provenance.
    alloyPath?: string;
    // Include GGUF quantized files if present in the forged dir (default: true)
    includeGguf?: boolean;
    // Publish as private repo (default: false)
    private?: boolean;
    // Mark the model card as 'eval in progress' with placeholder benchmark fields. Use when publishing before eval completes. Card will be updated via model/update-card when eval finishes.
    evalPending?: boolean;
    // If forgedDir is on a remote grid node, specify the node ID. The command will use grid/send to execute the publish on the remote node.
    nodeId?: string;
    // Additional HuggingFace tags beyond the auto-generated ones
    tags?: string[];
  }
): ModelPublishParams => createPayload(context, sessionId, {
  // userId is auto-injected by infrastructure at runtime
  org: data.org ?? '',
  cardPath: data.cardPath ?? '',
  alloyPath: data.alloyPath ?? '',
  includeGguf: data.includeGguf ?? false,
  private: data.private ?? false,
  evalPending: data.evalPending ?? false,
  nodeId: data.nodeId ?? '',
  tags: data.tags ?? [],
  ...data
}) as ModelPublishParams;

/**
 * Model Publish Command Result
 */
export interface ModelPublishResult extends CommandResult {
  success: boolean;
  // Whether the publish succeeded
  success: boolean;
  // Full HuggingFace repo URL (e.g., 'https://huggingface.co/continuum-ai/mixtral-8x7b-instruct-compacted-conservative')
  repoUrl: string;
  // HuggingFace repo ID (e.g., 'continuum-ai/mixtral-8x7b-instruct-compacted-conservative')
  repoId: string;
  // Number of files uploaded
  filesUploaded: number;
  // Total size of uploaded files in GB
  totalSizeGb: number;
  // Whether a model card was included
  cardIncluded: boolean;
  // Whether the alloy recipe was included
  alloyIncluded: boolean;
  error?: JTAGError;
}

/**
 * Factory function for creating ModelPublishResult with defaults
 */
export const createModelPublishResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether the publish succeeded
    success?: boolean;
    // Full HuggingFace repo URL (e.g., 'https://huggingface.co/continuum-ai/mixtral-8x7b-instruct-compacted-conservative')
    repoUrl?: string;
    // HuggingFace repo ID (e.g., 'continuum-ai/mixtral-8x7b-instruct-compacted-conservative')
    repoId?: string;
    // Number of files uploaded
    filesUploaded?: number;
    // Total size of uploaded files in GB
    totalSizeGb?: number;
    // Whether a model card was included
    cardIncluded?: boolean;
    // Whether the alloy recipe was included
    alloyIncluded?: boolean;
    error?: JTAGError;
  }
): ModelPublishResult => createPayload(context, sessionId, {
  success: data.success ?? false,
  repoUrl: data.repoUrl ?? '',
  repoId: data.repoId ?? '',
  filesUploaded: data.filesUploaded ?? 0,
  totalSizeGb: data.totalSizeGb ?? 0,
  cardIncluded: data.cardIncluded ?? false,
  alloyIncluded: data.alloyIncluded ?? false,
  ...data
});

/**
 * Smart Model Publish-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createModelPublishResultFromParams = (
  params: ModelPublishParams,
  differences: Omit<ModelPublishResult, 'context' | 'sessionId' | 'userId'>
): ModelPublishResult => transformPayload(params, differences);

/**
 * Model Publish — Type-safe command executor
 *
 * Usage:
 *   import { ModelPublish } from '...shared/ModelPublishTypes';
 *   const result = await ModelPublish.execute({ ... });
 */
export const ModelPublish = {
  execute(params: CommandInput<ModelPublishParams>): Promise<ModelPublishResult> {
    return Commands.execute<ModelPublishParams, ModelPublishResult>('model/publish', params as Partial<ModelPublishParams>);
  },
  commandName: 'model/publish' as const,
} as const;
