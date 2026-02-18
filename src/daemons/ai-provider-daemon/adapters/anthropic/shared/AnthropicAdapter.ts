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
  ToolCall,
  NativeToolSpec,
  ContentPart,
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
import { BaseAIProviderAdapter } from '../../../shared/BaseAIProviderAdapter';
import { MediaContentFormatter } from '../../../shared/MediaContentFormatter';

/**
 * Anthropic API usage response structure
 */
interface AnthropicUsage {
  input_tokens: number;
  output_tokens: number;
  cache_creation_input_tokens?: number;
  cache_read_input_tokens?: number;
}

export class AnthropicAdapter extends BaseAIProviderAdapter {
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
    super();
    this.apiKey = apiKey || getSecret('ANTHROPIC_API_KEY', 'AnthropicAdapter') || '';
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

  protected async generateTextImpl(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    if (!this.isInitialized) {
      throw new AIProviderError('Adapter not initialized', 'adapter', 'NOT_INITIALIZED');
    }

    const startTime = Date.now();
    const requestId = request.requestId || `req-${Date.now()}`;
    // Use Claude Sonnet 4.5 (updated September 2025 - current)
    const model = request.model || MODEL_IDS.ANTHROPIC.SONNET_4_5;

    try {
      // Log incoming request messages
      const hasNativeTools = request.tools && request.tools.length > 0;
      this.log(request, 'debug', `📸 [ANTHROPIC-ADAPTER] generateText() called with: ${request.messages.length} messages, ${request.messages.filter(m => typeof m.content !== 'string').length} multimodal, ${hasNativeTools ? `${request.tools!.length} native tools` : 'no native tools'}`);

      // Convert messages to Anthropic format using MediaContentFormatter
      // Handles text, multimodal, tool_use, and tool_result content blocks
      const messages = request.messages.map((msg, index) => {
        const role = msg.role === 'assistant' ? 'assistant' as const : 'user' as const;

        if (typeof msg.content === 'string') {
          this.log(request, 'debug', `📸 [ANTHROPIC-ADAPTER] Message ${index}: ${role}, text-only`);
          return { role, content: msg.content };
        }

        // Check for tool_use or tool_result content blocks
        const parts = msg.content as ContentPart[];
        const hasToolBlocks = parts.some(p => p.type === 'tool_use' || p.type === 'tool_result');

        if (hasToolBlocks) {
          // Convert our ContentPart tool blocks to Anthropic's native format
          const anthropicContent = parts.map(part => {
            if (part.type === 'tool_use') {
              return { type: 'tool_use' as const, id: part.id, name: part.name, input: part.input };
            }
            if (part.type === 'tool_result') {
              return { type: 'tool_result' as const, tool_use_id: part.tool_use_id, content: part.content, ...(part.is_error && { is_error: true }) };
            }
            if (part.type === 'text') {
              return { type: 'text' as const, text: part.text };
            }
            // Other types (image, audio, video) — pass through MediaContentFormatter
            return null;
          }).filter(Boolean);

          this.log(request, 'debug', `📸 [ANTHROPIC-ADAPTER] Message ${index}: ${role}, ${anthropicContent.length} blocks (tool protocol)`);
          return { role, content: anthropicContent };
        }

        // Standard multimodal content
        this.log(request, 'debug', `📸 [ANTHROPIC-ADAPTER] Message ${index}: ${role}, MULTIMODAL`);
        return {
          role,
          content: MediaContentFormatter.formatForAnthropic(parts),
        };
      });

      this.log(request, 'debug', `📸 [ANTHROPIC-ADAPTER] Sending API request with ${messages.length} messages to Anthropic${hasNativeTools ? ' (with native tools)' : ''}`);

      // Build request body
      const requestBody: Record<string, unknown> = {
        model,
        messages,
        system: request.systemPrompt,
        max_tokens: request.maxTokens || 1024,
        temperature: request.temperature ?? 0.7,
        top_p: request.topP,
        stop_sequences: request.stopSequences,
      };

      // Add native tools if provided (Anthropic's JSON tool format)
      if (hasNativeTools) {
        requestBody.tools = request.tools;
        if (request.toolChoice) {
          // Anthropic tool_choice format: 'auto', 'any', 'none', or { type: 'tool', name: 'tool_name' }
          if (typeof request.toolChoice === 'object' && 'name' in request.toolChoice) {
            requestBody.tool_choice = { type: 'tool', name: request.toolChoice.name };
          } else {
            requestBody.tool_choice = { type: request.toolChoice };
          }
        }
      }

      // Make API request
      const response = await this.makeRequest<any>('/v1/messages', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': this.apiKey,
          'anthropic-version': '2023-06-01',
        },
        body: JSON.stringify(requestBody),
      });

      const responseTime = Date.now() - startTime;

