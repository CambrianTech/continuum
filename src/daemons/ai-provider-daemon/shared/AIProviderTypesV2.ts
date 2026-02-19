/**
 * AI Provider Types V2 - Unified Type Layer
 * ==========================================
 *
 * Wire types come from Rust (via ts-rs generated types in shared/generated/ai/).
 * This file re-exports those types and adds TS-only infrastructure types
 * (adapter interface, error class, helpers, audio/image request/response types
 * not yet in Rust).
 *
 * ARCHITECTURE:
 * - Rust ai/types.rs is the single source of truth for wire types
 * - ts-rs generates TypeScript types at compile time
 * - This file re-exports generated types + defines TS-only extensions
 * - All 67+ consumers continue importing from this file (no import path changes)
 */

import type { JTAGContext, UUID } from '../../../system/core/types/JTAGTypes';
import type { ModelTier, ModelTags, ModelResolution } from './ModelTiers';

// ============================================================================
// WIRE TYPES (from Rust via ts-rs) — single source of truth
// ============================================================================

export type {
  ChatMessage,
  MessageContent,
  ContentPart,
  ImageInput,
  AudioInput,
  VideoInput,
} from '../../../shared/generated/ai';

export type {
  NativeToolSpec,
  ToolCall,
  ToolResult,
  ToolChoice,
  ToolInputSchema,
} from '../../../shared/generated/ai';

export type {
  FinishReason,
  UsageMetrics,
  RoutingInfo,
} from '../../../shared/generated/ai';

export type {
  ModelCapability,
  ModelInfo,
  CostPer1kTokens,
} from '../../../shared/generated/ai';

export type {
  HealthState,
  ActiveAdapterRequest,
} from '../../../shared/generated/ai';

export type {
  EmbeddingInput,
} from '../../../shared/generated/ai';

// ============================================================================
// TextGenerationRequest: Generated wire type + TS-only fields
// ============================================================================

import type { TextGenerationRequest as WireTextGenerationRequest } from '../../../shared/generated/ai';
import type { ModelCapability } from '../../../shared/generated/ai';

/**
 * TextGenerationRequest extends the Rust wire type with TS-only fields
 * that are consumed by the TypeScript adapter layer (not sent over IPC).
 *
 * Wire fields (from Rust): messages, systemPrompt, model, provider,
 *   temperature, maxTokens, topP, topK, stopSequences, tools, toolChoice,
 *   requestId, userId, roomId, purpose
 *
 * TS-only fields: intelligenceLevel, stream, context, preferredCapabilities,
 *   personaContext
 */
export interface TextGenerationRequest extends WireTextGenerationRequest {
  // Model intelligence level (PersonaUser property)
  // 1-30: Simple base models — 31-60: Capable — 61-85: Advanced — 86-100: Frontier
  intelligenceLevel?: number;

  // Streaming
  stream?: boolean;

  // Context
  context?: JTAGContext;

  // Capability preference for adapter selection
  preferredCapabilities?: ModelCapability[];

  // Persona context for logging (optional)
  personaContext?: {
    logDir: string;
    displayName: string;
    uniqueId: string;
  };

}

// ============================================================================
// TextGenerationResponse: Re-export generated type directly
// ============================================================================

export type { TextGenerationResponse } from '../../../shared/generated/ai';

// ============================================================================
// HealthStatus: Re-export generated type directly
// ============================================================================

export type { HealthStatus } from '../../../shared/generated/ai';

// ============================================================================
// Embedding types: Re-export generated wire types
// ============================================================================

export type { EmbeddingRequest as WireEmbeddingRequest } from '../../../shared/generated/ai';
export type { EmbeddingResponse as WireEmbeddingResponse } from '../../../shared/generated/ai';

/**
 * TS EmbeddingRequest extends the wire type with TS-only context fields.
 * Note: The wire type uses `input: EmbeddingInput` (string | string[]),
 * and has `provider` not `preferredProvider`.
 */
export interface EmbeddingRequest {
  input: string | string[];
  model?: string;

  context?: JTAGContext;
  requestId?: string;
  preferredProvider?: string;
}

export interface EmbeddingResponse {
  embeddings: number[][];
  model: string;
  provider: string;
  usage: { inputTokens: number; outputTokens: number; totalTokens: number; estimatedCost?: number };
  responseTimeMs: number;
  requestId: string;

  error?: string;
}

// ============================================================================
// TS-ONLY TYPES: Audio/Image requests (not yet in Rust)
// ============================================================================

export interface AudioGenerationRequest {
  text: string;
  voice?: string;
  model?: string;
  speed?: number;
  format?: 'mp3' | 'wav' | 'opus';

  context?: JTAGContext;
  requestId?: string;
  preferredProvider?: string;
}

export interface AudioTranscriptionRequest {
  audio: { url?: string; base64?: string; mimeType?: string; format?: 'mp3' | 'wav' | 'opus' | 'flac' };
  model?: string;
  language?: string;
  prompt?: string;
  temperature?: number;

  context?: JTAGContext;
  requestId?: string;
  preferredProvider?: string;
}

export interface ImageGenerationRequest {
  prompt: string;
  negativePrompt?: string;
  model?: string;
  size?: string;
  quality?: 'standard' | 'hd';
  style?: 'natural' | 'vivid';
  n?: number;

