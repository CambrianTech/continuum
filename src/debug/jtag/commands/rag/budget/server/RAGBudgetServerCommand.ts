import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { RAGBudgetParams, RAGBudgetResult } from '../shared/RAGBudgetTypes';
import { getContextWindow } from '../../../../system/shared/ModelContextWindows';

/**
 * RAG Budget Server Command - Calculate token budget for RAG context
 *
 * Transparent implementation of the token budget algorithm.
 * Shows exactly how safe message counts are calculated.
 */
export class RAGBudgetServerCommand extends CommandBase<RAGBudgetParams, RAGBudgetResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('rag/budget', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RAGBudgetResult> {
    const ragParams = params as RAGBudgetParams;

    try {
      const model = ragParams.model;
      const maxTokens = ragParams.maxTokens ?? 3000;
      const systemPromptTokens = ragParams.systemPromptTokens ?? 500;
      const targetUtilization = ragParams.targetUtilization ?? 0.8;
      const avgTokensPerMessage = ragParams.avgTokensPerMessage ?? 250;

      // Get context window from centralized configuration (provider-scoped)
      const provider = ragParams.provider;
      const contextWindow = getContextWindow(model, provider);

      // Calculate available tokens for messages
      const availableForMessages = contextWindow - maxTokens - systemPromptTokens;

      // Target 80% of available (20% safety margin)
      const targetTokens = availableForMessages * targetUtilization;

      // Calculate safe message count
      const safeMessageCount = Math.floor(targetTokens / avgTokensPerMessage);

      // Clamp between 5 and 50
      const clampedMessageCount = Math.max(5, Math.min(50, safeMessageCount));

      // Verification: Would this actually fit?
      const estimatedInputTokens = systemPromptTokens + (clampedMessageCount * avgTokensPerMessage);
      const estimatedTotalTokens = estimatedInputTokens + maxTokens;
      const wouldExceedLimit = estimatedTotalTokens > contextWindow;
      const utilizationPercent = (estimatedTotalTokens / contextWindow) * 100;

      console.log(`📊 RAG Budget Calculation:
  Model: ${model}
  Context Window: ${contextWindow} tokens
  Max Tokens (completion): ${maxTokens}
  System Prompt: ${systemPromptTokens}
  Available for Messages: ${availableForMessages}
  Target Utilization: ${(targetUtilization * 100).toFixed(0)}%
  Target Tokens: ${targetTokens.toFixed(0)}
  Avg Tokens/Message: ${avgTokensPerMessage}
  Safe Message Count: ${safeMessageCount} → ${clampedMessageCount} (clamped)

  Verification:
  Estimated Input: ${estimatedInputTokens} tokens
  Estimated Total: ${estimatedTotalTokens} tokens
  Would Exceed Limit: ${wouldExceedLimit ? '❌ YES' : '✅ NO'}
  Utilization: ${utilizationPercent.toFixed(1)}%`);

      return {
        ...ragParams,
        success: true,
        model,
        contextWindow,
        maxTokens,
        systemPromptTokens,
        availableForMessages,
        targetUtilization,
        targetTokens,
        avgTokensPerMessage,
        safeMessageCount,
        clampedMessageCount,
        estimatedInputTokens,
        estimatedTotalTokens,
        wouldExceedLimit,
        utilizationPercent
      };
    } catch (error) {
      console.error('❌ RAG Budget calculation failed:', error);
      return {
        ...ragParams,
        success: false,
        model: ragParams.model,
        contextWindow: 0,
        maxTokens: 0,
        systemPromptTokens: 0,
        availableForMessages: 0,
        targetUtilization: 0,
        targetTokens: 0,
        avgTokensPerMessage: 0,
        safeMessageCount: 0,
        clampedMessageCount: 0,
        estimatedInputTokens: 0,
        estimatedTotalTokens: 0,
        wouldExceedLimit: true,
        utilizationPercent: 0,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
}
