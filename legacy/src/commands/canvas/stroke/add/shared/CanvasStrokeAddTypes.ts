/**
 * Canvas Stroke Add Command Types
 *
 * Adds a stroke to a collaborative canvas.
 * Strokes are atomic drawing operations created by human or AI users.
 *
 * Real-time Collaboration:
 * - Server saves stroke to database
 * - Server emits 'canvas:stroke:added' event
 * - All clients receive and render the stroke
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CanvasTool, StrokePoint } from '@system/data/entities/CanvasStrokeEntity';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface CanvasStrokeAddParams extends CommandParams {
  /** Activity ID (canvas instance) to add stroke to */
  canvasId: UUID;

  /** Drawing tool used */
  tool: CanvasTool;

  /** Stroke points: [[x1,y1], [x2,y2], ...] or [[x1,y1,pressure1], ...] */
  points: StrokePoint[];

  /** Stroke color (CSS color string) */
  color: string;

  /** Brush/tool size in pixels */
  size: number;

  /** Optional: stroke opacity (0-1) */
  opacity?: number;

  /** Optional: composite operation for blend modes */
  compositeOp?: string;
}

export interface CanvasStrokeAddResult extends CommandResult {
  success: boolean;

  /** ID of the created stroke (for undo/delete) */
  strokeId?: UUID;

  /** Error message if failed */
  error?: string;

  /** Timestamp when stroke was added */
  timestamp?: string;
}

/**
 * Factory function for creating CanvasStrokeAddResult
 */
export const createCanvasStrokeAddResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<CanvasStrokeAddResult>, 'context' | 'sessionId'>
): CanvasStrokeAddResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  success: true,
  ...data
});

/**
 * Canvas stroke events for real-time sync
 */
export const CANVAS_STROKE_EVENTS = {
  STROKE_ADDED: 'canvas:stroke:added',
  STROKE_DELETED: 'canvas:stroke:deleted',
  CANVAS_CLEARED: 'canvas:cleared'
} as const;

/**
 * CanvasStrokeAdd — Type-safe command executor
 *
 * Usage:
 *   import { CanvasStrokeAdd } from '...shared/CanvasStrokeAddTypes';
 *   const result = await CanvasStrokeAdd.execute({ ... });
 */
export const CanvasStrokeAdd = {
  execute(params: CommandInput<CanvasStrokeAddParams>): Promise<CanvasStrokeAddResult> {
    return Commands.execute<CanvasStrokeAddParams, CanvasStrokeAddResult>('canvas/stroke/add', params as Partial<CanvasStrokeAddParams>);
  },
  commandName: 'canvas/stroke/add' as const,
} as const;
