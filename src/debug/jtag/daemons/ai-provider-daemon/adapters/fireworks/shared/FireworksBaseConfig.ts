/**
 * FireworksBaseConfig - Shared configuration for all Fireworks AI adapters
 *
 * This is the foundation of the modular architecture:
 * - ONE place for API key, base URL, auth
 * - Shared model definitions and pricing
 * - Consistent error handling across all capabilities
 *
 * Used by:
 * - FireworksAdapter (inference)
 * - FireworksFineTuningAdapter (training)
 * - Future: FireworksAudioAdapter, FireworksVisionAdapter, etc.
 *
 * Benefits:
 * - Zero code duplication
 * - Consistent auth across all capabilities
 * - Single source of truth for Fireworks config
 */

import { getSecret } from '../../../../../system/secrets/SecretManager';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';

/**
 * Shared configuration base for Fireworks AI
 *
 * All Fireworks adapters (inference and fine-tuning) share this config
 */
export class FireworksBaseConfig {
  readonly providerId = 'fireworks';
  readonly providerName = 'Fireworks AI';
  readonly baseUrl = 'https://api.fireworks.ai';
  readonly apiKey: string;

  constructor(apiKey?: string) {
    this.apiKey = apiKey || getSecret('FIREWORKS_API_KEY', 'FireworksBaseConfig') || '';

    if (!this.apiKey) {
      console.warn('⚠️  FireworksBaseConfig: No API key found. Set FIREWORKS_API_KEY in SecretManager.');
    }
  }

  /**
   * Check if API key is configured
   */
  hasApiKey(): boolean {
    return !!this.apiKey && this.apiKey.length > 0;
  }

  /**
   * Get available models for Fireworks AI
   *
   * Static list of popular models on Fireworks platform
   */
  getAvailableModels(): ModelInfo[] {
    return [
      {
        id: 'accounts/fireworks/models/llama-v3p1-405b-instruct',
        name: 'Llama 3.1 405B Instruct',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 131072,
        costPer1kTokens: { input: 0.003, output: 0.003 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'accounts/fireworks/models/llama-v3p1-70b-instruct',
        name: 'Llama 3.1 70B Instruct',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 131072,
        costPer1kTokens: { input: 0.0009, output: 0.0009 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'accounts/fireworks/models/llama-v3p1-8b-instruct',
        name: 'Llama 3.1 8B Instruct',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 131072,
        costPer1kTokens: { input: 0.0002, output: 0.0002 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'accounts/fireworks/models/mixtral-8x7b-instruct',
        name: 'Mixtral 8x7B Instruct',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 32768,
        costPer1kTokens: { input: 0.0005, output: 0.0005 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'accounts/fireworks/models/qwen2p5-72b-instruct',
        name: 'Qwen 2.5 72B Instruct',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 32768,
        costPer1kTokens: { input: 0.0009, output: 0.0009 },
        supportsStreaming: true,
        supportsFunctions: true
      }
    ];
  }

  /**
   * Get default model for inference
   */
  getDefaultModel(): string {
    return 'accounts/fireworks/models/llama-v3p1-8b-instruct';
  }

  /**
   * Get default model for fine-tuning
   */
  getDefaultFineTuningModel(): string {
    return 'accounts/fireworks/models/llama-v3p1-8b-instruct';
  }

  /**
   * Get supported fine-tuning models
   */
  getSupportedFineTuningModels(): string[] {
    return [
      'accounts/fireworks/models/llama-v3p1-8b-instruct',
      'accounts/fireworks/models/llama-v3p1-70b-instruct',
      'accounts/fireworks/models/mixtral-8x7b-instruct',
      'accounts/fireworks/models/qwen2p5-72b-instruct'
    ];
  }

  /**
   * Make authenticated request to Fireworks API
   *
   * Shared method for consistent error handling across all adapters
   */
  async makeRequest(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<Response> {
    const url = `${this.baseUrl}${endpoint}`;

    const headers = {
      'Authorization': `Bearer ${this.apiKey}`,
      'Content-Type': 'application/json',
      ...options.headers
    };

    return fetch(url, {
      ...options,
      headers
    });
  }
}
