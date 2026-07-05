/**
 * Chat Poll Server Command - Get recent messages or messages after a marker
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ChatPollCommand } from '../shared/ChatPollCommand';
import type { ChatPollParams, ChatPollResult } from '../shared/ChatPollTypes';
import { ORM } from '@daemons/data-daemon/server/ORM';
import type { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { resolveRoomIdentifier } from '@system/routing/RoutingService';

export class ChatPollServerCommand extends ChatPollCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatPoll(params: ChatPollParams): Promise<ChatPollResult> {
    try {
      // Resolve room identifier (single source of truth: RoutingService)
      let roomId: UUID | undefined = params.roomId;

      if (params.room && !roomId) {
        const resolved = await resolveRoomIdentifier(params.room);
        if (resolved) {
          roomId = resolved.id;
        }
      }

      const filter: {timestamp?: {$gt: string}, roomId?: UUID} = {};

      if (params.afterMessageId) {
        // Get the original message to find its timestamp.
        const originalMessageResult = await ORM.query<ChatMessageEntity>({
          collection: 'chat_messages',
          filter: { id: params.afterMessageId },
          limit: 1
        }, 'default');

        if (!originalMessageResult.success || !originalMessageResult.data || originalMessageResult.data.length === 0) {
          return {
            context: params.context,
            sessionId: params.sessionId,
            success: false,
            messages: [],
            count: 0,
            afterMessageId: params.afterMessageId,
            timestamp: new Date().toISOString(),
            error: `Message not found: ${params.afterMessageId}`
          };
        }

        const originalMessage = originalMessageResult.data[0];

        // Build filter for messages after this one.
        const afterTimestamp = originalMessage.data.timestamp instanceof Date
          ? originalMessage.data.timestamp.toISOString()
          : originalMessage.data.timestamp;

        filter.timestamp = { $gt: afterTimestamp };
      }

      // Optional room filter (from roomId or resolved room name)
      if (roomId) {
        filter.roomId = roomId;
      }

      const sortDirection = params.afterMessageId ? 'asc' : 'desc';

      // Query messages. No afterMessageId means "latest messages"; this is
      // the ergonomic smoke-test/default read path for CLI and agents.
      const result = await ORM.query<ChatMessageEntity>({
        collection: 'chat_messages',
        filter,
        sort: [{ field: 'timestamp', direction: sortDirection }],
        limit: params.limit || 50
      }, 'default');

      if (!result.success || !result.data) {
        return {
          context: params.context,
          sessionId: params.sessionId,
          success: false,
          messages: [],
          count: 0,
          afterMessageId: params.afterMessageId,
          timestamp: new Date().toISOString(),
          error: 'Failed to query messages'
        };
      }

      // Extract entity data from DataRecord<ChatMessageEntity>[] and normalize
      // latest-mode back to chronological order for display/readability.
      const messages = result.data
        .map(record => record.data)
        .sort((a, b) => {
          const aTime = new Date(a.timestamp).getTime();
          const bTime = new Date(b.timestamp).getTime();
          return aTime - bTime;
        });

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        messages,
        count: messages.length,
        afterMessageId: params.afterMessageId,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        messages: [],
        count: 0,
        afterMessageId: params.afterMessageId,
        timestamp: new Date().toISOString(),
        error: `Chat poll failed: ${errorMessage}`
      };
    }
  }
}
