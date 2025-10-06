/**
 * AI Generate Command Types
 * ==========================
 *
 * Types for text generation via AIProviderDaemon
 * Follows data command pattern for consistency
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { TextGenerationRequest, TextGenerationResponse } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypes';

// AI Generate Parameters
export interface AIGenerateParams extends JTAGPayload {
  // Input
  messages: Array<{
    role: 'system' | 'user' | 'assistant';
    content: string;
    name?: string;
  }>;
  systemPrompt?: string;

  // Model configuration
  model?: string;
  temperature?: number;
  maxTokens?: number;

  // Provider selection
  preferredProvider?: 'ollama' | 'openai' | 'anthropic';
}

// AI Generate Result
export interface AIGenerateResult extends JTAGPayload {
  readonly success: boolean;
  readonly text: string;
  readonly model: string;
  readonly provider: string;
  readonly usage: {
    inputTokens: number;
    outputTokens: number;
    totalTokens: number;
  };
  readonly responseTime: number;
  readonly requestId: string;
  readonly timestamp: string;
  readonly error?: string;
}

// Create params helper
export const createAIGenerateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIGenerateParams, 'context' | 'sessionId'>
): AIGenerateParams => createPayload(context, sessionId, data);

// Create result from params helper
export const createAIGenerateResultFromParams = (
  params: AIGenerateParams,
  differences: Omit<Partial<AIGenerateResult>, 'context' | 'sessionId'>
): AIGenerateResult => transformPayload(params, {
  success: false,
  text: '',
  model: '',
  provider: '',
  usage: {
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
  },
  responseTime: 0,
  requestId: '',
  timestamp: new Date().toISOString(),
  ...differences
});

// Helper to convert command params to daemon request
export function paramsToRequest(params: AIGenerateParams): TextGenerationRequest {
  return {
    messages: params.messages,
    systemPrompt: params.systemPrompt,
    model: params.model,
    temperature: params.temperature,
    maxTokens: params.maxTokens,
    preferredProvider: params.preferredProvider,
    context: params.context,
  };
}

// Helper to convert daemon response to command result
export function responseToResult(
  response: TextGenerationResponse,
  params: AIGenerateParams
): AIGenerateResult {
  return createAIGenerateResultFromParams(params, {
    success: true,
    text: response.text,
    model: response.model,
    provider: response.provider,
    usage: response.usage,
    responseTime: response.responseTime,
    requestId: response.requestId,
  });
}

// Error result helper
export function createErrorResult(
  params: AIGenerateParams,
  error: string
): AIGenerateResult {
  return createAIGenerateResultFromParams(params, {
    success: false,
    error,
  });
}
