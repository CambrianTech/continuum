import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';
import { DeepSeekBaseConfig } from './DeepSeekBaseConfig';

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
  private readonly sharedConfig: DeepSeekBaseConfig;

  constructor(apiKey?: string) {
    // Create shared config (used by inference + fine-tuning)
    const sharedConfig = new DeepSeekBaseConfig(apiKey);

    super({
      providerId: sharedConfig.providerId,
      providerName: sharedConfig.providerName,
      apiKey: sharedConfig.apiKey,
      baseUrl: sharedConfig.baseUrl,
      defaultModel: sharedConfig.getDefaultModel(),
      timeout: 120000,
      supportedCapabilities: ['text-generation', 'chat'],
      models: sharedConfig.getAvailableModels(),
    });

    this.sharedConfig = sharedConfig;
  }

  getSharedConfig(): DeepSeekBaseConfig {
    return this.sharedConfig;
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    return this.config.models ?? [];
  }
}
