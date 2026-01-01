/**
 * Canvas Stroke Add Server Command
 *
 * Saves stroke to database and emits real-time event for sync.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { CanvasStrokeAddParams, CanvasStrokeAddResult } from '../shared/CanvasStrokeAddTypes';
import { createCanvasStrokeAddResult, CANVAS_STROKE_EVENTS } from '../shared/CanvasStrokeAddTypes';
import { CanvasStrokeEntity } from '@system/data/entities/CanvasStrokeEntity';
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { Events } from '@system/core/shared/Events';
import { COLLECTIONS } from '@system/shared/Constants';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';

export class CanvasStrokeAddServerCommand extends CommandBase<CanvasStrokeAddParams, CanvasStrokeAddResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('canvas/stroke/add', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<CanvasStrokeAddResult> {
    const strokeParams = params as CanvasStrokeAddParams;
    const { canvasId, tool, points, color, size, opacity, compositeOp } = strokeParams;

    // Validate required params
    if (!canvasId) {
      return createCanvasStrokeAddResult(strokeParams.context, strokeParams.sessionId, {
        success: false,
        error: 'canvasId is required'
      });
    }

    if (!points || points.length === 0) {
      return createCanvasStrokeAddResult(strokeParams.context, strokeParams.sessionId, {
        success: false,
        error: 'points array is required and cannot be empty'
      });
    }

    try {
      // Get creator info from session
      const creatorId = strokeParams.sessionId;
      const creatorName = 'Unknown'; // TODO: Look up from session

      // Create stroke entity
      const stroke = new CanvasStrokeEntity();
      stroke.id = generateUUID();
      stroke.activityId = canvasId; // canvasId param maps to activityId field
      stroke.tool = tool || 'brush';
      stroke.points = points;
      stroke.color = color || '#000000';
      stroke.size = size || 5;
      stroke.opacity = opacity ?? 1;
      stroke.compositeOp = compositeOp;
      stroke.creatorId = creatorId;
      stroke.creatorName = creatorName;
      stroke.timestamp = new Date();

      // Calculate bounding box for efficient rendering
      stroke.calculateBounds();

      // Validate
      const validation = stroke.validate();
      if (!validation.success) {
        return createCanvasStrokeAddResult(strokeParams.context, strokeParams.sessionId, {
          success: false,
          error: validation.error
        });
      }

      // Save to database (throws on failure, returns entity on success)
      await DataDaemon.store(COLLECTIONS.CANVAS_STROKES, stroke);

      // Emit real-time event for all clients to sync
      Events.emit(CANVAS_STROKE_EVENTS.STROKE_ADDED, {
        canvasId,
        strokeId: stroke.id,
        stroke: {
          id: stroke.id,
          tool: stroke.tool,
          points: stroke.points,
          color: stroke.color,
          size: stroke.size,
          opacity: stroke.opacity,
          compositeOp: stroke.compositeOp,
          creatorId: stroke.creatorId,
          creatorName: stroke.creatorName,
          timestamp: stroke.timestamp.toISOString(),
          bounds: stroke.bounds
        }
      });

      return createCanvasStrokeAddResult(strokeParams.context, strokeParams.sessionId, {
        success: true,
        strokeId: stroke.id,
        timestamp: stroke.timestamp.toISOString()
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      console.error(`❌ CanvasStrokeAdd: Failed to add stroke:`, errorMessage);
      return createCanvasStrokeAddResult(strokeParams.context, strokeParams.sessionId, {
        success: false,
        error: errorMessage
      });
    }
  }
}
