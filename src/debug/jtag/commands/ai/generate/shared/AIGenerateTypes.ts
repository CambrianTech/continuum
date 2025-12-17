/**
 * AI Generate Command Types
 * ==========================
 *
 * Types for text generation via AIProviderDaemon
 * Follows data command pattern for consistency
 */

import type { CommandParams, JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { TextGenerationRequest, TextGenerationResponse } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

// AI Generate Parameters
export interface AIGenerateParams extends CommandParams {
  // Input - Two modes: Direct messages OR RAG context building
  // Mode 1: Direct messages (existing behavior)
  messages?: Array<{
    role: 'system' | 'user' | 'assistant';
    content: string;
    name?: string;
  }>;
  systemPrompt?: string;

  // Mode 2: RAG context building (chat-specific, will be template-driven later)
  roomId?: UUID;      // Chat room context
  personaId?: UUID;   // Persona generating response (optional, auto-selects first persona)
  maxMessages?: number; // How many messages to include in context (default: 20)
  includeArtifacts?: boolean; // Include images/attachments (default: true)
  includeMemories?: boolean;  // Include private memories (default: true)

  // Preview mode - returns request instead of calling LLM
  preview?: boolean;

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

  // Generation result (when preview=false)
  readonly text?: string;
  readonly model?: string;
  readonly provider?: string;
  readonly usage?: {
    inputTokens: number;
    outputTokens: number;
    totalTokens: number;
  };
  readonly responseTime?: number;
  readonly requestId?: string;

  // Preview result (when preview=true)
  readonly preview?: boolean;
  readonly request?: TextGenerationRequest; // Exact request that would be sent
  readonly formatted?: string;              // Human-readable preview
  readonly ragContext?: any;                // Full RAG context for debugging

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
  timestamp: new Date().toISOString(),
  ...differences
});

// Helper to convert command params to daemon request
// Note: Only call this when params.messages is provided (direct mode)
export function paramsToRequest(params: AIGenerateParams): TextGenerationRequest {
  if (!params.messages) {
    throw new Error('params.messages is required for paramsToRequest (use RAG mode with roomId instead)');
  }

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
