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
import { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import { RoomEntity } from '@system/data/entities/RoomEntity';
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { Events } from '@system/core/shared/Events';
import { Commands } from '@system/core/shared/Commands';
import { COLLECTIONS } from '@system/shared/Constants';
import { ROOM_UNIQUE_IDS } from '@system/data/constants/RoomConstants';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { UserIdentityResolver } from '@system/user/shared/UserIdentityResolver';

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
      // Get creator info - use UserIdentityResolver to detect caller (Claude Code, Joel, etc.)
      const identity = await UserIdentityResolver.resolve();
      const creatorId = identity.userId || strokeParams.sessionId;
      const creatorName = identity.displayName;

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

      // Emit stroke as inbox content (system message in canvas room)
      // This is how AIs naturally see canvas activity in their conversation history
      await this.notifyCanvasRoom(stroke, strokeParams);

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

  /**
   * Notify the canvas room chat with a system message about the stroke
   * This is the "inbox content" pattern - AIs see stroke activity as chat messages
   */
  private async notifyCanvasRoom(stroke: CanvasStrokeEntity, params: CanvasStrokeAddParams): Promise<void> {
    try {
      // Find the canvas room
      const roomResult = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: RoomEntity.collection,
          filter: { name: ROOM_UNIQUE_IDS.CANVAS },
          limit: 1,
          context: params.context,
          sessionId: params.sessionId
        }
      );

      if (!roomResult.success || !roomResult.items || roomResult.items.length === 0) {
        console.warn('⚠️ CanvasStrokeAdd: Canvas room not found, skipping chat notification');
        return;
      }

      const canvasRoom = roomResult.items[0];

      // Describe the stroke for AI context
      const strokeDescription = this.describeStroke(stroke);

      // Create system message
      const messageEntity = new ChatMessageEntity();
      messageEntity.roomId = canvasRoom.id;
      messageEntity.senderId = stroke.creatorId;
      messageEntity.senderName = 'Canvas';  // System identifier
      messageEntity.senderType = 'system';
      messageEntity.content = {
        text: `[Canvas Activity] ${stroke.creatorName} drew: ${strokeDescription}`,
        media: []
      };
      messageEntity.status = 'sent';
      messageEntity.priority = 'normal';
      messageEntity.timestamp = stroke.timestamp;
      messageEntity.reactions = [];
      messageEntity.metadata = {
        source: 'system',
        // Store stroke reference for potential analysis
        toolResult: false  // Not a tool result, just activity notification
      };

      // Store message
      await Commands.execute<DataCreateParams, DataCreateResult<ChatMessageEntity>>(
        DATA_COMMANDS.CREATE,
        {
          collection: ChatMessageEntity.collection,
          data: messageEntity,
          context: params.context,
          sessionId: params.sessionId
        }
      );

      console.log(`🎨 CanvasStrokeAdd: Notified canvas room of stroke by ${stroke.creatorName}`);

    } catch (error) {
      // Don't fail the stroke add if notification fails
      console.error('⚠️ CanvasStrokeAdd: Failed to notify canvas room:', error);
    }
  }

  /**
   * Create a human-readable description of a stroke for AI context
   */
  private describeStroke(stroke: CanvasStrokeEntity): string {
    const parts: string[] = [];

    // Tool
    parts.push(stroke.tool);

    // Color (convert hex to readable if possible)
    const colorName = this.hexToColorName(stroke.color);
    parts.push(colorName);

    // Size category
    if (stroke.size <= 3) {
      parts.push('thin');
    } else if (stroke.size <= 8) {
      parts.push('medium');
    } else {
      parts.push('thick');
    }

    // Position/area from bounds
    if (stroke.bounds) {
      const centerX = Math.round((stroke.bounds.minX + stroke.bounds.maxX) / 2);
      const centerY = Math.round((stroke.bounds.minY + stroke.bounds.maxY) / 2);
      const width = Math.round(stroke.bounds.maxX - stroke.bounds.minX);
      const height = Math.round(stroke.bounds.maxY - stroke.bounds.minY);
      parts.push(`at (${centerX}, ${centerY}), size ${width}x${height}px`);
    }

    // Point count for complexity hint
    const pointCount = stroke.points?.length || 0;
    if (pointCount > 50) {
      parts.push('(detailed)');
    } else if (pointCount > 20) {
      parts.push('(moderate)');
    } else {
      parts.push('(simple)');
    }

    return parts.join(' ');
  }

  /**
   * Convert hex color to readable name
   */
  private hexToColorName(hex: string): string {
    const colorMap: Record<string, string> = {
      '#000000': 'black',
      '#ffffff': 'white',
      '#ff0000': 'red',
      '#00ff00': 'green',
      '#0000ff': 'blue',
      '#ffff00': 'yellow',
      '#ff00ff': 'magenta',
      '#00ffff': 'cyan',
      '#ffa500': 'orange',
      '#800080': 'purple',
      '#ffc0cb': 'pink',
      '#808080': 'gray',
      '#a52a2a': 'brown'
    };

    const normalized = hex.toLowerCase();
    return colorMap[normalized] || hex;
  }
}
