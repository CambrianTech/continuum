/**
 * BaseOpenAICompatibleAdapter - Shared logic for OpenAI-compatible API providers
 *
 * Many providers use OpenAI's API format, so we can share 95% of the code:
 * ✅ OpenAI (official)
 * ✅ Together AI
 * ✅ Fireworks AI
 * ✅ Groq
 * ✅ Anyscale
 * ✅ Perplexity
 * ✅ Mistral AI
 * ✅ DeepInfra
 * ✅ Replicate (with adapter)
 *
 * Only differences:
 * - API base URL
 * - API key
 * - Available models
 * - Pricing (for cost tracking)
 *
 * This drastically simplifies adding new providers!
 */

import type {
  ModelCapability,
  ModelInfo,
  TextGenerationRequest,
  TextGenerationResponse,
  ImageGenerationRequest,
  ImageGenerationResponse,
  EmbeddingRequest,
  EmbeddingResponse,
  HealthStatus,
} from '../AIProviderTypesV2';
import { AIProviderError } from '../AIProviderTypesV2';
import { BaseAIProviderAdapter } from '../BaseAIProviderAdapter';
import { PricingManager } from '../PricingManager';
import { PricingFetcher } from '../PricingFetcher';
import { VisionCapabilityService } from '../VisionCapabilityService';
import { MediaContentFormatter } from '../MediaContentFormatter';

export interface OpenAICompatibleConfig {
  providerId: string;
  providerName: string;
  apiKey: string;
  baseUrl: string;
  defaultModel: string;
  timeout: number;
  supportedCapabilities: ModelCapability[];
  models?: ModelInfo[];  // Pre-defined model list (if API doesn't provide one)
}

/**
 * Base adapter for OpenAI-compatible APIs
 * Subclasses only need to provide config and optionally override pricing
 */
export abstract class BaseOpenAICompatibleAdapter extends BaseAIProviderAdapter {
  readonly providerId: string;
  readonly providerName: string;
  readonly supportedCapabilities: ModelCapability[];

  protected readonly config: OpenAICompatibleConfig;
  protected isInitialized = false;

  constructor(config: OpenAICompatibleConfig) {
    super();
    this.config = config;
    this.providerId = config.providerId;
    this.providerName = config.providerName;
    this.supportedCapabilities = config.supportedCapabilities;

    // Inject logger into PricingManager singleton (first adapter wins)
    PricingManager.getInstance().setLogger((msg: string) => this.log(null, 'warn', msg));
  }

  protected async initializeProvider(): Promise<void> {
    this.log(null, 'info', `🔌 ${this.providerName}: Initializing...`);

    // Verify API key is set
    if (!this.config.apiKey) {
      throw new AIProviderError(
        `${this.providerName} API key not configured`,
        'adapter',
        'MISSING_API_KEY'
      );
    }

    // Fetch live pricing if OpenRouter supports this provider
    await this.fetchAndCachePricing();

    // Health check to verify connectivity
    const health = await this.healthCheck();
    if (health.status === 'unhealthy') {
      this.log(null, 'warn', `⚠️  ${this.providerName}: Health check failed, but continuing (may work later)`);
    }

    this.isInitialized = true;
    this.log(null, 'info', `✅ ${this.providerName}: Initialized successfully`);
  }

  /**
   * Fetch live pricing from API sources (OpenRouter, provider API, etc.)
   * Override in subclass for provider-specific pricing sources
   */
  protected async fetchAndCachePricing(): Promise<void> {
    try {
      this.log(null, 'info', `💰 ${this.providerName}: Fetching live pricing...`);

      // Try OpenRouter first (aggregates many providers)
      const openRouterPricing = await PricingFetcher.fetchFromOpenRouter();

      let pricingCached = 0;
      const pricingManager = PricingManager.getInstance();

      for (const [modelId, pricing] of openRouterPricing.entries()) {
        // Parse OpenRouter model ID (e.g., "openai/gpt-4" → provider: "openai", model: "gpt-4")
        const parsed = PricingFetcher.parseOpenRouterModelId(modelId);
        if (parsed && parsed.provider === this.providerId) {
          pricingManager.registerAdapterPricing(parsed.provider, parsed.model, pricing);
          pricingCached++;
        }
      }

      if (pricingCached > 0) {
        this.log(null, 'info', `✅ ${this.providerName}: Cached live pricing for ${pricingCached} models from OpenRouter`);
      } else {
        this.log(null, 'info', `⚠️  ${this.providerName}: No live pricing found, falling back to static pricing`);
      }
    } catch (error) {
      this.log(null, 'warn', `⚠️  ${this.providerName}: Failed to fetch live pricing, using static fallback:`, error);
    }
  }

