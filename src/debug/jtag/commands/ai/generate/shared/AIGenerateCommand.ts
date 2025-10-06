/**
 * AI Generate Command - Shared Implementation
 * ============================================
 *
 * Shared command logic for text generation via AIProviderDaemon
 * Works in both browser and server environments
 * Follows data command pattern for consistency
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIGenerateParams, AIGenerateResult } from './AIGenerateTypes';
import { paramsToRequest, responseToResult, createErrorResult } from './AIGenerateTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';

export abstract class AIGenerateCommand extends CommandBase<AIGenerateParams, AIGenerateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai-generate', context, subpath, commander);
  }

  async execute(params: AIGenerateParams): Promise<AIGenerateResult> {
    console.log(`🤖 AI Generate: Generating text with ${params.preferredProvider || 'default provider'}...`);

    try {
      // Convert command params to daemon request
      const request = paramsToRequest(params);

      // Call AIProviderDaemon directly via static method (like DataDaemon.query)
      const response = await AIProviderDaemon.generateText(request);

      // Convert daemon response to command result
      const result = responseToResult(response, params);
      console.log(`✅ AI Generate: Generated ${result.usage.outputTokens} tokens in ${result.responseTime}ms`);

      return result;
    } catch (error) {
      console.error(`❌ AI Generate: Execution failed:`, error);
      return createErrorResult(params, error instanceof Error ? error.message : String(error));
    }
  }
}
