/**
 * Canvas Stroke List Server Command
 *
 * Retrieves strokes from database for canvas playback.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { CanvasStrokeListParams, CanvasStrokeListResult, StrokeData } from '../shared/CanvasStrokeListTypes';
import { createCanvasStrokeListResult } from '../shared/CanvasStrokeListTypes';
import { CanvasStrokeEntity } from '@system/data/entities/CanvasStrokeEntity';
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import type { UniversalFilter } from '@daemons/data-daemon/shared/DataStorageAdapter';
import { COLLECTIONS } from '@system/shared/Constants';

export class CanvasStrokeListServerCommand extends CommandBase<CanvasStrokeListParams, CanvasStrokeListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('canvas/stroke/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<CanvasStrokeListResult> {
    const listParams = params as CanvasStrokeListParams;
    const { canvasId, limit, offset, creatorId, tool, afterTimestamp } = listParams;

    // Validate required params
    if (!canvasId) {
      return createCanvasStrokeListResult(listParams.context, listParams.sessionId, {
        success: false,
        error: 'canvasId is required'
      });
    }

    try {
      // Build filter - canvasId param maps to activityId field
      const filter: UniversalFilter = { activityId: canvasId };

      if (creatorId) {
        filter.creatorId = creatorId;
      }

      if (tool) {
        filter.tool = tool;
      }

      if (afterTimestamp) {
        filter.timestamp = { $gt: afterTimestamp };
      }

      // Query database using DataDaemon.query
      const result = await DataDaemon.query<CanvasStrokeEntity>({
        collection: COLLECTIONS.CANVAS_STROKES,
        filter,
        sort: [{ field: 'timestamp', direction: 'asc' }], // Oldest first for replay
        limit: limit || 1000,
        offset: offset || 0
      });

      if (!result.success) {
        return createCanvasStrokeListResult(listParams.context, listParams.sessionId, {
          success: false,
          error: result.error || 'Failed to retrieve strokes'
        });
      }

      // Convert DataRecord wrappers to StrokeData
      // DataRecord has: { id, collection, data: T, metadata }
      const strokes: StrokeData[] = (result.data || []).map((record) => {
        const stroke = record.data;
        return {
          id: record.id,
          tool: stroke.tool,
          points: stroke.points,
          color: stroke.color,
          size: stroke.size,
          opacity: stroke.opacity,
          compositeOp: stroke.compositeOp,
          creatorId: stroke.creatorId,
          creatorName: stroke.creatorName,
          timestamp: stroke.timestamp instanceof Date ? stroke.timestamp.toISOString() : String(stroke.timestamp),
          bounds: stroke.bounds
        };
      });

      return createCanvasStrokeListResult(listParams.context, listParams.sessionId, {
        success: true,
        strokes,
        total: strokes.length
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      console.error(`❌ CanvasStrokeList: Failed to list strokes:`, errorMessage);
      return createCanvasStrokeListResult(listParams.context, listParams.sessionId, {
        success: false,
        error: errorMessage
      });
    }
  }
}