  /**
   * Text generation using OpenAI chat completions API
   * NOTE: This implements generateTextImpl (not generateText) - base class wraps with timeout/circuit breaker
   */
  protected async generateTextImpl(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    if (!this.isInitialized) {
      throw new AIProviderError('Adapter not initialized', 'adapter', 'NOT_INITIALIZED');
    }

    const startTime = Date.now();
    const requestId = request.requestId || `req-${Date.now()}`;
    const model = request.model || this.config.defaultModel;

    try {
      // Validate max_tokens doesn't exceed context window
      const modelInfo = this.config.models?.find(m => m.id === model);
      const contextWindow = modelInfo?.contextWindow || 8192; // Default to conservative 8K

      // Detect if model supports vision/multimodal using centralized VisionCapabilityService
      const visionService = VisionCapabilityService.getInstance();
      const supportsVision = visionService.supportsVision(this.providerId, model) ||
                           modelInfo?.capabilities?.includes('image-analysis') ||
                           modelInfo?.capabilities?.includes('multimodal') ||
                           this.supportedCapabilities.includes('image-analysis') ||
                           this.supportedCapabilities.includes('multimodal');

      // Convert messages to OpenAI format
      const messages = request.messages.map(msg => {
        if (typeof msg.content === 'string') {
          return { role: msg.role, content: msg.content, ...(msg.name && { name: msg.name }) };
        }

        // Multimodal content (ContentPart[])
        if (!supportsVision) {
          // Non-vision model: Extract text only using MediaContentFormatter
          const flattenedContent = MediaContentFormatter.extractTextOnly(msg.content);
          return { role: msg.role, content: flattenedContent, ...(msg.name && { name: msg.name }) };
        }

        // Vision model: Format multimodal content using MediaContentFormatter
        return {
          role: msg.role,
          content: MediaContentFormatter.formatForOpenAI(msg.content),
          ...(msg.name && { name: msg.name }),
        };
      });

      // Add system prompt if provided
      if (request.systemPrompt) {
        messages.unshift({
          role: 'system',
          content: request.systemPrompt,
        });
      }

      // Estimate input tokens (rough: 1 token ~= 4 chars)
      const messagesText = messages.map(m => typeof m.content === 'string' ? m.content : JSON.stringify(m.content)).join(' ');
      const estimatedInputTokens = Math.ceil(messagesText.length / 4);

      // Calculate available tokens for output
      const availableOutputTokens = contextWindow - estimatedInputTokens - 100; // Reserve 100 for formatting overhead

      // Cap max_tokens to fit within context window
      let adjustedMaxTokens = request.maxTokens;
      if (availableOutputTokens < (request.maxTokens || 0)) {
        adjustedMaxTokens = Math.max(100, availableOutputTokens); // Minimum 100 tokens output
        this.log(request, 'warn', `⚠️  ${this.providerName} (${model}): Requested ${request.maxTokens} output tokens, but only ${availableOutputTokens} available (context: ${contextWindow}, input: ${estimatedInputTokens}). Capping to ${adjustedMaxTokens}.`);
      }

      // Make API request
      const response = await this.makeRequest<any>('/v1/chat/completions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${this.config.apiKey}`,
        },
        body: JSON.stringify({
          model,
          messages,
          temperature: request.temperature ?? 0.7,
          max_tokens: adjustedMaxTokens,
          top_p: request.topP,
          stop: request.stopSequences,
          stream: false,
        }),
      });

      const responseTime = Date.now() - startTime;

      // Parse response
      const choice = response.choices?.[0];
      if (!choice) {
        throw new AIProviderError('No completion in response', 'provider', 'NO_COMPLETION');
      }

      const generationResponse: TextGenerationResponse = {
        text: choice.message?.content || '',
        finishReason: this.mapFinishReason(choice.finish_reason),
        model: response.model || model,
        provider: this.providerId,
        usage: {
          inputTokens: response.usage?.prompt_tokens || 0,
          outputTokens: response.usage?.completion_tokens || 0,
          totalTokens: response.usage?.total_tokens || 0,
          estimatedCost: this.calculateCost(response.usage, model),
        },
        responseTime,
        requestId,
      };

