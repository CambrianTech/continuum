/**
 * FastPathAdapter - High-priority decision adapter
 *
 * Handles cases where we ALWAYS want to respond without expensive LLM gating:
 * - Direct mentions by name (@PersonaName)
 *
 * Philosophy: If the human explicitly calls your name, you respond. Period.
 */

import type { IDecisionAdapter, DecisionContext, CognitiveDecision } from './IDecisionAdapter';
import type { BaseEntity } from '../../../../../data/entities/BaseEntity';
import { ChatRAGBuilder } from '../../../../../rag/builders/ChatRAGBuilder';
import type { ChatMessageEntity } from '../../../../../data/entities/ChatMessageEntity';

export class FastPathAdapter implements IDecisionAdapter {
  readonly name = 'FastPathAdapter';
  readonly priority = 100; // Highest priority - runs first

  /**
   * Evaluate if should respond (fast-path for mentions)
   *
   * @returns Decision if mentioned, null to try next adapter
   */
  async evaluate<TEvent extends BaseEntity>(context: DecisionContext<TEvent>): Promise<CognitiveDecision | null> {
    // Fast-path only handles direct mentions
    if (!context.isMentioned) {
      return null; // Try next adapter
    }

    // Build RAG context for decision logging
    // Bug #5 fix: Let ChatRAGBuilder use default calculation (no modelId available here yet)
    if ('content' in context.triggerEvent && 'roomId' in context.triggerEvent) {
      const chatMessage = context.triggerEvent as any as ChatMessageEntity;
      const ragBuilder = new ChatRAGBuilder();

      const ragContext = await ragBuilder.buildContext(
        chatMessage.roomId,
        context.personaId,
        {
          // No maxMessages or modelId - uses ChatRAGBuilder's conservative default (10)
          maxMemories: 0,
          includeArtifacts: false,
          includeMemories: false,
          currentMessage: {
            role: 'user',
            content: context.eventContent,
            name: chatMessage.senderName,
            timestamp: Date.now()
          }
        }
      );

      return {
        shouldRespond: true,
        confidence: 0.95 + Math.random() * 0.04, // 0.95-0.99
        reason: 'Directly mentioned by name - fast-path response',
        model: this.name,
        filteredRagContext: ragContext
      };
    }

    // Non-chat domain: simple response
    return {
      shouldRespond: true,
      confidence: 0.95,
      reason: 'Directly mentioned - fast-path response',
      model: this.name
    };
  }
}
