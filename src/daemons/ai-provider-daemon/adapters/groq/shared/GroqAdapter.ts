import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import type { ProviderCapabilities } from '../../../shared/AICapabilityRegistry';

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

  protected getCapabilityRegistration(): ProviderCapabilities {
    return {
      providerId: 'groq',
      providerName: 'Groq',
      defaultCapabilities: ['text-input', 'text-output', 'streaming', 'function-calling'],
      models: [
        {
          modelId: 'llama-3.3-70b-versatile',
          displayName: 'Llama 3.3 70B Versatile',
          capabilities: ['reasoning', 'coding', 'instruction-following', 'tool-use'],
          contextWindow: 128000,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'llama-3.2-90b-vision-preview',
          displayName: 'Llama 3.2 90B Vision',
          capabilities: ['image-input', 'multimodal', 'reasoning'],
          contextWindow: 8192,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'mixtral-8x7b-32768',
          displayName: 'Mixtral 8x7B',
          capabilities: ['coding', 'instruction-following'],
          contextWindow: 32768,
          costTier: 'low',
          latencyTier: 'fast',
        },
      ],
    };
  }
}
