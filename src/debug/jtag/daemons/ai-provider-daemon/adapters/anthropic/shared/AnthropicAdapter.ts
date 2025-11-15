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

  async initialize(): Promise<void> {
    console.log(`🔌 ${this.providerName}: Initializing...`);

    if (!this.apiKey) {
      throw new AIProviderError(
        `${this.providerName} API key not configured`,
        'adapter',
        'MISSING_API_KEY'
      );
    }

    // Basic connectivity check (Anthropic doesn't have a health endpoint)
    this.isInitialized = true;
    console.log(`✅ ${this.providerName}: Initialized successfully`);
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    if (!this.isInitialized) {
      throw new AIProviderError('Adapter not initialized', 'adapter', 'NOT_INITIALIZED');
    }

    const startTime = Date.now();
    const requestId = request.requestId || `req-${Date.now()}`;
    // Use Claude 3.5 Sonnet v2 (updated January 2025)
    const model = request.model || 'claude-3-5-sonnet-20250122';

    try {
      // Convert messages to Anthropic format
      const messages = request.messages.map(msg => ({
        role: msg.role === 'assistant' ? 'assistant' : 'user',
        content: typeof msg.content === 'string'
          ? msg.content
          : this.formatMultimodalContent(msg.content),
      }));

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
        id: 'claude-3-5-sonnet-20250122',
        name: 'Claude 3.5 Sonnet v2',
        provider: 'anthropic',
        capabilities: ['text-generation', 'chat', 'multimodal'],
        contextWindow: 200000,
        maxOutputTokens: 8192,
        costPer1kTokens: { input: 0.003, output: 0.015 },
        supportsStreaming: true,
        supportsFunctions: false,
      },
      {
        id: 'claude-3-5-sonnet-20241022',
        name: 'Claude 3.5 Sonnet (Legacy)',
        provider: 'anthropic',
        capabilities: ['text-generation', 'chat', 'multimodal'],
        contextWindow: 200000,
        maxOutputTokens: 8192,
        costPer1kTokens: { input: 0.003, output: 0.015 },
        supportsStreaming: true,
        supportsFunctions: false,
      },
      {
        id: 'claude-3-opus-20240229',
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
        id: 'claude-3-haiku-20240307',
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
          model: 'claude-3-haiku-20240307',
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
    console.log(`🔄 ${this.providerName}: Shutting down (API adapter, no cleanup needed)`);
    this.isInitialized = false;
  }

  // Helper methods

  private calculateCost(usage: any, model: string): number {
    if (!usage) return 0;

    const models = {
      'claude-3-5-sonnet-20241022': { input: 0.003, output: 0.015 },
      'claude-3-opus-20240229': { input: 0.015, output: 0.075 },
      'claude-3-haiku-20240307': { input: 0.00025, output: 0.00125 },
    };

    const pricing = models[model as keyof typeof models];
    if (!pricing) return 0;

    const inputCost = (usage.input_tokens / 1000) * pricing.input;
    const outputCost = (usage.output_tokens / 1000) * pricing.output;

    return inputCost + outputCost;
  }

  private formatMultimodalContent(content: any[]): any {
    return content.map(part => {
      if (part.type === 'text') {
        return { type: 'text', text: part.text };
      } else if (part.type === 'image') {
        return {
          type: 'image',
          source: {
            type: 'url',
            url: part.image.url,
          },
        };
      }
      return part;
    });
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
      [ModelTier.FAST]: 'claude-3-haiku-20240307',
      [ModelTier.BALANCED]: 'claude-3-5-sonnet-20250122', // Latest Sonnet
      [ModelTier.PREMIUM]: 'claude-3-opus-20240229',
      [ModelTier.LATEST]: 'claude-3-5-sonnet-20250122', // Always latest
      [ModelTier.FREE]: 'claude-3-haiku-20240307', // Cheapest, not truly free
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
