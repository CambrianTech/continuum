/**
 * TogetherBaseConfig - Shared configuration for all Together AI adapters
 *
 * This is the foundation of the modular architecture:
 * - ONE place for API key, base URL, auth
 * - Cached model list (fetched once, shared by all)
 * - Cached pricing data (updated in one place)
 * - Shared request method (consistent error handling)
 *
 * Used by:
 * - TogetherTextAdapter (inference)
 * - TogetherTextFineTuning (training)
 * - Future: TogetherAudioAdapter, TogetherVideoAdapter, etc.
 *
 * Benefits:
 * - Zero code duplication
 * - Consistent auth across all capabilities
 * - Single source of truth for Together AI config
 */

import { getSecret } from '../../../../../system/secrets/SecretManager';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';

/**
 * Shared configuration base for Together AI
 *
 * All Together AI adapters (inference and fine-tuning) share this config
 */
export class TogetherBaseConfig {
  readonly providerId = 'together';
  readonly providerName = 'Together AI';
  readonly baseUrl = 'https://api.together.xyz';
  readonly apiKey: string;

  private modelsCache?: ModelInfo[];
  private modelsFetchedAt?: number;
  private readonly CACHE_TTL = 3600000; // 1 hour

  constructor(apiKey?: string) {
    this.apiKey = apiKey || getSecret('TOGETHER_API_KEY', 'TogetherBaseConfig') || '';

    if (!this.apiKey) {
      console.warn('⚠️  TogetherBaseConfig: No API key found. Set TOGETHER_API_KEY in SecretManager.');
    }
  }

  /**
   * Get available models (cached across all adapters)
   *
   * Fetches from Together API on first call, then caches for 1 hour.
   * Shared by inference and fine-tuning adapters.
   */
  async getModels(): Promise<ModelInfo[]> {
    const now = Date.now();

    // Return cached models if fresh
    if (this.modelsCache && this.modelsFetchedAt && (now - this.modelsFetchedAt < this.CACHE_TTL)) {
      return this.modelsCache;
    }

    // Fetch fresh models
    try {
      const response = await fetch(`${this.baseUrl}/v1/models`, {
        headers: {
          'Authorization': `Bearer ${this.apiKey}`
        }
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Together API error: ${response.status} - ${errorText}`);
      }

      const data = await response.json() as {
        data: Array<{
          id: string;
          type: string;
          created: number;
          context_length?: number;
          max_tokens?: number;
        }>
      };

      // Map to ModelInfo format — use API-reported context_length when available
      this.modelsCache = data.data.map(model => ({
        id: model.id,
        name: model.id,
        provider: 'together',
        capabilities: ['text-generation', 'chat'] as import('../../../shared/AIProviderTypesV2').ModelCapability[],
        contextWindow: model.context_length || 128000,
        maxOutputTokens: model.max_tokens || 4096,
        costPer1kTokens: { input: 0.0002, output: 0.0002 },
        supportsStreaming: true,
        supportsTools: false
      }));

      this.modelsFetchedAt = now;

      return this.modelsCache;
    } catch (error) {
      console.error('❌ TogetherBaseConfig: Failed to fetch models:', error);

      // Return hardcoded defaults on error
      return this.getDefaultModels();
    }
  }

  /**
   * Get default models (fallback when API fails, also used directly by adapter)
   */
  getDefaultModels(): ModelInfo[] {
    return [
      {
        id: 'meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo',
        name: 'Llama 3.1 405B Instruct Turbo',
        provider: 'together',
        capabilities: ['text-generation', 'chat'],
        contextWindow: 128000,
        maxOutputTokens: 4096,
        costPer1kTokens: { input: 0.005, output: 0.015 },
        supportsStreaming: true,
        supportsTools: false,
      },
      {
        id: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
        name: 'Llama 3.1 70B Instruct Turbo',
        provider: 'together',
        capabilities: ['text-generation', 'chat'],
        contextWindow: 128000,
        maxOutputTokens: 4096,
        costPer1kTokens: { input: 0.0009, output: 0.0009 },
        supportsStreaming: true,
        supportsTools: false,
      },
      {
        id: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo',
        name: 'Llama 3.1 8B Instruct Turbo',
        provider: 'together',
        capabilities: ['text-generation', 'chat'],
        contextWindow: 128000,
        maxOutputTokens: 4096,
        costPer1kTokens: { input: 0.0002, output: 0.0002 },
        supportsStreaming: true,
        supportsTools: false,
      },
    ];
  }

  /**
   * Make authenticated API request (shared by all adapters)
   *
   * Consistent error handling and auth for all Together AI operations.
   */
  async request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`, {
      ...options,
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json',
        ...options.headers
      }
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Together API error: ${response.status} - ${errorText}`);
    }

    return await response.json() as T;
  }

  /**
   * Check if API key is configured
   */
  hasApiKey(): boolean {
    return !!this.apiKey && this.apiKey.length > 0;
  }
}
