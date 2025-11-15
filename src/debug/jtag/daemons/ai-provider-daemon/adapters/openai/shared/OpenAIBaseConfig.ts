/**
 * OpenAIBaseConfig - Shared configuration for all OpenAI adapters
 *
 * This is the foundation of the modular architecture:
 * - ONE place for API key, base URL, auth
 * - Shared model definitions and pricing
 * - Consistent error handling across all capabilities
 *
 * Used by:
 * - OpenAIAdapter (inference)
 * - OpenAIFineTuningAdapter (training)
 * - Future: OpenAIAudioAdapter, OpenAIVisionAdapter, etc.
 *
 * Benefits:
 * - Zero code duplication
 * - Consistent auth across all capabilities
 * - Single source of truth for OpenAI config
 */

import { getSecret } from '../../../../../system/secrets/SecretManager';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';

/**
 * Shared configuration base for OpenAI
 *
 * All OpenAI adapters (inference and fine-tuning) share this config
 */
export class OpenAIBaseConfig {
  readonly providerId = 'openai';
  readonly providerName = 'OpenAI';
  readonly baseUrl = 'https://api.openai.com';
  readonly apiKey: string;

  constructor(apiKey?: string) {
    this.apiKey = apiKey || getSecret('OPENAI_API_KEY', 'OpenAIBaseConfig') || '';

    if (!this.apiKey) {
      console.warn('⚠️  OpenAIBaseConfig: No API key found. Set OPENAI_API_KEY in SecretManager.');
    }
  }

  /**
   * Check if API key is configured
   */
  hasApiKey(): boolean {
    return !!this.apiKey && this.apiKey.length > 0;
  }

  /**
   * Get available models for OpenAI
   *
   * Static list since OpenAI model list is well-known and rarely changes
   */
  getAvailableModels(): ModelInfo[] {
    return [
      {
        id: 'gpt-4o',
        name: 'GPT-4 Optimized',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 128000,
        costPer1kTokens: { input: 0.0025, output: 0.01 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'gpt-4o-mini',
        name: 'GPT-4 Optimized Mini',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 128000,
        costPer1kTokens: { input: 0.00015, output: 0.0006 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'gpt-4-turbo',
        name: 'GPT-4 Turbo',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 128000,
        costPer1kTokens: { input: 0.01, output: 0.03 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'gpt-4',
        name: 'GPT-4',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 8192,
        costPer1kTokens: { input: 0.03, output: 0.06 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'gpt-3.5-turbo',
        name: 'GPT-3.5 Turbo',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 16385,
        costPer1kTokens: { input: 0.0005, output: 0.0015 },
        supportsStreaming: true,
        supportsFunctions: true
      }
    ];
  }

  /**
   * Get default model for inference
   */
  getDefaultModel(): string {
    return 'gpt-4o-mini';
  }

  /**
   * Get default model for fine-tuning
   */
  getDefaultFineTuningModel(): string {
    return 'gpt-4o-mini-2024-07-18';
  }

  /**
   * Get supported fine-tuning models
   */
  getSupportedFineTuningModels(): string[] {
    return [
      'gpt-3.5-turbo',
      'gpt-4',
      'gpt-4-turbo',
      'gpt-4o',
      'gpt-4o-mini-2024-07-18'
    ];
  }

  /**
   * Make authenticated request to OpenAI API
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
