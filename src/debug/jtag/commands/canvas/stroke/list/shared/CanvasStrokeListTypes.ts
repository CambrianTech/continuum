/**
 * Canvas Stroke List Command Types
 *
 * Query strokes from a canvas for replay/rendering.
 * Supports pagination and viewport filtering.
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CanvasTool, StrokePoint, StrokeBounds } from '@system/data/entities/CanvasStrokeEntity';

export interface CanvasStrokeListParams extends CommandParams {
  /** Activity ID (canvas instance) to get strokes from */
  canvasId: UUID;

  /** Filter to specific viewport (for efficient rendering) */
  viewport?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };

  /** Maximum strokes to return (default: 1000) */
  limit?: number;

  /** Pagination cursor (stroke timestamp) */
  cursor?: string;
}

export interface StrokeSummary {
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
  bounds?: StrokeBounds;
}

/** Alias for widget compatibility */
export type StrokeData = StrokeSummary;

export interface CanvasStrokeListResult extends CommandResult {
  success: boolean;
  
  /** Canvas ID */
  canvasId?: UUID;
  
  /** Strokes in draw order (oldest first) */
  strokes?: StrokeSummary[];
  
  /** Total stroke count for canvas */
  totalCount?: number;
  
  /** Next cursor for pagination */
  nextCursor?: string;
  
  /** True if more strokes available */
  hasMore?: boolean;

  error?: string;
}

export const createCanvasStrokeListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<CanvasStrokeListResult>, 'context' | 'sessionId'>
): CanvasStrokeListResult => createPayload(context, sessionId, {
  success: true,
  ...data
});
