/**
 * AI Provider Types - Phase 1: PersonaUser Text Generation
 * =========================================================
 *
 * Simplified types focused on enabling PersonaUsers to generate text responses
 * using local Ollama models (llama3.2:1b for coordination, phi3:mini for chat).
 *
 * Architecture:
 * - Provider-agnostic interface for text generation
 * - Pluggable adapter system (Ollama, OpenAI, Anthropic)
 * - Simple request/response pattern
 * - Usage tracking and health monitoring
 *
 * Future phases will add:
 * - Image generation
 * - Media analysis
 * - Embeddings
 * - Advanced provider selection
 * - Cost optimization
 */

import type { JTAGContext, UUID } from '../../../system/core/types/JTAGTypes';

// ========================
// Core Request/Response
// ========================

export interface TextGenerationRequest {
  // Input
  messages: ChatMessage[];
  systemPrompt?: string;

  // Model configuration
  model?: string;
  temperature?: number;                  // 0-2 (default: 0.7)
  maxTokens?: number;                    // Max output tokens

  // Context
  context?: JTAGContext;
  requestId?: string;

  // Preferences
  preferredProvider?: 'ollama' | 'openai' | 'anthropic';
}

export interface TextGenerationResponse {
  // Generated content
  text: string;
  finishReason: 'stop' | 'length' | 'error';

  // Metadata
  model: string;
  provider: string;
  usage: UsageMetrics;
  responseTime: number;                  // milliseconds
  requestId: string;

  // Error handling
  error?: string;
}

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
  name?: string;                         // Participant name (for PersonaUsers)
  timestamp?: number;
}

// ========================
// Provider Adapter Interface
// ========================

export interface AIProviderAdapter {
  // Provider identification
  readonly providerId: string;
  readonly providerName: string;

  // Core operation
  generateText(request: TextGenerationRequest): Promise<TextGenerationResponse>;

  // Health and monitoring
  healthCheck(): Promise<HealthStatus>;
  getAvailableModels(): Promise<string[]>;

  // Lifecycle
  initialize(): Promise<void>;
  shutdown(): Promise<void>;
}

// ========================
// Provider Configuration
// ========================

export interface ProviderConfiguration {
  // Authentication (optional for local models)
  apiKey?: string;
  apiEndpoint?: string;

  // Request configuration
  timeout: number;                       // milliseconds (default: 30000)
  retryAttempts: number;                 // Number of retries (default: 3)
  retryDelay: number;                    // milliseconds (default: 1000)
  maxConcurrent?: number;                // Maximum concurrent requests (Ollama-specific, default: 4)

  // Quality settings
  defaultModel: string;
  defaultTemperature: number;            // 0-2

  // Logging
  logRequests: boolean;
}

// ========================
// Usage Tracking
// ========================

export interface UsageMetrics {
  // Token counts
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;

  // Cost (optional, mainly for cloud providers)
  estimatedCost?: number;                // USD
}

export interface HealthStatus {
  status: 'healthy' | 'degraded' | 'unhealthy';

  // Service health
  apiAvailable: boolean;
  responseTime: number;                  // milliseconds
  errorRate: number;                     // 0-1 (last 100 requests)

  // Last check
  lastChecked: number;                   // timestamp
  message?: string;                      // Health status message
}

// ========================
// Provider Registry
// ========================

export interface ProviderRegistration {
  providerId: string;
  adapter: AIProviderAdapter;
  configuration: ProviderConfiguration;
  priority: number;                      // Higher priority = preferred (0-100)
  enabled: boolean;
}

// ========================
// Error Types
// ========================

export class AIProviderError extends Error {
  constructor(
    message: string,
    public readonly provider: string,
    public readonly code: string,
    public readonly details?: Record<string, unknown>
  ) {
    super(message);
    this.name = 'AIProviderError';
  }
}

// ========================
// Ollama-Specific Types
// ========================

export interface OllamaGenerateRequest {
  model: string;
  prompt: string;
  system?: string;

  // Generation parameters
  temperature?: number;
  num_predict?: number;                  // Max tokens to generate

  // Context
  context?: number[];                    // For continuing conversations

  // Streaming
  stream?: boolean;
}

export interface OllamaGenerateResponse {
  model: string;
  created_at: string;
  response: string;
  done: boolean;

  // Context for follow-up
  context?: number[];

  // Timing metrics
  total_duration?: number;               // nanoseconds
  load_duration?: number;
  prompt_eval_count?: number;
  prompt_eval_duration?: number;
  eval_count?: number;
  eval_duration?: number;
}

export interface OllamaModelInfo {
  name: string;
  modified_at: string;
  size: number;
  digest: string;
  details: {
    format: string;
    family: string;
    families?: string[];
    parameter_size: string;
    quantization_level: string;
  };
}

export interface OllamaListResponse {
  models: OllamaModelInfo[];
}

// ========================
// Helper Functions
// ========================

/**
 * Convert chat messages to Ollama prompt format
 */
export function chatMessagesToPrompt(messages: ChatMessage[]): { prompt: string; system?: string } {
  const systemMessages = messages.filter(m => m.role === 'system');
  const conversationMessages = messages.filter(m => m.role !== 'system');

  // System prompt (combine all system messages)
  const system = systemMessages.length > 0
    ? systemMessages.map(m => m.content).join('\n\n')
    : undefined;

  // Build conversation prompt
  const prompt = conversationMessages
    .map(m => {
      const name = m.name ? `${m.name}: ` : '';
      const rolePrefix = m.role === 'user' ? 'Human: ' : 'Assistant: ';
      return `${rolePrefix}${name}${m.content}`;
    })
    .join('\n\n');

  return { prompt, system };
}

/**
 * Estimate token count (rough approximation: 1 token ≈ 4 characters)
 */
export function estimateTokenCount(text: string): number {
  return Math.ceil(text.length / 4);
}

/**
 * Create a unique request ID
 */
export function createRequestId(): string {
  return `ai_${Date.now()}_${Math.random().toString(36).substring(2, 10)}`;
}
