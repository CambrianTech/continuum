/**
 * Canvas Vision Command Types
 *
 * Enables AIs to "see" and interact with the drawing canvas:
 * - describe: Vision AI describes what's on the canvas
 * - transform: Use image generation to transform the sketch
 * - analyze: Structured analysis of the drawing
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export type VisionAction = 'describe' | 'transform' | 'analyze';

export interface CanvasVisionParams extends CommandParams {
  /** Action to perform */
  action: VisionAction;

  /** Base64 image data (optional - will capture from canvas widget if not provided) */
  imageBase64?: string;

  /** For describe/analyze: custom prompt/question about the image */
  prompt?: string;

  /** For transform: style/prompt for image generation */
  transformPrompt?: string;

  /** For transform: which model to use (dalle, sd, etc) */
  transformModel?: 'dalle-3' | 'dalle-2' | 'stable-diffusion';

  /** For describe: which vision model to use */
  visionModel?: string;

  /** Persona requesting the vision (for logging/attribution) */
  personaId?: string;
  personaName?: string;
}

export interface CanvasVisionResult extends CommandResult {
  success: boolean;
  action: VisionAction;

  /** For describe/analyze: the AI's description */
  description?: string;

  /** For analyze: structured analysis */
  analysis?: {
    objects: string[];
    colors: string[];
    composition: string;
    style: string;
    suggestions?: string[];
  };

  /** For transform: the generated image */
  generatedImage?: {
    base64: string;
    mimeType: string;
    width?: number;
    height?: number;
    model: string;
    prompt: string;
  };

  /** Error message if failed */
  error?: string;

  /** Which model was used */
  model?: string;

  /** Token/cost info if available */
  usage?: {
    inputTokens?: number;
    outputTokens?: number;
    cost?: number;
  };
}

/**
 * Factory function for creating CanvasVisionResult
 */
export const createCanvasVisionResult = (
  context: JTAGContext,
  sessionId: UUID,
  action: VisionAction,
  data: Omit<Partial<CanvasVisionResult>, 'context' | 'sessionId' | 'action'>
): CanvasVisionResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: true,
  action,
  ...data
});

/**
 * CanvasVision — Type-safe command executor
 *
 * Usage:
 *   import { CanvasVision } from '...shared/CanvasVisionTypes';
 *   const result = await CanvasVision.execute({ ... });
 */
export const CanvasVision = {
  execute(params: CommandInput<CanvasVisionParams>): Promise<CanvasVisionResult> {
    return Commands.execute<CanvasVisionParams, CanvasVisionResult>('canvas/vision', params as Partial<CanvasVisionParams>);
  },
  commandName: 'canvas/vision' as const,
} as const;
