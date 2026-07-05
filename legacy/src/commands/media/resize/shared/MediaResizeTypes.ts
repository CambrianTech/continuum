/**
 * Media Resize Command Types
 *
 * Image resizing via sharp library with model-aware dimension calculation.
 * Enables PersonaUsers to receive appropriately-sized images based on their
 * model's context window capacity.
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Fit strategies for resizing
 */
export type FitStrategy = 'contain' | 'cover' | 'fill' | 'inside' | 'outside';

/**
 * Parameters for media resizing
 */
export interface MediaResizeParams extends CommandParams {
  /** Input image file path (required) */
  inputPath: string;

  /** Output file path (auto-generated if not provided) */
  outputPath?: string;

  // ===== DIMENSION SPECIFICATION =====
  /** Target width in pixels */
  width?: number;

  /** Target height in pixels */
  height?: number;

  /** Maximum width (maintains aspect ratio) */
  maxWidth?: number;

  /** Maximum height (maintains aspect ratio) */
  maxHeight?: number;

  // ===== MODEL-AWARE SIZING =====
  /** Model name for automatic dimension calculation (e.g., 'llama3.2:3b') */
  modelName?: string;

  /** Provider for scoped registry lookup (e.g., "candle") */
  providerName?: string;

  /** Target percentage of model's context window to use for image (default: 0.12 = 12%) */
  targetPercentage?: number;

  // ===== QUALITY OPTIONS =====
  /** Output quality (1-100, default: 80) */
  quality?: number;

  /** Fit strategy when both width and height specified (default: 'inside') */
  fit?: FitStrategy;

  /** Return base64-encoded image data in result (for AI vision consumption) */
  returnBase64?: boolean;
}

/**
 * Result of media resizing
 */
export interface MediaResizeResult extends CommandResult {
  success: boolean;

  /** Output file path */
  outputPath?: string;

  /** Original image dimensions */
  originalDimensions?: {
    width: number;
    height: number;
  };

  /** Resized image dimensions */
  resizedDimensions?: {
    width: number;
    height: number;
  };

  /** File size of output (bytes) */
  fileSize?: number;

  /** Processing time (milliseconds) */
  processingTime?: number;

  /** Base64-encoded image data (if returnBase64: true) */
  base64?: string;

  /** Estimated token count for base64 representation */
  estimatedTokens?: number;

  /** Model context window used (if modelName provided) */
  modelContextWindow?: number;

  /** Percentage of context window used by image */
  contextWindowPercentage?: number;

  /** Error message if processing failed */
  error?: string;
}

/**
 * Create MediaResizeResult from params and outcome
 */
export function createMediaResizeResult(
  params: MediaResizeParams,
  outcome: Partial<MediaResizeResult>
): MediaResizeResult {
  return {
    success: outcome.success ?? false,
    outputPath: outcome.outputPath,
    originalDimensions: outcome.originalDimensions,
    resizedDimensions: outcome.resizedDimensions,
    fileSize: outcome.fileSize,
    processingTime: outcome.processingTime,
    base64: outcome.base64,
    estimatedTokens: outcome.estimatedTokens,
    modelContextWindow: outcome.modelContextWindow,
    contextWindowPercentage: outcome.contextWindowPercentage,
    error: outcome.error,
    context: params.context,
    sessionId: params.sessionId
  };
}

/**
 * MediaResize — Type-safe command executor
 *
 * Usage:
 *   import { MediaResize } from '...shared/MediaResizeTypes';
 *   const result = await MediaResize.execute({ ... });
 */
export const MediaResize = {
  execute(params: CommandInput<MediaResizeParams>): Promise<MediaResizeResult> {
    return Commands.execute<MediaResizeParams, MediaResizeResult>('media/resize', params as Partial<MediaResizeParams>);
  },
  commandName: 'media/resize' as const,
} as const;
