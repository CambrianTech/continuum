import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';

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
 * Key Features:
 * - Fastest inference speed in industry (LPU hardware)
 * - Llama 3.1, Mixtral, Gemma models
 * - Streaming support
 * - Free tier: 14,400 requests/day
 * - Perfect for real-time applications
 *
 * Use Cases:
 * - Real-time chat (instant responses)
 * - High-throughput applications
 * - Evaluation/testing (fast iteration)
 * - Interactive AI experiences
 */
export class GroqAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey: string) {
    super({
      providerId: 'groq',
      providerName: 'Groq',
      apiKey: apiKey,
      baseUrl: 'https://api.groq.com/openai',
      defaultModel: 'llama-3.1-8b-instant',
      timeout: 60000,
      supportedCapabilities: ['text-generation', 'chat'],
      models: [
        // Llama 3.1 family (Meta) — Groq supports 128K context for these
        {
          id: 'llama-3.1-405b-reasoning',
          name: 'Llama 3.1 405B',
          provider: 'groq',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 131072,
          supportsStreaming: true,
          supportsFunctions: false
        },
        {
          id: 'llama-3.1-8b-instant',
          name: 'Llama 3.1 8B (Default)',
          provider: 'groq',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 131072,
          supportsStreaming: true,
          supportsFunctions: false
        },
        // Mixtral family (Mistral AI)
        {
          id: 'mixtral-8x7b-32768',
          name: 'Mixtral 8x7B MoE',
          provider: 'groq',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 32768,
          supportsStreaming: true,
          supportsFunctions: false
        },
        // Gemma family (Google)
        {
          id: 'gemma2-9b-it',
          name: 'Gemma 2 9B',
          provider: 'groq',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 8192,
          supportsStreaming: true,
          supportsFunctions: false
        }
      ]
    });
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    return this.config.models ?? [];
  }
}
