/**
 * AI Validate-Response Command Types
 *
 * After generating a response, AI evaluates if it actually answers the question.
 * Returns: SUBMIT (relevant) | CLARIFY (ask for context) | SILENT (off-topic)
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

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
