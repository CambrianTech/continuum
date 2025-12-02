/**
 * AnthropicAdapter - Anthropic Claude API
 *
 * Supports:
 * - Claude 3.5 Sonnet (best reasoning)
 * - Claude 3 Opus (most capable)
 * - Claude 3 Haiku (fast and cheap)
 * - Multimodal (vision)
 * - 200k context window
 *
 * Anthropic uses a proprietary API format (not OpenAI-compatible)
 */

import type {
  AIProviderAdapter,
  ModelCapability,
  ModelInfo,
  TextGenerationRequest,
  TextGenerationResponse,
  HealthStatus,
} from '../../../shared/AIProviderTypesV2';
import { AIProviderError } from '../../../shared/AIProviderTypesV2';
import { getSecret } from '../../../../../system/secrets/SecretManager';
import {
  ModelTier,
  ModelTags,
  ModelResolution,
  CostTier,
  calculateCostTier,
} from '../../../shared/ModelTiers';
import { MODEL_IDS } from '../../../../../system/shared/Constants';

export class AnthropicAdapter implements AIProviderAdapter {
  readonly providerId = 'anthropic';
  readonly providerName = 'Anthropic';
  readonly supportedCapabilities: ModelCapability[] = [
    'text-generation',
    'chat',
    'multimodal',
    'image-analysis',
  ];

  private apiKey: string;
  private baseUrl = 'https://api.anthropic.com';
  private timeout = 60000;
  private isInitialized = false;

  constructor(apiKey?: string) {
    this.apiKey = apiKey || getSecret('ANTHROPIC_API_KEY', 'AnthropicAdapter') || '';
  }

  /**
   * Helper to log with persona context
   * If persona context provided in request, prepends persona info
   * Otherwise just logs the message
   */
  protected log(request: TextGenerationRequest | null, level: 'info' | 'debug' | 'warn' | 'error', message: string): void {
    const prefix = request?.personaContext
      ? `[${request.personaContext.displayName}] `
      : '';
    console.log(`${prefix}${message}`);
  }

  async initialize(): Promise<void> {
    this.log(null, 'info', `🔌 ${this.providerName}: Initializing...`);

    if (!this.apiKey) {
      throw new AIProviderError(
        `${this.providerName} API key not configured`,
        'adapter',
        'MISSING_API_KEY'
      );
    }

    // Basic connectivity check (Anthropic doesn't have a health endpoint)
    this.isInitialized = true;
    this.log(null, 'info', `✅ ${this.providerName}: Initialized successfully`);
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    if (!this.isInitialized) {
      throw new AIProviderError('Adapter not initialized', 'adapter', 'NOT_INITIALIZED');
    }

    const startTime = Date.now();
    const requestId = request.requestId || `req-${Date.now()}`;
    // Use Claude Sonnet 4.5 (updated September 2025 - current)
    const model = request.model || MODEL_IDS.ANTHROPIC.SONNET_4_5;

    try {
      // Log incoming request messages
      this.log(request, 'debug', `📸 [ANTHROPIC-ADAPTER] generateText() called with: ${request.messages.length} messages, ${request.messages.filter(m => typeof m.content !== 'string').length} multimodal`);

      // Convert messages to Anthropic format
      const messages = request.messages.map((msg, index) => {
        const isMultimodal = typeof msg.content !== 'string';
        this.log(request, 'debug', `📸 [ANTHROPIC-ADAPTER] Message ${index}: ${msg.role}, ${isMultimodal ? 'MULTIMODAL' : 'text-only'}`);

        return {
          role: msg.role === 'assistant' ? 'assistant' : 'user',
          content: typeof msg.content === 'string'
            ? msg.content
            : this.formatMultimodalContent(msg.content),
        };
      });

      this.log(request, 'debug', `📸 [ANTHROPIC-ADAPTER] Sending API request with ${messages.length} messages to Anthropic`);

      // Make API request
      const response = await this.makeRequest<any>('/v1/messages', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': this.apiKey,
          'anthropic-version': '2023-06-01',
        },
        body: JSON.stringify({
          model,
          messages,
          system: request.systemPrompt,
          max_tokens: request.maxTokens || 1024,
          temperature: request.temperature ?? 0.7,
          top_p: request.topP,
          stop_sequences: request.stopSequences,
        }),
      });

      const responseTime = Date.now() - startTime;

      // Parse response
      const text = response.content?.[0]?.text || '';

