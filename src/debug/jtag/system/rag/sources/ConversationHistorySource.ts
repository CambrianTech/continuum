/**
 * ConversationHistorySource - Loads chat message history for RAG context
 *
 * Features:
 * - Uses queryWithJoin to load messages + sender info in one query (4.5x faster)
 * - Role assignment: own messages = 'assistant', others = 'user'
 * - Chronological ordering (oldest first for LLM context)
 * - Media attachment metadata in message text
 * - Token budget-aware message limiting
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { LLMMessage } from '../shared/RAGTypes';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('ConversationHistorySource', 'rag');

// Estimate ~4 tokens per word, ~5 words per line average
const TOKENS_PER_MESSAGE_ESTIMATE = 50;

export class ConversationHistorySource implements RAGSource {
  readonly name = 'conversation-history';
  readonly priority = 80;  // High - conversation is core context
  readonly defaultBudgetPercent = 40;  // Gets largest share of budget

  isApplicable(_context: RAGSourceContext): boolean {
    // Always applicable - every RAG build needs conversation context
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    // Calculate max messages based on budget
    const budgetBasedLimit = Math.max(5, Math.floor(allocatedBudget / TOKENS_PER_MESSAGE_ESTIMATE));

    // CRITICAL: Respect latency-aware limit from ChatRAGBuilder if provided
    // This prevents timeout on slow local models by limiting input tokens
    const optionsLimit = context.options?.maxMessages;
    const maxMessages = optionsLimit ? Math.min(budgetBasedLimit, optionsLimit) : budgetBasedLimit;

    log.debug(`Message limit: ${maxMessages} (budget=${budgetBasedLimit}, latencyLimit=${optionsLimit ?? 'none'})`);

    try {
      type MessageWithSender = ChatMessageEntity & { sender?: { displayName: string; userType: string } };
      let messages: MessageWithSender[] = [];

      // Try queryWithJoin first (4.5x faster), fall back to regular query
      try {
        const result = await DataDaemon.queryWithJoin<MessageWithSender>({
          collection: ChatMessageEntity.collection,
          filter: { roomId: context.roomId },
          joins: [{
            collection: 'users',
            alias: 'sender',
            localField: 'senderId',
            foreignField: 'id',
            type: 'left',
            select: ['displayName', 'userType']
          }],
          sort: [{ field: 'timestamp', direction: 'desc' }],
          limit: maxMessages
        });

        if (result.success && result.data && result.data.length > 0) {
          messages = result.data.map((record: { data: MessageWithSender }) => record.data);
        }
      } catch (joinError: any) {
        // queryWithJoin not supported - fall back to regular query
        log.debug(`queryWithJoin not available (${joinError.message}), using regular query`);

        const result = await DataDaemon.query<ChatMessageEntity>({
          collection: ChatMessageEntity.collection,
          filter: { roomId: context.roomId },
          sort: [{ field: 'timestamp', direction: 'desc' }],
          limit: maxMessages
        });

        if (result.success && result.data && result.data.length > 0) {
          messages = result.data.map((record: { data: ChatMessageEntity }) => record.data as MessageWithSender);
        }
      }

      if (messages.length === 0) {
        return this.emptySection(startTime);
      }

      // Reverse to get oldest-first (LLMs expect chronological order)
      const orderedMessages = messages.reverse();

      // Convert to LLM message format
      const llmMessages: LLMMessage[] = orderedMessages.map((msg: MessageWithSender) => {
        let messageText = msg.content?.text || '';

        // Add media metadata to message text so AIs know images exist
        if (msg.content?.media && msg.content.media.length > 0) {
          const mediaDescriptions = msg.content.media.map((item: { type?: string; filename?: string; mimeType?: string }, idx: number) => {
            const parts = [
              `[${item.type || 'attachment'}${idx + 1}]`,
              item.filename || 'unnamed',
              item.mimeType ? `(${item.mimeType})` : ''
            ].filter(Boolean);
            return parts.join(' ');
          });

          const mediaNote = `\n[Attachments: ${mediaDescriptions.join(', ')} - messageId: ${msg.id}]`;
          messageText += mediaNote;
        }

        // Role assignment: own messages = 'assistant', others = 'user'
        const isOwnMessage = msg.senderId === context.personaId;
        const role = isOwnMessage ? 'assistant' as const : 'user' as const;

        // Get sender name from JOIN result or fallback
        const senderName = (msg as any).sender?.displayName || msg.senderName || 'Unknown';

        // Convert timestamp to number (milliseconds)
        let timestampMs: number | undefined;
        if (msg.timestamp) {
          if (typeof msg.timestamp === 'number') {
            timestampMs = msg.timestamp;
          } else if (typeof msg.timestamp === 'string') {
            timestampMs = new Date(msg.timestamp).getTime();
          } else if (msg.timestamp instanceof Date) {
            timestampMs = msg.timestamp.getTime();
          }
        }

        return {
          role,
          content: messageText,
          name: senderName,
          timestamp: timestampMs
        };
      });

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = llmMessages.reduce((sum, m) => sum + this.estimateTokens(m.content), 0);

      log.debug(`Loaded ${llmMessages.length} messages in ${loadTimeMs.toFixed(1)}ms (~${tokenCount} tokens)`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        messages: llmMessages,
        metadata: {
          messageCount: llmMessages.length,
          roomId: context.roomId,
          personaId: context.personaId
        }
      };
    } catch (error: any) {
      log.error(`Failed to load conversation history: ${error.message}`);
      return this.emptySection(startTime, error.message);
    }
  }

  private emptySection(startTime: number, error?: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      messages: [],
      metadata: error ? { error } : {}
    };
  }

  private estimateTokens(text: string): number {
    // Rough estimate: ~4 characters per token
    return Math.ceil(text.length / 4);
  }
}