      // Database logging handled by AIProviderDaemon (single source of truth)
      return generationResponse;
    } catch (error) {
      // Error logging handled by AIProviderDaemon

      throw new AIProviderError(
        `Text generation failed: ${error instanceof Error ? error.message : String(error)}`,
        'provider',
        'GENERATION_FAILED',
        { model, requestId }
      );
    }
  }


  /**
   * Image generation using OpenAI images API (DALL-E)
   * Only implemented if provider supports it
   */
  async generateImage?(request: ImageGenerationRequest): Promise<ImageGenerationResponse> {
    if (!this.supportedCapabilities.includes('image-generation')) {
      throw new AIProviderError(
        `${this.providerName} does not support image generation`,
        'adapter',
        'UNSUPPORTED_CAPABILITY'
      );
    }

    const startTime = Date.now();
    const requestId = request.requestId || `req-${Date.now()}`;
    const model = request.model || 'dall-e-3';

    try {
      const response = await this.makeRequest<any>('/v1/images/generations', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${this.config.apiKey}`,
        },
        body: JSON.stringify({
          model,
          prompt: request.prompt,
          n: request.n || 1,
          size: request.size || '1024x1024',
          quality: request.quality || 'standard',
          style: request.style,
        }),
      });

      const responseTime = Date.now() - startTime;

      return {
        images: response.data.map((img: any) => ({
          url: img.url,
          base64: img.b64_json,
          revisedPrompt: img.revised_prompt,
        })),
        model: response.model || model,
        provider: this.providerId,
        responseTime,
        requestId,
      };
    } catch (error) {
      throw new AIProviderError(
        `Image generation failed: ${error instanceof Error ? error.message : String(error)}`,
        'provider',
        'GENERATION_FAILED'
      );
    }
  }

  /**
   * Create embeddings using OpenAI embeddings API
   */
  async createEmbedding?(request: EmbeddingRequest): Promise<EmbeddingResponse> {
    if (!this.supportedCapabilities.includes('embeddings')) {
      throw new AIProviderError(
        `${this.providerName} does not support embeddings`,
        'adapter',
        'UNSUPPORTED_CAPABILITY'
      );
    }

    const startTime = Date.now();
    const requestId = request.requestId || `req-${Date.now()}`;
    const model = request.model || 'text-embedding-3-small';

    try {
      const response = await this.makeRequest<any>('/v1/embeddings', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${this.config.apiKey}`,
        },
        body: JSON.stringify({
          model,
          input: request.input,
        }),
      });

      const responseTime = Date.now() - startTime;

      return {
        embeddings: response.data.map((item: any) => item.embedding),
        model: response.model || model,
        provider: this.providerId,
        usage: {
          inputTokens: response.usage?.prompt_tokens || 0,
          outputTokens: 0,
          totalTokens: response.usage?.total_tokens || 0,
        },
        responseTime,
        requestId,
      };
    } catch (error) {
      throw new AIProviderError(
        `Embedding creation failed: ${error instanceof Error ? error.message : String(error)}`,
        'provider',
        'EMBEDDING_FAILED'
      );
    }
  }

  /**
   * Get available models
   */
  async getAvailableModels(): Promise<ModelInfo[]> {
    // If models are pre-configured, return them
    if (this.config.models) {
      return this.config.models;
    }

    // Otherwise, try to fetch from API
    try {
      const response = await this.makeRequest<any>('/v1/models', {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${this.config.apiKey}`,
        },
      });

      return response.data.map((model: any) => this.parseModelInfo(model));
    } catch (error) {
      this.log(null, 'warn', `⚠️  ${this.providerName}: Failed to fetch models:`, error);
      return [];
    }
  }

  /**
   * Health check
   */
  async healthCheck(): Promise<HealthStatus> {
    const startTime = Date.now();

    try {
      // Try to list models as a health check
      await this.makeRequest<any>('/v1/models', {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${this.config.apiKey}`,
        },
      });

      const responseTime = Date.now() - startTime;

      return {
        status: 'healthy',
        apiAvailable: true,
        responseTime,
        errorRate: 0,
        lastChecked: Date.now(),
        message: `${this.providerName} API is accessible`,
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: Date.now() - startTime,
        errorRate: 1,
        lastChecked: Date.now(),
        message: `${this.providerName} API is not accessible: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }

  protected async shutdownProvider(): Promise<void> {
    this.log(null, 'info', `🔄 ${this.providerName}: Shutting down (API adapter, no cleanup needed)`);
    this.isInitialized = false;
  }

  /**
   * Restart API connection (default: clear state and reconnect)
   */
  protected async restartProvider(): Promise<void> {
    this.log(null, 'info', `🔄 ${this.providerName}: Restarting API connection...`);
    this.isInitialized = false;
    await new Promise(resolve => setTimeout(resolve, 2000));
    await this.initializeProvider();
  }

  // ========================
  // Helper Methods (Override in subclass for provider-specific behavior)
  // ========================

  /**
   * Calculate cost based on usage and model using PricingManager
   * Override in subclass ONLY if provider has special cost calculation needs
   */
  protected calculateCost(usage: any, model: string): number {
    if (!usage || (!usage.prompt_tokens && !usage.completion_tokens)) {
      return 0;
    }

    const pricingManager = PricingManager.getInstance();
    const pricing = pricingManager.getModelPricing(this.providerId, model);

    if (!pricing) {
      this.log(null, 'warn', `⚠️  ${this.providerName}: No pricing found for model ${model}, cost = $0`);
      return 0;
    }

    // Use PricingManager's conservative rounding (always round UP)
    return pricingManager.calculateCost(
      usage.prompt_tokens || 0,
      usage.completion_tokens || 0,
      pricing
    );
  }

  /**
   * Parse model info from API response
   * Override in subclass for provider-specific format
   */
  protected parseModelInfo(modelData: any): ModelInfo {
    return {
      id: modelData.id,
      name: modelData.id,
      provider: this.providerId,
      capabilities: ['text-generation'],  // Default, override in subclass
      contextWindow: 4096,  // Default, override in subclass
      supportsStreaming: true,
      supportsFunctions: false,
    };
  }

  // Multimodal content formatting now handled by MediaContentFormatter
  // See: daemons/ai-provider-daemon/shared/MediaContentFormatter.ts

  /**
   * Map OpenAI finish reason to our enum
   */
  protected mapFinishReason(reason: string): 'stop' | 'length' | 'error' {
    if (reason === 'stop') return 'stop';
    if (reason === 'length') return 'length';
    return 'error';
  }

  /**
   * Make HTTP request with retry logic
   */
  protected async makeRequest<T>(endpoint: string, options: RequestInit, retries = 3): Promise<T> {
    let lastError: Error | null = null;

    for (let attempt = 0; attempt < retries; attempt++) {
      try {
        const response = await fetch(`${this.config.baseUrl}${endpoint}`, {
          ...options,
          signal: AbortSignal.timeout(this.config.timeout),
        });

        if (!response.ok) {
          const errorBody = await response.text();

          // Detect specific error types for status reporting
          const statusCode = response.status;
          const errorLower = errorBody.toLowerCase();

          if (statusCode === 402 || statusCode === 429 ||
              errorLower.includes('insufficient_quota') ||
              errorLower.includes('quota') ||
              errorLower.includes('credits') ||
              errorLower.includes('billing') ||
              errorLower.includes('spending limit')) {

            // Determine specific status
            const isRateLimited = statusCode === 429 && !errorLower.includes('quota') && !errorLower.includes('credits');
            const status = isRateLimited ? 'rate_limited' : 'insufficient_funds';

            // Emit status event for UI (widgets can show 💰❌ or ⏳)
            const { Events } = await import('@system/core/shared/Events');
            await Events.emit('system:adapter:status', {
              providerId: this.providerId,
              status,
              message: `${this.providerName}: ${statusCode === 429 ? 'Rate limited or quota exhausted' : 'Insufficient funds/quota'}`,
              timestamp: Date.now(),
            });

            this.log(null, 'error', `💰 ${this.providerName}: ${status} - ${errorBody.slice(0, 200)}`);
          }

          throw new Error(`HTTP ${response.status}: ${errorBody}`);
        }

        return await response.json();
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        if (attempt < retries - 1) {
          await new Promise(resolve => setTimeout(resolve, 1000 * (attempt + 1)));
        }
      }
    }

    throw lastError || new Error('Request failed after retries');
  }
}
