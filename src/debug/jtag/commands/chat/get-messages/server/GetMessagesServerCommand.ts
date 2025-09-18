/**
 * Get Messages Command - Server Implementation
 */

import { GetMessagesCommand } from '../shared/GetMessagesCommand';
import { type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GetMessagesParams, MessageData } from '../shared/GetMessagesTypes';
import type { ChatMessageData } from '../../../../system/data/domains/ChatMessage';
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
      console.log(`🔧 CLAUDE-DEBUG-${Date.now()}: GetMessages called with roomId=${params.roomId}, limit=${limit}`);

      // Use data/list with elegant domain conversion
      const dataListParams: DataListParams = {
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: { roomId: params.roomId },
        orderBy: [{ field: DOMAIN_FIELDS.TIMESTAMP, direction: 'desc' }], // Get newest first from DB
        limit: limit * 2, // Buffer for filtering
        convertToDomain: true, // ✨ Elegant domain conversion in data layer
        context: this.context,
        sessionId: params.sessionId
      };

      console.log(`🔧 CLAUDE-DEBUG-${Date.now()}: Calling data/list with params:`, JSON.stringify(dataListParams, null, 2));
      const typedResult = await this.remoteExecute<DataListParams, DataListResult<ChatMessageData>>(
        dataListParams,
        'data/list'
      );
      console.log(`🔧 CLAUDE-DEBUG-${Date.now()}: data/list result:`, JSON.stringify(typedResult, null, 2));

      if (!typedResult.success || !typedResult.items) {
        console.warn(`📚 Server: No messages found for room ${params.roomId}`);
        console.log(`🔧 CLAUDE-DEBUG-${Date.now()}: Result success=${typedResult.success}, items length=${typedResult.items?.length}`);
        return [];
      }

      // Data layer already converted to domain objects - just slice and reverse
      const domainMessages = typedResult.items.slice(0, limit).reverse();

      console.log(`📚 Server: Retrieved ${domainMessages.length} messages for room ${params.roomId}`);
      console.log(`🔧 CLAUDE-TIMESTAMP-DEBUG: First message timestamp: ${domainMessages[0]?.timestamp}`);

      // Return domain data directly - no .toData() method since ChatMessageData is interface
      return domainMessages as MessageData[];

    } catch (error) {
      console.error(`❌ Server: Failed to retrieve messages:`, error);
      throw error;
    }
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}