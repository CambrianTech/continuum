/**
 * LLMAdapter - LLM-based decision adapter (fallback)
 *
 * Uses an LLM to decide whether to respond when:
 * - FastPath didn't trigger (no mention)
 * - ThermalAdapter didn't trigger (temperature too low)
 *
 * Philosophy: When simple rules fail, ask an AI to judge context and decide.
 * This is the "thoughtful consideration" path - slower but more nuanced.
 */

import type { IDecisionAdapter, DecisionContext, CognitiveDecision } from './IDecisionAdapter';
import type { BaseEntity } from '../../../../../data/entities/BaseEntity';
import type { ChatMessageEntity } from '../../../../../data/entities/ChatMessageEntity';
import { AIDecisionService } from '../../../../../ai/server/AIDecisionService';
import type { AIDecisionContext } from '../../../../../ai/server/AIDecisionService';
import { ChatRAGBuilder } from '../../../../../rag/builders/ChatRAGBuilder';

export class LLMAdapter implements IDecisionAdapter {
  readonly name = 'LLMAdapter';
  readonly priority = 10; // Lowest priority - runs last (fallback)

  /**
   * Evaluate using LLM gating (fallback adapter - always handles)
   *
   * @returns CognitiveDecision (never returns null - this is the final fallback)
   */
  async evaluate<TEvent extends BaseEntity>(context: DecisionContext<TEvent>): Promise<CognitiveDecision> {
    // For chat domain, use ChatRAGBuilder to get context
    if ('content' in context.triggerEvent && 'roomId' in context.triggerEvent) {
      const chatMessage = context.triggerEvent as any as ChatMessageEntity;

      // Build RAG context for LLM gating
      // Bug #5 fix: Let ChatRAGBuilder use default calculation (no modelId available here yet)
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

      // Filter to recent conversation (last 10 minutes by default)
      const contextWindowMinutes = context.contextWindowMinutes ?? 10;
      const cutoffTime = Date.now() - (contextWindowMinutes * 60 * 1000);
      const recentHistory = ragContext.conversationHistory.filter(
        (msg: { timestamp?: number }) => (msg.timestamp ?? 0) >= cutoffTime
      );

      // Build AI decision context
      const aiContext: AIDecisionContext = {
        personaId: context.personaId,
        personaName: context.personaDisplayName,
        roomId: chatMessage.roomId,
        triggerMessage: chatMessage,
        ragContext: {
          ...ragContext,
          conversationHistory: recentHistory
        }
      };

      // Use configured gating model (defaults to llama3.2:3b)
      const gatingModel = context.gatingModel ?? 'llama3.2:3b';

      // Call AIDecisionService for gating decision
      const result = await AIDecisionService.evaluateGating(aiContext, {
        model: gatingModel,
        temperature: 0.3
      });

      return {
        shouldRespond: result.shouldRespond,
        confidence: result.confidence,
        reason: result.reason,
        model: this.name
      };
    }

    // For non-chat domains, use simple heuristic for now
    // TODO: Implement domain-specific LLM gating
    return {
      shouldRespond: false,
      confidence: 0.3,
      reason: 'Non-chat domain not yet supported by LLM adapter',
      model: this.name
    };
  }
}
