/**
 * RAG Inspect Server Command
 *
 * Server-side RAG context inspection using ChatRAGBuilder
 */

import { RAGInspectCommand } from '../shared/RAGInspectCommand';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { RAGInspectParams, RAGInspectResult } from '../shared/RAGInspectTypes';
import { ChatRAGBuilder } from '../../../../../system/rag/builders/ChatRAGBuilder';
import { DataDaemon } from '../../../../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../../../../system/data/entities/ChatMessageEntity';

export class RAGInspectServerCommand extends RAGInspectCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/inspect', context, subpath, commander);
  }

  async execute(params: RAGInspectParams): Promise<RAGInspectResult> {
    try {
      console.log(`📚 RAG Inspect: Building context for persona ${params.personaId.slice(0, 8)} in context ${params.contextId.slice(0, 8)}`);

      // Build RAG context using ChatRAGBuilder
      const ragBuilder = new ChatRAGBuilder();
      const context = await ragBuilder.buildContext(
        params.contextId,
        params.personaId,
        {
          maxMessages: params.maxMessages ?? 20,
          includeArtifacts: params.includeArtifacts ?? true,
          includeMemories: params.includeMemories ?? true
        }
      );

      // Calculate summary stats
      const conversationTimespan = context.conversationHistory.length > 0
        ? {
            oldest: new Date(context.conversationHistory[0].timestamp ?? 0).toISOString(),
            newest: new Date(context.conversationHistory[context.conversationHistory.length - 1].timestamp ?? 0).toISOString()
          }
        : undefined;

      // Estimate tokens (rough: ~4 chars per token)
      const systemPromptTokens = Math.ceil(context.identity.systemPrompt.length / 4);
      const conversationTokens = context.conversationHistory.reduce((sum, msg) => {
        return sum + Math.ceil(msg.content.length / 4);
      }, 0);
      const totalTokensEstimate = systemPromptTokens + conversationTokens;

      // Validation warnings
      const warnings: string[] = [];
      if (context.conversationHistory.length === 0) {
        warnings.push('No conversation history found');
      }
      if (totalTokensEstimate > 8000) {
        warnings.push(`High token count (${totalTokensEstimate}) - may exceed model context window`);
      }
      if (!context.identity.systemPrompt.includes(context.identity.name)) {
        warnings.push('System prompt does not mention persona name');
      }

      console.log(`✅ RAG Inspect: Built context with ${context.conversationHistory.length} messages, ~${totalTokensEstimate} tokens`);

      // Determine learning mode status
      const learningModeStatus = context.learningMode
        ? 'enabled'
        : (context.participantRole !== undefined ? 'disabled' : 'not-configured');

      // Phase 2: Include learning mode fields if present
      if (context.learningMode) {
        console.log(`🧠 Learning Mode: ${context.learningMode}${context.participantRole ? ` (${context.participantRole})` : ''}${context.genomeId ? ` genome=${context.genomeId.slice(0, 8)}` : ''}`);
      }

      // Decision-point analysis (items 4-6)
      let decisionPoint: RAGInspectResult['decisionPoint'];
      if (params.triggerMessageId) {
        try {
          // Load the trigger message
          const msgResult = await DataDaemon.read<ChatMessageEntity>(ChatMessageEntity.collection, params.triggerMessageId);
          if (msgResult.success && msgResult.data) {
            const msg = msgResult.data.data;
            decisionPoint = {
              triggerMessage: {
                id: msg.id!,
                content: msg.content?.text || '',
                senderName: msg.senderName,
                timestamp: typeof msg.timestamp === 'number' ? msg.timestamp : new Date(msg.timestamp!).getTime()
              },
              decision: {
                shouldRespond: false, // TODO: Look up actual decision from thoughtstream
                action: 'SILENT',
                reasoning: 'Decision data not yet integrated'
              },
              learningContext: {
                mode: context.learningMode || 'not-configured',
                genomeActive: !!context.genomeId,
                participantRole: context.participantRole,
                adaptiveDataAvailable: context.privateMemories.length > 0
              }
            };
            console.log(`🎯 Decision Point: Message "${msg.content?.text?.slice(0, 50)}..." from ${msg.senderName}`);
          }
        } catch (error) {
          console.warn(`⚠️ Could not load trigger message ${params.triggerMessageId.slice(0, 8)}:`, error);
        }
      }

      return {
        ...params,
        success: true,
        ragContext: params.verbose ? context : undefined,  // Only include full context if --verbose
        summary: {
          messageCount: context.metadata.messageCount,
          artifactCount: context.metadata.artifactCount,
          memoryCount: context.metadata.memoryCount,
          conversationTimespan,
          systemPromptLength: context.identity.systemPrompt.length,
          totalTokensEstimate,

          // Phase 2: Surface learning mode fields in summary (always visible)
          learningMode: context.learningMode,
          genomeId: context.genomeId,
          participantRole: context.participantRole,
          learningModeStatus
        },
        decisionPoint,
        warnings: warnings.length > 0 ? warnings : undefined
      };
    } catch (error) {
      console.error('❌ RAG Inspect failed:', error);
      return {
        ...params,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
}
