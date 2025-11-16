/**
 * FireworksAdapter - Fireworks AI API (OpenAI-compatible)
 *
 * Fireworks AI provides fast inference with diverse open models:
 * - DeepSeek V3.1 (reasoning model)
 * - Llama, Mixtral, Qwen models
 * - Function calling & structured outputs
 * - Vision models (Qwen2.5-VL)
 * - Custom fine-tuning with LoRA
 *
 * API: OpenAI-compatible format
 * Base URL: https://api.fireworks.ai/inference
 * Full endpoint: https://api.fireworks.ai/inference/v1/chat/completions
 * Docs: https://docs.fireworks.ai/
 */

import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import { FireworksBaseConfig } from './FireworksBaseConfig';

export class FireworksAdapter extends BaseOpenAICompatibleAdapter {
  private readonly sharedConfig: FireworksBaseConfig;

  constructor(apiKey?: string) {
    // Create shared config (used by inference + fine-tuning)
    const sharedConfig = new FireworksBaseConfig(apiKey);

    super({
      providerId: sharedConfig.providerId,
      providerName: sharedConfig.providerName,
      apiKey: sharedConfig.apiKey,
      baseUrl: sharedConfig.baseUrl,
      defaultModel: sharedConfig.getDefaultModel(),
      timeout: 60000,
      supportedCapabilities: ['text-generation', 'chat', 'embeddings'],
      models: sharedConfig.getAvailableModels(),
    });

    this.sharedConfig = sharedConfig;
  }

  getSharedConfig(): FireworksBaseConfig {
    return this.sharedConfig;
  }
}
