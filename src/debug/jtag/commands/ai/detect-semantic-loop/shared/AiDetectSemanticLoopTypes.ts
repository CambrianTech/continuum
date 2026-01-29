/**
 * Ai Detect Semantic Loop Command - Shared Types
 *
 * Detects if an AI's response is semantically too similar to recent messages, preventing repetitive loop behavior
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Ai Detect Semantic Loop Command Parameters
 */
export interface AiDetectSemanticLoopParams extends CommandParams {
  // The message text to check for similarity
  messageText: string;
  // UUID of the persona to check message history for
  personaId: string;
  // How many recent messages to compare against (default: 5)
  lookbackCount?: number;
  // Similarity threshold 0-1, higher = more strict (default: 0.85)
  similarityThreshold?: number;
  // Only check messages within this timeframe in minutes (default: 10)
  timeWindowMinutes?: number;
  // Room context - checks across all rooms if omitted
  roomId?: string;
}

/**
 * Factory function for creating AiDetectSemanticLoopParams
 */
export const createAiDetectSemanticLoopParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // The message text to check for similarity
    messageText: string;
    // UUID of the persona to check message history for
    personaId: string;
    // How many recent messages to compare against (default: 5)
    lookbackCount?: number;
    // Similarity threshold 0-1, higher = more strict (default: 0.85)
    similarityThreshold?: number;
    // Only check messages within this timeframe in minutes (default: 10)
    timeWindowMinutes?: number;
    // Room context - checks across all rooms if omitted
    roomId?: string;
  }
): AiDetectSemanticLoopParams => createPayload(context, sessionId, {
  lookbackCount: data.lookbackCount ?? 0,
  similarityThreshold: data.similarityThreshold ?? 0,
  timeWindowMinutes: data.timeWindowMinutes ?? 0,
  roomId: data.roomId ?? '',
  ...data
});

/**
 * Ai Detect Semantic Loop Command Result
 */
export interface AiDetectSemanticLoopResult extends CommandResult {
  success: boolean;
  // Whether the message is detected as a loop
  isLoop: boolean;
  // Similarity score of the closest match (0-1)
  maxSimilarity: number;
  // Details of similar messages found
  matches: Array<{messageId: string, similarity: number, timestamp: string, excerpt: string}>;
  // Recommendation for what to do with the message
  recommendation: 'ALLOW' | 'WARN' | 'BLOCK';
  // Human-readable explanation of the result
  explanation: string;
  error?: JTAGError;
}

/**
 * Factory function for creating AiDetectSemanticLoopResult with defaults
 */
export const createAiDetectSemanticLoopResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether the message is detected as a loop
    isLoop?: boolean;
    // Similarity score of the closest match (0-1)
    maxSimilarity?: number;
    // Details of similar messages found
    matches?: Array<{messageId: string, similarity: number, timestamp: string, excerpt: string}>;
    // Recommendation for what to do with the message
    recommendation?: 'ALLOW' | 'WARN' | 'BLOCK';
    // Human-readable explanation of the result
    explanation?: string;
    error?: JTAGError;
  }
): AiDetectSemanticLoopResult => createPayload(context, sessionId, {
  isLoop: data.isLoop ?? false,
  maxSimilarity: data.maxSimilarity ?? 0,
  matches: data.matches ?? [],
  recommendation: data.recommendation ?? 'ALLOW',
  explanation: data.explanation ?? '',
  ...data
});

/**
 * Smart Ai Detect Semantic Loop-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiDetectSemanticLoopResultFromParams = (
  params: AiDetectSemanticLoopParams,
  differences: Omit<AiDetectSemanticLoopResult, 'context' | 'sessionId'>
): AiDetectSemanticLoopResult => transformPayload(params, differences);

/**
 * AiDetectSemanticLoop — Type-safe command executor
 *
 * Usage:
 *   import { AiDetectSemanticLoop } from '...shared/AiDetectSemanticLoopTypes';
 *   const result = await AiDetectSemanticLoop.execute({ ... });
 */
export const AiDetectSemanticLoop = {
  execute(params: CommandInput<AiDetectSemanticLoopParams>): Promise<AiDetectSemanticLoopResult> {
    return Commands.execute<AiDetectSemanticLoopParams, AiDetectSemanticLoopResult>('ai/detect-semantic-loop', params as Partial<AiDetectSemanticLoopParams>);
  },
  commandName: 'ai/detect-semantic-loop' as const,
} as const;
