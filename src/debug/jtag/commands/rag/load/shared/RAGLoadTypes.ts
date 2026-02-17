import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * RAG Load Command - Test incremental message loading with token counting
 *
 * Shows exactly which messages would be loaded for RAG context given a token budget.
 * Makes the incremental loading algorithm transparent and debuggable.
 */

export interface RAGLoadParams extends CommandParams {
  roomId?: UUID;               // Room UUID (if known)
  room?: string;               // Room unique ID (like "general") - will be resolved to UUID
  model: string;               // Model to calculate budget for
  provider?: string;           // Provider for scoped registry lookup (e.g., "candle", "together")
  maxTokens?: number;          // Max tokens for completion (default: 3000)
  systemPromptTokens?: number; // Estimated system prompt size (default: 500)
  targetUtilization?: number;  // Target % of available tokens (default: 0.8)
  showMessageContent?: boolean; // Include message text in results (default: false)
}

export interface LoadedMessage {
  messageId: string;
  shortId: string;
  timestamp: string;
  senderName: string;
  contentPreview: string;      // First 50 chars
  fullContent?: string;        // Only if showMessageContent=true
  estimatedTokens: number;
  cumulativeTokens: number;
}

export interface RAGLoadResult extends CommandResult {
  success: boolean;
  roomId: UUID;
  model: string;

  // Budget calculation
  contextWindow: number;
  tokenBudget: number;         // Available tokens for messages
  messagesInRoom: number;      // Total messages in room

  // Loading results
  messagesLoaded: number;      // How many messages were loaded
  tokensUsed: number;          // Total tokens in loaded messages
  budgetUtilization: number;   // % of budget used

  messages: LoadedMessage[];   // Loaded messages with token counts

  // Verification
  wouldFitInContext: boolean;  // Would this fit with completion?
  totalWithCompletion: number; // messages + system + completion

  error?: string;
}

/**
 * RAGLoad — Type-safe command executor
 *
 * Usage:
 *   import { RAGLoad } from '...shared/RAGLoadTypes';
 *   const result = await RAGLoad.execute({ ... });
 */
export const RAGLoad = {
  execute(params: CommandInput<RAGLoadParams>): Promise<RAGLoadResult> {
    return Commands.execute<RAGLoadParams, RAGLoadResult>('rag/load', params as Partial<RAGLoadParams>);
  },
  commandName: 'rag/load' as const,
} as const;
