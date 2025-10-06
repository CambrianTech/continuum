/**
 * AI Should-Respond Server Command
 *
 * Uses AIProviderDaemon to call llama3.2:3b for gating decisions
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
      const prompt = this.buildGatingPrompt(params);

      const request: TextGenerationRequest = {
        messages: [
          { role: 'system', content: 'You are a conversation coordinator. Respond ONLY with JSON.' },
          { role: 'user', content: prompt }
        ],
        model: params.model || 'llama3.2:3b',
        temperature: 0.3,
        maxTokens: 200,
        preferredProvider: 'ollama'
      };

      console.log(`🤖 AI Should-Respond: Calling LLM for ${params.personaName}...`);
      const response = await AIProviderDaemon.generateText(request);

      if (!response.text) {
        throw new Error(response.error || 'AI generation failed');
      }

      const parsed = this.parseGatingResponse(response.text);

      const confidence = parsed.confidence ?? 0.5;
      console.log(`✅ AI Should-Respond: ${params.personaName} → ${parsed.shouldRespond ? 'RESPOND' : 'SILENT'} (${(confidence * 100).toFixed(0)}% confidence)`);

      return {
        context: params.context,
        sessionId: params.sessionId,
        shouldRespond: parsed.shouldRespond || false,
        confidence,
        reason: parsed.reason || 'No reason provided',
        factors: parsed.factors || {
          mentioned: false,
          questionAsked: false,
          domainRelevant: false,
          recentlySpoke: false,
          othersAnswered: false
        }
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
