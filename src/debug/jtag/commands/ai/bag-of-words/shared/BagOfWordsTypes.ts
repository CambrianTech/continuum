/**
 * Bag of Words Command Types
 *
 * Orchestrates multi-persona conversations in a chat room.
 * "Bag of words" = collection of AI personas interacting naturally based on conversation context.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Orchestrate a multi-persona conversation in a chat room, selecting which AI personas participate and how they take turns responding.
 */
export interface BagOfWordsParams extends CommandParams {
  /**
   * Chat room ID where conversation happens
   */
  roomId: UUID;

  /**
   * Array of persona user IDs to participate
   * These are PersonaUser (AI) entities
   */
  personaIds: UUID[];

  /**
   * Initial message to kick off conversation
   * Optional - if not provided, personas start naturally
   */
  initialMessage?: string;

  /**
   * Maximum number of turns/messages in conversation
   * Default: unlimited (until manual stop)
   */
  maxTurns?: number;

  /**
   * Response strategy
   * - 'round-robin': Personas take turns in order
   * - 'free-for-all': Any persona can respond (based on should-respond logic)
   * - 'moderated': System controls who speaks when
   */
  strategy?: 'round-robin' | 'free-for-all' | 'moderated';

  /**
   * Delay between persona responses (milliseconds)
   * Simulates natural conversation pacing
   * Default: 1000ms (1 second)
   */
  responseDelay?: number;

  /**
   * Optional conversation goal/topic
   * Influences RAG context and persona behavior
   */
  topic?: string;

  /**
   * Whether to add human user as observer
   * Default: true
   */
  includeHumanObserver?: boolean;
}

/**
 * Factory function for creating BagOfWordsParams
 */
export const createBagOfWordsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<BagOfWordsParams, 'context' | 'sessionId'>
): BagOfWordsParams => createPayload(context, sessionId, data);

/**
 * Result from bag-of-words command
 */
export interface BagOfWordsResult extends CommandResult {
  /**
   * Conversation session ID
   */
  sessionId: UUID;

  /**
   * Number of messages exchanged
   */
  messageCount: number;

  /**
   * Participating personas
   */
  participants: Array<{
    personaId: UUID;
    personaName: string;
    messagesCount: number;
  }>;

  /**
   * Conversation status
   */
  status: 'active' | 'completed' | 'stopped';

  /**
   * When conversation started
   */
  startedAt: Date;

  /**
   * When conversation ended (if completed)
   */
  endedAt?: Date;
}

/**
 * Factory function for creating BagOfWordsResult
 */
export const createBagOfWordsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<BagOfWordsResult, 'context' | 'sessionId'>
): BagOfWordsResult => createPayload(context, sessionId, data);

/**
 * BagOfWords — Type-safe command executor
 *
 * Usage:
 *   import { BagOfWords } from '...shared/BagOfWordsTypes';
 *   const result = await BagOfWords.execute({ ... });
 */
export const BagOfWords = {
  execute(params: CommandInput<BagOfWordsParams>): Promise<BagOfWordsResult> {
    return Commands.execute<BagOfWordsParams, BagOfWordsResult>('ai/bag-of-words', params as Partial<BagOfWordsParams>);
  },
  commandName: 'ai/bag-of-words' as const,
} as const;
