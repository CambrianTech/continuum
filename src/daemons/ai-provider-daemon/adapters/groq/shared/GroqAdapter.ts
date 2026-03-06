import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';

/**
 * Groq Adapter
 *
 * Groq provides the world's fastest LLM inference using custom LPU (Language Processing Unit) hardware:
 * - 10x-100x faster than GPU inference
 * - Instant responses (< 100ms latency)
 * - Free tier available
 *
 * API: OpenAI-compatible format
 * Base URL: https://api.groq.com/openai/v1
 * Docs: https://console.groq.com/docs
 *
 * Model catalog is fetched dynamically from the Groq API at runtime.
 * No hardcoded model list — always up to date.
 */
export class GroqAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey: string) {
    super({
      providerId: 'groq',
      providerName: 'Groq',
      apiKey: apiKey,
      baseUrl: 'https://api.groq.com/openai',
      defaultModel: 'llama-3.3-70b-versatile',
      timeout: 60000,
      supportedCapabilities: ['text-generation', 'chat'],
      // No hardcoded models — fetched dynamically via GET /v1/models
    });
  }
}
