import { Commands } from '../../../../system/core/shared/Commands';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';

/**
 * RAG Budget Command - Calculate token budget for RAG context
 *
 * Shows the algorithm for calculating safe message counts based on model's context window.
 * Makes the token budget calculation transparent and debuggable.
 */

export interface RAGBudgetParams extends CommandParams {
  model: string;              // Model name (e.g., "gpt-4", "claude-3-sonnet")
  provider?: string;          // Provider for scoped registry lookup (e.g., "candle", "together")
  maxTokens?: number;         // Max tokens for completion (default: 3000)
  systemPromptTokens?: number; // Estimated system prompt size (default: 500)
  targetUtilization?: number;  // Target % of available tokens (default: 0.8 = 80%)
  avgTokensPerMessage?: number; // Estimated tokens per message (default: 250)
}

export interface RAGBudgetResult extends CommandResult {
  success: boolean;
  model: string;

  // Context window info
  contextWindow: number;      // Total context window in tokens
  maxTokens: number;          // Completion budget
  systemPromptTokens: number; // System prompt budget

  // Available tokens calculation
  availableForMessages: number; // contextWindow - maxTokens - systemPrompt
  targetUtilization: number;    // Percentage to use (0.8 = 80%)
  targetTokens: number;         // availableForMessages * targetUtilization

  // Message count calculation
  avgTokensPerMessage: number;  // Estimated tokens per message
  safeMessageCount: number;     // Calculated safe count
  clampedMessageCount: number;  // After clamping to [5, 50]

  // Verification
  estimatedInputTokens: number; // systemPrompt + (messages * avgTokensPerMessage)
  estimatedTotalTokens: number; // estimatedInputTokens + maxTokens
  wouldExceedLimit: boolean;    // Would this exceed context window?
  utilizationPercent: number;   // Actual % of context window used

  error?: string;
}

/**
 * RAGBudget — Type-safe command executor
 *
 * Usage:
 *   import { RAGBudget } from '...shared/RAGBudgetTypes';
 *   const result = await RAGBudget.execute({ ... });
 */
export const RAGBudget = {
  execute(params: CommandInput<RAGBudgetParams>): Promise<RAGBudgetResult> {
    return Commands.execute<RAGBudgetParams, RAGBudgetResult>('rag/budget', params as Partial<RAGBudgetParams>);
  },
  commandName: 'rag/budget' as const,
} as const;
