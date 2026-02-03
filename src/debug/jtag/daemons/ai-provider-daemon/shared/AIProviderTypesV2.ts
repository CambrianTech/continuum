/**
 * AI Provider Types V2 - Multimodal Support
 * ==========================================
 *
 * Unified interface for text, audio, video, image, and multimodal AI providers.
 * Supports both local inference (Ollama, MLX) and API providers (OpenAI, Anthropic, etc.)
 *
 * Capabilities:
 * - Text generation (LLMs)
 * - Audio generation/transcription (TTS, STT)
 * - Image generation/analysis (DALL-E, Stable Diffusion, vision models)
 * - Video generation/analysis
 * - Multimodal (text + image input, etc.)
 * - Embeddings
 */

import type { JTAGContext, UUID } from '../../../system/core/types/JTAGTypes';
import type { ModelTier, ModelTags, ModelResolution } from './ModelTiers';

// ========================
// Model Capabilities
// ========================

export type ModelCapability =
  | 'text-generation'      // LLMs (GPT, Claude, Llama)
  | 'text-completion'      // Completion-only models
  | 'chat'                 // Chat-optimized models
  | 'audio-generation'     // TTS (text-to-speech)
  | 'audio-transcription'  // STT (speech-to-text)
  | 'image-generation'     // DALL-E, Stable Diffusion
  | 'image-analysis'       // Vision models (GPT-4V, Claude 3)
  | 'video-generation'     // Sora, etc.
  | 'video-analysis'       // Video understanding
  | 'embeddings'           // Text/image embeddings
  | 'multimodal';          // Combines multiple modalities

export interface ModelInfo {
  id: string;
  name: string;
  provider: string;
  capabilities: ModelCapability[];
  contextWindow: number;
  maxOutputTokens?: number;
  costPer1kTokens?: { input: number; output: number };
  supportsStreaming: boolean;
  supportsFunctions: boolean;
}

// ========================
// Universal Request Types
// ========================

export type ContentPart =
  | { type: 'text'; text: string }
  | { type: 'image'; image: ImageInput }
  | { type: 'audio'; audio: AudioInput }
  | { type: 'video'; video: VideoInput }
  | { type: 'tool_use'; id: string; name: string; input: Record<string, unknown> }
  | { type: 'tool_result'; tool_use_id: string; content: string; is_error?: boolean };

export interface ImageInput {
  url?: string;
  base64?: string;
  mimeType?: string;
}

export interface AudioInput {
  url?: string;
  base64?: string;
  mimeType?: string;
  format?: 'mp3' | 'wav' | 'opus' | 'flac';
}

export interface VideoInput {
  url?: string;
  base64?: string;
  mimeType?: string;
}

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string | ContentPart[];  // Simple string or rich multimodal content
  name?: string;
  timestamp?: number;
}

// ========================
// Native Tool Support (for providers like Anthropic that support JSON tools)
// ========================

/**
 * Native tool specification for providers with JSON tool support
 * (Anthropic, OpenAI, etc.)
 */
export interface NativeToolSpec {
  name: string;
  description: string;
  input_schema: {
    type: 'object';
    properties: Record<string, {
      type: string;
      description?: string;
      enum?: string[];
    }>;
    required?: string[];
  };
}

/**
 * Tool call from AI response (when AI wants to use a tool)
 */
export interface ToolCall {
  id: string;         // Unique ID for this tool use (e.g., "toolu_01A...")
  name: string;       // Tool name
  input: Record<string, unknown>;  // Tool parameters
}

/**
 * Tool result to send back to AI after execution
 */
export interface ToolResult {
  tool_use_id: string;  // Matches ToolCall.id
  content: string;       // Tool execution result (or error message)
  is_error?: boolean;    // True if tool execution failed
}

// ========================
// Request Types by Capability
// ========================

export interface TextGenerationRequest {
  messages: ChatMessage[];
  systemPrompt?: string;

  // Model config
  model?: string;
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  topK?: number;
  stopSequences?: string[];

  // Native tool support (for Anthropic, OpenAI JSON tools)
  // When provided, adapter should use native tool calling instead of XML
  tools?: NativeToolSpec[];
  tool_choice?: 'auto' | 'any' | 'none' | { name: string };

  // Model intelligence level (PersonaUser property)
  // Determines prompt format and capabilities
  // 1-30: Simple base models (GPT-2, DistilGPT-2) - pattern matching only
  // 31-60: Capable instruction-tuned models (Llama 7B, Phi-2) - basic reasoning
  // 61-85: Advanced models (Claude Haiku, GPT-3.5) - strong reasoning
  // 86-100: Frontier models (Claude Sonnet/Opus, GPT-4) - exceptional reasoning
  intelligenceLevel?: number;

