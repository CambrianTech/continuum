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
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { UserEntity } from '../../../../system/data/entities/UserEntity';
import type { TextGenerationRequest } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { SystemPaths } from '../../../../system/core/config/SystemPaths';
import { LOCAL_MODELS } from '../../../../system/shared/Constants';

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

        // Find persona if not specified
        let targetPersonaId = params.personaId;
        let personaDisplayName = 'ai-generate-command'; // Fallback name for tracking
        if (!targetPersonaId) {
          const usersResult = await ORM.query<UserEntity>({
            collection: UserEntity.collection,
            filter: { type: 'persona' },
            limit: 1
          });

          if (!usersResult.success || !usersResult.data || usersResult.data.length === 0) {
            return createErrorResult(params, 'No personas found in system');
          }

          const personaRecord = usersResult.data[0];
          targetPersonaId = personaRecord.id;
          personaDisplayName = personaRecord.data.displayName;
        }

        // Build RAG context (SAME code as PersonaUser.respondToMessage line 207-215)
        const ragBuilder = RAGBuilderFactory.getBuilder('chat');
        ragContext = await ragBuilder.buildContext(
          params.roomId,
          targetPersonaId,
          {
            modelId: params.model,
            provider: params.provider,
            maxMessages: params.maxMessages || 20,
            includeArtifacts: params.includeArtifacts ?? true,
            includeMemories: params.includeMemories ?? true,
            triggeringTimestamp: Date.now(),
            maxTokens: params.maxTokens ?? 2000,
          }
        );

        // Convert to messages array with timestamps + gaps (SAME as PersonaUser.ts:376-415)
        const messages: TextGenerationRequest['messages'] = [];
        messages.push({
          role: 'system',
          content: ragContext.identity.systemPrompt
        });

        // Add conversation history with timestamp formatting + gap detection
        let lastTimestamp: number | undefined;
        for (const msg of ragContext.conversationHistory) {
          let timePrefix = '';
          if (msg.timestamp) {
            const date = new Date(msg.timestamp);
            const hours = date.getHours().toString().padStart(2, '0');
            const minutes = date.getMinutes().toString().padStart(2, '0');
            timePrefix = `[${hours}:${minutes}] `;

            // Detect significant time gaps (> 1 hour)
            if (lastTimestamp && (msg.timestamp - lastTimestamp > 3600000)) {
              const gapHours = Math.floor((msg.timestamp - lastTimestamp) / 3600000);
              messages.push({
                role: 'system',
                content: `⏱️ ${gapHours} hour${gapHours > 1 ? 's' : ''} passed - conversation resumed`
              });
            }
            lastTimestamp = msg.timestamp;
          }

          messages.push({
            role: msg.role,
            content: msg.name ? `${timePrefix}${msg.name}: ${msg.content}` : `${timePrefix}${msg.content}`
          });
        }

        // Identity reminder with current time
        const now = new Date();
        const currentTime = `${now.toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })} ${now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}`;
        messages.push({
          role: 'system',
          content: `IDENTITY REMINDER: You are ${ragContext.identity.name}. Respond naturally with JUST your message - NO name prefix.\n\nCURRENT TIME: ${currentTime}\n\nIMPORTANT: Pay attention to timestamps [HH:MM]. If messages are from hours ago but current question is recent, topic likely changed. Focus on MOST RECENT message.`
        });

        // Build request with personaContext for proper logging and routing
        request = {
          messages,
          model: params.model || LOCAL_MODELS.DEFAULT,
          temperature: params.temperature ?? 0.7,
          maxTokens: params.maxTokens ?? 150,
          provider: params.provider || 'candle',
          personaContext: {
            uniqueId: targetPersonaId,
            displayName: ragContext.identity?.name || personaDisplayName,
            logDir: SystemPaths.personas.dir(targetPersonaId)
          }
        };

      } else if (params.messages) {
        // DIRECT MODE: Use provided messages
        request = paramsToRequest(params);

      } else {
        return createErrorResult(params, 'Either roomId or messages must be provided');
      }

      // PREVIEW MODE: Return request without calling LLM
      if (params.preview) {
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
      const response = await AIProviderDaemon.generateText(request);
      return responseToResult(response, params);
    } catch (error) {
      return createErrorResult(params, error instanceof Error ? error.message : String(error));
    }
  }
}
