/**
 * AI Should-Respond Command Types
 *
 * Unified gating command with multiple strategies:
 * - fast: Bag-of-words scoring (<1ms, no LLM)
 * - llm: Deep reasoning with LLM (slower, more accurate)
 * - hybrid: Fast filter → LLM confirmation
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { RAGContext } from '../../../../system/rag/shared/RAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Response detection strategy
 */
export type ResponseStrategy = 'fast' | 'llm' | 'hybrid';

/**
 * Request for AI to decide if persona should respond
 */
export interface AIShouldRespondParams extends CommandParams {
  /** The persona being evaluated (e.g., "Teacher AI", "CodeReview AI") */
  readonly personaName: string;

  /** The persona's ID */
  readonly personaId: UUID;

  /** Context ID (room, thread, etc) */
  readonly contextId: UUID;

  /** Full RAG context with conversation history, room members, etc. */
  readonly ragContext?: RAGContext;

  /** The specific message that triggered this check */
  readonly triggerMessage: {
    readonly senderName: string;
    readonly content: string;
    readonly timestamp: string;
  };

  /** Detection strategy (default: 'fast') */
  readonly strategy?: ResponseStrategy;

  /** Optional: Override model (defaults to llama3.2:3b for LLM strategy) */
  readonly model?: string;

  /** Verbose mode - include full RAG context and prompt in response */
  readonly verbose?: boolean;

  /** Optional: Domain keywords for fast strategy */
  readonly domainKeywords?: string[];

  /** Optional: Response threshold for fast strategy (default: 50) */
  readonly responseThreshold?: number;
}

/**
 * AI's decision on whether persona should respond
 */
export interface AIShouldRespondResult extends CommandResult {
  /** Should this persona respond? */
  readonly shouldRespond: boolean;

  /** Confidence score (0.0 - 1.0 for LLM, 0-100+ for fast strategy) */
  readonly confidence: number;

  /** Explanation of decision (for debugging/tuning) */
  readonly reason: string;

  /** Error message if decision failed */
  readonly error?: string;

  /** Which factors influenced the decision */
  readonly factors: {
    readonly mentioned: boolean;
    readonly questionAsked: boolean;
    readonly domainRelevant: boolean;
    readonly recentlySpoke: boolean;
    readonly othersAnswered: boolean;
  };

  /** Verbose mode output - shows what AI saw */
  readonly debug?: {
    readonly ragContext: {
      readonly messageCount: number;
      readonly conversationPreview: string;  // First 500 chars
    };
    readonly promptSent: string;  // The actual prompt sent to LLM
    readonly aiResponse: string;  // Raw AI response
  };

  /** Score breakdown (for fast strategy) */
  readonly scoreBreakdown?: {
    readonly directMention: number;
    readonly domainKeywords: number;
    readonly conversationContext: number;
    readonly isQuestion: number;
    readonly publicMessage: number;
    readonly roomActivity: number;
  };

  /** Detection signals (for fast strategy) */
  readonly signals?: {
    readonly wasMentioned: boolean;
    readonly matchedKeywords: string[];
    readonly isQuestion: boolean;
    readonly recentlyActive: boolean;
  };
}