  // Streaming
  stream?: boolean;

  // Context
  context?: JTAGContext;
  requestId?: string;

  // Provider preference
  preferredProvider?: string;
  preferredCapabilities?: ModelCapability[];

  // Cost tracking metadata (optional)
  userId?: UUID;
  roomId?: UUID;
  purpose?: string;  // 'chat', 'should-respond', 'rag', 'embedding', etc.

  // Persona context for logging (optional)
  // When provided, adapters can log to persona-specific log files
  personaContext?: {
    logDir: string;      // e.g., '.continuum/personas/helper-ai-12345678/logs'
    displayName: string; // e.g., 'Helper AI'
    uniqueId: string;    // e.g., 'helper-ai-12345678'
  };

  /**
   * Active LoRA adapters to apply during generation (PersonaGenome integration)
   *
   * When provided, CandleAdapter will ensure these adapters are loaded and applied
   * before generation. This enables the "genome vision" - personas can have
   * skill-specific fine-tuned weights that modify base model behavior.
   *
   * Example: persona with typescript-expertise and code-review adapters active
   * will generate responses using those specialized weights.
   *
   * NOTE: Only supported by CandleAdapter. Other adapters will ignore this field.
   */
  activeAdapters?: Array<{
    /** Adapter name (e.g., 'typescript-expertise') */
    name: string;
    /** Path to adapter weights */
    path: string;
    /** Domain this adapter specializes in */
    domain: string;
  }>;
}

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
  audio: AudioInput;
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
  images: ImageInput[];
  prompt: string;
  model?: string;
  maxTokens?: number;

  context?: JTAGContext;
  requestId?: string;
  preferredProvider?: string;
}

export interface EmbeddingRequest {
  input: string | string[];
  model?: string;

  context?: JTAGContext;
  requestId?: string;
  preferredProvider?: string;
}

// ========================
// Response Types
// ========================

/**
 * Routing observability - see exactly what happened during inference
 * without grep'ing through logs
 */
export interface RoutingInfo {
  /** Which adapter actually handled this request */
  provider: string;           // 'candle' | 'anthropic' | 'openai' | 'groq' | etc.

  /** Was this local inference (Candle) vs cloud API? */
  isLocal: boolean;

  /** Why was this adapter selected? */
  routingReason:
    | 'explicit_provider'      // preferredProvider was specified
    | 'provider_aliasing'      // Legacy 'ollama' requests aliased to 'candle'
    | 'model_detection'        // Model name matched local pattern (DEPRECATED - to be removed)
    | 'default_priority'       // Selected by priority order
    | 'fallback';              // Primary failed, used fallback

  /** LoRA adapters that were active during generation */
  adaptersApplied: string[];   // e.g., ['typescript-expertise', 'code-review']

  /** If model was remapped (e.g., llama3.2:3b → Qwen/Qwen2-1.5B-Instruct) */
  modelMapped?: string;

  /** Original model requested (before any mapping) */
  modelRequested?: string;
}

export interface TextGenerationResponse {
  text: string;
  finishReason: 'stop' | 'length' | 'error' | 'tool_use';

  /**
   * Full content blocks from the model response.
   * Contains text blocks, tool_use blocks, etc. in the order the model produced them.
   * When finishReason is 'tool_use', this will contain both text and tool_use blocks.
   * Adapters MUST populate this for the canonical agent loop to work.
   */
  content?: ContentPart[];

  model: string;
  provider: string;
  usage: UsageMetrics;
  responseTime: number;
  requestId: string;

  /**
   * Routing observability - ALWAYS populated by AIProviderDaemon
   * Shows exactly how this request was handled:
   * - Which provider was used and why
   * - Whether local or cloud
   * - Which LoRA adapters were applied
   * - Any model mapping that occurred
   *
   * Optional at adapter level (adapter can provide additional info like adaptersApplied),
   * but AIProviderDaemon ALWAYS ensures this field is populated in the final response.
   */
  routing?: RoutingInfo;

  // Native tool calls (when AI wants to use tools)
  // Present when finishReason is 'tool_use'
  toolCalls?: ToolCall[];

  error?: string;
}

export interface AudioGenerationResponse {
  audio: {
    url?: string;
    base64?: string;
    format: string;
  };

  model: string;
  provider: string;
  responseTime: number;
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
  responseTime: number;
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
  responseTime: number;
  requestId: string;

  error?: string;
}

export interface ImageAnalysisResponse {
  text: string;
  finishReason: 'stop' | 'length' | 'error';

