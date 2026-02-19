import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';

/**
 * X.AI (xAI) Adapter - Grok Models
 *
 * X.AI provides Elon Musk's Grok models with exceptional reasoning capabilities:
 * - grok-4: Latest flagship model with advanced reasoning
 * - grok-vision-4: Multimodal model with image understanding
 * - Real-time access to X (Twitter) data
 * - Long context windows
 *
 * API: OpenAI-compatible format
 * Base URL: https://api.x.ai/v1
 * Docs: https://docs.x.ai
 *
 * Key Features:
 * - Advanced reasoning capabilities
 * - Multimodal vision support
 * - Real-time web search via X
 * - Structured outputs support
 * - Streaming support
 * - Function calling
 *
 * Use Cases:
 * - Complex reasoning tasks
 * - Real-time information queries
 * - Image analysis and understanding
 * - Current events and trending topics
 */
export class XAIAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey: string) {
    super({
      providerId: 'xai',
      providerName: 'X.AI (xAI)',
      apiKey: apiKey,
      baseUrl: 'https://api.x.ai',
      defaultModel: 'grok-4',
      timeout: 180000, // 3 minutes for reasoning models
      supportedCapabilities: ['text-generation', 'chat', 'image-analysis'],
      models: [
        {
          id: 'grok-4',
          name: 'Grok 4 (Latest)',
          provider: 'xai',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 128000,
          supportsStreaming: true,
          supportsTools: true
        },
        {
          id: 'grok-vision-4',
          name: 'Grok Vision 4',
          provider: 'xai',
          capabilities: ['text-generation', 'chat', 'image-analysis'],
          contextWindow: 128000,
          supportsStreaming: true,
          supportsTools: true
        },
        {
          id: 'grok-2-1212',
          name: 'Grok 2 (December 2024)',
          provider: 'xai',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 128000,
          supportsStreaming: true,
          supportsTools: true
        },
        {
          id: 'grok-2-vision-1212',
          name: 'Grok 2 Vision (December 2024)',
          provider: 'xai',
          capabilities: ['text-generation', 'chat', 'image-analysis'],
          contextWindow: 128000,
          supportsStreaming: true,
          supportsTools: true
        }
      ]
    });
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    return this.config.models ?? [];
  }
}
