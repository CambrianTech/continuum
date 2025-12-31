/**
 * Canvas Stroke List Command Types
 *
 * Retrieves strokes for a collaborative canvas.
 * Used to:
 * - Load all strokes when opening a canvas
 * - Replay drawing for new participants
 * - Export canvas as vector data
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CanvasTool, StrokePoint } from '@system/data/entities/CanvasStrokeEntity';

export interface CanvasStrokeListParams extends CommandParams {
  /** Activity ID (canvas instance) to get strokes for */
  canvasId: UUID;

  /** Optional: limit number of strokes (for pagination) */
  limit?: number;

  /** Optional: offset for pagination */
  offset?: number;

  /** Optional: filter by creator */
  creatorId?: UUID;

  /** Optional: filter by tool type */
  tool?: CanvasTool;

  /** Optional: get strokes after this timestamp (for incremental sync) */
  afterTimestamp?: string;
}

/**
 * Stroke data returned in list results
 */
export interface StrokeData {
  id: UUID;
  tool: CanvasTool;
  points: StrokePoint[];
  color: string;
  size: number;
  opacity?: number;
  compositeOp?: string;
  creatorId: UUID;
  creatorName: string;
  timestamp: string;
  bounds?: {
    minX: number;
    minY: number;
    maxX: number;
    maxY: number;
  };
}

export interface CanvasStrokeListResult extends CommandResult {
  success: boolean;

  /** List of strokes (in timestamp order for proper replay) */
  strokes?: StrokeData[];

  /** Total stroke count (for pagination) */
  total?: number;

  /** Error message if failed */
  error?: string;
}

/**
 * Factory function for creating CanvasStrokeListResult
 */
export const createCanvasStrokeListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<CanvasStrokeListResult>, 'context' | 'sessionId'>
): CanvasStrokeListResult => createPayload(context, sessionId, {
  success: true,
  ...data
});
