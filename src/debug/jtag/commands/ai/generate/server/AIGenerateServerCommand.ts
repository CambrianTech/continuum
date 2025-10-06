/**
 * AI Generate Command - Server Implementation
 * ============================================
 *
 * Server-side AI generation with RAG context building
 * All database access and LLM calls happen here
 */

import { AIGenerateCommand } from '../shared/AIGenerateCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIGenerateParams, AIGenerateResult } from '../shared/AIGenerateTypes';
import { paramsToRequest, responseToResult, createErrorResult, createAIGenerateResultFromParams } from '../shared/AIGenerateTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import { RAGBuilderFactory } from '../../../../system/rag/shared/RAGBuilder';
import { ChatRAGBuilder } from '../../../../system/rag/builders/ChatRAGBuilder';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { UserEntity } from '../../../../system/data/entities/UserEntity';
import type { TextGenerationRequest } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypes';

export class AIGenerateServerCommand extends AIGenerateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);

    // Register ChatRAGBuilder if not already registered
    if (!RAGBuilderFactory.hasBuilder('chat')) {
      RAGBuilderFactory.register('chat', new ChatRAGBuilder());
    }
  }

  async execute(params: AIGenerateParams): Promise<AIGenerateResult> {
    try {
      let request: TextGenerationRequest;
      let ragContext: any = undefined;

      // Mode selection: RAG context building OR direct messages
      if (params.roomId) {
        // RAG MODE: Build context from chat room (SAME code path as PersonaUser)
        console.log(`🤖 AI Generate: Building RAG context for room ${params.roomId.slice(0, 8)}...`);

        // Find persona if not specified
        let targetPersonaId = params.personaId;
        if (!targetPersonaId) {
          const usersResult = await DataDaemon.query<UserEntity>({
            collection: UserEntity.collection,
            filters: { type: 'persona' },
            limit: 1
          });

          if (!usersResult.success || !usersResult.data || usersResult.data.length === 0) {
            return createErrorResult(params, 'No personas found in system');
          }

          const personaRecord = usersResult.data[0];
          targetPersonaId = personaRecord.id;
          console.log(`✅ AI Generate: Using persona "${personaRecord.data.displayName}" (${targetPersonaId.slice(0, 8)})`);
        }

        // Build RAG context (SAME code as PersonaUser.respondToMessage line 207-215)
        const ragBuilder = RAGBuilderFactory.getBuilder('chat');
        ragContext = await ragBuilder.buildContext(
          params.roomId,
          targetPersonaId,
          {
            maxMessages: params.maxMessages || 20,
            includeArtifacts: params.includeArtifacts ?? true,
            includeMemories: params.includeMemories ?? true,
            triggeringTimestamp: Date.now()  // Preview shows current state (no race filtering for manual preview)
          }
        );

        // Convert to messages array (SAME as PersonaUser line 207-216)
        const messages: TextGenerationRequest['messages'] = [];
        messages.push({
          role: 'system',
          content: ragContext.identity.systemPrompt
        });
        messages.push(...ragContext.conversationHistory);

        // Build request
        request = {
          messages,
          model: params.model || 'llama3.2:1b',
          temperature: params.temperature ?? 0.7,
          maxTokens: params.maxTokens ?? 150,
          preferredProvider: params.preferredProvider || 'ollama'
        };

      } else if (params.messages) {
        // DIRECT MODE: Use provided messages
        console.log(`🤖 AI Generate: Using provided messages (${params.messages.length})...`);
        request = paramsToRequest(params);

      } else {
        return createErrorResult(params, 'Either roomId or messages must be provided');
      }

      // PREVIEW MODE: Return request without calling LLM
      if (params.preview) {
        console.log(`👁️  AI Generate: Preview mode - returning request without LLM call`);
        const formatted = this.formatRequestPreview(request, ragContext);

        return createAIGenerateResultFromParams(params, {
          success: true,
          preview: true,
          request,
          formatted,
          ragContext
        });
      }

      // GENERATION MODE: Call AIProviderDaemon
      console.log(`🤖 AI Generate: Calling LLM with ${request.messages.length} messages...`);
      const response = await AIProviderDaemon.generateText(request);

      const result = responseToResult(response, params);
      console.log(`✅ AI Generate: Generated ${result.usage?.outputTokens} tokens in ${result.responseTime}ms`);

      return result;
    } catch (error) {
      console.error(`❌ AI Generate: Execution failed:`, error);
      return createErrorResult(params, error instanceof Error ? error.message : String(error));
    }
  }
}
