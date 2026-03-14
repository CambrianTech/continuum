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
import type { ProviderCapabilities } from '../../../shared/AICapabilityRegistry';
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

  protected getCapabilityRegistration(): ProviderCapabilities {
    return {
      providerId: 'openai',
      providerName: 'OpenAI',
      defaultCapabilities: [
        'text-input', 'text-output', 'function-calling', 'streaming',
        'instruction-following',
      ],
      models: [
        {
          modelId: 'gpt-4o',
          displayName: 'GPT-4o',
          capabilities: ['image-input', 'multimodal', 'reasoning', 'coding', 'context-window-large',
            'prose', 'review', 'planning', 'research', 'tool-use'],
          contextWindow: 128000,
          costTier: 'medium',
          latencyTier: 'fast',
        },
        {
          modelId: 'gpt-4o-mini',
          displayName: 'GPT-4o Mini',
          capabilities: ['image-input', 'multimodal', 'coding', 'instruction-following'],
          contextWindow: 128000,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'gpt-4-turbo',
          displayName: 'GPT-4 Turbo',
          capabilities: ['image-input', 'multimodal', 'reasoning', 'coding', 'context-window-large',
            'prose', 'planning', 'tool-use'],
          contextWindow: 128000,
          costTier: 'high',
          latencyTier: 'medium',
        },
        {
          modelId: 'gpt-4',
          displayName: 'GPT-4',
          capabilities: ['reasoning', 'coding', 'prose', 'planning'],
          contextWindow: 8192,
          costTier: 'high',
          latencyTier: 'medium',
        },
        {
          modelId: 'gpt-3.5-turbo',
          displayName: 'GPT-3.5 Turbo',
          capabilities: ['coding'],
          contextWindow: 16385,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'dall-e-3',
          displayName: 'DALL-E 3',
          capabilities: ['image-output'],
          costTier: 'medium',
          latencyTier: 'slow',
        },
        {
          modelId: 'whisper',
          displayName: 'Whisper',
          capabilities: ['audio-input'],
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'tts-1',
          displayName: 'TTS-1',
          capabilities: ['audio-output'],
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'text-embedding',
          displayName: 'Text Embedding',
          capabilities: ['embeddings'],
          costTier: 'low',
          latencyTier: 'fast',
        },
      ],
    };
  }
}
