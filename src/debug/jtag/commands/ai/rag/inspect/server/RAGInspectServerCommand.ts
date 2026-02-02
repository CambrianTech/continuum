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
import { getThoughtStreamCoordinator } from '../../../../../system/conversation/server/ThoughtStreamCoordinator';
import type { Thought } from '../../../../../system/conversation/shared/ConversationCoordinationTypes';

export class RAGInspectServerCommand extends RAGInspectCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/inspect', context, subpath, commander);
  }

  async execute(params: RAGInspectParams): Promise<RAGInspectResult> {
    try {
      console.log(`📚 RAG Inspect: Building context for persona ${params.personaId?.slice(0, 8) ?? 'unknown'} in context ${params.contextId?.slice(0, 8) ?? 'default'}`);

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

      // Build context preview (always shown in compact mode)
      const contextPreview = {
        // Recipe rules (top 3) - if available
        recipeRules: (context.recipeStrategy?.responseRules ?? []).slice(0, 3).length > 0
          ? (context.recipeStrategy?.responseRules ?? []).slice(0, 3)
          : ['No recipe rules configured'],

        // System prompt excerpt (first 200 chars)
        systemPromptExcerpt: context.identity.systemPrompt.length > 200
          ? context.identity.systemPrompt.slice(0, 200) + '...'
          : context.identity.systemPrompt,

        // Recent messages (last 3)
        recentMessages: context.conversationHistory.slice(-3).map(msg => ({
          sender: msg.name ?? msg.role,
          contentSnippet: msg.content.length > 50
            ? msg.content.slice(0, 50) + '...'
            : msg.content,
          timestamp: msg.timestamp
        }))
      };

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
          const msg = await DataDaemon.read<ChatMessageEntity>(ChatMessageEntity.collection, params.triggerMessageId);
          if (msg) {

            // Get actual decision from ThoughtStream
            const coordinator = getThoughtStreamCoordinator();
            const stream = coordinator.getStream(params.triggerMessageId);

            let actualDecision: {
              shouldRespond: boolean;
              confidence?: number;
              reasoning?: string;
              action: 'POSTED' | 'SILENT' | 'ERROR' | 'TIMEOUT';
            };
            if (stream) {
              // Find this persona's thought in the stream
              const personaThought = stream.thoughts.find((t: Thought) => t.personaId === params.personaId);

              if (personaThought) {
                // Check if persona was granted permission
                const wasGranted = stream.decision?.granted.includes(params.personaId) || false;

                // Determine action based on thought state and coordination outcome
                let action: 'POSTED' | 'SILENT' | 'ERROR' | 'TIMEOUT';

                // Check for error/timeout conditions first
                if (personaThought.type === 'deferring' && personaThought.reasoning?.toLowerCase().includes('error')) {
                  action = 'ERROR';
                } else if (personaThought.type === 'deferring' && personaThought.reasoning?.toLowerCase().includes('timeout')) {
                  action = 'TIMEOUT';
                } else if (stream.phase === 'decided' && wasGranted) {
                  action = 'POSTED';
                } else {
                  action = 'SILENT';
                }

                actualDecision = {
                  shouldRespond: personaThought.type === 'claiming',
                  confidence: personaThought.confidence,
                  reasoning: personaThought.reasoning,
                  action
                };
                console.log(`✅ Found real decision: ${actualDecision.action} (confidence: ${actualDecision.confidence})${personaThought.reasoning ? ` - ${personaThought.reasoning.slice(0, 50)}` : ''}`);
              } else {
                // Persona didn't evaluate this message
                actualDecision = {
                  shouldRespond: false,
                  action: 'SILENT',
                  reasoning: 'Persona did not evaluate this message'
                };
              }
            } else {
              // ThoughtStream not found (old message or not yet processed)
              actualDecision = {
                shouldRespond: false,
                action: 'SILENT',
                reasoning: 'ThoughtStream not available (message may be too old or not yet processed)'
              };
            }

            decisionPoint = {
              triggerMessage: {
                id: msg.id!,
                content: msg.content?.text || '',
                senderName: msg.senderName,
                timestamp: typeof msg.timestamp === 'number' ? msg.timestamp : new Date(msg.timestamp!).getTime()
              },
              decision: actualDecision,
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
        contextPreview,  // Always show preview (compact mode visibility)
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
