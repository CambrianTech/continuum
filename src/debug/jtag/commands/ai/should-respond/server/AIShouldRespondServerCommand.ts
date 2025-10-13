/**
 * AI Should-Respond Server Command
 *
 * Uses AIProviderDaemon with proper RAG context (message array, not flattened string)
 */

import { AIShouldRespondCommand } from '../shared/AIShouldRespondCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIShouldRespondParams, AIShouldRespondResult } from '../shared/AIShouldRespondTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypes';

export class AIShouldRespondServerCommand extends AIShouldRespondCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/should-respond', context, subpath, commander);
  }

  async execute(params: AIShouldRespondParams): Promise<AIShouldRespondResult> {
    try {
      // Validate ragContext for LLM strategy
      if (!params.ragContext) {
        throw new Error('ragContext is required for LLM strategy');
      }

      // Build gating instruction
      const gatingInstruction = this.buildGatingInstruction(params);

      // Mark the trigger message in conversation history with >>> arrows <<<
      const markedHistory = params.ragContext.conversationHistory.map(msg => {
        const isTrigger = msg.content === params.triggerMessage.content &&
                         msg.name === params.triggerMessage.senderName;

        if (isTrigger) {
          return {
            ...msg,
            content: `>>> ${msg.content} <<<`
          };
        }
        return msg;
      });

      // Build proper messages array: system + conversation history (with marked trigger) + gating instruction
      const request: TextGenerationRequest = {
        messages: [
          { role: 'system', content: 'You are a conversation coordinator. Respond ONLY with JSON.' },
          ...markedHistory,  // Conversation with trigger message marked
          { role: 'user', content: gatingInstruction }
        ],
        model: params.model ?? 'llama3.2:3b',  // Instruction-tuned model
        temperature: 0.3,
        maxTokens: 200,
        preferredProvider: 'ollama'
      };

      const response = await AIProviderDaemon.generateText(request);

      if (!response.text) {
        throw new Error(response.error ?? 'AI generation failed');
      }

      const parsed = this.parseGatingResponse(response.text);

      const confidence = parsed.confidence ?? 0.5;

      // Build debug output if verbose mode enabled
      let debugOutput: AIShouldRespondResult['debug'] = undefined;
      if (params.verbose) {
        const conversationText = params.ragContext.conversationHistory
          .map(msg => `${msg.role}: ${msg.content}`)
          .join('\n');

        debugOutput = {
          ragContext: {
            messageCount: params.ragContext.conversationHistory.length,
            conversationPreview: conversationText.substring(0, 500) + (conversationText.length > 500 ? '...' : '')
          },
          promptSent: gatingInstruction,
          aiResponse: response.text
        };
      }

      return {
        context: params.context,
        sessionId: params.sessionId,
        shouldRespond: parsed.shouldRespond ?? false,
        confidence,
        reason: parsed.reason ?? 'No reason provided',
        factors: parsed.factors ?? {
          mentioned: false,
          questionAsked: false,
          domainRelevant: false,
          recentlySpoke: false,
          othersAnswered: false
        },
        debug: debugOutput
      };
    } catch (error) {
      console.error('❌ AI Should-Respond: Command failed:', error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        error: error instanceof Error ? error.message : String(error),
        shouldRespond: false,
        confidence: 0.0,
        reason: 'Command execution failed',
        factors: {
          mentioned: false,
          questionAsked: false,
          domainRelevant: false,
          recentlySpoke: false,
          othersAnswered: false
        }
      };
    }
  }
}
