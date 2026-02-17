/**
 * GoogleBaseConfig - Shared configuration for all Google AI adapters
 *
 * This is the foundation of the modular architecture:
 * - ONE place for API key, base URL, auth
 * - Shared model definitions and pricing
 * - Consistent error handling across all capabilities
 *
 * Used by:
 * - GoogleAdapter (text inference via OpenAI-compatible API)
 * - GeminiLiveAdapter (audio-native real-time API)
 * - Future: GoogleEmbeddingAdapter, GoogleVisionAdapter, etc.
 *
 * Benefits:
 * - Zero code duplication
 * - Consistent auth across all capabilities
 * - Single source of truth for Google config
 *
 * Note: Google provides an OpenAI-compatible API at:
 * https://generativelanguage.googleapis.com/v1beta/openai
 */

import { getSecret } from '../../../../../system/secrets/SecretManager';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';

/**
 * Shared configuration base for Google AI (Gemini)
 *
 * All Google adapters (text inference and audio-native) share this config
 */
export class GoogleBaseConfig {
  readonly providerId = 'google';
  readonly providerName = 'Google Gemini';
  // Google's OpenAI-compatible endpoint
  readonly baseUrl = 'https://generativelanguage.googleapis.com/v1beta/openai';
  readonly apiKey: string;

  constructor(apiKey?: string) {
    this.apiKey = apiKey || getSecret('GOOGLE_API_KEY', 'GoogleBaseConfig') || '';

    if (!this.apiKey) {
      console.warn('⚠️  GoogleBaseConfig: No API key found. Set GOOGLE_API_KEY in SecretManager.');
    }
  }

  /**
   * Check if API key is configured
   */
  hasApiKey(): boolean {
    return !!this.apiKey && this.apiKey.length > 0;
  }

  /**
   * Get available models for Google Gemini
   *
   * Google offers free tier for Gemini Flash models (up to 15 RPM)
   */
  getAvailableModels(): ModelInfo[] {
    return [
      {
        id: 'gemini-2.5-flash-preview-05-20',
        name: 'Gemini 2.5 Flash (Latest)',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat', 'multimodal', 'image-analysis'],
        contextWindow: 1048576, // 1M tokens context
        costPer1kTokens: { input: 0.00015, output: 0.0006 }, // Free tier available
        supportsStreaming: true,
        supportsTools: true
      },
      {
        id: 'gemini-2.0-flash',
        name: 'Gemini 2.0 Flash',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat', 'multimodal', 'image-analysis'],
        contextWindow: 1048576,
        costPer1kTokens: { input: 0.0001, output: 0.0004 },
        supportsStreaming: true,
        supportsTools: true
      },
      {
        id: 'gemini-1.5-flash',
        name: 'Gemini 1.5 Flash',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat', 'multimodal', 'image-analysis'],
        contextWindow: 1048576,
        costPer1kTokens: { input: 0.000075, output: 0.0003 },
        supportsStreaming: true,
        supportsTools: true
      },
      {
        id: 'gemini-1.5-pro',
        name: 'Gemini 1.5 Pro',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat', 'multimodal', 'image-analysis'],
        contextWindow: 2097152, // 2M tokens context
        costPer1kTokens: { input: 0.00125, output: 0.005 },
        supportsStreaming: true,
        supportsTools: true
      },
      // Audio-native models (handled by GeminiLiveAdapter, listed for discovery)
      // Note: multimodal with audio capabilities, but text adapter skips this
      {
        id: 'gemini-2.5-flash-native-audio-preview',
        name: 'Gemini 2.5 Flash Audio-Native',
        provider: this.providerId,
        capabilities: ['text-generation', 'chat', 'multimodal', 'audio-generation', 'audio-transcription'],
        contextWindow: 1048576,
        costPer1kTokens: { input: 0.00015, output: 0.0006 },
        supportsStreaming: true,
        supportsTools: false,
        // Custom flag: this model is audio-native (not in ModelCapability enum)
        // @ts-expect-error - isAudioNative is a custom extension
        isAudioNative: true
      }
    ];
  }

  /**
   * Get default model for text inference
   */
  getDefaultModel(): string {
    return 'gemini-2.0-flash';
  }

  /**
   * Get models suitable for text chat (excludes audio-native models)
   */
  getTextModels(): ModelInfo[] {
    return this.getAvailableModels().filter(
      // Exclude models that have both audio-generation and audio-transcription (audio-native)
      m => !(m.capabilities.includes('audio-generation') && m.capabilities.includes('audio-transcription'))
    );
  }

  /**
   * Make authenticated request to Google AI API
   *
   * Shared method for consistent error handling across all adapters
   * Google's OpenAI-compatible endpoint uses API key in header
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
