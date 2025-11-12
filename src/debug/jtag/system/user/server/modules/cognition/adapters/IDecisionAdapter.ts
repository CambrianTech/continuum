/**
 * Decision Adapter Interface
 *
 * Adapters implement different strategies for deciding if PersonaUser should respond.
 * Chain of Responsibility pattern: adapters try in order until one returns a decision.
 *
 * Adapter Types:
 * - FastPathDecisionAdapter: Check if mentioned → instant response
 * - LLMDecisionAdapter: Use LLM gating model → intelligent evaluation
 * - HeuristicDecisionAdapter: Bag-of-words scoring → fast approximation
 * - MultiModelDecisionAdapter: Vote across multiple models → high confidence
 */

import type { BaseEntity } from '../../../../../../system/data/entities/BaseEntity';
import type { UUID } from '../../../../../../types/CrossPlatformUUID';
import type { RAGContext } from '../../../../../../system/rag/shared/RAGTypes';

/**
 * Decision context passed to adapters
 *
 * DOMAIN-AGNOSTIC: Works for chat, game, code, web - any domain!
 * Using BaseEntity instead of ChatMessageEntity ensures cognition
 * is not permanently locked to chat.
 */
export interface DecisionContext<TEvent extends BaseEntity = BaseEntity> {
  // Trigger event being evaluated (domain-agnostic!)
  triggerEvent: TEvent;          // BaseEntity works for ANY domain
  eventContent: string;           // Extracted text/description from event

  // Persona making decision
  personaId: UUID;
  personaDisplayName: string;

  // Context flags
  senderIsHuman: boolean;
  isMentioned: boolean;

  // Configuration
  gatingModel?: string;  // Which LLM model to use for gating
  contextWindowMinutes?: number;  // Time window for RAG context
  minContextMessages?: number;  // Minimum messages for context
}

/**
 * Decision result from adapter
 */
export interface CognitiveDecision {
  // Core decision
  shouldRespond: boolean;
  confidence: number;  // 0.0-1.0
  reason: string;

  // Metadata
  model?: string;  // Which model/adapter made the decision

  // RAG context (for decision logging)
  filteredRagContext?: RAGContext;

  // Summary (for logging)
  ragContextSummary?: {
    totalMessages: number;
    filteredMessages: number;
    timeWindowMinutes?: number;
  };

  // Conversation history (for logging)
  conversationHistory?: Array<{
    name: string;
    content: string;
    timestamp?: number;
  }>;
}

/**
 * Decision adapter interface
 *
 * Adapters are tried in order (chain of responsibility).
 * If adapter returns null, try next adapter.
 * If adapter returns decision, use it and stop.
 */
export interface IDecisionAdapter {
  /**
   * Adapter name (for logging)
   */
  readonly name: string;

  /**
   * Evaluate if should respond
   *
   * @returns CognitiveDecision if adapter can decide, null if should try next adapter
   */
  evaluate(context: DecisionContext): Promise<CognitiveDecision | null>;
}
