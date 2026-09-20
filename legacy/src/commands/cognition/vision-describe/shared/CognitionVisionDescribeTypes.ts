/**
 * Cognition Vision Describe Command - Shared Types
 *
 * Describe an image via the best available vision-capable model. Selects a vision-capable model from the Rust model registry, builds the describe prompt from option flags, dispatches `ai/generate` with multimodal content (text + base64 image), and parses the response into a VisionDescription. Migrated from `system/vision/VisionInferenceProvider.ts` per #1276 (oxidizer freeform-shape outlier — pairs with codex's #1284 structured-decision shape). Returns null when no vision model is registered or generation fails.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { VisionDescribeOptions, VisionDescription } from '@shared/generated/cognition';


/**
 * Cognition Vision Describe Command Parameters
 */
export interface CognitionVisionDescribeParams extends CommandParams {
  // Base64-encoded image bytes. The Rust adapter shapes this for the destination provider (Anthropic native base64, OpenAI image_url, llama.cpp mmproj).
  base64Data: string;
  // Image MIME type (e.g. 'image/png', 'image/jpeg').
  mimeType: string;
  // Per-call describe knobs (preferredModel, preferredProvider, maxLength, prompt override, detectObjects, detectColors, detectText). Defaults: concise prose with no structured-extraction prompts.
  options?: VisionDescribeOptions;
}

/**
 * Factory function for creating CognitionVisionDescribeParams
 */
export const createCognitionVisionDescribeParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
  data: {
    // Base64-encoded image bytes. The Rust adapter shapes this for the destination provider (Anthropic native base64, OpenAI image_url, llama.cpp mmproj).
    base64Data: string;
    // Image MIME type (e.g. 'image/png', 'image/jpeg').
    mimeType: string;
    // Per-call describe knobs (preferredModel, preferredProvider, maxLength, prompt override, detectObjects, detectColors, detectText). Defaults: concise prose with no structured-extraction prompts.
    options?: VisionDescribeOptions;
  },
): CognitionVisionDescribeParams => createPayload(context, sessionId, {
  userId,
  options: data.options ?? undefined,
  ...data,
});

/**
 * Cognition Vision Describe Command Result
 */
export interface CognitionVisionDescribeResult extends CommandResult {
  success: boolean;
  // Description envelope or null when no vision model is registered / generation failed. See shared/generated/cognition/VisionDescription.ts.
  result: VisionDescription | null;
  error?: JTAGError;
}

/**
 * Factory function for creating CognitionVisionDescribeResult with defaults
 */
export const createCognitionVisionDescribeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Description envelope or null when no vision model is registered / generation failed. See shared/generated/cognition/VisionDescription.ts.
    result: VisionDescription | null;
    error?: JTAGError;
  }
): CognitionVisionDescribeResult => createPayload(context, sessionId, {

  ...data
});

/**
 * Smart Cognition Vision Describe-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCognitionVisionDescribeResultFromParams = (
  params: CognitionVisionDescribeParams,
  differences: Omit<CognitionVisionDescribeResult, 'context' | 'sessionId' | 'userId'>
): CognitionVisionDescribeResult => transformPayload(params, differences);

/**
 * Cognition Vision Describe — Type-safe command executor
 *
 * Usage:
 *   import { CognitionVisionDescribe } from '...shared/CognitionVisionDescribeTypes';
 *   const result = await CognitionVisionDescribe.execute({ ... });
 */
export const CognitionVisionDescribe = {
  execute(params: CommandInput<CognitionVisionDescribeParams>): Promise<CognitionVisionDescribeResult> {
    return Commands.execute<CognitionVisionDescribeParams, CognitionVisionDescribeResult>('cognition/vision-describe', params as Partial<CognitionVisionDescribeParams>);
  },
  commandName: 'cognition/vision-describe' as const,
} as const;
