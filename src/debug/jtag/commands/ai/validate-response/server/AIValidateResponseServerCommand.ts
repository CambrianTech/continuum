/**
 * AI Validate-Response Server Command
 *
 * After generating response, AI validates if it actually answers the question.
 * Uses AIProviderDaemon for LLM-based evaluation.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIValidateResponseParams, AIValidateResponseResult, ResponseDecision } from '../shared/AIValidateResponseTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

export class AIValidateResponseServerCommand extends CommandBase<AIValidateResponseParams, AIValidateResponseResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/validate-response', context, subpath, commander);
  }

  async execute(params: AIValidateResponseParams): Promise<AIValidateResponseResult> {
    // Build validation prompt
    const validationPrompt = this.buildValidationPrompt(params);

    // Simple LLM call for validation
    const request: TextGenerationRequest = {
      messages: [
        { role: 'system', content: 'You are a response validator. Reply ONLY with one word: SUBMIT, CLARIFY, or SILENT.' },
        { role: 'user', content: validationPrompt }
      ],
      model: params.model ?? 'llama3.2:3b',
      temperature: 0.1,  // Low temp for consistent decisions
      maxTokens: 10,     // Just need one word
      preferredProvider: 'candle'
    };

    const response = await AIProviderDaemon.generateText(request);

    if (!response.text) {
      throw new Error(response.error ?? 'AI validation failed');
    }

    // Parse decision
    const decision = this.parseDecision(response.text);
    const reason = this.getReasonForDecision(decision, params);

    return {
      context: params.context,
      sessionId: params.sessionId,
      decision,
      confidence: 0.9,  // High confidence for simple yes/no decisions
      reason,
      debug: params.verbose ? {
        promptSent: validationPrompt,
        aiResponse: response.text
      } : undefined
    };
  }

  private buildValidationPrompt(params: AIValidateResponseParams): string {
    return `You generated this response:
"${params.generatedResponse}"

Original question from ${params.questionSender}:
"${params.originalQuestion}"

Does your response actually answer their question?

Reply with ONLY ONE WORD:
- SUBMIT (your response clearly answers the question)
- CLARIFY (you're unsure, should ask for clarification)
- SILENT (your response is off-topic, stay silent)`;
  }

  private parseDecision(aiResponse: string): ResponseDecision {
    const text = aiResponse.trim().toUpperCase();

    if (text.includes('CLARIFY')) {
      return 'CLARIFY';
    } else if (text.includes('SILENT')) {
      return 'SILENT';
    }

    return 'SUBMIT';  // Default to submitting
  }

  private getReasonForDecision(decision: ResponseDecision, _params: AIValidateResponseParams): string {
    switch (decision) {
      case 'SUBMIT':
        return 'Response appears relevant to the question';
      case 'CLARIFY':
        return 'Uncertain if response answers question, should ask for clarification';
      case 'SILENT':
        return 'Response is off-topic or does not address the question';
      default:
        return 'Unknown decision';
    }
  }
}
