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
  AIProviderAdapter,
  ModelCapability,
  ModelInfo,
  TextGenerationRequest,
  TextGenerationResponse,
  ImageGenerationRequest,
  ImageGenerationResponse,
  EmbeddingRequest,
  EmbeddingResponse,
  HealthStatus,
  UsageMetrics,
} from '../AIProviderTypesV2';
import { AIProviderError } from '../AIProviderTypesV2';

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
export abstract class BaseOpenAICompatibleAdapter implements AIProviderAdapter {
  readonly providerId: string;
  readonly providerName: string;
  readonly supportedCapabilities: ModelCapability[];

  protected readonly config: OpenAICompatibleConfig;
  protected isInitialized = false;

  constructor(config: OpenAICompatibleConfig) {
    this.config = config;
    this.providerId = config.providerId;
    this.providerName = config.providerName;
    this.supportedCapabilities = config.supportedCapabilities;
  }

  async initialize(): Promise<void> {
    console.log(`🔌 ${this.providerName}: Initializing...`);

    // Verify API key is set
    if (!this.config.apiKey) {
      throw new AIProviderError(
        `${this.providerName} API key not configured`,
        'adapter',
        'MISSING_API_KEY'
      );
    }

    // Health check to verify connectivity
    const health = await this.healthCheck();
    if (health.status === 'unhealthy') {
      console.warn(`⚠️  ${this.providerName}: Health check failed, but continuing (may work later)`);
    }

    this.isInitialized = true;
    console.log(`✅ ${this.providerName}: Initialized successfully`);
  }

  /**
   * Text generation using OpenAI chat completions API
   */
  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    if (!this.isInitialized) {
      throw new AIProviderError('Adapter not initialized', 'adapter', 'NOT_INITIALIZED');
    }

    const startTime = Date.now();
    const requestId = request.requestId || `req-${Date.now()}`;
    const model = request.model || this.config.defaultModel;

    try {
      // Convert messages to OpenAI format
      const messages = request.messages.map(msg => ({
        role: msg.role,
        content: typeof msg.content === 'string' ? msg.content : this.formatMultimodalContent(msg.content),
        ...(msg.name && { name: msg.name }),
      }));

      // Add system prompt if provided
      if (request.systemPrompt) {
        messages.unshift({
          role: 'system',
          content: request.systemPrompt,
        });
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
          max_tokens: request.maxTokens,
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

      return {
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
    } catch (error) {
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
      console.warn(`⚠️  ${this.providerName}: Failed to fetch models:`, error);
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

  async shutdown(): Promise<void> {
    console.log(`🔄 ${this.providerName}: Shutting down (API adapter, no cleanup needed)`);
    this.isInitialized = false;
  }

  // ========================
  // Helper Methods (Override in subclass for provider-specific behavior)
  // ========================

  /**
   * Calculate cost based on usage and model
   * Override in subclass for provider-specific pricing
   */
  protected calculateCost(usage: any, model: string): number {
    // Default: $0 (override in subclass with actual pricing)
    return 0;
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

  /**
   * Format multimodal content for OpenAI API
   */
  protected formatMultimodalContent(content: any[]): any {
    return content.map(part => {
      if (part.type === 'text') {
        return { type: 'text', text: part.text };
      } else if (part.type === 'image') {
        return {
          type: 'image_url',
          image_url: {
            url: part.image.url || `data:image/jpeg;base64,${part.image.base64}`,
          },
        };
      }
      return part;
    });
  }

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
