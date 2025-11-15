/**
 * TogetherAIAdapter - Together AI Inference (Text Generation)
 *
 * Part of modular Together AI architecture:
 * - Uses TogetherBaseConfig for shared auth/models/pricing
 * - Handles text inference only (chat, completion)
 * - Fine-tuning handled by TogetherTextFineTuning
 * - Future: Audio, video, voice handled by separate adapters
 *
 * Supports:
 * - Llama 3.1 405B, 70B, 8B
 * - Mixtral, Qwen, DeepSeek
 * - Extremely fast inference
 *
 * Just 30 lines thanks to BaseOpenAICompatibleAdapter + TogetherBaseConfig!
 */

import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import { TogetherBaseConfig } from './TogetherBaseConfig';

/**
 * Together AI Text Inference Adapter
 *
 * Uses shared TogetherBaseConfig to eliminate duplication with fine-tuning adapter.
 */
export class TogetherAIAdapter extends BaseOpenAICompatibleAdapter {
  private readonly sharedConfig: TogetherBaseConfig;

  constructor(apiKey?: string) {
    // Create shared config (used by inference + fine-tuning)
    const sharedConfig = new TogetherBaseConfig(apiKey);

    super({
      providerId: sharedConfig.providerId,
      providerName: sharedConfig.providerName,
      apiKey: sharedConfig.apiKey,
      baseUrl: sharedConfig.baseUrl,
      defaultModel: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
      timeout: 60000,
      supportedCapabilities: [
        'text-generation',
        'chat',
        'embeddings',
      ],
      models: sharedConfig.getDefaultModels(), // Use shared model definitions
    });

    this.sharedConfig = sharedConfig;
  }

  /**
   * Get shared config (can be used by fine-tuning adapter)
   */
  getSharedConfig(): TogetherBaseConfig {
    return this.sharedConfig;
  }
}
