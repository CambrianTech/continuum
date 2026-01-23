import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { RAGLoadParams, RAGLoadResult, LoadedMessage } from '../shared/RAGLoadTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import { getContextWindow } from '../../../../system/shared/ModelContextWindows';
import { contentPreview, getTextSafe } from '../../../../shared/utils/StringUtils';

/**
 * RAG Load Server Command - Test incremental message loading
 *
 * Transparent implementation of incremental loading with token counting.
 * Shows exactly which messages get loaded and why.
 */
export class RAGLoadServerCommand extends CommandBase<RAGLoadParams, RAGLoadResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('rag/load', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RAGLoadResult> {
    const ragParams = params as RAGLoadParams;

    try {
      // Resolve room ID (accept either roomId UUID or room name)
      let roomId = ragParams.roomId;
      if (!roomId && ragParams.room) {
        try {
          const room = await this.findRoom(ragParams.room);
          roomId = room.id;
        } catch (error) {
          return {
            ...ragParams,
            success: false,
            roomId: ragParams.room as any,
            model: ragParams.model,
            contextWindow: 0,
            tokenBudget: 0,
            messagesInRoom: 0,
            messagesLoaded: 0,
            tokensUsed: 0,
            budgetUtilization: 0,
            messages: [],
            wouldFitInContext: false,
            totalWithCompletion: 0,
            error: error instanceof Error ? error.message : `Room not found: ${ragParams.room}`
          };
        }
      }

      if (!roomId) {
        return {
          ...ragParams,
          success: false,
          roomId: '' as any,
          model: ragParams.model,
          contextWindow: 0,
          tokenBudget: 0,
          messagesInRoom: 0,
          messagesLoaded: 0,
          tokensUsed: 0,
          budgetUtilization: 0,
          messages: [],
          wouldFitInContext: false,
          totalWithCompletion: 0,
          error: 'Must provide either roomId or room parameter'
        };
      }

      const model = ragParams.model;
      const maxTokens = ragParams.maxTokens ?? 3000;
      const systemPromptTokens = ragParams.systemPromptTokens ?? 500;
      const targetUtilization = ragParams.targetUtilization ?? 0.8;
      const showMessageContent = ragParams.showMessageContent ?? false;

      // Get context window from centralized configuration
      const contextWindow = getContextWindow(model);
      const availableForMessages = contextWindow - maxTokens - systemPromptTokens;
      const tokenBudget = Math.floor(availableForMessages * targetUtilization);

      // Load all messages from room (most recent first)
      const result = await DataDaemon.query<ChatMessageEntity>({
        collection: ChatMessageEntity.collection,
        filter: { roomId },
        limit: 100 // Cap at 100 for safety
      });

      if (!result.success || !result.data) {
        return {
          ...ragParams,
          success: false,
          roomId,
          model,
          contextWindow,
          tokenBudget,
          messagesInRoom: 0,
          messagesLoaded: 0,
          tokensUsed: 0,
          budgetUtilization: 0,
          messages: [],
          wouldFitInContext: false,
          totalWithCompletion: 0,
          error: result.error || 'Failed to load messages'
        };
      }

      const allMessages = result.data;
      const messagesInRoom = allMessages.length;

      // Incrementally load messages until budget exhausted
      const loadedMessages: LoadedMessage[] = [];
      let tokensUsed = 0;

      for (const messageRecord of allMessages) {
        const message = messageRecord.data;

        // Assign DataRecord.id to entity.id (entity.id may not be populated)
        message.id = messageRecord.id;

        // Skip messages with missing data
        if (!message || !message.id || !message.content || !message.content.text) {
          console.warn('⚠️ Skipping message with missing data', {
            hasMessage: !!message,
            hasId: !!message?.id,
            hasContent: !!message?.content,
            hasText: !!message?.content?.text
          });
          continue;
        }

        // Estimate tokens for this message
        // Format: "name: content" with metadata overhead
        const messageText = `${message.senderName || 'Unknown'}: ${message.content.text}`;
        const estimatedTokens = this.estimateTokens(messageText);

        // Would this exceed budget?
        if (tokensUsed + estimatedTokens > tokenBudget) {
          console.log(`🛑 Token budget reached (${tokensUsed}/${tokenBudget}), stopping at ${loadedMessages.length} messages`);
          break;
        }

        // Add to loaded messages
        tokensUsed += estimatedTokens;
        loadedMessages.push({
          messageId: message.id,
          shortId: message.id.slice(0, 8),
          timestamp: typeof message.timestamp === 'string' ? message.timestamp : new Date(message.timestamp).toISOString(),
          senderName: message.senderName || 'Unknown',
          contentPreview: contentPreview(message.content, 50),
          fullContent: showMessageContent ? getTextSafe(message.content) : undefined,
          estimatedTokens,
          cumulativeTokens: tokensUsed
        });
      }

      const budgetUtilization = (tokensUsed / tokenBudget) * 100;
      const totalWithCompletion = systemPromptTokens + tokensUsed + maxTokens;
      const wouldFitInContext = totalWithCompletion <= contextWindow;

      console.log(`📚 RAG Load Results:
  Model: ${model} (${contextWindow} tokens)
  Token Budget: ${tokenBudget} (${(targetUtilization * 100).toFixed(0)}% of available)
  Messages in Room: ${messagesInRoom}
  Messages Loaded: ${loadedMessages.length}
  Tokens Used: ${tokensUsed} (${budgetUtilization.toFixed(1)}% of budget)
  Total with Completion: ${totalWithCompletion} tokens
  Would Fit: ${wouldFitInContext ? '✅ YES' : '❌ NO'}`);

      return {
        ...ragParams,
        success: true,
        roomId,
        model,
        contextWindow,
        tokenBudget,
        messagesInRoom,
        messagesLoaded: loadedMessages.length,
        tokensUsed,
        budgetUtilization,
        messages: loadedMessages,
        wouldFitInContext,
        totalWithCompletion
      };
    } catch (error) {
      console.error('❌ RAG Load failed:', error);
      return {
        ...ragParams,
        success: false,
        roomId: (ragParams.roomId || ragParams.room || '') as any,
        model: ragParams.model,
        contextWindow: 0,
        tokenBudget: 0,
        messagesInRoom: 0,
        messagesLoaded: 0,
        tokensUsed: 0,
        budgetUtilization: 0,
        messages: [],
        wouldFitInContext: false,
        totalWithCompletion: 0,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Find room by ID or name
   */
  private async findRoom(roomIdOrName: string): Promise<{ id: string; }> {
    // Try by ID first
    const byIdResult = await DataDaemon.query({
      collection: 'rooms',
      filter: { id: roomIdOrName },
      limit: 1
    });

    if (byIdResult.success && byIdResult.data && byIdResult.data.length > 0) {
      const record = byIdResult.data[0];
      return { id: record.id };
    }

    // Try by name
    const byNameResult = await DataDaemon.query({
      collection: 'rooms',
      filter: { name: roomIdOrName },
      limit: 1
    });

    if (byNameResult.success && byNameResult.data && byNameResult.data.length > 0) {
      const record = byNameResult.data[0];
      return { id: record.id };
    }

    throw new Error(`Room not found: ${roomIdOrName}`);
  }

  /**
   * Estimate tokens for text
   * Simple approximation: ~4 characters per token
   * TODO: Use tiktoken library for accurate counting
   */
  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
