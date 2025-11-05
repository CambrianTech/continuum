import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';

/**
 * DeepSeek Adapter
 *
 * DeepSeek provides SOTA models at industry-leading prices:
 * - deepseek-chat: $0.27/M tokens (14x cheaper than GPT-4)
 * - deepseek-coder: $0.27/M tokens (specialized for code)
 * - deepseek-reasoner: R1 reasoning model
 *
 * API: OpenAI-compatible format
 * Base URL: https://api.deepseek.com/v1
 * Docs: https://platform.deepseek.com/docs
 *
 * Key Features:
 * - 90%+ GPT-4 quality at 14x lower cost
 * - 64K context window
 * - Function calling support
 * - Streaming support
 * - Excellent code generation (deepseek-coder)
 */
export class DeepSeekAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey: string) {
    super({
      providerId: 'deepseek',
      providerName: 'DeepSeek',
      apiKey: apiKey,
      baseUrl: 'https://api.deepseek.com',
      defaultModel: 'deepseek-chat',
      timeout: 120000,
      supportedCapabilities: ['text-generation', 'chat'],
      models: [
        {
          id: 'deepseek-chat',
          name: 'DeepSeek Chat',
          provider: 'deepseek',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 32000,
          supportsStreaming: true,
          supportsFunctions: true
        },
        {
          id: 'deepseek-coder',
          name: 'DeepSeek Coder',
          provider: 'deepseek',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 64000,
          supportsStreaming: true,
          supportsFunctions: true
        },
        {
          id: 'deepseek-reasoner',
          name: 'DeepSeek Reasoner (R1)',
          provider: 'deepseek',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 32000,
          supportsStreaming: true,
          supportsFunctions: true
        }
      ]
    });
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    return this.config.models ?? [];
  }
}
