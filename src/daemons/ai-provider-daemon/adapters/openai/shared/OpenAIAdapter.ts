/**
 * OpenAIAdapter - Official OpenAI API
 *
 * Supports:
 * - GPT-4, GPT-4 Turbo, GPT-3.5 Turbo
 * - DALL-E 3 image generation
 * - Text embeddings
 * - Multimodal (GPT-4V vision)
 *
 * Just 30 lines of code thanks to BaseOpenAICompatibleAdapter!
 */

import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';
import { OpenAIBaseConfig } from './OpenAIBaseConfig';

export class OpenAIAdapter extends BaseOpenAICompatibleAdapter {
  private readonly sharedConfig: OpenAIBaseConfig;

  constructor(apiKey?: string) {
    // Create shared config (used by inference + fine-tuning)
    const sharedConfig = new OpenAIBaseConfig(apiKey);

    super({
      providerId: sharedConfig.providerId,
      providerName: sharedConfig.providerName,
      apiKey: sharedConfig.apiKey,
      baseUrl: sharedConfig.baseUrl,
      defaultModel: 'gpt-4-turbo',
      timeout: 60000,
      supportedCapabilities: [
        'text-generation',
        'chat',
        'image-generation',
        'image-analysis',
        'embeddings',
        'multimodal',
      ],
      models: sharedConfig.getAvailableModels(),
    });

    this.sharedConfig = sharedConfig;
  }

  getSharedConfig(): OpenAIBaseConfig {
    return this.sharedConfig;
  }

  // Cost calculation now handled by BaseOpenAICompatibleAdapter using PricingManager
  // No need to override unless OpenAI has special pricing logic
}