      return {
        text,
        finishReason: this.mapFinishReason(response.stop_reason),
        model: response.model || model,
        provider: this.providerId,
        usage: {
          inputTokens: response.usage?.input_tokens || 0,
          outputTokens: response.usage?.output_tokens || 0,
          totalTokens: (response.usage?.input_tokens || 0) + (response.usage?.output_tokens || 0),
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

  async getAvailableModels(): Promise<ModelInfo[]> {
    return [
      {
        id: MODEL_IDS.ANTHROPIC.SONNET_4_5,
        name: 'Claude Sonnet 4.5',
        provider: 'anthropic',
        capabilities: ['text-generation', 'chat', 'multimodal'],
        contextWindow: 200000,
        maxOutputTokens: 8192,
        costPer1kTokens: { input: 0.003, output: 0.015 },
        supportsStreaming: true,
        supportsFunctions: false,
      },
      {
        id: MODEL_IDS.ANTHROPIC.OPUS_3,
        name: 'Claude 3 Opus',
        provider: 'anthropic',
        capabilities: ['text-generation', 'chat', 'multimodal'],
        contextWindow: 200000,
        maxOutputTokens: 4096,
        costPer1kTokens: { input: 0.015, output: 0.075 },
        supportsStreaming: true,
        supportsFunctions: false,
      },
      {
        id: MODEL_IDS.ANTHROPIC.HAIKU_3,
        name: 'Claude 3 Haiku',
        provider: 'anthropic',
        capabilities: ['text-generation', 'chat', 'multimodal'],
        contextWindow: 200000,
        maxOutputTokens: 4096,
        costPer1kTokens: { input: 0.00025, output: 0.00125 },
        supportsStreaming: true,
        supportsFunctions: false,
      },
    ];
  }

  async healthCheck(): Promise<HealthStatus> {
    const startTime = Date.now();

    try {
      // Anthropic doesn't have a dedicated health endpoint
      // Try a minimal API call
      const response = await fetch(`${this.baseUrl}/v1/messages`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': this.apiKey,
          'anthropic-version': '2023-06-01',
        },
        body: JSON.stringify({
          model: MODEL_IDS.ANTHROPIC.HAIKU_3,
          messages: [{ role: 'user', content: 'hi' }],
          max_tokens: 1,
        }),
        signal: AbortSignal.timeout(5000),
      });

      const responseTime = Date.now() - startTime;

      return {
        status: response.ok ? 'healthy' : 'unhealthy',
        apiAvailable: response.ok,
        responseTime,
        errorRate: response.ok ? 0 : 1,
        lastChecked: Date.now(),
        message: response.ok
          ? `${this.providerName} API is accessible`
          : `${this.providerName} API returned ${response.status}`,
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
    this.log(null, 'info', `🔄 ${this.providerName}: Shutting down (API adapter, no cleanup needed)`);
    this.isInitialized = false;
  }

  // Helper methods

  private calculateCost(usage: any, model: string): number {
    if (!usage) return 0;

    const models = {
      [MODEL_IDS.ANTHROPIC.SONNET_4_5]: { input: 0.003, output: 0.015 },
      [MODEL_IDS.ANTHROPIC.OPUS_3]: { input: 0.015, output: 0.075 },
      [MODEL_IDS.ANTHROPIC.HAIKU_3]: { input: 0.00025, output: 0.00125 },
    };

    const pricing = models[model as keyof typeof models];
    if (!pricing) return 0;

    const inputCost = (usage.input_tokens / 1000) * pricing.input;
    const outputCost = (usage.output_tokens / 1000) * pricing.output;

    return inputCost + outputCost;
  }

  private formatMultimodalContent(content: any[]): any {
    this.log(null, 'debug', `📸 [ANTHROPIC-ADAPTER] formatMultimodalContent() called with ${content.length} parts, ${content.some(p => p.type === 'image') ? 'has images' : 'no images'}`);

    const formatted = content.map((part, index) => {
      if (part.type === 'text') {
        this.log(null, 'debug', `📸 [ANTHROPIC-ADAPTER] Part ${index}: text (${part.text.length} chars)`);
        return { type: 'text', text: part.text };
      } else if (part.type === 'image') {
        this.log(null, 'debug', `📸 [ANTHROPIC-ADAPTER] Part ${index}: image detected (base64: ${!!part.base64}, url: ${!!part.url})`);

        // Handle base64 images (from screenshots, tool results)
        // Support both flat format (part.base64) and nested format (part.image.base64)
        const base64Data = part.base64 || part.image?.base64;
        const mimeType = part.mimeType || part.image?.mimeType || 'image/png';

        if (base64Data) {
          const formatted = {
            type: 'image',
            source: {
              type: 'base64',
              media_type: mimeType,
              data: base64Data
            }
          };
          this.log(null, 'debug', `📸 [ANTHROPIC-ADAPTER] Part ${index}: Formatted as base64 image (${formatted.source.data.length} chars, ${formatted.source.media_type})`);
          return formatted;
        }
        // Handle URL images
        if (part.url) {
          const formatted = {
            type: 'image',
            source: {
              type: 'url',
              url: part.url
            }
          };
          this.log(null, 'debug', `📸 [ANTHROPIC-ADAPTER] Part ${index}: Formatted as URL image (${formatted.source.url})`);
          return formatted;
        }
        // Fallback: try legacy format (part.image.url)
        if (part.image?.url) {
          const formatted = {
            type: 'image',
            source: {
              type: 'url',
              url: part.image.url
            }
          };
          this.log(null, 'debug', `📸 [ANTHROPIC-ADAPTER] Part ${index}: Formatted as legacy URL image (${formatted.source.url})`);
          return formatted;
        }

        this.log(null, 'warn', `📸 [ANTHROPIC-ADAPTER] Part ${index}: Image part has no valid source`);
      }
      return part;
    });

    this.log(null, 'debug', `📸 [ANTHROPIC-ADAPTER] formatMultimodalContent() result: ${formatted.length} parts (${formatted.filter(p => p.type === 'image').length} images, ${formatted.filter(p => p.type === 'text').length} text)`);

    return formatted;
  }

  private mapFinishReason(reason: string): 'stop' | 'length' | 'error' {
    if (reason === 'end_turn') return 'stop';
    if (reason === 'max_tokens') return 'length';
    return 'error';
  }

  private async makeRequest<T>(endpoint: string, options: RequestInit, retries = 3): Promise<T> {
    let lastError: Error | null = null;

    for (let attempt = 0; attempt < retries; attempt++) {
      try {
        const response = await fetch(`${this.baseUrl}${endpoint}`, {
          ...options,
          signal: AbortSignal.timeout(this.timeout),
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

  // =========================
  // Semantic Model Tier Resolution
  // =========================

  /**
   * Resolve semantic tier → actual model ID
   * User requirement: "I think you are on the right track with 'fast', 'free', 'balanced'"
   */
  async resolveModelTier(tier: ModelTier): Promise<ModelResolution> {
    // Anthropic tier mappings
    const tierMappings: Record<ModelTier, string> = {
      [ModelTier.FAST]: MODEL_IDS.ANTHROPIC.HAIKU_3,
      [ModelTier.BALANCED]: MODEL_IDS.ANTHROPIC.SONNET_4_5, // Latest Sonnet
      [ModelTier.PREMIUM]: MODEL_IDS.ANTHROPIC.OPUS_3,
      [ModelTier.LATEST]: MODEL_IDS.ANTHROPIC.SONNET_4_5, // Always latest
      [ModelTier.FREE]: MODEL_IDS.ANTHROPIC.HAIKU_3, // Cheapest, not truly free
    };

    const modelId = tierMappings[tier];
    if (!modelId) {
      throw new AIProviderError(
        `Unknown tier: ${tier}`,
        'adapter',
        'UNKNOWN_TIER'
      );
    }

    // Get model info to populate full resolution
    const models = await this.getAvailableModels();
    const modelInfo = models.find(m => m.id === modelId);

    if (!modelInfo) {
      throw new AIProviderError(
        `Model ${modelId} not found in available models`,
        'adapter',
        'MODEL_NOT_FOUND'
      );
    }

    // Classify the model to get full tags
    const tags = await this.classifyModel(modelId);

    return {
      modelId,
      provider: this.providerId,
      displayName: modelInfo.name,
      tags: tags!,
    };
  }

  /**
   * Classify model ID → semantic tier (BIDIRECTIONAL)
   * User requirement: "keep in mind you have to turn api results into these terms"
   */
  async classifyModel(modelId: string): Promise<ModelTags | null> {
    const models = await this.getAvailableModels();
    const modelInfo = models.find(m => m.id === modelId);

    if (!modelInfo) {
      return null;
    }

    // Determine tier based on model characteristics
    let tier: ModelTier;
    if (modelId.includes('haiku')) {
      tier = ModelTier.FAST;
    } else if (modelId.includes('sonnet')) {
      // Latest sonnet is BALANCED, older versions also BALANCED
      tier = ModelTier.BALANCED;
    } else if (modelId.includes('opus')) {
      tier = ModelTier.PREMIUM;
    } else {
      tier = ModelTier.BALANCED; // Default
    }

    const costTier = calculateCostTier(modelInfo.costPer1kTokens);

    return {
      tier,
      costTier,
      provider: this.providerId,
      actualModelId: modelId,
      displayName: modelInfo.name,
      contextWindow: modelInfo.contextWindow,
      costPer1kTokens: modelInfo.costPer1kTokens,
      capabilities: modelInfo.capabilities as string[],
      isLocal: false, // All Anthropic models are cloud-based
    };
  }

  /**
   * Get all models grouped by tier
   * Useful for UI: show "fast", "balanced", "premium" options
   */
  async getModelsByTier(): Promise<Map<ModelTier, ModelInfo[]>> {
    const models = await this.getAvailableModels();
    const tierMap = new Map<ModelTier, ModelInfo[]>();

    for (const model of models) {
      const tags = await this.classifyModel(model.id);
      if (tags) {
        const existing = tierMap.get(tags.tier) || [];
        existing.push(model);
        tierMap.set(tags.tier, existing);
      }
    }

    return tierMap;
  }
}
