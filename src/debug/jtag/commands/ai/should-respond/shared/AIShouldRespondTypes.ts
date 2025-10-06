/**
 * AI Should-Respond Command Types
 *
 * Sentinel/Coordinator pattern: Use cheap AI to gate expensive AI responses
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { RAGContext } from '../../../../system/rag/shared/RAGTypes';

/**
 * Request for AI to decide if persona should respond
 */
export interface AIShouldRespondParams extends CommandParams {
  /** The persona being evaluated (e.g., "Teacher AI", "CodeReview AI") */
  readonly personaName: string;

  /** The persona's ID */
  readonly personaId: string;

  /** Full RAG context with conversation history, room members, etc. */
  readonly ragContext: RAGContext;

  /** The specific message that triggered this check */
  readonly triggerMessage: {
    readonly senderName: string;
    readonly content: string;
    readonly timestamp: string;
  };

  /** Optional: Override model (defaults to llama3.2:3b) */
  readonly model?: string;
}

/**
 * AI's decision on whether persona should respond
 */
export interface AIShouldRespondResult extends CommandResult {
  /** Should this persona respond? */
  readonly shouldRespond: boolean;

  /** Confidence score (0.0 - 1.0) */
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
}
