import { Commands } from '@system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ChatAnalyzeCommand } from '../shared/ChatAnalyzeCommand';
import type {
  ChatAnalyzeParams,
  ChatAnalyzeResult,
  DuplicateGroup,
  TimestampAnomaly,
} from '../shared/ChatAnalyzeTypes';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import crypto from 'crypto';
import { resolveRoomIdentifier } from '@system/routing/RoutingService';

export class ChatAnalyzeServerCommand extends ChatAnalyzeCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatAnalyze(params: ChatAnalyzeParams): Promise<ChatAnalyzeResult> {
    const { roomId, checkDuplicates = true, checkTimestamps = true, limit = 500 } = params;

    // Resolve room name to UUID (single source of truth: RoutingService)
    const resolved = await resolveRoomIdentifier(roomId);
    if (!resolved) {
      return {
        success: false,
        roomId,
        totalMessages: 0,
        analysis: {
          hasDuplicates: false,
          duplicateCount: 0,
          hasTimestampIssues: false,
          anomalyCount: 0,
        },
        error: `Room not found: ${roomId}`,
      } as ChatAnalyzeResult;
    }
    const resolvedRoomId = resolved.id;

    // Get all messages from room
    const listResult = await Commands.execute<DataListParams, DataListResult<ChatMessageEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: ChatMessageEntity.collection,
        filter: { roomId: resolvedRoomId },
        orderBy: [{ field: 'timestamp', direction: 'asc' }],
        limit,
      }
    );

    if (!listResult.success || !listResult.items) {
      return {
        success: false,
        roomId,
        totalMessages: 0,
        analysis: {
          hasDuplicates: false,
          duplicateCount: 0,
          hasTimestampIssues: false,
          anomalyCount: 0,
        },
        error: `Failed to fetch messages: ${listResult.error || 'Unknown error'}`,
      } as ChatAnalyzeResult;
    }

    const messages = listResult.items as readonly ChatMessageEntity[];
    const duplicates = checkDuplicates ? this.findDuplicates(messages) : [];
    const timestampAnomalies = checkTimestamps ? this.findTimestampAnomalies(messages) : [];

    return {
      success: true,
      roomId,
      totalMessages: messages.length,
      duplicates,
      timestampAnomalies,
      analysis: {
        hasDuplicates: duplicates.length > 0,
        duplicateCount: duplicates.reduce((sum, g) => sum + g.count, 0),
        hasTimestampIssues: timestampAnomalies.length > 0,
        anomalyCount: timestampAnomalies.length,
      },
    } as ChatAnalyzeResult;
  }

  private findDuplicates(messages: readonly ChatMessageEntity[]): DuplicateGroup[] {
    const contentMap = new Map<string, DuplicateGroup>();

    for (const msg of messages) {
      // Create hash of content text (msg.content is MessageContent object)
      const contentText = msg.content?.text || '';
      const contentHash = crypto
        .createHash('sha256')
        .update(contentText)
        .digest('hex')
        .substring(0, 16);

      if (!contentMap.has(contentHash)) {
        contentMap.set(contentHash, {
          content: contentText.substring(0, 100), // First 100 chars
          contentHash,
          occurrences: [],
          count: 0,
        });
      }

      const group = contentMap.get(contentHash)!;
      group.occurrences.push({
        messageId: msg.id,
        shortId: msg.id.substring(0, 7),
        authorId: msg.senderId, // ChatMessageEntity uses senderId, not authorId
        timestamp: msg.timestamp.toISOString(), // Convert Date to string
      });
      group.count++;
    }

    // Only return groups with duplicates (count > 1)
    return Array.from(contentMap.values())
      .filter((group) => group.count > 1)
      .sort((a, b) => b.count - a.count); // Sort by most duplicates first
  }

  private findTimestampAnomalies(messages: readonly ChatMessageEntity[]): TimestampAnomaly[] {
    const anomalies: TimestampAnomaly[] = [];
    const timestamps = new Map<string, string[]>(); // timestamp ISO string -> messageIds

    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      const ts = msg.timestamp.getTime();
      const tsIso = msg.timestamp.toISOString();

      // Check for duplicate timestamps
      if (!timestamps.has(tsIso)) {
        timestamps.set(tsIso, []);
      }
      timestamps.get(tsIso)!.push(msg.id);

      if (i > 0) {
        const prevMsg = messages[i - 1];
        const prevTs = prevMsg.timestamp.getTime();
        const prevTsIso = prevMsg.timestamp.toISOString();

        // Check for out of order (should be sorted ascending)
        if (ts < prevTs) {
          anomalies.push({
            type: 'out_of_order',
            messageId: msg.id,
            shortId: msg.id.substring(0, 7),
            timestamp: tsIso,
            details: `Message timestamp ${tsIso} is before previous message ${prevTsIso}`,
          });
        }

        // Check for large gaps (> 5 minutes)
        const gap = ts - prevTs;
        if (gap > 5 * 60 * 1000) {
          anomalies.push({
            type: 'large_gap',
            messageId: msg.id,
            shortId: msg.id.substring(0, 7),
            timestamp: tsIso,
            details: `${Math.round(gap / 1000 / 60)} minute gap since previous message`,
          });
        }

        // Check for rapid bursts (< 100ms between messages from same author)
        if (gap < 100 && msg.senderId === prevMsg.senderId) {
          anomalies.push({
            type: 'rapid_burst',
            messageId: msg.id,
            shortId: msg.id.substring(0, 7),
            timestamp: tsIso,
            details: `${gap}ms between messages from same author`,
          });
        }
      }
    }

    // Check for duplicate timestamps
    for (const [timestamp, messageIds] of timestamps.entries()) {
      if (messageIds.length > 1) {
        for (const messageId of messageIds) {
          anomalies.push({
            type: 'duplicate_timestamp',
            messageId,
            shortId: messageId.substring(0, 7),
            timestamp,
            details: `${messageIds.length} messages share this exact timestamp`,
          });
        }
      }
    }

    return anomalies;
  }
}
