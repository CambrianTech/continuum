/**
 * AI Validate-Response Command Types
 *
 * After generating a response, AI evaluates if it actually answers the question.
 * Returns: SUBMIT (relevant) | CLARIFY (ask for context) | SILENT (off-topic)
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * AI's decision on what to do with generated response
 */
export type ResponseDecision = 'SUBMIT' | 'CLARIFY' | 'SILENT';

/**
 * Request for AI to validate if response answers question
 */
export interface AIValidateResponseParams extends CommandParams {
  /** The response that was generated */
  readonly generatedResponse: string;

  /** The original question/message */
  readonly originalQuestion: string;

  /** Who asked the question */
  readonly questionSender: string;

  /** Optional: Conversation context for better evaluation */
  readonly conversationContext?: string;

  /** Optional: Override model (defaults to llama3.2:3b) */
  readonly model?: string;

  /** Verbose mode - include prompt and AI reasoning */
  readonly verbose?: boolean;
}

/**
 * AI's validation decision
 */
export interface AIValidateResponseResult extends CommandResult {
  /** What should AI do with this response? */
  readonly decision: ResponseDecision;

  /** Confidence in this decision (0.0 - 1.0) */
  readonly confidence: number;

  /** Explanation of decision */
  readonly reason: string;

  /** Error message if validation failed */
  readonly error?: string;

  /** Verbose mode output - shows what AI saw */
  readonly debug?: {
    readonly promptSent: string;  // The actual prompt sent to LLM
    readonly aiResponse: string;  // Raw AI response
  };
}

/**
 * AIValidateResponse — Type-safe command executor
 *
 * Usage:
 *   import { AIValidateResponse } from '...shared/AIValidateResponseTypes';
 *   const result = await AIValidateResponse.execute({ ... });
 */
export const AIValidateResponse = {
  execute(params: CommandInput<AIValidateResponseParams>): Promise<AIValidateResponseResult> {
    return Commands.execute<AIValidateResponseParams, AIValidateResponseResult>('ai/validate-response', params as Partial<AIValidateResponseParams>);
  },
  commandName: 'ai/validate-response' as const,
} as const;

/**
 * Factory function for creating AiValidateResponseParams
 */
export const createAIValidateResponseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIValidateResponseParams, 'context' | 'sessionId' | 'userId'>
): AIValidateResponseParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AiValidateResponseResult with defaults
 */
export const createAIValidateResponseResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIValidateResponseResult, 'context' | 'sessionId' | 'userId'>
): AIValidateResponseResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart ai/validate-response-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAIValidateResponseResultFromParams = (
  params: AIValidateResponseParams,
  differences: Omit<AIValidateResponseResult, 'context' | 'sessionId' | 'userId'>
): AIValidateResponseResult => transformPayload(params, differences);

