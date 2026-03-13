import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/** Analyze a chat room for duplicate messages, timestamp anomalies, and data integrity issues. */
export interface ChatAnalyzeParams extends CommandParams {
  /** Room ID (UUID) to analyze */
  roomId: UUID;

  /** Check for duplicate messages (default: true) */
  checkDuplicates?: boolean;

  /** Check for timestamp anomalies (default: true) */
  checkTimestamps?: boolean;

  /** Maximum messages to analyze (default: 500) */
  limit?: number;
}

export interface DuplicateGroup {
  content: string;
  contentHash: string;
  occurrences: Array<{
    messageId: string;
    shortId: string;
    authorId: string;
    timestamp: string;
  }>;
  count: number;
}

export interface TimestampAnomaly {
  type: 'out_of_order' | 'duplicate_timestamp' | 'large_gap' | 'rapid_burst';
  messageId: string;
  shortId: string;
  timestamp: string;
  details: string;
}

export interface ChatAnalyzeResult extends CommandResult {
  success: boolean;
  roomId: UUID;
  totalMessages: number;
  duplicates?: DuplicateGroup[];
  timestampAnomalies?: TimestampAnomaly[];
  analysis: {
    hasDuplicates: boolean;
    duplicateCount: number;
    hasTimestampIssues: boolean;
    anomalyCount: number;
  };
  error?: string;
}

/**
 * ChatAnalyze — Type-safe command executor
 *
 * Usage:
 *   import { ChatAnalyze } from '...shared/ChatAnalyzeTypes';
 *   const result = await ChatAnalyze.execute({ ... });
 */
export const ChatAnalyze = {
  execute(params: CommandInput<ChatAnalyzeParams>): Promise<ChatAnalyzeResult> {
    return Commands.execute<ChatAnalyzeParams, ChatAnalyzeResult>('collaboration/chat/analyze', params as Partial<ChatAnalyzeParams>);
  },
  commandName: 'collaboration/chat/analyze' as const,
} as const;

/**
 * Factory function for creating CollaborationChatAnalyzeParams
 */
export const createChatAnalyzeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatAnalyzeParams, 'context' | 'sessionId' | 'userId'>
): ChatAnalyzeParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationChatAnalyzeResult with defaults
 */
export const createChatAnalyzeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatAnalyzeResult, 'context' | 'sessionId' | 'userId'>
): ChatAnalyzeResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/chat/analyze-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createChatAnalyzeResultFromParams = (
  params: ChatAnalyzeParams,
  differences: Omit<ChatAnalyzeResult, 'context' | 'sessionId' | 'userId'>
): ChatAnalyzeResult => transformPayload(params, differences);

