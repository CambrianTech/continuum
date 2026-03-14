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
import type { ProviderCapabilities } from '../../../shared/AICapabilityRegistry';
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

  protected getCapabilityRegistration(): ProviderCapabilities {
    return {
      providerId: 'fireworks',
      providerName: 'Fireworks AI',
      defaultCapabilities: ['text-input', 'text-output', 'streaming', 'function-calling'],
      models: [
        {
          modelId: 'accounts/fireworks/models/llama-v3p3-70b-instruct',
          displayName: 'Llama 3.3 70B Instruct',
          capabilities: ['reasoning', 'coding', 'instruction-following', 'context-window-large'],
          contextWindow: 131072,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'accounts/fireworks/models/llama-v3p1-405b-instruct',
          displayName: 'Llama 3.1 405B Instruct',
          capabilities: ['reasoning', 'coding', 'prose', 'planning', 'review', 'context-window-large'],
          contextWindow: 131072,
          costTier: 'medium',
          latencyTier: 'medium',
        },
        {
          modelId: 'accounts/fireworks/models/qwen2p5-72b-instruct',
          displayName: 'Qwen 2.5 72B Instruct',
          capabilities: ['reasoning', 'coding', 'math'],
          contextWindow: 32768,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'accounts/fireworks/models/mixtral-8x7b-instruct',
          displayName: 'Mixtral 8x7B Instruct',
          capabilities: ['coding', 'instruction-following'],
          contextWindow: 32768,
          costTier: 'low',
          latencyTier: 'fast',
        },
      ],
    };
  }
}
