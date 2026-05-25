/**
 * AI Generate Command - Server Implementation (thin shim)
 * =======================================================
 *
 * Rust owns response generation: prompt assembly (system prompt +
 * history + time prefixes + hour-gap markers + identity reminder),
 * provider selection, admission gating, timeout, and token-usage
 * stamping all live in `cognition/generate_response.rs`. This shim:
 *
 *   1. Builds the RAG context server-side (still TS — the
 *      `ChatRAGBuilder` factory + entity reads have not been ported
 *      to Rust yet; tracked separately).
 *   2. Adapts the RAG context onto `AIDecisionContext` and hands off
 *      to `AIDecisionService.generateResponse`, which is the proven
 *      IPC seam already used by PersonaUser's response path.
 *   3. Translates the Rust result back to `AIGenerateResult`.
 *
 * Direct-message and preview modes remain TS-side because they are
 * introspection/test paths that bypass admission and provider
 * selection — Rust intentionally does not expose a "skip the gate"
 * code path.
 */
import { AIGenerateCommand } from '../shared/AIGenerateCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIGenerateParams, AIGenerateResult } from '../shared/AIGenerateTypes';
import { paramsToRequest, responseToResult, createErrorResult, createAIGenerateResultFromParams } from '../shared/AIGenerateTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import { RAGBuilderFactory } from '../../../../system/rag/shared/RAGBuilder';
import { getContextWindow, getInferenceSpeed } from '../../../../system/shared/ModelContextWindows';
import { ChatRAGBuilder } from '../../../../system/rag/builders/ChatRAGBuilder';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { UserEntity } from '../../../../system/data/entities/UserEntity';
import { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import type { TextGenerationRequest } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { AIDecisionService, type AIDecisionContext } from '../../../../system/ai/server/AIDecisionService';

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
      // RAG MODE: build context, delegate to Rust generate-response
      if (params.roomId) {
        // Find persona if not specified
        let targetPersonaId = params.personaId;
        let personaDisplayName = 'ai-generate-command';
        if (!targetPersonaId) {
          const usersResult = await ORM.query<UserEntity>({
            collection: UserEntity.collection,
            filter: { type: 'persona' },
            limit: 1
          }, 'default');

          if (!usersResult.success || !usersResult.data || usersResult.data.length === 0) {
            return createErrorResult(params, 'No personas found in system');
          }

          const personaRecord = usersResult.data[0];
          targetPersonaId = personaRecord.id;
          personaDisplayName = personaRecord.data.displayName;
        }

        const ragBuilder = RAGBuilderFactory.getBuilder('chat');
        const ragContext = await ragBuilder.buildContext(
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
            contextWindow: getContextWindow(params.model, params.provider),
            tokensPerSecond: getInferenceSpeed(params.model, params.provider),
          }
        );

        // PREVIEW MODE: reconstruct the request Rust would build (best-effort
        // mirror; the source of truth is `build_response_generation_request`
        // in cognition/generate_response.rs). Returns without inference.
        if (params.preview) {
          const previewRequest = this.previewRequestFromRag(params, ragContext, targetPersonaId, personaDisplayName);
          const formatted = this.formatRequestPreview(previewRequest, ragContext);
          return createAIGenerateResultFromParams(params, {
            success: true,
            preview: true,
            request: previewRequest,
            formatted,
            ragContext: ragContext as unknown as Record<string, unknown>
          });
        }

        // Adapt onto AIDecisionContext for the Rust shim.
        // triggerMessage is the latest history entry — Rust uses it for
        // the admission lease/artifact key, not for prompt content.
        const history = ragContext.conversationHistory;
        const triggerMessage = this.synthesizeTriggerMessage(history, params.roomId);
        const decisionContext: AIDecisionContext = {
          personaId: targetPersonaId,
          personaName: ragContext.identity?.name || personaDisplayName,
          roomId: params.roomId,
          triggerMessage,
          ragContext,
          systemPrompt: ragContext.identity.systemPrompt,
        };

        const generation = await AIDecisionService.generateResponse(decisionContext, {
          model: params.model,
          temperature: params.temperature,
          maxTokens: params.maxTokens,
        });

        return createAIGenerateResultFromParams(params, {
          success: true,
          text: generation.text,
          model: generation.model,
          provider: params.provider || 'local',
          responseTimeMs: generation.responseTime,
          requestId: undefined,
          usage: generation.tokensUsed
            ? {
                inputTokens: generation.tokensUsed.input,
                outputTokens: generation.tokensUsed.output,
                totalTokens: generation.tokensUsed.total,
              }
            : undefined,
        });
      }

      // DIRECT MODE: pass-through to AIProviderDaemon. No admission gate
      // here — direct mode is a test/introspection path; production
      // traffic comes through RAG mode above.
      if (params.messages) {
        const request: TextGenerationRequest = paramsToRequest(params);

        if (params.preview) {
          const formatted = this.formatRequestPreview(request, undefined);
          return createAIGenerateResultFromParams(params, {
            success: true,
            preview: true,
            request,
            formatted,
            ragContext: undefined
          });
        }

        const response = await AIProviderDaemon.generateText(request);
        return responseToResult(response, params);
      }

      return createErrorResult(params, 'Either roomId or messages must be provided');
    } catch (error) {
      return createErrorResult(params, error instanceof Error ? error.message : String(error));
    }
  }

  private previewRequestFromRag(
    params: AIGenerateParams,
    ragContext: import('../../../../system/rag/shared/RAGTypes').RAGContext,
    targetPersonaId: string,
    personaDisplayName: string
  ): TextGenerationRequest {
    // Mirror of what cognition/generate_response.rs assembles. Kept
    // local so --preview stays useful without IPC. If the Rust prompt
    // assembly changes, this drifts — wire a `cognition/preview-request`
    // IPC if drift becomes a problem.
    const messages: TextGenerationRequest['messages'] = [
      { role: 'system', content: ragContext.identity.systemPrompt }
    ];
    let lastTimestamp: number | undefined;
    for (const msg of ragContext.conversationHistory) {
      let timePrefix = '';
      if (msg.timestamp) {
        const date = new Date(msg.timestamp);
        const hours = date.getHours().toString().padStart(2, '0');
        const minutes = date.getMinutes().toString().padStart(2, '0');
        timePrefix = `[${hours}:${minutes}] `;
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
    const now = new Date();
    const currentTime = `${now.toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })} ${now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}`;
    messages.push({
      role: 'system',
      content: `IDENTITY REMINDER: You are ${ragContext.identity?.name || personaDisplayName}. Respond naturally with JUST your message - NO name prefix.\n\nCURRENT TIME: ${currentTime}\n\nIMPORTANT: Pay attention to timestamps [HH:MM]. If messages are from hours ago but current question is recent, topic likely changed. Focus on MOST RECENT message.`
    });
    return {
      messages,
      model: params.model,
      temperature: params.temperature ?? 0.7,
      maxTokens: params.maxTokens ?? 150,
      provider: params.provider || 'local',
      personaContext: {
        uniqueId: targetPersonaId,
        displayName: ragContext.identity?.name || personaDisplayName,
        logDir: ''
      }
    };
  }

  private synthesizeTriggerMessage(
    history: import('../../../../system/rag/shared/RAGTypes').RAGContext['conversationHistory'],
    roomId: string
  ): ChatMessageEntity {
    // Latest message is the trigger. Rust uses this for the admission
    // lease key (room+persona+messageId) — the prompt content comes
    // from ragContext.conversationHistory regardless.
    const last = history[history.length - 1];
    const msg = new ChatMessageEntity();
    msg.roomId = roomId as ChatMessageEntity['roomId'];
    msg.content = { text: last?.content ?? '', media: [] };
    msg.timestamp = new Date(last?.timestamp ?? Date.now());
    return msg;
  }
}