  model: string;
  provider: string;
  usage: UsageMetrics;
  responseTime: number;
  requestId: string;

  error?: string;
}

export interface EmbeddingResponse {
  embeddings: number[][];
  model: string;
  provider: string;
  usage: UsageMetrics;
  responseTime: number;
  requestId: string;

  error?: string;
}

// ========================
// Provider Adapter Interface
// ========================

export interface AIProviderAdapter {
  readonly providerId: string;
  readonly providerName: string;
  readonly supportedCapabilities: ModelCapability[];

  // Core operations (only implement what the provider supports)
  generateText?(request: TextGenerationRequest): Promise<TextGenerationResponse>;
  generateAudio?(request: AudioGenerationRequest): Promise<AudioGenerationResponse>;
  transcribeAudio?(request: AudioTranscriptionRequest): Promise<AudioTranscriptionResponse>;
  generateImage?(request: ImageGenerationRequest): Promise<ImageGenerationResponse>;
  analyzeImage?(request: ImageAnalysisRequest): Promise<ImageAnalysisResponse>;
  createEmbedding?(request: EmbeddingRequest): Promise<EmbeddingResponse>;

  // Skill management (optional - only providers that support skill modification)
  // Examples:
  // - Ollama: Load LoRA adapter weights
  // - Claude/GPT: Inject RAG context or modify system prompt
  // - Any provider: Add few-shot examples or tools
  applySkill?(skillImplementation: unknown): Promise<void>;
  removeSkill?(skillId: string): Promise<void>;
  enableSkillTraining?(skillId: string): Promise<void>;
  disableSkillTraining?(skillId: string): Promise<void>;

  // Metadata
  getAvailableModels(): Promise<ModelInfo[]>;
  healthCheck(): Promise<HealthStatus>;

  // Queue monitoring (for load-aware PersonaInbox consolidation)
  // Returns current queue state for feedback-driven load management
  getQueueStats?(): {
    queueSize: number;      // Number of requests waiting
    activeRequests: number; // Number currently being processed
    maxConcurrent: number;  // Maximum allowed concurrent requests
    load: number;           // Queue pressure (0.0-1.0, calculated as (queueSize + activeRequests) / maxConcurrent)
  };

  // Health monitoring (called by AdapterHealthMonitor when adapter is unhealthy)
  handleRestartRequest?(): Promise<void>;

  // Semantic Model Tier Resolution (NEW)
  // Bidirectional mapping: tier ↔ model ID
  // User requirement: "turn api results into these terms"

  /**
   * Resolve semantic tier to actual model ID
   * Example: tier='balanced' → 'claude-3-5-sonnet-20250122'
   */
  resolveModelTier?(tier: ModelTier): Promise<ModelResolution>;

  /**
   * Classify model ID back into semantic tier (BIDIRECTIONAL)
   * Example: 'claude-3-5-sonnet-20250122' → { tier: 'balanced', costTier: 'medium', ... }
   */
  classifyModel?(modelId: string): Promise<ModelTags | null>;

  /**
   * Get all models grouped by tier
   * Useful for UI showing "fast", "balanced", "premium", "free" options
   */
  getModelsByTier?(): Promise<Map<ModelTier, ModelInfo[]>>;

  // Lifecycle
  initialize(): Promise<void>;
  shutdown(): Promise<void>;
}

// ========================
// Supporting Types
// ========================

export interface UsageMetrics {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  estimatedCost?: number;
}

export interface HealthStatus {
  /**
   * Adapter health status:
   * - healthy: Working normally
   * - degraded: Slow but functional
   * - unhealthy: Not responding
   * - insufficient_funds: API quota/credits exhausted (💰❌)
   * - rate_limited: Too many requests (⏳)
   */
  status: 'healthy' | 'degraded' | 'unhealthy' | 'insufficient_funds' | 'rate_limited';
  apiAvailable: boolean;
  responseTime: number;
  errorRate: number;
  lastChecked: number;
  message?: string;
}

export interface ProviderConfiguration {
  apiKey?: string;
  apiEndpoint?: string;
  timeout: number;
  retryAttempts: number;
  retryDelay: number;
  defaultModel: string;
  defaultTemperature: number;
  logRequests: boolean;
  maxConcurrent?: number;  // For request queue management
}

export interface ProviderRegistration {
  providerId: string;
  adapter: AIProviderAdapter;
  configuration: ProviderConfiguration;
  priority: number;
  enabled: boolean;
}

// ========================
// Helper Functions
// ========================

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

export function chatMessagesToPrompt(messages: ChatMessage[]): { prompt: string; systemPrompt?: string } {
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
