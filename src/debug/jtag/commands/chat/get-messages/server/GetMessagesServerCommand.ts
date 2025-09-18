/**
 * Get Messages Command - Server Implementation
 */

import { GetMessagesCommand } from '../shared/GetMessagesCommand';
import { type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GetMessagesParams, MessageData } from '../shared/GetMessagesTypes';
import type { DataListParams, DataListResult } from '../../../../commands/data/list/shared/DataListTypes';
import { COLLECTIONS, DOMAIN_FIELDS } from '../../../../system/data/core/FieldMapping';

// Domain record format (what adapter returns after Date conversion)
interface ChatMessageRecord {
  readonly id: string;
  readonly createdAt: Date;
  readonly updatedAt: Date;
  readonly version: number;
  readonly roomId: string;
  readonly senderId: string;
  readonly content: {
    readonly text: string;
    readonly attachments: readonly unknown[];
    readonly formatting: {
      readonly markdown: boolean;
      readonly mentions: readonly string[];
      readonly hashtags: readonly string[];
      readonly links: readonly string[];
      readonly codeBlocks: readonly string[];
    };
  };
  readonly priority: string;
  readonly mentions: readonly string[];
  readonly metadata: {
    readonly source: string;
    readonly deviceType?: string;
  };
  readonly timestamp: string; // Compatibility alias
  readonly senderName?: string; // API compatibility
  readonly reactions?: unknown[]; // API compatibility
  readonly status?: string; // API compatibility
}

export class GetMessagesServerCommand extends GetMessagesCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async retrieveMessages(
    params: GetMessagesParams,
    limit: number
  ): Promise<MessageData[]> {
    try {
      // Use data/list as pure generic storage abstraction with proper chat-specific ordering
      const dataListParams: DataListParams = {
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: { roomId: params.roomId },
        orderBy: [{ field: DOMAIN_FIELDS.TIMESTAMP, direction: 'desc' }], // Get newest first from DB
        limit: limit * 2, // Buffer for filtering
        context: this.context,
        sessionId: params.sessionId
      };

      const result = await this.remoteExecute(dataListParams, 'data/list');
      const typedResult = result as DataListResult<MessageData>;

      if (!typedResult.success || !typedResult.items) {
        console.warn(`📚 Server: No messages found for room ${params.roomId}`);
        return [];
      }

      // Return storage data as-is - adapter handles conversion
      const messages = typedResult.items
        .slice(0, limit)
        .reverse();

      console.log(`📚 Server: Retrieved ${messages.length} messages for room ${params.roomId}`);
      console.log(`🔧 CLAUDE-TIMESTAMP-DEBUG: First message timestamp: ${messages[0]?.timestamp}`);
      return messages;

    } catch (error) {
      console.error(`❌ Server: Failed to retrieve messages:`, error);
      throw error;
    }
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}