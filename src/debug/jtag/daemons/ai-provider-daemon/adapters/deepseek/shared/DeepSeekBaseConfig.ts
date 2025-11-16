/**
 * DeepSeekBaseConfig - Shared configuration for all DeepSeek adapters
 *
 * This is the foundation of the modular architecture:
 * - ONE place for API key, base URL, auth
 * - Shared model definitions and pricing
 * - Consistent error handling across all capabilities
 *
 * Used by:
 * - DeepSeekAdapter (inference)
 * - DeepSeekFineTuningAdapter (training)
 * - Future: DeepSeekAudioAdapter, DeepSeekVisionAdapter, etc.
 *
 * Benefits:
 * - Zero code duplication
 * - Consistent auth across all capabilities
 * - Single source of truth for DeepSeek config
 *
 * Note: DeepSeek API is OpenAI-compatible, making integration straightforward
 */

import { getSecret } from '../../../../../system/secrets/SecretManager';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';

/**
 * Shared configuration base for DeepSeek
 *
 * All DeepSeek adapters (inference and fine-tuning) share this config
 */
export class DeepSeekBaseConfig {
  readonly providerId = 'deepseek';
  readonly providerName = 'DeepSeek';
  readonly baseUrl = 'https://api.deepseek.com';
  readonly apiKey: string;

  constructor(apiKey?: string) {
    this.apiKey = apiKey || getSecret('DEEPSEEK_API_KEY', 'DeepSeekBaseConfig') || '';

    if (!this.apiKey) {
      console.warn('⚠️  DeepSeekBaseConfig: No API key found. Set DEEPSEEK_API_KEY in SecretManager.');
    }
  }

  /**
   * Check if API key is configured
   */
  hasApiKey(): boolean {
    return !!this.apiKey && this.apiKey.length > 0;
  }

  /**
   * Get available models for DeepSeek
   *
   * DeepSeek offers competitive pricing and strong reasoning capabilities
   */
  getAvailableModels(): ModelInfo[] {
    return [
      {
        id: 'deepseek-chat',
        name: 'DeepSeek Chat',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 32768,
        costPer1kTokens: { input: 0.0001, output: 0.0002 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'deepseek-coder',
        name: 'DeepSeek Coder',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 16384,
        costPer1kTokens: { input: 0.0001, output: 0.0002 },
        supportsStreaming: true,
        supportsFunctions: true
      },
      {
        id: 'deepseek-reasoner',
        name: 'DeepSeek Reasoner (R1)',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat'],
        contextWindow: 32768,
        costPer1kTokens: { input: 0.00055, output: 0.0022 },
        supportsStreaming: true,
        supportsFunctions: true
      }
    ];
  }

  /**
   * Get default model for inference
   */
  getDefaultModel(): string {
    return 'deepseek-chat';
  }

  /**
   * Get default model for fine-tuning
   */
  getDefaultFineTuningModel(): string {
    return 'deepseek-chat';
  }

  /**
   * Get supported fine-tuning models
   */
  getSupportedFineTuningModels(): string[] {
    return [
      'deepseek-chat',
      'deepseek-coder'
    ];
  }

  /**
   * Make authenticated request to DeepSeek API
   *
   * Shared method for consistent error handling across all adapters
   * DeepSeek uses OpenAI-compatible API format
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
