/**
 * Inference Generate Command - Shared Types
 *
 * Generate text using local or cloud AI inference. Auto-routes to best available backend (Candle → cloud). Handles model loading, LoRA adapters, and provider failover automatically.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
// Note: Using simple error string like ai/generate for consistency
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Inference Generate Command Parameters
 */
export interface InferenceGenerateParams extends CommandParams {
  // The prompt text to generate from
  prompt: string;
  // Model to use (e.g., 'llama3.2:3b', 'Qwen/Qwen2-1.5B-Instruct'). Defaults to LOCAL_MODELS.DEFAULT
  model?: string;
  // Preferred provider: 'candle' | 'anthropic' | 'openai' | 'groq' | 'together' | 'fireworks'. Auto-routes if not specified
  provider?: string;
  // Maximum tokens to generate (default: 2048)
  maxTokens?: number;
  // Sampling temperature 0.0-2.0 (default: 0.7)
  temperature?: number;
  // System prompt to prepend
  systemPrompt?: string;
  // LoRA adapter names to apply (local inference only). Skips missing adapters gracefully
  adapters?: string[];
}

/**
 * Factory function for creating InferenceGenerateParams
 */
export const createInferenceGenerateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // The prompt text to generate from
    prompt: string;
    // Model to use (e.g., 'llama3.2:3b', 'Qwen/Qwen2-1.5B-Instruct'). Defaults to LOCAL_MODELS.DEFAULT
    model?: string;
    // Preferred provider: 'candle' | 'anthropic' | 'openai' | 'groq' | 'together' | 'fireworks'. Auto-routes if not specified
    provider?: string;
    // Maximum tokens to generate (default: 2048)
    maxTokens?: number;
    // Sampling temperature 0.0-2.0 (default: 0.7)
    temperature?: number;
    // System prompt to prepend
    systemPrompt?: string;
    // LoRA adapter names to apply (local inference only). Skips missing adapters gracefully
    adapters?: string[];
  }
): InferenceGenerateParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  model: data.model ?? '',
  provider: data.provider ?? '',
  maxTokens: data.maxTokens ?? 0,
  temperature: data.temperature ?? 0,
  systemPrompt: data.systemPrompt ?? '',
  adapters: data.adapters ?? undefined,
  ...data
});

/**
 * Inference Generate Command Result
 */
export interface InferenceGenerateResult extends CommandResult {
  success: boolean;
  // Generated text
  text: string;
  // Actual model used (may differ from requested if mapped)
  model: string;
  // Provider that handled the request
  provider: string;
  // Whether inference was local (Candle) or cloud
  isLocal: boolean;
  // LoRA adapters that were actually applied
  adaptersApplied: string[];
  // Number of input tokens processed
  inputTokens: number;
  // Number of tokens generated
  outputTokens: number;
  // Total response time in milliseconds
  responseTimeMs: number;
  // Error message if failed
  error?: string;
}

/**
 * Factory function for creating InferenceGenerateResult with defaults
 */
export const createInferenceGenerateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Generated text
    text?: string;
    // Actual model used (may differ from requested if mapped)
    model?: string;
    // Provider that handled the request
    provider?: string;
    // Whether inference was local (Candle) or cloud
    isLocal?: boolean;
    // LoRA adapters that were actually applied
    adaptersApplied?: string[];
    // Number of input tokens processed
    inputTokens?: number;
    // Number of tokens generated
    outputTokens?: number;
    // Total response time in milliseconds
    responseTimeMs?: number;
    // Error message if failed
    error?: string;
  }
): InferenceGenerateResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  text: data.text ?? '',
  model: data.model ?? '',
  provider: data.provider ?? '',
  isLocal: data.isLocal ?? false,
  adaptersApplied: data.adaptersApplied ?? [],
  inputTokens: data.inputTokens ?? 0,
  outputTokens: data.outputTokens ?? 0,
  responseTimeMs: data.responseTimeMs ?? 0,
  ...data
});

/**
 * Smart Inference Generate-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInferenceGenerateResultFromParams = (
  params: InferenceGenerateParams,
  differences: Omit<InferenceGenerateResult, 'context' | 'sessionId'>
): InferenceGenerateResult => transformPayload(params, differences);

/**
 * InferenceGenerate — Type-safe command executor
 *
 * Usage:
 *   import { InferenceGenerate } from '...shared/InferenceGenerateTypes';
 *   const result = await InferenceGenerate.execute({ ... });
 */
export const InferenceGenerate = {
  execute(params: CommandInput<InferenceGenerateParams>): Promise<InferenceGenerateResult> {
    return Commands.execute<InferenceGenerateParams, InferenceGenerateResult>('inference/generate', params as Partial<InferenceGenerateParams>);
  },
  commandName: 'inference/generate' as const,
} as const;