      // Parse response - handle both text and tool_use content blocks
      // Build both flat text AND structured content blocks for the canonical agent loop
      let text = '';
      const toolCalls: ToolCall[] = [];
      const contentBlocks: ContentPart[] = [];

      for (const block of response.content || []) {
        if (block.type === 'text') {
          text += block.text;
          contentBlocks.push({ type: 'text', text: block.text });
        } else if (block.type === 'tool_use') {
          toolCalls.push({
            id: block.id,
            name: block.name,
            input: block.input || {},
          });
          contentBlocks.push({
            type: 'tool_use',
            id: block.id,
            name: block.name,
            input: block.input || {},
          });
          this.log(request, 'debug', `🔧 [ANTHROPIC-ADAPTER] Native tool call: ${block.name} (id: ${block.id})`);
        }
      }

      const hasToolCalls = toolCalls.length > 0;

      return {
        text,
        content: contentBlocks,
        finishReason: this.mapFinishReason(response.stop_reason),
        model: response.model || model,
        provider: this.providerId,
        usage: {
          inputTokens: response.usage?.input_tokens || 0,
          outputTokens: response.usage?.output_tokens || 0,
          totalTokens: (response.usage?.input_tokens || 0) + (response.usage?.output_tokens || 0),
          estimatedCost: this.calculateCost(response.usage, model),
        },
        responseTimeMs: responseTime,
        requestId,
        ...(hasToolCalls && { toolCalls }),
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
        supportsTools: true,  // Native tool_use support enabled
      },
      {
        id: MODEL_IDS.ANTHROPIC.OPUS_4,
        name: 'Claude 3 Opus',
        provider: 'anthropic',
        capabilities: ['text-generation', 'chat', 'multimodal'],
        contextWindow: 200000,
        maxOutputTokens: 4096,
        costPer1kTokens: { input: 0.015, output: 0.075 },
        supportsStreaming: true,
        supportsTools: true,  // Native tool_use support enabled
      },
      {
        id: MODEL_IDS.ANTHROPIC.HAIKU_3_5,
        name: 'Claude 3 Haiku',
        provider: 'anthropic',
        capabilities: ['text-generation', 'chat', 'multimodal'],
        contextWindow: 200000,
        maxOutputTokens: 4096,
        costPer1kTokens: { input: 0.00025, output: 0.00125 },
        supportsStreaming: true,
        supportsTools: true,  // Native tool_use support enabled
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
          model: MODEL_IDS.ANTHROPIC.HAIKU_3_5,
          messages: [{ role: 'user', content: 'hi' }],
          max_tokens: 1,
        }),
        signal: AbortSignal.timeout(5000),
      });

      const responseTime = Date.now() - startTime;

      return {
        status: response.ok ? 'healthy' : 'unhealthy',
        apiAvailable: response.ok,
        responseTimeMs: responseTime,
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
        responseTimeMs: Date.now() - startTime,
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

  protected async restartProvider(): Promise<void> {
    // API-based provider, no restart needed - just reinitialize
    await this.shutdown();
    await this.initialize();
  }

  // Helper methods

  private calculateCost(usage: AnthropicUsage | undefined, model: string): number {
    if (!usage) return 0;

    const models = {
      [MODEL_IDS.ANTHROPIC.SONNET_4_5]: { input: 0.003, output: 0.015 },
      [MODEL_IDS.ANTHROPIC.OPUS_4]: { input: 0.015, output: 0.075 },
      [MODEL_IDS.ANTHROPIC.HAIKU_3_5]: { input: 0.00025, output: 0.00125 },
    };

    const pricing = models[model as keyof typeof models];
    if (!pricing) return 0;

    const inputCost = (usage.input_tokens / 1000) * pricing.input;
    const outputCost = (usage.output_tokens / 1000) * pricing.output;

    return inputCost + outputCost;
  }

  // Multimodal content formatting now handled by MediaContentFormatter
  // See: daemons/ai-provider-daemon/shared/MediaContentFormatter.ts

  private mapFinishReason(reason: string): 'stop' | 'length' | 'error' | 'tool_use' {
    if (reason === 'end_turn') return 'stop';
    if (reason === 'max_tokens') return 'length';
    if (reason === 'tool_use') return 'tool_use';
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
      [ModelTier.FAST]: MODEL_IDS.ANTHROPIC.HAIKU_3_5,
      [ModelTier.BALANCED]: MODEL_IDS.ANTHROPIC.SONNET_4_5, // Latest Sonnet
      [ModelTier.PREMIUM]: MODEL_IDS.ANTHROPIC.OPUS_4,
      [ModelTier.LATEST]: MODEL_IDS.ANTHROPIC.SONNET_4_5, // Always latest
      [ModelTier.FREE]: MODEL_IDS.ANTHROPIC.HAIKU_3_5, // Cheapest, not truly free
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