  context?: JTAGContext;
  requestId?: string;
  preferredProvider?: string;
}

export interface ImageAnalysisRequest {
  images: { url?: string; base64?: string; mimeType?: string }[];
  prompt: string;
  model?: string;
  maxTokens?: number;

  context?: JTAGContext;
  requestId?: string;
  preferredProvider?: string;
}

// ============================================================================
// TS-ONLY RESPONSE TYPES: Audio/Image (not yet in Rust)
// ============================================================================

export interface AudioGenerationResponse {
  audio: {
    url?: string;
    base64?: string;
    format: string;
  };

  model: string;
  provider: string;
  responseTimeMs: number;
  requestId: string;

  error?: string;
}

export interface AudioTranscriptionResponse {
  text: string;
  language?: string;
  segments?: Array<{
    start: number;
    end: number;
    text: string;
  }>;

  model: string;
  provider: string;
  responseTimeMs: number;
  requestId: string;

  error?: string;
}

export interface ImageGenerationResponse {
  images: Array<{
    url?: string;
    base64?: string;
    revisedPrompt?: string;
  }>;

  model: string;
  provider: string;
  responseTimeMs: number;
  requestId: string;

  error?: string;
}

export interface ImageAnalysisResponse {
  text: string;
  finishReason: 'stop' | 'length' | 'error';

  model: string;
  provider: string;
  usage: { inputTokens: number; outputTokens: number; totalTokens: number; estimatedCost?: number };
  responseTimeMs: number;
  requestId: string;

  error?: string;
}

// ============================================================================
// PROVIDER ADAPTER INTERFACE (TS-only — not a wire type)
// ============================================================================

export interface AIProviderAdapter {
  readonly providerId: string;
  readonly providerName: string;
  readonly supportedCapabilities: ModelCapability[];

  // Core operations (only implement what the provider supports)
  generateText?(request: TextGenerationRequest): Promise<import('../../../shared/generated/ai').TextGenerationResponse>;
  generateAudio?(request: AudioGenerationRequest): Promise<AudioGenerationResponse>;
  transcribeAudio?(request: AudioTranscriptionRequest): Promise<AudioTranscriptionResponse>;
  generateImage?(request: ImageGenerationRequest): Promise<ImageGenerationResponse>;
  analyzeImage?(request: ImageAnalysisRequest): Promise<ImageAnalysisResponse>;
  createEmbedding?(request: EmbeddingRequest): Promise<EmbeddingResponse>;

  // Skill management (optional)
  applySkill?(skillImplementation: unknown): Promise<void>;
  removeSkill?(skillId: string): Promise<void>;
  enableSkillTraining?(skillId: string): Promise<void>;
  disableSkillTraining?(skillId: string): Promise<void>;

  // Metadata
  getAvailableModels(): Promise<import('../../../shared/generated/ai').ModelInfo[]>;
  healthCheck(): Promise<import('../../../shared/generated/ai').HealthStatus>;

  // Queue monitoring (for load-aware PersonaInbox consolidation)
  getQueueStats?(): {
    queueSize: number;
    activeRequests: number;
    maxConcurrent: number;
    load: number;
  };

  // Health monitoring
  handleRestartRequest?(): Promise<void>;

  // Semantic Model Tier Resolution
  resolveModelTier?(tier: ModelTier): Promise<ModelResolution>;
  classifyModel?(modelId: string): Promise<ModelTags | null>;
  getModelsByTier?(): Promise<Map<ModelTier, import('../../../shared/generated/ai').ModelInfo[]>>;

  // Lifecycle
  initialize(): Promise<void>;
  shutdown(): Promise<void>;
}

// ============================================================================
// SUPPORTING TS-ONLY TYPES
// ============================================================================

export interface ProviderConfiguration {
  apiKey?: string;
  apiEndpoint?: string;
  timeout: number;
  retryAttempts: number;
  retryDelay: number;
  defaultModel: string;
  defaultTemperature: number;
  logRequests: boolean;
  maxConcurrent?: number;
}

export interface ProviderRegistration {
  providerId: string;
  adapter: AIProviderAdapter;
  configuration: ProviderConfiguration;
  priority: number;
  enabled: boolean;
}

// ============================================================================
// HELPER FUNCTIONS & CLASSES (TS-only)
// ============================================================================

export class AIProviderError extends Error {
  constructor(
    message: string,
    public readonly source: 'adapter' | 'provider' | 'daemon',
    public readonly code?: string,
    public readonly details?: Record<string, unknown>
  ) {
    super(message);
    this.name = 'AIProviderError';
  }
}

export function chatMessagesToPrompt(messages: import('../../../shared/generated/ai').ChatMessage[]): { prompt: string; systemPrompt?: string } {
  let systemPrompt: string | undefined;
  const conversationParts: string[] = [];

  for (const message of messages) {
    const contentText = typeof message.content === 'string'
      ? message.content
      : message.content.map(part => part.type === 'text' ? part.text : `[${part.type}]`).join(' ');

    if (message.role === 'system') {
      systemPrompt = contentText;
    } else if (message.role === 'user') {
      conversationParts.push(`User: ${contentText}`);
    } else if (message.role === 'assistant') {
      conversationParts.push(`Assistant: ${contentText}`);
    }
  }

  return {
    prompt: conversationParts.join('\n\n'),
    systemPrompt,
  };
}
