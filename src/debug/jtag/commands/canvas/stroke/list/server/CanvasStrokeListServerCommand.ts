/**
 * Canvas Stroke List Server Command
 *
 * Returns strokes from a canvas for replay/rendering.
 * Supports pagination and viewport filtering.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { CanvasStrokeListParams, CanvasStrokeListResult, StrokeSummary } from '../shared/CanvasStrokeListTypes';
import { createCanvasStrokeListResult } from '../shared/CanvasStrokeListTypes';
import { CanvasStrokeEntity } from '@system/data/entities/CanvasStrokeEntity';
import { Commands } from '@system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { COLLECTIONS } from '@system/shared/Constants';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { BaseEntity } from '@system/data/entities/BaseEntity';

export class CanvasStrokeListServerCommand extends CommandBase<CanvasStrokeListParams, CanvasStrokeListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('canvas/stroke/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<CanvasStrokeListResult> {
    const listParams = params as CanvasStrokeListParams;
    const { canvasId, viewport, limit = 1000, cursor } = listParams;

    if (!canvasId) {
      return createCanvasStrokeListResult(listParams.context, listParams.sessionId, {
        success: false,
        error: 'canvasId is required'
      });
    }

    try {
      // Build query filter
      const filter: Record<string, unknown> = {
        activityId: canvasId
      };

      // Add cursor for pagination
      if (cursor) {
        filter.timestamp = { $gt: cursor };
      }

      // Query strokes (oldest first for proper replay)
      const result = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
        collection: COLLECTIONS.CANVAS_STROKES,
        filter,
        orderBy: [{ field: 'timestamp', direction: 'asc' }],
        limit: limit + 1, // Get one extra to check hasMore
        context: listParams.context,
        sessionId: listParams.sessionId
      });

      let strokes: CanvasStrokeEntity[] = (result.items || []) as CanvasStrokeEntity[];
      const hasMore = strokes.length > limit;
      if (hasMore) {
        strokes = strokes.slice(0, limit);
      }

      // Apply viewport filtering if specified
      if (viewport) {
        strokes = strokes.filter(stroke => {
          const entity = Object.assign(new CanvasStrokeEntity(), stroke);
          return entity.intersectsRect(viewport.x, viewport.y, viewport.width, viewport.height);
        });
      }

      // Convert to summary format
      const strokeSummaries: StrokeSummary[] = strokes.map(stroke => ({
        id: stroke.id,
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
      }));

      // Get next cursor if paginating
      const nextCursor = hasMore && strokes.length > 0 
        ? (strokes[strokes.length - 1].timestamp instanceof Date 
          ? strokes[strokes.length - 1].timestamp.toISOString() 
          : String(strokes[strokes.length - 1].timestamp))
        : undefined;

      return createCanvasStrokeListResult(listParams.context, listParams.sessionId, {
        success: true,
        canvasId,
        strokes: strokeSummaries,
        totalCount: result.count,
        nextCursor,
        hasMore
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
