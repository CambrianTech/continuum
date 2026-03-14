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
import type { ProviderCapabilities } from '../../../shared/AICapabilityRegistry';
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
      defaultModel: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo',
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

  protected getCapabilityRegistration(): ProviderCapabilities {
    return {
      providerId: 'together',
      providerName: 'Together AI',
      defaultCapabilities: ['text-input', 'text-output', 'streaming', 'function-calling'],
      models: [
        {
          modelId: 'meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo',
          displayName: 'Llama 3.1 405B Instruct Turbo',
          capabilities: ['reasoning', 'coding', 'context-window-large',
            'prose', 'planning', 'review', 'research', 'instruction-following'],
          contextWindow: 128000,
          costTier: 'high',
          latencyTier: 'medium',
        },
        {
          modelId: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
          displayName: 'Llama 3.1 70B Instruct Turbo',
          capabilities: ['reasoning', 'coding', 'context-window-large', 'instruction-following'],
          contextWindow: 128000,
          costTier: 'medium',
          latencyTier: 'fast',
        },
        {
          modelId: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo',
          displayName: 'Llama 3.1 8B Instruct Turbo',
          capabilities: ['coding', 'context-window-large', 'instruction-following'],
          contextWindow: 128000,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'meta-llama/Llama-3.2-90B-Vision-Instruct-Turbo',
          displayName: 'Llama 3.2 90B Vision',
          capabilities: ['image-input', 'multimodal', 'reasoning', 'context-window-large'],
          contextWindow: 128000,
          costTier: 'medium',
          latencyTier: 'medium',
        },
      ],
    };
  }
}
