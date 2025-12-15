import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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
