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

      return {
        ...params,
        success: true,
        ragContext: context,
        summary: {
          messageCount: context.metadata.messageCount,
          artifactCount: context.metadata.artifactCount,
          memoryCount: context.metadata.memoryCount,
          conversationTimespan,
          systemPromptLength: context.identity.systemPrompt.length,
          totalTokensEstimate
        },
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
